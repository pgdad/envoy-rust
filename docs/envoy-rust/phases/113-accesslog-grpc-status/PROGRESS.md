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
