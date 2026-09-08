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
