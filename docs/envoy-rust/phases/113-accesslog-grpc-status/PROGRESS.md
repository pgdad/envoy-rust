# Phase 113 — PROGRESS

> §5 **state 3** — the implementation of `docs/envoy-rust/phases/113-accesslog-grpc-status/PLAN.md`.
> Appended to on each task completion, with REAL quoted command output. Written
> for a stranger with zero prior context (D-3.4).
>
> **The unit:** the `%GRPC_STATUS%` access-log command-operator family —
> `%GRPC_STATUS%`, `%GRPC_STATUS(CAMEL_STRING|SNAKE_STRING|NUMBER)%` and the
> alias `%GRPC_STATUS_NUMBER%` — over the HTTP/1.1 local-reply surface, gated on
> the REQUEST being a gRPC request, witnessed by new differential fixture
> `0093-accesslog-grpc-status`.
>
> **Where `PLAN.md` and `SPEC.md` disagree, `PLAN.md` wins** (`ADR-0193`).

---

## A PLAN correction found at Task 1, before any task completed

**`PLAN.md`'s Global Constraints say "Every task ends green on `cargo build
--workspace --all-targets`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings` and `cargo fmt --all -- --check`." That is
STRUCTURALLY UNMEETABLE at the Task-1 and Task-2 boundaries, and it was measured
here rather than argued.**

Task 1 adds `GrpcStatusFormat`, `GRPC_STATUS_NAMES`, `grpc_status_code` and
`render_grpc_status`. Their only *production* consumer is the `Op::GrpcStatus`
render arm, which `PLAN.md` deliberately defers to Task 3 (the compiler forbids
a finer cut there). Between Task 1 and Task 3 those items are reachable only
from `#[cfg(test)]` code, and `dead_code` does not count a test-only use in the
**lib** target. So `-D warnings` fails at the Task-1 boundary BY CONSTRUCTION:

```
error: function `render_grpc_status` is never used
   --> crates/envoy-accesslog/src/command_operator.rs:138:15
    |
138 | pub(crate) fn render_grpc_status(raw: &str, format: GrpcStatusFormat) -> String {
    |               ^^^^^^^^^^^^^^^^^^
    = note: `-D dead-code` implied by `-D warnings`
error: could not compile `envoy-accesslog` (lib) due to 3 previous errors
```

(the other two are `GrpcStatusFormat` and `GRPC_STATUS_NAMES`, same cause).

**Why the PLAN-write did not catch it.** Its 1092-line size measurement was
taken on a scratch tree carrying *every* task's blocks at once. A whole-slice
prototype validates the SLICE; it cannot validate a TASK BOUNDARY, because at
the end of the slice every item has its consumer. This is a recurring class, not
a one-off.

**The correction, applied forward (`PLAN.md` is landed and is NOT edited).** The
task sequence and its per-task commits are kept exactly as planned. The
`-D warnings` clippy gate is DEFERRED from Tasks 1 and 2 to Task 3, the first
boundary at which a production consumer exists. `cargo build --workspace
--all-targets`, `cargo fmt --all -- --check` and the task's own tests are run
and required green at EVERY boundary including 1 and 2 — only the `dead_code`
arm of clippy is deferred, and it is discharged in full at Task 3 and again at
every later task. Nothing is suppressed: no `#[allow(dead_code)]` is added
anywhere, precisely so that a forgotten attribute cannot outlive the gap.
Recorded as `ADR-0194`.

---

## Task 1 — `GrpcStatusFormat`, the canonical name table, and the render helpers

**Status: COMPLETE.** Commit: `phase 113 task 1: GrpcStatusFormat + the MEASURED canonical gRPC status name table`.

### Step 1-2 — the failing tests, RUN and SEEN to fail

Four tests appended to `mod tests` in
`crates/envoy-accesslog/src/command_operator.rs`:
`grpc_status_canonical_table_all_seventeen_codes` (all 17 codes × both
spellings × the number), `grpc_status_out_of_enum_numeric_renders_the_number_in_every_format`,
`grpc_status_unparseable_renders_minus_one_in_every_format`,
`grpc_status_wire_value_tolerances`.

`cargo test -p envoy-accesslog --lib grpc_status`:

```
error[E0433]: cannot find type `GrpcStatusFormat` in this scope
    --> crates/envoy-accesslog/src/command_operator.rs:1160:41
     |
1160 |                 render_grpc_status(raw, GrpcStatusFormat::CamelString),
     |                                         ^^^^^^^^^^^^^^^^ use of undeclared type `GrpcStatusFormat`

error[E0425]: cannot find function `render_grpc_status` in this scope
    --> crates/envoy-accesslog/src/command_operator.rs:1160:17
     |
1160 |                 render_grpc_status(raw, GrpcStatusFormat::CamelString),
     |                 ^^^^^^^^^^^^^^^^^^ not found in this scope

error: could not compile `envoy-accesslog` (lib test) due to 16 previous errors
```

RED for exactly the reason `PLAN.md` Task 1 Step 2 predicts.

### Step 3-4 — implementation, then GREEN

`GrpcStatusFormat`, `GRPC_STATUS_NAMES` (17 entries), `grpc_status_code` and
`render_grpc_status` inserted immediately after the closing `}` of `enum Op`.
The insertion point was asserted structurally before the splice (the preceding
line is `DynamicMetadata { namespace: String, key: String },` and the line
itself is a bare `}`), not taken from an inherited line number.

```
running 4 tests
test command_operator::tests::grpc_status_canonical_table_all_seventeen_codes ... ok
test command_operator::tests::grpc_status_unparseable_renders_minus_one_in_every_format ... ok
test command_operator::tests::grpc_status_wire_value_tolerances ... ok
test command_operator::tests::grpc_status_out_of_enum_numeric_renders_the_number_in_every_format ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 112 filtered out; finished in 0.00s
```

**4 passed is non-tautological** — `112 filtered out` proves the filter selected
a real, non-empty subset rather than matching nothing (`0 passed; N filtered
out` is the false-green form).

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | `Finished \`dev\` profile ... in 5.83s`, exit **0** |
| `cargo fmt --all -- --check` | no output, exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **DEFERRED to Task 3** — `dead_code` only, see the correction above |

The `fmt --check` pass is worth one line: `PLAN.md`'s block contains the
unusual-looking

```rust
                Some((camel, snake)) => if format == GrpcStatusFormat::CamelString {
```

which rustfmt accepts unchanged. That is corroboration that the PLAN's code was
genuinely executed rather than written from reasoning — a hand-written block
would almost certainly have needed reformatting here.

---

## Task 2 — `AccessLogRecord.grpc_status` and the five-site E0063 sweep

**Status: COMPLETE.** Commit: `phase 113 task 2: AccessLogRecord.grpc_status + the five-site E0063 sweep`.

### Step 1-2 — the failing test, RUN and SEEN to fail

`record_grpc_status_defaults_absent_and_carries_the_raw_value` added to
`mod tests` in `crates/envoy-accesslog/src/record.rs`.
`cargo test -p envoy-accesslog --lib record_grpc_status`:

```
error[E0609]: no field `grpc_status` on type `record::AccessLogRecord`
   --> crates/envoy-accesslog/src/record.rs:248:26
    |
248 |         assert_eq!(coded.grpc_status.as_deref(), Some("13"));
    |                          ^^^^^^^^^^^ unknown field
    |
    = note: available fields are: `start_time`, `method`, `path`, `protocol`, `response_code` ... and 14 others
error: could not compile `envoy-accesslog` (lib test) due to 3 previous errors
```

### Step 3 — the field and the sweep

`pub grpc_status: Option<String>` added as the LAST field of `AccessLogRecord`,
with the doc comment `PLAN.md` specifies. Both `19 fields total` occurrences
(module doc and struct doc) updated to `20 fields total`, and the struct doc's
`plus 4 later-phase command-operator targets (...)` enumeration widened to 5 and
extended with `grpc_status` — the enumeration is a second, independent count of
the same thing, and leaving it at 4 while the total said 20 would have shipped a
document that contradicts itself.

**The E0063 site list was re-derived from disk rather than inherited**, by
grepping `AccessLogRecord {` across `crates/` and `tests/`. It returns nine
literals; the five exhaustive ones are exactly those `PLAN.md` names:

| # | site (re-derived at THIS commit) | kind |
|---|---|---|
| 1 | `crates/envoy-http1/src/hcm.rs:1662` `build_access_log_record` | production |
| 2 | `crates/envoy-http2/src/hcm.rs:1158` `finalize_h2_stream` | production |
| 3 | `crates/envoy-accesslog/src/record.rs:128` `test_baseline()` | `#[cfg(test)]` |
| 4 | `crates/envoy-accesslog/src/file_sink.rs:169` `make_record()` | `#[cfg(test)]` |
| 5 | `crates/envoy-http1/src/hcm.rs:2594` `record_get_200()` | `#[cfg(test)]` |

The four `..base` functional-update literals at `record.rs:166/178/190/205` were
NOT touched, as `PLAN.md` requires, and the five `test_baseline()`-based
builders (`command_operator.rs:704`/`:868`, `json_format.rs:293`/`:315`,
`default_format.rs:201`) needed no edit — they inherit the new field
transitively, which is that constructor's stated design intent.

Every replacement asserted its anchor occurs EXACTLY ONCE before splicing.
`grpc_status: None,` is a substring of itself at several sites, so a
count-blind `replace` would have been silently wrong at more than one of them.

### Step 4 — GREEN

`cargo build --workspace --all-targets` → `Finished \`dev\` profile ... in 6.51s`,
exit 0, **zero `E0063` remaining**. That is the load-bearing result: `PLAN.md`
says a sixth literal would mean the enumeration was incomplete and must be
reported. There is no sixth.

`cargo test -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib`:

```
     Running unittests src/lib.rs (target/debug/deps/envoy_accesslog-79243f11c2014c81)
test result: ok. 117 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
     Running unittests src/lib.rs (target/debug/deps/envoy_http1-2f7bcc9b69abd627)
test result: ok. 233 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
     Running unittests src/lib.rs (target/debug/deps/envoy_http2-fd244d1e8f63bdf3)
test result: ok. 124 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.54s
```

⚠ **`PLAN.md` Task 2 Step 4 predicts `127 / 238 / 125`. Those are END-OF-SLICE
prototype figures, not Task-2 figures** — the same whole-slice artefact behind
the clippy finding above. The remaining tasks add 5 http1 tests (Task 6) and 1
http2 test (Task 7), which lands those two exactly on 238 and 125. The
accesslog column is tracked to its final value at Task 5 rather than asserted
against 127 here.

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | exit **0** |
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **DEFERRED to Task 3** — the SAME three `dead_code` items as Task 1, no new one |

The clippy failure set was re-checked, not assumed: it is still exactly
`GRPC_STATUS_NAMES` / `grpc_status_code` / `render_grpc_status`. Task 2 added no
new dead item — the new record field is `pub` on a `pub` struct and so is
reachable.

---

## Task 3 — the parser: the `GRPC_STATUS` arm, the alias, and the empty-`()` correction

**Status: COMPLETE.** Commit: `phase 113 task 3: parse %GRPC_STATUS% family; accept empty () on no-arg operators (measured divergence)`.

This is the task the compiler forbids cutting finer: adding `Op::GrpcStatus`
breaks BOTH exhaustive `match`es over `Op`, so the variant, the parse arm, the
alias, the `render_op` arm and the `encode_single_op` arm land together. Tasks 4
and 5 add only the tests that pin the two arms.

### Step 1-2 — the failing tests, RUN and SEEN to fail

Six tests added: `grpc_status_default_format_is_camel_string`,
`grpc_status_number_alias_agrees_with_number_argument`,
`empty_parens_are_accepted_on_no_arg_operators`,
`grpc_status_rejects_every_measured_bad_argument` (9 spellings),
`grpc_status_rejects_length_suffix_and_argument_on_the_alias`,
`grpc_status_keyword_match_is_exact_not_prefix`.

```
error[E0599]: no variant named `GrpcStatus` found for enum `command_operator::Op`
error: could not compile `envoy-accesslog` (lib test) due to 1 previous error
```

### Step 3 — the implementation, five parts

(a) `Op::GrpcStatus { format: GrpcStatusFormat }` as the 15th variant.
(b) the `"GRPC_STATUS" => parse_grpc_status_op(rest)` arm plus the no-arg guard
relaxed from `rest.is_some()` to `rest.is_some_and(|r| r != "()")`.
(c) `"GRPC_STATUS_NUMBER"` added to `no_arg_op`, constructing the SAME variant
with `format: Number` rather than a second variant.
(d) `parse_grpc_status_op`.
(e) the two forced arms — `render_op` and `encode_single_op` — plus `number_opt`
in `json_format.rs`.

Every splice asserted its anchor occurs exactly once first.

### Step 4 — GREEN, and the regression question the change actually raises

`cargo test -p envoy-accesslog --lib grpc_status` → `10 passed; 0 failed; 113
filtered out`.

⚠ **`PLAN.md` says "Task 1's 4 tests plus these 6" = 10, and 10 is what ran —
but the COMPOSITION is not what that sentence describes.** The filter
`grpc_status` does not match `empty_parens_are_accepted_on_no_arg_operators`
(its name contains no such substring) and does match Task 2's
`record_grpc_status_defaults_absent_and_carries_the_raw_value`. The 10 is
4 (Task 1) + 5 (Task 3) + 1 (Task 2), not 4 + 6. The missing test was run
separately and passes:

```
test command_operator::tests::empty_parens_are_accepted_on_no_arg_operators ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 122 filtered out; finished in 0.00s
```

Recorded because an agreeing TOTAL is not a verified composition — this is the
same class as the PLAN-write's own `cp -r` finding, where two agreeing numbers
turned out to be one measurement.

Whole crate: `cargo test -p envoy-accesslog --lib` → `123 passed; 0 failed`.

**The `()` relaxation changes the contract of ELEVEN pre-existing operators, so
its scope was MEASURED, not argued.** `PLAN.md` requires it not to reach
`REQ`/`RESP`/`DYNAMIC_METADATA`. A temporary probe was appended, run, and then
reverted (the file was restored from a pre-probe copy and `grep -c
tmp_scope_probe` re-checked to 0):

```
    fn tmp_scope_probe_empty_parens_does_not_reach_arg_taking_operators() {
        for f in ["%REQ()%", "%RESP()%", "%DYNAMIC_METADATA()%", "%REQ%", "%DYNAMIC_METADATA%"] {
            assert!(parse_format(f).is_err(), "{f} must STILL be rejected");
        }
    }
```
```
test command_operator::tests::tmp_scope_probe_empty_parens_does_not_reach_arg_taking_operators ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 123 filtered out; finished in 0.00s
```

All five still reject. The relaxation is structurally scoped: `parse_operator`
dispatches `REQ`/`RESP`/`DYNAMIC_METADATA` by exact keyword BEFORE the `other =>
no_arg_op(other)` branch the relaxation lives in, so those three can never reach
it. The pre-existing suites confirm it from the other side —
`dynamic_metadata` 7 passed, `truncat` 6 passed, both unchanged.

The permanent guard against over-widening is inside
`grpc_status_rejects_length_suffix_and_argument_on_the_alias`, which asserts
`%RESPONSE_CODE(FOO)%` is STILL rejected: a NON-empty argument on a no-arg
keyword remains fatal.

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | exit **0** |
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | `Finished \`dev\` profile ... in 4.03s`, exit **0** |

**The deferred clippy gate is DISCHARGED here, in full and for the whole
workspace**, exactly at the boundary the Task-1 correction predicted: the three
`dead_code` items now have production consumers in `render_op` and
`encode_single_op`. No `#[allow]` was needed and none exists.

---

## Task 4 — the text-format render arm: tests

**Status: COMPLETE.** Commit: `phase 113 task 4: pin the %GRPC_STATUS% text-format rendering`.

The arm itself landed in Task 3(e) because the compiler forced it; this task
pins its behaviour end-to-end through `CompiledFormat`.

### Step 1 — the tests

`rec_grpc` / `render1` helpers plus
`grpc_status_text_renders_all_three_spellings` (all five spellings on a present
value) and `grpc_status_absent_renders_the_dash_sentinel` (the gate's
observable).

### Step 2 — the mutation, RUN and SEEN to fail

These tests cannot be seen RED by absence — the code they exercise already
exists. `PLAN.md` therefore specifies a MUTATION as the RED evidence, which is
the correct discipline for a characterization pin. The anchor was asserted
unique before mutating (`anchor occurrences = 1`), then
`None => out.push_str(empty_or_dash),` was changed to
`None => out.push_str("?"),`:

```
test command_operator::tests::grpc_status_absent_renders_the_dash_sentinel ... FAILED
assertion `left == right` failed
  left: "?"
 right: "-"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 124 filtered out; finished in 0.00s
```

Byte-for-byte the failure `PLAN.md` Task 4 Step 2 predicts. **The test bites.**

### Step 3 — revert, and verify the arm reads exactly as specified

Reverted, then confirmed by TEXT rather than by trusting the edit —
`grep -c 'push_str("?")'` returns **0**, and the arm reads:

```rust
        Op::GrpcStatus { format } => match record.grpc_status.as_deref() {
            Some(raw) => out.push_str(&render_grpc_status(raw, *format)),
            None => out.push_str(empty_or_dash),
        },
```

### Step 4 — GREEN

```
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 113 filtered out; finished in 0.00s
```

**12 is exactly the figure `PLAN.md` Task 4 Step 4 predicts.**

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | exit **0** (implied by the clippy `--all-targets` run) |
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit **0** |

---

## Task 5 — the JSON typed carve-out: tests

**Status: COMPLETE.** Commit: `phase 113 task 5: pin %GRPC_STATUS% under the JSON single-operator typed carve-out`.

### Step 1 — the tests

`rec_gs` helper plus `grpc_status_json_typing_present`,
`grpc_status_json_typing_fallbacks_keep_their_type`,
`grpc_status_json_absent_is_null_in_every_format` and
`grpc_status_json_multi_segment_leaves_the_carve_out`.

### Step 2 — the mutation, RUN and SEEN to fail

The `Number` branch of the `encode_single_op` arm was temporarily rewritten to
call `quote_opt` on the rendered string instead of `number_opt` (anchor asserted
unique first):

```
test json_format::tests::grpc_status_json_typing_present ... FAILED
assertion `left == right` failed
  left: "\"5\""
 right: "5"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 128 filtered out; finished in 0.00s
```

Byte-for-byte what `PLAN.md` Task 5 Step 2 predicts — quoted where an unquoted
number is required. **The typed carve-out is genuinely pinned**; had `number_opt`
been redundant, this mutation would have stayed green.

### Step 3-4 — revert, then GREEN

Reverted and re-checked by text
(`grep -c 'GrpcStatusFormat::Number => number_opt'` = **1**).

```
test json_format::tests::grpc_status_json_absent_is_null_in_every_format ... ok
test json_format::tests::grpc_status_json_typing_fallbacks_keep_their_type ... ok
test json_format::tests::grpc_status_json_multi_segment_leaves_the_carve_out ... ok
test json_format::tests::grpc_status_json_typing_present ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.00s
```

Whole crate: `test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.

### The accesslog test-count discrepancy, resolved by measurement

⚠ **`PLAN.md` Task 2 Step 4 predicts the `envoy-accesslog` crate finishes at
`127`. It finishes at `129`, and the extra two are NOT an over-transcription.**

The arithmetic closes exactly: Task 1's filtered run reported `4 passed; 112
filtered out`, so the pre-existing crate total was **112**. This phase adds
4 (T1) + 1 (T2) + 6 (T3) + 2 (T4) + 4 (T5) = **17**, and 112 + 17 = **129**.

The transcription was diffed against `PLAN.md` rather than assumed correct: the
set of test function names added under `crates/envoy-accesslog/` was compared
with the `fn` names appearing in `PLAN.md`'s code blocks, and the only
difference is the token `render`, which is the grep truncating the helper
`render1` (the pattern `[a-z_]+` does not span the digit). **No test in the tree
is absent from the plan, and no planned test is missing from the tree.**

So the `127` is a stale figure in the PLAN's prototype column, not a defect in
this implementation. The http1/http2 columns are checked against their own
predictions at Tasks 6 and 7.

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | exit **0** |
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit **0** |

---

## Task 6 — the HTTP/1.1 population site and the in-process gate pins

**Status: COMPLETE.** Commit: `phase 113 task 6: populate grpc_status at the H1 record build, gated on is_grpc_request`.

This task carries the phase's only behavioural risk, and its tests are the ONLY
witness of the request-side gate anywhere in the tree.

### Step 1-2 — the failing tests, and a PLAN prediction that did NOT hold

Module `grpc_status_access_log_tests` appended to
`crates/envoy-http1/src/hcm.rs`, five tests.

⚠ **`PLAN.md` Task 6 Step 2 predicts "FAIL — 5 failures, every
`grpc_status_for(...)` returning `None`". THREE failed, not five:**

```
test hcm::grpc_status_access_log_tests::gate_stays_shut_on_the_four_measured_negative_spellings ... ok
test hcm::grpc_status_access_log_tests::open_gate_with_no_header_is_none ... ok
test hcm::grpc_status_access_log_tests::gate_opens_on_grpc_content_types ... FAILED
test hcm::grpc_status_access_log_tests::raw_wire_value_is_stored_verbatim ... FAILED
test hcm::grpc_status_access_log_tests::header_name_lookup_is_case_insensitive ... FAILED
test result: FAILED. 2 passed; 3 failed; 0 ignored; 0 measured; 233 filtered out; finished in 0.00s
```

The reasoning behind the prediction is right but its conclusion is not: two of
the five assert that the value is **absent**, and Task 2's placeholder produces
absence unconditionally, so they pass **VACUOUSLY** rather than failing. A test
that asserts `None` cannot be made RED by a site that always returns `None`.

**This matters more than a miscount, because one of the two vacuous passers is
`gate_stays_shut_on_the_four_measured_negative_spellings` — the test
`ADR-0193` DECISION 2 designates as the gate's sole witness.** Its RED evidence
therefore cannot come from the absence of the implementation; it has to come
from the presence of a WRONG one. That check is Step 4 below.

### Step 3 — the implementation

The placeholder at `build_access_log_record` replaced with the gated extract
`PLAN.md` specifies, calling the existing `pub(crate) crate::grpc::is_grpc_request`
from inside `envoy-http1`. **No visibility was widened** — PV-3's requirement —
and none could be: `envoy-accesslog` is a leaf crate and calling into
`envoy-http1` would be a dependency cycle.

### Step 4 — GREEN, then the NON-VACUITY check that the gate actually needs

```
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 233 filtered out; finished in 0.00s
```

Whole crate: `test result: ok. 238 passed; 0 failed` — **exactly the 238
`PLAN.md` predicts for `envoy-http1`.**

**The gate-deletion mutation, run here rather than inherited from `ADR-0193`.**
The gate was deleted — `grpc_status` made an unconditional read of the response
header — and the module re-run:

```
test hcm::grpc_status_access_log_tests::open_gate_with_no_header_is_none ... ok
test hcm::grpc_status_access_log_tests::header_name_lookup_is_case_insensitive ... ok
test hcm::grpc_status_access_log_tests::gate_opens_on_grpc_content_types ... ok
test hcm::grpc_status_access_log_tests::raw_wire_value_is_stored_verbatim ... ok
test hcm::grpc_status_access_log_tests::gate_stays_shut_on_the_four_measured_negative_spellings ... FAILED
assertion `left == right` failed: content-type Some("application/grpc; charset=utf-8") must NOT open the gate
  left: Some("5")
 right: None
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 233 filtered out; finished in 0.00s
```

**EXACTLY ONE test discriminates the gate from no gate, and it is the one
`ADR-0193` DECISION 2 names.** The other four stay green under the mutation —
which is the point: they pin the VALUE path, not the GATE. This independently
re-verifies the in-process half of `CF-113-6` at this commit rather than
inheriting it as a banked claim. The fixture half is re-verified at Task 9.

Mutation reverted; `grep -c 'if crate::grpc::is_grpc_request(&request.req.headers)'`
= **1**, and the crate is back to 238 passed.

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | exit **0** |
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit **0** |

---

## Task 7 — the HTTP/2 boundary: a named helper, not an inline `None`

**Status: COMPLETE.** Commit: `phase 113 task 7: pin the H2 %GRPC_STATUS% boundary behind a named helper (CF-113-2)`.

### Step 1-2 — the failing test

```
error[E0425]: cannot find function `h2_grpc_status` in module `super`
error: could not compile `envoy-http2` (lib test) due to 1 previous error
```

### Step 3-4 — implementation, GREEN

`fn h2_grpc_status() -> Option<String>` added immediately above
`finalize_h2_stream`'s `#[allow(clippy::too_many_arguments)]` (anchor asserted
unique), and Task 2's placeholder in the `AccessLogRecord` literal changed from
`grpc_status: None,` to `grpc_status: h2_grpc_status(),`.

```
test hcm::h2_grpc_status_boundary_tests::h2_grpc_status_is_absent ... ok
test result: ok. 125 passed; 1 ignored; 0 failed; 0 measured; 0 filtered out; finished in 0.54s
```

**125 passed + 1 ignored is exactly `PLAN.md`'s prediction for `envoy-http2`.**
Together with Task 6's 238, two of the three prototype test-count predictions
held precisely; only the accesslog column (127 vs the measured 129) was stale.

`PLAN.md` explicitly forbids pinning this with an `include_str!` source-text
assertion, and none was used — the pin is a behavioural assertion on a named
function, so an unrelated edit to `hcm.rs` cannot break it and a phase lifting
CF-113-2 must delete a test deliberately.

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | exit **0** |
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit **0** |

---

## Task 8 — fuzz corpus seeds for the EXISTING `accesslog_format_parse` target

**Status: COMPLETE.** Commit: `phase 113 task 8: fuzz corpus seeds for the %GRPC_STATUS% keywords`.

No NEW fuzz target: §7.4's "parser, codec, or filter" trigger is satisfied by
the pre-existing `crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs`,
which already covers the format-string parser this phase extends. A new target
would need a `ci.yml` step, and `ci.yml` is on the untouchable list (§5
non-goal 5). The existing target's CI step was confirmed present, so gate (d)
has somewhere to run.

### Step 1 — the failing check

```
$ git check-ignore -q crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/grpc_status.txt && echo IGNORED
IGNORED
```

The PLAIN form was used, not `-v`: the `-v` form also reports negation rules and
its exit code does not answer "is it ignored?".

### Step 2-3 — three seeds, three `!` negations

```
grpc_status.txt: %GRPC_STATUS%
grpc_status_formats.txt: %GRPC_STATUS(SNAKE_STRING)% %GRPC_STATUS(NUMBER)% %GRPC_STATUS_NUMBER%
grpc_status_malformed.txt: %GRPC_STATUS(% %GRPC_STATUS(FOO)% %GRPC_STATUS()% %GRPC_STATUS(CAMEL_STRING):5%
```

The malformed seed deliberately spans the reject surface this phase created: an
unclosed paren, an unknown argument, the newly-ACCEPTED empty `()`, and the
`:N` length suffix.

### Step 4 — verified TRACKED, with `git ls-files` and not `ls`

```
$ git add -A && git ls-files crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/ | wc -l
11
```

**11 = the 8 pre-existing seeds plus these 3, exactly `PLAN.md`'s figure.** Each
of the three was then re-checked individually with the plain `git check-ignore`
form and all three now report NOT ignored. `ls` would have shown the files
whether or not git could see them — the whole point of this task's trap.

---

## Task 9 — differential fixture `0093-accesslog-grpc-status`

**Status: COMPLETE.** Commit: `phase 113 task 9: differential fixture 0093 — the %GRPC_STATUS% family, 12 probes`.

### Step 1-2 — the runner, RUN and SEEN to fail

```
Caused by:
    No such file or directory (os error 2)
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Step 3 — the four fixture files

Numbering re-derived: `tests/fixtures/` holds **92** directories, highest
`0092-tls-alpn-server-preference`, so `0093` is correct.

The two YAMLs were diffed against each other and differ in **exactly the four
harness-mandated ways**, none semantic:

```
2d1
< admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
6c5
<       address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
---
>       address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
14d12
<                 generate_request_id: false
19c17
<                       path: /tmp/0093-envoy-mount/access.log
---
>                       path: /tmp/0093-envoy-rust-mount/access.log
```

`expectations.yaml` was parsed back with a YAML loader and its twelve probes
enumerated, rather than eyeballed — all twelve match `PLAN.md`'s table on path,
content-type and `expected_status`, including the four explicit `404`s on
probes 9-12 that the driver's `200` default would otherwise have broken.

### Step 4 — GREEN, and TWO independent mutations

**The rebuild discipline was honoured on every run.** `cargo test -p
differential` spawns a PRE-BUILT `envoy-bin`; every proxy-side change below was
followed by `cargo build -p envoy-bin` and gated on the `Compiling` lines
appearing, never on the exit code.

Docker was confirmed up and the image identity asserted against the pin before
any conclusion was drawn:

```
$ docker image inspect envoyproxy/envoy:v1.33.0 --format '{{index .RepoDigests 0}}'
envoyproxy/envoy@sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2
```

which is byte-for-byte the `ENVOY_TARGET.md` digest.

**Baseline:** `test result: ok. 1 passed; 0 failed; ... finished in 10.81s`.

**Mutation A — the canonical table (non-vacuity + the upstream positive
control).** The anchor census ran first: the bare form
`    ("Unauthenticated", "UNAUTHENTICATED"),` occurs **2** times — once in the
`const` table and once in the test's own `EXPECT` table — so a naive `sed` would
have mutated impl AND expectation together and read as "vacuous tests". The
disambiguated form (trailing `];`) occurs **1** time and was used. After the
mutation, `grep -n` confirmed only line 131 (the const) changed and line 1240
(the test table) did not.

```
fixture green: access log byte-exact mismatch: line 2 not byte-identical:
  envoy="GS=Unauthenticated SNAKE=UNAUTHENTICATED NUM=16 CODE=200 PATH=/g-unauth"
  envoy-rust="GS=Unauthenticatd SNAKE=UNAUTHENTICATED NUM=16 CODE=200 PATH=/g-unauth"
test result: FAILED. 0 passed; 1 failed; ... finished in 10.72s
```

RED on a ONE-CHARACTER change. **The `envoy=` side is the positive control**: it
proves a real upstream Envoy container ran, served the probe and wrote that line
itself — the fixture's expectations are not being compared against themselves.
It also independently corroborates the measured canonical table's
`Unauthenticated` cell from upstream's own output.

Reverted (`grep -c Unauthenticatd` = 0), rebuilt, GREEN again in 10.72s.

**Mutation B — the gate deleted (re-verifying `CF-113-6` at THIS commit).**
`ADR-0193` DECISION 2 claims fixture `0093` cannot witness the request-side
gate. That is a banked conclusion, so it was re-measured rather than inherited.
The gate was removed from `build_access_log_record`, `envoy-bin` rebuilt, and
BOTH witnesses run against the same compiled tree:

| witness | result under gate deletion |
|---|---|
| fixture `0093` | **`test result: ok. 1 passed; 0 failed`** — GREEN |
| `hcm::grpc_status_access_log_tests` | **`test result: FAILED. 4 passed; 1 failed`** — RED |

**`ADR-0193` DECISION 2 is CONFIRMED and `SPEC.md` §6 is REFUTED at this
commit.** The fixture is blind to the gate, exactly as predicted, because on the
local-reply surface the only producer of a response `grpc-status` header is the
phase-110 transform and it shares the same predicate. The single RED test is
`gate_stays_shut_on_the_four_measured_negative_spellings` — the gate's sole
witness. The fixture `README.md` and `expectations.yaml` both state this
explicitly, so a reviewer cannot record the fixture as covering the gate.

Reverted; `git diff --stat` on both mutated files is EMPTY (byte-identical to
`HEAD`), and the full set re-run green:

```
envoy-http1 grpc_status_access_log : ok. 5 passed; 0 failed; 233 filtered out
fixture 0093                       : ok. 1 passed; 0 failed; finished in 10.69s
```

### PV-8 — the untouchable set

```
$ git diff --numstat 1ff03ba4 -- Cargo.toml Cargo.lock .github/workflows/ci.yml tests/differential/src/lib.rs
(no output)
```

All four untouched across the whole phase, as §5 non-goal 5 requires. No new
dependency, no new workspace crate, no new config surface, no new harness
driver, no new fuzz target.

### Gates at this boundary

| gate | result |
|---|---|
| `cargo build --workspace --all-targets` | exit **0** |
| `cargo fmt --all -- --check` | exit **0** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit **0** |
| fixture `0093` against BOTH real proxies | **GREEN** |

---

## Task 10 — `BEHAVIOR_CONTRACT.md`: extend the `## gRPC` section

**Status: COMPLETE.** Commit: `phase 113 task 10: BEHAVIOR_CONTRACT gRPC section — the %GRPC_STATUS% gate, table and typing`.

The only `docs/` change in the phase, and excluded from the §6.1 LoC gate.

### Step 1 — locate

`grep -c '^## gRPC'` = **1**, at line 644, section running to `## Response
trailers` at line 904. Asserted unique before editing, as `PLAN.md` requires.

### Step 2 — four new subsections appended after §H

`PLAN.md` asks for three; four were written because the empty-`()` rule earns
its own heading rather than a paragraph buried in the typing section — it
changes the contract for ELEVEN pre-existing operators and a reader looking for
`%RESPONSE_CODE()%` will not find it under a `%GRPC_STATUS%` typing heading.
The content is exactly the three items `PLAN.md` enumerates, plus that
promotion.

- **§I — the request-side gate.** The measured 6-row table with the
  `%RESP(grpc-status)%` witness column, the statement that the predicate is
  exactly §B's, and — stated explicitly — that **fixture `0093` does NOT witness
  the gate**, with the mutation result and the generalised rule (when the only
  producer of an observable shares a predicate with the consumer under test, no
  fixture on that surface can distinguish that predicate from no predicate).
- **§J — the canonical name table**, all 17 codes in both spellings, with the
  two uncorrectable cells (0 = `OK` in both; 1 = `Canceled`/`CANCELLED`) called
  out as traps.
- **§K — fallbacks, wire tolerance and JSON typing**, including that `-1` is two
  literal characters and is NOT the `-` absent sentinel.
- **§L — the empty `()` rule**, with its scope limit (does not reach
  `REQ`/`RESP`/`DYNAMIC_METADATA`), the full measured reject set, and the note
  that `(  )` with spaces is NOT the accepted empty `()`. Closes with the H2
  boundary (CF-113-2).

### Step 3 — no other section disturbed

```
$ git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
138	0	docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

**Additions only, deletions 0**, exactly as `PLAN.md` Step 3 requires. The `## `
heading count is unchanged against `HEAD` and the `## gRPC` section's `### `
count went 8 → 12, i.e. the four new subsections and nothing else.

---

# Implementation complete — the §5 state-3 close

**All TEN `PLAN.md` tasks landed, in order, each with TDD and each with its own
task commit.** The active unit is now phase 113 at §5 **state 4** (the §7.5
verification gate), which is a SEPARATE session (§5.1; `ADR-0127`).

## Task ledger

| task | commit | what landed |
|---|---|---|
| 1 | `abbe107` | `GrpcStatusFormat`, the MEASURED 17-code table, `grpc_status_code`, `render_grpc_status` |
| 2 | `2201c36` | `AccessLogRecord.grpc_status` + the five-site E0063 sweep |
| 3 | `5aeda62` | the parser, the alias, both compiler-forced `match` arms, `number_opt`, the empty-`()` correction |
| 4 | `8233136` | the text-render pin (mutation-proved) |
| 5 | `d3ce3ef` | the JSON typed carve-out pin (mutation-proved) |
| 6 | `f151731` | the H1 population site + the gate pins (gate-deletion-proved) |
| 7 | *(in `f151731`'s successor)* | the H2 boundary helper `h2_grpc_status()` + its pin |
| 8 | `9712ef4` | three fuzz corpus seeds + their `.gitignore` negations |
| 9 | `e53582f` | fixture `0093` (12 probes) + its runner, GREEN cross-proxy, two mutations |
| 10 | `0b0c7ea` | `BEHAVIOR_CONTRACT.md` §I-§L |

## Size — the §6.1 calibration datapoint

```
$ git diff --numstat 1ff03ba4 HEAD -- . ':(exclude)docs/'
added=1172 deleted=7 net=1165
```

**1165 landed vs the PLAN's MEASURED 1092 = 1.07×.** The §6.1 gate is ~1500, so
this clears by **335 lines / 22%**, and no mid-execution split trigger fired (no
task's sub-steps approached ~10 items).

Per-file against the PLAN's table: `json_format.rs` 85 = 85, `envoy-http1/hcm.rs`
133 = 133, the fuzz set 6 = 6, the runner 25 = 25 — four exact hits.
`command_operator.rs` 364 vs 362 and `record.rs` 26 vs 24 are within noise.
`envoy-http2/hcm.rs` came in UNDER at 27 vs 34. **The overage is essentially all
fixture prose**: 498 vs 422, because the `README.md` and `expectations.yaml`
carry the CF-113-6 limitation and the per-probe rationale that `PLAN.md`
required in words but did not budget lines for.

This is the **third measured-estimate datapoint**: `112.1` 1.00×, `112.2` 1.10×,
`113` 1.07× — all far under the projected-estimate band (`110.2` 1.33×, `110.1`
1.41×, `111` 1.66×). The discriminator remains METHOD, not luck.

## Test-suite state at this commit

`cargo test --workspace --lib` → **1884 passed, 1 failed**.

**The one failure is classified by ISOLATION, never by its text**:
`envoy-http2 client::tests::send_request_maps_h2_handshake_failure_to_typed_error`
(`expected H2ClientHandshake, got Ok(ClientStream { host: "test.example", .. })`).
Re-run ALONE with 5-second settle gaps it passes **3/3**:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 125 filtered out; finished in 0.00s
```

It is the documented pre-existing host flake (the handshake unexpectedly
SUCCEEDS on this host). The structural check agrees and is independent of the
isolation result: **phase 113's only `envoy-http2` change is in `hcm.rs`**, while
this test lives in `client.rs`, which
`git diff --stat 1ff03ba4 HEAD -- crates/envoy-http2/src/client.rs` shows
UNTOUCHED. CI is authoritative.

New tests added by this phase: **24** — 17 `envoy-accesslog` (112 → 129),
5 `envoy-http1` (233 → 238), 1 `envoy-http2` (124 → 125), plus the 1 differential
fixture test, which also adds ONE test binary.

## What this session did NOT do

- **Did not run the §7.5 verification gate.** That is state 4, a separate
  session. `cargo deny check`, `cargo fuzz`, the conformance suites and the
  full 93-fixture differential sweep are all its work, not this one's.
- **Did not touch `ROADMAP.md`.** Row `113` stays `planned` until the state-6
  close-out. Census unchanged: 121 rows / 120 `done` / 1 `planned`.
- **Did not edit `SPEC.md` or `PLAN.md`.** Both are landed and uneditable; every
  correction is forward, in this file and `ADR-0194`.
- **Did not fix anything outside phase 113** (§6.3; `ADR-0165`). Every
  carry-forward stands INTACT — `CF-113-1`…`CF-113-6`, `CF-112-1`…`CF-112-19`,
  the `112.1`/`112.2`/`111`/`110.x`/`109.x`/`108.2` REVIEW sets,
  `CF-111-1`…`CF-111-9`, `CF-110-1`…`9`, `CF-109-1/2/3`, `CF-108-1/2/3`,
  `CF-76-1`, `CF-75-2/3/4/5/6`, `CF-72-2`/`CF-75-1`, `M71-6`,
  `CF-74-1/2/3/4/6`, `CF-73-1` and the HTTP-filters-family (1)-(4).
  **`CF-111-4` remains consumed only in PART** (the `%TRAILER(name)%` half is
  untouched). **`CF-112-5` stays CLOSED.** The phase-112 ALPN cleanup rider was
  NOT taken.
- **Did not create a `stop` file.** All three stop-condition legs were
  re-measured FALSE from disk at session start.

## PV-8

```
$ git diff --numstat 1ff03ba4 -- Cargo.toml Cargo.lock .github/workflows/ci.yml tests/differential/src/lib.rs
(no output)
```

All four untouched across the whole phase: no new dependency, no new workspace
crate, no new config surface, no new harness driver, no new fuzz target.

## For the state-4 session

- **`cargo build -p envoy-bin` BEFORE any differential run** and gate on the
  `Compiling` line, not the exit code — the harness spawns a pre-built binary.
- **Only isolation classifies a local RED**, with a settle gap between Docker
  runs; back-to-back runs manufacture a false `FAILS-IN-ISOLATION`.
- **Do NOT record fixture `0093` as covering the request-side gate.** It cannot,
  and `ADR-0194` DECISION 3 re-measured that at this commit.
- Gate (d) is the PRE-EXISTING `accesslog_format_parse` target, which gained
  three seeds; there is no new fuzz target and `ci.yml` is unchanged.
