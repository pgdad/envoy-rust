# Phase 53 — `53-accesslog-rf-upstream-reset` — Implementation Progress

> §5 state-3 implementation log. Each task is RED→GREEN→commit per
> `superpowers:test-driven-development`. Per-task discipline: cargo-fmt-check +
> the Docker differential `0061` are CI-authoritative (memory
> `envoy-rust-state4-ci-first-execution`); `0061` is backend-spawning → expect
> LOCAL-RED on this dev host (memory `differential-host-bridge-ip-192-168-65-2`),
> GREEN on CI.

---

## Task 1 — `tcp-echo-server --close-on-accept` (read-then-close) mode ✅

**RED** (`cargo test -p tcp-echo-server argv_parses_close_on_accept`):
```
error[E0560]: struct `Args` has no field named `close_on_accept`
  --> tests/helpers/tcp-echo-server/src/main.rs:181:17
error: could not compile `tcp-echo-server` (bin "tcp-echo-server" test) due to 2 previous errors
```
(compile-fail — no `close_on_accept` field; `--close-on-accept` would also hit the `Trailing` arm.)

**Implementation:** `Args.close_on_accept: bool`; a `--close-on-accept` arm in
`parse_argv` (before the `_ =>` catch-all); `USAGE` updated; `run_on` gains a
`close_on_accept` param branching the per-conn task between echo (`io::copy`) and
read-then-close (one best-effort `read` to drain the request → `drop(stream)` =
graceful FIN, no response); flag threaded `main → run → run_on`. Updated the two
existing `run_on` test call sites (`echoes_round_trip`, `drain_exits_within_budget`)
to pass `false` + the `argv_parses_port` literal to include the new field.

**GREEN** (`cargo test -p tcp-echo-server`):
```
running 9 tests
test tests::argv_parses_close_on_accept ... ok
test tests::argv_parses_port ... ok
test tests::echoes_round_trip ... ok
test tests::drain_exits_within_budget ... ok
... (9 passed; 0 failed)
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Commit:** `phase 53 task 1: tcp-echo-server --close-on-accept (read-then-close) mode [ADR-0110]`

---

## Task 2 — reset synth status 502 → 503 (§A(i) + §G sweep) ✅

**RED** (`cargo test -p envoy-http1 h1_upstream_reset_returns_503`): a new in-process
test drives a real accept-then-close loopback backend (NO retry_policy) → the genuine
`AttemptOutcome::Reset` arm. Asserting `HTTP/1.1 503` it fired on the pre-change 502:
```
panicked at crates/envoy-http1/src/hcm.rs:7492:9:
upstream-reset surfaces the synth-503 downstream: HTTP/1.1 502 Bad Gateway
```
(confirms the accept-then-close listener drives the POST-connect reset arm, not the
connect-failure arm.)

**Implementation:** `hcm.rs:615` warn `returning 502`→`503`; `:618`
`synth_status(502,…)`→`synth_status(503,…)`; `:1140` comment `reset synth-502`→`503`;
`:4049` doc `send-fail-502`→`send-fail-503`. Whole-crate `grep -n 502` now shows the
only remaining `502` are the new test's own doc comment (describes the RED expectation)
— the production reset path is fully 503. `response.rs:88` "Bad Gateway" reason-phrase
table left UNTOUCHED (generic, still used by cdn_loop's filter-local 502).

**GREEN** (`cargo test -p envoy-http1`):
```
test hcm::tests::h1_upstream_reset_returns_503 ... ok
test result: ok. 149 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
No regression (the whole-crate grep confirmed no live test asserted reset→502).

**Commit:** `phase 53 task 2: reset synth status 502->503 to match Envoy + 502 comment/doc sweep [ADR-0110]`

---

## Task 3 — `reset_for_log` boolean + `%RESPONSE_FLAGS%` = `UC` derive branch (§A(ii) + §B) ✅

**RED** (`cargo test -p envoy-http1 h1_upstream_reset_access_log_carries_uc_flag`): the
accept-then-close reset path wired to a `{rc,rf}` FILE json access-log, asserting
`{"rc":503,"rf":"UC"}`:
```
assertion `left == right` failed: upstream-reset access-log line carries rf:UC: "{\"rc\":503,\"rf\":\"-\"}\n"
  left: "{\"rc\":503,\"rf\":\"-\"}\n"
 right: "{\"rc\":503,\"rf\":\"UC\"}\n"
```
(the reset rcd `via_upstream` falls to the derive's `_ => "-"` arm; no `reset_for_log`
boolean yet.)

**Implementation:** `hcm.rs` — `let mut reset_for_log = false;` decl alongside
`connect_failure_for_log` (`:863`-region); post-loop set
`reset_for_log = matches!(final_outcome, Some(AttemptOutcome::Reset));` immediately after
the `connect_failure_for_log` set (`:1185`); derive `else if reset_for_log { "UC" }`
branch after the `"UF"` branch (`:1346`) + a doc comment. Set ONLY on the reset
final-outcome path → URX/UF/NR/UH/UO arms unreachable-with-it-set → byte-identical.

**GREEN** (`cargo test -p envoy-http1`):
```
test hcm::tests::h1_upstream_reset_returns_503 ... ok
test hcm::tests::h1_upstream_reset_access_log_carries_uc_flag ... ok
test result: ok. 150 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
The new test logs `{"rc":503,"rf":"UC"}`; Task 2's status test still passes; no regression.

**Commit:** `phase 53 task 3: reset_for_log boolean + %RESPONSE_FLAGS%=UC derive branch [ADR-0110]`

---

## Task 4 — `TcpCloseBackend` harness struct (§C(ii)) ✅

Added `TcpCloseBackend` (a near-verbatim clone of `TcpProxyBackend`) to
`tests/differential/src/backend.rs` after `TcpProxyBackend`: `spawn()` reserves a port,
locates `tcp-echo-server`, spawns it with `--port <p> --close-on-accept`,
`wait_accept_ready` (1s), `kill_on_drop(true)`; `port()`; `container_host() =
"host.docker.internal"`; identical 2s-poll SIGKILL Drop. No standalone unit test (same
as `TcpProxyBackend`); exercised by the Task 7 differential test.

**Verify** (`cargo build -p differential --tests`):
```
Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.98s
```
Clean compile (pub struct → no dead-code warning despite not-yet-wired).

**Commit:** `phase 53 task 4: TcpCloseBackend harness struct (accept-then-close) [ADR-0110]`

---

## Task 5 — wire `{{CLOSE_BACKEND_PORT}}` into `run_fixture` (§C/§E) ✅

Added to `tests/differential/src/lib.rs`: (1) the `needs_close_backend` launch arm after
the `{{HTTP2_BACKEND_PORT}}` arm (`scan_needs_marker(...,"CLOSE_BACKEND_PORT")` → spawn
`TcpCloseBackend` → `close_backend_port_str`); (2) the `CLOSE_BACKEND_PORT` push into BOTH
`upstream_kvs` and `subject_kvs` (after each HTTP2 push); (3) `|| close_backend_port_str.is_some()`
added to the `BACKEND_HOST` OR-gate at BOTH sites (upstream `host.docker.internal`, subject
`127.0.0.1`).

**Verify** (`cargo build -p differential --tests`):
```
Compiling differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.17s
```
Clean compile. No fixture references `{{CLOSE_BACKEND_PORT}}` yet → `needs_close_backend`
false for all fixtures 0001–0060 → zero behavior change.

**Commit:** `phase 53 task 5: wire {{CLOSE_BACKEND_PORT}} marker -> TcpCloseBackend in run_fixture [ADR-0110]`

---

## Task 6 — fixture `0061-accesslog-rf-upstream-reset` (§D) ✅

Created 4 files modeled on 0060: `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`,
`README.md`. The cluster is swapped from 0060's dead-literal `127.0.0.1:1` STATIC to a
STRICT_DNS `{{BACKEND_HOST}}:{{CLOSE_BACKEND_PORT}}` cluster (the 0004 `TcpProxyBackend`
shape), NO `circuit_breakers`, NO `retry_policy`, `{rc,rf}` json_format, ONE `GET /`
probe with `expected_status: 503`, cross-proxy-equal `{"rc":503,"rf":"UC"}`. Per-side
deltas match 0060 (envoy-rust omits `admin:`, binds `127.0.0.1`, `0061-envoy-rust-mount`
path) plus the per-side `{{BACKEND_HOST}}` render (host.docker.internal / 127.0.0.1).
README flags: backend-spawning → LOCAL-RED on this dev host (bridge-IP flake), GREEN on
CI (CI-authoritative); the read-then-close POST-connect guarantee; the deferred
deterministic `UC` rcd (M53-1).

**Commit:** `phase 53 task 6: fixture 0061 accept-then-close reset UC witness [ADR-0110]`
