# Phase 75.1 — `HeaderMatcher` absence semantics: the MODE-SCOPED engine fix + the ROUTE-path differential witness — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the UNIFORM `mode_result ^ self.invert_match` at `crates/envoy-config/src/matcher.rs:52` with the MEASURED mode-scoped absence rule, closing two silent runtime divergences in the single `HeaderMatcher` engine that five subsystems share, and witness it cross-proxy with one new backend-free route-path differential fixture (`0083`).

**Architecture:** The entire behavioral change is ONE expression inside `HeaderMatcher::matches`. Everything else in this plan is coverage: in-process tests that pin the new rule, in-process tests that prove it propagates to all five consumers, one differential fixture that witnesses it against upstream Envoy, and documentation that stops asserting the old rule. **No call site is edited. No config surface is added.**

**Tech Stack:** Rust (toolchain pinned to `1.95.0` by `rust-toolchain.toml`), `serde`/`serde_yaml` for config, `tokio` for the async differential tests, `testcontainers` for the upstream-Envoy side of the differential harness. Upstream reference is `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`), pinned by `docs/envoy-rust/ENVOY_TARGET.md`.

---

## Global Constraints

Every task's requirements implicitly include this section.

- **The MEASURED rule this phase implements** (from `SPEC.md` §2.1; measured cross-proxy on both proxies at the phase-75 state-2 PLAN-write):
  ```
  present := the named header is present in the request
             (name matched case-insensitively; an EMPTY VALUE still counts as PRESENT)

  if mode is present_match(want):
          result = (present == want) XOR invert_match
  else if not present:
          result = false                    # <-- invert_match is NOT applied
  else:
          result = mode_matches(value) XOR invert_match
  ```
- **THE GUARD (P1).** `present_match: true` + `invert_match: true` + ABSENT header is **MEASURED PARITY** — both proxies KEEP. A naive uniform "absent ⇒ DROP" fix BREAKS it and mints a NEW divergence. The fix MUST be mode-scoped. Guard tests: `crates/envoy-config/src/matcher.rs:425` and `:463`; the latter's own doc comment instructs the fixer to preserve it.
- **Doctrine D-3.1 / `superpowers:test-driven-development`:** every task writes its failing test FIRST and runs it to observe the failure before writing implementation.
- **Doctrine D-3.4:** every artifact must be readable by a stranger with zero prior context. Never write "as discussed earlier".
- **Doctrine D-3.5:** `docs/envoy-rust/DECISIONS.md` is append-only. NEVER edit a landed ADR.
- **Doctrine D-3.8:** every crate root keeps `#![forbid(unsafe_code)]`. This phase adds no `unsafe`.
- **The parent `docs/envoy-rust/phases/75-headermatcher-absence-parity/SPEC.md` is a FROZEN artifact.** Do not edit it. Do not write a `PLAN.md` for parent phase 75.
- **Do NOT start sub-phase 75.2** (fixtures `0084`/`0085`, the `present_match`-polarity contract subsection, the CF-75-1 / CF-72-2 contract rows, the M74-31 five-site fold). 75.2's fixtures assert POST-fix behavior and are a separate ROADMAP row that depends on this one.
- **Fixture numbering:** the only new fixture in this sub-phase is **`0083`**. `0084` and `0085` belong to 75.2.
- **No new fuzz target, no new fuzz corpus seed, no `.github/workflows/ci.yml` edit.** Confirmed in `SPEC.md` §9: this sub-phase introduces no parser, codec or filter and adds no config surface. `HeaderMatcher` already carries `name` + `mode` + `invert_match`, already deserializes `present_match: false` (`crates/envoy-config/src/bootstrap.rs:3236-3239`) and already validates it (`validate_header_matcher`).
- **Never trim `tests/conformance/h2spec/known-failures.txt`** (currently **21** lines). This host scores h2spec 3.5/2 as PASS, so trimming on local evidence would break CI.
- **Never weaken an existing fixture.** The corpus is **82** fixtures before this phase, **83** after.
- **Doc-comment hazard** (memory `mechanical-fanout-scripts-corrupt-doc-comments`): `cargo fmt` does NOT reflow `///` / `//!` / `//` lines, so nothing catches a mis-wrapped or semantically-backwards comment. Wrap-check every touched comment BY HAND (keep `///` lines ≤ ~80 columns) and grep the commit's `+` lines for `///` before committing.
- **Before ANY local differential run:** `cargo build -p envoy-bin`. The harness executes `target/debug/envoy-bin`, not a release build; a stale binary silently mis-reports (memory `differential-harness-uses-debug-envoy-bin`).
- **Never pipe a verification run through `tail`** — it truncates the `failures:` block and destroys the failing test names. Redirect full output to a file (memory `never-pipe-verification-runs-through-tail`).

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/envoy-config/src/matcher.rs` | **Modify** `:22-53` (engine), `:15-21` + `:44-46` + `:61-63` + `:330` + `:439` (comments), `:342-346` + `:448-459` + `:503` (amended tests), `:425` + `:463` + `:348` (strengthened guards); **add** the engine matrix tests | The one behavioral change, plus the engine-level coverage |
| `crates/envoy-config/src/bootstrap.rs` | **Modify** `:3119-3121`, `:3142-3143`, `:1704` (doc comments only) | Config-surface doc comments that state the old rule |
| `crates/envoy-accesslog/src/filter.rs` | **Modify** `:135-138` (doc comment only) | The access-log arm's comment asserting the old XOR |
| `crates/envoy-http1/src/hcm.rs` | **Add** tests only | Route-walker propagation + the ADR-0150 trait-object propagation |
| `crates/envoy-http2/src/hcm.rs` | **Add** tests only | H2-route propagation via `envoy_http1::hcm::resolve_route` |
| `crates/envoy-filter/src/rbac.rs` | **Add** tests only | HTTP RBAC propagation |
| `crates/envoy-filter/src/fault.rs` | **Add** tests only | Fault header-gate propagation |
| `crates/envoy-filter/src/jwt_authn.rs` | **Add** tests only | JWT-authn rule-matching propagation |
| `tests/fixtures/0083-headermatcher-absence-parity/` | **Create** 4 files | The cross-proxy witness |
| `tests/differential/tests/headermatcher_absence_parity.rs` | **Create** | The fixture's test entrypoint |
| `tests/fixtures/0078-accesslog-header-filter/README.md` | **Modify** `:69-73` | Marks CF-72-1 CLOSED |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | **Modify** `:2357-2377` (§C rewrite), `:1878-1880` (C2) | The contract stops recording D1 as accepted |
| `docs/envoy-rust/DECISIONS.md` | **UNTOUCHED by state-3** | ADR-0159 already landed at the state-2 PLAN-write (house precedent ADR-0153/0155/0158). Append-only; do not edit |
| `docs/envoy-rust/PROGRESS.md` → `docs/envoy-rust/phases/75.1-.../PROGRESS.md` | **Create/append** | Per-task running log (state-3 artifact) |

**No file is created under `crates/`.** Every consumer test lands in the existing `mod tests` of the file that owns the consumer, because three of the five consumers are private or crate-private:

- `crates/envoy-filter/src/rbac.rs` — `RuntimeMatcher` and `eval` are `pub(crate)`.
- `crates/envoy-filter/src/fault.rs` — `header_gate_matches` is private.
- `crates/envoy-http1/src/hcm.rs` — `compile_access_log_filter` is private.
- `crates/envoy-accesslog/src/filter.rs` — cannot construct a real `envoy_config::HeaderMatcher` (ADR-0150 forbids the dependency edge), so the real engine only reaches `LogFilter::Header` from `envoy-http1`.

---

## Size re-derivation (§6.1 gate for THIS sub-phase)

Re-derived at this PLAN-write against the live tree, per PV-8 discipline. The parent split (ADR-0157) is settled and is NOT reopened.

| Task | Area | Net LoC |
|---|---|---|
| 1 | In-process engine matrix (RED) | ~185 |
| 2 | Engine fix + 3 amended tests + 3 strengthened guards | ~80 |
| 3 | Mutation check (worktree, reverted) | 0 |
| 4 | Eight doc comments + the in-source citation fix | ~45 |
| 5 | Route-walker propagation (H1 + H2) | ~75 |
| 6 | HTTP RBAC propagation | ~35 |
| 7 | Fault header-gate propagation | ~35 |
| 8 | JWT-authn propagation | ~45 |
| 9 | Access-log trait-object propagation | ~45 |
| 10 | Fixture `0083` — the two configs | ~240 |
| 11 | Fixture `0083` — `expectations.yaml` (22 probes) | ~230 |
| 12 | Fixture `0083` — README + test entrypoint | ~140 |
| 13 | `BEHAVIOR_CONTRACT.md` §C rewrite + C2 + citations + `0078` README | ~90 |
| | **Total** | **~1245 net LoC / 13 tasks** |

**Verdict: UNDER the ~1500 LoC / ~25 task gate. No further split.** This lands within 3% of the `SPEC.md` §12 projection of ~1210, and the fixture line (~610 across tasks 10-12) is measured against on-disk comparables: `0007-http1-direct-response` totals 183 lines for ONE matcher and TWO probes; `0017-http-filter-rbac` totals 347; `0083` carries EIGHT matchers and TWENTY-TWO probes.

---

## Pre-flight results (already run at the PLAN-write — do not redo, but do reproduce)

These were measured in a throwaway `git worktree` off `49b96ec2d1aa4d00bea6529e3fb2f0d293db0219` at this PLAN-write. They are recorded so the implementer knows the plan's literal Rust compiles, lints and behaves as claimed.

1. **The Task-2 engine restructure is clippy-clean.** `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` → `Finished` with zero warnings, after `touch`ing `matcher.rs` to force re-analysis. The run emitted `Checking envoy-config`, which is the proof it really re-analysed (memory `clippy-prints-checking-not-compiling`: clippy prints `Checking`, NOT `Compiling` — grepping for `Compiling` gives a FALSE NEGATIVE).
2. **The engine restructure + the three test amendments is GREEN.** `cargo test -p envoy-config --lib matcher` → `59 passed; 0 failed`. Critically, the P1 guard `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` (`matcher.rs:463`) PASSES under the fix.
3. **The Task-3 mutation reproduces a real, semantic RED.** Hoisting the `(_, None) => return false` arm ABOVE the `PresentMatch` arm (== the naive uniform absent-DROP the SPEC warns about) turns **three** tests RED: `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` (`:463`), `invert_match_inverts_present_match_result` (`:425`), and `present_match_false_returns_true_when_absent` (`:348`) → `56 passed; 3 failed`. These are assertion failures, not container-startup noise (memory `mutation-red-needs-unmutated-control`).

---

## Task 1: The in-process engine matrix (RED)

This is the coverage whose absence let divergence **D2** survive in-tree since phase 04.2. It is written FIRST and MUST fail on the unfixed engine.

**Files:**
- Modify/Test: `crates/envoy-config/src/matcher.rs` (append inside the existing `#[cfg(test)] mod tests`, after `value_matcher_present_match_resolved_semantics` which ends at `:523`)

**Interfaces:**
- Consumes: the existing test helpers already in that module — `h(name, value) -> (String, String)` (`:182`), `compile(pattern) -> SafeRegex` (`:186`), `hm(name, mode) -> HeaderMatcher` (`:193`), `hm_inverted(name, mode) -> HeaderMatcher` (`:201`).
- Produces: nothing consumed by later tasks. Task 2 makes this task's test pass.

- [ ] **Step 1: Write the failing test**

Append to `crates/envoy-config/src/matcher.rs`, inside `mod tests`, immediately before the closing `}` of that module:

```rust
    /// Phase 75.1 (ADR-0159): the full ABSENCE-SEMANTICS matrix for the shared
    /// engine — seven modes × {absent, present-matching, present-non-matching}
    /// × {invert, no-invert}, plus the empty-header-VALUE control.
    ///
    /// Every expectation below is the MEASURED upstream
    /// `envoyproxy/envoy:v1.33.0` verdict (`SPEC.md` §2.3, a 13-probe × 5-variant
    /// backend-free route matrix driven live against BOTH proxies). The rule:
    ///
    /// * `present_match(want)` is the ONLY mode evaluated with the header
    ///   ABSENT: `(present == want) ^ invert_match`.
    /// * every VALUE mode returns `false` when the header is absent —
    ///   `invert_match` is NOT applied to a missing header.
    /// * an EMPTY header VALUE counts as PRESENT.
    ///
    /// This matrix is the coverage whose absence let the `present_match: false`
    /// divergence (D2) survive from phase 04.2 to phase 75.1.
    #[test]
    fn absence_semantics_matrix_matches_measured_upstream() {
        let string_exact = |lit: &str| {
            HeaderMatcherMode::StringMatch(StringMatcher {
                mode: StringMatcherMode::Exact(lit.into()),
                ignore_case: false,
            })
        };

        // (label, mode, a value that MATCHES the mode, a value that does NOT)
        let value_modes: Vec<(&str, HeaderMatcherMode, &str, &str)> = vec![
            (
                "exact_match",
                HeaderMatcherMode::ExactMatch("v".into()),
                "v",
                "zzz",
            ),
            (
                "prefix_match",
                HeaderMatcherMode::PrefixMatch("v".into()),
                "v1",
                "zzz",
            ),
            (
                "suffix_match",
                HeaderMatcherMode::SuffixMatch("v".into()),
                "1v",
                "zzz",
            ),
            (
                "safe_regex_match",
                HeaderMatcherMode::SafeRegexMatch(compile("^v$")),
                "v",
                "zzz",
            ),
            (
                "range_match",
                HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 10 }),
                "5",
                "zzz",
            ),
            ("string_match", string_exact("v"), "v", "zzz"),
        ];

        for (label, mode, hit, miss) in value_modes {
            // --- no invert ---
            let m = hm("x-a", mode.clone());
            assert!(m.matches(&[h("x-a", hit)]), "{label}: present+matching");
            assert!(
                !m.matches(&[h("x-a", miss)]),
                "{label}: present+non-matching"
            );
            assert!(!m.matches(&[]), "{label}: absent");

            // --- invert ---
            let mi = hm_inverted("x-a", mode.clone());
            assert!(
                !mi.matches(&[h("x-a", hit)]),
                "{label}+invert: present+matching"
            );
            assert!(
                mi.matches(&[h("x-a", miss)]),
                "{label}+invert: present+non-matching"
            );
            // THE D1 CELL. Upstream DROPS: a missing header is an unconditional
            // value no-match that `invert_match` does NOT resurrect.
            assert!(
                !mi.matches(&[]),
                "{label}+invert: ABSENT must be false — invert_match is NOT \
                 applied to a missing header (D1 / CF-72-1)"
            );

            // --- empty VALUE counts as PRESENT, so it takes the value path ---
            assert!(
                !m.matches(&[h("x-a", "")]),
                "{label}: empty value is PRESENT and fails the value match"
            );
            assert!(
                mi.matches(&[h("x-a", "")]),
                "{label}+invert: empty value is PRESENT, so invert DOES apply"
            );
        }

        // --- present_match: the ONLY mode evaluated on an absent header ---
        let pm_true = hm("x-a", HeaderMatcherMode::PresentMatch(true));
        assert!(pm_true.matches(&[h("x-a", "v")]), "present(true): present");
        assert!(
            pm_true.matches(&[h("x-a", "")]),
            "present(true): EMPTY VALUE counts as PRESENT"
        );
        assert!(!pm_true.matches(&[]), "present(true): absent");

        let pm_true_inv = hm_inverted("x-a", HeaderMatcherMode::PresentMatch(true));
        assert!(
            !pm_true_inv.matches(&[h("x-a", "v")]),
            "present(true)+invert: present"
        );
        // THE P1 GUARD CELL — MEASURED PARITY on both proxies. A uniform
        // "absent => DROP" fix breaks this and mints a NEW divergence.
        assert!(
            pm_true_inv.matches(&[]),
            "present(true)+invert: ABSENT must stay KEEP (P1 — MEASURED PARITY)"
        );

        // D2: upstream `present_match: false` means the header must be ABSENT.
        let pm_false = hm("x-a", HeaderMatcherMode::PresentMatch(false));
        assert!(
            !pm_false.matches(&[h("x-a", "v")]),
            "present(false): a PRESENT header must NOT match (D2)"
        );
        assert!(
            !pm_false.matches(&[h("x-a", "")]),
            "present(false): an EMPTY VALUE is PRESENT, so it must NOT match (D2)"
        );
        assert!(pm_false.matches(&[]), "present(false): absent matches");

        let pm_false_inv = hm_inverted("x-a", HeaderMatcherMode::PresentMatch(false));
        assert!(
            pm_false_inv.matches(&[h("x-a", "v")]),
            "present(false)+invert: present matches (D2, inverted)"
        );
        assert!(
            !pm_false_inv.matches(&[]),
            "present(false)+invert: absent does not match"
        );

        // The name match stays case-insensitive under the restructure.
        assert!(
            hm("X-A", HeaderMatcherMode::PresentMatch(true)).matches(&[h("x-a", "v")]),
            "header NAME matching stays case-insensitive"
        );
    }
```

This test needs `HeaderMatcherMode` to be `Clone` (it already derives `Clone` at `crates/envoy-config/src/bootstrap.rs:3125`) and `Int64Range` / `StringMatcher` / `StringMatcherMode`, all already imported by the module (`:179-180`).

- [ ] **Step 2: Run the test to verify it fails**

```bash
cargo test -p envoy-config --lib matcher::tests::absence_semantics_matrix_matches_measured_upstream 2>&1 | tee /tmp/t1-red.txt
```

Expected: **FAIL**. The first assertion to fire is the D1 cell —
`exact_match+invert: ABSENT must be false — invert_match is NOT applied to a missing header (D1 / CF-72-1)` — because the unfixed engine computes `false ^ true` = `true`.

Confirm the run actually rebuilt: `grep -c 'Compiling envoy-config' /tmp/t1-red.txt` must be ≥ 1 (memory `mutation-check-needs-forced-rebuild`; for `cargo test`/`build` the token is `Compiling`, for `clippy` it is `Checking`). If it is 0, `touch crates/envoy-config/src/matcher.rs` and re-run.

- [ ] **Step 3: No implementation in this task**

Task 1 is a RED-only task. The implementation is Task 2. Do NOT edit the engine here.

- [ ] **Step 4: Record the RED in PROGRESS.md**

Create `docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/PROGRESS.md` if it does not exist, and append the verbatim failure output from `/tmp/t1-red.txt` under a `## Task 1 — engine matrix (RED)` heading.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-config/src/matcher.rs docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/PROGRESS.md
git commit -m "phase 75.1 task 1: RED — in-process absence-semantics matrix for the shared HeaderMatcher engine"
```

> The tree is intentionally RED after this commit. Task 2 makes it green. This is the TDD RED step required by doctrine D-3.1; it is the only commit in this sub-phase that is permitted to be red, and it must be followed immediately by Task 2.

---

## Task 2: The mode-scoped engine fix, the three amended tests, the three strengthened guards

**Files:**
- Modify: `crates/envoy-config/src/matcher.rs:22-53` (the engine), `:342-346`, `:348-351`, `:425-429`, `:432-460`, `:463-486`, `:489-504`

**Interfaces:**
- Consumes: Task 1's matrix test (must go green).
- Produces: the corrected `HeaderMatcher::matches` semantics that Tasks 5-9 assert propagate, and that fixture `0083` (Tasks 10-12) witnesses cross-proxy. The public signature is UNCHANGED: `pub fn matches(&self, headers: &[(String, String)]) -> bool`.

- [ ] **Step 1: Replace the engine**

In `crates/envoy-config/src/matcher.rs`, replace the whole body of `pub fn matches` (lines `22-53`, from `pub fn matches` through the closing `}` of the method) with:

```rust
    pub fn matches(&self, headers: &[(String, String)]) -> bool {
        let value = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&self.name))
            .map(|(_, v)| v.as_str());

        let mode_result = match (&self.mode, value) {
            // present_match is the ONLY mode that evaluates with the header
            // ABSENT, and the only one an absent header carries into
            // `invert_match`. present_match: true → must be PRESENT;
            // present_match: false → must be ABSENT.
            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
            // Every VALUE mode short-circuits on an absent header WITHOUT
            // reaching the XOR below. Order matters: this arm must sit after
            // the present_match arm and before every value arm.
            (_, None) => return false,
            (HeaderMatcherMode::ExactMatch(lit), Some(v)) => v == lit.as_str(),
            (HeaderMatcherMode::PrefixMatch(lit), Some(v)) => v.starts_with(lit.as_str()),
            (HeaderMatcherMode::SuffixMatch(lit), Some(v)) => v.ends_with(lit.as_str()),
            (HeaderMatcherMode::SafeRegexMatch(sr), Some(v)) => sr
                .compiled
                .as_ref()
                .expect("validator ensured HeaderMatcher SafeRegex compiled")
                .is_match(v),
            (HeaderMatcherMode::RangeMatch(r), Some(v)) => {
                v.parse::<i64>().is_ok_and(|n| n >= r.start && n < r.end)
            }
            (HeaderMatcherMode::StringMatch(sm), Some(v)) => sm.matches(v),
        };

        mode_result ^ self.invert_match
    }
```

**Why this shape and not another** (recorded in ADR-0159, Task 13): a single exhaustive tuple `match` over `(&self.mode, value)` puts the entire semantic content into ARM ORDER, which the compiler checks for exhaustiveness. The two rejected alternatives were (a) an early `if let HeaderMatcherMode::PresentMatch(..)` guard followed by a `let Some(v) = value else { return false }` — which splits one rule across three statements — and (b) a nested match with an `unreachable!()` PresentMatch arm, which introduces a panic path into a request-hot function for no benefit. This shape was verified clippy-clean at the PLAN-write.

Note the `RangeMatch` arm changes from `value.and_then(|v| v.parse::<i64>().ok()).is_some_and(...)` to `v.parse::<i64>().is_ok_and(...)`: a mechanical consequence of `v` already being a `&str` after the destructure. `Result::is_ok_and` is stable since Rust 1.70; the toolchain pin is 1.95.0.

- [ ] **Step 2: Amend the THREE divergence-encoding tests**

These three tests currently assert the OLD, divergent behavior. They must be amended in the SAME commit as the engine, or the build is red. Per `SPEC.md` §4.1 item 3 all three are renamed to describe PARITY rather than divergence.

**(a)** Replace `present_match_false_returns_true_when_present` (`:341-346`) with:

```rust
    #[test]
    fn present_match_false_requires_the_header_to_be_absent() {
        // MEASURED (SPEC §2.3 probe p12, both proxies): upstream
        // `present_match: false` means the header must be ABSENT. Before phase
        // 75.1 this test asserted the opposite ("no presence requirement,
        // always true") and was the in-tree test that PINNED divergence D2.
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(false));
        assert!(!m.matches(&[h("authorization", "Bearer x")]));
    }
```

**(b)** In `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream` (`:431-460`), rename it, replace its 12-line comment (`:433-445`) and flip both assertions (`:448-451` and `:456-459`):

```rust
    #[test]
    fn pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream() {
        // MEASURED (ADR-0151, re-measured at the phase-75 state-2 PLAN-write on
        // BOTH proxies): a VALUE-based matcher
        // (exact/prefix/suffix/regex/range/string_match) with `invert_match` +
        // an ABSENT header DROPS on upstream `envoyproxy/envoy:v1.33.0` — a
        // missing header is an unconditional value no-match that `invert_match`
        // does NOT resurrect. Until phase 75.1 the shared engine
        // (matcher.rs:52) applied `mode_result ^ invert_match` UNIFORMLY and
        // KEPT it, which was carry-forward CF-72-1; phase 75.1 CLOSED it by
        // short-circuiting every value mode to `false` on an absent header
        // BEFORE the XOR. Contrast the PARITY companion
        // `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`,
        // which must keep the OPPOSITE verdict — that asymmetry is the whole
        // point of the mode scoping.
        let hm = hm_inverted("x-log", HeaderMatcherMode::ExactMatch("yes".into()));
        // Direct engine (route path):
        assert!(
            !hm.matches(&[]),
            "value-matcher absent+invert DROPS, matching upstream (CF-72-1 CLOSED)"
        );
        // Same verdict through the access-log `HeaderMatch` seam:
        let via_trait: std::sync::Arc<dyn envoy_accesslog::HeaderMatch> = std::sync::Arc::new(
            hm_inverted("x-log", HeaderMatcherMode::ExactMatch("yes".into())),
        );
        assert!(
            !via_trait.matches(&[]),
            "access-log path drops value-matcher absent+invert too (CF-72-1 CLOSED)"
        );
    }
```

**(c)** In `header_match_trait_delegates_to_inherent_engine` (`:488-504`), flip the final assertion (`:503`) and restate its comment:

```rust
        // invert now DROPS through the seam too: a VALUE matcher + invert +
        // absent = DROP, matching upstream (phase 75.1; CF-72-1 CLOSED). See
        // `pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream`.
        let inv: std::sync::Arc<dyn envoy_accesslog::HeaderMatch> = std::sync::Arc::new(
            hm_inverted("x-log", HeaderMatcherMode::ExactMatch("yes".into())),
        );
        assert!(!inv.matches(&[])); // value-matcher absent + invert = drop (parity)
```

- [ ] **Step 3: Strengthen the THREE guards (keep them GREEN)**

These already yield the right answer. Two of them keep it for the right reason; one keeps it for the WRONG stated reason and must have its rationale restated WITHOUT flipping its assertion.

**(a)** `present_match_false_returns_true_when_absent` (`:347-351`) — right answer, wrong stated reason. Rename and restate:

```rust
    #[test]
    fn present_match_false_matches_when_absent() {
        // Right answer, and after phase 75.1 for the right reason: the rule is
        // `(present == want)`, so absent + want=false is `(false == false)` =
        // true. Before 75.1 this passed only because the mode arm returned an
        // UNCONDITIONAL `true` — the same wrong rule that made
        // `present_match_false_requires_the_header_to_be_absent` fail.
        let m = hm("authorization", HeaderMatcherMode::PresentMatch(false));
        assert!(m.matches(&[]));
    }
```

**(b)** `invert_match_inverts_present_match_result` (`:424-429`) — append a guard note, do NOT change the assertions:

```rust
    #[test]
    fn invert_match_inverts_present_match_result() {
        // GUARD (phase 75.1): `present_match` is the ONLY mode whose ABSENT
        // cell still reaches `invert_match`. A uniform "absent => DROP" fix of
        // the shared engine would flip the first assertion below and mint a NEW
        // divergence. MEASURED PARITY on both proxies (SPEC §2.3 probe p07).
        let m = hm_inverted("authorization", HeaderMatcherMode::PresentMatch(true));
        assert!(m.matches(&[]));
        assert!(!m.matches(&[h("authorization", "x")]));
    }
```

**(c)** `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` (`:462-486`) — assertions UNCHANGED. Update only the sentence that describes the in-tree engine, replacing "the in-tree engine's `false ^ true` also KEEPs" with a statement of the post-fix rule, and change "A future CF-72-1 fixer MUST PRESERVE this KEEP" to "The phase-75.1 fixer PRESERVED this KEEP; any future refactor MUST continue to". Keep the pointer to the value-matcher companion, updating its name to `pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p envoy-config --lib matcher 2>&1 | tee /tmp/t2-green.txt
```

Expected: **`test result: ok. 59 passed; 0 failed`** (58 pre-existing in the two `matcher` test modules, plus Task 1's matrix). Verified at the PLAN-write pre-flight.

Then confirm no other crate regressed on the engine:

```bash
cargo test -p envoy-config 2>&1 | tee /tmp/t2-config.txt
grep -E '^test result' /tmp/t2-config.txt
```

- [ ] **Step 5: Lint**

```bash
touch crates/envoy-config/src/matcher.rs
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/t2-clippy.txt
grep -c 'Checking envoy-config' /tmp/t2-clippy.txt   # must be >= 1 — clippy prints Checking, NOT Compiling
cargo fmt --all -- --check
```

Expected: clippy `Finished` with zero warnings; `fmt --check` silent. Verified at the PLAN-write pre-flight.

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-config/src/matcher.rs
git commit -m "phase 75.1 task 2: GREEN — mode-scoped HeaderMatcher absence rule; amend 3 divergence-encoding tests, strengthen 3 guards"
```

---

## Task 3: The MUTATION CHECK — the phase's guard-level RED evidence

Task 1 gave TDD's RED for the FIX. This task gives the RED for the **GUARD**: it proves that the three guard tests actually catch the specific wrong fix the SPEC warns about, rather than passing vacuously. It is a named task, not an afterthought.

**Files:**
- No file in the main tree is modified. All work happens in a throwaway `git worktree`.

**Interfaces:**
- Consumes: the Task-2 engine and the Task-2/Task-1 test bodies.
- Produces: a `PROGRESS.md` evidence block. No code.

- [ ] **Step 1: Create a scratch worktree**

**Never mutate the main tree for this.** A parallel reviewer's `git checkout -- <file>` can silently revert an in-place mutation mid-run, producing a FALSE GREEN that `Compiling` does not catch (memory `mutation-checks-collide-with-parallel-subagents`).

```bash
git worktree add /tmp/wt-75-1-mutation HEAD --detach
```

- [ ] **Step 2: Run the UNMUTATED control first**

A mutation RED is not automatically a SEMANTIC red — a run can "fail" on a build or startup error that never reached an assertion (memory `mutation-red-needs-unmutated-control`).

```bash
cd /tmp/wt-75-1-mutation
cargo test -p envoy-config --lib matcher 2>&1 | tee /tmp/mut-control.txt
grep -E '^test result' /tmp/mut-control.txt
```

Expected: `test result: ok. 59 passed; 0 failed`.

- [ ] **Step 3: Apply the mutation**

The mutation is exactly the mistake `SPEC.md` §2.2 warns about — a **uniform "absent ⇒ DROP"**. In this engine shape that is a one-line reordering: hoist the `(_, None) => return false` arm ABOVE the `PresentMatch` arm, so an absent header short-circuits for EVERY mode including `present_match`.

In `/tmp/wt-75-1-mutation/crates/envoy-config/src/matcher.rs`, change the head of the `match` from:

```rust
        let mode_result = match (&self.mode, value) {
            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
            (_, None) => return false,
```

to:

```rust
        let mode_result = match (&self.mode, value) {
            (_, None) => return false,
            (HeaderMatcherMode::PresentMatch(want_present), v) => v.is_some() == *want_present,
```

- [ ] **Step 4: Verify the mutation is actually present, then run**

```bash
cd /tmp/wt-75-1-mutation
grep -n -A2 'let mode_result = match' crates/envoy-config/src/matcher.rs   # (_, None) must now be FIRST
cargo test -p envoy-config --lib matcher 2>&1 | tee /tmp/mut-red.txt
grep -E '^test result|FAILED' /tmp/mut-red.txt
```

Expected: **`test result: FAILED. 56 passed; 3 failed`**, with these three named in the `failures:` block:

- `matcher::tests::pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` — the guard the SPEC names
- `matcher::tests::invert_match_inverts_present_match_result`
- `matcher::tests::present_match_false_matches_when_absent`

(Task 1's matrix also asserts the P1 cell, so if it is present in this worktree it fails too — that is expected and additive, not a discrepancy.)

Read the failure TEXT and confirm each is an `assertion failed` / left-vs-right mismatch, NOT a compile error, a panic, or a startup failure. Measured at the PLAN-write: exactly these three, all assertion failures.

- [ ] **Step 5: Tear down — the mutation is NEVER committed**

```bash
cd /home/esa/git/envoy-rust
git worktree remove --force /tmp/wt-75-1-mutation
git worktree list          # /tmp/wt-75-1-mutation must be GONE
git status --porcelain     # must be empty
```

> `git worktree list` will also show pre-existing `.claude/worktrees/agent-*` entries belonging to a parallel workstream. **Leave them alone** — remove only your own.

- [ ] **Step 6: Record the evidence and commit**

Append to `PROGRESS.md` under `## Task 3 — mutation check (guard RED evidence)`: the control result, the exact mutation diff, the three RED test names with their verbatim assertion messages, and the teardown confirmation.

```bash
git add docs/envoy-rust/phases/75.1-headermatcher-absence-engine-route/PROGRESS.md
git commit -m "phase 75.1 task 3: mutation check — uniform absent-DROP turns all three P1 guards RED; reverted"
```

---

## Task 4: The eight doc comments and the in-source citation fix

Pure documentation. No behavior changes, so no new test — the gate is a by-hand wrap check plus the existing suite staying green.

**Files:**
- Modify: `crates/envoy-config/src/matcher.rs:15-21`, `:44-46` (already replaced in Task 2 — verify), `:61-63`, `:330`, `:439`
- Modify: `crates/envoy-config/src/bootstrap.rs:3119-3121`, `:3142-3143`, `:1704`
- Modify: `crates/envoy-accesslog/src/filter.rs:135-138`

**Interfaces:**
- Consumes: nothing. Produces: nothing. Independent of Tasks 5-13.

- [ ] **Step 1: `crates/envoy-config/src/matcher.rs:15-21` — extend the engine's own doc comment**

Replace the existing `/// Returns true iff …` block (`:15-21`) with:

```rust
    /// Returns true iff this matcher matches the given header set.
    ///
    /// Header NAME matching is case-insensitive per HTTP/1.1 RFC 7230 §3.2.
    /// Header VALUE matching is case-sensitive by default; the StringMatcher
    /// variant's `ignore_case` flips it for the value (Exact/Prefix/Suffix/
    /// Contains only — SafeRegex callers express case insensitivity via the
    /// `(?i)` inline flag; SPEC §6 signpost 15).
    ///
    /// ABSENCE semantics are MODE-SCOPED (phase 75.1, ADR-0159; MEASURED
    /// cross-proxy against `envoyproxy/envoy:v1.33.0`):
    ///
    /// * `present_match(want)` is the ONLY mode evaluated with the header
    ///   ABSENT — `(present == want) ^ invert_match`. An absent header
    ///   therefore still reaches `invert_match` in this mode.
    /// * EVERY value mode short-circuits to `false` when the header is absent;
    ///   `invert_match` is NOT applied. Upstream treats a missing header as an
    ///   unconditional value no-match that inversion does not resurrect.
    /// * An EMPTY header VALUE counts as PRESENT.
    ///
    /// The `present_match` + `invert_match` + absent cell is MEASURED PARITY
    /// and is guarded by `invert_match_inverts_present_match_result` and
    /// `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`. A
    /// uniform "absent => DROP" simplification BREAKS it and mints a NEW
    /// divergence — do not "simplify" the arm order below.
```

- [ ] **Step 2: `matcher.rs:44-46` — verify the WRONG comment is gone**

Task 2 replaced the whole method body, which removed:

```rust
                // present_match: true  → header must be present
                // present_match: false → no presence requirement (always true)
                // SPEC §6 signpost 7.
```

Confirm it is gone — this comment stated divergence D2's rule verbatim:

```bash
grep -n 'no presence requirement' crates/envoy-config/src/matcher.rs
```

Expected: **no output.**

- [ ] **Step 3: `matcher.rs:61-63` — the ADR-0150 seam doc asserts the old XOR as a design GUARANTEE**

Replace the sentence that currently reads:

```
/// VERBATIM. This keeps PV-4 (`mode_result ^ invert_match`, incl. absent+invert
/// = keep) identical between route matching and access-log filtering with zero
/// duplication.
```

with:

```rust
/// VERBATIM. This keeps the MODE-SCOPED absence rule (phase 75.1: value modes
/// short-circuit to `false` when the header is absent; `present_match` alone
/// carries an absent header into `invert_match`) identical between route
/// matching and access-log filtering with zero duplication.
```

- [ ] **Step 4: `matcher.rs:330` — the cell-count comment above the PresentMatch tests**

Replace:

```rust
    // PresentMatch: 4 cells (true × present, true × absent, false × present, false × absent).
```

with:

```rust
    // PresentMatch: 4 cells (true × present, true × absent, false × present,
    // false × absent). Phase 75.1 flipped the two `false ×` expectations: the
    // measured rule is `(present == want)`, not "false ⇒ always true".
```

- [ ] **Step 5: `matcher.rs:439` — the stale IN-SOURCE citation**

Inside the comment of the test amended in Task 2 step 2(b), the text `engine (matcher.rs:51)` is stale — the XOR is at `:52`. The Task-2 replacement comment already drops the parenthetical and says `(matcher.rs:52)`. Confirm:

```bash
grep -n 'matcher.rs:51' crates/envoy-config/src/matcher.rs
```

Expected: **no output.**

> **Scope discipline — and a CORRECTION to the SPEC's own numbers.** Correct exactly TWO `matcher.rs:51` citations: this in-source one, and `BEHAVIOR_CONTRACT.md:2369` (Task 13). Everything else is append-only per D-3.5 — the `DECISIONS.md` hits, the historical phase docs, and `STATE_HISTORY.md`.
>
> Two figures in `75.1/SPEC.md` §6 item 3 and in ADR-0158 are **STALE**, re-measured at this PLAN-write:
> - The **count is 32**, not 26 (36 once this `PLAN.md` itself lands, which adds four references).
> - The `DECISIONS.md` line numbers **have DRIFTED, and will keep drifting.** The SPEC lists `:373`, `:397`, `:2479`, `:2546`, `:2555`, `:2624`, `:2631` (seven). Inserting ADR-0157 + ADR-0158 pushed them down ~40 lines *within the session that wrote those numbers*, and appending ADR-0159 at this PLAN-write pushed them down another 22. As of this PLAN-write the six pre-existing hits are at **`:2442`, `:2541`, `:2608`, `:2617`, `:2686`, `:2693`**, plus **two new ones inside ADR-0159 itself** (`:2408`, `:2411`) where it quotes the defect it documents.
>
> **The drift-proof rule, which is what you should actually follow:** *every* `matcher.rs:51` hit in `docs/envoy-rust/DECISIONS.md`, `STATE_HISTORY.md`, `ROADMAP.md`, `STATE.md`, and any historical phase doc is append-only and must NOT be touched — **regardless of what line it is on**. Do not verify this set by line number; verify it by FILE. The only two sites you edit are `crates/envoy-config/src/matcher.rs:439` (the sole `.rs` hit) and `docs/envoy-rust/BEHAVIOR_CONTRACT.md:2369`.
>
> Both stale figures are recorded in ADR-0159, which landed at the state-2 PLAN-write. **Do NOT "fix" them in the SPEC or in ADR-0158** — both are landed, append-only artifacts.

- [ ] **Step 6: `crates/envoy-config/src/bootstrap.rs:3119-3121` — the `invert_match` field doc**

Replace:

```rust
    /// If true, the entire mode-specific match result is inverted (XOR after
    /// the mode match runs, before AND-combination across sibling
    /// HeaderMatchers). SPEC §6 signpost 5.
```

with:

```rust
    /// If true, the mode-specific match result is inverted before
    /// AND-combination across sibling HeaderMatchers. The inversion is NOT
    /// unconditional: for every VALUE mode an ABSENT header short-circuits to
    /// `false` WITHOUT being inverted; only `present_match` carries an absent
    /// header through the inversion (phase 75.1, ADR-0159 — MEASURED). See
    /// `HeaderMatcher::matches` in `matcher.rs`. SPEC §6 signpost 5.
```

- [ ] **Step 7: `bootstrap.rs:3142-3143` — the `PresentMatch` variant doc**

Replace:

```rust
    /// `present_match: <bool>` — header presence (true) or "no presence
    /// requirement" (false; SPEC §6 signpost 7 for the subtle false semantics).
```

with:

```rust
    /// `present_match: <bool>` — the header must be PRESENT (true) or ABSENT
    /// (false). MEASURED against `envoyproxy/envoy:v1.33.0` at phase 75.1
    /// (ADR-0159): the rule is `(present == want)`. This variant previously
    /// documented `false` as "no presence requirement (always true)", which was
    /// divergence D2. An EMPTY header VALUE counts as PRESENT.
```

- [ ] **Step 8: `bootstrap.rs:1704` — the `ValueMatcher` cross-reference (Trap A)**

This is a DIFFERENT message with a DIFFERENT and CORRECT rule. **Do not change the `ValueMatcher` rule itself.** Only the cross-reference goes stale.

> **Precision note.** `75.1/SPEC.md` §6 item 2 says "the same stale parenthetical is mirrored in source at `crates/envoy-config/src/bootstrap.rs:1704`". That is imprecise, verified at this PLAN-write: the formula `want ? present : true` appears in **zero** `.rs` files — it lives only in `BEHAVIOR_CONTRACT.md:1879-1880` (corrected by Task 13 step 2). What `bootstrap.rs:1704` carries is a DIFFERENT, also-stale cross-reference ("NOT the HeaderMatcher `present_match` precedent"). Both need correcting; they are not the same sentence. Edit the text quoted below, not a formula that is not there.

Replace:

```rust
    /// §A1 (phase 36): match on KEY PRESENCE. Semantics `match = present && want`
    /// (`present_match: false` NEVER matches — NOT the HeaderMatcher `present_match` precedent).
```

with:

```rust
    /// §A1 (phase 36): match on KEY PRESENCE. Semantics `match = present && want`
    /// (`present_match: false` NEVER matches). Distinct from — and NOT derived
    /// from — the `HeaderMatcher` `present_match`, whose rule is
    /// `(present == want)` (phase 75.1): the two AGREE when the key/header is
    /// PRESENT and still DIFFER when it is ABSENT (`ValueMatcher` → false,
    /// `HeaderMatcher` → true). Do not unify them.
```

- [ ] **Step 9: `crates/envoy-accesslog/src/filter.rs:135-138` — the access-log arm comment**

Replace:

```rust
            // Phase 72: gate on whether the named request header matches. Present-
            // mismatch AND absent both drop (the engine's own semantics); PV-4's
            // `mode_result ^ invert_match` is preserved because the injected impl
            // calls `HeaderMatcher::matches` verbatim.
```

with:

```rust
            // Phase 72: gate on whether the named request header matches. The
            // injected impl calls the shared `HeaderMatcher` engine verbatim, so
            // this arm inherits its MODE-SCOPED absence rule (phase 75.1): for
            // every VALUE mode an absent header DROPS without reaching
            // `invert_match`; `present_match` alone carries an absent header
            // through the inversion.
```

- [ ] **Step 10: Wrap-check BY HAND, then verify**

`cargo fmt` does NOT reflow `///` / `//` lines. Read each of the nine edited comment blocks and confirm every line is ≤ ~80 columns and that no sentence was left describing the OLD behavior:

```bash
git diff -U0 | grep -E '^\+.*(///|//!)' | awk '{ if (length($0) > 82) print "TOO LONG: " $0 }'
grep -rn 'no presence requirement\|always true' crates/envoy-config/src/ crates/envoy-accesslog/src/
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p envoy-config -p envoy-accesslog 2>&1 | grep -E '^test result'
```

Expected: the length filter prints nothing; the stale-phrase grep prints nothing; fmt/clippy clean; all test results `ok`.

- [ ] **Step 11: Commit**

```bash
git add crates/envoy-config/src/matcher.rs crates/envoy-config/src/bootstrap.rs crates/envoy-accesslog/src/filter.rs
git commit -m "phase 75.1 task 4: correct eight doc comments stating the pre-75.1 uniform-XOR rule; fix the in-source matcher.rs:51 citation"
```

---

## Task 5: Consumer propagation — the route walker (H1 AND H2)

Call site 1 of 5. `crates/envoy-http1/src/hcm.rs:2165` (`route_matches`) serves BOTH protocols: H2 has no independent walker and delegates via `envoy_http1::hcm::resolve_route`, called at `crates/envoy-http2/src/hcm.rs:475`.

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` (append inside the existing `#[cfg(test)] mod tests`)
- Test: `crates/envoy-http2/src/hcm.rs` (append inside the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `envoy_http1::hcm::resolve_route(config: &HCMConfig, req: &Request) -> Option<ResolvedRoute>` (declared at `crates/envoy-http1/src/hcm.rs:2002`); `envoy_http1::hcm::ResolvedRoute::route(&self) -> &Route`.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing H1 test**

Append inside `mod tests` in `crates/envoy-http1/src/hcm.rs`. Follow the construction idiom of the existing `header_filter_membership_across_modes_and_absent_drop` (`:4996-5074`) — there is no `h(...)` helper in this module, so headers are literal `(String, String)` arrays.

```rust
    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates to the
    /// ROUTE walker — call site 1 of 5, and the one this sub-phase's
    /// differential fixture 0083 witnesses cross-proxy. `route_matches`
    /// AND-combines the route's HeaderMatchers, so a matcher that must now
    /// return `false` on an absent header must make the whole route not match.
    #[test]
    fn route_header_matcher_absence_rule_is_mode_scoped() {
        use envoy_config::{HeaderMatcher, HeaderMatcherMode, Route, RouteMatch};

        let route = |mode: HeaderMatcherMode, invert: bool| Route {
            name: "r".to_string(),
            r#match: RouteMatch {
                prefix: Some("/".to_string()),
                path: None,
                headers: vec![HeaderMatcher {
                    name: "x-a".to_string(),
                    mode,
                    invert_match: invert,
                }],
            },
            action: RouteAction::DirectResponse(DirectResponse {
                status: 200,
                body: DataSource {
                    filename: None,
                    inline_string: Some("hi".to_string()),
                },
            }),
            typed_per_filter_config: Default::default(),
        };
        let present = [("x-a".to_string(), "zzz".to_string())];
        let absent: [(String, String); 0] = [];

        // D1: a VALUE matcher + invert + ABSENT must NOT match the route.
        let r = route(HeaderMatcherMode::ExactMatch("v".into()), true);
        assert!(
            route_matches(&r, "/x", &present),
            "value+invert, present non-matching value → route matches"
        );
        assert!(
            !route_matches(&r, "/x", &absent),
            "value+invert, ABSENT → route must NOT match (D1 / CF-72-1 closed)"
        );

        // D2: a plain, NON-inverted `present_match: false` requires ABSENCE.
        let r = route(HeaderMatcherMode::PresentMatch(false), false);
        assert!(
            !route_matches(&r, "/x", &present),
            "present_match:false with the header PRESENT → route must NOT match (D2)"
        );
        assert!(
            route_matches(&r, "/x", &absent),
            "present_match:false with the header ABSENT → route matches"
        );

        // P1 THE GUARD: `present_match: true` + invert + ABSENT still matches.
        let r = route(HeaderMatcherMode::PresentMatch(true), true);
        assert!(
            route_matches(&r, "/x", &absent),
            "present_match:true+invert, ABSENT → route STILL matches (P1 parity)"
        );
        assert!(
            !route_matches(&r, "/x", &present),
            "present_match:true+invert, PRESENT → route does not match"
        );
    }
```

- [ ] **Step 2: Write the failing H2 test**

Append inside `mod tests` in `crates/envoy-http2/src/hcm.rs`. This one goes through the public `resolve_route` seam, proving H2 inherits the rule rather than carrying its own walker. Model the config literal on the existing `h2_resolve_route_reachable_and_returns_cors_route` (`crates/envoy-http2/src/hcm.rs:6501`, which calls `envoy_http1::hcm::resolve_route` at `:6594`).

```rust
    /// Phase 75.1 (ADR-0159): H2 has no independent route walker — it calls
    /// `envoy_http1::hcm::resolve_route` (hcm.rs:475). This pins that the
    /// MODE-SCOPED absence rule reaches the H2 path through that delegation,
    /// so the route-path fix is witnessed on BOTH protocols even though
    /// differential fixture 0083 is H1-only.
    #[tokio::test]
    async fn h2_resolve_route_inherits_mode_scoped_absence_rule() {
        use envoy_config::{HeaderMatcher, HeaderMatcherMode};

        // Two routes on the same prefix: the first carries the matcher under
        // test and is named "gated"; the second is an unguarded catch-all.
        let build = |mode: HeaderMatcherMode, invert: bool| {
            let gated = Route {
                name: "gated".to_string(),
                r#match: RouteMatch {
                    prefix: Some("/".to_string()),
                    path: None,
                    headers: vec![HeaderMatcher {
                        name: "x-a".to_string(),
                        mode,
                        invert_match: invert,
                    }],
                },
                action: RouteAction::DirectResponse(DirectResponse {
                    status: 200,
                    body: DataSource {
                        filename: None,
                        inline_string: Some("gated".to_string()),
                    },
                }),
                typed_per_filter_config: Default::default(),
            };
            let catch_all = Route {
                name: "catch-all".to_string(),
                r#match: RouteMatch {
                    prefix: Some("/".to_string()),
                    path: None,
                    headers: vec![],
                },
                action: RouteAction::DirectResponse(DirectResponse {
                    status: 200,
                    body: DataSource {
                        filename: None,
                        inline_string: Some("catch-all".to_string()),
                    },
                }),
                typed_per_filter_config: Default::default(),
            };
            RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![gated, catch_all],
                }],
            }
        };

        // Resolve through the REAL H1 seam H2 delegates to, and report which
        // route name won.
        let resolved = |route_config: RouteConfiguration, headers: Vec<(String, String)>| async move {
            let cfg = HttpConnectionManagerConfig {
                stat_prefix: "ingress_http_h2".to_string(),
                codec_type: CodecType::HTTP2,
                http2_protocol_options: None,
                access_log: vec![],
                route_config: Some(route_config),
                rds: None,
                http_filters: vec![HttpFilter {
                    name: "envoy.filters.http.router".to_string(),
                    typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
                }],
            };
            let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
            let registry = Arc::new(envoy_stats::StatsRegistry::new());
            let built = Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
                .await
                .expect("build HCM config");
            let mut req = envoy_http1::codec::Request::test("GET", "/x", &[]);
            req.headers = {
                let mut hs = vec![("host".to_string(), "any.test".to_string())];
                hs.extend(headers);
                hs
            };
            envoy_http1::hcm::resolve_route(&built, &req)
                .map(|r| envoy_http1::hcm::ResolvedRoute::route(&r).name.clone())
                .expect("a route always resolves — the catch-all has no matchers")
        };

        let present = vec![("x-a".to_string(), "zzz".to_string())];

        // D1: value matcher + invert + ABSENT → gated route must NOT win.
        let rc = build(HeaderMatcherMode::ExactMatch("v".into()), true);
        assert_eq!(resolved(rc, vec![]).await, "catch-all", "D1 on the H2 path");

        // D2: plain `present_match: false` + header PRESENT → must NOT win.
        let rc = build(HeaderMatcherMode::PresentMatch(false), false);
        assert_eq!(
            resolved(rc, present.clone()).await,
            "catch-all",
            "D2 on the H2 path"
        );

        // P1 THE GUARD: `present_match: true` + invert + ABSENT → still wins.
        let rc = build(HeaderMatcherMode::PresentMatch(true), true);
        assert_eq!(
            resolved(rc, vec![]).await,
            "gated",
            "P1 parity preserved on the H2 path"
        );
    }
```

> **Implementer note.** `envoy_http1::codec::Request::test(..)` is the constructor used by the sibling H2 tests; if its exact name or signature differs in the live tree, mirror whatever `h2_resolve_route_reachable_and_returns_cors_route` (`crates/envoy-http2/src/hcm.rs:6501`) does to build its `Request` — that test is the working precedent for calling `resolve_route` from `envoy-http2`. Do NOT add a new public constructor to `envoy-http1` for this.

- [ ] **Step 3: Run both tests to verify they fail**

Run them against the PRE-Task-2 engine to confirm they are real RED. If Tasks 1-2 are already committed (the normal ordering), instead verify RED by re-running them inside the Task-3 mutation worktree and confirming the P1 assertions flip.

```bash
cargo test -p envoy-http1 --lib route_header_matcher_absence_rule_is_mode_scoped 2>&1 | tee /tmp/t5-h1.txt
cargo test -p envoy-http2 --lib h2_resolve_route_inherits_mode_scoped_absence_rule 2>&1 | tee /tmp/t5-h2.txt
```

- [ ] **Step 4: Run to verify they pass on the fixed engine**

```bash
grep -E '^test result' /tmp/t5-h1.txt /tmp/t5-h2.txt
```

Expected: `1 passed; 0 failed` for each. **Assert on the passed COUNT, never on the exit code** — `cargo test -p <pkg> <name>` can exit 0 with `0 passed; N filtered out` when the name lives in another test binary (memory `cargo-test-p-name-false-green-filtered-out`).

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs crates/envoy-http2/src/hcm.rs
git commit -m "phase 75.1 task 5: pin mode-scoped absence propagation through the route walker (H1 + H2 via resolve_route)"
```

---

## Task 6: Consumer propagation — HTTP RBAC

Call site 2 of 5. `crates/envoy-filter/src/rbac.rs:60`, inside `pub(crate) fn eval`.

**Files:**
- Test: `crates/envoy-filter/src/rbac.rs` (append inside the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `RuntimeMatcher::Header(HeaderMatcher)` and `pub(crate) fn eval(m: &RuntimeMatcher, req: &FilterRequest) -> bool`; the module's existing `req_with(headers: Vec<(&'static str, &'static str)>) -> FilterRequest` helper (`rbac.rs:349-351`).

> **Do NOT use the shared `crate::types::header_matcher_exact` helper here.** It builds the `StringMatch` mode, not `ExactMatch` (`crates/envoy-filter/src/types.rs:157-166`), and this test needs to name the mode explicitly.

- [ ] **Step 1: Write the failing test**

```rust
    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates into the
    /// RBAC matcher tree — call site 2 of 5. `Permission`/`Principal` header
    /// conditions delegate straight to `HeaderMatcher::matches`, so an absent
    /// header must now DROP for value modes and `present_match: false` must
    /// require absence.
    #[test]
    fn rbac_header_condition_absence_rule_is_mode_scoped() {
        use envoy_config::{HeaderMatcher, HeaderMatcherMode};

        let cond = |mode: HeaderMatcherMode, invert: bool| {
            RuntimeMatcher::Header(HeaderMatcher {
                name: "x-a".to_string(),
                mode,
                invert_match: invert,
            })
        };
        let present = req_with(vec![("x-a", "zzz")]);
        let absent = req_with(vec![("x-other", "zzz")]);

        // D1: value matcher + invert + ABSENT → no longer matches.
        let c = cond(HeaderMatcherMode::ExactMatch("v".into()), true);
        assert!(eval(&c, &present), "value+invert, present non-matching");
        assert!(
            !eval(&c, &absent),
            "value+invert, ABSENT → must NOT match (D1 / CF-72-1 closed)"
        );

        // D2: plain `present_match: false` requires ABSENCE.
        let c = cond(HeaderMatcherMode::PresentMatch(false), false);
        assert!(!eval(&c, &present), "present_match:false, PRESENT (D2)");
        assert!(eval(&c, &absent), "present_match:false, ABSENT");

        // P1 THE GUARD.
        let c = cond(HeaderMatcherMode::PresentMatch(true), true);
        assert!(
            eval(&c, &absent),
            "present_match:true+invert, ABSENT → still matches (P1 parity)"
        );
        assert!(!eval(&c, &present), "present_match:true+invert, PRESENT");
    }
```

- [ ] **Step 2: Run to verify it fails**

Against the pre-fix engine (or in the Task-3 mutation worktree, for the P1 leg):

```bash
cargo test -p envoy-filter --lib rbac_header_condition_absence_rule_is_mode_scoped 2>&1 | tee /tmp/t6.txt
```

- [ ] **Step 3: No implementation**

`rbac.rs` is NOT edited. The behavior comes from Task 2's engine fix. This task only adds coverage.

- [ ] **Step 4: Run to verify it passes**

```bash
grep -E '^test result' /tmp/t6.txt
```

Expected: `test result: ok. 1 passed; 0 failed` — assert on the count, not the exit code.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-filter/src/rbac.rs
git commit -m "phase 75.1 task 6: pin mode-scoped absence propagation through the HTTP RBAC matcher tree"
```

---

## Task 7: Consumer propagation — the fault filter header gate

Call site 3 of 5. `crates/envoy-filter/src/fault.rs:76`, inside `fn header_gate_matches`.

**Files:**
- Test: `crates/envoy-filter/src/fault.rs` (append inside the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: the module's existing helpers `cfg(numerator: u32, headers: Vec<HeaderMatcher>) -> FaultConfig` and `req(headers: Vec<(String, String)>) -> FilterRequest` (`fault.rs:80-105`); `FaultFilter::build_from_config(&FaultConfig, &Arc<StatsRegistry>, &str)`; `Decision::{Continue, StopAndSend}`.

- [ ] **Step 1: Write the failing test**

```rust
    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates into the
    /// fault filter's header gate — call site 3 of 5. The gate AND-combines its
    /// matchers, and a 100% abort fires iff the gate matches, so the gate's
    /// verdict is directly observable as StopAndSend vs Continue.
    #[test]
    fn fault_header_gate_absence_rule_is_mode_scoped() {
        use envoy_config::HeaderMatcherMode;

        let gate = |mode: HeaderMatcherMode, invert: bool| {
            vec![HeaderMatcher {
                name: "x-a".to_string(),
                mode,
                invert_match: invert,
            }]
        };
        let registry = Arc::new(StatsRegistry::new());
        let aborts = |g: Vec<HeaderMatcher>, headers: Vec<(String, String)>| {
            let mut f = FaultFilter::build_from_config(&cfg(100, g), &registry, "ingress_http")
                .expect("fault config builds");
            let mut r = req(headers);
            matches!(f.decode_headers(&mut r), Decision::StopAndSend(_))
        };
        let present = vec![("x-a".to_string(), "zzz".to_string())];

        // D1: value matcher + invert + ABSENT → gate no longer fires.
        let g = gate(HeaderMatcherMode::ExactMatch("v".into()), true);
        assert!(
            aborts(g.clone(), present.clone()),
            "value+invert, present non-matching → gate fires"
        );
        assert!(
            !aborts(g, vec![]),
            "value+invert, ABSENT → gate must NOT fire (D1 / CF-72-1 closed)"
        );

        // D2: plain `present_match: false` requires ABSENCE.
        let g = gate(HeaderMatcherMode::PresentMatch(false), false);
        assert!(
            !aborts(g.clone(), present.clone()),
            "present_match:false, PRESENT → gate must NOT fire (D2)"
        );
        assert!(aborts(g, vec![]), "present_match:false, ABSENT → gate fires");

        // P1 THE GUARD.
        let g = gate(HeaderMatcherMode::PresentMatch(true), true);
        assert!(
            aborts(g.clone(), vec![]),
            "present_match:true+invert, ABSENT → gate STILL fires (P1 parity)"
        );
        assert!(
            !aborts(g, present),
            "present_match:true+invert, PRESENT → gate does not fire"
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p envoy-filter --lib fault_header_gate_absence_rule_is_mode_scoped 2>&1 | tee /tmp/t7.txt
```

- [ ] **Step 3: No implementation**

`fault.rs` production code is NOT edited.

- [ ] **Step 4: Run to verify it passes**

```bash
grep -E '^test result' /tmp/t7.txt
```

Expected: `1 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-filter/src/fault.rs
git commit -m "phase 75.1 task 7: pin mode-scoped absence propagation through the fault filter header gate"
```

---

## Task 8: Consumer propagation — JWT authn rule matching

Call site 4 of 5. `crates/envoy-filter/src/jwt_authn.rs:185`, inside `fn route_match_matches`.

**Files:**
- Test: `crates/envoy-filter/src/jwt_authn.rs` (append inside the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: the module's existing helpers `registry()`, `req(headers: Vec<(String, String)>, path: &str) -> FilterRequest`, `host() -> (String, String)`, `allowed_value(&Arc<StatsRegistry>) -> u64`, `denied_value(...) -> u64` (`jwt_authn.rs:291-322`); `JwtAuthnFilter::build_from_config`.

The observable is the SAME one the existing `header_matcher_gates_rule_match` test (`jwt_authn.rs:575-647`) uses: when a rule's header matcher does NOT match, the request takes the "no rule matched ⇒ allow without JWT check" path, so a request carrying NO token is still `Continue` and ticks `allowed`. When the rule DOES match, a tokenless request is DENIED.

- [ ] **Step 1: Write the failing test**

```rust
    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates into
    /// JWT-authn requirement-rule matching — call site 4 of 5. Observable
    /// without minting a token: a rule whose header matcher does NOT match is
    /// skipped, so a TOKENLESS request is allowed; a rule that DOES match
    /// demands a token, so a tokenless request is denied.
    #[test]
    fn jwt_rule_header_matcher_absence_rule_is_mode_scoped() {
        use envoy_config::{HeaderMatcher, HeaderMatcherMode};

        let (_kp, jwks) = keypair();

        // Returns true iff the RULE MATCHED (a tokenless request got denied).
        let rule_matched = |mode: HeaderMatcherMode, invert: bool, headers: Vec<(String, String)>| {
            let reg = registry();
            let mut providers = std::collections::BTreeMap::new();
            providers.insert(
                "prov".to_string(),
                envoy_config::JwtProvider {
                    issuer: ISS.to_string(),
                    audiences: vec![],
                    local_jwks: envoy_config::DataSource {
                        filename: None,
                        inline_string: Some(jwks.clone()),
                    },
                    forward: false,
                },
            );
            let cfg = envoy_config::JwtAuthnConfig {
                providers,
                rules: vec![envoy_config::RequirementRule {
                    r#match: envoy_config::RouteMatch {
                        prefix: Some("/".to_string()),
                        path: None,
                        headers: vec![HeaderMatcher {
                            name: "x-a".to_string(),
                            mode,
                            invert_match: invert,
                        }],
                    },
                    requires: envoy_config::JwtRequirement {
                        provider_name: "prov".to_string(),
                    },
                }],
            };
            let mut f = JwtAuthnFilter::build_from_config(&cfg, &reg, "ingress_http").unwrap();
            let mut hs = vec![host()];
            hs.extend(headers);
            let mut r = req(hs, "/api");
            let _ = f.decode_headers(&mut r);
            denied_value(&reg) == 1
        };

        let present = vec![("x-a".to_string(), "zzz".to_string())];

        // D1: value matcher + invert + ABSENT → the rule no longer matches.
        assert!(
            rule_matched(HeaderMatcherMode::ExactMatch("v".into()), true, present.clone()),
            "value+invert, present non-matching → rule matches → tokenless denied"
        );
        assert!(
            !rule_matched(HeaderMatcherMode::ExactMatch("v".into()), true, vec![]),
            "value+invert, ABSENT → rule must NOT match (D1 / CF-72-1 closed)"
        );

        // D2: plain `present_match: false` requires ABSENCE.
        assert!(
            !rule_matched(HeaderMatcherMode::PresentMatch(false), false, present.clone()),
            "present_match:false, PRESENT → rule must NOT match (D2)"
        );
        assert!(
            rule_matched(HeaderMatcherMode::PresentMatch(false), false, vec![]),
            "present_match:false, ABSENT → rule matches"
        );

        // P1 THE GUARD.
        assert!(
            rule_matched(HeaderMatcherMode::PresentMatch(true), true, vec![]),
            "present_match:true+invert, ABSENT → rule STILL matches (P1 parity)"
        );
        assert!(
            !rule_matched(HeaderMatcherMode::PresentMatch(true), true, present),
            "present_match:true+invert, PRESENT → rule does not match"
        );
    }
```

> **Implementer note.** `keypair()`, `ISS`, `registry()`, `req(..)`, `host()` and `denied_value(..)` are all existing helpers of that test module, used by `header_matcher_gates_rule_match` at `jwt_authn.rs:575`. If `denied_value` counts differently than assumed, read that neighbouring test and mirror its exact assertion style rather than inventing a new observable.

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p envoy-filter --lib jwt_rule_header_matcher_absence_rule_is_mode_scoped 2>&1 | tee /tmp/t8.txt
```

- [ ] **Step 3: No implementation**

`jwt_authn.rs` production code is NOT edited.

- [ ] **Step 4: Run to verify it passes**

```bash
grep -E '^test result' /tmp/t8.txt
```

Expected: `1 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-filter/src/jwt_authn.rs
git commit -m "phase 75.1 task 8: pin mode-scoped absence propagation through JWT-authn rule matching"
```

---

## Task 9: Consumer propagation — the access-log `header_filter` through the ADR-0150 trait object

Call site 5 of 5. `crates/envoy-accesslog/src/filter.rs:139`, reached as `Arc<dyn HeaderMatch>`.

**This test MUST live in `crates/envoy-http1/src/hcm.rs`.** `envoy-accesslog` cannot depend on `envoy-config` (ADR-0150 — the reverse edge exists, so it would be a dependency cycle), so its own tests can only use local stub trait objects. `envoy-http1` depends on both crates and owns `compile_access_log_filter`, which is where the real `HeaderMatcher` is boxed into the seam (`crates/envoy-http1/src/hcm.rs:1784-1786`).

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` (append inside the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: the private `fn compile_access_log_filter(f: &envoy_config::AccessLogFilter) -> envoy_accesslog::LogFilter` (`crates/envoy-http1/src/hcm.rs:1757`); `envoy_accesslog::LogFilter::should_log(status: u16, response_flags: &str, headers: &[(String, String)], dynamic_metadata: &BTreeMap<String, BTreeMap<String, String>>) -> bool`.

- [ ] **Step 1: Write the failing test**

Model the `AccessLogFilter` literal on the existing `header_filter_membership_across_modes_and_absent_drop` (`:4996-5074`) — all six oneof arms must be spelled out.

```rust
    /// Phase 75.1 (ADR-0159): the MODE-SCOPED absence rule propagates to the
    /// access-log `header_filter` — call site 5 of 5, and the only one reached
    /// through the ADR-0150 `Arc<dyn HeaderMatch>` trait object rather than the
    /// inherent method. Compiled end-to-end via `compile_access_log_filter`, so
    /// this exercises the real boxing the runtime performs, not a stub.
    ///
    /// The CROSS-PROXY witness for this call site is sub-phase 75.2 (fixtures
    /// 0084 + 0085); this in-process pin is what makes 75.1 a complete slice.
    #[test]
    fn access_log_header_filter_absence_rule_is_mode_scoped_through_the_seam() {
        use envoy_config::HeaderMatcherMode as M;

        let compile = |mode: M, invert: bool| {
            compile_access_log_filter(&envoy_config::AccessLogFilter {
                status_code_filter: None,
                response_flag_filter: None,
                header_filter: Some(envoy_config::HeaderFilter {
                    header: envoy_config::HeaderMatcher {
                        name: "x-a".into(),
                        mode,
                        invert_match: invert,
                    },
                }),
                and_filter: None,
                or_filter: None,
                metadata_filter: None,
            })
        };
        let present = [("x-a".to_string(), "zzz".to_string())];
        let absent: [(String, String); 0] = [];

        // D1: value matcher + invert + ABSENT → the record is now DROPPED.
        let f = compile(M::ExactMatch("v".into()), true);
        assert!(
            f.should_log(200, "-", &present, &Default::default()),
            "value+invert, present non-matching → KEEP"
        );
        assert!(
            !f.should_log(200, "-", &absent, &Default::default()),
            "value+invert, ABSENT → DROP (D1 / CF-72-1 closed) — this is the \
             divergence fixture 0078's README recorded as deferred"
        );

        // D2: plain `present_match: false` requires ABSENCE.
        let f = compile(M::PresentMatch(false), false);
        assert!(
            !f.should_log(200, "-", &present, &Default::default()),
            "present_match:false, PRESENT → DROP (D2)"
        );
        assert!(
            f.should_log(200, "-", &absent, &Default::default()),
            "present_match:false, ABSENT → KEEP"
        );

        // P1 THE GUARD — must stay KEEP through the seam.
        let f = compile(M::PresentMatch(true), true);
        assert!(
            f.should_log(200, "-", &absent, &Default::default()),
            "present_match:true+invert, ABSENT → STILL KEEP (P1 parity)"
        );
        assert!(
            !f.should_log(200, "-", &present, &Default::default()),
            "present_match:true+invert, PRESENT → DROP"
        );

        // An EMPTY VALUE counts as PRESENT through the seam too.
        let empty = [("x-a".to_string(), String::new())];
        assert!(
            !compile(M::PresentMatch(false), false)
                .should_log(200, "-", &empty, &Default::default()),
            "an EMPTY header value is PRESENT, so present_match:false DROPs"
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
cargo test -p envoy-http1 --lib access_log_header_filter_absence_rule_is_mode_scoped_through_the_seam 2>&1 | tee /tmp/t9.txt
```

- [ ] **Step 3: No implementation**

Neither `envoy-accesslog` nor `compile_access_log_filter` is edited. The ADR-0150 seam must not move: `envoy-accesslog` keeps ZERO workspace dependencies, matchers keep crossing as injected trait objects, and `LogFilter` keeps having NO `Eq`/`PartialEq`.

- [ ] **Step 4: Run to verify it passes**

```bash
grep -E '^test result' /tmp/t9.txt
```

Expected: `1 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 75.1 task 9: pin mode-scoped absence propagation through the ADR-0150 HeaderMatch trait object"
```

---

## Task 10: Fixture `0083` — the two configs

**Files:**
- Create: `tests/fixtures/0083-headermatcher-absence-parity/envoy.yaml`
- Create: `tests/fixtures/0083-headermatcher-absence-parity/envoy-rust.yaml`

**Interfaces:**
- Consumes: nothing in-tree.
- Produces: the route table Task 11's probes drive. The route/body naming contract is: for probe id `pNN`, prefix `/pNN` resolves to body **`pNN=MATCH`** when the matcher under test matches, else **`pNN=NOMATCH`**.

**Design.** One HTTP/1.1 HCM listener, `clusters: []`, `direct_response` only — so no backend container spawns. Backend-free-ness is decided by a literal substring scan for `{{BACKEND_PORT}}` (`tests/differential/src/lib.rs:3322-3330` → `scan_needs_marker`); this fixture carries no backend marker. Per matcher under test, an ORDERED route PAIR on prefix `/pNN`: the first carries the `HeaderMatcher`, the second is an unguarded catch-all. Discrimination is by `direct_response` body, byte-exact.

Eight matchers, chosen so every distinct code path in the §2.1 rule is witnessed and the guard is pinned:

| id | matcher | witnesses |
|---|---|---|
| p01 | `exact_match: "v"` + `invert_match: true` | **D1** — the plain value-matcher case |
| p06 | `range_match: {start: 1, end: 10}` + `invert_match: true` | **D1** on the numeric parse path |
| p07 | `present_match: true` + `invert_match: true` | **P1 — THE GUARD.** Must stay MATCH-on-absent |
| p08 | `present_match: false` + `invert_match: true` | **D2**, inverted |
| p09 | `string_match: {exact: "v"}` + `invert_match: true` | **D1** through the `StringMatcher` delegation |
| p10 | `exact_match: "v"` | parity control |
| p11 | `present_match: true` | parity control + the empty-value presence cell |
| p12 | `present_match: false` | **D2**, NON-inverted — the worst cell |

The ids are non-contiguous ON PURPOSE: they are the probe ids of the `SPEC.md` §2.3 measured matrix, so every expectation in Task 11 can be read straight off that table's **upstream** column without re-deriving anything.

- [ ] **Step 1: Write `envoy.yaml` (the upstream side)**

```yaml
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        # p01 — D1: exact_match + invert_match.
                        - match:
                            prefix: "/p01"
                            headers:
                              - name: "x-a"
                                exact_match: "v"
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "p01=MATCH" }
                        - match: { prefix: "/p01" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p01=NOMATCH" }
                        # p06 — D1 on the numeric parse path.
                        - match:
                            prefix: "/p06"
                            headers:
                              - name: "x-a"
                                range_match: { start: 1, end: 10 }
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "p06=MATCH" }
                        - match: { prefix: "/p06" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p06=NOMATCH" }
                        # p07 — P1, THE GUARD. absent MUST stay MATCH.
                        - match:
                            prefix: "/p07"
                            headers:
                              - name: "x-a"
                                present_match: true
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "p07=MATCH" }
                        - match: { prefix: "/p07" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p07=NOMATCH" }
                        # p08 — D2, inverted.
                        - match:
                            prefix: "/p08"
                            headers:
                              - name: "x-a"
                                present_match: false
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "p08=MATCH" }
                        - match: { prefix: "/p08" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p08=NOMATCH" }
                        # p09 — D1 through the StringMatcher delegation.
                        - match:
                            prefix: "/p09"
                            headers:
                              - name: "x-a"
                                string_match: { exact: "v" }
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "p09=MATCH" }
                        - match: { prefix: "/p09" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p09=NOMATCH" }
                        # p10 — parity control, plain exact_match.
                        - match:
                            prefix: "/p10"
                            headers:
                              - name: "x-a"
                                exact_match: "v"
                          direct_response:
                            status: 200
                            body: { inline_string: "p10=MATCH" }
                        - match: { prefix: "/p10" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p10=NOMATCH" }
                        # p11 — parity control + the empty-value presence cell.
                        - match:
                            prefix: "/p11"
                            headers:
                              - name: "x-a"
                                present_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "p11=MATCH" }
                        - match: { prefix: "/p11" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p11=NOMATCH" }
                        # p12 — D2, NON-inverted. The worst cell: a plain,
                        # single-line matcher silently matched everything
                        # in-tree before phase 75.1.
                        - match:
                            prefix: "/p12"
                            headers:
                              - name: "x-a"
                                present_match: false
                          direct_response:
                            status: 200
                            body: { inline_string: "p12=MATCH" }
                        - match: { prefix: "/p12" }
                          direct_response:
                            status: 200
                            body: { inline_string: "p12=NOMATCH" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
```

- [ ] **Step 2: Write `envoy-rust.yaml` (the subject side)**

Copy `envoy.yaml` verbatim, then apply exactly the three house per-side deltas (the same three `0007-http1-direct-response` uses):

1. **PREPEND** a `node:` block.
2. **CHANGE** the listener bind `0.0.0.0` → `127.0.0.1`.
3. **DROP** the trailing `admin:` block.

Keep `codec_type: HTTP1`, the filters, and the ENTIRE route table byte-identical. So the file starts:

```yaml
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
```

…and everything from `filter_chains:` through `  clusters: []` is byte-identical to `envoy.yaml`, with no `admin:` block at the end.

> **Two notes the implementer will otherwise trip on.**
> **(a) `codec_type` is NOT a per-side divergence.** `crates/envoy-config/src/bootstrap.rs:1103` declares `pub codec_type: CodecType` with no `#[serde(default)]` under `deny_unknown_fields`, so a missing key is a hard parse error for envoy-rust while upstream defaults to `AUTO`. But all 82 existing fixtures write `codec_type: HTTP1` on BOTH sides, so write it on both and do not treat it as a divergence (ADR-0158 correction C3).
> **(b) The unquoted `node: { cluster: y }` YAML-1.1 boolean trap does NOT apply here.** An unquoted `y` can parse as boolean `true` and upstream's JSON-proto path then rejects the whole bootstrap. That trap bites hand-rolled probe configs that send a `node:` block to UPSTREAM. Here the `node:` block exists ONLY on the envoy-rust side (exactly as in `0007`, where this form is proven green), and envoy-rust's `serde_yaml` reads bare `y` as the string `"y"`. Match the house form; do not "fix" it.

- [ ] **Step 3: Verify both configs parse on the subject side**

```bash
cargo build -p envoy-bin
sed 's/{{PORT}}/18083/' tests/fixtures/0083-headermatcher-absence-parity/envoy-rust.yaml > /tmp/0083-rust.yaml
./target/debug/envoy-bin -c /tmp/0083-rust.yaml &
sleep 2 && curl -sS -H 'Host: envoy-rust.test' http://127.0.0.1:18083/p12 ; echo
kill %1
```

Expected: `p12=NOMATCH` (no `x-a` sent… wait — `/p12` with the header ABSENT must print **`p12=MATCH`**). Confirm it prints `p12=MATCH`. Note `envoy-bin` writes `ConfigError` to **STDOUT**, not stderr, and takes only `-c <path>`.

- [ ] **Step 4: Verify the upstream side parses**

```bash
docker run --rm -v /tmp:/cfg envoyproxy/envoy:v1.33.0 --mode validate -c /cfg/0083-envoy-r1.yaml
```

Write the port-substituted upstream config to a **FRESH FILENAME for every revision** (`…-r1.yaml`, `…-r2.yaml`, …). This host's Docker bind mounts are STALE-CACHED: after editing a file in a bind-mounted directory the container keeps reading the PREVIOUS contents, so an in-place edit silently validates the old file.

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0083-headermatcher-absence-parity/envoy.yaml tests/fixtures/0083-headermatcher-absence-parity/envoy-rust.yaml
git commit -m "phase 75.1 task 10: fixture 0083 configs — 8 HeaderMatchers over 16 direct_response routes, backend-free"
```

---

## Task 11: Fixture `0083` — `expectations.yaml`

**Files:**
- Create: `tests/fixtures/0083-headermatcher-absence-parity/expectations.yaml`

**Interfaces:**
- Consumes: Task 10's route table and its `pNN=MATCH` / `pNN=NOMATCH` body contract.
- Produces: the file `differential::run_fixture` loads. Schema (all under `deny_unknown_fields`, re-verified at this PLAN-write against `tests/differential/src/lib.rs`):

| YAML key | type | required | note |
|---|---|---|---|
| `driver.kind` | `http1_probe_list` | yes | selects `Driver::Http1ProbeList` (`lib.rs:115-121`) |
| `driver.probes[].name` | String | **yes** | appears in failure messages |
| `driver.probes[].method` | `get` \| `options` \| `post` | **yes** | |
| `driver.probes[].path` | String | **yes** | |
| `driver.probes[].host` | String | **yes** | |
| `driver.probes[].extra_headers` | list of `[name, value]` pairs | no | default `[]` |
| `driver.probes[].expected_status` | u16 | no | |
| `driver.probes[].expected_body` | `{ kind: byte_exact, body: "<str>" }` | no | **`body:` is MANDATORY inside the rule** |
| `driver.probes[].expected_headers` | bare scalar `set_equal_modulo_allow_list` | no | |
| `equivalence.response_status` | bare scalar `exact` | no | |
| `equivalence.response_body` | `{ kind: byte_exact }` | no | |

**Sending vs omitting `x-a`.** `drive_http1` (`lib.rs:2182-2211`) emits `extra_headers` VERBATIM and in order right after `Host:`, and injects only `Host`, an optional `Content-Length` (bodies only) and `Connection: close`. So `extra_headers: [["x-a", "v"]]` sends the header, and **omitting the key entirely** makes it genuinely absent on the wire. An empty value is `["x-a", ""]`.

**Every `expected_body` below is read off the `SPEC.md` §2.3 table's UPSTREAM column** — which, after Task 2, is also envoy-rust's.

- [ ] **Step 1: Write the file**

```yaml
# Phase 75.1 (ADR-0159): 22-probe sequential burst against a backend-free HCM
# listener (`clusters: []`, direct_response only) carrying EIGHT HeaderMatchers.
# This is the FIRST differential witness of `invert_match` AND of
# `HeaderMatcher.present_match` in the whole fixture corpus.
#
# Per probe id pNN there is an ORDERED route PAIR on prefix /pNN: the first
# carries the matcher under test and answers "pNN=MATCH", the second is an
# unguarded catch-all answering "pNN=NOMATCH". So the response body IS the
# matcher's verdict, byte-exact.
#
# THE MEASURED RULE (SPEC §2.1, measured cross-proxy on envoyproxy/envoy:v1.33.0):
#   present_match(want) -> (present == want) XOR invert_match
#   any other mode, header ABSENT -> false   (invert_match is NOT applied)
#   any other mode, header PRESENT -> mode_matches(value) XOR invert_match
#   an EMPTY header VALUE counts as PRESENT.
#
# What each group witnesses:
#   p01/p06/p09 - D1 (= CF-72-1): a VALUE matcher + invert + ABSENT. Upstream
#                 DROPS; envoy-rust KEPT before phase 75.1. Three different
#                 value paths: literal compare, numeric parse, StringMatcher.
#   p08/p12     - D2: upstream `present_match: false` means the header must be
#                 ABSENT. p12 is NON-inverted - a plain, single-line matcher
#                 that silently matched every request in-tree before 75.1.
#   p07         - P1, THE GUARD. `present_match: true` + invert + ABSENT is
#                 MEASURED PARITY (both proxies KEEP). A naive uniform
#                 "absent => DROP" fix breaks this cell and mints a NEW
#                 divergence, so p07-absent expecting MATCH is load-bearing.
#   p10/p11     - parity controls. p11-empty-value and p12-empty-value pin
#                 presence-not-emptiness: an empty value is PRESENT.
driver:
  kind: http1_probe_list
  probes:
    # ---- p01: exact_match "v" + invert_match ---------------------------- D1
    - name: p01-absent-drops
      method: get
      path: "/p01"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p01=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p01-value-matches-so-invert-drops
      method: get
      path: "/p01"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p01=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p01-value-differs-so-invert-keeps
      method: get
      path: "/p01"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "zzz"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p01=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p06: range_match [1,10) + invert_match -------------------------- D1
    - name: p06-absent-drops
      method: get
      path: "/p06"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p06=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p06-non-numeric-so-invert-keeps
      method: get
      path: "/p06"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p06=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p06-in-range-so-invert-drops
      method: get
      path: "/p06"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "5"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p06=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p07: present_match true + invert_match ---------------- P1 THE GUARD
    - name: p07-absent-keeps-GUARD
      method: get
      path: "/p07"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p07=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p07-present-drops
      method: get
      path: "/p07"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p07=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p08: present_match false + invert_match ------------------------- D2
    - name: p08-absent-drops
      method: get
      path: "/p08"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p08=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p08-present-keeps
      method: get
      path: "/p08"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p08=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p09: string_match {exact: v} + invert_match --------------------- D1
    - name: p09-absent-drops
      method: get
      path: "/p09"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p09=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p09-value-matches-so-invert-drops
      method: get
      path: "/p09"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p09=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p09-value-differs-so-invert-keeps
      method: get
      path: "/p09"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "zzz"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p09=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p10: exact_match "v", no invert ------------------ parity control
    - name: p10-absent-drops
      method: get
      path: "/p10"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p10=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p10-value-matches
      method: get
      path: "/p10"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p10=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p10-value-differs
      method: get
      path: "/p10"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "zzz"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p10=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p11: present_match true, no invert --------------- parity control
    - name: p11-absent-drops
      method: get
      path: "/p11"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p11=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p11-present-keeps
      method: get
      path: "/p11"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p11=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p11-empty-value-counts-as-present
      method: get
      path: "/p11"
      host: "envoy-rust.test"
      extra_headers: [["x-a", ""]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p11=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p12: present_match false, NO invert ------------------- D2, the worst
    - name: p12-absent-keeps
      method: get
      path: "/p12"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p12=MATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p12-present-drops
      method: get
      path: "/p12"
      host: "envoy-rust.test"
      extra_headers: [["x-a", "v"]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p12=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
    - name: p12-empty-value-counts-as-present
      method: get
      path: "/p12"
      host: "envoy-rust.test"
      extra_headers: [["x-a", ""]]
      expected_status: 200
      expected_body: { kind: byte_exact, body: "p12=NOMATCH" }
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: { kind: byte_exact }
```

- [ ] **Step 2: Sanity-check the probe census**

```bash
grep -c '^    - name:' tests/fixtures/0083-headermatcher-absence-parity/expectations.yaml
```

Expected: **22**. Per group: p01=3, p06=3, p07=2, p08=2, p09=3, p10=3, p11=3, p12=3.

- [ ] **Step 3: The empty-value probes are the one wire-level unknown — verify them explicitly**

The `SPEC.md` §2.3 empty-value column was measured with `curl -H "x-a;"`, which puts `x-a:` on the wire. The harness instead emits `x-a: ` (a space before CRLF) because `drive_http1` formats `"{n}: {v}\r\n"`. HTTP header values are whitespace-trimmed, so both are an empty value — but this is the only cell in the fixture whose exact wire bytes differ from what was measured.

Before running the full fixture, probe both proxies directly with the harness's own byte shape:

```bash
printf 'GET /p11 HTTP/1.1\r\nHost: envoy-rust.test\r\nx-a: \r\nConnection: close\r\n\r\n' | nc 127.0.0.1 18083
```

Expected: body `p11=MATCH`. Repeat against a port-mapped upstream container. **If — and only if — the two proxies disagree on these two probes**, drop `p11-empty-value-counts-as-present` and `p12-empty-value-counts-as-present` from the fixture and record the finding as a new carry-forward in `PROGRESS.md`; presence-not-emptiness is already pinned in-process by Task 1's matrix, so the fixture stays a complete witness without them. Do NOT weaken any other probe.

- [ ] **Step 4: Commit**

```bash
git add tests/fixtures/0083-headermatcher-absence-parity/expectations.yaml
git commit -m "phase 75.1 task 11: fixture 0083 expectations — 22 probes across 8 matchers, all read off the measured upstream column"
```

---

## Task 12: Fixture `0083` — README and the test entrypoint

**Files:**
- Create: `tests/fixtures/0083-headermatcher-absence-parity/README.md`
- Create: `tests/differential/tests/headermatcher_absence_parity.rs`

**Interfaces:**
- Consumes: `differential::run_fixture(fixture_dir: &Path) -> Result<()>` (`tests/differential/src/lib.rs:3064`).
- Produces: the `cargo test --workspace` entry point for fixture `0083`.

**Registration cost is ONE file.** `tests/differential/Cargo.toml` has no `[[test]]` stanza (cargo autodiscovers `tests/*.rs`); the workspace root `Cargo.toml:19` already lists `tests/differential`; `.github/workflows/ci.yml:67` is `cargo test --workspace`; and there is no fixture registry — `run_fixture` takes a directory path. **No `ci.yml` edit, no workspace edit, no `[[test]]` stanza.**

- [ ] **Step 1: Write the test entrypoint**

House style is a `//!` header then a ~12-line `#[tokio::test]`, exactly as `tests/differential/tests/http1_direct_response.rs` does for fixture `0007`. The file name is the fixture slug minus the numeric prefix.

```rust
//! Phase 75.1 differential acceptance test (ADR-0159): the `HeaderMatcher`
//! ABSENCE-SEMANTICS parity witness on the ROUTE path. Drives 22 HTTP/1.1
//! probes across EIGHT header matchers at a backend-free HCM listener
//! (`clusters: []`, `direct_response` only) and requires identical
//! (status, body, header-set-modulo-allow-list) between upstream Envoy
//! v1.33.0 and envoy-rust.
//!
//! This is the FIRST differential witness of `invert_match` AND of
//! `HeaderMatcher.present_match` in the whole fixture corpus. It pins three
//! things at once:
//!   * D1 (= CF-72-1, CLOSED here) — a VALUE matcher + `invert_match` + an
//!     ABSENT header DROPS; the shared engine KEPT it before this phase
//!     (probes p01 / p06 / p09, covering the literal, numeric and
//!     StringMatcher value paths).
//!   * D2 — upstream `present_match: false` means the header must be ABSENT.
//!     Probe p12 is the NON-inverted form: a plain, single-line matcher that
//!     silently matched every request in-tree before this phase.
//!   * P1, THE GUARD — `present_match: true` + `invert_match` + ABSENT is
//!     MEASURED PARITY (both proxies KEEP). Probe `p07-absent-keeps-GUARD`
//!     is load-bearing: a naive uniform "absent => DROP" fix of the shared
//!     engine passes every other probe here and fails only that one.
//!
//! Docker-gated, backend-free (no `{{BACKEND_PORT}}` marker → no backend
//! container spawns). The ACCESS-LOG-path witness for the same rule is
//! sub-phase 75.2 (fixtures 0084 + 0085).

use std::path::PathBuf;

#[tokio::test]
async fn headermatcher_absence_parity_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0083-headermatcher-absence-parity");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 2: Write the fixture README**

Follow the house shape of `tests/fixtures/0078-accesslog-header-filter/README.md` (78-126 lines across the corpus). It must contain, as a stranger-readable record (D-3.4):

1. **What this fixture witnesses** — the §2.1 measured rule, quoted in full.
2. **The config shape** — one H1 HCM listener, `clusters: []`, `direct_response` only, 8 matchers over 16 routes as ordered pairs, discrimination by body.
3. **The full 8-row matcher table** from Task 10, with the D1 / D2 / P1 column.
4. **The 22-probe table** — probe name, path, `x-a` sent (or "omitted"), expected body — copied from Task 11 so the README and the expectations cannot drift silently.
5. **A "Why p07 is load-bearing" subsection** naming P1 explicitly: a naive uniform `absent ⇒ DROP` fix passes every other probe in this fixture and fails only `p07-absent-keeps-GUARD`.
6. **A per-side divergence table** — the three deltas of Task 10 step 2 (`node:` block envoy-rust-only, bind address, `admin:` block), and the explicit note that `codec_type: HTTP1` is written on BOTH sides and is NOT a divergence (ADR-0158 C3).
7. **Cross-references** — ADR-0156 (the phase-75 pick), ADR-0157 (the §6.1 split), ADR-0158 (the reconciliation), ADR-0159 (this sub-phase), `BEHAVIOR_CONTRACT.md` §C, and a pointer forward to sub-phase 75.2 for the access-log witness.
8. **A "Deferred (NOT in this differential)" section** listing, with the reason for each: CF-72-2's three REJECT-direction members (name-only `{ name }`; `treat_missing_header_as_empty`, which upstream ACCEPTS **and HONORS**; the top-level `contains_match` arm) — all boot-fatal on envoy-rust, so a differing config never runs and they cannot be differentially witnessed until implemented; and CF-75-1 (`exact_match: ""`, which MEASURED-degenerates to a PRESENCE match upstream).

- [ ] **Step 3: Rebuild the subject binary, then run the fixture**

```bash
cargo build -p envoy-bin
cargo test -p differential --test headermatcher_absence_parity 2>&1 | tee /tmp/t12.txt
grep -E '^test result' /tmp/t12.txt
```

Expected: `test result: ok. 1 passed; 0 failed`.

A stale `target/debug/envoy-bin` will mis-report this fixture with `unknown field` / `unknown filter` errors, so the rebuild is not optional. If dozens of differential fixtures fail at once with `client error (Connect)`, that is the Docker daemon being down, not a regression: `sudo setfacl -m u:esa:rw /dev/kvm && systemctl --user restart docker-desktop`, then re-run.

- [ ] **Step 4: Confirm the fixture census moved by exactly one**

```bash
ls -1d tests/fixtures/*/ | wc -l    # expect 83 (was 82)
git ls-files tests/fixtures/0083-headermatcher-absence-parity/ | wc -l   # expect 4 after `git add`
```

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0083-headermatcher-absence-parity/README.md tests/differential/tests/headermatcher_absence_parity.rs
git commit -m "phase 75.1 task 12: fixture 0083 README + differential entrypoint — the route-path absence-parity witness is green"
```

---

## Task 13: `BEHAVIOR_CONTRACT.md` §C rewrite, correction C2, the citation fix, and the `0078` README

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:2357-2377` (§C) and `:1878-1880` (C2)
- Modify: `tests/fixtures/0078-accesslog-header-filter/README.md:69-73`
- **NOT modified:** `docs/envoy-rust/DECISIONS.md` — ADR-0159 landed at state-2

**Interfaces:**
- Consumes: the landed behavior from Tasks 2-12.
- Produces: the contract a future phase reads. Nothing consumes it in-tree.

- [ ] **Step 1: Rewrite `BEHAVIOR_CONTRACT.md` §C**

**Boundaries, verified exact at this PLAN-write:** §C runs `:2357-2377`. The enclosing `### Phase 72 (ADR-0148/0149/0150): header_filter …` heading is at `:2334`. **The boundary a rewrite must NOT cross is `**§D Name-only + treat_missing_header_as_empty …**` at `:2379`.** Replace exactly lines 2357-2377 and nothing else.

The rewrite must:

- Retitle §C from an accepted-divergence record to the parity rule — e.g. `**§C Invert + ABSENT — the MODE-SCOPED absence rule (MEASURED; PARITY since phase 75.1).**`
- State the full §2.1 rule, including that an EMPTY header VALUE counts as PRESENT.
- **KEEP the mode-dependence warning and the "a fixer MUST preserve the `present_match` KEEP" instruction.** Both remain true and remain the guard; only re-tense them (the fixer preserved it; future refactors must continue to).
- Record **CF-72-1 CLOSED**, naming fixture `0083` as the cross-proxy pin and `matcher.rs`'s renamed pins as the in-process ones.
- Add D2 — §C omits it entirely today; there is no mention anywhere in the contract of the non-inverted `present_match: false` divergence. **The dedicated `present_match`-polarity subsection is 75.2's**, so §C here must at minimum stop asserting the old uniform-XOR rule and state the corrected `present_match` semantics inline.
- Fix the stale citation at `:2369`: the XOR is at **`matcher.rs:52`**, not `:51`.
- Update the two pinning test names to their post-Task-2 spellings: `pv4_value_matcher_absent_plus_invert_dropped_is_parity_with_upstream` and `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream`.

- [ ] **Step 2: Correct `BEHAVIOR_CONTRACT.md:1878-1880` (correction C2)**

Inside the phase-36 `ValueMatcher` block, the contract currently reads:

```
false` NEVER matches** (even when the key is present). This is a MATERIAL
DIVERGENCE from the existing `HeaderMatcherMode::PresentMatch` (`want ? present :
true`) — the RBAC `ValueMatcher::PresentMatch` does NOT use that precedent.
```

Both the parenthetical formula and the "MATERIAL DIVERGENCE" framing go stale. **The `ValueMatcher` rule itself — "`present_match: false` NEVER matches" — is CORRECT and must NOT be touched.** Restate the comparison; do not delete it. The replacement must say: the `HeaderMatcher` rule is `(present == want)` since phase 75.1; the two rules now AGREE when the key/header is PRESENT and still DIFFER when it is ABSENT (`ValueMatcher` → `false`, `HeaderMatcher` → `true`); and they remain distinct messages that must not be unified (Trap A).

> **Trap A, restated so it cannot be lost.** `HeaderMatcher.present_match` (this sub-phase) and `ValueMatcher.present_match` (RBAC / access-log metadata) are DIFFERENT fields on DIFFERENT messages with DIFFERENT, both-correct rules. Do not "unify" them. The same stale parenthetical is mirrored in source at `crates/envoy-config/src/bootstrap.rs:1704` and is corrected by Task 4 step 8.

- [ ] **Step 3: Update `tests/fixtures/0078-accesslog-header-filter/README.md:69-73`**

That bullet currently documents the invert+absent divergence as deferred and live:

```
- **Absent-drop** and **`invert_match` + absent** parity: the in-tree shared
  engine keeps absent+invert (`mode_result ^ invert_match`), diverging from
  upstream (which drops it on BOTH the route and access-log paths) — carry-forward
  **CF-72-1**. The opener uses a NON-inverted matcher; the divergence is pinned
  in-process + documented in `BEHAVIOR_CONTRACT.md` §C, not exercised here.
```

Rewrite it to say CF-72-1 is **CLOSED** by phase 75.1, that the engine now short-circuits every value mode to `false` on an absent header before the XOR, and that the cross-proxy witness is fixture `0083` (route path) with the access-log-path witness landing in sub-phase 75.2. **Leave the `0078` fixture's own configs, expectations and probes completely untouched** — never weaken a fixture.

- [ ] **Step 4: ADR-0159 — ALREADY LANDED. Do NOT create it; verify it and cite it.**

> **ADR-0159 was fired at the state-2 PLAN-write, not here.** House precedent (ADR-0153, ADR-0155, ADR-0158) is that the §6.2 empirical-reconciliation ADR lands with the PLAN, in the same session that measures it. It is already at the head of the newest-first block, immediately above `## ADR-0158`. **`DECISIONS.md` is append-only (D-3.5) — do NOT edit ADR-0159, and do NOT append an ADR-0160 for work this plan already covers.** The ledger head is **ADR-0159**; the next available number is **ADR-0160**.

Verify it is intact and unmodified, then move on:

```bash
grep -n '^## ADR-0159' docs/envoy-rust/DECISIONS.md      # expect exactly one hit
grep -c '^## ADR-0160' docs/envoy-rust/DECISIONS.md      # expect 0
git log --oneline -1 -- docs/envoy-rust/DECISIONS.md
```

`docs/envoy-rust/DECISIONS.md` is **NOT chronological**: `ADR-0001..0100` ascend from the top, then a newest-first block runs `ADR-0159..0101`. Locate anything in it with `grep -n '^## ADR-'` — never assume an offset. The file is ~3760 lines but ~310 000 tokens (single-line ADR blocks), so a whole-file read is refused by the tool; read it in offset/limit chunks.

**Only if a state-5 code review later forces a genuinely NEW decision** does a new ADR (ADR-0160) get appended, at the head of the newest-first block. For reference, ADR-0159 already records:

1. **The engine restructure SHAPE and the two rejected alternatives** — a single exhaustive tuple `match (&self.mode, value)` whose `(_, None) => return false` arm sits AFTER the `PresentMatch` arm, so arm ORDER carries the whole rule and the compiler checks exhaustiveness. Rejected: (a) an early `if let PresentMatch` + `let Some(v) = value else` split, which spreads one rule over three statements; (b) a nested match with an `unreachable!()` PresentMatch arm, which adds a panic path to a request-hot function for no benefit.
2. **The pre-validated mutation.** The state-3 RED evidence is not hoped-for: hoisting `(_, None)` above the `PresentMatch` arm was measured at the state-2 PLAN-write to turn **three** tests RED (`matcher.rs:463`, `:425`, `:348`) with real assertion failures, against a 59/59 green unmutated control from the same worktree.
3. **The amend list is THREE tests, not two**, carrying ADR-0158's correction C1 forward into the implementation.
4. **The re-derived size: ~1245 net LoC / 13 tasks**, under the ~1500 gate — so no further split, and the ADR-0157 parent split is NOT reopened.
5. **The `RangeMatch` arm's mechanical change** to `Result::is_ok_and`, and that the whole restructure was verified clippy-clean under `-D warnings` at the PLAN-write.
6. **The empty-value wire-shape caveat** of Task 11 step 3: the §2.3 measurement used `curl -H "x-a;"` (`x-a:`) while the harness emits `x-a: `; both are an empty value, and the fallback if they ever disagree is to drop the two empty-value PROBES only (the in-process matrix already pins presence-not-emptiness).
7. **§7.4 disposition CONFIRMED, not inherited:** no new fuzz target, no new corpus seed, no `ci.yml` step. Re-verified at this PLAN-write: `validate_header_matcher` (`bootstrap.rs:5559-5586`) never inspects `invert_match` and its `PresentMatch` arm is a no-op, so the behavioral change needs no validator change and adds no config surface.
8. **TWO stale figures in `75.1/SPEC.md` §6 item 3 / ADR-0158, re-measured here and NOT edited in place** (both append-only): the `matcher.rs:51` citation count is **32**, not 26; and the `DECISIONS.md` line numbers have DRIFTED — the live hits are `:2420`, `:2519`, `:2586`, `:2595`, `:2664`, `:2671` (six), not the `:373`/`:397`/`:2479`/`:2546`/`:2555`/`:2624`/`:2631` (seven) the SPEC lists, because inserting ADR-0157 and ADR-0158 pushed the later ones down ~40 lines. The correction SCOPE is unchanged: exactly two sites.
9. **A precision correction to `75.1/SPEC.md` §6 item 2:** the formula `want ? present : true` appears in ZERO `.rs` files. It lives only at `BEHAVIOR_CONTRACT.md:1879-1880`. `bootstrap.rs:1704` carries a different, separately-stale cross-reference, so the SPEC's "the same stale parenthetical is mirrored in source" phrasing names two distinct sentences as one.

- [ ] **Step 5: Verify the citation scope was respected**

```bash
grep -rn 'matcher.rs:51' --include=*.rs --include=*.md . | grep -v '.claude/worktrees' | cut -d: -f1 | sort | uniq -c | sort -rn
```

Expected: **ZERO hits in `crates/` and ZERO in `docs/envoy-rust/BEHAVIOR_CONTRACT.md`** — those are the only two sites this sub-phase corrects. Every remaining hit must be in `docs/envoy-rust/DECISIONS.md` (eight as of the PLAN-write, ADR-0159 having added two of its own), `docs/envoy-rust/STATE_HISTORY.md`, `ROADMAP.md`, `STATE.md`, or a historical phase doc. All of those are append-only (D-3.5) and must NOT be "fixed", **whatever line they are on**.

**Assert on the FILE list, never on a total count.** The count was 26 when the SPEC was written, 32 at this PLAN-write, and grows again with every artifact this sub-phase produces — `PLAN.md`, `PROGRESS.md`, `REVIEW.md` and ADR-0159 each legitimately quote the defect they fix. A rising count is expected and is not evidence of anything.

- [ ] **Step 6: Verify the §C rewrite stayed inside its boundary**

```bash
grep -n '^\*\*§[CD] ' docs/envoy-rust/BEHAVIOR_CONTRACT.md | sed -n '1,60p'
grep -c 'the shared-engine fix is carry-forward \*\*CF-72-1\*\*' docs/envoy-rust/BEHAVIOR_CONTRACT.md
```

The `**§D Name-only + treat_missing_header_as_empty` heading must still exist and must still be the block immediately following §C. The second grep must return **0** — the old "does NOT fix it" framing is gone.

> **Adjudicate greps by LINE, not by count.** A grep can legitimately return >0 because a record QUOTES the defect it fixed. If a count is non-zero, read the actual matching lines before concluding anything.

- [ ] **Step 7: Commit**

```bash
git status --porcelain docs/envoy-rust/DECISIONS.md   # MUST be empty — ADR-0159 landed at state-2
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md tests/fixtures/0078-accesslog-header-filter/README.md
git commit -m "phase 75.1 task 13: BEHAVIOR_CONTRACT §C rewritten to the measured parity rule (CF-72-1 CLOSED); C2 correction; 0078 README"
```

`DECISIONS.md` is deliberately NOT in the `git add` — it must be unmodified by state-3.

---

## After all 13 tasks: hand off to state-4, do NOT self-verify past it

State-3 ends when all 13 tasks are committed and `PROGRESS.md` records each one. The §7.5 gate is run by the **next** session (§5 state-4, `superpowers:verification-before-completion`) — a separate session per §5.1, because the context that wrote the code must not grade it (ADR-0127).

The state-4 session runs, at minimum:

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo build -p envoy-bin           # BEFORE the differential
cargo test --workspace --no-fail-fast > /tmp/state4-tests.txt 2>&1
cargo deny check
```

**Risk posture it inherits — read this before adjudicating anything.**

- **The dominant §7.5 risk is gate (b), not gate (a).** This is a shared-engine behavior change under five subsystems. Fixture `0083` will pass; the danger is a PRE-EXISTING fixture or in-process test that silently depended on the old semantics.
- **PV-9 enumerated the COMPLETE break set** at the phase-75 state-2 PLAN-write: **four in-process assertions across three tests in one file** (`matcher.rs:342-346`, `:448-451`, `:456-459`, `:503`) — all amended by Task 2 — **and nothing else.** ZERO risk to every fixture YAML (`invert_match` appears in ZERO `.yaml` anywhere in the repo; no fixture exercises `HeaderMatcher.present_match` — the only `present_match:` in fixture YAML is `0044`'s, a `ValueMatcher` on RBAC METADATA). ZERO risk to every fuzz corpus seed (`parse_bootstrap` is PARSE-ONLY and never calls `HeaderMatcher::matches`; `present_match: false` appears in ZERO of the 57 `present_match` corpus files). ZERO risk from `Default` (neither `HeaderMatcher` nor `HeaderMatcherMode` derives it, and the deserializer requires exactly one mode key). ZERO risk to the `ValueMatcher`/RBAC/metadata surface.
- **Watch nonetheless:** `0007-http1-direct-response` (the only other route-header-matching witness), `0017-http-filter-rbac`, `0018-http-filter-fault`, and `0078`-`0082` (the access-log filter family).
- **Use `--no-fail-fast` and redirect full output to a file, never `tail`.** A bare `cargo test --workspace` aborts at the first failing BINARY and never exercises the rest of the gate. Run 2-3× and diff the failing SET, then re-run each member in isolation naming its target binary — core failures are deterministic (environmental), tail failures are parallel-load flakes.
- **The documented host-flake set is CI-authoritative, not a regression:** `eds_cluster_with_neither_is_fatal`, `no_rds_is_inert`, `happy_reload_flips_endpoint_and_ticks_counters`, `happy_path_dynamic_cluster_serves_and_reports`, `wait_accept_ready_times_out_for_closed_socket`, `access_log_rf_retry_exhausted`, `upstream_h2_connection_pooling`, `network_filter_direct_response_fixture`, `send_request_maps_h2_handshake_failure_to_typed_error`, the `TcpCloseBackend` IPv6-unreachable 4-witness set (fixtures `0061`/`0062`/`0069`), and `admin_config_dump_server_info` (the `192.168.65.2` bridge-IP family). The last two families fail DETERMINISTICALLY in isolation — that is the ENVIRONMENTAL signature, not a regression.
- **`cargo deny check` can red on a freshly-published RustSec advisory against an existing dependency** even though this phase adds no dependency. Patch-bump the dep (`cargo update -p <name> --precise <ver>`); do not treat it as a phase regression.
- **Conformance is unchanged.** h2spec stays at its declared threshold and `known-failures.txt` stays **21** lines. Never trim it — this host scores h2spec 3.5/2 as PASS, so trimming on local evidence would break CI.
- **§7.4 gate (d) is vacuous by design here** — no new fuzz target, so nothing new to run. Confirm rather than assume: no `fuzz_targets/*.rs` was added and `.github/workflows/ci.yml` is untouched.

---

## Self-review against `SPEC.md`

| `SPEC.md` requirement | Task |
|---|---|
| §4.1(1) engine fix to the §2.1 rule | 2 |
| §4.1(2) the corrected doc comments (SPEC names six; this plan corrects eight) | 4 |
| §4.1(3) THREE divergence-encoding tests amended (`:342`, `:432`, `:489`) | 2 |
| §4.1(4) three guards kept green + strengthened (`:425`, `:463`, `:348`) + `:330`'s comment | 2, 4 |
| §4.1(5) full in-process engine matrix + empty-VALUE control | 1 |
| §4.1(6) consumer propagation across all FIVE call sites incl. the trait object | 5, 6, 7, 8, 9 |
| §4.1(7) new differential fixture `0083` | 10, 11, 12 |
| §4.1(8) the ~19-line test entrypoint | 12 |
| §4.1(9) `BEHAVIOR_CONTRACT.md` §C rewrite + C2 + citations | 13 |
| §6(4) `0078` README updated to CLOSED | 13 |
| §9 no fuzz target / seed / `ci.yml` step | Global Constraints; re-confirmed at state-4 |
| §10 mutation check as the guard RED evidence | 3 |
| §12 size re-derived under the gate | Size re-derivation section |

**Out of scope and deliberately absent from every task** (all 75.2, or permanently out): fixtures `0084`/`0085`; the `present_match`-polarity contract subsection; the CF-75-1 and CF-72-2 contract rows; the M74-31 five-site fold; CF-72-2's three REJECT-direction members; `exact_match: ""` (CF-75-1); any edit to the five call sites themselves; any edit to a landed ADR or to the frozen parent `75/SPEC.md`.
