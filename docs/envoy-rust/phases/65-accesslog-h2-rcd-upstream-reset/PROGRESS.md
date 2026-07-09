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

---

## Task 4 — §C + §D: fixture `0070` + the differential test

**Files created:** `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`,
`tests/differential/tests/access_log_h2_rcd_upstream_reset.rs`.

**ZERO harness / `ci.yml` / allowlist change** — the `{{H2_CLOSE_BACKEND_PORT}}`
marker auto-spawns `Http2CloseBackend` via the existing launch arm in
`tests/differential/src/lib.rs`; `tests/differential/tests/*.rs` are
cargo-auto-discovered (no `[[test]]` entries in its `Cargo.toml`).

### Steps 1-5 — the four fixture files + the thin test wrapper

A structural clone of `0069`, the ONLY change being the added
`rcd: "%RESPONSE_CODE_DETAILS%"` json_format key. Per-side deltas are exactly the
documented ones `0069` already uses (bind address, `admin:` block,
`{{BACKEND_HOST}}`).

### Step 6 — rebuild the DEBUG binary, then run the fixture

`cargo build -p envoy-bin` was re-run FIRST (memory
`differential-harness-uses-debug-envoy-bin` — the harness runs
`target/debug/envoy-bin`, so a stale binary would RED with `unknown field`).

`cargo test -p differential --test access_log_h2_rcd_upstream_reset -- --nocapture`

**LOCAL: RED — the DOCUMENTED host flake, NOT a phase regression.** Verbatim:

```
fixture green: access log byte-exact mismatch: line 0 not byte-identical:
 envoy     ="{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:35329}","rf":"UF"}"
 envoy-rust="{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.14s
```

**Diagnosis — this is memory `tcpclosebackend-ipv6-unreachable-host-flake`
exactly, and it is maximally informative:**

- **The SUBJECT (envoy-rust) side emits the TARGET LINE EXACTLY:**
  `{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`
  — byte-for-byte the string this phase set out to produce, with keys in UTF-8
  byte order per ADR-0094 §A (confirming PLAN-VERIFY §3.7's key-order correction).
  **The §A/§B implementation is therefore demonstrably correct.**
- **The REFERENCE (real Envoy v1.33.0, in-container) side NEVER REACHED the
  backend at all:** its rcd is
  `{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:35329}`
  with `rf:"UF"` — an IPv6 (`fdc4:…`) **"Network is unreachable"** connect
  failure. It never completed a handshake, so it never observed a reset. This is
  the host's Docker bridge/IPv6 routing defect, not a behavioral divergence.
  Note the reference rcd here carries the OS-derived text of the connect-failure
  family (M45-2) — further proof this is the connect-failure path, not the reset
  path the fixture intends to exercise.

**CI is AUTHORITATIVE.** The fixture is NOT weakened.

### Step 7 — additivity spot-check: fixture `0069` still byte-identical ✅

`cargo test -p differential --test access_log_h2_uc_upstream_reset -- --nocapture`

```
 envoy     ="{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}"
 envoy-rust="{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}"

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.77s
```

RED for the **identical** host reason (reference side reports `UF` — it never
reached the backend). `0069` landed **CI-green at phase 64** on this exact code
path, so this local RED is **pre-existing and NOT caused by phase 65**.

**The load-bearing additivity check PASSES:** `0069`'s SUBJECT-side line is
`{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}` — **byte-identical to its
pre-phase-65 value.** `rf:"UC"` now arrives via the rcd-match instead of the
retired boolean, and the emitted bytes are unchanged. This is exactly the
output-equivalence PLAN-VERIFY §3.1 predicted.

**Task 4: COMPLETE** (local RED on both fixtures = one documented host flake,
diagnosed on the reference side; CI adjudicates at Task 6 Step 4 / state-4).

---

## Task 5 — §E: BEHAVIOR_CONTRACT updates

**Files changed:** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_FLAGS%`
row + the `%RESPONSE_CODE_DETAILS%` row).

Applied as byte-exact literal replacements via a checked script (each pattern
asserted to match **exactly once** before substitution) — the rows are
single, multi-kilobyte table lines, so an unanchored edit would be unsafe.

### Step 1 — INVERT the H2-`UC` clause in the `%RESPONSE_FLAGS%` row ✅

The clause reading *"**On H2, `UC` is witnessed differently** … so H2's `UC` is
derived from a `reset_for_log_h2` boolean … **NOT 1:1 from rcd**, exactly like
H2's own `URX`/`UF` siblings and UNLIKE H1's rcd-derived `UC`"* was **INVERTED,
not appended to** (phase-54 spec-review M2 discipline — appending would leave the
row self-contradictory). It now reads *"**On H2, `UC` is now derived EXACTLY as on
H1** (fixture **0070**, phase 65, ADR-0122) … `UC` derives **1:1 from that rcd** …
The phase-64 boolean discriminator was RETIRED — **CONSUMING carry-forward
M64-1** … both protocols now share the identical derivation split: `{NR, UH, UO,
UC}` rcd-derived, `{URX, UF}` boolean-derived."* The stale `hcm.rs:1091` derive-site
anchor was refreshed to `crates/envoy-http2/src/hcm.rs` (phase-54 spec-review M1
discipline — a file-level anchor, immune to future line drift).

The row's **evidence column** gained the phase-65 witness sentence after the
phase-64/fixture-0069 one, explicitly noting **no `%RESPONSE_FLAGS%` value
changed** (0069's line is byte-identical; `UC` merely arrives via the rcd-match).

A **third** edit was required beyond the PLAN's two: the evidence column's
phase-64 sentence asserted, in the present tense, that H2's `UC` *is* "set via …
a `reset_for_log_h2` boolean … NOT derivable from `%RESPONSE_CODE_DETAILS%`".
Unlike fixture 0069's README (phase *history* — left verbatim, see Task 3), the
BEHAVIOR_CONTRACT is by layout-invariant 5 "the canonical reference" for
**today's** equivalence rules, so a false active-state claim there is a genuine
defect. It was corrected to attribute the boolean to phase 64 explicitly and to
record that **phase 65 has since CONSUMED M64-1** and retired the boolean + its
`finalize_h2_stream` parameter. This also makes PLAN Task 5 Step 3's grep
satisfiable.

### Step 2 — add the H2 reset rcd to the `%RESPONSE_CODE_DETAILS%` row ✅

**Definition column:** after the phase-54 H1 pure-reset clause, added the H2
pure-reset clause (final-outcome `AttemptOutcome::Reset`, guarded
`!retry_limit_exceeded_for_log_h2`, at the post-loop reconciliation region of
`crates/envoy-http2/src/hcm.rs`) → `Some("upstream_reset_before_response_started{connection_termination}")`,
phase 65 / ADR-0122, overriding the in-loop `via_upstream`.

**Evidence column:** the stale trailing sentence *"The remaining H2 failure-path
details … remain deferred as the continuing carry-forward **M56-1**"* was replaced
by the phase-65 witness sentence (fixture `0070`; **M64-1 CONSUMED**; `M56-1` was
already fully closed at phase 64; the connect-failure rcd remains the sole
non-deterministic reset-reason, M45-2). The trailing **default-absent fixture list**
was extended to `0063-0069`, explicitly noting that `0069` drives the same H2 reset
path as `0070` but logs no `rcd`, so it stays byte-identical across the migration.

### Step 3 — no self-contradiction left ✅

```
$ grep -c "reset_for_log_h2" docs/envoy-rust/BEHAVIOR_CONTRACT.md
0
```

```
$ grep -o ".\{80\}NOT 1:1 from rcd.\{40\}" docs/envoy-rust/BEHAVIOR_CONTRACT.md
(no output)
```

All four remaining `M64-1` mentions describe it as **CONSUMED**. The single
surviving "deferred as carry-forward **M64-1**" phrase sits inside the past-tense
phase-64 narrative and is superseded **within the same sentence** by "**phase 65
(ADR-0122) has since CONSUMED M64-1**" — so no mention leaves it standing as open.

**Task 5: COMPLETE.**
