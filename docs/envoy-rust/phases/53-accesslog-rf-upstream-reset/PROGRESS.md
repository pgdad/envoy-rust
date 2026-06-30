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
