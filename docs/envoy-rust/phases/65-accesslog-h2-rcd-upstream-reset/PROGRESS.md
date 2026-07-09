# Phase 65 — `65-accesslog-h2-rcd-upstream-reset` — PROGRESS

> Running execution log for the §5 **state-3 implementation** of
> `PLAN.md` (6 TDD tasks, scope locked by **ADR-0122**). Each task records its
> observed fail-first output and its observed pass output, verbatim.
>
> **Goal:** differentially witness the deterministic H2 upstream-reset
> `%RESPONSE_CODE_DETAILS%` string
> `upstream_reset_before_response_started{connection_termination}` byte-exact,
> and migrate the H2 `UC` `%RESPONSE_FLAGS%` derivation from the phase-64
> `reset_for_log_h2` boolean onto that now-unique rcd (retiring the boolean).
> **Consumes carry-forward M64-1.**

**Session start state (STEP 0, disk is authoritative):** `git status --porcelain`
clean; branch `main`; `HEAD` = `origin/main` = `d5b6dd4` (the phase-65 state-2
PLAN-write commit). `git fetch origin --prune` confirmed no sibling autonomous-loop
session had advanced phase 65 (no `PROGRESS.md`, no `tests/fixtures/0070-*`, and
`grep -c reset_for_log_h2 crates/envoy-http2/src/hcm.rs` = 5, matching PLAN-VERIFY
§3.4). All PLAN line anchors re-confirmed against the live tree with zero drift.

---

## Task 1 — §A: set the deterministic reset rcd (positive backstop first)

**Files changed:** `crates/envoy-http2/src/hcm.rs` (the post-loop reset set-site;
the extended backstop `h2_upstream_reset_access_log_carries_uc_flag` + its doc
comment).

### Step 1 — extend the existing backstop to assert the rcd (the failing test)

Added an `rcd: %RESPONSE_CODE_DETAILS%` key to the backstop's `json_format`
`BTreeMap` and replaced the final assertion with the full three-key line. Updated
the test doc comment: the phrase describing the rcd as the shared `via_upstream`
(deferred as M64-1) was replaced by the phase-65 rcd-derived description.

### Step 2 — RUN the test, observe it FAIL ✅ (fail-first observed)

`cargo test -p envoy-http2 h2_upstream_reset_access_log_carries_uc_flag -- --nocapture`

```
thread 'hcm::tests::h2_upstream_reset_access_log_carries_uc_flag' (2959356) panicked at crates/envoy-http2/src/hcm.rs:5036:9:
assertion `left == right` failed: upstream-reset access-log line carries the deterministic rcd + rf:UC: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"UC\"}\n"
  left: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"UC\"}\n"
 right: "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}\n"
test hcm::tests::h2_upstream_reset_access_log_carries_uc_flag ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.12s
```

**Exactly the PLAN Step-2 prediction:** the emitted rcd is the shared
`via_upstream` written in-loop at `hcm.rs:757`; `rf` is already `UC` (still
derived from the `reset_for_log_h2` boolean at this point).

### Step 3 — implement §A (the guarded rcd set)

At the post-loop reconciliation region (immediately after the phase-64
`reset_for_log_h2` set), added the guarded rcd set:

```rust
if reset_for_log_h2 && !retry_limit_exceeded_for_log_h2 {
    response_code_details_for_log_h2 = Some(
        "upstream_reset_before_response_started{connection_termination}".to_owned(),
    );
}
```

The `!retry_limit_exceeded_for_log_h2` guard (that boolean is computed just
above, at `hcm.rs:869`) preserves the retry-exhausted-reset edge: such a request
KEEPS `via_upstream` and renders `URX`. The stale trailing sentence of the
phase-64 comment (which described the now-superseded URX-before-UC derive
ordering as the mechanism guarding this combination) was replaced by the §A
comment stating the guard explicitly.

### Step 4 — RUN the test, observe it PASS ✅

```
running 1 test
test hcm::tests::h2_upstream_reset_access_log_carries_uc_flag ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 87 filtered out; finished in 0.12s
```

Emitted line is now
`{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`.
(`rf` still arrives via the boolean at this point — Task 3 migrates it.)

### Step 5 — no in-process regression ✅

`cargo test -p envoy-http2`

```
test result: ok. 87 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.45s
   Doc-tests envoy_http2
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

In particular `h2_retry_limit_exceeded_path_always_503` and
`h2_connect_failure_access_log_carries_uf_flag` stay green — the guard leaves
their rcd untouched. (The documented host-flake
`client::tests::send_request_maps_h2_handshake_failure_to_typed_error`,
memory `envoyrust-h2-handshake-test-host-flake`, did not fire this run.)

**Task 1: COMPLETE.**
