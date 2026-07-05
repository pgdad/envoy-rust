# Phase 58 — `58-accesslog-h2-uo-overflow` — Progress

> State-3 implementation, executed via `superpowers:executing-plans` directly
> in a single session (PLAN.md's 6 tasks were fully detailed with exact code
> diffs already re-confirmed against the tree at state-2, so direct execution
> was used rather than `subagent-driven-development`). Per
> `docs/envoy-rust/SKILL_ROUTING.md` step 3, TDD (RED test committed, then
> GREEN fix committed) on every task. Session opened by pulling unrelated
> concurrent phases 59/60 (H1 perf work, no ROADMAP rows of their own yet)
> from `origin/main` and confirming the workspace still builds/tests clean
> before starting phase-58 work.

## Task 1 — Caller-loop overflow discriminator + three-arm derive + pool-overflow backstop (§A + §C + §F1)

Commits: `a132f43` (RED) + `d3393dd` (GREEN).

Re-confirmed the three cited `hcm.rs` line ranges against the live tree
before editing (`:691`-`703` the caller-loop discriminator site, `:949`-`963`
the two-arm derive) — exact, no drift. Wrote the failing backstop test
`h2_pool_overflow_access_log_carries_uo_flag` first (a configured
`H2PoolManager` — `circuit_breakers.thresholds:[{max_connections:1,
max_pending_requests:0}]` + a dead endpoint `127.0.0.1:1`, manual `HCM::new`
spawn mirroring `h2_hcm_pool_reuses_upstream_conn_across_sequential_requests`
since `spawn_h2_hcm` hard-codes `pool: None`) — RED: logged line
`{"rc":503,"rcd":"via_upstream","rf":"-"}`, exactly as PLAN predicted. Added
the `attempt.outcome.is_none()` discriminator at the caller-loop site
(mirroring H1's `hcm.rs:1045`-`1052` exactly) and extended the
`%RESPONSE_FLAGS%` derive to a third arm
(`Some("upstream_reset_before_response_started{overflow}") => "UO"`).

`cargo test -p envoy-http2 h2_pool_overflow_access_log_carries_uo_flag`:
GREEN (logged line
`{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`).
Full `cargo test -p envoy-http2`: 83 passed, 0 failed, 1 ignored — no
regression (the phase-56/57 `NR`/`UH` backstops and the pool-reuse test's
successful-dispatch path are unaffected, since `outcome.is_none()` is `false`
on every real response/reset/connect-failure outcome).

## Task 2 — Request-budget-arm tag + budget-overflow backstop (§B + §F2)

Commits: `3ad55fb` (RED) + `b7fa13a` (GREEN).

Wrote the failing backstop test
`h2_request_budget_overflow_access_log_carries_uo_flag` first (a STATIC
cluster, `circuit_breakers.thresholds:[{max_requests:0}]`, plain H1 upstream
— this arm is protocol-agnostic and never touches the pool, so `spawn_h2_hcm`
with `pool: None` suffices) — RED: logged line `{"rc":503,"rcd":null,"rf":"-"}`,
exactly as PLAN predicted (the `Rejected` arm never sets
`response_code_details_for_log_h2` before this task). Added the direct tag
`response_code_details_for_log_h2 = Some("upstream_reset_before_response_started{overflow}".to_owned())`
right after `synth_h2_overflow()` is constructed in the pre-route
`BudgetAcquisition::Rejected` arm, mirroring H1's `hcm.rs:951`-`952` exactly
— this arm bypasses the retry loop entirely (no `run_h2_attempt` call), so it
is tagged directly here rather than via Task 1's §A discriminator.

`cargo test -p envoy-http2 h2_request_budget_overflow_access_log_carries_uo_flag`:
GREEN (logged line
`{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`).
Full `cargo test -p envoy-http2`: 84 passed, 0 failed, 1 ignored — no
regression (Task 1's backstop, a different arm, is unaffected; no other
existing test configures `circuit_breakers.thresholds.max_requests: 0`).

## Task 3 — Fixture `0066-accesslog-h2-rf-overflow` (§D)

Commit: `68ced77`.

Re-confirmed `0066` was still next-free (`ls tests/fixtures/ | sort | tail`
showed `0065` as the highest — no sibling race). Created
`envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md`: fixture
`0058`'s H1 `UO` cluster shape (`circuit_breakers.thresholds:[{max_connections:1,
max_pending_requests:0}]` + a literal dead endpoint `127.0.0.1:1`) combined
with fixture `0064`/`0065`'s H2C listener shape, PLUS
`typed_extension_protocol_options` (an H2-upstream cluster, needed for
envoy-rust's side to route through the H2 pool and hit
`PoolError::PendingOverflow`). One probe, `expected_status: 503`, reusing
`Driver::Http2AccessLogByteExact` verbatim (no harness change).

## Task 4 — Differential test `access_log_h2_rf_overflow.rs` (§E)

Commit: `130183d`.

A structural clone of `access_log_h2_rf_no_healthy.rs`, pointing at the
`0066` fixture directory. `cargo test -p differential --no-run`: compiles
clean.

`cargo test -p differential --test access_log_h2_rf_overflow` run standalone
(after rebuilding the debug `envoy-bin`, see below): PASS — byte-exact
`{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`
on both proxies. Re-ran fixtures `0064`/`0065` alongside for additivity —
both still green.

**Debug-binary staleness caught and fixed:** the FIRST attempt to run this
fixture locally FAILED with envoy-rust still emitting the OLD
`via_upstream`/`-` output despite Task 1's source fix already landing (log
showed `H2 pending-request overflow (max_pending_requests:0) — emitting
503` but the access-log line was unchanged) — traced to a stale
`target/debug/envoy-bin` (last built before this session's `hcm.rs` edits;
the differential harness runs this binary as a subprocess, per memory
`differential-harness-uses-debug-envoy-bin`). Rebuilt (`cargo build -p
envoy-bin`) and the fixture immediately went GREEN — not a code regression,
a harness-staleness trap.

## Task 5 — `BEHAVIOR_CONTRACT.md` updates (§G)

Commit: `a9c4c45`.

Updated the `%RESPONSE_FLAGS%` row's H2-witness sentence to record `UO`
witnessed byte-exact on H2 by fixture `0066` — set on BOTH the pool-overflow
arm (§A) and the request-budget arm (§B), advancing carry-forward **M56-1**
(the `UO` slice consumed; `URX`/`UF`/`UC` remain open). Updated the
`%RESPONSE_CODE_DETAILS%` row to record `upstream_reset_before_response_started{overflow}`
witnessed on H2, set on both arms.

## Task 6 — Local verification sweep (state-3 close-out)

`cargo clippy -p envoy-http2 -p differential --all-targets --all-features --
-D warnings`: clean, no warnings.

`cargo fmt --all -- --check`: needed one fix (two multi-line call-site
reflows introduced by Task 1's new backstop test — `Http1HCMConfig::from_config`
and `HCMConfig::wrap` call sites). Applied `cargo fmt --all`, committed
(`70f257e`).

`cargo test --workspace` (run three times across the session, twice
`--no-fail-fast`, debug `envoy-bin` rebuilt first per memory
`differential-harness-uses-debug-envoy-bin`): green except a rotating set of
already-documented host-only flakes, none touching this phase's changed
files (`crates/envoy-http2/src/hcm.rs`, fixture `0066`, its differential
test, `BEHAVIOR_CONTRACT.md`):

- **`access_log_rcd_upstream_reset`** (fixture `0062`, phase 54) +
  **`access_log_rf_upstream_reset`** (fixture `0061`, phase 53) +
  **`admin_config_dump_server_info`** (fixture `0014`, phase 08.1) — the
  `192.168.65.2` Docker-bridge-IP divergence class, memory
  `differential-host-bridge-ip-192-168-65-2`; all three predate phase 58.
- The `envoy-http2` h2-handshake test
  (`send_request_maps_h2_handshake_failure_to_typed_error`) — memory
  `envoyrust-h2-handshake-test-host-flake`; re-confirmed passing in
  isolation, INCLUDING both new phase-58 backstops (84 passed, 0 failed).
- A same-class Docker-container-startup-under-parallel-load flake, observed
  rotating across different differential targets on different runs
  (`upstream_retry` once, `access_log_command_operators` once) — each
  confirmed passing standalone immediately after.

Byte-preservation re-check: `for f in 0009 0010 0018 0021 0064 0065; do grep
-n circuit_breakers tests/fixtures/${f}-*/envoy-rust.yaml || echo "(none)";
done` → only `0021` shows a hit (`max_connections: 4`, headroom only, no
`max_pending_requests`/`max_requests` cap) — confirms `0001`-`0065` stay
unreachable via either new code path, so they remain byte-identical; only
`0066` observes the changed rcd/rf.

`cargo fmt --all` re-run at close: nothing to reformat, working tree clean.

## Summary

All 6 PLAN.md tasks landed GREEN with no regressions found locally. Fixture
`0066` was additionally confirmed GREEN standalone against a real live Envoy
v1.33.0 container — unusual for a Docker differential fixture on this dev
host, since it needs no live backend/sibling-container (only a literal dead
endpoint), so it isn't subject to the host's bridge-IP quirk that REDs
several other fixtures locally. This session did **not** run the full §7.5
verification gate (h2spec, cargo-deny, fuzz, CI confirmation) — that is
state-4, a separate session per `BOOTSTRAP_PROMPT.md` §5.1. No new ADR fired
(SPEC §A-§H were not overturned during implementation); ADR-0115 remains the
ledger head. No status-code change anywhere; no new `Op`/`AccessLogRecord`
field/crate/dependency/`ConfigError` variant.

## State-4 verification

Executed via `superpowers:verification-before-completion`. Cold-started
fully (`BOOTSTRAP_PROMPT.md` §1, `STATE.md`, `ROADMAP.md`, `DECISIONS.md`
tail, `SKILL_ROUTING.md`, plus this phase's SPEC/PLAN/PROGRESS) before
running the gate. `git status --porcelain` clean and `HEAD` at the phase-58
state-3 commit `ee0ae95` both before and after the local run (concurrency
guard, memory `concurrent-loop-sessions-race-on-phase-pick`) — no sibling
had advanced; `git fetch origin --prune` confirmed `origin/main` unmoved
(only an unrelated new branch `perf/listener-so-reuseport` appeared, no new
commits on `main`).

### `cargo build --workspace --all-targets`

Clean. `Finished \`dev\` profile [unoptimized + debuginfo] target(s)`.

### `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Clean, no warnings. `Finished \`dev\` profile [unoptimized + debuginfo]
target(s) in 1.12s`.

### `cargo fmt --all -- --check`

Clean (exit 0, no output).

### `cargo test --workspace` (debug `envoy-bin` rebuilt first via `cargo
build -p envoy-bin`, per memory `differential-harness-uses-debug-envoy-bin`)

Ran twice (once fail-fast, once `--no-fail-fast` for the full picture). The
`--no-fail-fast` run surfaced 5 target-level failures, every one
independently re-confirmed passing in isolation and traced to
already-documented host-only flake classes, none touching this phase's
changed files (`crates/envoy-http2/src/hcm.rs`, fixture `0066`, its
differential test, `BEHAVIOR_CONTRACT.md`):

- **`access_log_rcd_upstream_reset`** (fixture `0062`, phase 54) +
  **`access_log_rf_upstream_reset`** (fixture `0061`, phase 53) — both show
  the `UF`-vs-`UC` divergence keyed to an IPv6 `remote_address:[fdc4:...]`
  connect-failure address, matching memory
  `differential-host-bridge-ip-192-168-65-2` exactly; re-run standalone
  twice each, failed identically both times (deterministic on this host, not
  a parallel-load flake) — both fixtures predate phase 58.
- **`admin_config_dump_server_info`** (fixture `0014`, phase 08.1) — diverges
  on `backend::192.168.65.2:<port>::*` stats fields, the same
  `192.168.65.2` bridge-IP divergence memory documents by name; re-run
  standalone, failed identically.
- **`http_filter_buffer`** — failed once with "upstream Envoy never
  became accept-ready" (Docker-container-startup-under-parallel-load class,
  memory `differential-fixtures-flake-under-parallel-load`); re-run
  standalone immediately after — **passed** (`test http_filter_buffer_fixture
  ... ok`), confirming transient, not a regression.
- **`envoy-http2` lib test
  `client::tests::send_request_maps_h2_handshake_failure_to_typed_error`** —
  memory `envoyrust-h2-handshake-test-host-flake`; re-run standalone —
  **passed** (`test ... ok`, 1 passed, 0 failed).

Fixture `0066`'s differential test (`access_log_h2_rf_overflow`) passed
cleanly in every run, including a dedicated standalone run alongside
`0064`/`0065` for additivity (`test access_log_h2_rf_overflow ... ok`, `test
access_log_h2_rf_no_route ... ok`, `test access_log_h2_rf_no_healthy ...
ok` — all green, 0 failed). Both new in-process backstops from Tasks 1/2
passed as part of the full workspace run: `test
hcm::tests::h2_pool_overflow_access_log_carries_uo_flag ... ok` and `test
hcm::tests::h2_request_budget_overflow_access_log_carries_uo_flag ... ok`.

### `cargo deny check`

Clean: `advisories ok, bans ok, licenses ok, sources ok` (only the same 5
pre-existing `license-not-encountered` warnings for unmatched allow-list
entries — `0BSD`/`BSD-2-Clause`/`MPL-2.0`/`Unicode-DFS-2016`/`Zlib` —
present on `main` before this phase, unrelated to phase 58).

### Docker differential suite (fixture `0066` + all `0001`-`0065`), h2spec,
fuzz — **CI-authoritative** (memory `envoy-rust-state4-ci-first-execution`)

Per the handoff prompt: CI run `28753634192` was already green on the
current HEAD `ee0ae959e373841ff09bfeff16e7b35b06165547`, confirmed via `gh
run view 28753634192 --json status,conclusion,headSha,jobs`: `headSha` =
`ee0ae959...`, top-level `status` = `completed`, `conclusion` = `success`,
both jobs (`build + test + lint`, `fuzz (parse_bootstrap + jwt_parse +
cdn_loop_parse + accesslog_format_parse, 30s each)`) `conclusion` =
`success`, every individual step within both jobs `conclusion` = `success`.
This session's own local re-verification found no regression to fix (the
local gate above is clean modulo the 5 documented, independently
re-confirmed host-only flakes), so per the handoff's own guidance no new
commit/re-trigger was needed — the existing green run is cited directly as
the authoritative §7.5(a)/(b)/(c)/(d) evidence.

Pulled the full job log (`gh run view 28753634192 --log`, 27645 lines) and
confirmed:

- **Zero occurrences of the literal `FAILED` anywhere in the entire job
  log** — every differential fixture, including the ones that flake locally
  on this dev host, passed in CI's environment.
- `test access_log_h2_rf_overflow ... ok` (fixture `0066`, this phase's
  witness) — present in the `build + test + lint` job's `test` step output.
- `test h2spec_pass_rate_gate ... ok` — CI's h2spec gate passes with the
  unmodified `known-failures.txt` (no H2 codec/framing change this phase, so
  the gate is unmoved from phase 57's baseline).
- `envoy-http2`'s lib-test step: `test result: ok. 84 passed; 0 failed; 1
  ignored` — includes both new phase-58 backstops, no regression versus the
  phase-58 state-3 session's local counts.
- The `cargo deny check` step ran the same 5 pre-existing
  `license-not-encountered` warnings as the local run, and the job step
  completed successfully (job `build + test + lint` `conclusion` =
  `success`).
- The `fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse +
  accesslog_format_parse, 30s each)` job completed `success` — no new fuzz
  target required this phase (SPEC §H), and the existing 4 targets ran
  clean.

§7.5 gate status: **(a)** fixture `0066` green ✅ (CI: `access_log_h2_rf_overflow
... ok`, cross-proxy-equal status 503 + whole-line
`{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`,
also confirmed locally both in this session and the state-3 session);
**(b)** all `0001`-`0065` green simultaneously ✅ (CI: zero `FAILED` in the
job log; the fixtures that flake locally are pre-existing host-environment
issues, all independently re-confirmed non-regressions this session);
**(c)** h2spec ≥95% ✅ (CI: `h2spec_pass_rate_gate ... ok`, unmoved — no H2
codec/framing change this phase per SPEC §H); **(d)** no new fuzz target ✅
(SPEC §H — N/A; the fuzz job itself green); **(e)** build/clippy/fmt/test/deny
all clean, confirmed both locally (this session) and in CI (job `success`);
**(f)** `REVIEW.md` not yet authored — that is the state-5 code-review, a
separate session per §5.1.

**Conclusion: the §7.5 phase-done gate is satisfied for phase 58's
implementation surface**, on the authoritative CI run `28753634192` @
`ee0ae95`, corroborated by this session's own local re-verification (clean
modulo 5 documented, independently-reconfirmed host-only flakes unrelated to
this phase). No code changes were needed this session. The next session is
the **§5 state-5 code-review** (`superpowers:requesting-code-review`).
