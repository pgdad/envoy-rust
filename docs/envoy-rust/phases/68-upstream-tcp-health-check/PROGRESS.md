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

## Traps honored
- No CidrRange changes (M-1 untouched); no revert of landed 67/12 work; no fixture
  weakened; `known-failures.txt` untouched; no ROADMAP malformed-row "fixes".
- `next-prompt.txt` is gitignored (`.gitignore:9`) — refreshed on disk, not `git add`ed.
- `#![forbid(unsafe_code)]` holds at every crate root.
