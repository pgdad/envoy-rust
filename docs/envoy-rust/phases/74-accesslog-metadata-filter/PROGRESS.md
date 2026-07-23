# Phase 74 — access-log `metadata_filter` — §5 state-3 implementation log

> **State-3 implementation** (`superpowers:executing-plans` + `test-driven-development`).
> This session implements `PLAN.md`'s 10 TDD tasks IN ORDER (the ordering is
> load-bearing — see the interim-state note below), each RED-first. It does NOT
> run the full §7.5 gate (that IS the §5 state-4 verification, a SEPARATE later
> session), but it DOES run each task's own named tests to green and (per the
> plan's T7/T8 steps) both new Docker differentials in isolation. No ADR fires
> (the §6.2 reconciliation ADR-0155 was the state-2 output; ADR-0156 stays
> UNFIRED — no split).

## Execution approach

The DAG is a strict linear chain: **T1→T6** each consume the previous task's
types and all touch overlapping files (`bootstrap.rs`, `filter.rs`, `hcm.rs`), so
they run SERIALLY in the main session. T7/T8/T9/T10 are genuinely independent of
one another once T6 has landed, but they are small (two fixture directories, one
seed file, one docs insert) and every test run serializes on the cargo lock, so
worktree-subagent coordination cost exceeds the win (the plan's own §5.1 guidance:
"fully linear DAG, or coordination cost > win → do not parallelize"). The main
session implements all ten directly, task-by-task.

**Interim-state note (load-bearing, the phase-73 T1/T4 precedent).** Between T1
and T6 a config setting `metadata_filter` parses and validates but falls into
`compile_access_log_filter`'s `_ => unreachable!()` and PANICS. That window closes
in T6, before any fixture (T7/T8) exercises it. The `unreachable!` is NOT weakened
to "fix" it early — it is the guard proving the validator and the compile match
stay in lockstep.

## STEP 0/0.5 confirmation

- `git status --porcelain` clean; branch `main`; `HEAD` = the phase-74 §5 state-2
  PLAN-write commit `53893b6730d1bf4d41611f2d1e36eb3ef8d870ad`;
  `git fetch origin --prune` showed `origin/main` at the same SHA — no sibling had
  advanced, `PROGRESS.md`/`REVIEW.md` absent.
- CI on `53893b67…` = `completed`/`success` (run `30024334930`), both jobs green
  with full step counts (15 and 13) — confirmed at the end of the state-2 session.

## Line-number drift at session start

`PLAN.md`'s quoted offsets were re-grepped before each edit (the plan's own
instruction). Drift found and used: `AccessLogFilter` struct `721→723`, `OrFilter`
`755→757`, the destructure `5254` (matched), `lib.rs` re-export line `30`
(matched), `ValueMatcher::compile_safe_regexes` `5539→5541`, `SafeRegex` struct
`2915→2832`, `MetadataMatcher` `1628→1634`. The 13 full `AccessLogFilter` literal
sites matched the plan exactly (10 in `envoy-http1/src/hcm.rs`, 3 in
`bootstrap.rs`).

## Per-task log (each RED-first → GREEN → commit)

### T1 — `MetadataFilter` + the `metadata_filter` oneof arm + 6-arm cardinality (folds N73-R1) — commit `3bb4e9e`

- **RED:** added `metadata_filter_deserialize_round_trip_and_defaults` +
  `six_arm_cardinality_counts_every_arm` to `bootstrap.rs`'s test module.
  `cargo test -p envoy-config --lib metadata_filter` → **9 compile errors**,
  exactly as the plan predicted:
  `error[E0425]: cannot find type 'MetadataFilter' in this scope` (×3),
  `error[E0433]: cannot find type 'MetadataFilter' in this scope` (×3),
  `error[E0609]/[E0560]: no field 'metadata_filter' on type 'AccessLogFilter'` (×3),
  `error: could not compile 'envoy-config' (lib test) due to 9 previous errors`.
- **GREEN:**
  - Added `pub metadata_filter: Option<MetadataFilter>` as the SIXTH
    `AccessLogFilter` arm.
  - Added `pub struct MetadataFilter { matcher: Option<MetadataMatcher>,
    match_if_key_not_found: Option<bool> }`
    (`#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]`
    `#[serde(default, deny_unknown_fields)]`, NO `Clone`), immediately after
    `OrFilter`, documenting the two MEASURED reasons both fields are `Option`
    (the R-0.2 matcher-less load-parity trap; the `google.protobuf.BoolValue`
    wrapper whose absent≠explicit-`false` distinction a bare `bool` would lose).
  - **N73-R1 FOLDED/CONSUMED:** the stale `AccessLogFilter` doc comment
    ("This type now models THREE oneof arms") rewritten to SIX, enumerating all
    six arms with their phases.
  - Re-exported `MetadataFilter` from `lib.rs` (sorted slot between
    `MetadataEntry` and `MetadataMatcher`).
  - Grew the `validate_access_log_filter` destructure (still NO `..`) and the
    `set_arms` array to 6, and updated the helper's doc from "all FIVE arms" to
    "all SIX arms". Per-arm leaf checks deliberately left for T2.
  - Fixed all **13 full `AccessLogFilter { … }` literals** so the tree compiles —
    3 in `bootstrap.rs` (the ambiguity test, `fn exact_header`, the nested-bad-leaf
    literal) and **10** in `envoy-http1/src/hcm.rs`. The 6 `..AccessLogFilter::default()`
    shorthand sites needed no change, as predicted.
- **Verification:** `cargo build -p envoy-config -p envoy-http1 --all-targets` →
  clean (no errors, no unused warnings).
  `cargo test -p envoy-config --lib metadata_filter` →
  `test bootstrap::tests::metadata_filter_deserialize_round_trip_and_defaults ... ok`,
  `3 passed; 0 failed`.
  `cargo test -p envoy-config --lib six_arm_cardinality` →
  `test bootstrap::tests::six_arm_cardinality_counts_every_arm ... ok`,
  `1 passed; 0 failed`.
  Regressions: `cargo test -p envoy-config --lib access_log` → `14 passed; 0 failed`;
  `cargo test -p envoy-http1 --lib access_log` → `22 passed; 0 failed`.

### T2 — access-log-scoped matcher validation + `AccessLogMetadataMatcherInvalid` — commit `867e17d`

- **RED:** added the `md_filter`/`md_matcher` helpers + 7 tests
  (`matcher_less_metadata_filter_is_accepted`,
  `metadata_filter_empty_namespace_is_rejected`,
  `metadata_filter_empty_path_is_rejected`,
  `metadata_filter_multi_segment_path_is_rejected`,
  `metadata_filter_empty_segment_key_is_rejected`,
  `metadata_filter_safe_regex_compiles_in_place_and_rejects_bad_pattern`,
  `metadata_filter_nested_in_or_filter_surfaces_through_recursion`).
  `cargo test -p envoy-config --lib metadata_filter_` →
  `error[E0599]: no variant named 'AccessLogMetadataMatcherInvalid' found for enum 'ConfigError'`
  ×5, `could not compile 'envoy-config' (lib test) due to 5 previous errors`.
- **GREEN:**
  - Added `ConfigError::AccessLogMetadataMatcherInvalid { detail: String }` in the
    slot the plan named (immediately after `UnknownResponseFlag`), documenting why
    it is distinct from the RBAC-scoped `RbacMetadataMatcherInvalid` (which
    structurally carries `listener`/`policy_name`, neither in scope here).
  - Added `fn validate_access_log_metadata_matcher(&mut MetadataMatcher)` —
    `filter` non-empty, `path.len() == 1`, segment `key` non-empty, then
    `value.compile_safe_regexes()` IN PLACE. **The empty-segment-`key` check is
    the ADR-0155 SPEC correction** — the RBAC validator omits it though upstream
    PGV enforces `min_len 1` (the RBAC gap stays open as CF-74-4).
  - Wired it into `validate_access_log_filter` as a **let-chain**
    (`if let Some(mf) = metadata_filter && let Some(mm) = mf.matcher.as_mut()`),
    matching the existing `status_code_filter` idiom — the plan's clippy pre-flight
    for `collapsible_if` held.
  - Updated the `validate_access_logs` doc comment: FIVE→SIX arms, plus a new
    item 7 describing the metadata-matcher checks and the matcher-less ACCEPT.
    The M70-R1 no-`..` rationale was preserved verbatim.
- **Verification:** `cargo test -p envoy-config --lib metadata_filter` →
  `10 passed; 0 failed` (8 phase-74 tests + 2 unrelated name matches).
  `cargo test -p envoy-config --lib matcher_less` → `1 passed` — the LOAD-PARITY
  pin, which per the handoff must NEVER go red, was green from the first run
  (it passes under T1's validator too, exactly as the plan predicted).
  Regressions: `cargo test -p envoy-config --lib nested_` → `8 passed`;
  `cargo test -p envoy-config --lib _filter` → `99 passed; 0 failed`.

### T3 — `should_log` 4th-parameter widening (behavior-neutral) — commit `796450d`

- **RED:** added `existing_arms_ignore_the_dynamic_metadata_argument` to
  `filter.rs`. `cargo test -p envoy-accesslog --lib …` →
  `error[E0061]: this method takes 3 arguments but 4 arguments were supplied` ×6.
- **GREEN:** widened `LogFilter::should_log` and `FileSink::should_log` with
  `dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>`; threaded it
  through the `And`/`Or` recursion; passed `&record.dynamic_metadata` at BOTH HCM
  emit gates (H1 `hcm.rs`, H2 `hcm.rs`) — the record is built BEFORE the per-sink
  loop on both codecs, as PV-5 verified.
- **Call-site fan-out:** driven by the compiler. Rather than hand-editing ~98
  sites, a balanced-paren script appended `&Default::default()` to exactly those
  `should_log(` calls with THREE top-level arguments (skipping the definition
  sites and the two already-widened production gates): **80 call sites patched** —
  `filter.rs` 36, `file_sink.rs` 4, `envoy-http1/hcm.rs` 38, `envoy-http2/hcm.rs` 2.
  `cargo build --workspace --all-targets` → **0 errors**.
- **DEVIATION from the plan (resolved per D-3.5, recorded here):** the plan's
  T3 Step 5 expects `cargo clippy … -D warnings` clean, but it REDs with
  `error: … only_used_in_recursion` on `dynamic_metadata` — a genuine and
  predictable consequence of the deliberate task split: between T3 and T4 the new
  parameter is threaded through the `And`/`Or` recursion but consumed by NO arm.
  Resolved with a scoped, self-documenting `#[allow(clippy::only_used_in_recursion)]`
  on `should_log` stating that T4 removes it — chosen over leaving a clippy-RED
  commit, so every commit in the series stays gate-clean. **T4 removed it** and
  clippy passes without it, confirming the allow was genuinely transient.
- **Verification:** `cargo test -p envoy-accesslog --lib` → `110 passed; 0 failed`;
  `cargo test -p envoy-http1 --lib access_log` → `22 passed`;
  `cargo test -p envoy-http2 --lib` → `108 passed; 0 failed; 1 ignored`;
  `cargo clippy -p envoy-accesslog -p envoy-http1 -p envoy-http2 --all-targets
  --all-features -- -D warnings` → clean (exit 0).

### T4 — `MetadataMatch` trait seam + `LogFilter::Metadata` + its `should_log` arm — commit `cd5a675`

- **RED:** added `metadata_arm_implements_the_measured_decision_rule` +
  `metadata_arm_composes_under_and_or` with the local `NsKeyEquals` stub (the
  crate cannot build a real `MetadataMatcher` — ADR-0150 cycle).
  `cargo test -p envoy-accesslog --lib metadata_arm` →
  `error[E0405]: cannot find trait 'MetadataMatch' in this scope`.
- **GREEN:** added `pub trait MetadataMatch: Debug + Send + Sync` returning
  **`Option<bool>`** (documenting why `bool` via `matches_resolved` was
  measured-rejected: it collapses "unresolved" into `false` and would drop every
  key-absent record); the `LogFilter::Metadata { matcher: Option<Arc<dyn
  MetadataMatch>>, match_if_key_not_found: bool }` variant; the `should_log` arm
  applying `None => *match_if_key_not_found, Some(m) => m.matches(..)
  .unwrap_or(*match_if_key_not_found)`; and the `MetadataMatch` re-export.
  **Removed T3's transient `#[allow]`.** No `Eq`/`PartialEq` added; no
  `envoy-config` dep (ADR-0150 holds).
- **Verification:** `cargo test -p envoy-accesslog --lib metadata_arm` →
  `2 passed`; full crate `112 passed; 0 failed`;
  `cargo clippy -p envoy-accesslog --all-targets --all-features -- -D warnings`
  → clean WITHOUT the allow. `cargo build --workspace --all-targets` → 0 errors
  (no exhaustive `LogFilter` match elsewhere broke on the new variant).

### T5 — `impl MetadataMatch for MetadataMatcher` — commit `096efd9`

- **RED:** added `mod metadata_match_tests` (3 tests) to `matcher.rs`.
  `cargo test -p envoy-config --lib metadata_match_tests` →
  `error[E0599]: no method named 'matches' found for struct 'MetadataMatcher'`
  ×6 (the trait in scope, unimplemented) — exactly the predicted RED.
- **GREEN:** appended the sole impl after the phase-72 `HeaderMatch` impl:
  `let key = &self.path.first()?.key; let resolved =
  dynamic_metadata.get(&self.filter)?.get(key)?; Some(self.value.matches(resolved))`
  — reusing `ValueMatcher::matches` VERBATIM and deliberately NOT
  `matches_resolved`. Added `MetadataMatcher` to the file's import list. As PV-4
  found, there is no inherent `impl MetadataMatcher`, so no delegation trick and
  no recursion hazard (unlike the `HeaderMatch` impl).
- **Verification:** `cargo test -p envoy-config --lib metadata_match_tests` →
  `3 passed; 0 failed`; `cargo clippy -p envoy-config --all-targets --all-features
  -- -D warnings` → clean.

### T6 — 6-tuple `compile_access_log_filter` + the wrapper default — commit `1792037`

- **RED (the load-bearing one):**
  `cargo test -p envoy-http1 --lib compile_access_log_filter_builds_metadata_arm_with_wrapper_default`
  → `panicked at crates/envoy-http1/src/hcm.rs:1792:14: internal error: entered
  unreachable code: validated by validate_access_logs: exactly one filter arm is
  set`, `test result: FAILED. 0 passed; 1 failed`. **This is the T1→T6 interim
  window the plan predicted, observed directly** — the 5-tuple match ignored
  `metadata_filter`, so a set arm fell through to `_`. The `unreachable!` was NOT
  weakened; T6 closes the window by construction.
- **PLAN DEFECT found and fixed (D-3.5, recorded):** the plan's own literal Rust
  for case (d) writes `match_if_key_not_found: false` inside an
  `envoy_config::MetadataFilter` literal, but that CONFIG-side field is
  `Option<bool>` (only the runtime `LogFilter::Metadata` field is a bare `bool`).
  It failed to compile with `error[E0308]: mismatched types … expected
  'Option<bool>', found 'bool'`. Resolved to `Some(false)`. The plan's
  Type-consistency self-review missed this one nested literal; every other
  occurrence in the plan was correct.
- **GREEN:** widened the match to a 6-tuple, added `None` to the five existing
  arms' patterns, and added the `metadata_filter` arm with the explicit
  `as Arc<dyn MetadataMatch>` cast (required — without it the closure infers
  `Arc<MetadataMatcher>` and the field type will not unify) and
  `match_if_key_not_found.unwrap_or(true)`. Updated the fn doc FIVE→SIX arms.
- **Verification:** `cargo test -p envoy-http1 --lib compile_access_log_filter` →
  `3 passed; 0 failed` (the new test plus both pre-existing compile tests);
  `cargo test -p envoy-http1 --lib` → `184 passed; 0 failed`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean.

### T7 — fixture `0081-accesslog-metadata-filter` + entrypoint — commit `d8522aa`

- **Files:** `tests/fixtures/0081-accesslog-metadata-filter/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
  + `tests/differential/tests/access_log_metadata_filter.rs`.
- Modelled on `0080-accesslog-or-filter`, swapping the filter for a
  `metadata_filter`, adding an `envoy.filters.http.header_to_metadata` filter
  BEFORE the router (mapping `x-a` → `com.example:k`), and extending the format
  with `%DYNAMIC_METADATA(com.example:k)%` — which, unlike `%REQ(X-A)%`, is NOT
  `REQ_ALLOW_LIST`-gated, so the LINE echoes the gating value.
- The four per-side divergences were generated and then DIFFED to confirm
  exactly: `admin` (envoy only), listener bind `0.0.0.0` vs `127.0.0.1`,
  `generate_request_id: false` (envoy only), mount path. Everything else —
  including the whole `header_to_metadata` stanza and the `metadata_filter`
  block — is byte-identical across the two sides.
- Probes are kept-LAST (ADR-0147): probe 1 `x-a: 2` (`k="2"` → value mismatch)
  DROPPED first, probe 2 `x-a: 1` KEPT last → the cheap 2 s `CF70_3_SETTLE`.
- **Verification (Docker differential, run locally):** `cargo build -p envoy-bin`
  then `cargo test -p differential --test access_log_metadata_filter` →
  `test access_log_metadata_filter ... ok`, `1 passed; 0 failed` in 10.18 s,
  **GREEN on the first run** (no host-flake family fired). One byte-identical
  line `STATUS=200 PATH=/x M=1` on each side.

### T8 — fixture `0082-accesslog-metadata-filter-key-not-found` + entrypoint — commit `7f175aa`

- Derived from `0081` with the node id / mount paths retargeted, the comment
  block rewritten, and `match_if_key_not_found: false` added to the filter.
- **The ADR-0155 PV-6 correction was applied and then VERIFIED:** the
  `header_to_metadata` rule carries `on_header_present` ONLY. `grep -n
  "^\s*on_header_missing:"` over both side configs returns NOTHING (the single
  textual hit is inside an explanatory comment), so a request without `x-a`
  writes nothing and `com.example:k` is genuinely absent.
- **Verification:** `cargo test -p differential --test
  access_log_metadata_filter_key_not_found` → `1 passed; 0 failed` in 3.36 s,
  GREEN first run.
- **MUTATION CHECK — the fixture is NON-VACUOUS.** Deleting the
  `match_if_key_not_found: false` line from BOTH sides turned the fixture RED:
  `CF-71-1: an access log grew beyond 1 lines under a 2s settle (envoy_rust=2,
  envoy=1) — a suppressed record leaked`. The fixture therefore genuinely depends
  on the flag rather than passing for an unrelated reason. Both config files were
  restored from a scratchpad backup and re-DIFFED byte-identical, and the fixture
  re-run GREEN (`1 passed`) before committing.
- **The mutation's per-side counts prompted a direct LIVE RE-MEASUREMENT of
  SPEC R-0.4** (they read `envoy=1`, which naively suggests upstream DROPS the
  key-absent record even under the default — contradicting R-0.4). Two standalone
  `envoyproxy/envoy:v1.33.0` runs on this exact config (port-mapped per memory
  `state0-recon-docker-needs-port-mapping`; graceful `docker stop -t 10` to flush):

  | `match_if_key_not_found` | real Envoy access log |
  |---|---|
  | **absent** | `STATUS=200 PATH=/x M=-`<br>`STATUS=200 PATH=/x M=1` (2 lines — key-absent **KEPT**) |
  | **`false`** | `STATUS=200 PATH=/x M=1` (1 line — key-absent **DROPPED**) |

  **SPEC R-0.4 is CONFIRMED**: the wrapper default is `true`, and envoy-rust's
  `unwrap_or(true)` is correct. The mutation's `envoy=1` was a FLUSH-TIMING
  artifact, not a semantic difference — the driver aborts mid-settle as soon as
  envoy-rust exceeds the expected line count, before real Envoy has flushed its
  buffered second line. This measurement is quoted into `BEHAVIOR_CONTRACT.md` §B
  and the `0082` README, and it independently re-derives the R-0.4 polarity flip
  that ADR-0155 recorded from the state-0/1 recon. Both recon containers were
  removed afterwards.

### T9 — `parse_bootstrap` fuzz seed + the 63rd `!`-un-ignore line — commit `e02161a`

- Created `crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml`
  (a `metadata_filter` with a `string_match` value + `match_if_key_not_found:
  false`, over a `header_to_metadata` producer) and inserted
  `!corpus/parse_bootstrap/metadata_filter.yaml` after the `and_or_filter.yaml`
  line in `crates/envoy-config/fuzz/.gitignore`.
- **NO new fuzz target and NO `ci.yml` edit** (PV-7): the seed rides the existing
  `parse_bootstrap` target, whose CI step names no corpus path.
- **Verification of the silent-untracked trap** (memory
  `fuzz-corpus-seed-gitignored-by-default`): after `git add`,
  `git ls-files …/metadata_filter.yaml` PRINTS the path, and the tracked seed
  count went **62 → 63**.
- Ran the target over the seed from the CRATE dir (memory
  `cargo-fuzz-runs-from-crate-dir-not-repo-root`). NB `cargo fuzz run` takes a
  DIRECTORY, not a file — passing the file errors with `ERROR: The required
  directory "…/metadata_filter.yaml" does not exist`; re-run against a
  single-seed scratch dir: `1 files found`, `#2 INITED cov: 2367 ft: 2368`,
  `Done 2 runs in 0 second(s)` — parses clean, no crash.

### T10 — `BEHAVIOR_CONTRACT.md` `metadata_filter` subsection — commit `3455815`

- Inserted `### Phase 74 (ADR-0154/0155)` after the phase-73 subsection, with
  §A Schema / §B Decision / §C `invert` accepted-but-INERT / §D where envoy-rust
  is STRICTER / §E mutual exclusion / §F rendering the gating value / §G derived
  not separately measured (CF-74-5) / §H authoritative fixtures.
- §B carries the T8 live-measurement table above; §D also records the CF-74-4
  RBAC asymmetry; §H documents why `0082` omits `on_header_missing`.
- **Section-separator fix:** the scripted insert initially consumed the `---`
  that closed the phase-73 subsection and left a doubled blank before
  `## xDS wire state machine`. Both were repaired so the file matches the
  established convention (`---` between every `### Phase NN` sibling), verified
  by re-reading both boundaries.

### Post-task: `cargo fmt --all` — commit `50078d7`

Per memory `envoy-rust-state4-ci-first-execution` the per-task discipline defers
`cargo fmt --check` to the state-4 gate, so CI is routinely red-at-fmt mid-phase.
Run here as a de-risk (the phase-73 precedent): `cargo fmt --all -- --check`
reported real drift from T3's mechanical call-site edits (long `should_log(…)`
calls needing multi-line wrapping), T2's validator string, and T1's `lib.rs`
re-export block. Applied `cargo fmt --all` → `--check` clean, then re-verified:
`cargo build --workspace --all-targets` 0 errors; `cargo clippy --workspace
--all-targets --all-features -- -D warnings` clean; `envoy-accesslog` 112 passed;
`envoy-config --lib metadata` 46 passed; `envoy-http1 --lib
compile_access_log_filter` 3 passed.

## State-3 exit summary

All **10 TDD tasks landed**, each RED-first with the failure quoted above, each
committed separately (`3bb4e9e`, `867e17d`, `796450d`, `cd5a675`, `096efd9`,
`1792037`, `d8522aa`, `7f175aa`, `e02161a`, `3455815`, plus the `50078d7` fmt
pass). Both new differential fixtures are GREEN locally on the first run. The
§7.5 gate was deliberately NOT run — that is the SEPARATE §5 state-4
verification session.

**Carry-forward disposition unchanged from `PLAN.md`:** N73-R1 CONSUMED (T1);
CF-74-1/2/3 remain deferred by design; **CF-74-4** (the RBAC validator's missing
empty-segment-`key` check) and **CF-74-5** (`present_match` on the RESOLVED
branch, pinned in-process only) remain OPEN as opened at the PLAN-write; M71-3,
M73-R2, M70-R4/R9 NOT folded.

**Two deviations from `PLAN.md`, both resolved per D-3.5 and detailed above:**
(1) T3's clippy `only_used_in_recursion` (a structural consequence of the T3/T4
split — scoped `#[allow]`, removed in T4); (2) T6's plan literal writing
`match_if_key_not_found: false` where the CONFIG-side field is `Option<bool>`
(→ `Some(false)`). Neither changes the phase's design or scope.
