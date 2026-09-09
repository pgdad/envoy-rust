# Phase 114 — PROGRESS

> §5 **state 3** — the implementation of
> `docs/envoy-rust/phases/114-accesslog-grpc-status-filter/PLAN.md`.
> Appended to on each task completion, with REAL quoted command output. Written
> for a stranger with zero prior context (D-3.4).
>
> **The unit:** the `grpc_status_filter` access-log FILTER arm — the SEVENTH of
> upstream Envoy's twelve `envoy.config.accesslog.v3.AccessLogFilter` oneof arms
> — gating a sink's per-record emission on the request's **UNGATED** effective
> gRPC status: the response `grpc-status` header if present, else the phase-110
> `http_to_grpc_status` map over the response code. Witnessed by NEW differential
> fixture `0094-accesslog-grpc-status-filter`.
>
> **Where `PLAN.md` and `SPEC.md` disagree, `PLAN.md` wins** (`ADR-0197`, which
> corrects five landed `SPEC.md` claims).

---

## Session preconditions, re-derived from disk before Task 1

`git rev-parse HEAD` = `c9136ae5f89ada5f1e64e93e2a1a62a19bc5fcc9`, the phase-114
§5 state-2 **CI-record** commit; `git status --porcelain` empty; branch `main`;
`git fetch origin --prune` exit **0** with `origin/main` at the same SHA. The
`## Last commit` block of `STATE.md` was read STRUCTURALLY and already carries a
`CI CONFIRMED` answer for `adc4c7d2aafa8487442a005714e322e8e7217538`, so **no
outstanding CI-record commit was owed** and none was written.

**The §5 state-3 detection rule was re-derived, not inherited:** the phase
directory holds `SPEC.md` (275 lines) and `PLAN.md` (1567 lines) and NO
`PROGRESS.md` and NO `REVIEW.md`.

**Baseline `cargo build --workspace --all-targets` exit 0** before any edit.

### The three stop-condition legs, re-measured from disk — ALL THREE FALSE

- **Leg (i) — FALSE.** `ROADMAP.md`: **122 rows / 121 `done` / 0 `in-progress` /
  1 `planned`**, the `planned` row being `114` at file line **196**. Driven from
  the `^\| [0-9]` prefix with status at field **4** of a `' | '` (spaces) split;
  the status buckets SUM to the row count (121 + 1 = 122 ✓). The forbidden
  `NF == 6` filter was run as a control and reads **120**, dropping exactly the
  two rows with unescaped in-cell pipes (file lines 168 at NF=7 and 169 at
  NF=10). Those are append-only history and were NOT "fixed". This session does
  not touch `ROADMAP.md`, so leg (i) is unchanged at its end.
- **Leg (ii) — FALSE.** **14** crates
  (`envoy-{accesslog,admin,bin,cluster,config,filter,health,http1,http2,jwt,listener,stats,tcp,tls}`).
  `envoy-http3` / `envoy-grpc` / `envoy-wasm` / `envoy-protos` / `envoy-runtime`
  all re-confirmed absent by `test -d`. Over the **28** manifests of
  `git ls-files '*Cargo.toml'` (the OTHER method, `crates/*/Cargo.toml` + root,
  gives 15 — the denominator is method-dependent and this is the set counted):
  `quinn` / `wasmtime` / `tonic` / `opentelemetry` / `prost` = **0** each.
  **Positive control, IDENTICAL invocation shape: `tokio` = 19 of 28.**
- **Leg (iii) — FALSE.** **11** `### ` family headings, of which **ONE** carries
  ZERO rows (`### WASM host family`). Censused from a single `/^### /` rule that
  seeds every heading at 0, because a naive `awk` never emits the zero-row one:
  10 / 5 / 3 / 14 / 3 / 4 / 6 / **31** / 6 / **0** / 13, with **27** rows before
  the first `### ` heading, summing to 122 ✓. The filing defect is intact and was
  NOT repaired: **7** rows whose title begins `Observability family:` sit
  physically under `### Deprecated / edge features`.

  ⚠ **One inherited figure did not reproduce.** The handoff states the LOGICAL
  Observability family is **38** (= the heading slice's 31 + the 7 misfiled).
  Counting instead by the row TITLE field (`$2 ~ /^Observability family/`) gives
  **34** (= 27 + 7). The gap is **4** rows filed under the Observability heading
  whose titles read `Observability / HTTP-filters family:` (ids 34, 35, 36, 37) —
  they belong to the heading slice but not to the title census. Both numbers are
  defensible; they answer different questions. Neither is load-bearing for leg
  (iii), which turns on the heading count and the zero-row heading.

**Overall: the mission is NOT complete and NOT ONE LEG HOLDS.** `ADR-0167`
DECISION 2 governs. `ls stop` returns `No such file or directory` and **no
`stop` file was created**.

---

## A PLAN prediction corrected at Task 1 — the numstat, not the edit

`PLAN.md` Task 1 Step 3 predicts `git diff --numstat` will read
`14	14	crates/envoy-http2/src/hcm.rs` and instructs: *"If the two numbers
differ, text was altered; revert and redo."*

**The measured numstat is `10	10`.** The two numbers do **not** differ, so the
step's own stated invariant — equal insertions and deletions, nothing but
position changed — HOLDS. What is wrong is the predicted *magnitude*: git found
a more compact minimal edit than the 14-line block move, because four of the
moved lines are the bare `///` separators and the closing `}`/blank, which align
against the destination's own text and are therefore not re-emitted.

**A numstat is a rendering of a diff algorithm's choice, not a property of the
edit.** So the move was verified against the property the step actually cares
about, by a stronger check than the one the plan names:

```
before lines: 7762  after lines: 7762
MULTISET IDENTICAL: True
BYTE COUNT before/after: 352248 352248 equal: True
TEXT CHANGED AT ALL: True
```

The line multiset and the byte count are both invariant while the text is not —
which is exactly and only "position changed". `PLAN.md` is landed and is NOT
edited; this is the forward correction.

---

## Task 1 — RIDER: re-attach `finalize_h2_stream`'s doc comment (CF-113-7)

**Status: COMPLETE.** Commit: `phase 114 rider: re-attach finalize_h2_stream's stolen doc comment (CF-113-7)`.

This is a deliberate **rider**, not a deliverable — `SPEC.md` §5 non-goal 6
authorises it because this phase touches this file, and requires it be taken in
its own commit, explicitly labelled. It is taken FIRST because it moves every
line number below 989 in a 7761-line file.

### Step 1 — locate by TEXT, assert the anchors are unique

```
$ grep -cF '/// 06.2 Task 7: factored per-stream finalization — sends the' crates/envoy-http2/src/hcm.rs
1
$ grep -cF 'fn h2_grpc_status() -> Option<String> {' crates/envoy-http2/src/hcm.rs
1
$ grep -cF '#[allow(clippy::too_many_arguments)]' crates/envoy-http2/src/hcm.rs
1
```

All three unique, so the third was usable as an anchor (the plan warns it may
not be). Line numbers re-derived at HEAD, not inherited: the doc block at
**989**, the helper at **1009**, file length **7761**.

**The defect, confirmed on disk before the edit.** `h2_grpc_status()`'s own
10-line doc comment sat *below* `finalize_h2_stream`'s 10-line doc comment with
no `///`-blank separator, so rustdoc concatenated them: `finalize_h2_stream`'s
prose documented a function whose entire body is `None`, and `finalize_h2_stream`
itself carried no doc comment at all. `fmt` and `clippy` cannot see this.

### Step 2 — the move

The 14-line run (file lines 999–1012: the helper's doc comment, the `fn`, and
its trailing blank) was cut and re-inserted immediately above file line 989. The
destination index needed no adjustment because everything removed sits BELOW it —
asserted in the script rather than assumed.

### Step 3 — byte-neutrality (see the correction above)

Resulting order, read back from disk:

```
/// The `%GRPC_STATUS%` backing value for the HTTP/2 access-log record.
///
/// ALWAYS `None` (CF-113-2). H2's gRPC status would have to come from the
/// response TRAILER block, and `finalize_h2_stream` MOVES `trailers` into
/// `send_envoy_response` before it builds the record — so the value is not
/// live at the record build. Phase 113 is HTTP/1.1-only by charter.
///
/// This is a named function rather than an inline `None` so the boundary is
/// pinned by `h2_grpc_status_is_absent` below: a phase that lifts it must
/// change a tested function, not silently edit a struct literal.
fn h2_grpc_status() -> Option<String> {
    None
}

/// 06.2 Task 7: factored per-stream finalization — sends the
/// downstream response via `send_envoy_response`, then (if the HCM
/// config carries access-log sinks) builds an `AccessLogRecord` and
/// emits it once per sink. Mirrors the H1 factored join-point at
/// `envoy_http1::serve_connection`'s tail.
///
/// Per PLAN-write SPEC correction 2 the access-log dispatch lands
/// AFTER `send_envoy_response` returns (covers both the empty-body
/// `send_response(.., end_of_stream=true)` branch and the non-empty
/// `send_data(.., end_of_stream=true)` branch uniformly).
#[allow(clippy::too_many_arguments)]
async fn finalize_h2_stream(
```

Byte-identical to the block `PLAN.md` Step 2 specifies.

### Step 4 — verify nothing broke

```
$ cargo test -p envoy-http2 --lib h2_grpc_status
   Compiling envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.59s
     Running unittests src/lib.rs (target/debug/deps/envoy_http2-3fa56dc263ab8ca6)

running 1 test
test hcm::h2_grpc_status_boundary_tests::h2_grpc_status_is_absent ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.00s
```

`h2_grpc_status_is_absent` PASSES, as predicted — it calls
`super::h2_grpc_status()` and is position-independent.

```
$ cargo clippy -p envoy-http2 --all-targets -- -D warnings
    Checking envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.01s
clippy_exit=0
$ cargo fmt --all -- --check
fmt_exit=0
```

Both silent. The clippy run emitted a real `Checking envoy-http2` line, so it was
not a cached no-op.

### Step 5 — commit

`PLAN.md`'s `git add` list is used verbatim, **plus this `PROGRESS.md`**: the §5
state machine requires appending to `PROGRESS.md` on each task completion, and
the phase-113 state-3 precedent (`abbe107` … `0b0c7ea`) carried it in all ten
task commits. The commit MESSAGE is the plan's, verbatim.

**`CF-113-7` is CONSUMED.**

---

## Task 2 — widen `http_to_grpc_status` to `pub`, narrowly (PV-3)

**Status: COMPLETE.** Commit: `phase 114 task 2: narrow pub on http_to_grpc_status so both codecs share ONE map (PV-3)`.

This **DEPARTS** from phase 113's `ADR-0193` DECISION 6, deliberately and on a
scope argument: that decision refused a visibility widening because
`envoy-accesslog` is a LEAF crate and calling `envoy_http1::grpc` from it would
be a dependency **cycle**. The caller here is `envoy-http2`, which already
depends on `envoy-http1`, so no cycle exists on this edge.

### Steps 1–2 — the failing test, RUN and SEEN to fail

```
$ cargo test -p envoy-http2 --lib the_phase_110_map_is_reachable
   Compiling envoy-http2 v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-http2)
error[E0425]: cannot find function `http_to_grpc_status` in crate `envoy_http1`
    --> crates/envoy-http2/src/hcm.rs:7768:33
     |
7768 |         assert_eq!(envoy_http1::http_to_grpc_status(404), 12);
     |                                 ^^^^^^^^^^^^^^^^^^^ not found in `envoy_http1`

error[E0425]: cannot find function `http_to_grpc_status` in crate `envoy_http1`
    --> crates/envoy-http2/src/hcm.rs:7769:33
     |
7769 |         assert_eq!(envoy_http1::http_to_grpc_status(200), 2);
     |                                 ^^^^^^^^^^^^^^^^^^^ not found in `envoy_http1`

error: could not compile `envoy-http2` (lib test) due to 2 previous errors
```

RED for exactly the reason `PLAN.md` Task 2 Step 2 predicts.

### A SECOND PLAN correction — the `pub use` insertion point

**`PLAN.md` Task 2 Step 3 says to add the re-export line "immediately above
`pub use response::{Http1Response, Response};`". Doing that literally FAILS the
plan's own `cargo fmt --all -- --check` gate.** rustfmt sorts the `pub use`
block alphabetically, and `grpc` sorts between `error` and `hcm`, not above
`response`:

```
$ cargo fmt --all -- --check
Diff in /home/esa/git/envoy-rust/crates/envoy-http1/src/lib.rs:31:
 pub use error::Http1Error;
+pub use grpc::http_to_grpc_status; // 114: the ONE HTTP->gRPC map, shared with envoy-http2.
 pub use hcm::{BuildOutcome, HCM, HCMConfig, HCMStats, build_response};
Diff in /home/esa/git/envoy-rust/crates/envoy-http1/src/lib.rs:37:
-pub use grpc::http_to_grpc_status; // 114: the ONE HTTP->gRPC map, shared with envoy-http2.
 pub use response::{Http1Response, Response};
fmt_exit=1
```

`cargo fmt --all` was run and the line moved to the sorted position. **This is
consistent with the prototype rather than a divergence from it**, and the
numstat proves it: `PLAN.md`'s own measured LoC table lists
`crates/envoy-http1/src/lib.rs` at **1 insertion / 0 deletions**, and the
post-`fmt` diff here reads exactly `1	0`. Had the prototype's line stayed above
`pub use response::`, that file would have shown a deletion too. So the plan's
*prose* mis-describes where its own *measured* line ended up. `PLAN.md` is landed
and is NOT edited; this is the forward correction.

### Step 3 — the widening, item-level only

`pub(crate) fn http_to_grpc_status` → `pub fn`, with the five-line comment
`PLAN.md` specifies, and ONE `pub use` in `lib.rs`. The module declaration and
both mutating/gating items were left exactly as they were — a `pub use`
re-exports an item out of a `pub(crate)` module without widening the module.

### Step 4 — GREEN, and nothing else leaked

```
$ cargo test -p envoy-http2 --lib the_phase_110_map_is_reachable
running 1 test
test hcm::h2_grpc_status_code_tests::the_phase_110_map_is_reachable_from_http2 ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 126 filtered out; finished in 0.00s
```

The three leak checks, which are the load-bearing half of this task:

```
pub(crate) mod grpc;                 = 1
pub(crate) fn is_grpc_request        = 1
pub(crate) fn apply_grpc_local_reply = 1
```

`1`, `1`, `1` as `PLAN.md` predicts. `is_grpc_request` and
`apply_grpc_local_reply` — the two items the module doc's hazard is actually
about — remain unreachable from outside `envoy-http1`.

### Boundary gates

```
$ cargo build --workspace --all-targets   -> build_exit=0
$ cargo clippy --workspace --all-targets --all-features -- -D warnings -> clippy_exit=0
$ cargo fmt --all -- --check              -> fmt_exit=0
```

Per-file numstat at this task's commit:

```
6	1	crates/envoy-http1/src/grpc.rs
1	0	crates/envoy-http1/src/lib.rs
10	0	crates/envoy-http2/src/hcm.rs
```

`grpc.rs` 6/1 and `lib.rs` 1/0 match `PLAN.md`'s measured table cell-for-cell.

---

## Task 3 — the `GrpcStatusFilter` config surface and the MEASURED token grammar

**Status: COMPLETE.** Commit: `phase 114 task 3: the GrpcStatusFilter config arm and its MEASURED token grammar`.

### A THIRD PLAN trap, hit and recovered: "add to `mod tests`" is not "append at EOF"

The first insertion of the four tests went to the last column-0 `}` in
`bootstrap.rs`, on the assumption that it closes `mod tests`. **It does not.**
The run was RED for the wrong reason:

```
error[E0425]: cannot find type `AccessLogFilter` in this scope
```

`AccessLogFilter` is in scope under `mod tests`' `use super::*`, so an
unresolved `AccessLogFilter` is a *location* failure, not a missing-item
failure. Censused structurally:

```
6102:#[cfg(test)]    6103:mod tests {              <- closes at 20338
20344/20345: mod serialize_roundtrip_tests {
20898/20899: mod typed_per_filter_config_tests {
21184/21185: mod per_route_absent_filter_tests {
21381/21382: mod csrf_validator_tests {
21617/21618: mod cdn_loop_config_tests {
21769/21770: mod set_metadata_config_tests {
21884/21885: mod header_to_metadata_config_tests {
21974/21975: mod header_to_metadata_validator_tests {
22103/22104: mod json_format_value_tests {         <- where the tests actually landed
```

`bootstrap.rs` carries **TEN** column-0 `#[cfg(test)] mod` blocks. `mod tests`
spans lines **6103–20338**; the file is 22169 lines. The insert was reverted with
`git checkout --` (the file was otherwise untouched at that point, so the revert
was total, and `git status --porcelain` was re-checked empty) and redone at the
FIRST column-0 `}` after `mod tests {`, with the neighbours asserted before the
splice.

### Steps 1–2 — the failing tests, RUN and SEEN to fail

```
$ cargo test -p envoy-config --lib grpc_status_filter
error[E0433]: cannot find type `GrpcStatusToken` in this scope
error[E0425]: cannot find function `resolve_grpc_status_token` in this scope
error[E0609]: no field `grpc_status_filter` on type `bootstrap::AccessLogFilter`
error: could not compile `envoy-config` (lib test) due to 18 previous errors
```

RED for exactly the two reasons `PLAN.md` Task 3 Step 2 predicts.

### Step 3 — the implementation

The seventh `Option` arm on `AccessLogFilter`, then `GrpcStatusFilter`,
`GrpcStatusToken` (untagged, `Num` FIRST — arm order is load-bearing),
`GRPC_STATUS_FILTER_NAMES` (17 entries, index IS the code) and
`resolve_grpc_status_token`, all verbatim from the plan. Three re-exports added
to `envoy-config`'s `lib.rs`.

### Step 4 — the `E0063` blast radius, driven from the compiler's error list

**Never from a text match.** The compiler reported **FIFTEEN** exhaustive
literals across two crates:

```
crates/envoy-config/src/bootstrap.rs   4   (14616, 14654, 14714, 14906)
crates/envoy-http1/src/hcm.rs         11   (4837, 4991, 5074, 5114, 5130,
                                            5151, 5158, 5298, 5463, 5483, 10754)
```

The `envoy-http1` set is why `PLAN.md`'s Task 3 `git add` list names that file —
adding a `pub` field to `AccessLogFilter` is a cross-crate change. Each literal
was located by brace-matching from the compiler-reported column to its own
closing brace, and `grpc_status_filter: None,` inserted at that literal's indent.

A guard was then run over both files asserting that **no** inserted line sits
inside a literal carrying `..AccessLogFilter::default()` / `..Default::default()`
— those absorb the field silently and must NOT be edited:

```
suspect functional-update literals touched: 0
grpc_status_filter: None,  ->  crates/envoy-http1/src/hcm.rs:11
                               crates/envoy-config/src/bootstrap.rs:4
```

By construction this could not have gone wrong: `rustc` only reports the
exhaustive ones. The guard exists because the plan names four textual
false-positive classes that a `grep`-driven sweep would have hit.

### A FOURTH finding — `E0027`, which the plan does not name

Beyond the fifteen `E0063`s the compiler also raised, at the Task-3 boundary:

```
error[E0027]: pattern does not mention field `grpc_status_filter`
    --> crates/envoy-config/src/bootstrap.rs:5816:9
5816 |       let AccessLogFilter {
```

`validate_access_log_filter`'s destructure has **no `..`**, so the seventh
binding is compiler-forced at the moment the field appears — at Task 3 — while
the plan schedules the destructure growth in Task **4**. The binding was
therefore added here, as the minimal edit that makes the crate compile.

**Its consumer is Task 4's token loop, so at THIS boundary it is unused:**

```
$ cargo clippy --workspace --all-targets --all-features -- -D warnings
error: unused variable: `grpc_status_filter`
    --> crates/envoy-config/src/bootstrap.rs:5823:9
5823 |         grpc_status_filter,
     |         ^^^^^^^^^^^^^^^^^^ help: try ignoring the field: `grpc_status_filter: _`
     = note: `-D unused-variables` implied by `-D warnings`
clippy_exit=101
```

**This needs no new ADR — `PLAN.md`'s own Global Constraints already govern it**,
citing `ADR-0194` DECISION 2: *"a whole-slice prototype validates the SLICE,
never a TASK BOUNDARY — these are the plan's gates for the executor to run, not a
measured claim about each boundary."* This is the same failure class phase 113
measured at its Task-1 boundary, recurring for the same structural reason.

**The deferral, and its limits.** Only the `unused_variables` arm of `-D warnings`
is deferred, from Task 3 to Task 4 — the first boundary at which a consumer
exists. `cargo build --workspace --all-targets`, `cargo fmt --all -- --check` and
the task's own tests are required green HERE and were:

```
build_exit=0        (sole warning: the unused binding above)
fmt_exit=0
```

**Nothing is suppressed.** No `#[allow(unused_variables)]` and no
`grpc_status_filter: _` was added anywhere, precisely so that a forgotten
attribute cannot outlive the gap. Task 4 discharges `-D warnings` in full.

### Step 5 — GREEN

```
$ cargo test -p envoy-config --lib grpc_status_filter
running 4 tests
test bootstrap::tests::grpc_status_filter_rejects_every_measured_reject ... ok
test bootstrap::tests::grpc_status_filter_accepts_every_measured_token ... ok
test bootstrap::tests::grpc_status_filter_exclude_defaults_false ... ok
test bootstrap::tests::grpc_status_filter_mixed_token_list_deserializes ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 716 filtered out; finished in 0.00s
```

4 passed, as predicted. The arithmetic corroborates `PLAN.md`'s end-of-phase
target independently: `4 + 716 = 720`, and the plan's Task 4 Step 5 states the
baseline is **716** and the crate finishes at **722** after the six tests Tasks 3
and 4 add. Four are here; Task 4 adds two.

Per-file numstat at this task's commit:

```
202	0	crates/envoy-config/src/bootstrap.rs
20	19	crates/envoy-config/src/lib.rs
11	0	crates/envoy-http1/src/hcm.rs
```

`lib.rs`'s 19 deletions are rustfmt re-flowing the `pub use bootstrap::{…}`
block around the three new names, not removed exports.

---

## Task 4 — validation: the seventh arm and a fail-loud bad-token error

**Status: COMPLETE.** Commit: `phase 114 task 4: validate the seventh AccessLogFilter arm fail-loud on a bad status token`.

### A FIFTH trap, hit and recovered: an anchor that is NOT unique

The first scripted edit aborted on its own uniqueness assertion:

```
AssertionError: ('        assert!(matches!(\n            err,\n            crate::ConfigEr', 2)
```

That `assert!(matches!(…AmbiguousAccessLogFilter…))` run occurs **twice** in
`bootstrap.rs`. The script asserts `count == 1` **before** any write and writes
only at the end, so nothing was modified — `git status --porcelain` was re-checked
empty afterwards. The anchor was extended to include the preceding all-arms
literal tail and the `validate_access_logs(…)` call, which is unique, and the
edit was redone. **Assert the anchor occurs exactly once, and do the write last.**

### Steps 1–2 — the failing tests, RUN and SEEN to fail

`six_arm_cardinality_counts_every_arm` → `seven_arm_cardinality_counts_every_arm`,
a seventh entry in its `single_arms` vector, `assert_eq!(single_arms.len(), 7)`,
and the all-arms literal's `grpc_status_filter` flipped `None` → `Some(…)` (the
binding renamed `all_six` → `all_seven` to match). Plus the two new tests.

```
$ cargo test -p envoy-config --lib grpc_status_filter_bad_token
error[E0599]: no variant named `UnknownGrpcStatus` found for enum `ConfigError`
error: could not compile `envoy-config` (lib test) due to 1 previous error; 1 warning emitted
```

RED for exactly the reason `PLAN.md` Task 4 Step 2 predicts. (The trailing
"1 warning emitted" is the Task-3 unused destructure binding, still outstanding
at this point and discharged below.)

### Steps 3–4 — the error variant, the validator, and three stale doc counts

`ConfigError::UnknownGrpcStatus { token: String }` added immediately after
`UnknownResponseFlag`, its exact analogue. In `validate_access_log_filter`:
`grpc_status_filter.is_some(),` appended to the `set_arms` array, and the token
loop spliced before the closing `Ok(())`. The seventh destructure binding was
already present — Task 3 was forced to add it by `E0027`.

**The `set_arms` growth is the load-bearing half of this task**, and it is the
one the compiler does NOT force: the array is not length-checked, so an arm
present in the struct but missing from the array counts as ZERO and turns a
valid single-arm filter into `AmbiguousAccessLogFilter{"no filter variant is
set"}`. `seven_arm_cardinality_counts_every_arm` is what catches that, and it
passes.

Splice location verified structurally rather than by line number: the loop sits
at line 5899 and `validate_access_log_filter` closes at 5915.

**Beyond what the plan names, THREE doc statements became factually false** the
moment the seventh arm landed, and all three were corrected:

| site | was | now |
|---|---|---|
| the `AccessLogFilter` struct doc | *"This type models SIX oneof arms"* | SEVEN, naming `grpc_status_filter` (phase 114) |
| `validate_access_logs`' contract doc item 3 | *"Phases 70/71/72/73/74 give SIX arms"* | *"Phases 70/71/72/73/74/114 give SEVEN arms"* |
| `validate_access_log_filter`'s own doc | *"cardinality, all SIX arms"* | *"all SEVEN arms"* |

`PLAN.md` Step 4 names only the second. The other two sit in the same two files
this task edits and would otherwise have been left asserting a count the code
contradicts.

### Step 5 — GREEN, and the Task-3 deferral discharged

```
$ cargo test -p envoy-config --lib grpc_status
running 6 tests
test bootstrap::tests::grpc_status_filter_accepts_every_measured_token ... ok
test bootstrap::tests::grpc_status_filter_rejects_every_measured_reject ... ok
test bootstrap::tests::grpc_status_filter_exclude_defaults_false ... ok
test bootstrap::tests::grpc_status_filter_mixed_token_list_deserializes ... ok
test bootstrap::tests::grpc_status_filter_bad_token_is_fail_loud ... ok
test bootstrap::tests::grpc_status_filter_empty_statuses_loads ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 716 filtered out; finished in 0.00s

$ cargo test -p envoy-config --lib seven_arm_cardinality
running 1 test
test bootstrap::tests::seven_arm_cardinality_counts_every_arm ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 721 filtered out; finished in 0.00s

$ cargo test -p envoy-config --lib
test result: ok. 722 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

**`722 passed; 0 failed` is exactly the figure `PLAN.md` §6.1 states the
prototype measured for this crate at the end of the phase.** The crate is now
complete for this phase (Tasks 5–10 touch other crates), so the two numbers are
measuring the same thing and they agree.

Boundary gates — **all three green, including `-D warnings`**:

```
build_exit=0
clippy_exit=0     <- the Task-3 unused-binding deferral is DISCHARGED IN FULL
fmt_exit=0
```

No `#[allow]` was added at Task 3 and none was removed here; the warning went
away because its consumer landed, which is the only correct way for it to go
away.

Per-file numstat at this task's commit:

```
59	10	crates/envoy-config/src/bootstrap.rs
6	0	crates/envoy-config/src/lib.rs
```

Cumulative for `envoy-config` across Tasks 3+4: `bootstrap.rs` 261/10 and
`lib.rs` 26/19, against `PLAN.md`'s measured 249/4 and 26/19. `lib.rs` matches
exactly. `bootstrap.rs` runs +12 insertions and +6 deletions over the prototype —
the three stale doc-count corrections above, which the prototype did not make.

---

## Task 5 — the UNGATED derivation: `effective_grpc_status` and both record builds

**Status: COMPLETE.** Commit: `phase 114 task 5: the UNGATED effective-gRPC-status derivation on both codecs (PV-4)`.

This is the phase's real derivation work. The field is `u8`, **not** `Option<u8>`:
a status is defined on EVERY request, including plain HTTP, which is the whole
finding of the phase.

### PV-4 re-verified on disk before writing any code

`PLAN.md` asserts the response-header map IS live at the H2 record build, against
`SPEC.md` §5 non-goal 3 which left it open. Confirmed structurally in
`crates/envoy-http2/src/hcm.rs`:

```
1105:    let response_status_for_log: u16 = resp.status;
1108:    let response_headers_for_log: &[(String, String)] = &response_headers_for_log_owned;
...
1182:            upstream_service_time: extract_upstream_service_time(response_headers_for_log),
```

The borrow taken at 1108 is still being read at 1182, **inside the record
literal**. So the borrow is alive at the record build by construction, and
`h2_grpc_status()`'s "not live at the record build" doc is true of **trailers**
only. H2 therefore gets BOTH legs, through the same helper H1 uses.

Also confirmed before writing: `pub mod hcm;` at `crates/envoy-http1/src/lib.rs:22`,
so `envoy_http1::hcm::effective_grpc_status` is a reachable path;
`access_log_header_value` at `hcm.rs:1972`; `headers::GRPC_STATUS` at
`crates/envoy-http1/src/headers.rs:18`.

### Steps 1–2 — the failing tests, RUN and SEEN to fail

Four tests appended to `envoy-http1`'s `hcm.rs`, and the Task-2 placeholder in
`envoy-http2`'s `hcm.rs` REPLACED by the real H2 pin.

```
$ cargo test -p envoy-http1 -p envoy-http2 --lib grpc_status
error[E0425]: cannot find function `effective_grpc_status` in this scope
error[E0425]: cannot find function `effective_grpc_status` in module `envoy_http1::hcm`
error: could not compile `envoy-http1` (lib test) due to 5 previous errors; 1 warning emitted
error: could not compile `envoy-http2` (lib test) due to 2 previous errors
```

RED on BOTH codecs, for exactly the reason `PLAN.md` Task 5 Step 2 predicts.

### Steps 3–5 — the field, the shared helper, both record builds

`AccessLogRecord.grpc_status_code: u8` added immediately after
`grpc_status: Option<String>`, with the doc spelling out that the two fields are
NOT the same value; `grpc_status_code: 2,` added to `test_baseline`.
`effective_grpc_status` added immediately above `build_access_log_record`. Both
production record builds populated, verbatim from the plan.

### Step 6 — a SIXTH finding: the plan's E0063 COUNT is four, its ENUMERATION is five

`PLAN.md`'s Global Constraints say *"There are exactly FOUR such literals"*, and
Task 5 Step 6 says *"The four are: the H1 production build, the H2 production
build, `record.rs`'s `test_baseline`, and one test literal each in
`file_sink.rs` and `envoy-http1/src/hcm.rs`"* — which **enumerates FIVE**.

**The enumeration is right and the count is wrong.** Measured, the exhaustive
`AccessLogRecord` literals are:

```
crates/envoy-accesslog/src/record.rs:170      grpc_status_code: 2,          (test_baseline)
crates/envoy-accesslog/src/file_sink.rs:190   grpc_status_code: 2,          (test literal)
crates/envoy-http1/src/hcm.rs:1791            effective_grpc_status(...)    (H1 production)
crates/envoy-http1/src/hcm.rs:2651            grpc_status_code: 2,          (test literal)
crates/envoy-http2/src/hcm.rs:1202            effective_grpc_status(...)    (H2 production)
```

**FIVE.** The sweep was driven to a FIXPOINT from the compiler's own error list,
never from a text match, and it took **two rounds** — not because a site was
missed, but because `cargo` stops at the first failing crate, so
`envoy-http1/src/hcm.rs:2630` was invisible until `envoy-accesslog` compiled:

```
round 0: crates/envoy-accesslog/src/file_sink.rs -> 1 site(s) [169]
round 0: crates/envoy-http1/src/hcm.rs           -> 1 site(s) [2630]
round 1: no E0063 sites left
build_exit=0
```

A single-pass sweep that trusted round 0 alone would have been complete only by
luck. **Loop the compiler-driven sweep to a fixpoint.**

### Step 7 — GREEN, and the per-crate arithmetic converges on the prototype

```
$ cargo test -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib
test hcm::grpc_status_filter_tests::derivation_prefers_the_response_header ... ok
test hcm::grpc_status_filter_tests::derivation_falls_back_to_the_phase_110_map ... ok
test hcm::grpc_status_filter_tests::derivation_is_ungated_and_differs_from_the_phase_113_field ... ok
test hcm::grpc_status_filter_tests::derivation_ignores_an_unparseable_or_out_of_range_header ... ok
test hcm::h2_grpc_status_code_tests::h2_uses_the_shared_effective_status ... ok
test hcm::h2_grpc_status_boundary_tests::h2_grpc_status_is_absent ... ok

envoy-accesslog  test result: ok. 129 passed; 0 failed; 0 ignored
envoy-http1      test result: ok. 242 passed; 0 failed; 0 ignored
envoy-http2      test result: ok. 126 passed; 0 failed; 1 ignored
```

**These reconcile with `PLAN.md`'s prototype end-of-phase figures of 133 / 243 /
126+1 ignored, exactly:**

| crate | now | still to come | predicted end |
|---|---:|---|---:|
| `envoy-accesslog` | 129 | Task 6's neutrality pin (1) + Task 7's arm tests (3) | **133** ✓ |
| `envoy-http1` | 242 | Task 8's compile test (1) | **243** ✓ |
| `envoy-http2` | 126 (+1 ignored) | nothing | **126 (+1)** ✓ |

`envoy-config` already closed at **722** ✓ at Task 4. All four per-crate targets
are now either met or account for exactly.

Boundary gates:

```
build_exit=0    clippy_exit=0    fmt_exit=0
```

Per-file numstat at this task's commit:

```
1	0	crates/envoy-accesslog/src/file_sink.rs
11	0	crates/envoy-accesslog/src/record.rs
91	0	crates/envoy-http1/src/hcm.rs
24	3	crates/envoy-http2/src/hcm.rs
```

`record.rs` 11/0 against `PLAN.md`'s measured 12/0 — one line, and the plan's
figure is the whole-phase total for a file no later task touches. Re-checked: the
plan's block is 10 doc/field lines plus a blank; the blank was absorbed by the
existing separator here.

---

## Task 6 — the fifth `should_log` widening (behaviour-neutral)

**Status: COMPLETE.** Commit: `phase 114 task 6: widen should_log with the effective gRPC status (behavior-neutral)`.

### `ADR-0197` SPEC correction 1 re-verified on disk

`SPEC.md` says the production `should_log` call sites are TWO and the other 125
are mechanical test edits. `ADR-0197` says FIVE and 122. **Measured here, the
plan is right:**

```
crates/envoy-accesslog/src/filter.rs      68
crates/envoy-accesslog/src/file_sink.rs    5
crates/envoy-http1/src/hcm.rs             53
crates/envoy-http2/src/hcm.rs              1
                                    total 127
```

and a workspace-wide sweep of `crates/` + `tests/` finds `.should_log(` in **no
other file**, so 127 is the whole population. The five production sites, located
by TEXT:

```
crates/envoy-accesslog/src/file_sink.rs:114   the FileSink -> LogFilter delegation
crates/envoy-accesslog/src/filter.rs:148      the And recursion
crates/envoy-accesslog/src/filter.rs:151      the Or  recursion
crates/envoy-http1/src/hcm.rs:1575            the H1 dispatch
crates/envoy-http2/src/hcm.rs:1217            the H2 dispatch
```

The three the SPEC missed are none of them a mechanical test edit.

### Steps 1–3 — the definitions, the production sites, the 122-site sweep

Both definitions and both docs widened (now `Phase 70/71/72/73/74/114`), the two
recursion arms and the `FileSink` delegation threaded, and both production
dispatch sites given one more argument line.

**The sweep was a paren-matching pass, not a `sed`** — the plan warns the last
argument has eight spellings, three with nested parentheses, and that ten sites
are already multiline. Each `.should_log(` was brace-matched to its own closing
paren; a site whose inner text already contained `grpc_status_code` was skipped,
which is what makes the pass idempotent and self-checking:

```
crates/envoy-accesslog/src/filter.rs          widened  66
crates/envoy-accesslog/src/file_sink.rs       widened   4
crates/envoy-http1/src/hcm.rs                 widened  52
crates/envoy-http2/src/hcm.rs                 widened   0
seen=127 widened=122 already-widened(production)=5
```

**127 = 122 + 5 exactly**, and the per-file split 66 / 4 / 52 / 0 matches
`PLAN.md`'s own Task-6 file list cell-for-cell ("2 production recursion sites +
66 test sites", "1 production delegation + 4 test sites", "1 production site + 52
test sites", "1 production site"). `cargo fmt --all` was run as part of THIS
task, per the plan and the phase-72 precedent.

### A SEVENTH finding: Step 5's expected counts predate Step 4

`PLAN.md` Task 6 Step 5 says *"Expected counts: `68`, `5`, `53`, `1` — unchanged
from before the sweep, because the sweep adds arguments and not call sites."*
The reasoning is right and the number is stale: **Step 4 of the same task adds a
neutrality pin containing FOUR `.should_log(` calls.** Measured after Step 4:

```
crates/envoy-accesslog/src/filter.rs      72     (68 + the pin's 4)
crates/envoy-accesslog/src/file_sink.rs    5     ✓
crates/envoy-http1/src/hcm.rs             53     ✓
crates/envoy-http2/src/hcm.rs              1     ✓
```

Three of the four match; `filter.rs` is 68 + 4 = 72 and the sweep is still
argument-only. The expected-count line was written against the pre-Step-4 file.

### The `only_used_in_recursion` gate — the plan's contingency, taken as written

`PLAN.md` Task 6 says: *"Do NOT add `#[allow(clippy::only_used_in_recursion)]` …
If the executor gates Task 6 in isolation and clippy fires that lint, add the
allow with a comment saying it is TRANSIENT and REMOVE it in Task 7."*

**It fired.** Gating this task in isolation is exactly the case the plan's
contingency covers:

```
error: parameter is only used in recursion
   --> crates/envoy-accesslog/src/filter.rs:117:9
117 |         grpc_status_code: u8,
    |         ^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore
note: parameter used here
   --> crates/envoy-accesslog/src/filter.rs:155:21  (the And arm)
   ...  164  (the Or arm)
```

The cause is structural and temporary: Task 6 threads the parameter through the
And/Or recursion while the arm that CONSUMES it is Task 7's. So the allow was
added on the plan's own instruction, carrying a six-line note naming itself
TRANSIENT, the task that must delete it, and the check that will prove it gone.

⚠ **This is the one suppression in the phase, and Task 7 removes it.** The
Task-3 `-D warnings` deferral was handled the other way — by deferring the gate
rather than suppressing the lint — because no plan instruction covered it there.
Here the plan makes the call explicitly, so it is followed, with the removal
made checkable rather than remembered.

### Step 5 — verification

```
build_exit=0    clippy_exit=0    fmt_exit=0

$ cargo test -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib
test filter::tests::existing_arms_ignore_the_grpc_status_argument ... ok
test filter::tests::existing_arms_ignore_the_dynamic_metadata_argument ... ok

envoy-accesslog  test result: ok. 130 passed; 0 failed; 0 ignored
envoy-http1      test result: ok. 242 passed; 0 failed; 0 ignored
envoy-http2      test result: ok. 126 passed; 0 failed; 1 ignored
```

The new pin passes over all four probe codes × four arms, and the phase-74 pin it
is modelled on still passes beside it — the widening is behaviour-neutral in both
directions. `envoy-accesslog` moves 129 → **130**; Task 7's three arm tests take
it to the plan's **133**.

Per-file numstat at this task's commit:

```
16	9	crates/envoy-accesslog/src/file_sink.rs
121	79	crates/envoy-accesslog/src/filter.rs
61	52	crates/envoy-http1/src/hcm.rs
1	0	crates/envoy-http2/src/hcm.rs
```

---

## Task 7 — `LogFilter::GrpcStatus` and its `should_log` arm

**Status: COMPLETE.** Commit: `phase 114 task 7: LogFilter::GrpcStatus — plain-integer membership with exclude inversion`.

The arm carries **plain data, not a trait object**. `envoy-accesslog` depends
only on `tokio`, `bytes`, `tracing` and `thiserror`, so the `Header` and
`Metadata` arms inject an `Arc<dyn …>` through the `ADR-0150` seam. This arm
needs no `envoy-config` type at all — the compile step resolves every token to an
integer before the runtime sees it — so `ADR-0150` is not involved and this arm
is SIMPLER than phase 72's or phase 74's.

### Steps 1–2 — the failing tests, RUN and SEEN to fail

```
$ cargo test -p envoy-accesslog --lib grpc_status_arm
error[E0599]: no variant named `GrpcStatus` found for enum `filter::LogFilter`
error: could not compile `envoy-accesslog` (lib test) due to 4 previous errors
```

RED for exactly the reason `PLAN.md` Task 7 Step 2 predicts.

### Step 3 — the variant, the arm, and the removal of Task 6's suppression

`LogFilter::GrpcStatus { codes: Vec<u8>, exclude: bool }` appended after
`Metadata`, and the predicate added as one expression:

```rust
LogFilter::GrpcStatus { codes, exclude } => {
    codes.contains(&grpc_status_code) != *exclude
}
```

One expression covers both directions. An empty `codes` makes `contains` false
for every record, so `exclude: false` keeps nothing and `exclude: true` keeps
everything — exactly what was MEASURED.

**The TRANSIENT `#[allow(clippy::only_used_in_recursion)]` Task 6 added was
deleted in this same task**, as `PLAN.md` Task 6 requires. It was not left to be
noticed later; its removal is asserted:

```
only_used_in_recursion occurrences in crates/envoy-accesslog/src/ : 0 file(s)
`TRANSIENT, PHASE-114` occurrences anywhere in crates/           : 0
clippy_exit=0
```

`-D warnings` now passes **without** the allow, which is the proof that the lint
went away because its consumer landed rather than because it was silenced. The
phase ends carrying **zero** suppressions.

### Step 4 — GREEN

```
$ cargo test -p envoy-accesslog --lib
test filter::tests::grpc_status_arm_is_membership_over_the_effective_code ... ok
test filter::tests::grpc_status_arm_exclude_inverts_over_the_same_code ... ok
test filter::tests::grpc_status_arm_empty_statuses_keeps_nothing ... ok

test result: ok. 133 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

**`133 passed; 0 failed` is exactly the figure `PLAN.md` Task 7 Step 4 states the
prototype measured for this crate at the end of the phase.** No later task
touches `envoy-accesslog`, so the crate is closed at its predicted number.

`grpc_status_arm_exclude_inverts_over_the_same_code` is the in-process witness
`CF-114-4` is banked against: it asserts the inversion over **all 17 codes**,
which is more than the single bit a second differential fixture would have
bought.

Boundary gates:

```
build_exit=0    clippy_exit=0    fmt_exit=0
```

---

## Task 8 — `compile_access_log_filter` grows to seven arms

**Status: COMPLETE.** Commit: `phase 114 task 8: compile_access_log_filter grows to seven arms`.

### An EIGHTH finding: a filtered-out test run is a FALSE GREEN

The first attempt at Step 1 aborted on a mis-written guard, so the test was never
inserted — and the Step-2 run then reported, with **exit code 0**:

```
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 242 filtered out; finished in 0.00s
```

`cargo test -p <pkg> <name>` exits **0** when the name filter matches nothing.
Had this step been judged by exit code, a test that does not exist would have
read as a passing test. Every RED/GREEN claim in this document was therefore
taken from the `running N tests` line and the named result row, never from `$?`.
The guard was corrected and the insert redone.

### Steps 1–2 — the failing test, RUN and SEEN to fail

```
$ cargo test -p envoy-http1 --lib compile_produces_the_seventh_arm
running 1 test
test hcm::grpc_status_filter_tests::compile_produces_the_seventh_arm_with_resolved_codes ... FAILED

thread '...' panicked at crates/envoy-http1/src/hcm.rs:1902:14:
internal error: entered unreachable code: validated by validate_access_logs: exactly one filter arm is set

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 242 filtered out; finished in 0.00s
```

`running 1 test` proves it ran, and it is RED as a **panic** rather than a
compile error — exactly the failure mode `PLAN.md` Task 8 Step 2 predicts, and
for the right reason: the six-tuple does not yet include `grpc_status_filter`, so
the `_ =>` fallback catches it.

The test's three tokens deliberately cover all three input shapes the grammar
accepts — a canonical NAME (`UNIMPLEMENTED` → 12), a bare INTEGER (`13`), and a
string-spelled integer (`"14"`) — so a compile step that resolved only one shape
would go RED here.

### Step 3 — the match

The scrutinee tuple grew to seven, a seventh `None` was added to each of the six
existing patterns, and the new arm was inserted immediately before the `_ =>`
fallback. The compile step may `expect()` because the validator (Task 4) has
already proved every token resolves — the same posture the `header_filter` and
`metadata_filter` arms take with their pre-compiled `SafeRegex`.

The doc comment's *"SIX arms ship"* was updated to SEVEN and now names
`grpc_status_filter` (phase 114).

### Step 4 — GREEN, and all four per-crate targets met

```
$ cargo test -p envoy-http1 --lib
test hcm::grpc_status_filter_tests::compile_produces_the_seventh_arm_with_resolved_codes ... ok
test result: ok. 243 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s

build_exit=0    clippy_exit=0    fmt_exit=0
```

**`243 passed; 0 failed` is exactly `PLAN.md` Task 8 Step 4's stated prototype
figure**, and it is the last of the four. Every affected crate has now landed on
the number the prototype measured, independently reached:

| crate | measured here | `PLAN.md` prototype |
|---|---:|---:|
| `envoy-accesslog` | 133 | 133 ✓ |
| `envoy-config` | 722 | 722 ✓ |
| `envoy-http1` | 243 | 243 ✓ |
| `envoy-http2` | 126 (+1 ignored) | 126 (+1 ignored) ✓ |
| **sum** | **1224** | **1224** ✓ |

Four independent numbers agreeing is meaningful corroboration in a way one
aggregate would not be: a compensating error in either direction would have to
cancel across separate crates.

Per-file numstat at this task's commit:

```
51	12	crates/envoy-http1/src/hcm.rs
```

⚠ **Corrected during Task 9.** This line first read `41	7`, a figure TYPED rather
than read off the `git diff --numstat` that had just printed `51	12` immediately
above it in the same shell. Nothing about the code changed; the record was wrong
and is now what was measured. A transcribed number is a claim like any other.

---

## Task 9 — differential fixture `0094-accesslog-grpc-status-filter`

**Status: COMPLETE.** Commit: `phase 114 task 9: differential fixture 0094 — the grpc_status_filter arm, 8 probes`.

### Numbering and environment, re-derived

`tests/fixtures/` holds **93** directories; `git ls-files` agrees at **93**;
`tests/differential/tests/` holds **92** runners. So `0094` is free and this
phase takes the counts to **94 / 93**. Docker healthy; the pinned image is
present locally with the digest `ENVOY_TARGET.md` names,
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`.
Seventeen foreign containers from the parallel workstream were running
throughout; the harness picks ephemeral ports, and none collided.

### Steps 1–3 — the two YAMLs and `expectations.yaml`

`envoy.yaml` was GENERATED from `envoy-rust.yaml` by applying exactly the four
harness hunks, each located by a uniqueness-asserted predicate rather than a line
number. The resulting diff is exactly four hunks, at exactly the offsets
`PLAN.md` Step 2 predicts:

```
$ diff envoy.yaml envoy-rust.yaml
2d1    < admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
6c5    <       address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
       >       address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
14d12  <                 generate_request_id: false
22c20  <                       path: /tmp/0094-envoy-mount/access.log
       >                       path: /tmp/0094-envoy-rust-mount/access.log
```

**The `filter:` block is byte-identical on both sides**, asserted by md5 over the
block rather than by eye — both `ed76347273f1dade0ad7ad32f7f0df9c`. `ADMIN_PORT`
occurs **0** times in either file.

Line counts land on `PLAN.md`'s measured table exactly: `envoy.yaml` **93**,
`envoy-rust.yaml` **91**, `expectations.yaml` **100**, runner **24**.

### Step 6 — GREEN against BOTH real proxies

```
$ cargo build -p envoy-bin        # the harness uses the DEBUG binary
$ cargo test -p differential --test accesslog_grpc_status_filter -- --nocapture
running 1 test
test accesslog_grpc_status_filter ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.73s
```

**The green was audited, not trusted.** Both log files were deleted with `rm -f`
BEFORE the run, so their contents afterwards are produced evidence:

```
/tmp/0094-envoy-mount/access.log        5 lines, 167 bytes, md5 15e3707efd6a476098b64976932db1ac
/tmp/0094-envoy-rust-mount/access.log   5 lines, 167 bytes, md5 15e3707efd6a476098b64976932db1ac

PATH=/g-unimpl CODE=200 GS=Unimplemented
PATH=/g-internal CODE=200 GS=Internal
PATH=/p-unimpl CODE=404 GS=-
PATH=/p-internal CODE=400 GS=-
PATH=/g-param CODE=404 GS=-
```

Byte-identical across the two proxies, and **byte-identical to the five lines
`PLAN.md` Step 6 states were measured at the PLAN-write** — an independent
reproduction on a different day from a different tree. 12.73 s is a normal
backend-free duration, not the ~1 s that would suggest the harness short-circuited.

Note what the last three lines are: the record was KEPT while `%GRPC_STATUS%`
rendered the `-` sentinel. That is the formatter's gated value and the filter's
ungated value disagreeing about the same record, visible in the log file itself.

### Step 7 — THE MUTATION, with its control (PV-7)

Run in a scratch worktree created by `git worktree add --detach` at `c39f7a3`,
with its **own `CARGO_TARGET_DIR`** (sharing the main tree's poisons the test
binary), SEEDED with this task's still-uncommitted fixture files. The main tree
was verified clean of the mutation work afterwards.

Target asserted unique immediately before editing (`grep -cF` → `1`), and a
pristine copy + md5 taken first.

**The mutation** — the derivation gated on `is_grpc_request`, i.e. the shape an
implementation reusing `record.grpc_status` would produce:

```rust
grpc_status_code: if crate::grpc::is_grpc_request(&request.req.headers) {
    effective_grpc_status(response.headers, response.status)
} else {
    2
},
```

Rebuild confirmed real, not cached:

```
   Compiling envoy-http1 v0.0.0 (/.../wt-mut/crates/envoy-http1)
   Compiling envoy-bin v0.0.0 (/.../wt-mut/crates/envoy-bin)
```

**RED, and byte-for-byte the failure `PLAN.md` predicts:**

```
thread 'accesslog_grpc_status_filter' panicked at tests/differential/tests/accesslog_grpc_status_filter.rs:23:10:
fixture green: envoy-rust emitted 2 access-log lines but 5 were expected to be logged;
lines: ["PATH=/g-unimpl CODE=200 GS=Unimplemented", "PATH=/g-internal CODE=200 GS=Internal"]

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.29s
```

The two survivors are probes 1 and 2 — the gRPC-content-type ones the gate lets
through. **The three lost are exactly probes 5, 6 and 8**, the ungated cells,
which is the specific prediction and not merely "some lines went missing".

**THE CONTROL.** File restored from the pristine copy and the restore verified by
md5, not assumed:

```
2c549cbb2a3313cc9813e36ae5468588  (pre-edit)
2c549cbb2a3313cc9813e36ae5468588  (post-restore)      RESTORE EXACT: True
```

Rebuilt (1 `Compiling envoy-http1` line), logs deleted, re-run **from the same
tree**:

```
CONTROL_EXIT=0
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.72s
5 /tmp/0094-envoy-mount/access.log   5 /tmp/0094-envoy-rust-mount/access.log   BYTE-IDENTICAL
```

A mutation RED without its control is not evidence; this one has it, from the
same worktree, same target dir, same fixture. **PV-7 is discharged.** The scratch
worktree was then removed; the four `.claude/worktrees/agent-*` worktrees belong
to a parallel workstream and were left alone.

### PV-9 — and a positive control that initially proved nothing

`PLAN.md` requires `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and
`tests/differential/src/lib.rs` to be untouched. The first probe used
`git diff --stat HEAD -- <files>` and returned empty — **but so did its positive
control**, because at that moment the touched files were already committed (no
diff vs `HEAD`) and the fixture was untracked (invisible to `git diff`). An
empty result from a probe whose control is also empty says nothing at all.

Re-run over the whole phase arc, where the control does discriminate:

```
$ git diff --numstat c9136ae..HEAD -- Cargo.toml Cargo.lock .github/workflows/ci.yml tests/differential/src/lib.rs
[empty]

$ git diff --numstat c9136ae..HEAD -- crates/envoy-config/src/bootstrap.rs crates/envoy-accesslog/src/filter.rs
178	79	crates/envoy-accesslog/src/filter.rs
260	9	crates/envoy-config/src/bootstrap.rs
```

Identical command shape, non-empty on files that ARE touched. **PV-9 holds.**

### A NINTH finding: the README is 166 lines against a measured 73

`PLAN.md` specifies the fixture README as a **section list** rather than verbatim
text — the one code block in the plan that is not quoted in full — and its
measured LoC table carries **73** lines for it. The README written here is
**166**, +93 over the prototype.

**This is the single largest contributor to this phase landing above its measured
938**, and it is a deliberate choice rather than drift: every one of the nine
sections `PLAN.md` Step 5 enumerates is present, and the extra length is the
probe table, the four MEASURED rules, the six authoring constraints and the
quoted four-hunk diff — content the plan asks for but did not size. The closest
landed comparator, `0093`'s README, is 110 lines. Trimming documentation to hit a
LoC figure would be optimising the wrong quantity; the reconciliation is stated
in full at the end of this document instead.

---

## Task 10 — `BEHAVIOR_CONTRACT.md`: the grammar and the runtime rule

**Status: COMPLETE.** Commit: `phase 114 task 10: BEHAVIOR_CONTRACT — the grpc_status_filter grammar and runtime rule`.

The only `docs/` change in the phase, and EXCLUDED from the §6.1 LoC gate.

### Step 1 — locate the section, asserting uniqueness

`PLAN.md` warns not to guess the heading in a 4694-line file. The access-log
FILTER arms turn out to occupy six CONTIGUOUS `### ` sections:

```
2807  ### Phase 70 … status_code_filter — the per-record emission gate
2923  ### Phase 71 … response_flag_filter — the SECOND emission-gate arm
3001  ### Phase 72 … header_filter — the THIRD emission-gate arm
3173  ### Phase 73 … and_filter / or_filter — the FOURTH & FIFTH emission-gate arms
3221  ### Phase 74 … metadata_filter — the SIXTH emission-gate arm
3409  ### Phase 75 … HeaderMatcher ABSENCE semantics        <- the next section
```

So the insertion point is immediately before the phase-75 heading, keeping the
seven arms contiguous and in arm order. That heading was asserted to occur
**exactly once** by whole-line match, and the line above it asserted blank,
before any write.

### Step 2 — the section

`### Phase 114 (ADR-0196/0197): grpc_status_filter — the SEVENTH emission-gate
arm (the UNGATED gRPC-STATUS gate)`, in the house `§A…§I` style, covering the
three sub-sections `PLAN.md` requires:

- **§A** the token grammar as a verdict table, including the PERMISSIVE
  string-numeric forms and the YAML-version note on `y`/`n`/`on`/`off`.
- **§B** the code-1 asymmetry, recorded EXPLICITLY as the plan requires:
  `CANCELED` (one L) here versus `CANCELLED` (two Ls) in the
  `%GRPC_STATUS(SNAKE_STRING)%` table, with the instruction not to unify them.
- **§C–§F** the runtime rule: the header-then-derivation source, the **ungated**
  gate (§D, the load-bearing rule), `exclude` inversion, and empty-keeps-nothing.
- **§G** mutual exclusion as the seventh oneof arm.
- **§H** the envoy-rust scope: two sources of three, both codecs on both landed
  legs, and the four carry-forwards `CF-114-1`/`-3`/`-4`/`-5` each named at the
  claim it bounds.
- **§I** the authoritative fixture with its eight-probe table and the mutation
  result.

### Step 3 — additions only

```
$ git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
135	0	docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

Deletions **0**, as `PLAN.md` Step 3 requires — no existing section was
disturbed. Heading order after the insert: 73 → 74 → **114** → 75, with each
`### ` heading still unique.

---

# Findings — the whole-phase reconciliation

## 1. All ten tasks landed, in the plan's order, with the plan's commit messages

```
2cf0830 phase 114 rider: re-attach finalize_h2_stream's stolen doc comment (CF-113-7)
9a74ac5 phase 114 task 2: narrow pub on http_to_grpc_status so both codecs share ONE map (PV-3)
1935e8e phase 114 task 3: the GrpcStatusFilter config arm and its MEASURED token grammar
45be84a phase 114 task 4: validate the seventh AccessLogFilter arm fail-loud on a bad status token
537d95b phase 114 task 5: the UNGATED effective-gRPC-status derivation on both codecs (PV-4)
bb4b311 phase 114 task 6: widen should_log with the effective gRPC status (behavior-neutral)
dabd773 phase 114 task 7: LogFilter::GrpcStatus — plain-integer membership with exclude inversion
c39f7a3 phase 114 task 8: compile_access_log_filter grows to seven arms
915bde8 phase 114 task 9: differential fixture 0094 — the grpc_status_filter arm, 8 probes
ae8cbd4 phase 114 task 10: BEHAVIOR_CONTRACT — the grpc_status_filter grammar and runtime rule
```

Each `git add` list is the plan's, plus this `PROGRESS.md` (the §5 state machine
requires appending per task, and all ten phase-113 task commits carried it).

## 2. Size — 1038 net against a MEASURED 938 (1.107×), reconciled to FOUR files

Same **14 files** as the prototype, `docs/` excluded (the §6.1 gate's scope):

| file | landed net | prototype | Δ |
|---|---:|---:|---:|
| `tests/fixtures/0094-…/README.md` | 166 | 73 | **+93** |
| `crates/envoy-config/src/bootstrap.rs` | 251 | 245 | +6 |
| `crates/envoy-accesslog/src/filter.rs` | 99 | 97 | +2 |
| `crates/envoy-accesslog/src/record.rs` | 11 | 12 | −1 |
| `crates/envoy-accesslog/src/file_sink.rs` | 8 | 8 | 0 |
| `crates/envoy-config/src/lib.rs` | 7 | 7 | 0 |
| `crates/envoy-http1/src/grpc.rs` | 5 | 5 | 0 |
| `crates/envoy-http1/src/hcm.rs` | 150 | 150 | 0 |
| `crates/envoy-http1/src/lib.rs` | 1 | 1 | 0 |
| `crates/envoy-http2/src/hcm.rs` | 32 | 32 | 0 |
| `tests/differential/tests/accesslog_grpc_status_filter.rs` | 24 | 24 | 0 |
| `tests/fixtures/0094-…/envoy-rust.yaml` | 91 | 91 | 0 |
| `tests/fixtures/0094-…/envoy.yaml` | 93 | 93 | 0 |
| `tests/fixtures/0094-…/expectations.yaml` | 100 | 100 | 0 |
| **TOTAL** | **1038** | **938** | **+100** |

`938 + 93 + 6 + 2 − 1 = 1038` ✓ — the reconciliation closes exactly, with no
residual.

**TEN of the fourteen files land on the prototype's number EXACTLY**, including
all three fixture YAMLs, the runner and both HCM files. The overrun is localised,
not diffuse drift, and its dominant term is the one artifact `PLAN.md` specifies
as a section list rather than quoting — the fixture README, which the plan
therefore never actually sized. See the Task-9 note; it was not trimmed.

**1.107× sits inside the project's MEASURED-estimate band** (`112.1` 1.00×,
`113` 1.07×, `112.2` 1.10×) and nowhere near the PROJECTED band (1.33×–1.66×).
**The §6.1 gate (~25 tasks OR ~1500 net LoC) does not fire at 1038 either**, so
the state-2 no-split adjudication stands on the LANDED number, not only the
predicted one.

The `docs/` slice, excluded from the gate, is **+1424 / −0** (`PROGRESS.md`,
`ADR-0198`, the `BEHAVIOR_CONTRACT.md` section, `STATE.md`, `STATE_HISTORY.md`).

## 3. The four per-crate test targets were reproduced independently

| crate | measured here | `PLAN.md` prototype |
|---|---:|---:|
| `envoy-accesslog` | 133 | 133 ✓ |
| `envoy-config` | 722 | 722 ✓ |
| `envoy-http1` | 243 | 243 ✓ |
| `envoy-http2` | 126 (+1 ignored) | 126 (+1 ignored) ✓ |
| **sum** | **1224** | **1224** ✓ |

Four separate numbers, each reached at the task that closes its crate, on a
different day and a different tree from the prototype.

## 4. The CI-identity prediction — DERIVED BEFORE ANY LOG WAS READ

The baseline `binaries=169 passed=2298 failed=0` has held byte-identical across
**six** consecutive docs-only commits. **This phase lands executable lines, so it
MUST move.** Predicted:

| | baseline | predicted | Δ |
|---|---:|---:|---:|
| `binaries` | 169 | **170** | +1 |
| `passed` | 2298 | **2315** | +17 |
| `failed` | 0 | **0** | 0 |

**Two independent derivations agree**, which is why this is stated as a
prediction rather than a guess:

- **By diff:** `git diff BASE..HEAD -- crates/ tests/` adds **17**
  `#[test]`/`#[tokio::test]` attributes and removes **0**.
- **By per-crate arithmetic:** `envoy-config` 716→722 (+6), `envoy-accesslog`
  129→133 (+4), `envoy-http1` 238→243 (+5), `envoy-http2` 126→127 total (+1
  passed) = **+16** unit tests, plus the new differential runner's **1** test =
  **+17**.

The `+1` binary is the new `accesslog_grpc_status_filter` runner. Fixture census
**93 → 94**, differential runner census **92 → 93** — both re-derived on disk.

⚠ **An UNMOVED identity would mean the new tests did not actually run**, not that
nothing changed.

## 5. Nine `PLAN.md` corrections, applied forward

None is a design error; all are mis-predictions about mechanics, and all are
recorded in `ADR-0198` DECISION 1.

| # | task | the correction |
|---|---|---|
| 1 | 1 | predicted numstat `14	14` measured `10	10`; the *invariant* held, the magnitude did not |
| 2 | 2 | the stated `pub use` insertion point fails the plan's own `fmt` gate; rustfmt sorts that block |
| 3 | 3 | "add to `mod tests`" ≠ "append at EOF" — `bootstrap.rs` has TEN column-0 test modules |
| 4 | 3 | `E0027` forces the validator destructure at Task **3**, not Task 4 |
| 5 | 4 | a plan-supplied anchor is not unique (occurs twice) |
| 6 | 5 | the `E0063` **count** says four; the **enumeration** says five, and the enumeration is right |
| 7 | 6 | Step 5's expected `.should_log(` counts predate Step 4's own pin (`68` → `72`) |
| 8 | 8 | a name-filtered `cargo test` exits **0** with `0 passed; 242 filtered out` — a false green |
| 9 | 9 | the fixture README is 166 lines against a measured 73 |

## 6. Gate posture at the end of the phase

```
cargo build  --workspace --all-targets                            -> 0
cargo clippy --workspace --all-targets --all-features -- -D warnings -> 0
cargo fmt    --all -- --check                                     -> 0
```

**ZERO suppressions survive.** The one `#[allow(clippy::only_used_in_recursion)]`
the plan directed at Task 6 was removed at Task 7, and its absence is asserted
(`only_used_in_recursion` = 0 in `crates/envoy-accesslog/src/`; the marker
`TRANSIENT, PHASE-114` = 0 anywhere in `crates/`). No `#[allow(dead_code)]` and
no `_`-prefixed binding was added at the Task-3 deferral.

⚠ **This is NOT the §5 state-4 verification gate.** `cargo deny`, `cargo fuzz`,
the full differential suite and the conformance suites were NOT run here, and no
§7.5 verdict is claimed. State 4 is a separate session (§5.1; `ADR-0127`).

## 7. Carry-forwards

**`CF-113-7` is CONSUMED** by the Task-1 rider, in its own labelled commit.
**Nothing else was fixed** (§6.3; `ADR-0165`); the phase-112 ALPN cleanup was
neither taken nor re-costed.

`CF-114-1` (no TRAILER source, blocked behind `CF-111-2`), `CF-114-2` (five arms
still unbuilt), `CF-114-3` (no H2 cross-proxy fixture; pinned in-process by
`h2_uses_the_shared_effective_status`), `CF-114-4` (the `exclude` witness;
pinned in-process by `grpc_status_arm_exclude_inverts_over_the_same_code` over
all 17 codes) and `CF-114-5` (a present-but-unparseable `grpc-status` header;
pinned by `derivation_ignores_an_unparseable_or_out_of_range_header`) all stand
as `ADR-0197` left them. Every other banked carry-forward carries forward INTACT;
**`CF-113-4` stays CONSUMED**, **`CF-111-4` consumed only in PART**, **`CF-112-5`
CLOSED**, **`CF-112-8` Consequence 2 BANKED as structurally unwitnessable**.

## 8. What the state-4 session should check first

- The CI identity against the §4 prediction — **and the reason for any gap**,
  rather than recording whatever appears.
- The full differential suite, not just `0094`. Four `access_log_*_upstream_reset`
  tests are in the known stable-core local RED set **and this phase touched the
  access-log filter path**, so a RED naming them is a SUSPECT, not a known flake;
  classify by ISOLATION, never by failure text.
- `cargo deny check` and the fuzz targets, neither run here.
- That the four PV-9 files are still untouched — with a positive control that
  actually discriminates (the first one tried here did not).
