# Sub-phase 76.1 — REVIEW

**Verdict: APPROVED.**

**0 Critical / 0 Issue / 6 Minor / 11 Nit.**

Per `BOOTSTRAP_PROMPT.md` §5, an approved `REVIEW.md` closes §7.5 gate **(f)** — the only
gate that was open at the end of state 4. The next session is the **§5 state-6 close-out**
for `76.1`, a SEPARATE session (§5.1: one state per session, do not chain).

No finding below blocks the close-out. The six Minors and eleven Nits are recorded as
**carry-forwards**, not as re-entry work; §5.2 is NOT triggered. The close-out session
should bank them (see §6 for the exact list, including one NEW carry-forward, **CF-76-2**,
which has real forward consequence for `76.2`).

---

## 0. How this review was conducted

Read for zero-context reconstruction (D-3.4): `BOOTSTRAP_PROMPT.md`, `STATE.md` (paged —
226 lines / 94 817 chars, a full Read truncates), `ENVOY_TARGET.md`, `ROADMAP.md` rows
`76`/`76.1`/`76.2`, and the sub-phase's `SPEC.md` (448 lines), `PLAN.md` (1685) and
`PROGRESS.md` (1534).

**Git range reviewed:** base `cf5cf85d0a2c477b90636b74fd93f6d36038f890` (the state-2
PLAN-write commit — before a single line of `76.1` existed) → head
`41a45e4c1cc5d487b9e147ca8b5fbc5b707dccb7`. The eight code commits are `3e8dd80`,
`20dd682`, `fea479b`, `27c8a05`, `68dd907`, `c8002da`, `68e39b1`, `c5e1024`.

Five READ-ONLY review dimensions were fanned out to subagents (schema; visitor +
`Serialize`; validators + placeholder; the 26-item test charter; fuzz seed + PLAN
fidelity), each forbidden to write or to run `cargo`. **Every subagent finding was
re-verified on disk by this session before entering this document**, and the decisive
measurements below were made by this session directly, not quoted. Three findings
(**M-1**, **N-3**, and the exhaustiveness observation in **M-5**) were found by this
session independently of any subagent.

**The §7.5 gate was NOT re-run** — it is run and landed, quoted verbatim in `PROGRESS.md`
§S4.0-§S4.14, and CI-confirmed (run `30585270124` on `ff2871c877457d7a198454f29855195e012b9de6`,
`162 binaries passed=2137 failed=0`). Per ADR-0127/ADR-0165 the reviewer does not fix what
it grades, and per the state-4 record it does not re-run what is already gated.

### Censuses RE-DERIVED by this session (never inherited)

| census | inherited claim | re-derived | method |
|---|---|---|---|
| ROADMAP rows / `done` / `in-progress` / `planned` | 107 / 104 / 2 / 1 | **107 / 104 / 2 / 1** ✓ | split on `' | '`, status is field **4** |
| fixture dirs | 85 | **85** ✓ | `git ls-files 'tests/fixtures/*'`, dedup to first path segment |
| differential test files | 85 | **85** ✓ | `git ls-files 'tests/differential/tests/*.rs'` |
| `ConfigError` variants | 125 | **125** ✓ | two independent counts (`#[error(` attrs; variant idents) — both 125 |
| `fuzz/.gitignore` lines / `!` lines / tracked seeds | 67 / 64 / 64 | **67 / 64 / 64** ✓ | `wc -l`; `grep -c '^!'`; `git ls-files` |
| new tests added in range | 32 | **32** ✓ (31 `bootstrap.rs` + 1 `hcm.rs`) | `git diff … \| grep -cE '^\+\s*#\[(tokio::)?test\]'` |
| net `crates/`+`tests/` LoC | added=793 deleted=19 net=774 | **793 / 19 / 774** ✓ | `git diff --numstat` |

The **32** figure independently confirms the state-4 arithmetic identity: `2105` (last CI)
`+ 32` (this sub-phase's new tests) `= 2137` (this sub-phase's CI). That the review's own
test census closes the CI identity is the strongest available cross-check that no test was
lost or double-counted.

---

## 1. Strengths

These are specific and verified, not courtesy.

1. **The presence-not-truthiness semantics — the single hardest thing in this sub-phase —
   are correct, and pinned in BOTH directions.** Both validator predicates are pure
   presence tests: `rd.path_redirect.is_some() && rd.prefix_rewrite.is_some()`
   (`crates/envoy-config/src/bootstrap.rs:4076`) and
   `rd.https_redirect.is_some() && rd.scheme_redirect.is_some()` (`:4082`). No truthiness,
   no `!s.is_empty()`, no `.unwrap_or(false)` anywhere. The matched pair T-R8
   (`bootstrap.rs:10740`, asserting the `ConfigError::RedirectSchemeRewriteConflict`
   **variant** via `matches!`, not a string) and T-A5 (`:10505`, asserting
   `Some(false)` — an assertion a bare `bool` could not even express, plus an end-to-end
   case at `:10788`) genuinely discriminate presence from value. Collapsing
   `https_redirect` to `#[serde(default)] bool` would fail to **compile** at both `:10508`
   and `:4082` — the strongest possible RED.

2. **The three-way action cardinality is exhaustive and correct.** Independently derived
   all 8 presence combinations against the match at `bootstrap.rs:2581-2597`: the four
   explicit arms cover exactly `100`/`010`/`001`/`000`, and the `_` catch-all (`:2591`)
   takes exactly the four multi-action combinations. **J3** (`redirect`+`route`, `011`) and
   **J4** (`redirect`+`direct_response`, `101`) both reject. No gap, no over-rejection.
   The `expecting` string was widened in lockstep (`:2502-2506`), as SPEC §4.4 item 4
   required "or it becomes a lie".

3. **Both `Serialize` impls are correct, including the arithmetic.** `Route`'s
   `serialize_map` length (`bootstrap.rs:2618-2620`) is `2 + name + typed_per_filter_config`,
   where the fixed `2` is `match` plus exactly one action key; verified against every key
   the impl can emit (`:2622-2633`) — the action `match` is exhaustive and every arm emits
   exactly one entry, so a third variant changes the count by zero. Correct, and correctly
   unchanged. `RouteAction`'s is `Some(1)` (`:2647`) with three one-entry arms.

4. **A vacuous assertion was caught by the implementer's own mutation testing and fixed —
   and the fix held.** `ser.contains("redirect:")` is satisfied by the unrelated field
   `port_redirect:`. Both assertions are now column-0-anchored —
   `ser.lines().any(|l| l.starts_with("redirect:"))` at `bootstrap.rs:10887` and `:10907` —
   and the failure is documented in-line at `:10881-10885` so it cannot be re-introduced.
   A repo-wide grep confirms **no** surviving unanchored `redirect:` substring assertion.
   Running three mutation checks during state 3, recording that two of them **corrected the
   plan**, and leaving the post-mortem in the code is exemplary practice.

5. **The honest 501 placeholder is exactly what `BOOTSTRAP_PROMPT.md` §6.3 asks for.**
   `crates/envoy-http1/src/hcm.rs:2121` returns the **existing**
   `BuildOutcome::Synth(synth_501(close), None)` — byte-identical to the two pre-existing
   `synth_501` call sites — rather than a fabricated 3xx, and it is pinned by T-C9
   (`hcm.rs:9731`), which asserts both `status == 501` and `detail == None`. A
   configured-but-unserved redirect is loudly wrong rather than silently wrong, and 76.2's
   replacement becomes a visible edit to a named test. The seven-line rationale comment at
   `hcm.rs:2114-2120` will save the 76.2 session real time.

6. **The full 26-item test charter is discharged — no charter item is missing.** Verified
   item-by-item across T-R1..T-R10, T-A1..T-A7, T-C1..T-C9. Seven additional tests beyond
   the charter were added, all legitimate. `git diff … | grep -c '^-\s*fn '` returns **0** —
   **nothing was deleted or weakened**, including the three pre-existing cardinality tests
   the widened message deliberately still satisfies.

7. **The fuzz corpus seed is substantive, not a token.**
   `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml` populates
   **all eight** `RedirectAction` fields across five routes (`https_redirect`,
   `host_redirect`+`port_redirect`, `path_redirect`+`response_code: FOUND`,
   `prefix_rewrite`+`strip_query`, `scheme_redirect`+`response_code: PERMANENT_REDIRECT`).
   It is a valid bootstrap that parses deep rather than dying at field one, it is **tracked**
   (`.gitignore:65` carries the `!`-un-ignore line), and it has its own parse test
   (`bootstrap.rs:10920`) plus membership in the cohort walk. CI confirmed consumption:
   `64 files found in .../fuzz/corpus/parse_bootstrap`.

8. **Scope discipline is exact.** The whole range touches **8 files** and nothing else:
   the 5 code files, `STATE.md`, `STATE_HISTORY.md`, `PROGRESS.md`. **Zero** changes to
   `.github/workflows/ci.yml`, `Cargo.toml`, `Cargo.lock`, `tests/fixtures/`,
   `tests/differential/`, `tests/conformance/`, `known-failures.txt`, `DECISIONS.md`,
   `ROADMAP.md` or `BEHAVIOR_CONTRACT.md`. All three `SPEC.md` files are **byte-unchanged**.
   No non-goal leaked in: no `location` construction, no 3xx synthesis, no `prefix_rewrite`
   path mutation, no `regex_rewrite` support. No carry-forward was opportunistically fixed.

9. **Gate (b)'s inertness claim is real, and I re-measured it.** The only fixture matching
   `redirect` is `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`, whose 7
   hits are all Prometheus metric **names** (`envoy_http_rq_redirect`,
   `envoy_http_passthrough_internal_redirect_*`, …). **Zero** of the 85 fixtures configures
   a `redirect:` route, so the new arm is genuinely inert and gate (b) is a true regression
   assertion rather than a re-baseline.

---

## 2. Issues (Must Fix)

**None.** No Critical and no Issue-severity finding. Every defect below is documentation,
diagnostics, or test-strength — the shipped config-surface semantics are correct against
all seven measured upstream rejections (J1-J7) and all measured acceptances (A1-A7).

---

## 3. Minor

### M-1 — `RouteAction` lost its doc comment; `RedirectResponseCode` inherited it

`crates/envoy-config/src/bootstrap.rs:2170-2182` and `:2245-2246`.

`RedirectResponseCode` was inserted **between** `RouteAction`'s doc comment and
`RouteAction`'s `#[derive]`. Verified against the base tree: at `cf5cf85` the block
`/// 04.3 NEW (under SPEC §3 D2): the action variant a route's HCM router invocation
dispatches into. …` sat at `:2170-2176` immediately above `#[derive(Debug, Clone,
PartialEq)]` / `pub enum RouteAction` at `:2177-2178`. Today `:2176` is followed with **no
blank line** by `:2177` (`/// 76.1 (§4.1): …`), so rustdoc treats `:2170-2182` as one run
attached to `pub enum RedirectResponseCode` at `:2185` — and `pub enum RouteAction` at
`:2246` is preceded directly by its `#[derive]` at `:2245`, with **no doc comment at all**.

Two consequences: an enum of five HTTP status names now carries public rustdoc describing
the route-action field-name oneof and asserting "both-present and neither-present are
errors"; and `RouteAction` — a `pub` type re-exported at `lib.rs:35` and the central
dispatch enum of the routing layer — is undocumented.

**Root cause is in `PLAN.md`, not in the execution.** `PLAN.md:225-226` ("insert
immediately BEFORE the `#[derive(Debug, Clone, PartialEq)]` / `pub enum RouteAction {`
pair") and `PLAN.md:318-319` (same instruction, restated at the implementation step) both
specify an insertion point *inside* the doc/item pair. The implementer followed the plan
exactly; the plan was wrong. This is the documented project failure mode — `cargo fmt`
does not reflow doc comments and there is no compiler warning, so nothing in gate (e)
catches it, which is precisely why it survived a fully green gate.

**Severity dissent, recorded rather than hidden.** This session found M-1 independently and
all five review dimensions that touched the region corroborated it; **two of the three
reviewers that assigned it a severity argued for Issue rather than Minor**, on the grounds
that it is a public-API documentation regression on a re-exported type which no gate can
catch (`envoy-config` enables no `missing_docs` lint — verified: no `#![warn]`/`#![deny]`
for it in `crates/envoy-config/src/lib.rs`). I have graded it **Minor**, because it changes
no accept/reject verdict, no wire behaviour and no runtime path, and this project's
equivalence contract is entirely about verdicts and wire behaviour — and because a §5.2
re-entry session is disproportionate to relocating a doc comment. That reasoning is a
judgement call, not a measurement, so the dissent is on the record. **M-1+M-2 are the
highest-priority carry-forward of this review**, and `76.2` edits this exact region
(`bootstrap.rs:2170-2258`), so it should close them there.

### M-2 — the orphaned doc text is itself stale: it describes a TWO-way action oneof

`crates/envoy-config/src/bootstrap.rs:2172-2176`.

Independent of where it ends up attached, the block states the route's peer keys are
`direct_response: { ... }` **OR** `route: { ... }` and that "both-present and
neither-present are errors". `76.1` made this three-way. Relevant because a naive fix for
M-1 — moving the block back above `:2245` verbatim — would re-attach a factually stale
paragraph to `RouteAction`. Both must be fixed together.

### M-3 — no accept-direction pin for a lone `path_redirect` / `prefix_rewrite`: an `&&`→`||` mutation at the path arm survives the entire suite

`crates/envoy-config/src/bootstrap.rs:4076`.

The **scheme** arm is pinned in both directions: mutating `:4082` from `&&` to `||` would
RED the `"T-A4 empty scheme_redirect"` and `"T-A5 https_redirect: false ALONE"` cases in the
accept table at `:10783-10790`. The **path** arm has no equivalent pin. Verified by census:
every `path_redirect`/`prefix_rewrite` occurrence that reaches `parse_bootstrap` sets
**both** members (`:10695`, `:10708`, `:10759`, `:10858`); the two lone-member cases
(`:10465-10466`, `:10518-10521`) call `serde_yaml::from_str::<RedirectAction>` directly and
therefore never execute `validate_hcm`. The accept table at `:10773-10792` covers
`port_redirect`, `host_redirect`, `scheme_redirect`, `https_redirect` and bare `{}` — but
neither path-arm member.

Consequence: mutating `:4076` to `||` makes envoy-rust boot-fatally reject
`redirect: { path_redirect: "/new" }` — the most common redirect config there is, and one
upstream unambiguously accepts — and **zero tests would go RED**. The shipped predicate is
correct; this is a coverage asymmetry, not a defect in behaviour. The fuzz seed contains
lone `path_redirect` and lone `prefix_rewrite` routes but a corpus asserts only "no panic",
so it cannot detect a wrongful reject. Two rows added to the `cases` array at `:10773`
would close it.

Note this gap is **not** a shortfall against the charter: SPEC §6's T-A1..T-A7 never asked
for a lone-`path_redirect` accept, and SPEC §3.2's measured acceptance table has no such
row. The under-specification is in the SPEC.

### M-4 — the three cardinality tests cannot distinguish the two cardinality messages

`crates/envoy-config/src/bootstrap.rs:10571-10572`, `:10584-10585`, `:10594-10599`.

T-R3 and T-R4 assert only `msg.contains("exactly one")` and `msg.contains("redirect")`.
**Both** emitted messages — `"…; neither is present"` (`:2587`) and `"…; more than one is
present"` (`:2593`) — contain both substrings, so a bug routing a both-present config into
the *neither*-present arm passes. T-R10 (`:10590`) asserts the message names all three arm
names but never asserts `"neither is present"` — which SPEC §6 explicitly asked of it
("error, three-way `neither is present` message"). A mutation that swaps the two string
literals REDs nothing in the suite.

The reject **verdict** is genuinely pinned (verified: if cardinality checking broke, T-R3's
`route: { cluster: backend }` under the `NO_CLUSTERS` scaffold yields an `UnknownCluster`
error containing neither substring, and T-R4 would parse clean and fail `expect_err`), and
error text is explicitly outside the equivalence contract — which is why this is Minor and
not an Issue. One line per test closes it.

### M-5 — the RDS hot-reload path installs `redirect:` routes without running either oneof validator

`crates/envoy-config/src/rds.rs:135`, reached from `crates/envoy-http1/src/rds_watcher.rs`.

`reparse_and_select_route_config` re-validates with `if let crate::RouteAction::Route(ar) =
&route.action` — an `if let`, **not** an exhaustive `match`. Adding the `Redirect` variant
therefore did **not** trip a compiler error here, and the reload path silently gained a hole:
an RDS **reload** delivering `redirect: { path_redirect: "/p", prefix_rewrite: "/q" }` is
accepted warm and installed live, while the byte-identical config at **boot** is boot-fatal
via `validate()` → `validate_hcm` → `bootstrap.rs:4076`.

**Adjudicated as Minor, not an Issue, on measured grounds.** I checked whether `76.1` minted
a *new* class of gap: it did not. `rds.rs` contains no `InvalidStatusCode`, no
`validate_data_source`, no `validate_hcm` and no `validate(` call, so the reload path
**already** skips the pre-existing `direct_response` status-range and body-shape validators
(`bootstrap.rs:4068-4071`). The partial re-validation is deliberate, documented in-line at
`rds.rs:114-131`, and deferred under ADR-0028, which is **not lifted**. The redirect variant
joined an existing, sanctioned hole rather than creating one. Blast radius today is **nil**,
because the runtime arm is the inert 501 whichever path installed the route.

**It stops being nil in `76.2`**, which makes these routes serve a real 3xx built from
fields that were never checked for mutual exclusivity. Recorded as **CF-76-2** (§6). Fixing
it here would widen `76.1` past its declared scope and is correctly out of bounds for the
close-out too.

**Generalisation worth carrying:** SPEC §2.3 leans on "the compiler enforces the seam —
`RouteAction` is matched non-exhaustively, so adding a third variant fails to compile until
every site is handled". That forcing function is **weaker than the SPEC claims**. It holds
only at genuine exhaustive `match` sites; it does not hold at an `if let` (`rds.rs:135`),
and it will not hold at the visitor's own `_` catch-all (`bootstrap.rs:2591`), where a
future fourth action variant (`non_forwarding_action`, `weighted_clusters` — SPEC §5 item 4)
would silently fall into "more than one is present" rather than failing to build. Any future
`RouteAction` variant must be added by **auditing every site by grep**, not by trusting the
build.

### M-6 — T-C8 has no second line of defence, and its value assertion is unanchored

`crates/envoy-config/src/bootstrap.rs:10910-10913`.

`assert!(ser.contains("70000"))` is unanchored: a mutation emitting `port_redirect` under a
wrong field name, nested a level deeper, or serialized as a string still satisfies it. The
T-C7 sibling has `assert_eq!(&back, route)` at `:10891` as an independent lossless-round-trip
check; T-C8 has none — the exact asymmetry the `68e39b1` post-mortem identified. A literal
round-trip is **impossible** here (`RouteAction` has no `Deserialize` impl — `:2245` derives
only `Debug, Clone, PartialEq`), so the fix is a tightened value assertion, e.g.
`ser.lines().any(|l| l.trim() == "port_redirect: 70000")`.

---

## 4. Nit

- **N-1** `crates/envoy-config/src/bootstrap.rs:2485` — the `impl Deserialize for Route`
  doc still names only two action peers (`` `direct_response: { ... }` and `route: { ... }` ``)
  on the very impl whose behaviour `76.1` widened. Same lockstep-staleness class as M-2.
- **N-2** `crates/envoy-config/src/bootstrap.rs:2227-2242` — `RedirectAction`'s eight `pub`
  fields carry zero per-field doc comments. The sibling `RouteAction_Route` (`:2267-2279`)
  documents each non-obvious field with its owning phase, which is the house convention. The
  struct-level doc (`:2209-2223`) does carry the load-bearing rationale, so nothing is
  undocumented — but a `76.2` reader landing on `https_redirect: Option<bool>` at `:2236`
  sees no local marker that the `Option` is deliberate and load-bearing.
- **N-3** `crates/envoy-http1/src/hcm.rs:9689-9695` vs `:9731` — the six-line T-C9 doc block,
  including the load-bearing instruction **"76.2 MUST flip this test."**, is attached to the
  fixture helper `redirect_placeholder_config()` (`:9696`), not to the test
  `build_response_redirect_is_not_implemented_placeholder()` (`:9731`), which carries no doc
  comment. A `76.2` session grepping for the test name finds no note; the instruction labels
  the thing `76.2` will *not* flip. *(Found independently by this session and one subagent.)*
- **N-4** `crates/envoy-config/src/bootstrap.rs:4079` and `:4085` — the `route` context field
  on both new `ConfigError` variants is `r.name.clone()`, and `Route.name` is optional and
  defaults to empty. Every conflict fixture uses an unnamed route, so the rendered message is
  always ``route `` `` and the field is never asserted; it could be replaced by
  `String::new()` with the suite still green. The shape is per-SPEC §4.3 and per `PLAN.md`
  Task 5, so this is a design that degrades rather than a deviation. The virtual-host name
  and route index are both cheaply available at the `:4060` loop.
- **N-5** `crates/envoy-config/src/bootstrap.rs:10628-10633` —
  `rejects_route_with_duplicate_redirect_key` is a bare `expect_err` with no error
  constraint, so it cannot distinguish the visitor's `M::Error::duplicate_field("redirect")`
  arm (`:2560`) from a YAML-layer duplicate-key rejection by `serde_yaml`. The branch it
  nominally covers may be untested. One `contains("duplicate")` resolves it.
- **N-6** `crates/envoy-config/src/bootstrap.rs:10457` and `:10869` — `strip_query` is pinned
  as defaulting to `false`, and fed as `true` into the T-C7 round-trip, but round-trip
  *equality* holds identically if the field were dropped on parse (`false == false`). No test
  pins a non-default `strip_query` as parsing to `true`. Outside the charter; every other
  `RedirectAction` field does have a positive value pin.
- **N-7** `crates/envoy-config/src/bootstrap.rs:10898` — `route_action_serialize_round_trips_the_redirect_key`
  is named `..._round_trips_...` but cannot round-trip (see M-6). The name over-promises.
- **N-8** `crates/envoy-config/src/bootstrap.rs:10446` and `:10847` — the two numeric
  `response_code` reject tests are bare `expect_err`. Judged **acceptable**: both run the
  scaffold that the accept suite proves parses cleanly, so the only thing wrong with the
  document is the cell under test. But the doc comment at `:10444` asserts a *mechanism*
  ("a unit enum will not accept an integer") that no assertion verifies — `serde_yaml` may
  instead resolve `302` as a variant *name* and emit an unknown-variant error. Same verdict,
  possibly different mechanism, unverified claim.
- **N-9** `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_redirect_action.yaml:28,32` —
  only **two** of the five `RedirectResponseCode` wire names appear in any tracked corpus
  seed. Re-measured across the whole corpus: `FOUND` and `PERMANENT_REDIRECT` appear (both
  in this seed); `MOVED_PERMANENTLY`, `SEE_OTHER` and `TEMPORARY_REDIRECT` appear in **no**
  tracked seed. Since the enum deserializes by exact name, libFuzzer cannot realistically
  synthesize the missing tokens, so three of five arms are unreachable from the corpus. Unit
  tests cover all five (`bootstrap.rs:10390`), so this is coverage *shape*, not a
  correctness gap. Cheap fix if `76.2` touches the seed: vary the codes across the five
  routes.
- **N-10** `PROGRESS.md:24-32` — the Contents block lists 7 sections, but the file gained
  15 more (`## S4.0` … `## S4.14`) in `ff2871c`/`41a45e4` and Contents was never extended.
  Navigational only. **Not fixable by this review** — `PROGRESS.md` is a landed historical
  artifact (D-3.5) and this session must not edit it.
- **N-11** `PROGRESS.md:1434-1435` vs `PROGRESS.md:602-603` — §S4.13 briefs the state-5
  reviewer that §6 records "four deliberate deviations from `PLAN.md`, **all of which ADD
  coverage**", while §6 itself correctly says "**Three** ADD coverage" (deviation 1 replaces
  a misaimed mutation with a sound one; it adds no coverage). A mis-brief of the incoming
  reviewer, caught here by reading §6 directly rather than trusting the §S4.13 summary —
  which is the standing rule. Same non-editable status as N-10.

---

## 5. Deliberate decisions verified, NOT filed as findings

Recorded so a later reviewer does not re-raise them.

- **The `Option`s are load-bearing.** `Option<bool>` / `Option<String>` on the four oneof
  members is correct and required; the validators test `.is_some()` and only `.is_some()`.
- **`port_redirect` has no range bound.** Correct — upstream accepts `0` and `70000`
  (`bootstrap.rs:2230`, pinned at `:10475-10486` asserting `Some(70000)` survives verbatim).
- **J2 rejects via `deny_unknown_fields`, not via a oneof error.** A DIFFERENT mechanism with
  the SAME verdict; error text is outside the equivalence contract.
- **The cardinality wording.** `neither is present` kept verbatim, `both are present` →
  `more than one is present`, catch-all `_` holding the match at five arms; three
  pre-existing tests deliberately unedited and still green. ("neither" for a three-way choice
  is a mild infelicity, deliberately retained for test compatibility — noted, not filed.)
- **The 501 placeholder** is correct behaviour at this sub-phase, not a stub to finish.
- **No new fixture, no new fuzz target, no `ci.yml` edit** — all correct; gate (a) is
  vacuously met and both `PROGRESS.md` §4 and §S4.10 say so explicitly.
- **Net LoC 774 vs the ≈515 projection** — a +50% overshoot living almost entirely in the
  test half. Does not re-open the §6.1 gate (52% of ~1500, 8 tasks against ~25). Not
  re-litigated (ADR-0169 / ADR-0170 stand).
- **The four `PLAN.md` deviations recorded in `PROGRESS.md` §6** — all four ADD coverage and
  none weakens anything; read and accepted. The `hcm.rs` test-import citation drift
  (`:2353`, not `:2361`) is correctly recorded there.
- **CF-76-1, CF-75-2..CF-75-6** all remain OPEN and untouched, correctly.

---

## 6. Carry-forwards for the state-6 close-out to bank

**One NEW:**

- **CF-76-2 (NEW, opened by this review)** — the RDS **hot-reload** path
  (`crates/envoy-config/src/rds.rs:135`) installs `redirect:` routes without running either
  intra-`RedirectAction` oneof validator, because its re-validation uses `if let
  RouteAction::Route(ar)` rather than an exhaustive `match`. Same config, two verdicts
  depending only on whether it arrived at boot or on reload. Inert today (the runtime arm is
  the 501 placeholder); **becomes live behaviour in `76.2`**. The pre-existing
  `direct_response` validators are skipped on the same path, so this is a widening of an
  ADR-0028-sanctioned hole, not a new one. Owner: `76.2` should note it in its SPEC, or it
  becomes its own phase. **Do NOT fix it in the `76.1` close-out.**

**Carried from this review (Minor):** M-1, M-2 (fix together — a verbatim restore of M-1
re-attaches stale text), M-3, M-4, M-6.

**Carried from this review (Nit):** N-1 through N-11. N-10 and N-11 are defects in the landed `PROGRESS.md` and are NOT editable by any later session (D-3.5, append-only historical artifact) — they are recorded for accuracy, not for repair.

None of these is a re-entry trigger. `76.2` touches `bootstrap.rs:2170-2258`,
`hcm.rs:2110-2125` and the cardinality/`Serialize` sites directly, so M-1/M-2/N-1/N-2/N-3
are cheapest to close there; M-3/M-4/M-6/N-5/N-6/N-7/N-8 are self-contained test edits
closable in any later phase touching this surface.

---

## 7. Assessment

**Ready to close out? YES — APPROVED.**

**Reasoning.** `76.1` is a config-surface slice that had to get one subtle thing exactly
right — that upstream's protobuf oneofs are exclusive on field **presence**, not on value —
and it does, in both the model (`Option<bool>` / `Option<String>`), the validators
(`.is_some()` and nothing else), and a matched pair of tests that discriminate the two
failure modes and would fail to *compile* under the most likely wrong model. The three-way
cardinality is exhaustive over all eight presence combinations, both `Serialize` impls are
arithmetically correct, the full 26-item charter is discharged with nothing deleted or
weakened, scope discipline is exact at 8 files, and the implementer's own mutation testing
caught and fixed a vacuous assertion and corrected the plan twice. Every defect found is
documentation, diagnostics, or test-strength — none changes the accept/reject verdict on any
of the seven measured upstream rejections or any measured acceptance, which is the entire
differential surface of this sub-phase.

The most consequential finding, **M-5/CF-76-2**, is a pre-existing ADR-0028-sanctioned gap
that the new variant joined rather than created, is inert at `76.1`, and is correctly out of
scope here — but it must reach `76.2` before the runtime lands, which is why it is banked as
a named carry-forward rather than left in prose.

§7.5 gate **(f)** is now MET. Gates (a), (b), (d), (e) were MET and (c) CI-confirmed at
state 4. **All six gates are closed.** The next session is the §5 state-6 close-out for
`76.1` — a SEPARATE session.
