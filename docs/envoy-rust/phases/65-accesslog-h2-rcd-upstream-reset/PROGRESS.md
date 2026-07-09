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

---

## Task 6 — Workspace-green pre-flight

This is a **pre-flight**, not the §7.5 gate. The authoritative gate runs at
**state-4** (`superpowers:verification-before-completion`), where the Docker
differential + h2spec + fuzz surface is CI-authoritative (memory
`envoy-rust-state4-ci-first-execution`).

### Step 1 — the five local gates

| Gate | Result |
|---|---|
| `cargo build --workspace --all-targets` | **clean** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **clean** |
| `cargo fmt --all -- --check` | **clean** (after one reformat — see below) |
| `cargo test --workspace --no-fail-fast` | 5 failing targets, **all documented host flakes** — see below |
| `cargo deny check` | **clean** — `advisories ok, bans ok, licenses ok, sources ok` (exit 0) |

**`cargo fmt` note:** the first `--check` run RED'd on the Task-2 backstop's
`assert_eq!` (rustfmt collapses the 2-line form the PLAN prescribed onto one
line). `cargo fmt --all` applied it; re-check exits 0. This is exactly the
mid-phase fmt trap recorded in memory `envoy-rust-state4-ci-first-execution` —
caught here rather than at the state-4 CI gate.

**`cargo deny` note:** emits a non-fatal `unmatched license allowance` warning for
`"Zlib"` in `deny.toml:50` (pre-existing, unrelated to this phase). Exit code 0.
No freshly-published advisory fired, so no `cargo update -p <dep> --precise` bump
was needed.

### Step 1a — the 5 failing `cargo test --workspace` targets, adjudicated

**Every crate unit-test suite is GREEN** (`envoy-http2`: 88 passed / 0 failed;
`envoy-config` 538; `envoy-filter` 206; `envoy-http1` 157; `envoy-cluster` 160; …).
The documented `envoyrust-h2-handshake-test-host-flake` did NOT fire this run.
All 5 failures are Docker **differential** targets:

| # | Target | Fixture | Adjudication |
|---|---|---|---|
| 1 | `access_log_h2_rcd_upstream_reset` | `0070` (NEW, this phase) | **Documented flake** `tcpclosebackend-ipv6-unreachable-host-flake`. The SUBJECT side emits the exact target line; the REFERENCE Envoy container cannot reach the host-spawned backend (IPv6 `fdc4:…` "Network is unreachable") and reports a connect-failure `UF`. See Task 4 Step 6. |
| 2 | `access_log_h2_uc_upstream_reset` | `0069` | Same flake; **landed CI-green at phase 64** ⇒ pre-existing. Subject line byte-identical to pre-phase (Task 4 Step 7). |
| 3 | `access_log_rcd_upstream_reset` | `0062` (H1) | Same flake, named explicitly in memory `tcpclosebackend-ipv6-unreachable-host-flake` (fixtures 0061/0062/0069). **Untouched by this phase** (H1 code path). |
| 4 | `access_log_rf_upstream_reset` | `0061` (H1) | Same flake, likewise named in that memory. **Untouched by this phase.** |
| 5 | `admin_config_dump_server_info` | `0014` | **Documented flake** `differential-host-bridge-ip-192-168-65-2` — this host routes the backend via `192.168.65.2` rather than the allow-listed `192.168.65.254`/`172.17.0.1`, so envoy's `/clusters` output carries `backend::192.168.65.2:<port>::*` per-endpoint counters the subject lacks. That memory names **this exact test**, from phase-32 state-4. It was independently re-verified anyway — see below. |

**Failure 5 was proven pre-existing, not assumed.** (It is in fact named by the
`differential-host-bridge-ip-192-168-65-2` memory, which records the identical
`0014` / `/clusters` signature from phase-32 state-4 — but it was the one failure
whose adjudication rested on a *host-networking* claim rather than on this phase's
own surface, so it was re-verified from scratch rather than taken on trust.)
It (a) fails in ISOLATION as
well as under parallel load, ruling out
`differential-fixtures-flake-under-parallel-load`; (b) is untouched by every
phase-65 commit (`git log d5b6dd4..HEAD -- <test> <fixture> crates/envoy-admin/`
→ **0 commits**); (c) exercises no H2/reset path (fixture `0014` carries no
`H2_CLOSE_BACKEND` marker); and (d) — decisively — **fails identically at the
PRE-PHASE commit `d5b6dd4`**, reproduced in a detached `git worktree`:

```
$ git worktree add --detach <tmp> d5b6dd4 && cargo build -p http1-echo-server -p envoy-bin
$ cargo test -p differential --test admin_config_dump_server_info
  envoy-only: ["backend::192.168.65.2:36899::canary::false", "backend::192.168.65.2:36899::cx_active::0", …]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.72s
```

Same `192.168.65.2` bridge-IP signature as on the phase-65 tree. **Confirmed
environmental; NOT a phase-65 regression.** (Worktree removed afterwards.)

**No fixture was weakened to make any local run green. CI is AUTHORITATIVE.**

### Step 2 — the retired boolean is gone ✅

```
$ grep -rn "reset_for_log_h2" crates/ docs/envoy-rust/BEHAVIOR_CONTRACT.md
(no output — exit 1)
```

The one surviving repo-wide hit is
`tests/fixtures/0069-accesslog-h2-uc-upstream-reset/README.md:18`, which is
**backward-looking phase-64 historical narrative** that SPEC §F explicitly orders
left verbatim (D-3.4/D-3.5 no-retroactive-rewrite). Its stale *active-state*
carry-forward claim was corrected additively in Task 3. See the §F sweep
discipline note under Task 3 Step 2 for the full rationale.

**Task 6: COMPLETE.**

---

## State-3 implementation — SUMMARY

All 6 PLAN tasks are complete, in the load-bearing order. The `crates/` change is
a **net simplification**: one guarded rcd-set added, one boolean (declaration,
set, call-site arg, parameter, derive branch — 5 sites) and its 3 comment blocks
removed, one rcd-match arm added.

- **§A** rcd-set → Task 1 (fail-first observed, then green).
- **§G** backstops → Task 1 (positive, extended in place) + Task 2 (the REQUIRED
  negative case, **with a mutation check proving it is not vacuous**).
- **§B/§F** derive migration + boolean retirement → Task 3.
- **§C/§D** fixture `0070` + differential test → Task 4 (ZERO harness/`ci.yml` change).
- **§E** BEHAVIOR_CONTRACT (H2-`UC` clause INVERTED, M64-1 → CONSUMED) → Task 5.
- **§H** no new fuzz target → n/a by SPEC.

`#![forbid(unsafe_code)]` holds. NO new crate/dependency/`Op`/`AccessLogRecord`
field/`ConfigError` variant/test-harness code. **NO `%RESPONSE_FLAGS%` value
changed** — the witnessed H2 flag stays `UC`; only its DERIVATION moved from the
boolean to the rcd, and fixture `0069`'s emitted line is byte-identical
(empirically confirmed at Task 4 Step 7).

**ADR-0123 remains reserved-but-UNFIRED** — no §6.2 reconciliation fired: no
§A-§G fact was overturned. The single execution-time deviation (the §F sweep's
comment wording vs. PLAN Task 6 Step 2's over-broad grep) is a PLAN-step
precision issue resolved by SPEC §F + doctrine D-3.4/D-3.5, documented at Task 3.

**Next session = §5 state-4 verification** (`superpowers:verification-before-completion`,
the full §7.5 (a)-(f) gate, CI-authoritative). Per §5.1, one state per session —
the state-4 gate was deliberately NOT run this session.

---

## State-3 push — CI adjudication (AUTHORITATIVE)

The state-3 commits were pushed to `origin/main` (`d5b6dd4..8407ffa`, 7 commits).
**CI run `28985774369` — GREEN on the first attempt, no rerun needed.** Every job
step passed: `fmt`, `clippy`, `build`, `test (includes differential harness →
Docker)`, `cargo deny check`.

**All three local-RED adjudications are VINDICATED by CI:**

```
test access_log_h2_rcd_upstream_reset ... ok      <-- fixture 0070 (NEW, this phase)
test access_log_h2_uc_upstream_reset ... ok       <-- fixture 0069 (additivity)
test admin_config_dump_server_info ... ok         <-- the bridge-IP flake
```

- **Fixture `0070` is GREEN on CI** — the deterministic H2 upstream-reset
  `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}`
  is now **differentially witnessed byte-exact against real upstream Envoy
  v1.33.0**. This is the phase's core claim, and it is proven, not asserted.
  **Carry-forward M64-1 is CONSUMED.**
- **Fixture `0069` is GREEN on CI** — the load-bearing additivity invariant holds
  across the `UC` derive migration, on the authoritative runner.
- **`admin_config_dump_server_info` is GREEN on CI** — confirming the local RED was
  purely the host bridge-IP artifact, exactly as the pre-phase worktree bisect showed.

This CI evidence is a strong leading indicator for the state-4 §7.5 gate, but it
does **not** discharge it: state-4 must still run the gate itself and quote its
own outputs (h2spec ≥95% in particular was not separately extracted here).

---

# State-4 verification — the §7.5 (a)-(f) gate

> Run by the §5 **state-4 verification** session (`superpowers:verification-before-completion`),
> 2026-07-09. `PLAN.md` + `PROGRESS.md` present, `REVIEW.md` absent → the §5
> state-4 detection rule. Per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session)
> the state-5 code-review was deliberately NOT run.
>
> **STEP 0 (disk is authoritative):** `git status --porcelain` clean; branch
> `main`; `HEAD` = `origin/main` = `f66fe9c` (the phase-65 state-3 CI-adjudication
> commit). `git fetch origin --prune` confirmed no sibling autonomous-loop session
> had advanced phase 65 (no `REVIEW.md` in the phase dir; ROADMAP row `65` still
> `in-progress`). Cold-start read of `MISSION.md` / `STATE.md` / `ROADMAP.md` /
> `DECISIONS.md` / `BEHAVIOR_CONTRACT.md` / `SKILL_ROUTING.md` + the phase's
> `SPEC.md`/`PLAN.md`/`PROGRESS.md` completed in full.
>
> **`CI IS AUTHORITATIVE`** for gates (a)/(b)/(c) (memory
> `envoy-rust-state4-ci-first-execution`). The authoritative run is
> **`28986078817`**, whose `headSha` is **`f66fe9c` — the exact tree under gate**
> (working tree clean, so CI ran precisely these bytes). Conclusion: **success**.

## Gate (e) — build / clippy / fmt / test / deny

All five run locally against the tree under gate.

```
$ cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.40s        # exit 0

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.72s
clippy exit=0

$ cargo fmt --all -- --check
fmt exit=0                                                                     # no output

$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
deny exit=0
```

`cargo deny` emits three **non-fatal** `warning[license-not-encountered]`
`unmatched license allowance` lines (`deny.toml:48` `Unicode-DFS-2016`,
`:50` `Zlib`, `:52` `MPL-2.0`) — pre-existing, unrelated to this phase, **exit 0**.
No freshly-published advisory fired, so no `cargo update -p <dep> --precise` bump
was needed (memory `cargo-deny-reds-on-unrelated-advisory` did not apply).

### `cargo test --workspace` — LOCAL, and the flake adjudication

**On CI (authoritative, run `28986078817` @ `f66fe9c`): 145 `test result:` lines,
ZERO of them non-`ok`; zero `... FAILED` anywhere in the log.**

Locally the workspace suite REDs on Docker-differential targets. Because a prior
session's adjudication is not evidence, the suite was run **three times** and the
failing SET was compared:

| Run | Failing targets | `-p envoy-http2 --lib` |
|---|---|---|
| 1 | `0070`, `0069`, `0062`, `0061`, `admin_config_dump_server_info` + `-p envoy-http2 --lib` | **FAILED** |
| 2 | the same 5, **plus** `access_log_h2_urx_retry_exhausted`, `access_log_json_nested`, `access_log_response_code_details`, `http_filter_jwt_authn`, `rbac_url_path`, `upstream_active_health_check`, `upstream_outlier_detection_consecutive_5xx` (12 total) | passed (88/0) |
| 3 | the same 5, **plus** `access_log_rf_no_route` (6 total) | passed (88/0) |

Two facts fall out, and both are load-bearing:

1. **An INVARIANT CORE of exactly 5 targets fails in every run** —
   `access_log_h2_rcd_upstream_reset` (`0070`), `access_log_h2_uc_upstream_reset`
   (`0069`), `access_log_rcd_upstream_reset` (`0062`), `access_log_rf_upstream_reset`
   (`0061`), and `admin_config_dump_server_info` (`0014`). These are precisely the
   five adjudicated at the state-3 pre-flight.
2. **The TAIL varies run-to-run** (12 targets, then 6, with different membership,
   incl. `access_log_rf_no_route` in run 3 but not runs 1-2). A failing set whose
   membership changes across identical invocations of an unchanged tree is
   **non-deterministic by construction** — memory
   `differential-fixtures-flake-under-parallel-load`. Every one of these varying
   targets is `... ok` on CI.

The four close-backend fixtures were re-run **in isolation** (after
`cargo build -p envoy-bin` — memory `differential-harness-uses-debug-envoy-bin`),
and the divergence is **entirely reference-side**:

```
$ cargo test -p differential --test access_log_h2_rcd_upstream_reset -- --nocapture
access log byte-exact mismatch: line 0 not byte-identical:
 envoy     ="{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{remote_connection_failure|immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:43067}","rf":"UF"}"
 envoy-rust="{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.71s
```

The **SUBJECT (envoy-rust) side emits the exact target line**, keys in UTF-8 byte
order per ADR-0094 §A. The **REFERENCE** Envoy container never reached the
host-spawned backend: its rcd is an **IPv6 (`fdc4:…`) "Network is unreachable"**
connect-failure with `rf:"UF"` — it never completed a handshake, so it never
observed a reset. That is memory `tcpclosebackend-ipv6-unreachable-host-flake`
verbatim, and the OS-derived rcd text proves it is the connect-failure family
(M45-2), NOT the reset path the fixture drives. Fixture `0069` fails identically
(`envoy="…rf":"UF"}"` vs `envoy-rust="…rf":"UC"}"`), and its subject-side line is
**byte-identical to its pre-phase-65 value** — the load-bearing additivity proof.
`admin_config_dump_server_info` fails with the `backend::192.168.65.2:38767::*`
signature of memory `differential-host-bridge-ip-192-168-65-2` (proven pre-existing
at `d5b6dd4` by a detached-worktree bisect at state-3). **NO fixture was weakened.**

### The sixth run-1 target — `-p envoy-http2 --lib` — reported HONESTLY

Run 1 additionally failed `-p envoy-http2 --lib`. This is **NOT** in the state-3
adjudicated set, so it was investigated rather than waved through. **Its test name
was not captured** (run 1's output was piped through `tail`, which truncated the
`failures:` block before it was read) — this is stated plainly rather than guessed.
It did **not** reproduce in **54 subsequent executions**:

- 8× `cargo test -p envoy-http2 --lib` in isolation → `88 passed; 0 failed` every time;
- 40× the only network-dependent test in the crate,
  `send_request_maps_h2_handshake_failure_to_typed_error` (the documented
  `envoyrust-h2-handshake-test-host-flake`) → **0 / 40** failures;
- 8× `cargo test -p envoy-http2 --lib` **while a full `cargo test -p differential`
  Docker run saturated the host** (reproducing run 1's exact load conditions) → clean;
- workspace runs 2 and 3 → `88 passed; 0 failed`;
- CI run `28986078817` → the `envoy-http2` suite is `ok`.

**Adjudication:** a load-induced, non-deterministic failure of the `envoy-http2`
unit suite; unreproducible under 54 attempts including the exact concurrent-Docker
condition, and green on the authoritative runner. It is **not** a phase-65
regression — every phase-65-authored test in that crate
(`h2_upstream_reset_access_log_carries_uc_flag`,
`h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx`) passes in all
54 runs and on CI. `superpowers:systematic-debugging` was **not** escalated to,
per the standing rule that escalation requires a rerun to re-fail the SAME test
deterministically — no rerun failed at all.

## Gate (a) — fixture `0070` green

From the authoritative CI log (run `28986078817`, `headSha f66fe9c`):

```
test access_log_h2_rcd_upstream_reset ... ok      <-- fixture 0070 (NEW, this phase)
```

The cross-proxy-equal status `503` + byte-identical whole line
`{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`
is therefore **differentially witnessed byte-exact against real upstream Envoy
v1.33.0**. The driver asserts pure cross-proxy whole-line equality (no static
literal), so a green fixture IS the witness. **Gate (a): MET.**

## Gate (b) — all `0001`-`0069` still green SIMULTANEOUSLY (additivity)

`ls tests/fixtures/ | wc -l` → **70**; `ls tests/differential/tests/*.rs | wc -l` →
**70** (one auto-discovered test binary per fixture). On CI at `f66fe9c`:

```
$ grep -c "test result:"                 -> 145
$ grep "test result:" | grep -vc "ok\."  ->   0
$ grep -cE "\.\.\. FAILED|test result: FAILED" -> 0
```

Zero failures anywhere in the run — so all 70 fixtures (`0001`-`0070`) are green
**in the same run**, which is exactly the simultaneity the invariant demands. The
two load-bearing witnesses:

```
test access_log_h2_uc_upstream_reset ... ok       <-- fixture 0069 (additivity)
test admin_config_dump_server_info ... ok         <-- the bridge-IP flake, green on CI
```

`0069` logs `{method,proto,rc,rf}` and no `rcd`; its `rf:"UC"` now arrives via the
rcd-match rather than the retired boolean, and its emitted line is byte-identical.
**Gate (b): MET.**

### Independent re-verification of the `!retry_limit_exceeded_for_log_h2` guard

The guard is the single most error-prone line in the phase, and fixture `0070`
**cannot** exercise the retry-exhausted-reset path. `PROGRESS.md` Task 2 reports a
mutation check; a report is not evidence, so **the mutation check was re-run from
scratch this session**. Guard clause deleted:

```
test hcm::tests::h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx ... FAILED
  left: "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"URX\"}\n"
 right: "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.15s
```

Guard restored (`git diff --stat crates/envoy-http2/src/hcm.rs` → empty, tree
byte-identical):

```
test hcm::tests::h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 88 filtered out; finished in 0.15s
```

**Red-green confirmed independently.** The negative backstop genuinely pins the
guard; it is not vacuous. The retired boolean is likewise gone:

```
$ grep -rn "reset_for_log_h2" crates/ docs/envoy-rust/BEHAVIOR_CONTRACT.md
(no output — exit 1)
```

## Gate (c) — h2spec ≥95%

The h2spec binary is **absent on this dev host**, so the runner takes its
documented `eprintln!`-skip path (`tests/conformance/h2spec/tests/h2spec_runner.rs`:
*"When `which h2spec` fails locally the test eprintln!-skips per phase 05.2 SPEC §3
D7. CI provisions the binary"*). Gate (c) is therefore **CI-only by construction**.
On CI at `f66fe9c`:

```
test h2spec_pass_rate_gate ... ok
```

That test IS the gate: `const PASS_RATE_GATE: f64 = 0.95;` with
`assert!(pass_rate >= PASS_RATE_GATE, …)`, plus the lockstep known-failure check
(a listed-but-now-passing test also REDs it). Its passing therefore establishes
**≥95% AND** an exact known-failures match. `known-failures.txt` was **NOT trimmed**
(`git diff d5b6dd4..HEAD -- tests/conformance/h2spec/known-failures.txt` → 0 lines),
honoring memory `h2spec-3-5-2-preface-host-sensitive`. NO H2 codec/framing change
landed this phase. **Gate (c): MET.**

## Gate (d) — no new fuzz target

SPEC §H declares no new fuzz target (the phase adds a new VALUE on an existing
operator, not a new operator/grammar).

```
$ git log --oneline d5b6dd4..HEAD -- .github/workflows/ci.yml | wc -l   -> 0
$ git diff d5b6dd4..HEAD -- .github/workflows/ci.yml | wc -l            -> 0
$ git diff --stat d5b6dd4..HEAD -- '*fuzz*' | wc -l                     -> 0
```

The four pre-existing targets are unchanged (`accesslog_format_parse`,
`parse_bootstrap`, `cdn_loop_parse`, `jwt_parse`), and CI's fuzz job —
`fuzz (parse_bootstrap + jwt_parse + cdn_loop_parse + accesslog_format_parse, 30s each)`
— concluded **success**. Memory `new-fuzz-target-needs-a-ci-yml-step` does not
apply (nothing to wire). **Gate (d): MET.**

## Gate (f) — `REVIEW.md` approved

**NOT this session.** Gate (f) is the §5 **state-5** code-review
(`superpowers:requesting-code-review`), per `BOOTSTRAP_PROMPT.md` §5.1's
one-state-per-session rule.

## Finding for the state-5 code-review (doc precision, NON-blocking)

Surfaced while verifying the §E contract edits; **not** a gate-(a)-(e) failure, and
deliberately NOT fixed here (§5.2: fixes re-enter at state-3, and §5.1 forbids
chaining states).

`BEHAVIOR_CONTRACT.md`'s two touched rows now **disagree about when carry-forward
M56-1 closed**:

- the `%RESPONSE_FLAGS%` row (`:1020`) ends the phase-64/65 narrative with
  *"…the boolean discriminator + its `finalize_h2_stream` parameter were RETIRED —
  **CLOSING carry-forward M56-1**"* — attaching the closure to **phase 65**;
- the `%RESPONSE_CODE_DETAILS%` row (`:1031`) states *"`M56-1` was already fully
  closed at **phase 64**"* — as do `STATE.md` and the phase-64 close-out.

Provenance: the `— **CLOSING carry-forward M56-1**` clause is **pre-existing
phase-64 text** (confirmed present at `d5b6dd4`), where it attached to phase 64's
`UC` witness. Phase 65's Task 5 inserted its own sentence *between* that narrative
and the clause, so the em-dash now binds to the phase-65 retirement. It is a
grammatical re-attachment, not a new claim — cosmetic, zero behavioral impact, and
no `%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%` value is affected. But the contract
is the canonical reference for **today's** rules (layout-invariant 5), and phase 65's
own Task 5 discipline was explicitly about not leaving a row self-contradictory —
so this belongs in `REVIEW.md` as a Minor for the state-5 session to fold in.

## State-4 verification — SUMMARY

| Gate | Verdict | Evidence |
|---|---|---|
| (a) fixture `0070` green | **MET** | CI `28986078817` @ `f66fe9c`: `test access_log_h2_rcd_upstream_reset ... ok` |
| (b) `0001`-`0069` green simultaneously | **MET** | same run: 145 `test result:` lines, 0 non-`ok`, 0 `FAILED`; 70/70 fixtures; `0069` byte-identical |
| (c) h2spec ≥95% | **MET** | same run: `test h2spec_pass_rate_gate ... ok` (the test asserts `>= 0.95` + known-failures lockstep); `known-failures.txt` untrimmed |
| (d) no new fuzz target | **MET** | `ci.yml` + `*fuzz*` diffs empty across `d5b6dd4..HEAD`; CI fuzz job success |
| (e) build/clippy/fmt/test/deny clean | **MET** | build/clippy/fmt/deny exit 0 locally; `cargo test --workspace` green on CI (0 failures); local REDs are a 5-target environmental invariant core + a varying flake tail, each adjudicated above |
| (f) `REVIEW.md` approved | **state-5** | next session (`superpowers:requesting-code-review`) |

**§7.5 (a)-(e) are MET; (f) is the next session's gate.** The deterministic H2
upstream-reset `%RESPONSE_CODE_DETAILS%`
`upstream_reset_before_response_started{connection_termination}` is differentially
witnessed byte-exact against real upstream Envoy v1.33.0, and the H2 `UC`
`%RESPONSE_FLAGS%` now derives 1:1 from it with the `reset_for_log_h2` boolean
retired at all 5 sites. **Carry-forward M64-1 is CONSUMED.** `#![forbid(unsafe_code)]`
holds. **NO new ADR fired this session** — no §6.2 reconciliation, no §A-§G fact
overturned; **ADR-0123 remains reserved-but-UNFIRED**. **DECISIONS.md ledger head:
ADR-0122.** ADR-0014 in force; ADR-0028 open; ADR-0049 governs config-validity.

**Next session = §5 state-5 code-review** (`superpowers:requesting-code-review`,
producing `REVIEW.md`). Per §5.1, one state per session — the code-review was
deliberately NOT run here.
