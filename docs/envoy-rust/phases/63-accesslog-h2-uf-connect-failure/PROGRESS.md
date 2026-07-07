# Phase 63 — `63-accesslog-h2-uf-connect-failure` — PROGRESS

> State-3 implementation log. Records what landed per task, the exact files
> touched, and the local command outputs (Tasks 1/2/6). The state-4
> verification (the full §7.5 gate, run in CI) is a separate future session.

## Task 1 — H2 connect-failure synth status 502→503 (commit `3c6ef1d`)

Flipped the 2 existing 502-asserting tests to 503 (RED), declared
`synth_h2_connect_failure()` (adjacent to `synth_h2_502()`, `hcm.rs`), redirected
the SOLE `AcquireOutcome::ConnectFailure` match arm to call it, and fixed the 3
stale "returning 502" `tracing::warn!` strings (GREEN).

**File:** `crates/envoy-http2/src/hcm.rs`

RED (`cargo test -p envoy-http2 h2_connect_failure -- --nocapture`):
```
assertion `left == right` failed: downstream must be connect-failure synth-503
  left: 502
 right: 503
test result: FAILED. 0 passed; 2 failed; ...
```

GREEN (same command, post-fix):
```
test hcm::tests::h2_connect_failure_synth_does_not_tick_upstream_rq_5xx ... ok
test hcm::tests::h2_connect_failure_retried_on_connect_failure_policy ... ok
test result: ok. 2 passed; 0 failed; ...
```

Full `envoy-http2` suite (collateral check): `cargo test -p envoy-http2` →
**85 passed; 0 failed; 1 ignored.**

## Task 2 — thread `connect_failure_for_log_h2` + render `UF` + backstop (commit `2861a6e`)

Declared the discriminator boolean + the loop-scoped `final_outcome_h2` capture,
captured it every retry-loop iteration, set the boolean post-loop from the
final attempt's outcome, threaded it through `finalize_h2_stream`'s signature
and its sole call site, extended the `%RESPONSE_FLAGS%` derive with an
`else if connect_failure_for_log_h2 { "UF" }` branch ordered after `URX`, and
added a new in-process backstop `h2_connect_failure_access_log_carries_uf_flag`.

**File:** `crates/envoy-http2/src/hcm.rs`

RED (`cargo test -p envoy-http2 h2_connect_failure_access_log_carries_uf_flag -- --nocapture`):
```
assertion `left == right` failed: connect-failure access-log line carries rf:UF: "{\"rc\":503,\"rf\":\"-\"}\n"
  left: "{\"rc\":503,\"rf\":\"-\"}\n"
 right: "{\"rc\":503,\"rf\":\"UF\"}\n"
test result: FAILED. 0 passed; 1 failed; ...
```

GREEN (same command, post-fix): `test result: ok. 1 passed; 0 failed; ...`

Full `envoy-http2` suite (collateral check on NR/UH/UO/URX backstops):
`cargo test -p envoy-http2` → **86 passed; 0 failed; 1 ignored** (the new
backstop adds one to Task 1's 85). `h2_route_miss_access_log_carries_nr_flag`,
`h2_host_miss_access_log_carries_nr_flag`, `h2_no_healthy_access_log_carries_uh_flag`,
`h2_pool_overflow_access_log_carries_uo_flag`,
`h2_request_budget_overflow_access_log_carries_uo_flag`, and
`h2_retry_limit_exceeded_access_log_carries_urx_flag` all unchanged.

## Task 3 — fixture `0068-accesslog-h2-uf-connect-failure` (commit `3c46492`)

Created the 4-file fixture (`envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/
`README.md`) — the `0066` H2C/H2-upstream shape minus `circuit_breakers`
(mirrors the `0060`-minus-`0058` H1 delta). YAML pair diff: the admin line,
bind-address, and log-path hunks, plus one comment-only hunk on the cluster
(the SAME 4-hunk shape fixture `0066` itself exhibits — 3 substantive + 1
cosmetic comment block).

## Task 4 — differential test `access_log_h2_uf_connect_failure.rs` (commit `84a4df7`)

Thin `differential::run_fixture` wrapper, structural clone of
`access_log_h2_rf_overflow.rs`. `cargo build -p differential --tests` — clean.
Rebuilt debug `envoy-bin` (`cargo build -p envoy-bin`) — clean.

**Ran locally (this host's Docker bridge worked for this run):** the new
`access_log_h2_uf_connect_failure` differential test PASSED —
`test access_log_h2_uf_connect_failure ... ok` (real Envoy v1.33.0 vs
envoy-rust, byte-exact `{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}`
on both sides). Per memory `envoy-rust-state4-ci-first-execution`, the
authoritative green/red verdict is still deferred to the state-4 CI gate; this
local pass is a bonus confirmation, not a substitute.

## Task 5 — BEHAVIOR_CONTRACT.md updates (commit `9683782`)

Updated the `%RESPONSE_FLAGS%` row's H2-witness sentence (now records `UF`
witnessed on H2 by fixture `0068`, ADR-0120, leaving only `UC` open in
carry-forward M56-1) and the `%RESPONSE_CODE_DETAILS%` row's H2-witness
sentence (records the H2 connect-failure rcd stays the shared `via_upstream`
+ non-deterministic transport-failure reason, `rf`=`UF` is the discriminating
signal, rcd omitted from the fixture).

## Task 6 — local verification sweep

- `cargo build --workspace --all-targets` → clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` →
  clean (the `#[allow(unused_assignments)]` on `final_outcome_h2` suppresses
  the expected lint, mirroring `final_retriable`).
- `cargo fmt --all -- --check` → one reflow needed (the `final_outcome_h2`
  type declaration line-wrapped); `cargo fmt --all` applied and committed
  separately (commit `c3a1ab7`, per memory
  `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase).
- `cargo test --workspace --no-fail-fast` → 8 pre-existing target failures,
  ALL matching documented host-flake memory entries, NONE related to phase
  63's change:
  - `access_log_file_sink`, `access_log_json_format`, `admin_drain_listeners`,
    `admin_ready` — "upstream Envoy never became accept-ready" (Docker
    accept-ready flake on this host, memory `envoy-rust-state4-ci-first-execution`
    / `differential-fixtures-flake-under-parallel-load`).
  - `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset` — the
    documented `TcpCloseBackend IPv6-unreachable-host-flake` (fixtures
    0061/0062; real Envoy resolves the accept-then-close backend to an
    unreachable IPv6 address on this host, reporting `UF`/connect-failure
    instead of the intended reset/`UC`).
  - `admin_config_dump_server_info` — the documented
    `differential-host-bridge-ip-192-168-65-2` divergence (this host routes
    the backend via `192.168.65.2`, not the allow-listed bridge IP).
  - `envoy-http2 --lib` (`client::tests::send_request_maps_h2_handshake_failure_to_typed_error`)
    — the documented `envoyrust-h2-handshake-test-host-flake` (non-deterministic
    on this host); re-ran `cargo test -p envoy-http2` alone afterward and it
    PASSED (86/86), confirming the flake.
  - **This phase's own new artifacts** — the 2 flipped Task-1 tests, the new
    Task-2 backstop, AND the new differential test `access_log_h2_uf_connect_failure`
    — all GREEN, every run.
- `cargo deny check` → clean (`advisories ok, bans ok, licenses ok, sources ok`;
  5 pre-existing `license-not-encountered` informational warnings, not errors).
- Byte-preservation re-grep (`for f in 0009 0010 0018 0021 0064 0065 0066 0067`)
  → matches the PLAN §3 item 2 derivation exactly: `0021`'s `circuit_breakers`
  (headroom, reachable backend), `0065`'s `127.0.0.1:1` (comment-only, excluded
  pre-dial), `0066`'s `circuit_breakers` (pending-gate), `0067`'s
  `retry_policy` (real always-503 upstream) — NONE reaches
  `AcquireOutcome::ConnectFailure`; `0001`-`0067` stay byte-identical.

## ADR-0121

Stayed reserved-but-unfired. The §3 PLAN-VERIFY re-confirmation (done at
PLAN-write) confirmed all SPEC §A-§M facts held against the live tree; no
§6.2 reconciliation fired during implementation either — every replace-block
in PLAN.md matched the live tree's surrounding context exactly at edit time.

## Docker differential — deferred to state-4 CI (per PLAN, host limitation)

Per memory `envoy-rust-state4-ci-first-execution` /
`differential-host-bridge-ip-192-168-65-2`, this dev host's Docker
differential runs are not authoritative pass/fail gates — even though the new
`0068` fixture's differential test happened to pass locally this run
(reported above), the AUTHORITATIVE verdict for fixture `0068` green + all
`0001`-`0067` simultaneously green + h2spec ≥95% is the state-4 CI run, not
this session's local run.

## Commits this session (state-3 implementation)

```
3c6ef1d phase 63 task 1: H2 connect-failure synth status 502->503 (new helper + redirect + 2 tests + 3 warns) [ADR-0120]
2861a6e phase 63 task 2: thread connect_failure_for_log_h2 + render UF + in-process backstop [ADR-0120]
3c46492 phase 63 task 3: fixture 0068-accesslog-h2-uf-connect-failure (one probe, H2 rf:UF byte-exact) [ADR-0120]
84a4df7 phase 63 task 4: differential test access_log_h2_uf_connect_failure (fixture 0068) [ADR-0120]
9683782 phase 63 task 5: BEHAVIOR_CONTRACT rf/rcd rows — H2 UF witnessed (fixture 0068) [ADR-0120]
c3a1ab7 phase 63: cargo fmt [ADR-0120]
```

## State-4 verification

> `superpowers:verification-before-completion`, run this session. STEP 0 /
> STEP 0.5 confirmed clean: `git status --porcelain` empty, branch `main`,
> `HEAD` = `origin/main` = `3c7e0b4` (unmoved since the state-3 session — no
> sibling advanced phase 63 or moved `main` in between).

### Local §7.5 pre-flight (re-confirming what state-3 already ran)

**`cargo build --workspace --all-targets`** — clean:
```
   Compiling http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.40s
```

**`cargo clippy --workspace --all-targets --all-features -- -D warnings`** — clean:
```
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Checking http2-echo-server v0.0.0 (/home/esa/git/envoy-rust/tests/helpers/http2-echo-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.94s
```

**`cargo fmt --all -- --check`** — clean, exit 0, no diff output.

**`cargo test --workspace --no-fail-fast`** — **4 failures this run, ALL matching
documented host-flake memory entries, NONE related to phase 63's change** (a
different subset of the flake population than the state-3 session's 8 —
expected, per memory `differential-fixtures-flake-under-parallel-load`: the
Docker differential is non-deterministic under parallel load on this host, so
the exact flaky-test set varies run to run):

- `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset` — the
  documented `tcpclosebackend-ipv6-unreachable-host-flake` (fixtures
  0061/0062): real Envoy resolved the accept-then-close backend to an
  unreachable IPv6 address (`[fdc4:f303:9324::254]`) and reported
  `UF`/`remote_connection_failure`, where envoy-rust reported its intended
  `UC`/`connection_termination` — a host-networking artifact, not a code
  regression.
  ```
  envoy="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:35203}\",\"rf\":\"UF\"}"
  envoy-rust="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"
  ```
- `access_log_route_name` — `"upstream Envoy never became accept-ready"` /
  `Connection refused (os error 111)` — the documented Docker accept-ready
  flake class (memory `envoy-rust-state4-ci-first-execution` /
  `differential-fixtures-flake-under-parallel-load`).
- `admin_config_dump_server_info` — the documented
  `differential-host-bridge-ip-192-168-65-2` divergence (`backend::192.168.65.2:...`
  present on the envoy-only side; this host routes the backend via
  `192.168.65.2`, not an allow-listed IP).

**This phase's own new/changed artifacts stayed green**: `envoy-http2`'s two
flipped Task-1 tests, the new Task-2 backstop `h2_connect_failure_access_log_carries_uf_flag`,
and the new differential test `access_log_h2_uf_connect_failure` (not in the
4-failure set above — full log saved at
`/tmp/claude-1000/-home-esa-git-envoy-rust/9ddf91ea-fd86-4065-91a8-ece477df3cfe/scratchpad/phase63_state4_test_output.log`
this session, not committed — a local scratch artifact).

**`cargo deny check`** — clean:
```
advisories ok, bans ok, licenses ok, sources ok
```
(5 pre-existing `license-not-encountered` informational warnings — not errors,
same set as every prior phase.)

### CI-authoritative Docker differential + h2spec + fuzz (run `28863138464` @ `3c7e0b4`)

Per memory `envoy-rust-state4-ci-first-execution`, this dev host's Docker
differential is non-deterministic under parallel load — the Docker-gated
surface is CI-authoritative this session. `HEAD` (`3c7e0b4`) was already
pushed by the state-3 session and CI already ran; re-confirmed via
`gh run view 28863138464 --json ...` this session that `origin/main` is
STILL `3c7e0b4` (no sibling moved it) and the run is `conclusion: success`
for BOTH jobs:

```
{"conclusion":"success","id":28863138464,"jobs":[
  {"conclusion":"success","name":"fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse, 30s each)"},
  {"conclusion":"success","name":"build + test + lint"}
],"sha":"3c7e0b49121bee6ee2805ef83e11238791a7b0cf","status":"completed"}
```

Pulled the full `build + test + lint` job log (`gh run view 28863138464
--job=85606481629 --log`, 3543 lines) and extracted:

- **Fixture `0068`'s own differential test — green.** `test access_log_h2_uf_connect_failure ... ok`. The preceding warn line confirms the Task-1 status fix landed on the CI runner too: `H2 pool connect failed — returning 503 cluster=backend_cluster addr=127.0.0.1:1 error=UpstreamConnect { ... ConnectionRefused ... }` (503, not the pre-phase-63 502).
- **Zero `FAILED` lines anywhere in the job log** (`grep -c "FAILED"` → `0`) and zero `##[error]` / non-zero-exit-code lines — confirming `0001`-`0067` stay green alongside `0068` simultaneously (the additivity invariant holds on CI, not just locally). 141 `test result: ok` summary blocks across the whole job (unit suites + differential fixtures + h2spec + conformance harness's own unit tests).
- **`h2spec_pass_rate_gate` passed**: `test h2spec_pass_rate_gate ... ok` (the gate's own internal ≥95% threshold check is inside the test body — a passing test result means the threshold held; no H2 codec/framing change this phase, so the pass rate is unmoved from phase 61's baseline, as projected).
- **`cargo fmt --all -- --check`** and **`cargo clippy --workspace --all-targets --all-features -- -D warnings`** CI steps both completed with no `##[error]` lines (job-level `conclusion: success` covers this — a failing fmt/clippy step would have failed the job).
- **`cargo deny check`** on CI: `advisories ok, bans ok, licenses ok, sources ok` (same 5 informational license-not-encountered warnings as local).
- **Fuzz job** (`gh run view 28863138464 --job=85606481573 --log`) — all 4 existing targets (`parse_bootstrap`, `jwt_parse`, `cdn_loop_parse`, `accesslog_format_parse`) ran their 30s short-budget clean; the log's tail shows `accesslog_format_parse`'s corpus-minimization summary ending `Done 2239492 runs in 31 second(s)` with no crash/leak report. No new fuzz target this phase (SPEC §M) — nothing new to wire in, and nothing new to check.

### §7.5 gate-item disposition table

| Gate item | Disposition | Evidence |
|---|---|---|
| (a) new/changed fixture `0068` green | ✅ | CI job log: `test access_log_h2_uf_connect_failure ... ok`; local state-3 run also passed (bonus, not authoritative) |
| (b) all pre-existing fixtures `0001`-`0067` still green | ✅ | CI job log: zero `FAILED` lines anywhere; 141 `test result: ok` blocks; additivity invariant holds on CI |
| (c) conformance suites (h2spec) pass at declared threshold | ✅ | CI job log: `test h2spec_pass_rate_gate ... ok`; no H2 codec/framing change this phase, threshold unmoved from phase 61 |
| (d) any new fuzzer ran clean for its short-budget CI run | ✅ N/A | No new fuzz target this phase (SPEC §M); existing 4 targets (`parse_bootstrap`/`jwt_parse`/`cdn_loop_parse`/`accesslog_format_parse`) ran clean on CI, no crash/leak |
| (e) build/clippy/fmt/test/deny clean | ✅ | Local: all 5 clean this session (test: 4 pre-existing host-flakes, none related to phase 63). CI: job `conclusion: success` for both jobs, zero `FAILED`/`##[error]` lines, `cargo deny check` clean |
| (f) `REVIEW.md` approved | ⏳ pending | State-5 code-review is a separate future session (§5.1: one state per session) |

**Verdict: §7.5 gate items (a)-(e) are ALL GREEN, confirmed both locally and
via CI run `28863138464` @ `3c7e0b4`.** Item (f) is the state-5 code-review,
not attempted this session.
