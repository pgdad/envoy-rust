# Sub-phase 75.1 — `HeaderMatcher` absence semantics: the MODE-SCOPED engine fix + the ROUTE-path differential witness — §5 state-5 CODE-REVIEW

> `superpowers:requesting-code-review`, run in its OWN session per §5.1 / ADR-0127
> (the context that wrote an artifact must not grade it). Reviews the sub-phase-75.1
> implementation diff
> `git diff 2856976f95da1bf920d6b914220b9099c76acc11..1bbc3727a5ba7f4869ce71c4b48b988ab437377d`
> (18 files, +3145/−116; the 13 task commits `f68b160` → `a845b6d`, the `cargo fmt`
> normalisation `d601365`, the state-3 `STATE.md` advance `ee86574`, and the state-4
> verification commit `1bbc372`). Base = the last pre-implementation commit
> `2856976f…`. Session HEAD = `1bbc3727a5ba7f4869ce71c4b48b988ab437377d`, CI
> `completed`/`success` on the FULL 40-char SHA (run `30177720891`, both jobs green
> at full step counts **15** and **13**, no runner-starvation signature — re-confirmed
> first thing this session).
>
> **The §7.5 gate was NOT re-run.** It was RUN and ADJUDICATED GREEN (a)–(e) at
> state-4 and its evidence is quoted verbatim in `PROGRESS.md`
> `# §5 STATE-4 VERIFICATION`. Gate (f) IS this review. I re-measured only what a
> specific review question needed.
>
> **Method** (memory `state5-must-probe-untested-compositions`): four FRESH
> zero-context read-only reviewers fanned out across independent dimensions
> (consumer propagation, fixture `0083` end-to-end, docs + contract, plan/TDD
> fidelity), each forbidden to write or to run `cargo`. The MAIN session then made
> every decisive measurement ITSELF — a **live cross-proxy probe** of the untested
> compositions against both proxies, and a **mutation check** in a scratch
> `git worktree` (memory `mutation-checks-collide-with-parallel-subagents`) — and
> **RE-VERIFIED every subagent finding on disk before adopting it.** Two reported
> findings were dropped as unverifiable; every finding below I confirmed myself with
> the file:line quoted. **A green gate proves the code does what its tests ASK, not
> that the tests ask the right question.**

---

## Verdict: **APPROVED — 0 Critical, 0 Important. Next = the §5 state-6 CLOSE-OUT.**

**No behavioral defect. No divergence introduced.** The entire behavioral change is
ONE `match` expression in `crates/envoy-config/src/matcher.rs:45-67`, and it
implements the MEASURED §2.1 rule exactly. I verified independently that **outside
`matcher.rs`, every production-side line in the whole 16-commit diff is a comment** —
the five call sites' production code is untouched, exactly as `SPEC.md` §4.2 scoped
it (evidence in *Scope discipline* below).

The three things that could have gone wrong, all measured rather than assumed:

- **The P1 guard is genuinely load-bearing.** My own mutation check (scratch
  worktree, `Compiling` confirmed, unmutated control `60 passed; 0 failed` from the
  same tree) hoists `(_, None)` above the `PresentMatch` arm and turns **four** tests
  RED with semantic assertion text naming the guard. The arm order is not decorative.
- **The untested compositions are PARITY.** Fixture `0083` carries exactly one
  matcher per route, so a matcher LIST mixing an inverted value matcher with a
  `present_match` was unexercised anywhere. I live-probed it cross-proxy: **all 7
  cells across two compositions are byte-identical** to `envoyproxy/envoy:v1.33.0`.
- **A plausible regression hypothesis was FALSIFIED, and the truth is better than the
  claim.** I hypothesised that 75.1 might have *introduced* a divergence at
  `exact_match: ""` + `invert_match` + ABSENT (a cell CF-75-1 never measured). It did
  the opposite: pre-75.1 that cell KEPT while upstream DROPS; post-75.1 it DROPS.
  **75.1 silently closed one more CF-75-1 cell than anyone recorded.**

The five Minor findings are **documentation- and evidence-accuracy only** — three in
text this very phase wrote or was required to correct, one **pre-existing,
out-of-scope cross-proxy divergence I measured** (duplicate-header comma-join) that
belongs to a future phase, and one evidence-provenance defect in `PROGRESS.md` whose
*substance* I independently reproduced. None changes runtime behavior; none blocks the
close-out. Per §5.2 a re-entry is for resuming *implementation*, and nothing here
requires code.

**Size, for the record (measured, not a finding):** the phase landed **+1553 / −96 =
1457 net LoC** excluding the running log and the `STATE` files, against `SPEC.md`
§12's projection of **~1210** and the §6.1 gate of **~1500**. Still under the gate, so
the "no further split" call holds — but with ~43 lines of margin rather than the ~290
the projection implied. The overrun sits in the consumer propagation tests and the
`0083` README (219 lines vs a projected ~120). Worth noting only because the §6.1
split was adjudicated on the smaller number.

ROADMAP rows `75` / `75.1` stay `in-progress` — no flip until the state-6 close-out.
`DECISIONS.md` is UNTOUCHED by this review (see *Why no ADR-0160* below).

---

## LIVE-PROBE evidence (MEASURED this session — envoy-rust DEBUG `envoy-bin` vs. `envoyproxy/envoy:v1.33.0`)

Method per memories `state0-recon-docker-needs-port-mapping` (port-mapped `docker -p`,
never `--network host`) and `docker-bind-mounts-are-stale-cached-on-this-host` (a
FRESH directory per config revision). `cargo build -p envoy-bin` was run first
(memory `differential-harness-uses-debug-envoy-bin`). One config per side differing
ONLY in the house divergences (`admin` block dropped, listener bind `0.0.0.0` →
`127.0.0.1`); `node:` scalars quoted (the YAML-1.1 `cluster: y` trap). Backend-free
throughout (`clusters: []`, `direct_response` only). All probe containers removed
afterwards; the parallel workstream's `quizzical_goldstine` was LEFT ALONE.

**The probe harness is proven non-vacuous** — it detected four divergences in this
very run (e01/e02 value cells, d01, j01/j03), so a PARITY result is a real
measurement, not a silent skip.

### Probe group 1 — the two untested COMPOSITIONS → **FULL PARITY** (the phase's biggest untested surface)

`c01` = one route, header list of TWO matchers ANDed: `x-a: exact_match "v" +
invert_match` **AND** `x-b: present_match true`.
`c02` = `x-a: present_match false` **AND** `x-b: exact_match "v"`.

| probe | request | upstream | envoy-rust | verdict |
|---|---|---|---|---|
| c01 | no `x-a`, no `x-b` | NOMATCH | NOMATCH | PARITY |
| c01 | `x-b: 1` only (**the D1 cell inside a list**) | NOMATCH | NOMATCH | **PARITY** |
| c01 | `x-a: v`, `x-b: 1` | NOMATCH | NOMATCH | PARITY |
| c01 | `x-a: zzz`, `x-b: 1` | MATCH | MATCH | PARITY |
| c02 | no `x-a`, no `x-b` | NOMATCH | NOMATCH | PARITY |
| c02 | `x-b: v` (**the D2 cell inside a list**) | MATCH | MATCH | **PARITY** |
| c02 | `x-a: 1`, `x-b: v` | NOMATCH | NOMATCH | PARITY |

The two cells in bold are the ones that would have failed before 75.1 and that no
test or fixture exercises in composition. Both are parity. The composition is ANDed
identically on both sides — `hcm.rs:2165` is `.all(|m| m.matches(headers))` over
matchers evaluated independently by a pure function, so this is correct by
construction, and it is now also **measured**.

### Probe group 2 — `exact_match: ""` + `invert_match` + ABSENT → **75.1 CLOSED an unrecorded divergence**

CF-75-1 banked only three NON-inverted cells for `exact_match: ""`
(`75/SPEC.md:208`). The inverted absent cell was never measured. It matters because
`exact_match: ""` degenerates to a PRESENCE match upstream, so one could reasonably
expect upstream to carry the absent header into the inversion (as it does for a real
`present_match`) — which 75.1's value-mode short-circuit would then break.

| probe | matcher | request | upstream | envoy-rust | verdict |
|---|---|---|---|---|---|
| e01 | `exact_match: ""` + invert | absent | NOMATCH | NOMATCH | **PARITY (newly closed)** |
| e01 | " | `x-a: v` | NOMATCH | MATCH | diverges — **known CF-75-1** |
| e01 | " | `x-a:` (empty) | NOMATCH | NOMATCH | PARITY |
| e02 | `exact_match: ""` (control) | absent | NOMATCH | NOMATCH | PARITY |
| e02 | " | `x-a: v` | MATCH | NOMATCH | diverges — **banked CF-75-1** |
| e02 | " | `x-a:` (empty) | MATCH | MATCH | PARITY |
| e03 | `string_match: {exact: ""}` + invert | absent | NOMATCH | NOMATCH | PARITY |
| e03 | " | `x-a: v` | MATCH | MATCH | PARITY |
| e03 | " | `x-a:` (empty) | NOMATCH | NOMATCH | PARITY |

**Read-off.** Upstream applies the value-mode absence short-circuit to
`exact_match: ""` too — the `""` degeneracy affects only the *value comparison*, not
the *absence* axis. Pre-75.1 envoy-rust computed `None == Some("")` = `false`, then
`false ^ true` = **KEEP**, diverging; the diff's removed line
`- HeaderMatcherMode::ExactMatch(lit) => value == Some(lit.as_str()),` is the proof.
Post-75.1 it returns `false` before the XOR → **DROP** → parity. So the mode-scoped
fix closed a CF-75-1 cell as a side effect. **CF-75-1 is correctly still OPEN** (its
present-value cells still diverge, both polarities), but its record is now
incomplete — see *Carry-forward disposition*.

`e03` additionally confirms `string_match: { exact: "" }` does **not** degenerate on
either side and is full parity across all three variants.

### Probe group 3 — DUPLICATE header values → **a real divergence, PRE-EXISTING and out of scope**

| probe | matcher | request | upstream | envoy-rust | verdict |
|---|---|---|---|---|---|
| d01 | `exact_match: "v"` | `x-a: v` ×2 | NOMATCH | **MATCH** | **DIVERGE** |
| d01 | " | `x-a: v`, `x-a: zzz` | NOMATCH | **MATCH** | **DIVERGE** |
| d01 | " | `x-a: zzz`, `x-a: v` | NOMATCH | NOMATCH | agree (coincidentally) |
| d02 | `present_match: false` | `x-a: 1`, `x-a: 2` | NOMATCH | NOMATCH | PARITY |

**Decisive control** (second probe pair, both headers `x-a: v`):

| probe | matcher | upstream | envoy-rust |
|---|---|---|---|
| j01 | `exact_match: "v,v"` | **MATCH** | NOMATCH |
| j02 | `present_match: true` | MATCH | MATCH |
| j03 | `prefix_match: "v,"` | **MATCH** | NOMATCH |

j01/j03 confirm the mechanism beyond doubt: **upstream comma-joins duplicate header
values before value matching; envoy-rust matches only the FIRST occurrence**
(`crates/envoy-config/src/matcher.rs:41-43`, `.find(...)`). j02 confirms the
*presence* axis is unaffected, which is why 75.1's rule is untouched by this.

**This is NOT a 75.1 defect.** The `.find(...)` lines are context (unchanged) lines in
the diff — the behavior predates the phase, and 75.1's scope is the ABSENCE axis, not
multi-value handling. I record it as a new carry-forward rather than a finding
against this diff.

---

## MUTATION check (MEASURED this session, in a scratch `git worktree`)

Run in `git worktree add … HEAD --detach`, never in the main tree (memory
`mutation-checks-collide-with-parallel-subagents`); the main tree was verified clean
and at `1bbc372` after the worktree was removed.

**Unmutated control, same worktree:** `cargo test -p envoy-config --lib matcher` →
`test result: ok. 60 passed; 0 failed`, with `Compiling envoy-config` present in the
output (memory `mutation-check-needs-forced-rebuild` — a cached binary would have
given a false pass).

**Mutation:** hoist `(_, None) => return false` ABOVE the `PresentMatch` arm — i.e.
exactly the naive uniform "absent ⇒ DROP" the SPEC warns against.

**Result:** `test result: FAILED. 56 passed; 4 failed`, `Compiling envoy-config`
present. The four:

| test | failure text |
|---|---|
| `absence_semantics_matrix_matches_measured_upstream` | `matcher.rs:674` — `present(true)+invert: ABSENT must stay KEEP (P1 — MEASURED PARITY)` |
| `pv4_present_match_absent_plus_invert_kept_is_parity_with_upstream` | `matcher.rs:509` — `present_match absent+invert = KEEP on BOTH proxies (PARITY, not a divergence)` |
| `invert_match_inverts_present_match_result` | `matcher.rs:459` — `assertion failed: m.matches(&[])` |
| `present_match_false_matches_when_absent` | `matcher.rs:378` — `assertion failed: m.matches(&[])` |

All four are **semantic assertion failures**, not startup/argument errors (memory
`mutation-red-needs-unmutated-control` — the failure TEXT was read, and the control
was run from the same tree). This independently reproduces the state-3 claim of FOUR
RED guards and proves the pins are non-vacuous.

---

## Independent re-verification of the phase's own claims

| claim | verdict | evidence |
|---|---|---|
| Fixture `0083` green on the reviewed tree | **CONFIRMED** | `cargo test -p differential --test headermatcher_absence_parity` → `1 passed; 0 failed`, 1.04 s (backend-free + warm image; the ~1 s green is normal, not a skip) |
| No production code edited at the five call sites | **CONFIRMED** | every hunk in `fault.rs` (178), `jwt_authn.rs` (649), `rbac.rs` (1388), `envoy-http1/hcm.rs` (9984), `envoy-http2/hcm.rs` (6731) starts AFTER that file's `#[cfg(test)]` (79 / 240 / 340 / 2350 / 1297) |
| Production-side edits elsewhere are comment-only | **CONFIRMED** | every changed non-comment line in `envoy-accesslog/src/filter.rs` + `envoy-config/src/bootstrap.rs` is empty — the only non-comment production delta in the phase is `matcher.rs` |
| PLAN correction (a): the filter selects **60** tests, not 59 | **TRUE** | my control run reports `60 passed`; `PLAN.md:104` and `:466` both say 59 |
| PLAN correction (b): `envoy_http1::codec::Request::test(..)` does not exist | **TRUE** | no `fn test(` in `crates/envoy-http1/src/codec.rs`; cited at `PLAN.md:981`. *Mitigating:* `PLAN.md:1016` already hedged it with an implementer note naming the fallback precedent |
| PLAN correction (c): `d601365` is pure formatting | **TRUE** | whitespace-stripped file contents are byte-identical before/after for all three source files (6670 / 19330 / 316516 chars each side). My first check appeared to show a field moving across a brace — that was an artifact of dropping diff context lines; the correct whole-file comparison is clean |
| Scope: `DECISIONS.md`, `ROADMAP.md`, `ci.yml`, the frozen parent `75/SPEC.md`, `known-failures.txt` all UNTOUCHED | **CONFIRMED** | `git diff --stat` per path is empty for each; `known-failures.txt` still **21** lines |
| Standing invariants hold | **CONFIRMED** | `#![forbid(unsafe_code)]` at every crate root; `envoy-accesslog` has ZERO workspace deps (only `tokio`/`bytes`/`tracing`/`thiserror`); `LogFilter` derives only `Debug, Clone` (no `Eq`/`PartialEq`) — the ADR-0150 seam is intact and 75.1 moved nothing across it; 83 fixture directories |
| Conflation traps NOT unified | **CONFIRMED** | `ValueMatcher::matches`/`matches_resolved` (`matcher.rs:178-192`) are byte-identical to base; `MetadataMatcher` (`bootstrap.rs`) still has no `invert` field |
| 13 tasks, one commit each, in plan order; no squashing, no mixed commits | **CONFIRMED** | `git log --oneline 2856976..HEAD` — tasks 1→13 in sequence, then the fmt pass, the state-3 advance, the state-4 gate |
| `next-prompt.txt` in zero commits (it is gitignored) | **CONFIRMED** | absent from all 16 diffstats |
| Task 1's RED is genuine, not a formality | **CONFIRMED** | the engine at `f68b160` is still the uniform `mode_result ^ self.invert_match`; the added matrix asserts `!mi.matches(&[])` for every value mode, which that body computes as `false ^ true` = `true` — it cannot pass |
| `d601365` touches zero doc-comment lines | **CONFIRMED** | the doc-comment corruption hazard (memory `mechanical-fanout-scripts-corrupt-doc-comments`) did not bite: no `///` line in its `crates/` diff |

---

## Findings

### Critical

**None.**

### Important (MUST-FIX)

**None.** Nothing below requires code, and none of it makes a reader take a wrong
action on the runtime rule.

### Minor

#### M-1 — [Minor] The `matcher.rs:52` citation the phase was chartered to *fix* went stale inside the same phase

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:2408`:

> The engine is `HeaderMatcher::matches` (the XOR is at `matcher.rs:52`), shared
> verbatim by five subsystems…

`SPEC.md` §6 item 3 made "the XOR is at `matcher.rs:52`, not `:51`" a named
deliverable, and task 13 landed exactly that. But task 4 (`810a177`, which landed
*earlier*) inserted a 17-line doc block above `pub fn matches`, so by the time the
corrected citation was written the XOR had already moved. **Verified: the XOR is at
`crates/envoy-config/src/matcher.rs:69`.** The sentence is present tense ("The engine
**is**"), so it reads as a live pointer.

*Contrast — deliberately NOT a finding:* the in-source citation at `matcher.rs:471`
is **past tense** ("Until phase 75.1 the shared engine (matcher.rs:52) applied…") and
is therefore correct as a historical reference. Adjudicated by line, per the standing
trap.

**Suggested fix:** re-point `:2408` to `matcher.rs:69`, or better, make it
line-number-free ("the XOR that closes `HeaderMatcher::matches`"). This citation class
has now gone stale three times (`:51` → `:52` → `:69`).

#### M-2 — [Minor, two sites] Correction C2's restatement is over-broad: the two `present_match` rules also AGREE in one ABSENT cell

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:1883-1885`, and mirrored verbatim in source at
`crates/envoy-config/src/bootstrap.rs:1706-1708`:

> The two now **AGREE when the key/header is PRESENT** and still **DIFFER when it is
> ABSENT** — `ValueMatcher` → `false`, `HeaderMatcher` → `true`.

Truth table, derived from the two implementations I read
(`ValueMatcher::matches_resolved` = `resolved.is_some() && *want`, `matcher.rs:190`;
`HeaderMatcher` = `v.is_some() == *want`, `matcher.rs:50`):

| | `want = true` | `want = false` |
|---|---|---|
| PRESENT | `true` / `true` — agree | `false` / `false` — agree |
| ABSENT | **`false` / `false` — AGREE** | `false` / `true` — differ |

Only **one of four** cells differs. The landed text asserts they differ across the
whole ABSENT column and gives an unqualified "`HeaderMatcher` → `true`", which is
false for `present_match: true` + absent.

**Severity Minor, not Important:** the load-bearing instruction — "the `ValueMatcher`
rule is CORRECT … do NOT unify them, and do not 'fix' the `ValueMatcher` rule to
match" — is intact and correct, and the `ValueMatcher` implementation was confirmed
untouched. A reader is led to the right *action* by imprecise *reasoning*.

**Suggested fix:** qualify to "differ in exactly one cell — ABSENT × `present_match:
false`, where `ValueMatcher` → `false` and `HeaderMatcher` → `true`."

#### M-3 — [Minor, two sites] Stale live claim that `HeaderMatcher.invert_match` still has a divergence

`docs/envoy-rust/BEHAVIOR_CONTRACT.md:2545` and
`tests/fixtures/0081-accesslog-metadata-filter/README.md:100`, identical sentence:

> Note this is a DIFFERENT field on a DIFFERENT message from
> `HeaderMatcher.invert_match` (CF-72-1), whose divergence is mode-scoped.

After 75.1 both D1 and D2 are closed, so `HeaderMatcher.invert_match` has **no
remaining divergence**; the trailing clause is now false in the present tense. The
surrounding CF-74-1 conflation warning (the *point* of the sentence) remains correct
and must be KEPT — only the trailing clause needs re-tensing (e.g. "…whose divergence
*was* mode-scoped and is CLOSED by phase 75.1").

Both files are live (non-append-only): `BEHAVIOR_CONTRACT.md` is the canonical
contract, and fixture READMEs are working documentation. This is a gap in task 13's
sweep, which correctly caught `0078`'s README but not `0081`'s.

#### M-4 — [Minor — PRE-EXISTING, out of 75.1's scope] Duplicate header values: upstream comma-joins, envoy-rust matches only the first occurrence

MEASURED this session with a decisive control (Probe group 3). `exact_match: "v,v"`
matching a request that sends `x-a: v` twice returns **MATCH** on upstream and
**NOMATCH** on envoy-rust; `prefix_match: "v,"` behaves the same way. Root cause is
`crates/envoy-config/src/matcher.rs:41-43`:

```rust
let value = headers
    .iter()
    .find(|(n, _)| n.eq_ignore_ascii_case(&self.name))
    .map(|(_, v)| v.as_str());
```

`.find()` returns the FIRST match; upstream Envoy coalesces duplicate values with `,`
before value matching. The *presence* axis is unaffected (j02/d02 both parity), which
is precisely why 75.1's absence rule is untouched by this.

**Why this is not a finding against this diff:** those three lines are unchanged
context in the diff, the divergence predates the phase, and it is a *multi-value*
concern rather than an *absence* concern — squarely outside `SPEC.md` §4.1. It
surfaced only because this review probed the function 75.1 rewrote. It affects all
five consumers and all six value modes, so it deserves a carry-forward with the
measurement banked rather than an ad-hoc fix here.

**Proposed: new carry-forward `CF-75-2`** (see *Carry-forward disposition*).

#### M-5 — [Minor, three instances] Some quoted "verbatim" evidence in `PROGRESS.md` was transcribed rather than freshly captured

`PROGRESS.md` is otherwise an exemplary log — I spot-checked a dozen of its census
figures and they all reproduce. But three blocks presented as verbatim command output
do not reproduce against the tree they claim to describe:

| # | site | quoted | actual | diagnosis |
|---|---|---|---|---|
| 1 | `PROGRESS.md:551` + `:563` (Task 6) | `212 filtered out` (⇒ total **213**) | `envoy-filter` lib had **212** tests at Task 6's commit `93acdcf`; the correct figure is `211 filtered out` | the numbers match the **Task-7** tree (213), so Task 6's RED/GREEN pair was captured on a tree that already carried Task 7's test |
| 2 | `PROGRESS.md:1503-1507` (state-4) | a `grep -n` block at lines `26:` / `31:` / `35:` | at `ee86574` those lines are `45:` / `50:` / `54:` | a uniform **19-line** offset — exactly the size of the doc block Task 4 inserted, so the quote predates Task 4 and was re-used at state-4 rather than re-run |
| 3 | Task 3 (mutation check) | ``grep -c 'Compiling envoy-config'` = **1**` for the mutated run only | — | the standing trap asks the run to **show** `Compiling`; a count is asserted instead, and the unmutated control has no rebuild evidence at all |

**Method note:** I validated the counting method before relying on it —
`cargo test -p envoy-filter --lib` on the current tree reports `214 passed` and my
test-attribute count over `crates/envoy-filter/src` is also `214`, an exact match.
Tasks 7 and 8 both reconcile correctly (213 and 214); only Task 6 is off, by one.

**Severity Minor, and the substance is corroborated, not doubted.** Every *semantic*
claim in these three blocks is TRUE and I verified each independently: the arm order
is correct (instance 2), and my own from-scratch mutation run reproduced the recorded
figures **exactly** — control `60 passed; 0 failed`, mutated `56 passed; 4 failed`,
`589 filtered out` in both, with `Compiling envoy-config` present in both runs
(instance 3). The Task-6 RED is likewise real: the RBAC test asserts the D1 and D2
cells, which the pre-75.1 uniform-XOR body fails by construction. So this is an
evidence-presentation defect, not a fabrication and not a correctness problem — but
`PROGRESS.md` claims every output is quoted verbatim, and in these three places it is
not.

### Nit

- **N-1 — `crates/envoy-config/src/matcher.rs:348-350`** says "Phase 75.1 flipped the
  **two** `false ×` expectations". Only ONE flipped:
  `present_match_false_requires_the_header_to_be_absent` went `true` → `false`, while
  `present_match_false_matches_when_absent` keeps its verdict — as the test's own body
  comment 20 lines below says explicitly ("Right answer, and after phase 75.1 for the
  right reason"). The block comment contradicts the test it introduces. Mirrored at
  `PROGRESS.md:381`.
- **N-2 — `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1887`** "See §C for the
  `HeaderMatcher` rule in full" is ambiguous: the file has **8** `**§C ` headings
  (verified by grep); the intended target is the phase-72 §C at `:2364`, and the
  nearest sibling (`:2536`) discusses a *different* `invert`. Should read "See the
  Phase 72 §C".
- **N-3 — commit-message precision.** `4717b3b`'s message cites the three P1 guards at
  their **pre-Task-2** line numbers (`:463`, `:425`, `:348`; post-Task-2 they are
  `:433`, `:475`, `:351`) — `PROGRESS.md:312` frames these correctly as "the three
  guards `PLAN.md` names", but the commit message drops that framing. Separately,
  `1bbc372`'s title says "all **7** REDs" while its body enumerates 5 + 4 = **9**:
  7 is the per-run count and 9 the union across the two runs. `PROGRESS.md:1397` gets
  this right ("All 9 distinct failures across both runs"); only the commit title
  conflates them.
- **N-4 — coverage symmetry.** The "empty value counts as PRESENT" cell is pinned
  in-process at only ONE of the five consumers (the access-log seam,
  `envoy-http1/src/hcm.rs`). Exhaustively covered by the engine matrix and
  cross-proxy by `0083` probes p11/p12, so this is redundant coverage rather than
  risk — no consumer inspects header values itself. Record only.

### Reported but DROPPED after re-verification (recorded so they are not re-raised)

- A reviewer flagged `crates/envoy-http1/src/hcm.rs:2019`
  (`find_header(HOST).filter(|h| !h.is_empty())?`) as making a `host`-targeted
  `present_match: false` route unmatchable. The code is real and I confirmed it, but
  the reviewer explicitly could not measure upstream, and upstream Envoy rejects
  HTTP/1.1 requests without a `Host` at the codec — so the divergence window is
  plausibly empty. **Unmeasured ⇒ not adopted as a finding.** Noted here only so a
  future phase touching `Host` handling can measure it properly.
- A "the commit says eight doc comments but edits seven hunks" bookkeeping claim: not
  adopted. Adjacent comment blocks merge into one hunk under `-U0`, so 8 blocks in 7
  hunks is consistent, and I could not establish an actual error.

---

## Strengths

- **The engine restructure is the right shape and is self-documenting.** The
  exhaustive tuple `match (&self.mode, value)` makes the mode-scoping structural
  rather than conventional: the `(_, None) => return false` arm *cannot* be reordered
  without the compiler still accepting it, so the risk is real — and the code answers
  that with a doc block that names the hazard, plus two guard tests and a matrix that
  all go RED on the exact mistake. Reviewed against the §2.1 rule cell by cell; every
  one of the 13 measured probe ids evaluates correctly.
- **No behavior smuggled into the refactor.** The six value arms are semantically
  identical to their pre-75.1 forms on the `Some` path (`value == Some(lit)` →
  `v == lit`, `value.and_then(parse).is_some_and(..)` → `v.parse().is_ok_and(..)`);
  only the `None` path changed. This is exactly the minimal diff the phase claimed.
- **The amended tests were RENAMED to describe parity, not silently flipped.**
  `pv4_value_matcher_absent_plus_invert_kept_diverges_from_upstream` →
  `…_dropped_is_parity_with_upstream`, with the old rule quoted as retired and the
  reason recorded. A future reader can reconstruct what changed and why.
- **The guard's asymmetry is documented at the point of danger**, in the engine doc
  block, in both guard tests, and in the contract's §C — each explicitly telling the
  next refactorer not to "simplify" the arm order.
- **Fixture `0083` is a genuine assertion, not a cross-proxy diff.** Each probe's
  expected body is asserted against the literal on BOTH sides, so a wrong expectation
  cannot pass by both proxies agreeing. Its `p07-absent-keeps-GUARD` probe is the one
  a uniform absent-DROP fails, and it is present and correct.
- **The 22 probes' expectations are all correct** against the SPEC §2.3 measured
  upstream column — independently re-derived from the §2.1 rule as a cross-check. No
  prefix shadowing (all eight prefixes are distinct 4-char strings), no unreachable or
  duplicated route, 16 unique response bodies, and no global catch-all (so a mistyped
  path 404s rather than passing vacuously).
- **The three PLAN defects were caught, measured and recorded** rather than silently
  worked around — including the honest admission that the plan's own literal Rust
  fails the plan's own `fmt` gate. All three verified TRUE by me.
- **Scope discipline was exact.** `DECISIONS.md` untouched through state-3 AND
  state-4, no ROADMAP flip, no `ci.yml` edit, no fuzz target or corpus seed, the frozen
  parent SPEC untouched, `known-failures.txt` never trimmed, and zero production edits
  at the five call sites.

---

## Carry-forward disposition (after this review)

**CONSUMED by 75.1:** **CF-72-1** — the shared-engine value-matcher `absent + invert`
divergence, CLOSED by the D1 half of the fix and pinned cross-proxy by `0083`
p01/p06/p09. D2 closes with it (it never had its own id). *This consumption becomes
effective at the state-6 close-out, not at this review.*

**NEW — `CF-75-2` (opened by this review, MEASURED):** duplicate header values are
comma-joined by upstream before value matching; envoy-rust matches only the first
occurrence (`crates/envoy-config/src/matcher.rs:41-43`). Affects all SIX value modes
and all FIVE consumers; the presence axis (`present_match`, and therefore 75.1's whole
rule) is unaffected. Measurement banked in *Probe group 3* above, including the
`exact_match: "v,v"` / `prefix_match: "v,"` control that identifies the mechanism.
Pre-existing, silent, and fixable without new config surface — a strong future
candidate.

**AMENDED — `CF-75-1` stays OPEN but its record is now incomplete.** The
`exact_match: ""` + `invert_match` + ABSENT cell, never measured at the pick, was
DIVERGENT before 75.1 and is PARITY after it (Probe group 2). CF-75-1's remaining
divergence is confined to the PRESENT-value cells, both polarities. **75.2 already
owns the CF-75-1 contract row** — it should bank this correction there rather than a
new artifact being minted here.

**Travels to 75.2, unchanged:** the `present_match`-polarity `BEHAVIOR_CONTRACT.md`
subsection, the CF-72-2 row updates, and **M74-31**.

**Untouched, carried forward:** CF-72-2, CF-74-1/2/3/4/6 (CF-74-5 CLOSED), CF-73-1,
N73-R2, M73-R1/M73-R2, M71-3, M71-6/7/8, M70-R4/R9, M69-A..I, CF-69-1/2/3/5, M68-1,
M-1, CF-67-3/5/6/7, M74-3..M74-14, M74-16, M74-17/18/20/21/22/26/27/28/29,
M74-30..M74-39, the older Minors, and the HTTP-filters-family (1)-(4).

**Plus this review's own findings.** Disposition differs by kind:

- **M-1, M-2, M-3, N-1, N-2** are live-document accuracy fixes, cheap and co-located.
  **Fold into 75.2**, which already rewrites the `present_match` contract surface and
  touches `0081`'s neighbourhood.
- **M-5 and N-3** need **no fix at all.** `PROGRESS.md` and the commit messages are
  landed historical artifacts of a completed state; editing them retroactively would
  be worse than the imprecision (D-3.5). They are recorded here so the *record* is
  accurate and so the next verification session tightens its evidence capture —
  specifically: re-run rather than re-use a `grep -n` block, and **show** the
  `Compiling` line rather than a count of it.
- **N-4** is a coverage note; record only.

**None is a precondition for the 75.1 close-out.**

### Why no ADR-0160

`DECISIONS.md` is UNTOUCHED by this review, and deliberately. No genuinely NEW
*decision* arose: CF-75-2 is a measured observation banked for a future phase (the
project records carry-forwards in `REVIEW.md`/`STATE.md`, not by ADR), the CF-75-1
amendment is a correction to a record 75.2 already owns, and leaving multi-value
handling out of 75.1 is not a new judgement — `SPEC.md` §4.1 already scoped this
sub-phase to the ABSENCE axis. Ledger head remains **ADR-0159**; next available
**ADR-0160**.

---

## Next state

**§5 state-6 CLOSE-OUT for sub-phase 75.1** — a **SEPARATE session** per §5.1 and
ADR-0127, and per memory `closeout-and-pick-are-separate-sessions` the *next-phase
pick is a separate session again*. That close-out:

1. flips ROADMAP row `75.1` → `done` (parent row `75` stays `in-progress`; it flips
   only at 75.2's close-out), preserving 6 cells and escaping `\|`;
2. relocates the superseded `STATE.md` blocks and the final `### Phase-75.1 …` Notes
   subsection per ADR-0035 (DELTA-based check, never "no duplicates");
3. advances `STATE.md` to "awaiting next planning" / sub-phase `75.2`;
4. records CF-72-1 as CONSUMED and CF-75-2 as OPENED.

**It must NOT** start 75.2, re-run the §7.5 gate, re-open the seven adjudicated
host-environmental REDs, or fix the Minors above (they travel to 75.2).
