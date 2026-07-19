# Phase 71 — `response_flag_filter` — PROGRESS (§5 state-3 implementation)

The state-3 implementation of `PLAN.md` (12 TDD tasks), executed
task-by-task (RED → GREEN → commit each; D-3.1). One state per session (§5.1):
this session lands all 12 code/docs tasks; the §7.5 verification (the full Docker
differential + conformance) is the SEPARATE state-4 session — Task 12 here is a
fold-in dry-run of the deterministic gates only, NOT the state-4 verdict.

**Cold-start confirmed:** `git status` clean, branch `main`, HEAD at the state-2
PLAN-write commit `2fad12dd09544a9a1405788f2743779e6db37c0c`; CI `success` on that
FULL 40-char SHA (run `29648672697`); no sibling had advanced (STATE.md still
state-2). Detection rule (§5.1): `PLAN.md` exists, `PROGRESS.md` did not → state 3.

**Plan-gap resolution (T1/T2 coupling, documented):** T1's acceptance test
`parses_response_flag_filter_nr` calls `parse_bootstrap`, which runs
`validate_access_logs`. The pre-existing `set_arms` array counted ONLY
`status_code_filter`, so a `response_flag_filter`-only config yielded
`set_arms == 0 → AmbiguousAccessLogFilter`. There is no way to make the new arm
ACCEPTED without also counting it — which is T2's compiler-forcing destructuring.
The two tasks are transitively coupled: T1 cannot be green without T2's validator
change. Resolution (keeps every commit green, TDD-honest): the destructuring
landed as part of T1 (it is what makes the arm functional); T2 then contributed
its dedicated both-arm-rejection test, verified NON-VACUOUS by a mutation check
(reverting to the 1-element array made the T2 test RED, with a forced
`Compiling envoy-config` rebuild confirmed). 12 commits, plan's end-state intact.

---

## Task 1 — `ResponseFlagFilter` schema + the `response_flag_filter` oneof arm — commit `e3208e4`

- RED: `cargo test -p envoy-config parses_response_flag_filter_nr` → compile error
  `no field response_flag_filter on type &AccessLogFilter`.
- Added `pub struct ResponseFlagFilter { pub flags: Vec<String> }` (after
  `RuntimeUInt32`), the `response_flag_filter: Option<ResponseFlagFilter>` arm on
  `AccessLogFilter` (doc updated to "two arms"), the `lib.rs` re-export, and (see
  the plan-gap note) the M70-R1 `set_arms` compiler-forcing 2-arm destructuring.
- GREEN: `cargo test -p envoy-config` → **616 passed; 0 failed** (targeted
  `parses_response_flag_filter_nr` + `rejects_access_log_filter_with_no_variant`
  both ok).

## Task 2 — both-arm rejection test (M70-R1 destructuring reachable) — commit `d7d386c`

- Added `rejects_access_log_filter_with_both_arms` (a `filter` carrying BOTH arms
  → `AmbiguousAccessLogFilter`, ADR-0145 R-0.3). GREEN with the T1 destructuring.
- Mutation check (non-vacuity): reverted the array to `[status_code_filter.is_some()]`
  → `Compiling envoy-config` + `rejects_access_log_filter_with_both_arms ... FAILED`
  (both-arms wrongly accepted). Restored the destructuring → **1 passed**.

## Task 3 — `UnknownResponseFlag` fail-loud validator (29-token in-list) + empty/inert acceptance — commit `10f7282`

- RED: `cargo test -p envoy-config response_flag_filter` → `variant UnknownResponseFlag not found`.
- Added `RESPONSE_FLAG_TOKENS: [&str; 29]`, `ConfigError::UnknownResponseFlag { token }`,
  and the validation branch (each `flags` token must be in the 29-set).
  Acceptance test: `flags: []`, `response_flag_filter: {}`, `flags: ["DI"]` all validate.
- GREEN: `response_flag_filter` filter → **4 passed** (`rejects_..._unknown_token`,
  `rejects_..._lowercase_token`, `accepts_..._empty_and_inert`, `parses_..._nr`);
  full crate **616 passed; 0 failed**.

## Task 4 — `LogFilter::ResponseFlag` + widen `should_log(status, response_flags)` — commit `6a99fa5`

- RED: `cargo test -p envoy-accesslog filter::` → 23 errors (`should_log` takes 1
  arg; `LogFilter::ResponseFlag` missing).
- Added the `ResponseFlag { flags }` variant; widened `LogFilter::should_log` and
  `FileSink::should_log` to `(status: u16, response_flags: &str)`. Empty-`flags`
  branch = `response_flags != "-"` (MEASURED PV-6); non-empty = token membership.
  Fixed the in-crate `file_sink.rs` test callers. (Widening breaks the H1/H2 emit
  call sites — fixed in T5/T6; crate stays green.)
- GREEN: `cargo test -p envoy-accesslog` → **107 passed; 0 failed**.

## Task 5 — H1 compile 2-arm match (CF-70-1) + widened emit gate — commit `aae7211`

- RED: `cargo test -p envoy-http1 from_config_compiles_response_flag_filter_into_sink`
  → fails to compile (`expect()` on `status_code_filter`; 1-arg `should_log` gate).
- Converted `compile_access_log_filter` to `match (&status_code_filter, &response_flag_filter)`
  — `(Some,None)`/`(None,Some)` arms + `_ => unreachable!()` (CF-70-1, the zero-arm
  `expect()` gone). Threaded `should_log(record.response_code, &record.response_flags)`
  at the H1 emit gate. Added the `hcm_config_with_response_flag_access_log` builder;
  fixed all status-only H1 `should_log` call sites to pass `"-"`.
- GREEN: `from_config_compiles_response_flag_filter_into_sink` ok; full H1
  **176 passed; 0 failed**.

## Task 6 — H2 widened emit gate (inert-correct parity) — commit `f851aca`

- RED: `cargo test -p envoy-http2 h2_response_flag_filter_suppresses_no_flag`
  → `this method takes 2 arguments but 1 was supplied`.
- Threaded `should_log(record.response_code, &record.response_flags)` at the H2
  emit gate (`hcm.rs:1135`). Added an end-to-end test: `flags:[NR]` keeps a
  no-route 404 (rf=NR, 1 line), drops a clean 503 (rf=-, 0 lines); `access_logs_total`
  counts emitted only.
- GREEN: H2 test ok; full H2 **107 passed; 0 failed**; **`cargo build --workspace`
  Finished** (the widening is now fully threaded — workspace whole again).

## Task 7 — CF-70-3 ordering-witness hardening + M70-R2 helper witness — commit `9bf5a41`

- Added `expected_logged_count_counts_only_kept` (M70-R2 boundary: all-suppressed=0,
  lone-kept=1 — the cases the existing `expected_logged_count_excludes_suppressed`
  omits). GREEN immediately (helper exists).
- Added `CF70_3_SETTLE` const + a `has_suppression`-gated ordering-witness
  precondition (a suppression fixture's LAST probe must be `expect_logged=true`)
  and a bounded settle (`bail!` if either log grows past `expected_lines`) to BOTH
  `run_http1_access_log_byte_exact_arm` and `run_http2_access_log_byte_exact_arm`.
  No-op for the 30 all-kept fixtures.
- GREEN: `cargo build -p differential --tests` Finished; `expected_logged_count`
  witnesses **2 passed**.

## Task 8 — differential fixture `0077-accesslog-response-flag-filter` — commit `cab4992`

- Created `envoy.yaml` / `envoy-rust.yaml` / `expectations.yaml` / `README.md`
  (mirror `0076`): `flags:["NR"]`, one `direct_response /direct`→503, no-route
  `/nowhere`→404; ordering witness — dropped `/direct` FIRST, kept `/nowhere` LAST.
  Added `tests/differential/tests/access_log_response_flag_filter.rs`.
- `cargo build -p envoy-bin` (mandatory — harness runs `target/debug/envoy-bin`);
  config validated through envoy-bin (node registered; parses+validates).
- Docker differential: `cargo test -p differential --test access_log_response_flag_filter`
  → **1 passed** — single byte-identical `STATUS=404 PATH=/nowhere FLAGS=NR` line
  across upstream Envoy v1.33.0 and envoy-rust; `/direct` suppressed on both; the
  CF-70-3 `has_suppression` settle path executed. (Docker was UP.)

## Task 9 — in-process regressions under the widened `should_log` — commit `25a3a70`

- Added `no_filter_sink_logs_every_record_after_widening` (a filterless sink logs
  every record regardless of status/flags) and `status_code_filter_unchanged_under_widening`
  (the phase-70 GE-500 filter still gates purely on status, ignoring `response_flags`).
- GREEN: **2 passed** (reused the phase-70 `hcm_config_with_filtered_access_log`).

## Task 10 — `parse_bootstrap` corpus seed + un-ignore — commit `5df0438`

- Created `crates/envoy-config/fuzz/corpus/parse_bootstrap/response_flag_filter.yaml`
  (`node.id: fuzz-71`, `flags: ["NR","UF"]`); added the `!`-un-ignore line to
  `crates/envoy-config/fuzz/.gitignore`.
- `git ls-files …/response_flag_filter.yaml` → prints the path (TRACKED). No new
  fuzz target; `parse_bootstrap` is the sole config target → no `ci.yml` edit.

## Task 11 — `BEHAVIOR_CONTRACT.md` `response_flag_filter` subsection — commit `76bad75`

- Added the phase-71 subsection (sibling to the phase-70 `status_code_filter` one):
  the 29/6/23 token vocabulary, token-membership over the single `%RESPONSE_FLAGS%`
  token, empty/absent `flags` = match-any-flag-set (PV-6), mutual exclusion, and
  the authoritative fixture-0077 line.

## Task 12 — §7.5 verification dry-run (fold-in, NOT the state-4 verdict)

Deterministic gates (the Docker differential + conformance are the state-4 gate):
- `cargo fmt --all -- --check` → RED on the phase-71 additions → applied
  `cargo fmt --all`, committed the normalization (`e43cdb4`); re-check CLEAN.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → **clean** (exit 0).
- `cargo build --workspace --all-targets` → **Finished** (every test binary compiles).
- `cargo deny check` → **advisories ok, bans ok, licenses ok, sources ok** (exit 0).
- `cargo test --workspace --lib --bins --no-fail-fast` → **1862 passed; 0 failed**
  (22 unit-test binaries). Full Docker differential/conformance deferred to state-4.

No commit for Task 12 (dry-run only).

---

## Session close

All 12 PLAN tasks landed (commits `e3208e4`..`76bad75` + fmt `e43cdb4`). STATE
advanced to state-4 (docs-only). `#![forbid(unsafe_code)]` holds. Consumed
CF-70-1 + M70-R1, CLOSED CF-70-3 (driver ordering witness + fixture 0077 probe
order), FOLDED IN M70-R2. `next-prompt.txt` refreshed for the §5 state-4
verification. No `stop` file (the §9 feature families remain largely unbuilt).
