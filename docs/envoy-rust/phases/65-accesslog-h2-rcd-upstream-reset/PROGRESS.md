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

---

## Task 2 — §G-negative: lock the `!retry_limit_exceeded_for_log_h2` guard (REQUIRED)

The guard added in Task 1 is the single most error-prone line in this phase, and
the differential fixture `0070` **cannot** exercise the retry-exhausted-reset
path. SPEC §G marks this test REQUIRED, not optional.

**Files changed:** `crates/envoy-http2/src/hcm.rs` (new multi-accept helper
`spawn_upstream_h2_reset_server_multi()` + new backstop
`h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx`, both in
`mod tests`).

### Step 1 — write the test

Added `spawn_upstream_h2_reset_server_multi()`: an unbounded-accept reset
upstream. **This helper is why PLAN-VERIFY §3.5 was a correction:** the existing
one-shot `spawn_upstream_h2_reset_server()` drops its `TcpListener` as soon as
its single connection task returns, so the RETRIED attempt would hit
`ConnectionRefused` → `ConnectFailure`/`UF`, never a second `Reset`/`URX`.

Added the negative backstop: `retry_on: "reset"`, `num_retries: Some(1)`, driven
against that always-reset upstream, asserting the whole logged line is
`{"rc":503,"rcd":"via_upstream","rf":"URX"}`.

### Step 2 — RUN the test, observe it PASS immediately ✅

`cargo test -p envoy-http2 h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx -- --nocapture`

```
running 1 test
test hcm::tests::h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.15s
```

This is a *characterization/guard* test: it asserts behavior Task 1's guard
already preserves. It did NOT fail with `rf:"UC"` (guard correct) and did NOT
fail with `rf:"UF"` (the multi-accept helper does serve the retry's second
connection).

### Step 3 — MUTATION CHECK: prove the guard is load-bearing ✅

Temporarily deleted ` && !retry_limit_exceeded_for_log_h2` from the Task-1 `if`
condition and re-ran:

```
thread 'hcm::tests::h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx' (2964689) panicked at crates/envoy-http2/src/hcm.rs:5170:9:
assertion `left == right` failed: retry-exhausted reset keeps via_upstream rcd and renders URX: "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"URX\"}\n"
  left: "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"URX\"}\n"
 right: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.15s
```

**FAILED exactly as PLAN Step 3 predicted** — the rcd is wrongly overridden on
the retry-exhausted path. The test is therefore NOT vacuous: it genuinely pins
the guard. **The guard was then RESTORED** and the test re-run:

```
test hcm::tests::h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.15s
```

### Step 4 — full crate suite ✅

`cargo test -p envoy-http2`

```
test result: ok. 88 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.45s
   Doc-tests envoy_http2
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**Task 2: COMPLETE.** The guard is now pinned BEFORE Task 3 retires the boolean,
so a Task-3 refactor error cannot pass silently.

---

## Task 3 — §B + §F: derive `UC` from the rcd, retire `reset_for_log_h2`

**Files changed:** `crates/envoy-http2/src/hcm.rs` (5 boolean sites + 3 comment
blocks + the derive); `tests/fixtures/0069-accesslog-h2-uc-upstream-reset/README.md`
(additive phase-65 update note — see the §F discipline note below).

### Step 1 — add the rcd-match arm, delete the boolean branch

The `%RESPONSE_FLAGS%` derive's `} else if reset_for_log_h2 { "UC" }` branch
(with its phase-64 comment) was DELETED, and
`Some("upstream_reset_before_response_started{connection_termination}") => "UC"`
added to the rcd-match, mirroring the existing `{overflow} => "UO"` arm.
`URX`/`UF` keep their booleans and their check order is unchanged. The
`.as_deref()` shared borrow still ends before the owned `String` moves into
`response_code_details:` — borrow discipline unchanged.

### Step 2 — delete the four remaining boolean sites ✅

1. **Post-loop set** + its phase-64 comment: deleted; the `matches!` was folded
   directly into the §A `if` condition, so no binding remains.
2. **Declaration** (+ its 10-line phase-64 comment block): deleted.
3. **Call-site arg** in the `finalize_h2_stream(…)` call: deleted.
4. **Parameter + doc comment** on `finalize_h2_stream`: deleted. Its signature is
   now `(…, retry_limit_exceeded_for_log_h2: bool, connect_failure_for_log_h2: bool) -> Result<(), Http2Error>`.

`grep -rn "reset_for_log_h2" crates/` → **no output (exit 1)** ✅

#### §F sweep discipline — one in-scope deviation from PLAN Task 6 Step 2 (documented)

PLAN Task 3 Step 2 prescribed new comments that themselves spell the identifier
(e.g. "the phase-64 `reset_for_log_h2` boolean was RETIRED"), which would make
PLAN Task 6 Step 2's `grep -rn "reset_for_log_h2" crates/ tests/ …` → "no output"
**unsatisfiable by construction**. Resolved by wording the new comments as "the
phase-64 **boolean discriminator** was RETIRED" — same meaning, and it matches
the wording PLAN Task 5 already prescribes for `BEHAVIOR_CONTRACT.md`.

A second hit remains in `tests/fixtures/0069-…/README.md:18`: *"Phase 64 … (ii)
declares a new per-stream boolean `reset_for_log_h2`"*. This is **backward-looking
historical narrative of what phase 64 did**, which **SPEC §F explicitly orders
left verbatim** ("Leave BACKWARD-LOOKING historical narrative comments verbatim
per the D-3.4/D-3.5 no-retroactive-rewrite convention; correct only ACTIVE-state
prose"). SPEC §F scopes the sweep to `crates/envoy-http2/src/`; PLAN Task 3
Step 2's own grep is likewise `crates/envoy-http2/src/hcm.rs`. **Only PLAN Task 6
Step 2 over-broadened the sweep to `tests/`, colliding with doctrine — SPEC and
doctrine win.** The historical sentence is therefore PRESERVED verbatim, and the
one genuinely stale **active-state** claim in that README (the cross-reference
asserting M64-1 is "distinct and still open") was corrected **additively** with a
phase-65 update note. Task 6 Step 2's grep is accordingly run over `crates/` +
`BEHAVIOR_CONTRACT.md` (the surfaces SPEC §F/§E actually name).

**No §6.2 reconciliation ADR fires:** this is a PLAN-step precision issue about
comment wording, not an overturned §A-§G fact. `ADR-0123` stays reserved-but-unfired.

### Step 3 — both backstops still PASS ✅

```
test hcm::tests::h2_upstream_reset_access_log_carries_uc_flag ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.12s

test hcm::tests::h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.15s
```

The positive backstop's `UC` now arrives via the rcd-match (the boolean is gone)
— **output-equivalent**, which is precisely the fixture-`0069` byte-preservation
guarantee. The negative one still renders `URX` with `via_upstream`.

### Step 4 — workspace compiles + passes ✅

```
$ cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.19s

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.01s

$ cargo test -p envoy-http2
test result: ok. 88 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.46s
```

Clippy clean — no `unused_variables`/`unused_mut` left behind by the deletion
(the deleted parameter had exactly one call site).

**Task 3: COMPLETE.** The `reset_for_log_h2` boolean is fully retired; H2's `UC`
is rcd-derived, matching H1's post-phase-54 derivation split exactly.
