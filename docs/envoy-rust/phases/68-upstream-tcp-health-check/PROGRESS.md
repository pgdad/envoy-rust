# Phase 68 — Active TCP Health Checking — PROGRESS

> **State-3 implementation** (`superpowers:executing-plans` + `superpowers:test-driven-development`).
> One `PLAN.md` task per section; each was TDD (failing test first → RED → minimal
> impl → GREEN → commit). This file is the running log for the state-4 verification
> session (D-3.4: readable by a stranger with zero prior context).

## Session summary

Executed all 7 `PLAN.md` tasks in order under TDD. Landed active **TCP** health
checking (`tcp_health_check`) reusing the entire phase-12 `envoy-health` scheduler
/ `EndpointHealth` state machine / ejection / `pick()` exclusion / the
`cluster.<n>.health_check.*` + `membership_*` stat tree + the fixture-0019
`http1_after_settle` differential harness. Added only: the config schema + decode
+ validation, the L4 probe, the scheduler dispatch, fixture 0074, the
BEHAVIOR_CONTRACT subsection, and a fuzz corpus seed. `#![forbid(unsafe_code)]`
holds; no new deps beyond `base64 = "0.22"` (envoy-config) + the `net`/`io-util`
tokio features (envoy-health). No new stat names.

**Deviation from PLAN task/commit granularity (recorded):** the PLAN placed the
Task-2 parse tests (`parses_empty_tcp_health_check_connection_only`,
`parses_tcp_health_check_send_receive`) in Task 2 with an "expected PASS" at Task
2's commit. Those tests call `crate::parse_bootstrap`, which runs the validator,
and a TCP-only checker cannot validate until the **Task-3** validator restructure
(the pre-existing validator hard-required `http_health_check` via `ok_or_else` at
`bootstrap.rs:4769`). To preserve green-commit discipline (D-3.6), Tasks 2 and 3
were implemented together and landed in a single green commit. All Task-2 and
Task-3 tests are present and green; only the commit boundary changed.

## Task-by-task log

### Task 1 — `HealthCheckPayload` schema + hex/base64 decode + `ConfigError` variants
- Added `base64 = "0.22"` to `crates/envoy-config/Cargo.toml`.
- Added `HealthCheckPayload { text, binary }`, `PayloadDecodeError { InvalidHex, InvalidBase64, Empty }`,
  `HealthCheckPayload::decode()`, and a hand-rolled `decode_hex()` in `bootstrap.rs`.
- Added 3 `ConfigError` variants in `lib.rs`: `InvalidHealthCheckPayloadHex`,
  `InvalidHealthCheckPayloadBase64`, `EmptyHealthCheckPayload`.
- RED: `cargo test -p envoy-config payload_decode` → 12 compile errors (types undefined).
- GREEN: `cargo test -p envoy-config payload_decode` → **6 passed**.
- Commit: `phase 68: HealthCheckPayload schema + hex/base64 decode + ConfigError variants`.

### Task 2 + Task 3 — `TcpHealthCheck` field + validator TCP arm (merged commit, see Deviation above)
- `TcpHealthCheck { send, receive }` + `HealthCheck.tcp_health_check: Option<TcpHealthCheck>`.
- Repointed the pinning test `cluster_rejects_unknown_health_check_field`
  (`bootstrap.rs:14942`) from `tcp_health_check: {}` (now supported) to
  `grpc_health_check: {}` (still `deny_unknown_fields`-rejected).
- `lib.rs`: updated `UnsupportedHealthCheckType` message (neither http nor tcp);
  added `BothHttpAndTcpHealthCheck`; re-exported `HealthCheckPayload` /
  `PayloadDecodeError` / `TcpHealthCheck`.
- Restructured `validate_health_checks` (`bootstrap.rs:4768`): both-checkers
  rejection (oneof), neither→Unsupported, shared threshold/timing, per-checker
  validation (HTTP path/statuses; TCP payload decode → typed errors).
- RED: the Task-2 parse tests failed at `deny_unknown_fields` (field unknown),
  then at `UnsupportedHealthCheckType` (validator) pre-restructure; the Task-3
  validator tests failed on the missing `BothHttpAndTcpHealthCheck` variant.
- GREEN: `cargo test -p envoy-config` → **601 passed, 0 failed**.
- Commit: `phase 68: TcpHealthCheck field + validator TCP arm ...; repoint pinning test to grpc_health_check`.

### Task 4 — L4 TCP probe (`tcp_probe_once` / `tcp_probe_loop` / `receive_matches`)
- Added `net` + `io-util` to `envoy-health`'s tokio features.
- `receive_matches` (pure, contiguous-substring, sequential multi-block),
  `find_subslice`, `TcpProbeError`, `tcp_probe_once` (ONE `timeout(probe_timeout)`
  bounds connect + send + receive-scan), `tcp_probe_loop` (interval ticker +
  cancel branch, sibling of `probe_loop`).
- RED: `cargo test -p envoy-health receive_matches` → 15 compile errors (undefined).
- GREEN: `cargo test -p envoy-health` → **16 passed** (3 matcher + 5 probe-integration
  against ephemeral `TcpListener`s + the existing 8).
- Commit: `phase 68: L4 TCP probe (connect/send/receive-scan) + pure receive_matches`.

### Task 5 — Scheduler HTTP-vs-TCP dispatch
- `Scheduler::spawn` selects the checker type by presence (validator guarantees
  exactly one); re-decodes TCP payloads at spawn (defense-in-depth, the
  `parse_duration` precedent); counters/ejection/`pick()` untouched.
- RED: `cargo test -p envoy-health spawns_tcp_probe` → panic at the old
  `http_health_check.expect(...)`.
- GREEN: `cargo test -p envoy-health` → **17 passed**; `cargo build -p envoy-bin` clean
  (call site unchanged — dispatch is internal to `Scheduler::spawn`).
- Commit: `phase 68: Scheduler dispatches HTTP vs TCP probe by checker presence`.

### Task 6 — fixture 0074 + `DEAD_BACKEND_PORT` harness marker
- `tests/differential/src/lib.rs`: `DEAD_BACKEND_PORT` marker — `reserve_port()`,
  NO listener spawned (hermetic ECONNREFUSED); pushed into both kv maps; extended
  the BACKEND_HOST gate on both sides.
- `tests/fixtures/0074-upstream-tcp-health-check/` — `envoy.yaml`, `envoy-rust.yaml`
  (connection-only `tcp_health_check: {}`, `unhealthy_threshold: 2`, STRICT_DNS to
  `{{BACKEND_HOST}}:{{DEAD_BACKEND_PORT}}`), `expectations.yaml` (http1_after_settle,
  settle 3500 ms, 503 + `no healthy upstream` byte-exact), `README.md`.
- Added the runner `tests/differential/tests/upstream_tcp_health_check.rs`
  (mirrors the 0019 runner) — required for the fixture to be exercised.
- Health-aware backend spawn is gated on `{{BACKEND_PORT}}`, which 0074 does NOT
  use, so no backend spawns (intended).
- GREEN (LIVE differential, Docker up on this host): `cargo test -p differential
  --test upstream_tcp_health_check` → **1 passed** in 6.08s. The subject ejected
  the endpoint (`no healthy endpoint for cluster tcp_hc_backend`) and both proxies
  converged to synth-503; the 5-axis equivalence cascade passed.
- Commit: `phase 68: fixture 0074 (connection-only TCP-HC ejection) + DEAD_BACKEND_PORT marker`.

### Task 7 — `BEHAVIOR_CONTRACT.md` subsection + `parse_bootstrap` fuzz seed
- Added the `## Active TCP health check (tcp_health_check)` behavior section
  (MEASURED facts: connection-only, `Payload` oneof, both-checkers oneof, the
  contiguous-substring `receive` scan single-block-pinned, the whole-probe timeout,
  the outcomes/stats) + a "68 entries" note in the Stat-name mapping table (same
  stat tree, no new names).
- Added un-ignored fuzz corpus seed
  `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_tcp_health_check.yaml`
  (send hex + receive hex/base64) with a `!`-un-ignore line in the fuzz
  `.gitignore`. Verified `git ls-files` tracks it. NO new fuzz TARGET → no
  `ci.yml` change (ADR-0137 §7.4).
- Fuzz smoke: `cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap
  -- -runs=0` → loaded 6870 corpus files (incl. the new seed), **no crash**.
- Commit: `phase 68: BEHAVIOR_CONTRACT tcp_health_check subsection + parse_bootstrap fuzz seed`.

## What the state-4 verification session must run (full §7.5 gate)

The state-4 session runs `superpowers:verification-before-completion` over the
whole tree (memory `envoy-rust-state4-ci-first-execution` — the fmt-check +
Docker differential first run at the state-4 gate):
`cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test
--workspace` (`--no-fail-fast`, redirect to a file — memories
`never-pipe-verification-runs-through-tail`, `local-red-set-varies-run-to-run`),
`cargo deny check`, the fuzz short-budget CI run, and the differential suite
(rebuild `target/debug/envoy-bin` first — memory
`differential-harness-uses-debug-envoy-bin`; the documented host-flake set is
CI-authoritative). Quote all outputs into this file.

## §5 state-4 verification — full §7.5 gate run (`superpowers:verification-before-completion`)

> Session cold-started clean (`git status --porcelain` empty, branch `main`,
> `HEAD` = `9ac38d8`, `git fetch origin --prune` showed no sibling ahead). **STEP
> 0.5:** CI run `29216603720` for the FULL 40-char SHA
> `9ac38d89a710972daeb55041a3933edb7d83dedf` is `completed`/`success`. Below is
> the state-4 gate re-run over the whole tree with every command's output quoted
> (memory `envoy-rust-state4-ci-first-execution` — fmt-check + Docker differential
> first run at THIS gate; the gate is run itself, not skipped because CI is green).

**(1) `cargo build --workspace --all-targets`** — EXIT 0.
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 17.40s
```

**(2) `cargo clippy --workspace --all-targets --all-features -- -D warnings`** — EXIT 0, zero warnings.
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
```

**(3) `cargo fmt --all -- --check`** — EXIT 0 (no output = fully formatted; fmt-check
first runs at this gate and is clean, no `cargo fmt --all` re-format needed).

**(4) `cargo test --workspace --no-fail-fast`** (redirected to a file — memories
`never-pipe-verification-runs-through-tail`, `local-red-set-varies-run-to-run`) —
EXIT 101; **1973 passed, 6 failed** across all binaries. The phase-68 fixture
`0074` (`upstream_tcp_health_check_fixture`) is **GREEN** locally:
```
2026-07-13T06:38:11Z  WARN no healthy endpoint for cluster — returning 503 cluster=tcp_hc_backend
test upstream_tcp_health_check_fixture ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.13s
```
All 6 local REDs are in the `differential` crate and each maps to a DOCUMENTED
host-environmental flake; the SAME tree passes all 6 on CI (run `29216603720`,
`success`), so none is a phase regression (`local passed+failed` = 1979 = `CI
passed`):

| Failing test | Local signature | Documented flake |
|---|---|---|
| `access_log_h2_rcd_upstream_reset` | ref Envoy routes host-spawned close backend to unreachable IPv6 `[fdc4:f303:9324::254]` (`immediate_connect_error: Network is unreachable`) → `UF`; envoy-rust reaches it → `UC` | `tcpclosebackend-ipv6-unreachable-host-flake` |
| `access_log_h2_uc_upstream_reset` | same IPv6-unreachable `UF` vs `UC` | `tcpclosebackend-ipv6-unreachable-host-flake` |
| `access_log_rcd_upstream_reset` | same | `tcpclosebackend-ipv6-unreachable-host-flake` |
| `access_log_rf_upstream_reset` | same (`envoy="{\"rc\":503,\"rf\":\"UF\"}"` vs `envoy-rust="{...\"rf\":\"UC\"}"`) | `tcpclosebackend-ipv6-unreachable-host-flake` |
| `admin_config_dump_server_info` | envoy-only `/clusters` lines `backend::192.168.65.2:34247::…` — ref Envoy routes the backend via the host bridge IP `192.168.65.2` (not allow-listed) | `differential-host-bridge-ip-192-168-65-2` |
| `xds_file_based_eds` | `upstream Envoy never became accept-ready within 10s: Connection refused` — port-reuse/parallel-load startup race | `eds-fatal-startup-test-port-reuse-flake` / `differential-fixtures-flake-under-parallel-load` |

Adjudication per memory `local-red-set-varies-run-to-run` (re-run each member in
isolation — the environmental host-networking ones fail DETERMINISTICALLY, the
parallel-load startup one PASSES):
- `cargo test -p differential --test xds_file_based_eds` (isolation) → **EXIT 0**,
  `test result: ok. 1 passed; 0 failed` — parallel-load startup flake, NOT a regression.
- `cargo test -p differential --test access_log_rf_upstream_reset` (isolation) →
  EXIT 101, SAME `UF` vs `UC` signature — deterministic host-networking (this host's
  Docker cannot reach the host-spawned close backend), environmental; CI (correct
  networking) passes it. NOT a regression, fixture NOT weakened.

**No `gh run rerun --failed`** is needed — CI on the HEAD SHA is ALREADY
`success` (there is no red CI run to rerun); the rerun guidance only applies when
CI itself reds.

**(5) `cargo deny check`** — EXIT 0.
```
advisories ok, bans ok, licenses ok, sources ok
```
(The `license-not-encountered` lines are pre-existing unmatched-allowance warnings
in `deny.toml`, unrelated to phase 68.)

**(6) `cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`**
(the §7.4 CI-style short-budget run; memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`;
NO new target — a `parse_bootstrap` seed only, so no `ci.yml` change) — EXIT 0, no crash.
```
INFO:     6870 files found in .../fuzz/corpus/parse_bootstrap   (incl. the new seed)
#9359	DONE   cov: 15244 ft: 30658 corp: 2589/1689Kb ...
Done 9359 runs in 91 second(s)
```

**(7) The differential suite for the HC surface** — `target/debug/envoy-bin`
rebuilt by the `--all-targets` build above (memory
`differential-harness-uses-debug-envoy-bin`); fixture `0074`
(`upstream_tcp_health_check_fixture`) GREEN locally (quoted in (4)) and on CI; the
pre-existing `0001`–`0073` stay green modulo the documented host-flake set (§7.5
(a)+(b)).

**Gate verdict: GREEN.** Every §7.5 command passes; the only local REDs are 6
DOCUMENTED host-environmental flakes (all green on the HEAD-SHA CI run). The
implementation is verified. Advancing `STATE.md` to §5 state-5 (code-review).

## Traps honored
- No CidrRange changes (M-1 untouched); no revert of landed 67/12 work; no fixture
  weakened; `known-failures.txt` untouched; no ROADMAP malformed-row "fixes".
- `next-prompt.txt` is gitignored (`.gitignore:9`) — refreshed on disk, not `git add`ed.
- `#![forbid(unsafe_code)]` holds at every crate root.
