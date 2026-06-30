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
