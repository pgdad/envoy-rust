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

### T6 — H2 emit gate threads `envoy_req.headers` — DONE

- Threaded `&envoy_req.headers` at the H2 emit gate (`crates/envoy-http2/src/hcm.rs:1138`)
  — the shared `compile_access_log_filter` (via `config.inner`) needs no H2
  compile change. Updated a stale doc comment to the 3-arg signature. No H2 test
  call sites of `should_log` (grep-confirmed). `cargo test -p envoy-http2` = 107
  pass; `cargo build --workspace` clean (no stale 2-arg calls anywhere).

### T7 — CF-71-1 ordering-aware settle + M71-2 doc fixes — DONE

- Added `CF71_1_SETTLE` (12s) + `suppression_settle(probes) -> Duration` (long
  settle when the LAST probe is dropped, else the cheap `CF70_3_SETTLE`); wired it
  into both `run_http1_/run_http2_access_log_byte_exact_arm` sleeps + bail
  messages. New unit test `settle_is_ordering_aware`. Differential tests compile.
- M71-2 doc fixes #1 (the `CF70_3_SETTLE` doc — no longer calls the ordering
  witness the "primary soundness guarantee") + #2 (`access_log_response_flag_filter.rs:10`
  — ADR-0146 retirement). Fix #3 (BEHAVIOR_CONTRACT §F) lands in T11.

### T8 — differential fixture `0078-accesslog-header-filter` — DONE (GREEN)

- Created `envoy.yaml`/`envoy-rust.yaml`/`expectations.yaml`/`README.md` +
  `access_log_header_filter.rs`. `header_filter { header: { name: x-log,
  string_match: { exact: "yes" } } }`; probes dropped-FIRST (`x-log: no`)/
  kept-LAST (`x-log: yes`). Built debug `envoy-bin`; `cargo test -p differential
  --test access_log_header_filter` = **1 passed** (Docker up, no flake).
- **Measured fixture-design correction (SPEC R-0.1/§2.2 boundary):** the PLAN's
  format string `H=%REQ(X-LOG)%` is BOOT-FATAL on envoy-rust — its `%REQ(NAME)%`
  operator supports only an allow-list (`:path`, `user-agent`, …) because the
  `AccessLogRecord` carries no arbitrary request-header map (SPEC §2.2 — no new
  record field this phase). Fixed by formatting only `STATUS=%RESPONSE_CODE%
  PATH=%REQ(:PATH)%` (expected line `STATUS=200 PATH=/x`). The `header_filter`
  still gates on `x-log` (it reads the raw request-header slice, NOT the record),
  so the keep/drop differential witness is intact — echoing the header value is a
  FORMATTER concern orthogonal to the FILTER this phase builds. No new ADR (this
  is the already-documented SPEC §2.2 boundary, not a new decision); documented
  in the fixture README + `.rs` doc.

### T9 — in-process coverage + PV-4/PV-5 pins + regressions — DONE

- **Membership across modes + absent-drop** (envoy-http1
  `header_filter_membership_across_modes_and_absent_drop`): exact/prefix/suffix/
  present/string_match end-to-end through `compile_access_log_filter →
  LogFilter::Header::should_log`; keep on match, drop on mismatch AND absent.
  SafeRegex membership is covered on the shared engine (matcher.rs) which the
  access-log path reuses verbatim (delegation test).
- **PV-4 pin** (matcher.rs `pv4_absent_plus_invert_is_kept_inherited_shared_engine_boundary`):
  absent+invert = KEEP (the shared-engine XOR), MEASURED-divergent from upstream
  (drops on both route+access-log), deferred to CF-72-1; pinned on BOTH the
  direct engine and the access-log `HeaderMatch` seam.
  - **[CORRECTED at §5.2 state-3 — ADR-0151, D-3.5]:** this T9 pin used
    `PresentMatch(true)` — the state-5 LIVE-PROBE MEASURED that mode is PARITY
    (upstream ALSO keeps), so it mislabeled parity as divergence. Replaced by two
    mode-scoped pins: `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream`
    (the REAL divergence — value matcher `exact`+invert+absent → KEEP vs upstream
    DROP = CF-72-1) + `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`
    (PARITY). See the §5.2 re-implementation log below.
- **PV-5 pins** (envoy-config `pv5_name_only_...` + `pv5_treat_missing_...`):
  name-only `{name}` and `treat_missing_header_as_empty` both REJECTED fail-loud
  (inherited phase-04.2 boundary, ADR-0049), deferred to CF-72-2.
- **Cardinality/detail:** strengthened `rejects_access_log_filter_with_no_variant`
  to assert `detail.contains("no filter variant")` (zero-arm branch). The both-arm
  + precedence cases are pinned in T2.
- **Regressions:** the no-`filter`-logs-every-record (`no_filter_logs_every_record`),
  `status_code`/`response_flag` unchanged, and `runtime_key_is_rtds_inert`
  (adjusted in T5) tests all remain green.

### T10 — `parse_bootstrap` corpus seed `header_filter.yaml` — DONE

- Created `crates/envoy-config/fuzz/corpus/parse_bootstrap/header_filter.yaml`
  (a full HCM bootstrap with an `access_log[].filter.header_filter`) + one
  `!`-un-ignore line in the fuzz `.gitignore`. `git ls-files` confirms it is
  tracked (memory `fuzz-corpus-seed-gitignored-by-default`). NO new fuzz target,
  NO ci.yml edit (ADR-0137 config-only-sub-message precedent). Local smoke
  `cargo +nightly fuzz run parse_bootstrap -- -runs=0` loaded the corpus clean.

### T11 — BEHAVIOR_CONTRACT `header_filter` subsection + M71-2 §F — DONE

- Added the phase-72 `header_filter` subsection (§A schema/§B decision incl. the
  ADR-0150 trait-object seam/§C PV-4 CF-72-1/§D PV-5 CF-72-2/§E mutual exclusion/
  §F fixture-0078 — documenting the actual `STATUS=200 PATH=/x` line + the
  `%REQ(NAME)%` allow-list formatter boundary). Updated the phase-71 §E to the
  3-arm reality. Fixed M71-2 doc phrase #3 (the §F "CF-70-3 ordering witness"
  phrase → ADR-0146/0147 framing) — all three M71-2 phrases now consumed.

### T12 — §7.5 pre-flight gate dry-run — DONE (all green)

Self-check (NOT the authoritative state-4 gate — that is a SEPARATE session per §5.1):

- `cargo fmt --all -- --check`: clean AFTER a fixup — rustfmt wrapped the widened
  `should_log`/`FileSink::should_log` signatures (the per-task commits deferred
  fmt; memory `envoy-rust-state4-ci-first-execution`). `cargo fmt --all` applied.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean
  AFTER removing 2 unused `use envoy_accesslog::HeaderMatch as _;` imports in the
  matcher.rs tests (trait-object/inherent calls need no import).
- `cargo build --workspace --all-targets`: clean. `cargo build -p envoy-bin`: clean.
- `cargo test --workspace --lib --bins --no-fail-fast`: ALL green (envoy-config
  628, envoy-http1 181, envoy-http2 107, envoy-accesslog 108, +others; exit 0).
- `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`.
- Differentials `0076`/`0077`/`0078`: all GREEN — `0076` (dropped-LAST) now pays
  the 12s `CF71_1_SETTLE` (13.3s) with no regression; `0077`/`0078` (kept-LAST)
  pay the cheap settle. Fuzz corpus loads clean (T10).

The full Docker differential suite + conformance is the authoritative state-4
run (a separate session); this dry-run exercised the touched surface only.

---

## §5 STATE-4 VERIFICATION (`superpowers:verification-before-completion`) — GREEN

> The AUTHORITATIVE full §7.5 gate, run in its OWN session per §5.1 (the state-3
> log above is state-3's; this section is the state-4 gate). Base = the state-3
> head commit `510d6118992e6edca083ca38533a7bfb416ca11a`, CI-confirmed `success`
> (run `29778954239`, jobs `build + test + lint` + `fuzz` both `success`). `git
> fetch` showed no sibling had advanced; no `REVIEW.md`. DEBUG `envoy-bin` rebuilt
> first (memory `differential-harness-uses-debug-envoy-bin`). Outputs quoted below.

**Gate (e) — workspace static checks — all CLEAN:**

- `cargo fmt --all -- --check` → **exit 0** (no diff).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → **exit 0**
  (`Finished \`dev\` profile … in 0.09s`; zero warnings).
- `cargo build --workspace --all-targets` → **exit 0**.
- `cargo deny check` → **exit 0** — `advisories ok, bans ok, licenses ok, sources ok`.

**Gate (e) — `cargo test --workspace --no-fail-fast` — adjudicated GREEN** (memory
`local-red-set-varies-run-to-run` + `never-pipe-verification-runs-through-tail`;
full output redirected, never `tail`):

```
2056 passed; 6 failed  (exit 101)
```

All **6 REDs are documented host-flakes** — CI-authoritative, NOT regressions.
Each adjudicated in isolation naming the target binary (memory
`cargo-test-p-name-false-green-filtered-out`):

| Failing test | Documented family | Isolation verdict |
|---|---|---|
| `differential::tests::wait_accept_ready_times_out_for_closed_socket` | `wait-accept-ready-closed-socket-port-reuse-flake` | **flips GREEN in isolation** (`1 passed`) — parallel-load flake |
| `access_log_rcd_upstream_reset` | `tcpclosebackend-ipv6-unreachable-host-flake` | deterministic environmental RED — sig `remote_address:[fdc4:f303:9324::254]` / `immediate_connect_error:_Network_is_unreachable` (real Envoy `UF`, envoy-rust `UC`) |
| `access_log_rf_upstream_reset` | `tcpclosebackend-ipv6-unreachable-host-flake` | same IPv6-unreachable root cause |
| `access_log_h2_rcd_upstream_reset` | `tcpclosebackend-ipv6-unreachable-host-flake` | same (H2 close-backend) |
| `access_log_h2_uc_upstream_reset` | `tcpclosebackend-ipv6-unreachable-host-flake` | same (H2 close-backend) |
| `admin_config_dump_server_info` | `differential-host-bridge-ip-192-168-65-2` | deterministic environmental RED — `/clusters` envoy-only per-host stats `backend::192.168.65.2:PORT::…` (this host's bridge IP, not allow-listed) |

Cross-check (memory `local-red-set-varies-run-to-run`): local `2056 passed + 6
failed = 2062`; CI is `success` on this exact SHA (all 6 pass in CI's env) →
`local passed+failed == CI passed`. Consistent. None of the 6 touch the
`header_filter` surface — the reset four diverge only on the backend-connection
`rcd`/`rf` VALUES (orthogonal to the FILTER), the admin one on host-bridge
per-host stat lines.

**Gate (a) — new/changed differential fixtures — GREEN (isolation):**

- `access_log_header_filter` (**fixture 0078**, the NEW header_filter witness) →
  **exit 0** (`test result: ok. 1 passed`).
- `access_log_response_flag_filter` (**0076 dropped-LAST → 12s `CF71_1_SETTLE`;
  0077 kept-LAST → cheap settle**, the CF-71-1-touched pair) → **exit 0** (`1 passed`).
- `differential::…::settle_is_ordering_aware` (the ordering-aware settle unit
  pin) → **exit 0** (`1 passed`).

**Gate (b) — pre-existing differentials still green:** the full-suite `cargo test
--workspace` above (2056 passed) IS the pre-existing suite; the widened
`should_log` signature threading `req.headers`/`envoy_req.headers` at both HCM
gates introduced no regression (the only REDs are the documented host-flakes).

**Gate (c) — conformance at threshold:** NO protocol-conformance surface this
phase (h2spec/h3spec unchanged; `known-failures.txt` untouched — memory
`h2spec-3-5-2-preface-host-sensitive`). CI job `build + test + lint` (which runs
conformance) = `success` on `510d611`.

**Gate (d) — fuzz short-budget run — CLEAN:** `cd crates/envoy-config && cargo
+nightly fuzz run parse_bootstrap -- -max_total_time=30` → **exit 0** —
`Done 23747 runs in 92 second(s)`, `cov: 16406`, **no crashes / no leaks / no
panics** (the new `header_filter.yaml` corpus seed rides the EXISTING
`parse_bootstrap` target — NO new target, NO ci.yml edit).

**Gate (f) — `REVIEW.md`:** NOT this session — that is the §5 state-5 code-review
(a SEPARATE session per §5.1).

**Verdict:** the phase-72 §5 gate is **GREEN**. No MEASURED surprise → no new ADR
(next-available ADR-0151 unspent). STATE advanced to §5 state-5; ROADMAP row `72`
stays `in-progress` (no flip until state-6 close-out). Docs-only close (the gate
needed no code fixups). Next: the §5 state-5 code-review.

---

## §5.2 STATE-3 RE-IMPLEMENTATION (`superpowers:systematic-debugging` → `test-driven-development`) — addresses `REVIEW.md` (NOT APPROVED)

> Its OWN session per §5.1. `REVIEW.md` (state-5 code-review) was NOT APPROVED —
> one Important MUST-FIX (F-1) + one Important (F-2). Per BOOTSTRAP §5.2 a review
> with issues re-enters step 3 (implementation), so this is state-3, not a
> re-verify. Base = the state-5 review commit `88dd2460e436f417eb935b7e15a79ab6de32c457`,
> CI `completed`/`success` on the FULL 40-char SHA (run `29789909864`). `git
> fetch` showed no sibling had advanced. **NO runtime code change** — F-1/F-2/F-3
> are test-accuracy + coverage + documentation fixes on already-correct,
> already-MEASURED behavior; TDD's RED step is honored via **mutation checks**
> (each new/corrected pin is proven non-vacuous by breaking the underlying
> engine/threading and watching the pin go RED, then reverting).

### F-1 — [Important MUST-FIX] the PV-4 divergence pin exercised the NON-divergent mode — DONE

The state-4 pin `pv4_absent_plus_invert_is_kept_inherited_shared_engine_boundary`
(`matcher.rs:397`) used `PresentMatch(true)` and labeled it "diverges from
upstream — CF-72-1". The state-5 LIVE-PROBE MEASURED that `present_match`+invert+
absent is **PARITY** (upstream ALSO keeps); only a VALUE matcher (`exact`/`prefix`/
`suffix`/`safe_regex`/`range`/`string_match`)+invert+absent diverges (envoy-rust
KEEP vs upstream DROP). Fix:

- **Replaced the mislabeled pin with two mode-scoped pins** (`matcher.rs`):
  - `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream` — `exact`+
    invert+absent → KEEP, commented as MEASURED-divergent (upstream DROPS) =
    CF-72-1, on BOTH the direct engine and the `HeaderMatch` seam.
  - `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` —
    `present_match`+invert+absent → KEEP, commented as PARITY (upstream keeps too);
    a future CF-72-1 fixer MUST preserve this KEEP.
  - The `header_match_trait_delegates_to_inherent_engine` invert leg now exercises
    the VALUE-matcher divergence through the trait object (was `PresentMatch`).
- **TDD RED-equivalent (mutation check):** applied the exact naive uniform
  CF-72-1 "fix" the review warns about (`if value.is_none() && invert_match {
  return false; }` at `matcher.rs:51`) with a FORCED rebuild (`Compiling
  envoy-config` confirmed — memory `mutation-check-needs-forced-rebuild`). BOTH
  new pins went **RED** — proving non-vacuity AND that the parity pin catches the
  mode-breaking fix (its exact purpose). Reverted → both GREEN (`34 passed`).
- **Docs:** fired **ADR-0151** (the corrected mode-scoped characterization — does
  NOT supersede ADR-0149, whose decision stands; only its CF-72-1 *characterization*
  is refined). Rewrote `BEHAVIOR_CONTRACT.md` §C to the mode-dependent truth.
  Strike-corrected the historical `PLAN.md` PV-4 note + the T9 pin note above
  (D-3.5). **CF-72-1 re-scoped** to "value-based matcher + invert + absent".
- **NO runtime code change** — envoy-rust's value-matcher absent+invert = KEEP
  stays the correctly-scoped CF-72-1 boundary; fixture `0078` (non-inverted) is
  unaffected.

### F-2 — [Important] H2 `header_filter` header-slice threading was UNASSERTED — DONE

`crates/envoy-http2/src/hcm.rs:1138` threads `&envoy_req.headers` into the widened
`should_log`, but no H2 test exercised `header_filter` keep/drop. Added
`h2_header_filter_keeps_match_drops_mismatch_and_absent` (mirrors
`h2_response_flag_filter_suppresses_no_flag`): a `LogFilter::Header { exact "yes"
on x-log }` sink KEEPS `GET /x` with `x-log: yes` (1 line, `access_logs_total`
ticks) and DROPS both present-mismatch (`x-log: no`) and absent-header requests
(0 lines). A small `h2_header_filter_roundtrip` helper drives H2 requests with
custom headers.

- **TDD RED-equivalent (mutation check):** replaced `&envoy_req.headers` with
  `&[]` at the H2 emit gate (forced rebuild confirmed) → the test went **RED**
  (keep leg dropped: log `""`), proving it genuinely exercises the threaded slice
  (F-2's exact concern). Reverted → GREEN (`1 passed`).
- The full H2 differential fixture stays deferred = **M71-6** (unchanged).

### F-3 — [Minor, opportunistic] multi-sink mixed-filter composition (M71-5) — DONE

Added `two_sinks_with_mixed_filters_gate_independently` (`envoy-http1` hcm.rs):
one HCM with TWO file sinks — sink A `header_filter { exact "yes" on x-log }`,
sink B `status_code_filter { EQ 200 }`. Three `GET /x` requests (all → 200) drive
sink A to KEEP only the 1 matching request and sink B to KEEP all 3 — the exact
shape the state-5 LIVE-PROBE MEASURED byte-exact parity for (REVIEW.md Probe 1).
The 1-vs-3 line-count distinction pins per-sink independence (no cross-sink
leakage of the `req.headers` slice). **Closes M71-5** (was a live carry-forward).
GREEN (`1 passed`).

### F-4 — [Minor] stale "two arms" comment — DONE

`crates/envoy-config/src/bootstrap.rs:5169` now reads "three arms (phase 72 added
`header_filter`)".

### F-5 / F-6 — no action (per REVIEW.md)

F-5 (safe_regex/range through the access-log seam) is transitively covered (the
inherent engine + the proven trait delegation); F-6 (SPEC/PLAN pre-change
`H=%REQ(X-LOG)%` format string) is expected historical planning drift. Neither
requires a change.

### Local verification (before commit; the AUTHORITATIVE full §7.5 gate is the SEPARATE state-4 re-verification)

- `cargo test -p envoy-config` (F-1 pins) — GREEN.
- `cargo test -p envoy-http2` (F-2) — GREEN.
- `cargo test -p envoy-http1` (F-3) — GREEN.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo
  fmt --all -- --check` — clean (outputs at commit time).

Next: the §5 state-4 RE-VERIFICATION (re-run the full §7.5 gate — its OWN session
per §5.1). ROADMAP row `72` stays `in-progress`.
