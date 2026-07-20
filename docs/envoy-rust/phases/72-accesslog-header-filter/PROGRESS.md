# Phase 72 — access-log `header_filter` — PROGRESS

> §5 state-3 implementation log (`superpowers:executing-plans`, inline). One
> entry per PLAN task. TDD on every task (failing test FIRST → RED → implement →
> GREEN → commit, per D-3.1). This file's presence is the state-3 detection
> signal; the authoritative §7.5 gate is the SEPARATE state-4 session.

## Implementation-time decision (recorded before Task 4) — the `envoy-accesslog` dependency posture → **ADR-0150**

PLAN Task 4 Step 3 flagged an unresolved fork: whether `LogFilter::Header` can
carry an `envoy_config::HeaderMatcher` directly, contingent on the crate
dependency direction. **MEASURED at implementation time:**

- `crates/envoy-config/Cargo.toml:14` — `envoy-config` **depends on**
  `envoy-accesslog` (load-bearing: `envoy_accesslog::parse_format` compiles
  access-log format strings at config-validation time — the ADR-0141
  compiled-config posture).
- `crates/envoy-accesslog/Cargo.toml` — has **no** `envoy-config` dep.

Therefore the PLAN's stated DEFAULT (a) — "add `envoy-config` to
`envoy-accesslog`" — is a **dependency CYCLE** (`envoy-config → envoy-accesslog →
envoy-config`) and impossible; Cargo rejects it. ADR-0149 assumed the reverse
direction. This is a measured design surprise → **ADR-0150** fires the
resolution.

**Resolution (ADR-0150): a trait-object seam (the PLAN's "Preferred"
accesslog-owned type, realized as a trait object instead of a concrete
re-model).**

- `envoy-accesslog` defines `pub trait HeaderMatch: Send + Sync + Debug { fn
  matches(&self, headers: &[(String, String)]) -> bool; }` and
  `LogFilter::Header { matcher: Arc<dyn HeaderMatch> }`.
- `envoy-config` (already depends on `envoy-accesslog`) impls `HeaderMatch for
  HeaderMatcher` by calling the REAL phase-04.2 engine (`HeaderMatcher::matches`)
  verbatim — **zero matching duplication**, and PV-4's `mode_result ^
  invert_match` semantics are preserved automatically (no re-model to get wrong).
- `envoy-http1`'s `compile_access_log_filter` builds `LogFilter::Header { matcher:
  Arc::new(hf.header.clone()) }`.
- `LogFilter` drops BOTH `Eq` and `PartialEq` (grep-confirmed: no `==`,
  `assert_eq!`, or set/map consumer of `LogFilter` anywhere; `FileSink` — the
  sole container — derives only `Debug`). The PLAN dropped only `Eq`; a trait
  object is not `PartialEq`-derivable, and nothing needs it.

This is strictly cleaner than the concrete `HeaderPredicate` re-model the PLAN
sketched: no `regex` dep added to `envoy-accesslog`, no ~80-line matcher
duplication, and it directly honors the SPEC's core thesis ("reuse the 7-mode
engine VERBATIM, zero new matching logic"). Consequence for tests: the
accesslog-side `should_log` `Header` test uses a local `HeaderMatch` stub
(proves the gate delegates to `matcher.matches(headers)`); the real per-mode
membership + absent-drop coverage lives in `envoy-http1` (Task 9), where
`envoy_config::HeaderMatcher` is constructible.

---

## Task log

### T1 — `HeaderFilter` schema + `header_filter` oneof arm — DONE

- Added `pub struct HeaderFilter { header: HeaderMatcher }` (mirrors
  `StatusCodeFilter`; `deny_unknown_fields`, no `Default`), the
  `header_filter: Option<HeaderFilter>` arm on `AccessLogFilter`, and the
  `lib.rs` export. Added a reusable `access_log_filter_yaml(arm)` test helper.
- Tests (RED→GREEN): `header_filter_parses_into_the_arm`,
  `empty_header_filter_is_rejected` (missing required `header` → serde
  `ConfigError::Yaml`, PLAN PV-1). Both pass.
- **Task-boundary adjustment (D-3.6):** adding the field to the `deny_unknown_fields`
  `AccessLogFilter` breaks the exhaustive `set_arms` destructure in
  `validate_access_logs` (no `..`), so the minimal 3-arm destructure extension
  was folded INTO T1 to keep the commit green (the PLAN placed it in T2, which
  would have left T1 non-compiling). T2 keeps the M71-1/M71-4 test + docstring
  work. The header_filter per-arm VALIDATION delegation still lands in T3.

### T2 — M71-1 `detail` assert + header-inclusive cardinality + M71-4 docstring — DONE

- Rewrote `rejects_access_log_filter_with_both_arms` to assert `detail.contains("more
  than one")` (M71-1); added `rejects_header_filter_paired_with_another_arm` and
  `cardinality_is_checked_before_per_arm_validation` (cardinality fires before
  per-arm validation). All pass.
- Refreshed the `validate_access_logs` docstring (M71-4): item 3 no longer calls
  the >1 branch "unreachable"; added items 5 (response-flag tokens) + 6
  (header_filter matcher via `validate_header_matcher`).
- **PLAN-example trap hit (memory `plan-md-example-code-trips-clippy`):** the
  plan's `matches!(err, ...{ detail } if ...)` guard binds `detail` by-move then
  reuses `err` in the panic message → E0382; fixed with `ref detail`.

### T3 — `header_filter` validation delegation (`&mut` plumbing) — DONE

- `validate_access_logs` → `&mut [AccessLog]` (iterates `.iter_mut()`, destructure
  binds `&mut` fields); added the `if let Some(hf) = header_filter {
  validate_header_matcher(&mut hf.header)? }` delegation AFTER the cardinality
  block (so cardinality precedes per-arm, per T2's pin). Updated the sole caller
  `validate_hcm` (already `&mut hcm`) to pass `&mut hcm.access_log`.
- Tests (RED→GREEN): `header_filter_empty_name_rejected` (→ `EmptyHeaderName`),
  `header_filter_bad_regex_rejected` (→ `InvalidRegex`),
  `header_filter_safe_regex_is_compiled` (`sr.compiled.is_some()`). All 20
  `header_filter`/`access_log` tests pass; clippy clean; grep-confirmed
  `validate_access_logs` has exactly one caller.

### T4 — `LogFilter::Header` + 2nd `should_log` widening (trait-object seam) — DONE — **ADR-0150 fired**

- Implemented the ADR-0150 trait-object seam (see the decision block above):
  `HeaderMatch` trait + `LogFilter::Header { matcher: Arc<dyn HeaderMatch> }` in
  `envoy-accesslog`; exported `HeaderMatch`; `envoy-config` impls it for
  `HeaderMatcher` in `matcher.rs` (delegates to the inherent engine, no
  recursion — confirmed by `header_match_trait_delegates_to_inherent_engine`,
  incl. the PV-4 absent+invert=keep pin).
- Widened `LogFilter::should_log` + `FileSink::should_log` to
  `(&self, status, response_flags, headers: &[(String, String)])`; `Header` arm
  = `matcher.matches(headers)`. Dropped BOTH `Eq` and `PartialEq` from
  `LogFilter` (ADR-0150; no consumers).
- Updated 22 `should_log` call sites in `filter.rs` + 4 in `file_sink.rs` to the
  3-arg form (perl regex; verified 0 residual 2-arg calls). New accesslog test
  `header_filter_should_log_delegates_to_matcher` (local stub). `cargo test -p
  envoy-accesslog` = 108 pass; `envoy-config` matcher test passes.
- **No Cargo.toml dependency added** — the trait seam is precisely what avoids the
  cycle. `envoy-http1`/`envoy-http2` `should_log` call sites now need the 3rd arg
  (T5/T6).

### T5 — H1 compile 3-arm match + thread `req.headers` — DONE

- Extended `compile_access_log_filter` to a 3-tuple match; the `(None, None,
  Some(hf))` arm boxes `Arc::new(hf.header.clone())` into `LogFilter::Header`
  (ADR-0150 seam). Threaded `&req.headers` at the H1 emit gate (the same
  downstream-request-header snapshot that feeds forwarded_for/authority).
- Fixed the H1 test call sites (perl regex; 0 residual 2-arg) + added
  `header_filter: None` to both `AccessLogFilter` test constructions. New test
  `compile_access_log_filter_builds_header_arm` (kept on match, dropped on
  present-mismatch AND absent). `cargo test -p envoy-http1` = 180 pass.
- **ADR-0150 "no `LogFilter` comparison consumer" correction:** the T4 grep
  MISSED one — `runtime_key_is_rtds_inert` did `assert_eq!(inert, named)` on two
  `LogFilter` values (variables, so no literal `LogFilter` on the line). The
  ADR-0150 DECISION (drop `PartialEq`) still stands — a trait-object `Header` arm
  can't be `PartialEq` and a hand-impl would be ill-defined. Reconciled by
  comparing the inner `StatusCodeComparison` (still `PartialEq`/`Eq`) after
  matching both arms — the structural-identity assertion is preserved exactly.
