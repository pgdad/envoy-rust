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
  sites and the two already-widened production gates): **80 lines patched, of
  which 78 are genuine CALL SITES and 2 are DOC COMMENTS** —
  `filter.rs` 36, `file_sink.rs` 4, `envoy-http1/hcm.rs` 38 (all call sites), and
  `envoy-http2/hcm.rs` 2 (**both `///` prose, NOT calls**).
  `cargo build --workspace --all-targets` → **0 errors**.
  > **CORRECTED at the §5.2 state-3 re-entry** (`REVIEW.md` I-2b — the phase's
  > only undocumented deviation). This bullet originally read "**80 call sites
  > patched**", counting two doc-comment lines as code. The script's
  > balanced-paren rule had no `///` guard, so it rewrote prose describing the
  > production gate into `&Default::default()` — i.e. into the claim that the
  > gate feeds an EMPTY metadata store, precisely backwards. `cargo fmt` does not
  > reflow doc comments, so the fmt pass did not surface it. Both comments were
  > restored (and re-wrapped) at the re-entry. Re-derived on disk with
  > `git show 796450d --numstat -- crates/envoy-http2/src/hcm.rs` → `3 2`, whose
  > three `+` lines are ONE production argument (`&record.dynamic_metadata,`) and
  > TWO `///` lines; and `grep "should_log(" crates/envoy-http2/src/hcm.rs |
  > grep -v "///"` → exactly **1**, the real gate.
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

---

## §7.5 gate (state-4 verification)

> **State-4 verification** (`superpowers:verification-before-completion`, per
> `BOOTSTRAP_PROMPT.md` §5 state-4 + D-3.6 + `PLAN.md` §7.5). A SEPARATE session
> from the state-3 implementation above. It RUNS and ADJUDICATES the §7.5
> phase-done gate on the FINAL tree and records the verdict; it does NOT write
> `REVIEW.md` (that is the §5 state-5 code-review, ADR-0127) and implements no
> new behavior. The state-3 de-risk runs recorded above are PRIOR EVIDENCE ONLY —
> every gate item below was re-run here from scratch.

### STEP 0 confirmation (disk-authoritative)

- `git status --porcelain` clean; branch `main`; `HEAD` = the phase-74 §5 state-3
  implementation commit `a790d72d1e1e7f86eb6f6b5c5e75625055c6205e`.
- `git fetch origin --prune` → `origin/main` at the SAME SHA. No sibling
  workstream had advanced; `REVIEW.md` absent, so the `SKILL_ROUTING.md` state
  machine resolves to state 4 unambiguously.
- Toolchain: `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` / `rustc 1.95.0 (59807616e 2026-04-14)`.

### The CI baseline used for the numeric cross-check

CI run `30043010031` on the FULL 40-char SHA `a790d72d1e1e7f86eb6f6b5c5e75625055c6205e`
is `completed` / `success`, both jobs green with full step counts (`build + test + lint`
15 steps, `fuzz` 13 steps — NOT the `cancelled` + `steps:0` runner-starvation
signature). The job log was downloaded (`gh run view --job 89327288859 --log`,
526 626 bytes) and its `test result:` lines summed:

```
CI passed=2094 failed=0 lines=159
```

This is the decisive reference for gate (b) (memory `local-red-set-varies-run-to-run`:
`local passed+failed == CI passed`).

### (a) The two NEW fixtures — **GREEN**

Run inside the full `cargo test --workspace --no-fail-fast` sweep below (the
debug `envoy-bin` the differential harness executes was current — `cargo build
--workspace --all-targets` immediately preceded it, memory
`differential-harness-uses-debug-envoy-bin`):

```
     Running tests/access_log_metadata_filter.rs
test access_log_metadata_filter ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.25s

     Running tests/access_log_metadata_filter_key_not_found.rs
test access_log_metadata_filter_key_not_found ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.36s
```

Both GREEN on the first attempt, under full parallel differential load — not only
in the isolated state-3 runs. `tests/fixtures/` now holds **82** directories
(`0081-accesslog-metadata-filter`, `0082-accesslog-metadata-filter-key-not-found`
are the two newest); **36** fixtures carry an `access_log` stanza (34 prior + 2
new), matching the STATE.md figure.

### (b) All pre-existing fixtures `0001`–`0080` — **GREEN except 5 documented host-flakes**

```
cargo test --workspace --no-fail-fast   (full output redirected to a file — NEVER
                                         piped through tail, memory
                                         never-pipe-verification-runs-through-tail)
→ 159 `test result:` lines
→ LOCAL passed=2089 failed=5 sum=2094
→ TEST_EXIT=101
```

**The decisive numeric cross-check PASSES exactly:**
`local 2089 passed + 5 failed = 2094` **==** `CI 2094 passed`, with **159**
`test result:` lines on BOTH sides. No test silently failed to run, and the local
RED set is exactly the 5 tests below — nothing else regressed.

Each of the 5 was re-run in ISOLATION naming its test binary (memory
`cargo-test-p-name-false-green-filtered-out` — `0 passed` is not a pass; the
`N passed` count is what is asserted, never the exit code):

| # | test | isolation re-run | adjudication |
|---|---|---|---|
| 1 | `access_log_h2_rcd_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` | host-flake family `tcpclosebackend-ipv6-unreachable-host-flake` |
| 2 | `access_log_h2_uc_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` | same family |
| 3 | `access_log_rcd_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` | same family |
| 4 | `access_log_rf_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` | same family |
| 5 | `admin_config_dump_server_info` | `test result: FAILED. 0 passed; 1 failed` | host-flake family `differential-host-bridge-ip-192-168-65-2` |

All five fail **deterministically** in isolation, which is the signature of the
ENVIRONMENTAL host-networking class rather than the parallel-load class — and the
failure text names the environmental cause directly, so the diagnosis is
MEASURED, not assumed:

- **#1–#4** — real Envoy cannot reach the host-spawned close backend and reports a
  connect failure against an **IPv6** address instead of the intended reset:

  ```
  envoy="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{remote_connection_failure|
         immediate_connect_error:_Network_is_unreachable|remote_address:[fdc4:f303:9324::254]:32959}\",\"rf\":\"UF\"}"
  envoy-rust="{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}"
  ```

  envoy-rust produces the CORRECT `UC` / `connection_termination`; the reference
  side never got a reset to observe. Exactly the documented 4-witness set.

- **#5** — the host routes the backend via `192.168.65.2`, which is not in the
  fixture's allow-list (`192.168.65.254` / `172.17.0.1`), so all 18 `/clusters`
  host lines land `envoy-only`:

  ```
  text_lines diverged after allow-lists:
    envoy-only:      ["backend::192.168.65.2:43109::canary::false", … 18 entries …]
    envoy-rust-only: []
  ```

**None of the five touches this phase's surface** — four are upstream-reset
`%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%` witnesses and one is the admin
`/clusters` scrape; none sets an `AccessLogFilter` of any arm. All five are
CI-authoritative and were `success` on this exact SHA in run `30043010031`. **Not
regressions.** No mass `client error (Connect)` RED appeared, so the Docker
daemon was healthy throughout (memory `docker-desktop-down-after-reboot-kvm-acl`
did not apply).

### (c) Conformance — **no new suite required; the existing suite is GREEN**

Access-log emission gating is not codec-conformance-gated, so the phase declares
no new suite. CONFIRMED on disk rather than asserted:

- `tests/conformance/` holds exactly one suite (`h2spec/`), and
  `git diff --stat 53893b67…..HEAD -- tests/conformance/` is **EMPTY** — phase 74
  touched no conformance file.
- `git diff --stat 53893b67…..HEAD -- tests/conformance/h2spec/known-failures.txt`
  is likewise EMPTY — the known-failures list was NOT trimmed (memory
  `h2spec-3-5-2-preface-host-sensitive`).
- The existing gate nonetheless RAN inside the workspace sweep and passed:
  `test h2spec_pass_rate_gate ... ok`.

### (d) Fuzz — **CLEAN**

`cargo fuzz` was run from the CRATE dir (memory
`cargo-fuzz-runs-from-crate-dir-not-repo-root`), at the same short budget the CI
step uses (`.github/workflows/ci.yml:107`):

```
cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30

INFO: 10647 files found in /home/esa/git/envoy-rust/crates/envoy-config/fuzz/corpus/parse_bootstrap
#10648  INITED cov: 16603 ft: 34756 corp: 3230/2173Kb exec/s: 10648 rss: 372Mb
Done 429637 runs in 119 second(s)
FUZZ_EXIT=0
```

No crash, no leak, no timeout; the tree stayed clean afterwards (new libFuzzer
artifacts are `*`-ignored).

The new seed's TRACKED status was re-verified (memory
`fuzz-corpus-seed-gitignored-by-default`):

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/ | wc -l
63
```

**NO `ci.yml` edit was needed or made** — `crates/envoy-config/fuzz/fuzz_targets/`
still holds exactly ONE target (`parse_bootstrap.rs`), this phase added no new
target, and the existing step names no corpus path, so the 63rd seed is globbed
automatically (ADR-0137 precedent; memory `new-fuzz-target-needs-a-ci-yml-step`
therefore does not apply).

### (e) build / clippy / fmt / test / deny — **ALL CLEAN**

```
cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
BUILD_EXIT=0

cargo fmt --all -- --check
(no output)
FMT_EXIT=0

cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s
CLIPPY_EXIT=0

cargo deny check
advisories ok, bans ok, licenses ok, sources ok
DENY_EXIT=0

cargo test --workspace --no-fail-fast
2089 passed; 5 failed  (adjudicated in (b) above)
TEST_EXIT=101
```

**`cargo deny check` was the item flagged most likely to red fresh** (it is NOT in
the CI job set, and memory `cargo-deny-reds-on-unrelated-advisory` warns a newly
published RustSec advisory against an existing dep can red it). It came back
**clean** — `advisories ok, bans ok, licenses ok, sources ok`, exit 0. Its only
output is five pre-existing `license-not-encountered` WARNINGS naming allow-list
entries in `deny.toml` (`0BSD`, `BSD-2-Clause`, `MPL-2.0`, `Unicode-DFS-2016`,
`Zlib`) that no dependency uses — a policy-hygiene note, not a check failure, and
unchanged by this phase. **No dep patch-bump was needed.**

**On the cached build/clippy timings.** The standalone `build` and `clippy`
invocations finished in ~0.1 s because the state-3 session had already built this
exact tree, so cargo's fingerprints were fresh. Rather than accept a cached
green, the phase's ten changed `.rs` files were `touch`ed and workspace clippy
re-run — it then genuinely re-analysed **15 workspace crates**
(`Checking envoy-accesslog`, `Checking envoy-config`, `Checking envoy-http1`,
`Checking envoy-http2`, `Checking differential`, …; `cargo clippy` emits
`Checking`, not `Compiling`) and still reported **zero warnings, zero errors,
exit 0**. (Corrected at the §5.2 state-3 re-entry per `REVIEW.md` M74-15(b):
15 is the number RE-CHECKED after touching four crates, not the workspace size —
`[workspace] members` lists **22**.) Independently, `cargo test --workspace` emitted **159 `test result:` lines** on
this tree, which is itself full-target compilation evidence. (Corrected at the
§5.2 state-3 re-entry per `REVIEW.md` M74-15(a): 159 counts `test result:` LINES,
which include doc-test lines, so it over-counts test BINARIES.)

### Invariant spot-checks (cheap, non-negotiable)

- **ADR-0150 seam HOLDS** — `crates/envoy-accesslog/Cargo.toml` `[dependencies]`
  is still `tokio`, `bytes`, `tracing`, `thiserror` and **ZERO workspace crates**;
  the metadata matcher is an injected `Arc<dyn MetadataMatch>` trait object.
- **`LogFilter` still derives ONLY `#[derive(Debug, Clone)]`** — no `Eq`, no
  `PartialEq` was added.
- **D-3.8 holds** — `#![forbid(unsafe_code)]` present in **14 of 14** crate roots
  under `crates/`, and in fact in **22 of 22** workspace MEMBER roots (the
  workspace also has `tests/differential`, `tests/conformance/h2spec` and the six
  `tests/helpers/*` crates). Re-measured at the §5.2 state-3 re-entry per
  `REVIEW.md` M74-15(c), which noted the original figure UNDERSTATES coverage
  rather than overclaiming.
- **ROADMAP row `74` untouched** (`in-progress`); **`DECISIONS.md` ledger head is
  ADR-0155** and there is **no `## ADR-0156` section** — the §6.1 split
  reservation stays UNFIRED, as PV-8 determined.

### Verdict

**The §7.5 gate is GREEN: (a) ✅ (b) ✅ (c) ✅ (d) ✅ (e) ✅.** Gate (f)
(`REVIEW.md` approved) is the SEPARATE §5 state-5 code-review and is deliberately
NOT attempted here (ADR-0127: the context that wrote an artifact must not grade
it).

**No real defect was found**, so no §5.2 state-3 re-entry is owed. Every gate item
above was actually RUN in this session on the final tree — none is inferred from
the state-3 de-risk or from CI. The two items where prior evidence existed
(`fmt`/`clippy` and the two new fixtures) were re-run regardless, and the two new
fixtures additionally passed under full parallel differential load, which the
state-3 isolated runs could not show.

Next: the §5 state-5 code-review (a SEPARATE session) — write `REVIEW.md`.

---

## §5.2 state-3 RE-ENTRY (fixing `REVIEW.md`'s four Important findings)

> **§5.2 state-3 re-entry** (`superpowers:executing-plans` +
> `superpowers:test-driven-development`, per `BOOTSTRAP_PROMPT.md` §5.2 — a
> `REVIEW.md` with issues re-enters at **step 3, NOT step 4**). A SEPARATE
> session from the state-5 code-review that produced the findings (§5.1 /
> ADR-0127). It does NOT re-run the §7.5 gate (that is the SEPARATE state-4
> RE-VERIFICATION), does NOT write a new `REVIEW.md`, does NOT flip ROADMAP row
> `74`, and fires NO ADR (ledger head stays **ADR-0155**; **ADR-0156** remains
> reserved for the §6.1 split and UNFIRED — no split occurred).

### STEP 0 confirmation (disk-authoritative)

- `git status --porcelain` clean; branch `main`; `HEAD` = the phase-74 §5 state-5
  code-review commit `93ec7393c2648751ac8323e1e02cc6d09b15f2e8`.
- `git fetch origin --prune` → `origin/main` at the SAME SHA. No sibling
  workstream had advanced; `REVIEW.md` present with verdict **APPROVED WITH
  MUST-FIX**, so the `SKILL_ROUTING.md` state machine resolves to the §5.2
  state-3 re-entry unambiguously.
- CI on `93ec7393…` was already confirmed `completed`/`success` (run
  `30055024452`, both jobs green with full step counts 15 and 13).

### The governing fact, and how TDD's RED was honored

**All four Important findings pin ALREADY-CORRECT code.** The state-5 review
LIVE-PROBED every one of them cross-proxy against `envoyproxy/envoy:v1.33.0` and
measured PARITY — they are pins the suite was MISSING, not bugs it was hiding
(the one exception, I-1, is a documentation-accuracy defect about a REJECT-side
load-parity gap). Every new pin therefore passes on its first run, which would
make a naive "watch it fail" impossible.

Per memory `state3-reentry-fixes-are-characterization-pins-red-via-mutation`,
TDD's RED was honored with a **MUTATION check** on each: break the engine in
exactly the way the finding warns about, watch the NEW pin go RED, revert.

Mutations ran in a **scratch `git worktree`** (`git worktree add --detach`, per
memory `mutation-checks-collide-with-parallel-subagents`), `git checkout --`-restored
between runs and removed at the end; **the MAIN tree never carried a mutation** —
verified after every check by grepping the mutated expression in the main tree
(`unwrap_or(true)` intact at `envoy-http1/src/hcm.rs:1806`;
`&record.dynamic_metadata` intact at `envoy-http2/src/hcm.rs:1142`). Every
mutation run shows `Compiling <crate>` in its output, so none is a stale-binary
false pass (memory `mutation-check-needs-forced-rebuild`; note `cargo clippy`
prints `Checking`, not `Compiling` — memory `clippy-prints-checking-not-compiling`).
No tree-mutating subagent was dispatched this session.

### I-1 — the wrapped `BoolValue` load-parity divergence (DOC + PIN; NOT a code change)

**What was wrong:** `BEHAVIOR_CONTRACT.md` §A presented the `BoolValue` wrapper
acceptance as a **shared** property (`"{ value: true } is accepted alongside a
bare true"`), and §D's "where envoy-rust is STRICTER" list omitted it — so a
reader would conclude the wrapped spelling works here. MEASURED at state-5:
upstream ACCEPTS `match_if_key_not_found: { value: false }`, boots, and HONORS it
(the key-absent record was DROPPED); envoy-rust is BOOT-FATAL (`invalid type:
map, expected a boolean`, exit 1). A `{ bogus: false }` CONTROL is rejected
upstream naming `message google.protobuf.BoolValue`, proving the field is
genuinely wrapper-typed rather than ignored. Project invariant §4.5 / D-3.3
requires the contract be corrected, never left silently wrong.

**Fixed:**
- `BEHAVIOR_CONTRACT.md` **§A** corrected — it now states plainly that the two
  spellings are **NOT at parity**, that upstream takes both while envoy-rust
  takes only the bare one, and points at §D/CF-74-6.
- `BEHAVIOR_CONTRACT.md` **§D** gained the divergence with its full measured
  table (bare / wrapped / `{ bogus: false }` control), the reason the
  `Option<bool>` model is CORRECT and must not be "fixed" in isolation, and the
  ADR-0063 `UInt32Value` bare-only house precedent.
- **CF-74-6 OPENED** in `BEHAVIOR_CONTRACT.md` §D and `SPEC.md` §10. Owner = a
  future wrapper-spelling-parity phase, which should ALSO survey the other
  `Option<bool>`/`Option<u32>` wrapper fields rather than close this one field
  alone.
- **Serde pin added** to `metadata_filter_deserialize_round_trip_and_defaults`
  (`bootstrap.rs`): BOTH wrapped polarities (`{ value: true }` / `{ value: false }`)
  must be errors, AND the error must name the wrapper shape
  (`contains("expected a boolean")`) so the pin cannot pass for an unrelated
  reason.
- `MetadataFilter`'s own doc comment corrected — its `{ value: true }` example
  read as if the wrapped spelling parsed HERE; it now says upstream accepts it
  and that only the bare form parses here, naming CF-74-6.

**`Option<bool>` was NOT replaced** with a wrapper-accepting deserializer, exactly
as `REVIEW.md` directs: the model correctly preserves absent-vs-explicit-`false`,
which is what makes `unwrap_or(true)` meaningful.

**RED (mutation):** the pin guards a DELIBERATE POSTURE, so the faithful mutation
is the change it exists to catch — swapping `Option<bool>` for a
wrapper-accepting `deserialize_with` (an untagged `Bare(bool) | Wrapped { value }`
enum) in the worktree:

```
---- bootstrap::tests::metadata_filter_deserialize_round_trip_and_defaults stdout ----
panicked at crates/envoy-config/src/bootstrap.rs:13528:18:
the wrapped BoolValue spelling stays boot-fatal (CF-74-6):
    MetadataFilter { matcher: None, match_if_key_not_found: Some(true) }
test result: FAILED. 0 passed; 1 failed
```

**GREEN:** `cargo test -p envoy-config --lib metadata_filter_deserialize` →
`1 passed; 0 failed`.

### I-2 — the H2 emit gate's metadata threading had ZERO coverage

**What was wrong:** `crates/envoy-http2/src/hcm.rs:1142` passes
`&record.dynamic_metadata`, and mutating that ONE argument to
`&Default::default()` failed **no test in the workspace**: no H2 fixture carries
an access-log filter (`0076`–`0082` are all H1), and the H2 in-process filter
tests build only `StatusCode`/`ResponseFlag`/`Header` arms, none of which reads
the 4th argument. The state-5 review measured the gate at full cross-proxy parity
over HTTP/2, so this was an **undefended line, not a broken one** — but a future
regression would be silent and severe (every H2 `metadata_filter` would see an
empty store, logging everything or nothing).

**Fixed:** added `h2_metadata_filter_gate_reads_the_threaded_dynamic_metadata` to
`crates/envoy-http2/src/hcm.rs`, modelled on the phase-72 precedent
`h2_header_filter_keeps_match_drops_mismatch_and_absent` in the same file (phases
70/71/72 each added exactly such a test). The chain is
`[header_to_metadata (x-a → com.example:k), router]`, mirroring fixtures
`0081`/`0082` on H1, with `on_header_missing` deliberately absent so the
no-`x-a` probe leaves the key genuinely unresolved.

**TWO sinks share one server run**, pinning BOTH `match_if_key_not_found`
polarities against the SAME threaded store — which is what makes the pin
discriminating in both directions:

| probe | `com.example:k` | sink `mifknf=false` | sink `mifknf=true` (default) |
|---|---|---|---|
| `x-a: 2` | `"2"` — resolved, value mismatch | DROP | DROP |
| *(no `x-a`)* | absent — unresolved | DROP | **KEEP** |
| `x-a: 1` | `"1"` — resolved, value match | **KEEP** | **KEEP** |

→ asserts the `false` sink's file is exactly `"200\n"` and the default-`true`
sink's is exactly `"200\n200\n"`.

**RED (mutation):** `&record.dynamic_metadata` → `&Default::default()` at the H2
gate (`hcm.rs:1142`), the exact prescription in `REVIEW.md`:

```
---- hcm::tests::h2_metadata_filter_gate_reads_the_threaded_dynamic_metadata stdout ----
panicked at crates/envoy-http2/src/hcm.rs:3817:9:
assertion `left == right` failed: match_if_key_not_found=false must keep ONLY the
value-matching `x-a: 1` request — the mismatch and the key-absent request both drop: ""
  left: ""
 right: "200\n"
test result: FAILED. 0 passed; 1 failed
```

With an empty store every request looks key-absent, so the `false` sink drops
everything (0 lines, not 1) — and the `true` sink would keep everything (3 lines,
not 2). Both halves are load-bearing.

**GREEN:** `cargo test -p envoy-http2 --lib h2_metadata_filter_gate` →
`1 passed; 0 failed`. Full crate: `109 passed; 0 failed; 1 ignored`.

### I-2b — the phase's ONLY undocumented deviation: two doc comments describing the gate BACKWARDS

**What was wrong:** T3's balanced-paren fan-out script had no `///` guard, so it
appended the 4th argument to **prose it should have skipped**. The two lines it
touched in `crates/envoy-http2/src/hcm.rs` were both `///` doc comments, which
then asserted that the production emit gate feeds an **EMPTY** metadata store —
precisely backwards, and precisely the misreading that would motivate a wrong
"fix" to the gate (D-3.4). `cargo fmt` does not reflow doc comments, so the fmt
pass never surfaced it, and `:3559` additionally overran the file's wrap width.

**Re-derived on disk at this re-entry** (not taken on the review's word):

```
$ git show 796450d --numstat -- crates/envoy-http2/src/hcm.rs
3	2	crates/envoy-http2/src/hcm.rs
$ git show 796450d -- crates/envoy-http2/src/hcm.rs | grep -E "^\+" | grep -v "^+++"
+                &record.dynamic_metadata,
+    /// &envoy_req.headers, &Default::default())` gate (phase 72 added the header slice) end-to-end;
+    /// `should_log(status, flags, headers, &Default::default())` gate (hcm.rs ~1138); this test
```

— ONE production argument and TWO `///` lines. And, per-file, the number of ADDED
lines containing `&Default::default()` in that commit is `filter.rs` **36**,
`file_sink.rs` **4**, `envoy-http1/hcm.rs` **38**, `envoy-http2/hcm.rs` **2** — of
which the envoy-http2 pair is **2 of 2 doc comments**. So the true split is
**78 call sites + 2 doc-comment edits**, not "80 call sites".

**Fixed:** both doc comments restored to `&record.dynamic_metadata` /
`dynamic_metadata` and re-wrapped; the T3 bullet at `PROGRESS.md`'s
"Call-site fan-out" corrected in place with a quoted `> CORRECTED …` note
carrying the re-derivation above. Verified afterwards:
`grep -n "///.*should_log.*Default::default" crates/envoy-http2/src/hcm.rs` → **0
hits**, and `grep "should_log(" … | grep -v "///"` → exactly **1** (the real
gate). Re-verified AFTER the `cargo fmt` pass, since fmt is precisely what does
NOT police doc comments.

This is an instance of memory `mechanical-fanout-scripts-corrupt-doc-comments`
reaching a landed commit.

### I-3 — no committed fixture read the `match_if_key_not_found` default-`true` branch

**What was wrong:** the phase's headline observable — the `None → true` KEEP
branch, which `--mode validate` provably cannot reach because it is a proto3
`google.protobuf.BoolValue` default — was pinned **only by envoy-rust asserting
against itself in-process**. `0081` omitted the field but BOTH its probes sent
`x-a` (the key always resolved, so `unwrap_or` was never consulted; its only
textual `match_if_key_not_found` occurrence is a COMMENT), and `0082` pins the
field to explicit `false`.

**Fixed:** a **THIRD probe** in
`tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml` — `GET /x` with
**no** `x-a`, `expected_status: 200`, `expect_logged: true` — placed **SECOND**,
so the kept-LAST convention (ADR-0147) holds and the fixture keeps paying the
cheap 2 s `CF70_3_SETTLE` rather than the 12 s `CF71_1_SETTLE`.
`expected_logged_count` becomes **2**, and the two kept lines are byte-DISTINCT
(`…M=-` then `…M=1`), so the fixture now pins line ORDER as well as count. No new
fixture, no driver change, no config change. `0081/README.md` updated: the
three-probe table with the branch each takes, the two-line expected output, why
probe 2 exists, and an explicit warning that `on_header_missing` must NOT be
added to `0081` either (it would make the key RESOLVE and silently vacate the
witness — the same ADR-0155 PV-6 trap `0082` documents). The README's claim that
the `-` rendering is witnessed "on both proxies" (M74-8) is now TRUE.

**RED (mutation) — a TWO-PART demonstration**, because the finding is not just
"a mutation REDs" but "the fixture was BLIND to it". Mutation:
`unwrap_or(true)` → `unwrap_or(false)` at `envoy-http1/src/hcm.rs:1806`, in the
worktree, with `cargo build -p envoy-bin` there first (memory
`differential-harness-uses-debug-envoy-bin`; the harness resolves
`target/debug/envoy-bin` from the differential crate's own manifest dir, so the
worktree run used the worktree's mutated binary).

**Part 1 — mutated engine + the PRE-FIX 2-probe `expectations.yaml`:**

```
test access_log_metadata_filter ... ok
test result: ok. 1 passed; 0 failed; ... finished in 3.21s
```

**GREEN — the fixture did not notice the wrapper default had been inverted.**
That is finding I-3 reproduced directly, not merely argued.

**Part 2 — the same mutated engine + the NEW 3-probe `expectations.yaml`:**

```
---- access_log_metadata_filter stdout ----
panicked at tests/differential/tests/access_log_metadata_filter.rs:33:10:
fixture green: envoy-rust emitted 1 access-log lines but 2 were expected to be
logged; lines: ["STATUS=200 PATH=/x M=1"]
test result: FAILED. 0 passed; 1 failed
```

**RED** — and for exactly the right reason: the missing line is `STATUS=200
PATH=/x M=-`, the wrapper-default witness. Real Envoy is unaffected by the
mutation and still emits both lines, so the assertion is genuinely cross-proxy.

**GREEN (unmutated main tree, after `cargo build -p envoy-bin`):**
`cargo test -p differential --test access_log_metadata_filter` →
`1 passed; 0 failed` in 12.76 s. Sibling `0082` re-run unchanged and green:
`1 passed; 0 failed` in 3.32 s.

### I-4 — a `SafeRegex` metadata value was compiled but never EVALUATED

**What was wrong:** `crates/envoy-config/src/matcher.rs:142` carries
`.expect("validator ensured StringMatcher SafeRegex compiled")` — a **request-time
panic** path, not a wrong-verdict path. Nothing anywhere in the workspace ever ran
`MetadataMatcher::matches` with a `SafeRegex` value, so that `.expect()` was
unexercised on the metadata route. Compounding it,
`reuses_the_value_matcher_engine_verbatim` claimed in its own comment that "Every
modelled StringMatcher mode routes through ValueMatcher::matches" while SKIPPING
SafeRegex — false as written.

**Fixed:** added a `SafeRegex` block to `reuses_the_value_matcher_engine_verbatim`
that compiles the pattern exactly as the validator does
(`ValueMatcher::compile_safe_regexes`, the same call
`validate_access_log_metadata_matcher` makes in place) and then matches, asserting
a match → `Some(true)`, a non-match → `Some(false)` (**not** `None` — only an
unresolved PATH yields `None`, so a regex rejection must not fall back to
`match_if_key_not_found`), and an absent key → `None`. The false comment is
replaced with an accurate one naming all five modes and explaining why SafeRegex
is the one that matters most.

> **One assertion was written and then deliberately REMOVED before commit.** A
> draft asserted the behavior of an UNANCHORED pattern and captioned it with a
> claim about upstream's full-match `SafeRegex` semantics. That claim was **not
> measured** by this project, and the assertion actually documented search
> semantics — i.e. the comment and the assertion contradicted each other. Writing
> it would have put a fresh unmeasured claim into the tree: exactly the I-1
> defect class this same session is fixing (D-3.3 — the contract is the contract;
> never assert equivalence that has not been measured). Anchoring semantics are
> pre-existing phase-35/36 `StringMatcher` surface, not the metadata route, and
> the test now says so explicitly instead.

**RED (mutation):** `REVIEW.md` prescribes "clear the `compiled` field" — done by
dropping the `compile_safe_regexes()` call so the validator's in-place compile is
simulated away:

```
---- matcher::metadata_match_tests::reuses_the_value_matcher_engine_verbatim stdout ----
panicked at crates/envoy-config/src/matcher.rs:142:18:
validator ensured StringMatcher SafeRegex compiled
test result: FAILED. 0 passed; 1 failed
```

The panic is at `matcher.rs:142` — the exact `.expect()` I-4 names, reached from
the metadata route for the first time in an automated test.

**GREEN:** `cargo test -p envoy-config --lib metadata_match_tests` →
`3 passed; 0 failed`.

### CF-74-5 → CLOSED; `BEHAVIOR_CONTRACT.md` §G upgraded to MEASURED

§G previously read "**Derived, not separately measured**" — `present_match` on the
RESOLVED branch was inferred from the structural rule (the value matcher is
consulted only when the path resolves) and pinned in-process only. The state-5
code-review MEASURED it cross-proxy in BOTH polarities. §G now carries the S4/S5
table verbatim:

| sink | filter | kept — real Envoy | kept — envoy-rust | verdict |
|---|---|---|---|---|
| S4 | `present_match: true`, `match_if_key_not_found: false` | r1 r2 r5 r6 | r1 r2 r5 r6 | **PARITY** |
| S5 | `present_match: false`, `match_if_key_not_found: true` | r3 r4 r7 | r3 r4 r7 | **PARITY** |

S4 and S5 are **exact complements** over the seven requests and both proxies agree
on every cell (per-side concatenation `md5sum` `380b58e471f8c0c545d02a5e8b7b9df3`
on both sides). **CF-74-5 is CLOSED** in §G, in `SPEC.md` §10 and in `STATE.md`.
The in-process pin's comment in `matcher.rs` was updated from "pinned in-process,
not live-probed" to record the measurement.

### Minors folded (files this re-entry already touched)

- **M74-1** — stale arm-count doc comments in `bootstrap.rs` rewritten to the
  future-proof "the other arms" phrasing T1 already adopted, so arm #7 need not
  touch them: the `response_flag_filter` field doc (was "Mutually exclusive with
  `status_code_filter`"), `header_filter` ("the other **two** arms"), `and_filter`
  and `or_filter` ("the other **four** arms"), the `HeaderFilter` type doc, and
  the test comment "all **FIVE** arms". Verified: `grep "the other two arms\|the
  other four arms\|all FIVE arms"` → **0 hits**.
- **M74-2** — `bootstrap.rs`'s "A matcher-less filter keeps every record"
  overclaimed: true only when `match_if_key_not_found` is absent or `true`. Now
  mirrors the correct adjacent `hcm.rs` wording (it takes the not-found policy for
  every record — keeping all under absent/`true`, DROPPING all under explicit
  `false`), citing the state-5 probe-group-1 S6/S7 measurement of both polarities.
- **M74-15** — the three counting imprecisions, each **re-measured on disk** here
  rather than transcribed: (a) "159 test binaries" → "159 `test result:` lines"
  (that count includes doc-test lines, so it over-counted binaries); (b) "all 15
  workspace crates" → "15 workspace crates", with a note that 15 is the number
  re-checked after touching four and `[workspace] members` lists **22**; (c) "14
  of 14 workspace crate roots" kept but qualified — it is right for `crates/`, and
  the attribute is in fact present at **22 of 22 member roots** (measured by
  walking `[workspace] members` and reading each `src/lib.rs`/`src/main.rs`), so
  the original figure UNDERSTATED coverage rather than overclaiming.

M74-3..M74-14 and M74-16 were optional and are **NOT** folded; they carry forward.

### Verification run at this re-entry (NOT the §7.5 gate)

The §7.5 gate is deliberately NOT re-run here — that is the SEPARATE state-4
RE-VERIFICATION. What WAS run, on the final tree:

```
cargo fmt --all -- --check                                          → clean (exit 0)
    (drift from the new H2 test was applied with `cargo fmt --all`
     first; the two restored doc comments were re-verified AFTER the
     fmt pass, since fmt does not reflow doc comments)
cargo clippy --workspace --all-targets --all-features -- -D warnings → clean (exit 0)
cargo test -p envoy-config    --lib                                  → 648 passed; 0 failed
cargo test -p envoy-accesslog --lib                                  → 112 passed; 0 failed
cargo test -p envoy-http1     --lib                                  → 184 passed; 0 failed
cargo test -p envoy-http2     --lib                                  → 109 passed; 0 failed; 1 ignored
cargo build -p envoy-bin                                             → clean
cargo test -p differential --test access_log_metadata_filter                → 1 passed; 0 failed (12.76s)
cargo test -p differential --test access_log_metadata_filter_key_not_found  → 1 passed; 0 failed (3.32s)
```

### Invariant spot-checks

- **ADR-0150 seam HOLDS** — `crates/envoy-accesslog/Cargo.toml` `[dependencies]`
  unchanged (`tokio`, `bytes`, `tracing`, `thiserror`; ZERO workspace crates). The
  new H2 test constructs the matcher on the `envoy-config` side and boxes it
  through `Arc<dyn envoy_accesslog::MetadataMatch>`, exactly as
  `compile_access_log_filter` does — it does not introduce a reverse edge.
- **`LogFilter` still derives ONLY `Debug, Clone`** — no `Eq`, no `PartialEq`.
- **The `unreachable!` lockstep guard is untouched** — no production match arm was
  added, removed or reordered this session.
- **NO production behavior changed.** The diff is: two new/extended tests, one new
  test, two restored doc comments, doc-comment wording, one fixture's
  `expectations.yaml` + `README.md`, and four docs files. `envoy-http1/src/hcm.rs`
  and every other engine file are **byte-unchanged** — confirmed by
  `git status --porcelain` listing neither.
- **`known-failures.txt` / `tests/conformance/` untouched**; **`DECISIONS.md`
  untouched** (ledger head **ADR-0155**, no `## ADR-0156`); **ROADMAP row `74`
  untouched** (`in-progress`, 6 cells) — no flip until the state-6 close-out.
- **The scratch worktree was removed** and the main tree verified unmutated.

### Carry-forward disposition after this re-entry

- **CLOSED:** **CF-74-5** (measured cross-proxy; §G upgraded).
- **OPENED / now recorded in the contract:** **CF-74-6** (the wrapped `BoolValue`
  spelling — `BEHAVIOR_CONTRACT.md` §D + `SPEC.md` §10 + a serde pin).
- **CONSUMED earlier in the phase:** N73-R1.
- **FOLDED here:** M74-1, M74-2, M74-15.
- **STILL OPEN:** CF-74-1, CF-74-2, CF-74-3, CF-74-4; M74-3..M74-14, M74-16;
  M73-R2 (probe groups 1/4 advanced it, but it asks for a committed FIXTURE, so it
  stays open), M71-3, M71-6 (this re-entry adds an in-process H2 metadata test —
  the standalone H2 *differential* stays deferred), M71-7/8, M70-R4/R9,
  CF-72-1/CF-72-2 (still the strongest NEXT candidate), CF-73-1, N73-R2, M73-R1,
  M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7, the older Minors and the
  HTTP-filters-family (1)–(4).

### Verdict

**All four Important findings are FIXED, each with recorded mutation-RED
evidence; CF-74-5 is CLOSED and CF-74-6 is OPENED and contract-documented; three
Minors folded.** No production behavior changed — every fix is a pin, a document
correction, or a fixture probe.

Next: the §5 state-4 **RE-VERIFICATION** (a SEPARATE session) — re-run and
re-adjudicate the full §7.5 gate (a)–(e) on this tree. Then a state-5 RE-REVIEW,
then the state-6 close-out. ROADMAP row `74` stays `in-progress` throughout.

---

## §7.5 gate (state-4 RE-VERIFICATION)

> **State-4 RE-VERIFICATION** (`superpowers:verification-before-completion`, per
> `BOOTSTRAP_PROMPT.md` §5 state-4 + §5.2 + D-3.6). A SEPARATE session from the
> §5.2 state-3 re-entry above. Per §5.2 a `REVIEW.md` with issues re-enters at
> step 3; step 3 is now COMPLETE, so the cycle resumes at step 4. This session
> RE-RUNS and RE-ADJUDICATES the full §7.5 phase-done gate (a)–(e) **from
> scratch** on the post-re-entry tree and records the verdict. It writes no new
> `REVIEW.md` (that is the SEPARATE state-5 RE-REVIEW, ADR-0127), implements no
> new behavior, does not flip ROADMAP row `74`, and relocates no close-out Notes.
> **Every gate item below was actually RUN in this session.** The re-entry's own
> verification runs are PRIOR EVIDENCE ONLY.

### STEP 0 confirmation (disk-authoritative)

- `git status --porcelain` clean; branch `main`; `HEAD` = the phase-74 §5.2
  state-3 re-entry commit `cab381d2784e1497aa46fd5054c1faa08c6c5d97`.
- `git fetch origin --prune` → `origin/main` at the **SAME** SHA. No sibling
  workstream had advanced (memory `concurrent-loop-sessions-race-on-phase-pick`).
- `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md` all present, and `STATE.md`
  `## Next expected skill` names the state-4 RE-VERIFICATION → the
  `SKILL_ROUTING.md` state machine resolves to step 4 unambiguously.
- Toolchain: `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` / `rustc 1.95.0 (59807616e 2026-04-14)`.

### The GOVERNING FACT, re-verified on disk rather than taken from the handoff

The re-entry changed **NO production behavior**:

```
$ git diff 93ec7393c2648751ac8323e1e02cc6d09b15f2e8..HEAD --stat
 crates/envoy-config/src/bootstrap.rs               |  65 ++-
 crates/envoy-config/src/matcher.rs                 |  63 ++-
 crates/envoy-http2/src/hcm.rs                      | 168 +++++++-
 docs/envoy-rust/BEHAVIOR_CONTRACT.md               |  78 +++-
 docs/envoy-rust/STATE.md                           |  57 +--
 docs/envoy-rust/STATE_HISTORY.md                   |  28 ++
 .../74-accesslog-metadata-filter/PROGRESS.md       | 451 ++++++++++++++++++++-
 .../phases/74-accesslog-metadata-filter/SPEC.md    |  32 ++
 .../0081-accesslog-metadata-filter/README.md       |  67 ++-
 .../expectations.yaml                              |  46 ++-
 10 files changed, 965 insertions(+), 90 deletions(-)

$ git diff 93ec7393…..HEAD -- crates/envoy-http1/src/hcm.rs crates/envoy-accesslog/src/filter.rs | wc -l
0
```

The two engine files are **byte-unchanged**, so the behavior state-4 and state-5
already adjudicated is the behavior that ships. What is genuinely NEW to verify is
that the added pins pass under FULL parallel load and that fixture `0081`'s THIRD
probe is stable cross-proxy — both confirmed in (a)/(b) below.

### The CI baseline used for the numeric cross-check

CI run `30083395623` on the FULL 40-char SHA
`cab381d2784e1497aa46fd5054c1faa08c6c5d97` is `completed` / `success`, both jobs
green with full step counts (`build + test + lint` 15 steps, `fuzz` 13 steps —
NOT the `cancelled` + `runner_name:""` + `steps:0` runner-starvation signature,
memory `ci-run-cancelled-with-no-runner-is-starvation`). Job `89449889518`'s
`test result:` lines sum to:

```
CI passed=2095 failed=0 lines=159
```

This is the decisive reference for gate (b) (memory
`local-red-set-varies-run-to-run`: `local passed+failed == CI passed`).

### (a) The two phase-74 fixtures — **GREEN**

`cargo build --workspace --all-targets` immediately preceded the sweep, so the
debug `envoy-bin` the differential harness executes was current (memory
`differential-harness-uses-debug-envoy-bin` — decisive here, because `0081`'s
expectations changed at the re-entry and a stale binary would mis-report it). The
rebuild is PROVEN, not assumed: the build log carries
`Compiling envoy-bin v0.0.0`, and `target/debug/envoy-bin`'s mtime
(`2026-07-24 11:15:19`) is 17 s before the sweep began.

Both ran inside the full `cargo test --workspace --no-fail-fast` sweep, i.e.
under full parallel differential load:

```
     Running tests/access_log_metadata_filter.rs
test access_log_metadata_filter ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.66s

     Running tests/access_log_metadata_filter_key_not_found.rs
test access_log_metadata_filter_key_not_found ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.13s
```

**`0081`'s NEW third probe is stable cross-proxy under parallel load** — its
12.66 s (vs `0082`'s 3.13 s) is the three-probe shape plus the 2 s
`CF70_3_SETTLE`, matching the re-entry's isolated 12.76 s. The fixture asserts
TWO byte-distinct kept lines in order (`STATUS=200 PATH=/x M=-` then
`STATUS=200 PATH=/x M=1`), so line ORDER is pinned as well as count.

Fixture-shape invariants re-confirmed on disk:

- `tests/fixtures/` holds **82** directories; **36** fixtures carry an
  `access_log` stanza (34 prior + 2 new), matching `STATE.md`.
- **`on_header_missing` appears in NEITHER fixture as a config key.** The two
  textual hits in `0082`'s config pair are `#` COMMENTS documenting the
  deliberate omission (`envoy.yaml:27`, `envoy-rust.yaml:25` — "deliberately NO
  `on_header_missing`"); `0081` has **0** hits of any kind. The ADR-0155 PV-6
  trap — adding it would make the key RESOLVE and silently vacate both the
  key-not-found and the `match_if_key_not_found`-default witnesses — holds on
  both fixtures.

### (b) All pre-existing fixtures — **GREEN except the 5 documented host-flakes**

```
cargo test --workspace --no-fail-fast   (FULL output redirected to a file — NEVER
                                         piped through tail, memory
                                         never-pipe-verification-runs-through-tail)
→ 159 `test result:` lines
→ LOCAL passed=2090 failed=5 sum=2095
→ TEST_EXIT=101
```

**The decisive numeric cross-check PASSES exactly:**
`local 2090 passed + 5 failed = 2095` **==** `CI 2095 passed`, with **159**
`test result:` lines on BOTH sides. **No test silently failed to RUN** — the
failure mode the handoff correctly flags as worse than a RED — and the local RED
set is exactly the 5 below, so nothing else regressed. (The count rose from the
prior state-4's `2089 + 5 = 2094` by exactly ONE: the re-entry's single new test
fn `h2_metadata_filter_gate_reads_the_threaded_dynamic_metadata`. The
`test result:` line count is unchanged at 159 because it landed in an EXISTING
binary, adding no new test target — exactly as predicted.)

Each of the 5 was re-run in ISOLATION **naming its test binary**, asserting on the
`N passed` count and never the exit code (memory
`cargo-test-p-name-false-green-filtered-out` — `0 passed; N filtered out` would
mean the test never ran; every line below reads `0 passed; 1 failed`, i.e. it
genuinely RAN and genuinely failed):

| # | test | isolation re-run (`cargo test -p differential --test <binary>`) | adjudication |
|---|---|---|---|
| 1 | `access_log_h2_rcd_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` (9.22s) | host-flake family `tcpclosebackend-ipv6-unreachable-host-flake` |
| 2 | `access_log_h2_uc_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` (2.71s) | same family |
| 3 | `access_log_rcd_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` (2.74s) | same family |
| 4 | `access_log_rf_upstream_reset` | `test result: FAILED. 0 passed; 1 failed` (2.76s) | same family |
| 5 | `admin_config_dump_server_info` | `test result: FAILED. 0 passed; 1 failed` (2.72s) | host-flake family `differential-host-bridge-ip-192-168-65-2` |

All five fail **deterministically** in isolation — the signature of the
ENVIRONMENTAL host-networking class, not the parallel-load class. The diagnosis is
MEASURED from the failure text, not assumed:

- **#1–#4** — real Envoy cannot reach the host-spawned close backend, so it reports
  an upstream **connect failure** (`UF`) where the fixture intends a reset;
  envoy-rust produces the CORRECT `UC` / connection-termination observation:

  ```
  access log byte-exact mismatch: line 0 not byte-identical:
    envoy="{\"rc\":503,\"rf\":\"UF\"}"  envoy-rust="{\"rc\":503,\"rf\":\"UC\"}"          (0061)
    envoy="{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rf\":\"UF\"}"
    envoy-rust="{\"method\":\"GET\",\"proto\":\"HTTP/2\",\"rc\":503,\"rf\":\"UC\"}"      (0069)
  ```

  The reference side never got a reset to observe. Exactly the documented
  4-witness set.

- **#5** — the host routes the backend via `192.168.65.2`, which is not in the
  fixture's allow-list (`192.168.65.254` / `172.17.0.1`), so all 18 `/clusters`
  host lines land `envoy-only`:

  ```
  text_lines diverged after allow-lists:
    envoy-only:      ["backend::192.168.65.2:35601::canary::false", … 18 entries …]
    envoy-rust-only: []
  ```

**None of the five touches this phase's surface** — verified NON-VACUOUSLY (the
first attempt at this check globbed an empty fixture name and returned a
meaningless `0`; it was redone against the resolved names, confirming 3 YAML files
were actually scanned per fixture):

```
0070-accesslog-h2-rcd-upstream-reset: yaml files=3  AccessLogFilter-arm hits=0
0069-accesslog-h2-uc-upstream-reset:  yaml files=3  AccessLogFilter-arm hits=0
0062-accesslog-rcd-upstream-reset:    yaml files=3  AccessLogFilter-arm hits=0
0061-accesslog-rf-upstream-reset:     yaml files=3  AccessLogFilter-arm hits=0
0014-admin-config-dump-server-info:   yaml files=3  AccessLogFilter-arm hits=0
```

Four are upstream-reset `%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%` witnesses and
one is the admin `/clusters` scrape; **none sets an `AccessLogFilter` of any arm**.
All five are CI-authoritative and were `success` on this exact SHA in run
`30083395623`. **Not regressions.** `grep -c 'client error (Connect)'` over the
sweep → **0**, so the Docker daemon was healthy throughout (memory
`docker-desktop-down-after-reboot-kvm-acl` did not apply).

### (c) Conformance — **no new suite required; the existing suite is GREEN**

Access-log emission gating is not codec-conformance-gated, so the phase declares no
new suite. CONFIRMED on disk rather than asserted:

- `tests/conformance/` holds exactly one suite (`h2spec/`), and
  `git diff 93ec7393…..HEAD -- tests/conformance/` is **EMPTY** (0 lines) — the
  re-entry touched no conformance file.
- `known-failures.txt` is **21 lines**, last modified by `dac3f8b` (*phase 05.2*).
  It was **NOT trimmed** (memory `h2spec-3-5-2-preface-host-sensitive`: this host
  scores invalid-preface 3.5/2 as PASS, so trimming on local evidence would break
  CI).
- The existing gate RAN inside the workspace sweep and passed:
  `test h2spec_pass_rate_gate ... ok`.

### (d) Fuzz — **CLEAN**

Run from the CRATE dir (memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`), at
the same short budget the CI step uses:

```
cd crates/envoy-config && cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30

INFO: 11780 files found in /home/esa/git/envoy-rust/crates/envoy-config/fuzz/corpus/parse_bootstrap
#11781  INITED cov: 16833 ft: 35434 corp: 3387/2293Kb exec/s: 5890 rss: 378Mb
Done 225765 runs in 107 second(s)
FUZZ_EXIT=0
```

No crash, no leak, no timeout (`grep -ciE 'crash|leak|ERROR:'` → **0**); the tree
stayed clean afterwards (`git status --porcelain` empty — new libFuzzer artifacts
are `*`-ignored).

Seed tracking re-verified (memory `fuzz-corpus-seed-gitignored-by-default`):

```
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml
crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml
$ git ls-files crates/envoy-config/fuzz/corpus/parse_bootstrap/ | wc -l
63
```

**NO `ci.yml` edit was needed or made** — `crates/envoy-config/fuzz/fuzz_targets/`
still holds exactly ONE target (`parse_bootstrap.rs`); neither this phase nor the
re-entry added a target, so memory `new-fuzz-target-needs-a-ci-yml-step` does not
apply.

### (e) build / clippy / fmt / test / deny — **ALL CLEAN**

```
cargo build --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.74s
BUILD_EXIT=0        (15 `Compiling` lines, 0 warnings, 0 errors)

cargo fmt --all -- --check
(no output)
FMT_EXIT=0

cargo clippy --workspace --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.83s
CLIPPY_EXIT=0       (14 `Checking` lines, 0 warnings, 0 errors — after a forced
                     re-analysis; see below)

cargo deny check
advisories ok, bans ok, licenses ok, sources ok
DENY_EXIT=0

cargo test --workspace --no-fail-fast
2090 passed; 5 failed  (adjudicated in (b) above)
TEST_EXIT=101
```

**The build was NOT a cached false pass.** It emitted 15 `Compiling` lines
including all four re-entry-touched crates and the binary the differential needs:

```
Compiling envoy-accesslog / envoy-config / envoy-http1 / envoy-http2 / envoy-bin
```

**Clippy's FIRST run WAS a cached green and was rejected as evidence.** It
reported `Finished … in 0.09s` with **0** `Checking` lines — cargo's fingerprints
were fresh from the build. Rather than accept it, the five changed `.rs` files
were `touch`ed and workspace clippy re-run; it then genuinely re-analysed **14**
crates (`Checking envoy-accesslog`, `Checking envoy-config`, `Checking
envoy-listener`, `Checking envoy-cluster`, `Checking envoy-filter`,
`Checking envoy-tls`, …) and still reported **zero warnings, zero errors,
exit 0**. Note `cargo clippy` prints **`Checking`**, not `Compiling` — grepping
for `Compiling` here yields a FALSE NEGATIVE (memory
`clippy-prints-checking-not-compiling`).

**`cargo deny check` was the item flagged most likely to red FRESH** (it is NOT in
the CI job set, and memory `cargo-deny-reds-on-unrelated-advisory` warns a newly
published RustSec advisory against an existing dep can red it). It came back
**clean** — `advisories ok, bans ok, licenses ok, sources ok`, exit 0. Its only
output is the same five pre-existing `license-not-encountered` WARNINGS naming
allow-list entries in `deny.toml` (`0BSD`, `BSD-2-Clause`, `MPL-2.0`,
`Unicode-DFS-2016`, `Zlib`) that no dependency uses — a policy-hygiene note, not a
check failure. **No dep patch-bump was needed.**

### (f) — NOT THIS SESSION

Gate (f) (`REVIEW.md` approved) is the SEPARATE §5 state-5 **RE-REVIEW** and is
deliberately NOT attempted here (ADR-0127: the context that wrote an artifact must
not grade it).

### Invariant spot-checks (cheap, non-negotiable)

- **ADR-0150 seam HOLDS** — `crates/envoy-accesslog/Cargo.toml` `[dependencies]`
  is `tokio`, `bytes`, `tracing`, `thiserror` and **ZERO workspace crates**. The
  re-entry's new H2 test constructs the matcher on the `envoy-config` side and
  boxes it through `Arc<dyn MetadataMatch>`, adding no reverse edge.
- **`LogFilter` derives ONLY `#[derive(Debug, Clone)]`** (`filter.rs:67`) — no
  `Eq`, no `PartialEq`.
- **The `unreachable!` lockstep guard is UNCHANGED.**
  `git diff 53893b67…..HEAD -- crates/envoy-http1/src/hcm.rs | grep -E "^[+-].*unreachable!"`
  → **0 lines added or removed across the whole phase**. *Count reconciliation:*
  a bare `grep -c` over the file returns **2**, not the `REVIEW.md` figure of 1 —
  because `:1750` is a DOC COMMENT mentioning the guard and `:1808` is the guard
  expression itself (`_ => unreachable!("validated by validate_access_logs: exactly
  one filter arm is set")`). One real guard, as reviewed. **No defect.**
- **D-3.8 holds — `#![forbid(unsafe_code)]` at 22 of 22 workspace member roots**,
  enumerated programmatically from `[workspace] members` (22 entries) with each
  root resolved to `src/lib.rs` or `src/main.rs`; **0 missing**. This is the figure
  the re-entry corrected — "14 of 14" is right for `crates/` only and UNDERSTATES
  coverage.
- **ROADMAP row `74`** — **6 cells**, status `in-progress`, correctly NOT flipped
  (the flip is the state-6 close-out).
- **`DECISIONS.md` ledger head is ADR-0155**; `grep -c "^## ADR-0156"` → **0**.
  The §6.1 split reservation stays UNFIRED.
- **82** fixture directories; **36** fixtures carrying an `access_log` stanza;
  **63** tracked `parse_bootstrap` corpus seeds; exactly **one** fuzz target;
  exactly **one** conformance suite.

### Verdict

**The §7.5 gate is GREEN: (a) ✅ (b) ✅ (c) ✅ (d) ✅ (e) ✅.** Gate (f) is the
SEPARATE state-5 RE-REVIEW.

**No real defect was found, so no further §5.2 state-3 re-entry is owed.** Every
gate item above was actually RUN in this session on the post-re-entry tree — none
is inferred from the re-entry's own runs or from CI. Three items where a shortcut
was available were deliberately refused: the cached clippy green (re-run after a
`touch`, 14 `Checking` lines), the vacuous fixture-arm grep (redone against
resolved fixture names), and the `unreachable!` count discrepancy (traced to a
doc-comment line rather than waved through). The two genuinely NEW things this
gate could show — that the re-entry's added pins pass under FULL parallel load,
and that `0081`'s third probe is stable cross-proxy — are both confirmed.

Next: the §5 state-5 **RE-REVIEW** (a SEPARATE session) — write the updated
`REVIEW.md`. Then the state-6 close-out. ROADMAP row `74` stays `in-progress`
throughout.
