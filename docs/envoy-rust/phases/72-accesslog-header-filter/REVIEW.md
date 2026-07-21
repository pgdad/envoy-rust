# Phase 72 — access-log `header_filter` — §5 state-5 CODE-REVIEW

> `superpowers:requesting-code-review`, run in its OWN session per §5.1. Reviews
> the phase-72 diff (`git diff c741832..3e8dce3`, commits T1 `e0ba1ca` → T12
> `3e8dce3`) and LIVE-PROBES the untested compositions across BOTH proxies
> (memory `state5-must-probe-untested-compositions`: a GREEN gate proves the code
> does what its tests ASK, not that the tests ask the right question). Base =
> state-4 commit `9529a91eed20ecde2cd29a438d3137147661d8d1`, CI `completed` /
> `success` on the FULL 40-char SHA (run `29788989303`).

## Verdict: **NOT APPROVED — re-enter §5.2 state-3** (one Important MUST-FIX + one Important coverage gap)

The implementation is otherwise clean and correct-as-scoped (the trait-object
seam, the 3-arm cardinality, the `&mut` validation delegation, the H1/H2 emit
threading, and fixture `0078` are all sound — see **Strengths**). But a
**LIVE-PROBE of the headline PV-4 divergence measured that the phase's sole
`absent+invert` test pin exercises the ONE matcher mode where envoy-rust and
upstream AGREE, and labels that parity case as the divergence — while the
genuinely-divergent mode is pinned by NO test.** Per the state machine
(BOOTSTRAP §5.2) a review with issues re-enters step 3 (state-3 implementation),
NOT step 6. ROADMAP row `72` stays `in-progress`.

---

## LIVE-PROBE evidence (MEASURED this session — envoy-rust DEBUG `envoy-bin` vs. `envoyproxy/envoy:v1.33.0`, port-mapped, graceful-stop flush)

### Probe 1 — multi-sink MIXED filters (M71-5 gap) → **PARITY** (de-risked)

One H1 HCM, `direct_response /x → 200`, TWO file sinks:
sink **A** `filter: header_filter{ header:{ name:x-log, string_match:{ exact:"yes" }}}`,
sink **B** `filter: status_code_filter{ comparison:{ op:EQ, value:{ default_value:200 }}}`.
Drove `GET /x` with `x-log:yes`, with `x-log:no`, and with no header.

| sink | envoy-rust | real Envoy v1.33.0 |
|---|---|---|
| A.log (header_filter) | `A STATUS=200 PATH=/x` (**1 line** — only `x-log:yes`) | `A STATUS=200 PATH=/x` (**1 line**) |
| B.log (status EQ 200) | 3 lines (all three) | 3 lines |

**Byte-identical.** The per-sink `continue` in the emit loop
(`crates/envoy-http1/src/hcm.rs:1508-1526`) gates each sink independently — no
cross-sink leakage of the `req.headers` slice. M71-5 stays a documented
carry-forward but is now MEASURED parity, not a latent risk.

### Probe 2 — PV-4 `absent + invert_match` → **the headline divergence is MODE-DEPENDENT** (the must-fix)

`header_filter{ header:{ name:x-log, <mode>, invert_match:true }}`, driving a
request WITH `x-log` present vs. one with it ABSENT, distinguished by path:

| matcher mode, `invert:true`, **ABSENT** header | envoy-rust | real Envoy v1.33.0 | verdict |
|---|---|---|---|
| `present_match: true` | **KEEP** | **KEEP** | **PARITY** |
| `string_match:{ exact:"yes" }` (value-based) | **KEEP** | **DROP** | **DIVERGENCE** |

(Present-header cases match on both proxies for both modes: present+match+invert →
DROP; present+mismatch+invert → KEEP.)

**Upstream Envoy special-cases a MISSING header:** for a value-based matcher
(exact/prefix/suffix/regex) a missing header is an unconditional no-match that
`invert_match` does NOT resurrect (→ DROP); for `present_match` the present-check
is `false` and `invert_match` DOES flip it (→ KEEP). envoy-rust's shared engine
(`crates/envoy-config/src/matcher.rs:51`) applies `mode_result ^ invert_match`
**uniformly**, so it KEEPS absent+invert in BOTH modes — matching upstream for
`present_match` (coincidence) but diverging for value matchers.

---

## Findings

### F-1 — [**Important — MUST-FIX**] The PV-4 divergence pin exercises the non-divergent mode; the genuine divergence is untested and mischaracterized

`crates/envoy-config/src/matcher.rs:397` —
`pv4_absent_plus_invert_is_kept_inherited_shared_engine_boundary` and the
`header_match_trait_delegates_to_inherent_engine` invert leg (matcher.rs:431)
both use `hm_inverted("x-log", HeaderMatcherMode::PresentMatch(true))` and assert
`.matches(&[]) == true` with the comment *"in-tree engine keeps absent+invert
(diverges from upstream — CF-72-1)"*.

**MEASURED (Probe 2): `present_match(true) + invert + absent` is PARITY — upstream
ALSO keeps it.** The comment's claim is false for the mode the test uses. The
actually-divergent case is `value-based-matcher + invert + absent`
(exact/prefix/suffix/regex → upstream DROP, envoy-rust KEEP), and NO test
exercises it: `invert_match_inverts_exact_match_result` (matcher.rs:384) covers
only PRESENT cases (mismatch→keep, match→drop), never the absent case.

- **Failure scenario / why it matters:** the phase's HEADLINE measured divergence
  (PV-4, called out as such in SPEC §0/R-0.6, PLAN, ADR-0149) is pinned with a
  test that proves the *opposite* mode and documents parity as divergence. The
  true divergence is unpinned. A future **CF-72-1** fixer, reading this pin,
  would reasonably "fix" the shared engine to DROP `absent+invert` uniformly — and
  thereby BREAK `present_match+invert+absent` (which upstream KEEPS), introducing
  a NEW divergence. The mislabeled pin actively misleads.
- **No runtime code change** is required — the value-matcher divergence is a
  correctly-scoped pre-existing phase-04.2 shared-engine boundary (CF-72-1), and
  fixture `0078` uses a non-inverted matcher, so nothing that ships behaves
  wrongly beyond the already-deferred boundary. This is a **test-accuracy +
  documentation** defect on the phase's headline risk.
- **Fix (state-3, TDD):**
  1. Correct the PV-4 pin to exercise the ACTUAL divergence with a value matcher:
     `ExactMatch/StringMatch + invert + ABSENT` → assert envoy-rust KEEP, comment
     it as the MEASURED-divergent-from-upstream (DROP) case = CF-72-1.
  2. Add/relabel a companion pin: `present_match(true) + invert + absent` →
     assert KEEP, comment it as **PARITY** with upstream (NOT a divergence) — so
     the CF-72-1 fixer knows this mode must stay KEEP.
  3. Refine the CF-72-1 characterization to "value-based matcher + invert +
     absent" (not the blanket "absent+invert") in `DECISIONS.md` (ADR-0149 PV-4
     note), `BEHAVIOR_CONTRACT.md` §C (lines ~2358-2365), `PLAN.md` PV-4 (line
     28), and `PROGRESS.md`.
  4. Fire **ADR-0151** — the corrected, mode-scoped PV-4/CF-72-1 characterization
     (a MEASURED surprise: the pinned "divergence" was parity; the divergence is
     value-matcher-specific).

### F-2 — [**Important**] H2 `header_filter` header-slice threading is wired but UNASSERTED

`crates/envoy-http2/src/hcm.rs:1138` threads `&envoy_req.headers` into the widened
`should_log`, but no H2 test exercises `header_filter` keep/drop or passes a
non-empty header slice (the only H2 emit-gate test,
`h2_response_flag_filter_suppresses_no_flag`, is the response-flag arm). If the
threaded field were wrong, no test would catch it — H1's equivalent is caught by
the live differential `0078`, H2's is not.

- Divergence risk is LOW (both HCMs call the identical `should_log(status,
  flags, headers)` signature; the correctness sub-review confirmed
  `envoy_req.headers` is the correct post-decode downstream snapshot). But it is a
  genuine coverage hole in shipped runtime code.
- **Fix (state-3):** add a cheap H2 unit test asserting `header_filter`
  keep-on-match / drop-on-mismatch-AND-absent through the H2 emit gate. (The full
  H2 differential fixture stays deferred as **M71-6** — unchanged.)

### F-3 — [Minor] Multi-sink mixed filters (M71-5) untested — MEASURED parity (Probe 1)

No in-process test has 2+ sinks with different filter arms. Probe 1 measured
byte-exact parity across both proxies, so the gap is de-risked. A cheap
in-process multi-sink mixed-filter test (sink A `header_filter`, sink B
`status_code_filter`, one request landing differently on each) would close M71-5;
optional in the state-3 re-entry.

### F-4 — [Minor] Stale "two arms" comment

`crates/envoy-config/src/bootstrap.rs:5169` still reads *"With two arms the > 1
(both-set) branch is now REACHABLE"* — there are now THREE arms. The schema
docstrings (713-732) and the `compile_access_log_filter` docstring were correctly
updated to "three". Fix opportunistically in state-3.

### F-5 — [Minor] safe_regex / range membership not exercised through the access-log seam

`header_filter_membership_across_modes_and_absent_drop` (envoy-http1
hcm.rs:4769) covers exact/prefix/suffix/present/string_match end-to-end but not
`safe_regex`/`range` through `LogFilter::Header`. Both are covered on the inherent
engine (`matcher.rs` SafeRegex/Range cells) and the trait delegation is proven
verbatim (`header_match_trait_delegates_to_inherent_engine`), so coverage is
transitively sound. No action required; noted for completeness.

### F-6 — [Minor] SPEC/PLAN retain the pre-change `H=%REQ(X-LOG)%` format string

`SPEC.md` §0/§2.1/§5 and `PLAN.md` (726/774) still show
`STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% H=%REQ(X-LOG)%`. The shipped fixture,
README, `.rs` doc, and `BEHAVIOR_CONTRACT` correctly reflect the T8 correction
(`STATUS=200 PATH=/x`; `%REQ(NAME)%` is allow-list-only, SPEC §2.2 boundary).
These are pre-implementation planning artifacts and the change is logged in
PROGRESS T8 — expected historical drift, no action required.

---

## Strengths (verified — correctness + parity sub-review, and re-checked directly)

- **ADR-0150 trait-object seam is genuinely non-recursive.**
  `impl HeaderMatch for HeaderMatcher { fn matches(&self,h){ self.matches(h) } }`
  (`matcher.rs:63`) resolves the inner call to the INHERENT `HeaderMatcher::matches`
  (Rust method resolution: inherent methods shadow same-named trait methods for a
  compatible receiver). Empirically pinned — it would stack-overflow via
  `Arc<dyn HeaderMatch>` if it recursed. Correctly avoids the
  `envoy-config → envoy-accesslog → envoy-config` cycle.
- **`compile_access_log_filter`'s `unreachable!()` is genuinely unreachable.**
  `validate_access_logs` (bootstrap.rs:5159) destructures all three arms with no
  `..` and rejects `set_arms != 1`; `parse_bootstrap` → `validate` → `validate_hcm`
  → `validate_access_logs` always runs before `from_config` → compile. All-None
  and multi-arm configs can never reach compile.
- **`&mut [AccessLog]` compile path is sound** — sole caller `validate_hcm`
  (bootstrap.rs:3841) already holds `&mut hcm`; the in-place SafeRegex compile
  survives to runtime (same mutate-then-consume pattern as route matchers);
  pinned by `header_filter_safe_regex_is_compiled`.
- **No new panic risk** — the only `.expect()` (SafeRegex, matcher.rs:35) is
  guaranteed compiled by validation before any runtime `matches`.
- **`Eq`/`PartialEq` drop** handled at the one comparison site
  (`runtime_key_is_rtds_inert`, hcm.rs) by comparing the inner
  `StatusCodeComparison`.
- **Fixture `0078`** is a true cross-proxy byte-exact witness (`STATUS=200
  PATH=/x`, dropped-FIRST/kept-LAST), and the `%REQ(NAME)%` allow-list correction
  is faithfully documented. **DECISIONS.md ADR-0150** accurately describes the
  seam.
- **§7.5 gate** (state-4) GREEN and CI `success` on the base SHA.

---

## Disposition

Re-enter **§5.2 state-3** (`superpowers:systematic-debugging` → `test-driven-development`)
to address **F-1** (MUST-FIX) and **F-2**, refresh the CF-72-1 characterization
docs, and fire **ADR-0151** (the corrected mode-scoped PV-4 divergence). F-3/F-4
optional. NO runtime behavior change — the value-matcher `absent+invert`
divergence stays the correctly-scoped CF-72-1 boundary; the fix is to pin and
document it ACCURATELY. ROADMAP row `72` stays `in-progress`.
