# Phase 113 — PLAN

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the `%GRPC_STATUS%` access-log command-operator family — `%GRPC_STATUS%`, `%GRPC_STATUS(CAMEL_STRING|SNAKE_STRING|NUMBER)%` and `%GRPC_STATUS_NUMBER%` — over the HTTP/1.1 local-reply surface, witnessed by new differential fixture `0093-accesslog-grpc-status`.

**Architecture:** One new `Op` variant carrying a format enum, backed by ONE new `Option<String>` field on `AccessLogRecord` holding the RAW wire value of the response `grpc-status` header. The HTTP/1.1 HCM populates that field at its existing record-build site, gated on the phase-110 `is_grpc_request` predicate; the renderer stays a pure function of the record, so the engine's "backed by a distilled field" invariant is preserved. HTTP/2 pins the field absent behind a named, tested helper.

**Tech Stack:** Rust 2024, `envoy-accesslog` (a leaf crate with zero intra-workspace dependencies), `envoy-http1`, `envoy-http2`, the existing `Driver::Http1AccessLogByteExact` differential driver, the existing `accesslog_format_parse` fuzz target.

**Spec:** `docs/envoy-rust/phases/113-accesslog-grpc-status/SPEC.md`. **Read it together with this plan — but see "SPEC corrections" below: this PLAN-write measured four SPEC claims to be wrong, and `ADR-0193` is the forward correction.**

---

## Global Constraints

- Upstream target is `envoyproxy/envoy:v1.33.0`, digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`. Every behavioural cell in this plan was MEASURED against it at the PLAN-write; none was recalled or derived.
- `#![forbid(unsafe_code)]` in every crate root (D-3.8). No `unsafe` anywhere in this phase.
- **No new dependency, no new workspace crate, no new config surface, no new harness driver, no new fuzz target.** `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` and `tests/differential/src/lib.rs` MUST be untouched — verified untouched on the prototype (PV-8). If a task appears to need one of them, STOP: the scope has drifted.
- Every task ends green on `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`. All three were clean on the prototype.
- `AccessLogRecord` deliberately does NOT implement `Default` (`record.rs:26-28`), so a new field is an `E0063` at every exhaustive struct literal. **There are exactly FIVE such literals** (Task 2).
- `envoy-accesslog` has **zero intra-workspace dependencies** and is depended on BY `envoy-http1`/`envoy-http2`/`envoy-config`/`envoy-admin`. It therefore CANNOT call `envoy-http1::grpc` — that would be a dependency cycle. All gRPC detection happens HCM-side (PV-3).
- Nothing is fixed (§6.3; `ADR-0165`). No carry-forward outside this phase is consumed.

---

## SPEC corrections this PLAN-write measured — read before Task 1

Four claims in the landed `SPEC.md` did not survive PV-1…PV-8. `SPEC.md` is landed and is NOT edited; `ADR-0193` carries the forward correction. Where this plan and the SPEC disagree, **this plan wins**.

1. **`SPEC.md` §7's "E0063 sweep over 10 construction sites (2 production, 8 test)" is WRONG.** There are 9 `AccessLogRecord` struct literals; only **5** are exhaustive and break (2 production + 3 test). The other 4 (`record.rs:166/178/190/205`) use functional-update `..base` syntax and absorb a new field silently. The remaining five accesslog-crate test builders go through `AccessLogRecord::test_baseline()` and need no edit at all.
2. **`SPEC.md` §4 deliverable 2's "two new `Op` variants (14 → 16)" is over-specified.** `%GRPC_STATUS_NUMBER%` and `%GRPC_STATUS(NUMBER)%` were MEASURED byte-identical upstream in BOTH the text and the JSON format, so they are one operator with two spellings. This plan adds **ONE** variant (14 → 15) and makes the alias construct it.
3. **`SPEC.md` §4 deliverable 3's reject set is incomplete: an EMPTY `()` argument is ACCEPTED upstream** — on `%GRPC_STATUS%`, on `%GRPC_STATUS_NUMBER%`, and on EVERY pre-existing no-arg operator (`%RESPONSE_CODE()%` and `%PROTOCOL()%` both load `configuration ... OK`). envoy-rust currently REJECTS all of these. That is a real, previously-undiscovered divergence in the landed engine, and Task 3 closes it generally because doing so is strictly less code than special-casing the new keyword.
4. **`SPEC.md` §6's claim that probes 9-12 are the gate's negative controls — "an implementation that ignores the gate and always reads the header would go RED on four probes" — is FALSE.** It was tested: deleting the gate and rebuilding `envoy-bin` leaves fixture `0093` **GREEN**. On the local-reply surface the only producer of a response `grpc-status` header is the phase-110 transform, which is gated on the SAME predicate — so with the gate shut there is no header to read either way. **The gate is pinned IN-PROCESS instead (Task 6), and that test IS red under the same mutation.** Probes 9-12 still earn their place by witnessing that the transform did not fire (`expected_status: 404`), but their rationale must be stated correctly.

---

## File Structure

| file | responsibility | change |
|---|---|---|
| `crates/envoy-accesslog/src/command_operator.rs` | format enum, canonical name table, the two render helpers, the `Op` variant, the parse arm, the `no_arg_op` alias, the text render arm | modify |
| `crates/envoy-accesslog/src/record.rs` | the `grpc_status` field + `test_baseline` | modify |
| `crates/envoy-accesslog/src/file_sink.rs` | one E0063 test literal | modify |
| `crates/envoy-accesslog/src/json_format.rs` | `number_opt` helper + the `encode_single_op` arm | modify |
| `crates/envoy-http1/src/hcm.rs` | the population site + one E0063 test literal + the in-process gate tests | modify |
| `crates/envoy-http2/src/hcm.rs` | the `h2_grpc_status()` boundary helper + its pin | modify |
| `crates/envoy-accesslog/fuzz/.gitignore` | three `!` negations | modify |
| `crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/grpc_status*.txt` | three fuzz seeds | create |
| `tests/fixtures/0093-accesslog-grpc-status/{envoy,envoy-rust,expectations}.yaml`, `README.md` | the differential fixture | create |
| `tests/differential/tests/accesslog_grpc_status.rs` | the fixture runner | create |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | the `## gRPC` section extension | modify |

**Task order is dependency-forced.** Task 4 (the `Op` variant) breaks the build until BOTH exhaustive `match`es over `Op` gain an arm, so those three edits are ONE task — the compiler will not let them be separate.

---

## §6.1 SPLIT GATE — MEASURED, DOES NOT FIRE

The gate is ~25 tasks OR ~1500 net LoC. **This plan is 9 tasks and MEASURED 1092 net LoC excluding `docs/`.**

The measurement is not a projection. Every task below was built twice in scratch worktrees, each with its own `CARGO_TARGET_DIR`: first as a free-hand prototype, then a second tree carrying **this plan's code blocks inserted verbatim**. The second tree is the authoritative one, because it is the thing an executor will actually produce. It builds, passes `clippy --workspace --all-targets --all-features -- -D warnings`, passes `fmt --all --check`, passes all **492** unit tests across the three affected crates (129 / 238 / 125+1 ignored), and turns fixture `0093` GREEN against both real proxies.

| file | net |
|---|---|
| `crates/envoy-accesslog/src/command_operator.rs` | 362 |
| `crates/envoy-accesslog/src/json_format.rs` | 85 |
| `crates/envoy-accesslog/src/record.rs` | 24 |
| `crates/envoy-accesslog/src/file_sink.rs` | 1 |
| `crates/envoy-http1/src/hcm.rs` | 133 |
| `crates/envoy-http2/src/hcm.rs` | 34 |
| `crates/envoy-accesslog/fuzz/.gitignore` + 3 seeds | 6 |
| `tests/differential/tests/accesslog_grpc_status.rs` | 25 |
| `tests/fixtures/0093-accesslog-grpc-status/` (4 files) | 422 |
| **TOTAL NET** | **1092** |

The SPEC's central projection was ≈980, so this lands at **1.11×** the projection — inside the measured-estimate band (`112.1` 1.00×, `112.2` 1.10×) and nowhere near the projected-estimate band (1.33×–1.66×). Against the 1500 gate that leaves a **408-line / 27% margin**; even a further 1.10× drift lands ≈1201 and still clears.

⚠⚠ **A METHOD WARNING, PAID FOR AT THIS PLAN-WRITE.** The first attempt to measure the two trees produced 1092 for BOTH and read as corroboration. It was not: `/tmp/planverify` was created with `cp -r` of a git worktree, and a worktree's `.git` is a *pointer file*, so both trees shared ONE index and each `git add -A` overwrote the other's measurement. The real figures — taken with a per-tree `GIT_INDEX_FILE` seeded by `git read-tree <HEAD>` — are 1067 (prototype) and **1092 (this plan)**. **Two numbers that agree are not two measurements until you have proved they came from two indexes.**

⚠ **`ADR-0189`'s measured 595 landed at 652 purely because the plan was edited AFTER the measurement.** The 1092 above was taken against this plan's FINAL content, including the fixture README and the `.gitignore` negations that an earlier pass silently omitted. **If you edit this plan, re-measure.**

**Therefore: NO SPLIT. `ADR-0193`'s reservation for a split is RELEASED and the number is consumed by the PLAN-write ADR instead**, so `DECISIONS.md` gains no new gap.

---

## Task 1: The format enum, the canonical name table, and the render helpers

Pure functions with no dependency on `Op` or on the record — they compile and are fully testable on their own.

**Files:**
- Modify: `crates/envoy-accesslog/src/command_operator.rs` (add after the `Op` enum)
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub enum GrpcStatusFormat { CamelString, SnakeString, Number }` (derives `Debug, Clone, Copy, PartialEq, Eq`); `pub(crate) fn grpc_status_code(raw: &str) -> i64`; `pub(crate) fn render_grpc_status(raw: &str, format: GrpcStatusFormat) -> String`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/envoy-accesslog/src/command_operator.rs`:

```rust
    // The full canonical table, all 17 codes, both spellings. MEASURED against
    // envoyproxy/envoy:v1.33.0 by driving grpc-status 0..16 through an access
    // log. Codes 0 and 1 are the reason this is measured and not derived: 0 is
    // `OK` in BOTH spellings (not `Ok`), and 1 is `Canceled` (one L) in camel
    // but `CANCELLED` (two Ls) in snake.
    #[test]
    fn grpc_status_canonical_table_all_seventeen_codes() {
        const EXPECT: [(&str, &str); 17] = [
            ("OK", "OK"),
            ("Canceled", "CANCELLED"),
            ("Unknown", "UNKNOWN"),
            ("InvalidArgument", "INVALID_ARGUMENT"),
            ("DeadlineExceeded", "DEADLINE_EXCEEDED"),
            ("NotFound", "NOT_FOUND"),
            ("AlreadyExists", "ALREADY_EXISTS"),
            ("PermissionDenied", "PERMISSION_DENIED"),
            ("ResourceExhausted", "RESOURCE_EXHAUSTED"),
            ("FailedPrecondition", "FAILED_PRECONDITION"),
            ("Aborted", "ABORTED"),
            ("OutOfRange", "OUT_OF_RANGE"),
            ("Unimplemented", "UNIMPLEMENTED"),
            ("Internal", "INTERNAL"),
            ("Unavailable", "UNAVAILABLE"),
            ("DataLoss", "DATA_LOSS"),
            ("Unauthenticated", "UNAUTHENTICATED"),
        ];
        for (code, (camel, snake)) in EXPECT.iter().enumerate() {
            let raw = code.to_string();
            assert_eq!(
                render_grpc_status(&raw, GrpcStatusFormat::CamelString),
                *camel,
                "camel code {code}"
            );
            assert_eq!(
                render_grpc_status(&raw, GrpcStatusFormat::SnakeString),
                *snake,
                "snake code {code}"
            );
            assert_eq!(
                render_grpc_status(&raw, GrpcStatusFormat::Number),
                raw,
                "number code {code}"
            );
        }
    }

    // An OUT-OF-ENUM numeric falls back to the NUMBER in EVERY format — not
    // `-`, not `Unknown`, not an error. MEASURED: `99` renders `99` under
    // CAMEL_STRING.
    #[test]
    fn grpc_status_out_of_enum_numeric_renders_the_number_in_every_format() {
        for raw in ["17", "99", "-1"] {
            for f in [
                GrpcStatusFormat::CamelString,
                GrpcStatusFormat::SnakeString,
                GrpcStatusFormat::Number,
            ] {
                assert_eq!(render_grpc_status(raw, f), raw, "raw {raw} format {f:?}");
            }
        }
    }

    // An UNPARSEABLE value renders the literal `-1` in every format — two
    // characters, NOT the `-` absent-sentinel and NOT the raw string.
    #[test]
    fn grpc_status_unparseable_renders_minus_one_in_every_format() {
        for raw in ["notanumber", "5.0", ""] {
            for f in [
                GrpcStatusFormat::CamelString,
                GrpcStatusFormat::SnakeString,
                GrpcStatusFormat::Number,
            ] {
                assert_eq!(render_grpc_status(raw, f), "-1", "raw {raw:?} format {f:?}");
            }
        }
    }

    // MEASURED wire-value tolerances: surrounding whitespace is stripped, and
    // `+5` / `05` both parse to 5.
    #[test]
    fn grpc_status_wire_value_tolerances() {
        for raw in [" 5", "5 ", "  5  ", "05", "+5"] {
            assert_eq!(
                render_grpc_status(raw, GrpcStatusFormat::CamelString),
                "NotFound",
                "raw {raw:?}"
            );
        }
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-accesslog --lib grpc_status`
Expected: FAIL to COMPILE — `cannot find function 'render_grpc_status' in this scope` and `cannot find type 'GrpcStatusFormat' in this scope`.

- [ ] **Step 3: Write the implementation**

Insert into `crates/envoy-accesslog/src/command_operator.rs`, immediately AFTER the closing `}` of the `Op` enum:

```rust
/// The rendering format of a `%GRPC_STATUS%` operator. `CamelString` is the
/// default (`%GRPC_STATUS%` with no argument renders `NotFound`, not `5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcStatusFormat {
    /// `CAMEL_STRING` — upstream's `Grpc::Utility::grpcStatusToString` spelling.
    CamelString,
    /// `SNAKE_STRING` — the SCREAMING_SNAKE_CASE spelling.
    SnakeString,
    /// `NUMBER` — the integer code.
    Number,
}

/// The canonical gRPC status code table, MEASURED against
/// `envoyproxy/envoy:v1.33.0` by driving codes 0-16 through an access log.
///
/// Two cells are counter-intuitive and are the reason this table is measured
/// rather than derived: code 0 is `OK` in BOTH spellings (not `Ok`), and code 1
/// is `Canceled` (one L) in camel but `CANCELLED` (two Ls) in snake.
const GRPC_STATUS_NAMES: &[(&str, &str)] = &[
    ("OK", "OK"),
    ("Canceled", "CANCELLED"),
    ("Unknown", "UNKNOWN"),
    ("InvalidArgument", "INVALID_ARGUMENT"),
    ("DeadlineExceeded", "DEADLINE_EXCEEDED"),
    ("NotFound", "NOT_FOUND"),
    ("AlreadyExists", "ALREADY_EXISTS"),
    ("PermissionDenied", "PERMISSION_DENIED"),
    ("ResourceExhausted", "RESOURCE_EXHAUSTED"),
    ("FailedPrecondition", "FAILED_PRECONDITION"),
    ("Aborted", "ABORTED"),
    ("OutOfRange", "OUT_OF_RANGE"),
    ("Unimplemented", "UNIMPLEMENTED"),
    ("Internal", "INTERNAL"),
    ("Unavailable", "UNAVAILABLE"),
    ("DataLoss", "DATA_LOSS"),
    ("Unauthenticated", "UNAUTHENTICATED"),
];

/// Normalise a raw wire `grpc-status` value to the integer upstream renders.
/// MEASURED: surrounding whitespace is tolerated (` 5` and `5 ` both render
/// `NotFound`), `+5` and `05` both parse to 5, and anything unparseable (e.g.
/// `5.0`, `notanumber`) renders the literal `-1` in EVERY format.
pub(crate) fn grpc_status_code(raw: &str) -> i64 {
    raw.trim().parse::<i64>().unwrap_or(-1)
}

/// Render a raw wire `grpc-status` value in the requested format. An
/// out-of-enum numeric renders as the NUMBER in every format (MEASURED: `99`
/// renders `99` under CAMEL_STRING, not `Unknown` and not an error).
pub(crate) fn render_grpc_status(raw: &str, format: GrpcStatusFormat) -> String {
    let code = grpc_status_code(raw);
    match format {
        GrpcStatusFormat::Number => code.to_string(),
        GrpcStatusFormat::CamelString | GrpcStatusFormat::SnakeString => {
            match usize::try_from(code)
                .ok()
                .and_then(|i| GRPC_STATUS_NAMES.get(i))
            {
                Some((camel, snake)) => if format == GrpcStatusFormat::CamelString {
                    camel
                } else {
                    snake
                }
                .to_string(),
                None => code.to_string(),
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib grpc_status`
Expected: PASS, 4 tests.
Then: `cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`
Expected: both clean.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/command_operator.rs
git commit -m "phase 113 task 1: GrpcStatusFormat + the MEASURED canonical gRPC status name table"
```

---

## Task 2: The `AccessLogRecord.grpc_status` field and the five-site E0063 sweep

**Files:**
- Modify: `crates/envoy-accesslog/src/record.rs` (field + `test_baseline`)
- Modify: `crates/envoy-accesslog/src/file_sink.rs` (one test literal)
- Modify: `crates/envoy-http1/src/hcm.rs` (production build site + one test literal)
- Modify: `crates/envoy-http2/src/hcm.rs` (production build site)

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `AccessLogRecord.grpc_status: Option<String>` (the RAW wire value). All five sites set `None` in this task; population arrives in Tasks 6 and 7.

**The complete E0063 list — these FIVE and no others.** Do not go hunting; this was enumerated exhaustively at the PLAN-write and confirmed by a clean compile.

| # | site | kind |
|---|---|---|
| 1 | `crates/envoy-http1/src/hcm.rs` — inside `build_access_log_record`, last field `dynamic_metadata: request.dynamic_metadata.clone(),` | production |
| 2 | `crates/envoy-http2/src/hcm.rs` — inside `finalize_h2_stream`, last field `dynamic_metadata,` | production |
| 3 | `crates/envoy-accesslog/src/record.rs` — `AccessLogRecord::test_baseline()` | `#[cfg(test)]` |
| 4 | `crates/envoy-accesslog/src/file_sink.rs` — `fn make_record()` | `#[cfg(test)]` |
| 5 | `crates/envoy-http1/src/hcm.rs` — `fn record_get_200()` | `#[cfg(test)]` |

⚠ **Do NOT touch `record.rs`'s four functional-update literals** (`AccessLogRecord { … , ..empty }` / `..absent`). They are immune to `E0063` and compile unchanged. ⚠ **Do NOT touch the five `test_baseline()`-based builders** in `command_operator.rs`, `json_format.rs` and `default_format.rs` — adding the field to `test_baseline` covers them all transitively, which is the design intent stated in that function's own doc.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `crates/envoy-accesslog/src/record.rs`:

```rust
    // Phase 113: the %GRPC_STATUS% backing field. Absent by default — the HCM
    // populates it only when the REQUEST was a gRPC request.
    #[test]
    fn record_grpc_status_defaults_absent_and_carries_the_raw_value() {
        let absent = AccessLogRecord::test_baseline();
        assert!(absent.grpc_status.is_none());

        let coded = AccessLogRecord {
            grpc_status: Some("13".into()),
            ..absent
        };
        assert_eq!(coded.grpc_status.as_deref(), Some("13"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-accesslog --lib record_grpc_status`
Expected: FAIL to COMPILE — `struct 'AccessLogRecord' has no field named 'grpc_status'`.

- [ ] **Step 3: Write the implementation**

In `crates/envoy-accesslog/src/record.rs`, add the field as the LAST field of the struct (after `dynamic_metadata`):

```rust
    /// Raw wire value of the response `grpc-status` header, captured ONLY when
    /// the REQUEST was a gRPC request (phase 113). `None` on every non-gRPC
    /// request, even when the response carries the header — that gate is the
    /// measured upstream rule, not an optimisation. Stored as the raw string
    /// (not a parsed integer) so the renderer can reproduce upstream's
    /// out-of-enum and unparseable fallbacks. Rendered by `%GRPC_STATUS%` /
    /// `%GRPC_STATUS(CAMEL_STRING|SNAKE_STRING|NUMBER)%` / `%GRPC_STATUS_NUMBER%`
    /// — absent → `-` sentinel / json `null`.
    pub grpc_status: Option<String>,
```

In the same file, update the struct doc's field count from `19 fields total` to `20 fields total`, and add `grpc_status: None,` as the last field of `test_baseline()`.

Then add `grpc_status: None,` as the last field of sites 1, 2, 4 and 5 in the table above. (Sites 1 and 2 get their real values in Tasks 6 and 7; `None` here keeps the tree green.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo build --workspace --all-targets`
Expected: SUCCESS. If any `E0063` remains, the missing site is a SIXTH literal — stop and report it, because the enumeration above is meant to be complete.
Run: `cargo test -p envoy-accesslog -p envoy-http1 -p envoy-http2 --lib`
Expected: PASS — 127 / 238 / 125 tests respectively on the prototype, all green.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/record.rs crates/envoy-accesslog/src/file_sink.rs crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 113 task 2: AccessLogRecord.grpc_status + the five-site E0063 sweep"
```

---

## Task 3: The parser — the `GRPC_STATUS` arm, the `GRPC_STATUS_NUMBER` alias, and the empty-`()` correction

⚠ This task adds the `Op::GrpcStatus` variant, which breaks BOTH exhaustive `match`es over `Op`. Those two arms land in Task 4 and Task 5. **To keep every task individually green, add the variant AND both render arms in this task's Step 3** — the arms' bodies are given in Tasks 4 and 5, which then only add their tests. This is the one place the compiler forbids a finer cut.

**Files:**
- Modify: `crates/envoy-accesslog/src/command_operator.rs`
- Modify: `crates/envoy-accesslog/src/json_format.rs`
- Test: `crates/envoy-accesslog/src/command_operator.rs`, `mod tests`

**Interfaces:**
- Consumes: `GrpcStatusFormat`, `render_grpc_status`, `grpc_status_code` (Task 1); `AccessLogRecord.grpc_status` (Task 2).
- Produces: `Op::GrpcStatus { format: GrpcStatusFormat }` — the 15th `Op` variant; `fn parse_grpc_status_op(rest: Option<&str>) -> Result<Op, FormatParseError>`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/envoy-accesslog/src/command_operator.rs`:

```rust
    // The default (no argument) format is CAMEL_STRING, NOT the number and NOT
    // the snake spelling.
    #[test]
    fn grpc_status_default_format_is_camel_string() {
        assert_eq!(
            parse_format("%GRPC_STATUS%").unwrap(),
            vec![Segment::Op(Op::GrpcStatus {
                format: GrpcStatusFormat::CamelString
            })]
        );
    }

    // `%GRPC_STATUS_NUMBER%` and `%GRPC_STATUS(NUMBER)%` are the SAME operator:
    // MEASURED byte-identical upstream in both the text and the JSON format.
    #[test]
    fn grpc_status_number_alias_agrees_with_number_argument() {
        assert_eq!(
            parse_format("%GRPC_STATUS_NUMBER%").unwrap(),
            parse_format("%GRPC_STATUS(NUMBER)%").unwrap()
        );
    }

    // An EMPTY `()` argument is ACCEPTED and means the default — on
    // `%GRPC_STATUS%` AND on every pre-existing no-arg operator. MEASURED:
    // `%GRPC_STATUS()%`, `%GRPC_STATUS_NUMBER()%`, `%RESPONSE_CODE()%` and
    // `%PROTOCOL()%` all load `configuration ... OK` on
    // envoyproxy/envoy:v1.33.0. envoy-rust rejected all four before this phase.
    #[test]
    fn empty_parens_are_accepted_on_no_arg_operators() {
        for (with, without) in [
            ("%GRPC_STATUS()%", "%GRPC_STATUS%"),
            ("%GRPC_STATUS_NUMBER()%", "%GRPC_STATUS_NUMBER%"),
            ("%RESPONSE_CODE()%", "%RESPONSE_CODE%"),
            ("%PROTOCOL()%", "%PROTOCOL%"),
        ] {
            assert_eq!(
                parse_format(with).unwrap(),
                parse_format(without).unwrap(),
                "{with} must parse as {without}"
            );
        }
    }

    // The MEASURED reject set. Upstream:
    // `GrpcStatusFormatter only supports CAMEL_STRING, SNAKE_STRING or NUMBER.`
    // Case-sensitive, no whitespace tolerance, no multi-argument form.
    #[test]
    fn grpc_status_rejects_every_measured_bad_argument() {
        for bad in [
            "%GRPC_STATUS(camel_string)%",
            "%GRPC_STATUS(Camel_String)%",
            "%GRPC_STATUS(number)%",
            "%GRPC_STATUS( CAMEL_STRING )%",
            "%GRPC_STATUS(CAMEL_STRING )%",
            "%GRPC_STATUS( CAMEL_STRING)%",
            "%GRPC_STATUS(  )%",
            "%GRPC_STATUS(FOO)%",
            "%GRPC_STATUS(NUMBER,CAMEL_STRING)%",
        ] {
            assert!(
                matches!(
                    parse_format(bad).unwrap_err(),
                    FormatParseError::MalformedArgument { .. }
                ),
                "{bad} must be rejected"
            );
        }
    }

    // A trailing `:N` length is separately fatal upstream
    // (`GRPC_STATUS does not allow length to be specified.`), and
    // `%GRPC_STATUS_NUMBER(NUMBER)%` is fatal as
    // `GRPC_STATUS_NUMBER does not take any parameters or length`.
    #[test]
    fn grpc_status_rejects_length_suffix_and_argument_on_the_alias() {
        assert!(matches!(
            parse_format("%GRPC_STATUS(CAMEL_STRING):5%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
        assert!(matches!(
            parse_format("%GRPC_STATUS_NUMBER(NUMBER)%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
        // The `()` tolerance must NOT have widened this: a NON-empty argument
        // on a no-arg keyword is still rejected.
        assert!(matches!(
            parse_format("%RESPONSE_CODE(FOO)%").unwrap_err(),
            FormatParseError::MalformedArgument { .. }
        ));
    }

    // `%GRPC_STATUSX%` stays an UnknownKeyword — the new arm matches the
    // keyword EXACTLY, not by prefix.
    #[test]
    fn grpc_status_keyword_match_is_exact_not_prefix() {
        assert!(matches!(
            parse_format("%GRPC_STATUSX%").unwrap_err(),
            FormatParseError::UnknownKeyword(k) if k == "GRPC_STATUSX"
        ));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-accesslog --lib grpc_status`
Expected: FAIL to COMPILE — `no variant named 'GrpcStatus' found for enum 'Op'`.

- [ ] **Step 3: Write the implementation**

**(a)** Add the variant as the LAST variant of `enum Op`:

```rust
    /// `%GRPC_STATUS%` / `%GRPC_STATUS(CAMEL_STRING|SNAKE_STRING|NUMBER)%` /
    /// `%GRPC_STATUS_NUMBER%` (phase 113). Backed by
    /// [`crate::record::AccessLogRecord::grpc_status`], which the HCM populates
    /// only when the REQUEST was gRPC. `%GRPC_STATUS_NUMBER%` is an ALIAS for
    /// `%GRPC_STATUS(NUMBER)%` — MEASURED byte-identical upstream in both the
    /// text and the JSON format — so both spellings parse to this one variant.
    GrpcStatus { format: GrpcStatusFormat },
```

**(b)** In `parse_operator`'s `match keyword`, add the `GRPC_STATUS` arm after the `DYNAMIC_METADATA` arm, and relax the no-arg guard:

```rust
        "DYNAMIC_METADATA" => parse_dynamic_metadata_op(rest),
        "GRPC_STATUS" => parse_grpc_status_op(rest),
        other => match no_arg_op(other) {
            // Non-arg keywords: must NOT carry parens — EXCEPT an EMPTY `()`,
            // which upstream accepts on every no-arg operator (MEASURED at the
            // phase-113 PLAN-write: `%RESPONSE_CODE()%` and `%PROTOCOL()%` both
            // load OK on envoyproxy/envoy:v1.33.0, while `%RESPONSE_CODE(FOO)%`
            // is rejected with `does not take any parameters or length`).
            Some(op) => {
                if rest.is_some_and(|r| r != "()") {
                    return Err(FormatParseError::MalformedArgument {
                        keyword: other.to_string(),
                        detail: "this operator takes no '(...)' argument".to_string(),
                    });
                }
                Ok(op)
            }
```

**(c)** Add the alias to `no_arg_op`, before the `_ => return None,` arm:

```rust
        // Phase 113: an ALIAS for `%GRPC_STATUS(NUMBER)%` (MEASURED identical
        // upstream in both the text and the JSON format), so it constructs the
        // same `Op` rather than a second variant.
        "GRPC_STATUS_NUMBER" => Op::GrpcStatus {
            format: GrpcStatusFormat::Number,
        },
```

**(d)** Add the parse helper immediately after `no_arg_op`:

```rust
/// Parse `%GRPC_STATUS%`, `%GRPC_STATUS()%` and
/// `%GRPC_STATUS(CAMEL_STRING|SNAKE_STRING|NUMBER)%`.
///
/// This is the engine's FIRST operator with an OPTIONAL parenthesized argument:
/// `REQ`/`RESP`/`DYNAMIC_METADATA` all REQUIRE one and every `no_arg_op`
/// keyword FORBIDS one. MEASURED reject set (upstream
/// `GrpcStatusFormatter only supports CAMEL_STRING, SNAKE_STRING or NUMBER.`):
/// lower/mixed case, any surrounding whitespace, a comma-separated pair, and
/// any unknown spelling. A trailing `:N` length is separately fatal upstream
/// (`GRPC_STATUS does not allow length to be specified.`).
fn parse_grpc_status_op(rest: Option<&str>) -> Result<Op, FormatParseError> {
    let Some(rest) = rest else {
        return Ok(Op::GrpcStatus {
            format: GrpcStatusFormat::CamelString,
        });
    };
    debug_assert!(rest.starts_with('('));

    let close = rest
        .find(')')
        .ok_or_else(|| FormatParseError::MalformedArgument {
            keyword: "GRPC_STATUS".to_string(),
            detail: "missing closing ')' on the format argument".to_string(),
        })?;
    if !rest[close + 1..].is_empty() {
        return Err(FormatParseError::MalformedArgument {
            keyword: "GRPC_STATUS".to_string(),
            detail: "does not allow a ':N' length to be specified".to_string(),
        });
    }

    let format = match &rest[1..close] {
        "" | "CAMEL_STRING" => GrpcStatusFormat::CamelString,
        "SNAKE_STRING" => GrpcStatusFormat::SnakeString,
        "NUMBER" => GrpcStatusFormat::Number,
        other => {
            return Err(FormatParseError::MalformedArgument {
                keyword: "GRPC_STATUS".to_string(),
                detail: format!(
                    "only supports CAMEL_STRING, SNAKE_STRING or NUMBER (got '{other}')"
                ),
            });
        }
    };
    Ok(Op::GrpcStatus { format })
}
```

**(e)** Add the two exhaustive-`match` arms so the tree compiles. Their tests come in Tasks 4 and 5.

In `render_op` (`command_operator.rs`), before the `Op::Req { .. }` arm:

```rust
        Op::GrpcStatus { format } => match record.grpc_status.as_deref() {
            Some(raw) => out.push_str(&render_grpc_status(raw, *format)),
            None => out.push_str(empty_or_dash),
        },
```

In `encode_single_op` (`json_format.rs`), before the `Op::Req { .. }` arm — plus the `number_opt` helper immediately above `fn quote_opt`:

```rust
/// Emit an OPTIONAL numeric value: the unquoted number when present, `null`
/// when absent. `quote_opt` cannot express this — `%GRPC_STATUS(NUMBER)%` is
/// the engine's FIRST operator that is both numeric-typed AND `Option`-backed
/// (every prior numeric operator reads a non-`Option` field and is always
/// present). MEASURED upstream: `{"gs_num_op":5}` on a gRPC request, and
/// `{"gs_num_op":null}` when the gate is closed.
fn number_opt(out: &mut String, v: Option<i64>) {
    match v {
        Some(n) => {
            let _ = write!(out, "{n}");
        }
        None => out.push_str("null"),
    }
}
```

```rust
        // Phase 113. MEASURED upstream: the two string formats are quoted
        // strings even when the value falls back to the number (`"99"`) or to
        // the unparseable sentinel (`"-1"`); the NUMBER format is an UNQUOTED
        // number in exactly those same cells (`99`, `-1`). Both are `null` when
        // the gate is closed.
        Op::GrpcStatus { format } => match format {
            crate::command_operator::GrpcStatusFormat::Number => number_opt(
                out,
                r.grpc_status
                    .as_deref()
                    .map(crate::command_operator::grpc_status_code),
            ),
            _ => quote_opt(
                out,
                r.grpc_status
                    .as_deref()
                    .map(|raw| crate::command_operator::render_grpc_status(raw, *format))
                    .as_deref(),
            ),
        },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib grpc_status`
Expected: PASS — Task 1's 4 tests plus these 6.
Run: `cargo test -p envoy-accesslog --lib`
Expected: PASS, no regression in the 100+ pre-existing engine tests. In particular the pre-existing `%DYNAMIC_METADATA%` and `:N` truncation tests must stay green — the `()` relaxation is scoped to the `no_arg_op` branch and must not reach `REQ`/`RESP`/`DYNAMIC_METADATA`.
Then: `cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all -- --check`

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/command_operator.rs crates/envoy-accesslog/src/json_format.rs
git commit -m "phase 113 task 3: parse %GRPC_STATUS% family; accept empty () on no-arg operators (measured divergence)"
```

---

## Task 4: The text-format render arm — tests

The arm itself landed in Task 3(e) because the compiler forced it. This task pins its behaviour.

**Files:**
- Test: `crates/envoy-accesslog/src/command_operator.rs`, `mod tests`

**Interfaces:**
- Consumes: `Op::GrpcStatus`, `render_grpc_status`, `AccessLogRecord.grpc_status`.
- Produces: nothing new.

- [ ] **Step 1: Write the failing tests**

```rust
    fn rec_grpc(v: Option<&str>) -> AccessLogRecord {
        let mut r = AccessLogRecord::test_baseline();
        r.grpc_status = v.map(str::to_owned);
        r
    }

    fn render1(fmt: &str, r: &AccessLogRecord) -> String {
        let segs = parse_format(fmt).expect("parses");
        CompiledFormat::new(segs).render(r)
    }

    // End-to-end through the compiled format: the three spellings on a present
    // value.
    #[test]
    fn grpc_status_text_renders_all_three_spellings() {
        let r = rec_grpc(Some("5"));
        assert_eq!(render1("%GRPC_STATUS%", &r), "NotFound");
        assert_eq!(render1("%GRPC_STATUS(CAMEL_STRING)%", &r), "NotFound");
        assert_eq!(render1("%GRPC_STATUS(SNAKE_STRING)%", &r), "NOT_FOUND");
        assert_eq!(render1("%GRPC_STATUS(NUMBER)%", &r), "5");
        assert_eq!(render1("%GRPC_STATUS_NUMBER%", &r), "5");
    }

    // THE GATE'S OBSERVABLE: an absent `grpc_status` renders the `-` sentinel,
    // which is how a non-gRPC request logs. This is the cell four of fixture
    // 0093's twelve probes exercise.
    #[test]
    fn grpc_status_absent_renders_the_dash_sentinel() {
        let r = rec_grpc(None);
        assert_eq!(render1("%GRPC_STATUS%", &r), "-");
        assert_eq!(render1("%GRPC_STATUS(SNAKE_STRING)%", &r), "-");
        assert_eq!(render1("%GRPC_STATUS_NUMBER%", &r), "-");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Temporarily break the arm to prove the tests bite — change `None => out.push_str(empty_or_dash),` to `None => out.push_str("?"),`, then:
Run: `cargo test -p envoy-accesslog --lib grpc_status_absent_renders`
Expected: FAIL with `assertion `left == right` failed: left: "?", right: "-"`.
**Then revert that edit before Step 3.**

- [ ] **Step 3: Write the implementation**

Already landed in Task 3(e). No new implementation code. Verify the arm reads exactly:

```rust
        Op::GrpcStatus { format } => match record.grpc_status.as_deref() {
            Some(raw) => out.push_str(&render_grpc_status(raw, *format)),
            None => out.push_str(empty_or_dash),
        },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib grpc_status`
Expected: PASS, 12 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/command_operator.rs
git commit -m "phase 113 task 4: pin the %GRPC_STATUS% text-format rendering"
```

---

## Task 5: The JSON typed carve-out — tests

`%GRPC_STATUS(NUMBER)%` is the engine's FIRST operator that is both numeric-typed AND `Option`-backed, so it is the first to need `number_opt`. Every expected value below was MEASURED from upstream's own `json_format` output.

**Files:**
- Test: `crates/envoy-accesslog/src/json_format.rs`, `mod tests`

**Interfaces:**
- Consumes: `number_opt`, `Op::GrpcStatus`, `grpc_status_code`, `render_grpc_status`.
- Produces: nothing new.

- [ ] **Step 1: Write the failing tests**

```rust
    fn rec_gs(v: Option<&str>) -> AccessLogRecord {
        let mut r = rec();
        r.grpc_status = v.map(str::to_owned);
        r
    }

    // The two STRING formats are quoted; the NUMBER format is an UNQUOTED
    // number.
    #[test]
    fn grpc_status_json_typing_present() {
        let r = rec_gs(Some("5"));
        assert_eq!(enc("%GRPC_STATUS%", &r), "\"NotFound\"");
        assert_eq!(enc("%GRPC_STATUS(SNAKE_STRING)%", &r), "\"NOT_FOUND\"");
        assert_eq!(enc("%GRPC_STATUS(NUMBER)%", &r), "5");
        assert_eq!(enc("%GRPC_STATUS_NUMBER%", &r), "5");
    }

    // The fallbacks keep their TYPE: an out-of-enum numeric is the QUOTED
    // string `"99"` under CAMEL_STRING but the UNQUOTED number `99` under
    // NUMBER; the unparseable sentinel behaves the same way with `-1`.
    #[test]
    fn grpc_status_json_typing_fallbacks_keep_their_type() {
        let r = rec_gs(Some("99"));
        assert_eq!(enc("%GRPC_STATUS%", &r), "\"99\"");
        assert_eq!(enc("%GRPC_STATUS_NUMBER%", &r), "99");
        let r = rec_gs(Some("notanumber"));
        assert_eq!(enc("%GRPC_STATUS%", &r), "\"-1\"");
        assert_eq!(enc("%GRPC_STATUS_NUMBER%", &r), "-1");
    }

    // Gate closed → `null` in EVERY format, string and numeric alike.
    #[test]
    fn grpc_status_json_absent_is_null_in_every_format() {
        let r = rec_gs(None);
        assert_eq!(enc("%GRPC_STATUS%", &r), "null");
        assert_eq!(enc("%GRPC_STATUS(SNAKE_STRING)%", &r), "null");
        assert_eq!(enc("%GRPC_STATUS(NUMBER)%", &r), "null");
        assert_eq!(enc("%GRPC_STATUS_NUMBER%", &r), "null");
    }

    // A MULTI-SEGMENT leaf LEAVES the typed carve-out and becomes a quoted
    // string, with the absent value rendering the `-` sentinel INSIDE the
    // quotes. MEASURED: `{"mixed_num":"x5"}` and, gate-closed, `{"mixed_num":"x-"}`.
    #[test]
    fn grpc_status_json_multi_segment_leaves_the_carve_out() {
        assert_eq!(enc("x%GRPC_STATUS_NUMBER%", &rec_gs(Some("5"))), "\"x5\"");
        assert_eq!(enc("x%GRPC_STATUS%", &rec_gs(Some("5"))), "\"xNotFound\"");
        assert_eq!(enc("x%GRPC_STATUS_NUMBER%", &rec_gs(None)), "\"x-\"");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Temporarily change the `Number` branch of the `encode_single_op` arm to call `quote_opt` with the rendered string instead of `number_opt`, then:
Run: `cargo test -p envoy-accesslog --lib grpc_status_json_typing_present`
Expected: FAIL — `left: "\"5\"", right: "5"` (quoted where an unquoted number is required).
**Then revert that edit before Step 3.**

- [ ] **Step 3: Write the implementation**

Already landed in Task 3(e). No new implementation code.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-accesslog --lib grpc_status_json`
Expected: PASS, 4 tests.
Run: `cargo test -p envoy-accesslog --lib`
Expected: PASS, all green.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-accesslog/src/json_format.rs
git commit -m "phase 113 task 5: pin %GRPC_STATUS% under the JSON single-operator typed carve-out"
```

---

## Task 6: The HTTP/1.1 population site and the in-process gate pins

**This is the task that carries the phase's only behavioural risk, and its tests are the ONLY witness of the request-side gate** — fixture `0093` cannot express it (see SPEC correction 4).

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — `build_access_log_record`
- Test: `crates/envoy-http1/src/hcm.rs` — a new `#[cfg(test)] mod grpc_status_access_log_tests`

**Interfaces:**
- Consumes: `crate::grpc::is_grpc_request` (already `pub(crate)` and already in this crate — **no visibility change is needed or permitted**, PV-3); `access_log_header_value`; `crate::headers::GRPC_STATUS`.
- Produces: a populated `AccessLogRecord.grpc_status` on the H1 path.

**Why this needs no new plumbing.** `apply_grpc_local_reply(&mut outgoing, &req.headers)` runs at the single H1 local-reply site; `outgoing` is then READ-ONLY until `build_access_log_record` is called in the same function; and the record builder already receives `req: &Request` and `headers: &outgoing.headers`. Both inputs are live and correctly ordered. **Re-derive those two line numbers before editing** — any phase that touches `hcm.rs` moves them; locate by the text `crate::grpc::apply_grpc_local_reply(` and `let record = build_access_log_record(`, each of which occurs exactly once.

- [ ] **Step 1: Write the failing tests**

Append to `crates/envoy-http1/src/hcm.rs`:

```rust
// ── Phase 113: the %GRPC_STATUS% population gate at the H1 record build ─────
// These are IN-PROCESS backstops. `gate_stays_shut_on_the_four_measured_negative_spellings`
// is the ONLY witness of the request-side gate anywhere in the tree: fixture
// 0093 cannot express it, because on the local-reply surface the sole producer
// of a response `grpc-status` header is the phase-110 transform, which is gated
// on the SAME predicate. Verified by mutation at the PLAN-write: deleting the
// gate leaves fixture 0093 GREEN and turns this module RED.
#[cfg(test)]
mod grpc_status_access_log_tests {
    use super::*;

    fn req_with_content_type(ct: Option<&str>) -> Request {
        Request {
            method: "POST".to_string(),
            path: "/g".to_string(),
            version: HttpVersion::Http11,
            headers: match ct {
                Some(v) => vec![(headers::CONTENT_TYPE.to_string(), v.to_string())],
                None => vec![],
            },
            bytes_consumed: 0,
            body: None,
        }
    }

    fn grpc_status_for(ct: Option<&str>, resp_headers: &[(String, String)]) -> Option<String> {
        let req = req_with_content_type(ct);
        let dm = std::collections::BTreeMap::new();
        let record = build_access_log_record(
            AccessLogRequestInfo {
                req: &req,
                start_time: std::time::UNIX_EPOCH,
                bytes_received: 0,
                matched_route: None,
                dynamic_metadata: &dm,
            },
            AccessLogResponseInfo {
                status: 200,
                bytes_sent: 0,
                duration: std::time::Duration::from_millis(0),
                headers: resp_headers,
                upstream_host: None,
                upstream_cluster: None,
                response_code_details: None,
                retry_limit_exceeded: false,
                connect_failure: false,
            },
        );
        record.grpc_status
    }

    fn gs_header(v: &str) -> Vec<(String, String)> {
        vec![(headers::GRPC_STATUS.to_string(), v.to_string())]
    }

    // The gate OPENS on the two content-types `is_grpc_request` accepts.
    #[test]
    fn gate_opens_on_grpc_content_types() {
        for ct in ["application/grpc", "application/grpc+proto"] {
            assert_eq!(
                grpc_status_for(Some(ct), &gs_header("5")),
                Some("5".to_string()),
                "content-type {ct} must open the gate"
            );
        }
    }

    // The gate STAYS SHUT on the four measured negative spellings, EVEN THOUGH
    // the response carries `grpc-status`. An implementation that ignored the
    // gate would return `Some("5")` on all four.
    #[test]
    fn gate_stays_shut_on_the_four_measured_negative_spellings() {
        for ct in [
            Some("application/grpc; charset=utf-8"),
            Some("application/grpc-web"),
            Some("APPLICATION/GRPC"),
            None,
        ] {
            assert_eq!(
                grpc_status_for(ct, &gs_header("5")),
                None,
                "content-type {ct:?} must NOT open the gate"
            );
        }
    }

    // The gate can be open and the header still absent — the record then holds
    // `None`, which renders the `-` sentinel. (SPEC §2.4: unreachable on the
    // local-reply surface, but the path must not panic or invent a value.)
    #[test]
    fn open_gate_with_no_header_is_none() {
        assert_eq!(grpc_status_for(Some("application/grpc"), &[]), None);
    }

    // The RAW wire value is stored verbatim — parsing happens at RENDER time,
    // which is what lets the renderer reproduce upstream's `99` and `-1` cells.
    #[test]
    fn raw_wire_value_is_stored_verbatim() {
        for raw in ["0", "99", "notanumber", " 5 "] {
            assert_eq!(
                grpc_status_for(Some("application/grpc"), &gs_header(raw)),
                Some(raw.to_string()),
                "raw {raw:?} must be stored unparsed"
            );
        }
    }

    // The header NAME lookup is CASE-INSENSITIVE (HTTP/1.1 §3.2), even though
    // the content-type gate is case-SENSITIVE on the VALUE.
    #[test]
    fn header_name_lookup_is_case_insensitive() {
        let hs = vec![("GRPC-Status".to_string(), "7".to_string())];
        assert_eq!(
            grpc_status_for(Some("application/grpc"), &hs),
            Some("7".to_string())
        );
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p envoy-http1 --lib grpc_status_access_log`
Expected: FAIL — 5 failures, every `grpc_status_for(...)` returning `None` because Task 2 left the production site pinned to `None`.

- [ ] **Step 3: Write the implementation**

In `build_access_log_record`, replace the placeholder `grpc_status: None,` from Task 2 with:

```rust
        // phase 113: %GRPC_STATUS%. GATED on the REQUEST being a gRPC request
        // — MEASURED upstream, which renders `-` even when the response
        // carries `grpc-status`, unless the request content-type is gRPC. The
        // gate is the SAME predicate the phase-110 local-reply transform uses,
        // so the two can never disagree. `response.headers` is the
        // POST-transform header vec (`apply_grpc_local_reply` runs at the
        // single call site before this build and `outgoing` is read-only
        // between), so the transform's own `grpc-status` is visible here.
        grpc_status: if crate::grpc::is_grpc_request(&request.req.headers) {
            access_log_header_value(response.headers, crate::headers::GRPC_STATUS)
        } else {
            None
        },
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http1 --lib grpc_status_access_log`
Expected: PASS, 5 tests.
Run: `cargo test -p envoy-http1 --lib`
Expected: PASS, 238 tests on the prototype.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 113 task 6: populate grpc_status at the H1 record build, gated on is_grpc_request"
```

---

## Task 7: The HTTP/2 boundary — a named helper, not an inline `None`

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs`
- Test: same file, a new `#[cfg(test)] mod h2_grpc_status_boundary_tests`

**Interfaces:**
- Consumes: nothing.
- Produces: `fn h2_grpc_status() -> Option<String>` — always `None`.

**Why a named helper.** A bare `grpc_status: None,` in a struct literal can be changed silently. Routing it through a tested function means a later phase that lifts `CF-113-2` must delete a test deliberately. ⚠ Do NOT pin this with a source-text assertion (`include_str!` + `matches().count()`); that form is brittle against unrelated edits to the same file.

- [ ] **Step 1: Write the failing test**

```rust
// ── Phase 113 boundary pin: H2 has NO %GRPC_STATUS% (CF-113-2) ─────────────
#[cfg(test)]
mod h2_grpc_status_boundary_tests {
    // Pins the CF-113-2 boundary. A phase that lifts it must DELETE this test
    // deliberately rather than change H2 behaviour silently (the ADR-0049
    // silent-divergence class).
    #[test]
    fn h2_grpc_status_is_absent() {
        assert_eq!(super::h2_grpc_status(), None);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p envoy-http2 --lib h2_grpc_status`
Expected: FAIL to COMPILE — `cannot find function 'h2_grpc_status' in module 'super'`.

- [ ] **Step 3: Write the implementation**

Add immediately above `finalize_h2_stream`'s `#[allow(clippy::too_many_arguments)]`:

```rust
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
```

Then change Task 2's placeholder in the `AccessLogRecord { … }` literal from `grpc_status: None,` to `grpc_status: h2_grpc_status(),`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p envoy-http2 --lib`
Expected: PASS, 125 tests + 1 ignored on the prototype.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 113 task 7: pin the H2 %GRPC_STATUS% boundary behind a named helper (CF-113-2)"
```

---

## Task 8: Fuzz corpus seeds for the EXISTING `accesslog_format_parse` target

§7.4's "parser, codec, or filter" trigger is satisfied by the pre-existing target, which already covers the format-string parser this phase extends. **No new fuzz target** — adding one would need a `ci.yml` step, and §5 non-goal 5 forbids touching `ci.yml`.

**Files:**
- Create: `crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/grpc_status.txt`, `grpc_status_formats.txt`, `grpc_status_malformed.txt`
- Modify: `crates/envoy-accesslog/fuzz/.gitignore`

⚠ **The corpus directory is gitignored by a `corpus/accesslog_format_parse/*` rule with a per-file `!` allow-list.** A new seed is INVISIBLE to git until its own `!` line is added. Verify with `git ls-files`, not `ls` — this was hit at the PLAN-write, where the first size measurement silently omitted all three seeds.

- [ ] **Step 1: Write the failing check**

Run: `git check-ignore -q crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/grpc_status.txt && echo IGNORED`
Expected (before the fix): `IGNORED`. ⚠ Use the PLAIN form — `git check-ignore -v` also reports negation rules and its exit code does not answer "is it ignored?".

- [ ] **Step 2: Create the seeds**

```bash
S=crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse
printf '%%GRPC_STATUS%%\n' > $S/grpc_status.txt
printf '%%GRPC_STATUS(SNAKE_STRING)%% %%GRPC_STATUS(NUMBER)%% %%GRPC_STATUS_NUMBER%%\n' > $S/grpc_status_formats.txt
printf '%%GRPC_STATUS(%% %%GRPC_STATUS(FOO)%% %%GRPC_STATUS()%% %%GRPC_STATUS(CAMEL_STRING):5%%\n' > $S/grpc_status_malformed.txt
```

- [ ] **Step 3: Add the three `!` negations**

Append to `crates/envoy-accesslog/fuzz/.gitignore`, after the `dynamic_metadata.txt` line:

```
!corpus/accesslog_format_parse/grpc_status.txt
!corpus/accesslog_format_parse/grpc_status_formats.txt
!corpus/accesslog_format_parse/grpc_status_malformed.txt
```

- [ ] **Step 4: Verify they are now tracked**

Run: `git add -A && git ls-files crates/envoy-accesslog/fuzz/corpus/accesslog_format_parse/ | wc -l`
Expected: `11` (the 8 pre-existing seeds plus these 3).

- [ ] **Step 5: Commit**

```bash
git commit -m "phase 113 task 8: fuzz corpus seeds for the %GRPC_STATUS% keywords"
```

---

## Task 9: Differential fixture `0093-accesslog-grpc-status`

**Files:**
- Create: `tests/fixtures/0093-accesslog-grpc-status/envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`
- Create: `tests/differential/tests/accesslog_grpc_status.rs`

**Interfaces:**
- Consumes: the whole implementation, Tasks 1-7.
- Produces: the phase's differential witness.

**Template:** copy the shape of `tests/fixtures/0084-headermatcher-absence-accesslog/`. The two YAMLs differ in exactly four ways: the upstream side has an `admin:` line and `generate_request_id: false`, binds `0.0.0.0` vs `127.0.0.1`, and writes to `/tmp/0093-envoy-mount/access.log` vs `/tmp/0093-envoy-rust-mount/access.log`. `{{PORT}}` is the only token; `clusters: []`; no backend spawns.

**Format string (identical on both sides):**

```
GS=%GRPC_STATUS% SNAKE=%GRPC_STATUS(SNAKE_STRING)% NUM=%GRPC_STATUS_NUMBER% CODE=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n
```

**Routes** — twelve `path:` matches plus a `prefix: "/"` catch-all, every one a `direct_response` with `body: { inline_string: "hi\n" }` (⚠ `direct_response.body` is MANDATORY in envoy-rust and optional upstream — CF-110-7 — so it must be present):

| path | status | path | status |
|---|---|---|---|
| `/g-ok` | 200 | `/g-default` | 501 |
| `/g-internal` | 400 | `/g-proto` | 404 |
| `/g-unauth` | 401 | `/g-param` | 404 |
| `/g-denied` | 403 | `/g-web` | 404 |
| `/g-unimpl` | 404 | `/g-upper` | 404 |
| `/g-unavail` | 503 | `/g-plain` | 404 |

**Probes** — twelve, each `method: get`, `host: envoy-rust.test`, `expect_logged: true`, with `extra_headers: [["content-type", …]]` except probe 12 which sends none:

| # | path | `content-type` | `expected_status` |
|---|---|---|---|
| 1 | `/g-ok` | `application/grpc` | 200 |
| 2 | `/g-internal` | `application/grpc` | 200 |
| 3 | `/g-unauth` | `application/grpc` | 200 |
| 4 | `/g-denied` | `application/grpc` | 200 |
| 5 | `/g-unimpl` | `application/grpc` | 200 |
| 6 | `/g-unavail` | `application/grpc` | 200 |
| 7 | `/g-default` | `application/grpc` | 200 |
| 8 | `/g-proto` | `application/grpc+proto` | 200 |
| 9 | `/g-param` | `application/grpc; charset=utf-8` | **404** |
| 10 | `/g-web` | `application/grpc-web` | **404** |
| 11 | `/g-upper` | `APPLICATION/GRPC` | **404** |
| 12 | `/g-plain` | *(none)* | **404** |

⚠ **Set every `expected_status` explicitly.** Probes 1-8 are 200 because the transform rewrites the status; probes 9-12 are the underlying 404 because it does not fire. Relying on the `200` default would make probes 9-12 RED.

⚠ **Do NOT use route-level `response_headers_to_add`** to set `grpc-status` directly. It works upstream and is BOOT-FATAL in envoy-rust (`Route` has no such field, `deny_unknown_fields` is on), so the fixture would go RED for a reason unrelated to gRPC.

⚠ **Do NOT echo `content-type` in the format string.** `%REQ(NAME)%` is allow-list gated by the seven-name `REQ_ALLOW_LIST` and `%REQ(CONTENT-TYPE)%` is BOOT-FATAL. `:path` IS on the list, which is why the probes are attributed by path.

**The twelve expected log lines, MEASURED byte-identical on both proxies:**

```
GS=Unknown SNAKE=UNKNOWN NUM=2 CODE=200 PATH=/g-ok
GS=Internal SNAKE=INTERNAL NUM=13 CODE=200 PATH=/g-internal
GS=Unauthenticated SNAKE=UNAUTHENTICATED NUM=16 CODE=200 PATH=/g-unauth
GS=PermissionDenied SNAKE=PERMISSION_DENIED NUM=7 CODE=200 PATH=/g-denied
GS=Unimplemented SNAKE=UNIMPLEMENTED NUM=12 CODE=200 PATH=/g-unimpl
GS=Unavailable SNAKE=UNAVAILABLE NUM=14 CODE=200 PATH=/g-unavail
GS=Unknown SNAKE=UNKNOWN NUM=2 CODE=200 PATH=/g-default
GS=Unimplemented SNAKE=UNIMPLEMENTED NUM=12 CODE=200 PATH=/g-proto
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-param
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-web
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-upper
GS=- SNAKE=- NUM=- CODE=404 PATH=/g-plain
```

**The `README.md` must state what this fixture does NOT witness** (SPEC correction 4): that the request-side gate is NOT observable here, that deleting the gate leaves the fixture GREEN, and that the in-process test from Task 6 is the gate's only witness. A fixture whose README overstates its coverage is worse than one with no README.

- [ ] **Step 1: Write the failing runner**

Create `tests/differential/tests/accesslog_grpc_status.rs`:

```rust
//! Fixture 0093 — the `%GRPC_STATUS%` access-log command-operator family
//! (phase 113): `%GRPC_STATUS%`, `%GRPC_STATUS(SNAKE_STRING)%` and
//! `%GRPC_STATUS_NUMBER%` over the HTTP/1.1 local-reply surface.
//!
//! Cluster-free and backend-free (`clusters: []`, every route a
//! `direct_response`), so it is verifiable on a development host rather than
//! CI-only. Twelve probes: eight drive the phase-110 HTTP→gRPC map through the
//! access log — corroborating that landed table through a NEW observable — and
//! four cover the content-type spellings the transform rejects.
//!
//! ⚠ This fixture does NOT witness the operator's request-side gate; see the
//! fixture README and the in-process tests in `crates/envoy-http1/src/hcm.rs`.

use std::path::PathBuf;

#[tokio::test]
async fn accesslog_grpc_status() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0093-accesslog-grpc-status");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p differential --test accesslog_grpc_status`
Expected: FAIL — the fixture directory does not exist yet.

- [ ] **Step 3: Write the four fixture files**

Per the tables above.

- [ ] **Step 4: Run the fixture and PROVE it is not vacuous**

⚠⚠ **The differential harness spawns a PRE-BUILT `envoy-bin` process. `cargo test -p differential` does NOT rebuild it.** A mutation check without an explicit rebuild reads as a false GREEN — this was hit at the PLAN-write. Always:

```bash
cargo build -p envoy-bin && cargo test -p differential --test accesslog_grpc_status
```

Expected: PASS.

Then the non-vacuity check, with an unmutated control from the same tree:

```bash
# MUTATION: corrupt ONE entry of the canonical table. Assert the anchor is
# unique first — the same text also appears in the test's EXPECT table, and a
# sed that hits both fakes a GREEN.
grep -c '    ("Unauthenticated", "UNAUTHENTICATED"),' crates/envoy-accesslog/src/command_operator.rs   # must be 2
python3 - <<'PY'
p='crates/envoy-accesslog/src/command_operator.rs'
s=open(p).read()
a='    ("Unauthenticated", "UNAUTHENTICATED"),\n];'   # the const table only
assert s.count(a) == 1
open(p,'w').write(s.replace(a, '    ("Unauthenticatd", "UNAUTHENTICATED"),\n];'))
PY
cargo build -p envoy-bin && cargo test -p differential --test accesslog_grpc_status
```

Expected: **RED**, with the harness naming the upstream line
`envoy="GS=Unauthenticated SNAKE=UNAUTHENTICATED NUM=16 CODE=200 PATH=/g-unauth"` — which is also the positive control proving upstream Envoy really ran. Revert the mutation, `cargo build -p envoy-bin`, and confirm GREEN again.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0093-accesslog-grpc-status tests/differential/tests/accesslog_grpc_status.rs
git commit -m "phase 113 task 9: differential fixture 0093 — the %GRPC_STATUS% family, 12 probes"
```

---

## Task 10: `BEHAVIOR_CONTRACT.md` — extend the `## gRPC` section

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `## gRPC` section phase 110.2 created)

This is the only `docs/` change in the phase and is EXCLUDED from the §6.1 LoC gate.

- [ ] **Step 1: Locate the section**

Run: `grep -n '^## gRPC' docs/envoy-rust/BEHAVIOR_CONTRACT.md`
⚠ Assert it occurs exactly once before editing.

- [ ] **Step 2: Append the three sub-sections**

1. **The gate.** `%GRPC_STATUS%` renders the `-` sentinel (json `null`) unless the REQUEST is a gRPC request, even when the response carries `grpc-status`. The predicate is exactly the phase-110 one: `content-type` exactly `application/grpc` or beginning `application/grpc+`, case-sensitive, a parameter defeats it.
2. **The canonical name table**, all 17 codes in both spellings, with the two counter-intuitive cells called out (code 0 = `OK` in both; code 1 = `Canceled` / `CANCELLED`).
3. **The fallbacks and the typing.** Out-of-enum numeric → the number in every format. Unparseable → `-1` in every format. Surrounding whitespace stripped; `+5` and `05` parse to 5. In JSON, the two string formats are quoted (including the `"99"` / `"-1"` fallbacks), NUMBER is an unquoted number, and an absent value is `null` in every format. **Also record the empty-`()` rule** — accepted on every no-arg operator — since it changes the contract for eleven pre-existing operators.

- [ ] **Step 3: Verify no other section was disturbed**

Run: `git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md`
Expected: additions only, deletions `0`.

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 113 task 10: BEHAVIOR_CONTRACT gRPC section — the %GRPC_STATUS% gate, table and typing"
```

---

## Carry-forwards this phase opens or amends

- **CF-113-1 … CF-113-4** stand as `SPEC.md` §10 records them. `CF-111-4` is consumed only in PART.
- **CF-113-5 (NEW, opened by this PLAN-write).** The io_uring seam applies the phase-110 gRPC transform (`crates/envoy-http1/src/uring.rs`, the second and only other `apply_grpc_local_reply` call site) but has **no access-log dispatch at all**. `%GRPC_STATUS%` — and every other operator — is therefore unlogged on that path. This is pre-existing and far wider than this phase; the feature is off by default and requires `ENVOY_RUST_URING=1`.
- **CF-113-6 (NEW, opened by this PLAN-write).** The `%GRPC_STATUS%` request-side gate is **not witnessable by any differential fixture** on the local-reply surface, because the only producer of a response `grpc-status` header there shares the same gate. Witnessing it needs a PROXIED gRPC response, which is blocked behind `CF-111-2` / `CF-113-3`. Until then the in-process tests in Task 6 are the sole witness, and a REVIEW must not record fixture `0093` as covering the gate.

---

## Self-review

**Spec coverage.** `SPEC.md` §4's eleven deliverables map to tasks as: 1→T1, 2→T3, 3→T3, 4→T1, 5→T2, 6→T6, 7→T7, 8→T3/T5, 9→T9, 10→T8, 11→T10. Deliverable 2 is intentionally narrowed to ONE variant (SPEC correction 2) and deliverable 3's reject set is widened (SPEC correction 3); both are recorded in `ADR-0193`.

**Placeholder scan.** No `TBD`, no "handle edge cases", no "similar to Task N". Every code step carries the literal code. The two "already landed in Task 3(e)" steps in Tasks 4 and 5 quote the exact arm to verify rather than deferring it.

**Type consistency.** `GrpcStatusFormat` / `grpc_status_code` / `render_grpc_status` / `number_opt` / `h2_grpc_status` / `parse_grpc_status_op` / `AccessLogRecord.grpc_status` are spelled identically in every task that names them, and each is defined before it is consumed.

**Every code block in this plan was EXECUTED**, not written from reasoning. The blocks were inserted VERBATIM into a scratch tree, which then compiled, passed `clippy --workspace --all-targets --all-features -- -D warnings`, passed `fmt --all --check`, passed 492 unit tests across the three affected crates, and turned fixture `0093` green against both real proxies — with the non-vacuity mutation in Task 9 Step 4 verified RED and its unmutated control verified GREEN from the same tree.
