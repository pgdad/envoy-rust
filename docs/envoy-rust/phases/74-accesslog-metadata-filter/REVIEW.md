# Phase 74 — access-log `metadata_filter` (the DYNAMIC-METADATA emission gate) — §5 state-5 CODE-REVIEW

> `superpowers:requesting-code-review`, run in its OWN session per §5.1 / ADR-0127
> (the context that wrote an artifact must not grade it). Reviews the phase-74
> implementation diff
> `git diff 53893b6730d1bf4d41611f2d1e36eb3ef8d870ad..a790d72d1e1e7f86eb6f6b5c5e75625055c6205e`
> (24 files, +2246/−141; code commits T1 `3bb4e9e` → T10 `3455815`, plus the fmt
> pass `50078d7` and the PROGRESS/STATE commit `a790d72`). Base = the state-2
> PLAN-write commit `53893b67…` (last pre-implementation commit). Session HEAD =
> the state-4 verification commit `7f481feeef5f94b2fd12edbdfc5acfd86353090a`,
> CI `completed`/`success` on the FULL 40-char SHA (run `30048991125`, both jobs
> green with full step counts 15 and 13).
>
> **The §7.5 gate was NOT re-run** — it was RUN and ADJUDICATED GREEN (a)–(e) at
> state-4 and its evidence is quoted in `PROGRESS.md` `## §7.5 gate (state-4
> verification)`. Gate (f) IS this review.
>
> **Method** (memory `state5-must-probe-untested-compositions`): five FRESH
> zero-context read-only reviewers fanned out across independent dimensions
> (runtime engine, config model + validator, fixtures + fuzz, test quality,
> plan/doc alignment), each forbidden to write or to run `cargo`. The MAIN session
> then did the decisive **MUTATION** and **LIVE-PROBE** measurements ITSELF —
> mutations in a scratch `git worktree` (memory
> `mutation-checks-collide-with-parallel-subagents`), live probes against BOTH
> proxies. **A green gate proves the code does what its tests ASK, not that the
> tests ask the right question.** This phase adds a SIXTH arm over the phase-73
> `And`/`Or` recursion, so N−1 combinations ship untested; I grepped what the two
> fixtures actually exercise (`string_match` leaves, H1 only, key always resolving
> in `0081`) and LIVE-PROBED the rest cross-proxy.

---

## Verdict: **APPROVED WITH MUST-FIX — 4 Important. Next = §5.2 state-3 RE-ENTRY (step 3, NOT step 4).**

> **THIS VERDICT IS SUPERSEDED (reasoning preserved verbatim per D-3.5 — do NOT
> re-litigate it).** All four Important findings below were consumed by the §5.2
> state-3 re-entry (commit `cab381d2`) and CONFIRMED CLOSED by the **§5 state-5
> RE-REVIEW appended at the end of this file**, which is the current verdict.
> Read that section for the live disposition.

**No Critical. No behavioral defect.** The evaluation engine, the config model, the
validator recursion, the compile lowering and the `Option<bool>` seam are correct
on every path I traced, and **every one of the 12 untested compositions I
live-probed is BYTE-IDENTICAL cross-proxy against `envoyproxy/envoy:v1.33.0`** —
including the two the reviewers flagged as the riskiest (the H2 emit gate, and a
`SafeRegex` metadata value buried two levels deep inside a composition, whose
failure mode is a request-time panic). The four load-bearing invariants (ADR-0150
seam, `LogFilter` derives, the `unreachable!` lockstep guard, `#![forbid(unsafe_code)]`)
all HOLD, and three mutation checks confirm the phase's key pins are non-vacuous.

The four MUST-FIX items are **one contract-accuracy defect and three coverage
gaps**, not divergences:

- **I-1 is the only one that makes a document say something untrue.** The wrapped
  `BoolValue` spelling `match_if_key_not_found: { value: false }` is BOOT-FATAL in
  envoy-rust while upstream ACCEPTS and HONORS it — I measured both halves with a
  control. `BEHAVIOR_CONTRACT.md` §A currently presents the wrapper acceptance as a
  *shared* property and §D's "where envoy-rust is STRICTER" list omits it, so a
  reader would conclude the wrapped form works. Per §4 invariant 5 / D-3.3 the
  contract must be corrected (or the implementation changed) — never left silently
  wrong. This is a **fail-loud** gap in the REJECT direction, so it is never a
  silent runtime difference; the `Option<bool>` model itself is CORRECT and must
  not be "fixed".
- **I-2/I-3/I-4 are coverage.** I measured all three at parity, so they are pins
  the suite is missing, not bugs it is hiding. I-2 carries one genuine
  **undocumented deviation** (I-2b): T3's mechanical fan-out script rewrote two
  `envoy-http2` DOC COMMENTS so they now describe the production emit gate backwards,
  and `PROGRESS.md` counts those two prose lines among its "80 call sites patched"
  (the true split is 78 call sites + 2 doc-comment edits). It is the only undocumented
  deviation found across all ten tasks.

ROADMAP row `74` stays `in-progress` — no flip until the state-6 close-out.

---

## LIVE-PROBE evidence (MEASURED this session — envoy-rust DEBUG `envoy-bin` vs. `envoyproxy/envoy:v1.33.0`)

Method per memories `state0-recon-docker-needs-port-mapping` (port-mapped `docker -p`,
never `--network host`) and `differential-per-side-counts-mid-settle-are-flush-artifacts`
(graceful `docker stop -t 15` SIGTERM to flush real Envoy's buffered access-log
writes; envoy-rust stopped with SIGTERM). Each probe used ONE config per side
differing **only** in the four documented per-side divergences (`admin`,
listener bind, `generate_request_id`, mount path) — verified by `diff` before every
run. All probe containers were removed afterwards.

### Probe group 1 — seven sinks, one H1 config pair, seven requests (the untested compositions + CF-74-5 + the load-parity trap)

Requests: `r1 x-a:1` · `r2 x-a:2` · `r3` (none) · `r4 x-b:1` · `r5 x-a:1,x-b:1` ·
`r6 x-a:2,x-b:1` · `r7 x-c:1`. Format
`P=%REQ(:PATH)% M=%DYNAMIC_METADATA(com.example:k)%`.

| sink | filter | kept — real Envoy | kept — envoy-rust | verdict |
|---|---|---|---|---|
| S1 | `or_filter[ metadata(exact "1", mifknf=false), header_filter(x-b=1) ]` | r1 r4 r5 r6 | r1 r4 r5 r6 | **PARITY** |
| S2 | `and_filter[ metadata(exact "1", mifknf=false), header_filter(x-b=1) ]` | r5 | r5 | **PARITY** |
| S3 | **DEPTH-2** `or_filter[ and_filter[ metadata, header(x-b) ], header(x-c) ]` | r5 r7 | r5 r7 | **PARITY** |
| S4 | `metadata(present_match: **true**, mifknf=false)` | r1 r2 r5 r6 | r1 r2 r5 r6 | **PARITY** |
| S5 | `metadata(present_match: **false**, mifknf=true)` | r3 r4 r7 | r3 r4 r7 | **PARITY** |
| S6 | `metadata_filter: {}` (matcher-less, default policy) | all 7 | all 7 | **PARITY** |
| S7 | `metadata_filter: { match_if_key_not_found: false }` (matcher-less) | *(0 lines)* | *(0 lines)* | **PARITY** |

All seven files byte-identical; `md5sum` of the per-side concatenation matched
(`380b58e471f8c0c545d02a5e8b7b9df3` both sides).

What this closes:

- **`Metadata` nested under `and_filter`/`or_filter` and at DEPTH 2** (S1/S2/S3) —
  previously pinned **in-process only** (T4's `metadata_arm_composes_under_and_or`
  uses a local stub matcher, not the real engine). Now measured cross-proxy over
  the REAL `MetadataMatcher`. Note S1/S2 also exercise `Metadata` as a **sibling of
  `header_filter`**, and S3 as a sibling of a nested composition — three of the
  "none"-covered rows the test reviewer's matrix flagged.
- **CF-74-5 — `present_match` on the RESOLVED branch — is now MEASURED, not
  derived.** S4 and S5 are exact complements: with the key RESOLVED,
  `present_match: true` KEEPS and `present_match: false` DROPS; with the key ABSENT
  both defer to `match_if_key_not_found`. Both proxies agree on every cell. This is
  precisely what `BEHAVIOR_CONTRACT.md` §G labels "derived, not separately
  measured" — **§G can now be upgraded to MEASURED and CF-74-5 CLOSED** (see
  Recommendations).
- **The matcher-less load-parity trap (R-0.2)** is confirmed in BOTH polarities
  (S6 keeps everything, S7 drops everything) — the config reviewer's Minor-3 claim
  that the matcher-less + explicit-`false` combination is "unmeasured against
  upstream and untested on either side" is **answered: it is at parity.**

### Probe group 2 — the HTTP/2 emit gate (`codec_type: HTTP2`, prior-knowledge h2)

Requests `q1 x-a:1` · `q2 x-a:2` · `q3` (none); all served over HTTP/2 (`curl`
reported `http_version=2` for every request on both sides).

| sink | filter | kept — real Envoy | kept — envoy-rust | verdict |
|---|---|---|---|---|
| `h2_meta` | `metadata(exact "1", **mifknf=false**)` | q1 | q1 | **PARITY** |
| `h2_meta_default` | `metadata(exact "1", **mifknf ABSENT**)` | q1 q3 | q1 q3 | **PARITY** |

**The H2 gate is CORRECT** — `&record.dynamic_metadata` genuinely reaches
`should_log` on the H2 codec, and the `unwrap_or(true)` wrapper default resolves
correctly there too (`q3`, key-absent, is KEPT under the absent field and DROPPED
under explicit `false`). Two reviewers independently flagged the H2 gate as the
single production line with zero coverage; **this measurement establishes it is a
coverage gap, not a live bug** (I-2).

### Probe group 3 — the wrapped `google.protobuf.BoolValue` spelling (**the one DIVERGENCE found**)

| probe | real Envoy v1.33.0 | envoy-rust |
|---|---|---|
| `match_if_key_not_found: { value: false }`, `--mode validate` | `configuration OK` | — |
| same, RUNTIME (3 requests) | boots; logs `/w1` only → **key-absent `/w3` DROPPED**, so the wrapper was parsed as `false`, not defaulted to `true` | **BOOT-FATAL**, exit 1: `parsing bootstrap YAML: … invalid type: map, expected a boolean` |
| **CONTROL** `{ bogus: false }` | **REJECTED**, error names `message google.protobuf.BoolValue … no such field: 'bogus'` | — |
| **CONTROL** bare `false` | `configuration OK` | accepted (fixture `0082`) |

The `{ bogus: false }` control is what makes this decisive: it proves upstream's
parser genuinely interprets the map **as a `BoolValue` message** rather than
ignoring it, and the runtime probe proves the value is **honored**, not merely
accepted. See I-1.

### Probe group 4 — a `SafeRegex` metadata value at DEPTH 2 (a request-time panic path)

`and_filter[ header(x-b=1), or_filter[ header(x-c=1), metadata(safe_regex "^[0-9]+$", mifknf=false) ] ]`

| request | expected | real Envoy | envoy-rust |
|---|---|---|---|
| `x1` `x-b:1 x-a:42` (regex matches) | KEEP | `P=/x1 M=42` | `P=/x1 M=42` |
| `x2` `x-b:1 x-a:abc` (regex fails) | DROP | — | — |
| `x3` `x-b:1` (key absent → mifknf false) | DROP | — | — |
| `x4` `x-b:1 x-c:1` (OR short-circuits) | KEEP | `P=/x4 M=-` | `P=/x4 M=-` |

**PARITY, and envoy-rust did not panic** (`grep -ci panic` over its stderr → `0`).
This exercises `StringMatcher::matches`' `.expect("validator ensured … SafeRegex
compiled")` on the metadata route for the first time anywhere, and proves the
validator's **in-place regex compile recurses to depth 2** — the failure mode the
config and test reviewers both flagged as the least-covered panic-class path
(I-4). The config reviewer's structural claim that `validate_access_log_filter` is
a single recursive function whose `metadata_filter` block runs at every level is
**confirmed empirically**.

---

## MUTATION checks (MEASURED this session, in a scratch `git worktree`)

Run in `git worktree add --detach` per memory `mutation-checks-collide-with-parallel-subagents`
(a parallel reviewer's `git checkout --` would silently revert an in-place mutation
and produce a false green that a `Compiling` grep does NOT catch). The worktree was
`git checkout --`-restored between mutations, verified `git status --porcelain`
empty, and removed at the end; the MAIN tree stayed clean (`0` porcelain lines,
HEAD `7f481fee`) throughout.

| # | mutation | result | conclusion |
|---|---|---|---|
| **M1** | delete `metadata_filter.is_some()` from the hand-maintained `set_arms` array (`bootstrap.rs:5299-5305`) — the exact silent failure PV-3 warns about (an arm in the struct but missing from the array counts as ZERO → a valid filter becomes `AmbiguousAccessLogFilter`) | `six_arm_cardinality_counts_every_arm ... **FAILED**` (+7 further tests RED) | the cardinality pin is **NON-VACUOUS**; the compiler-unchecked array is genuinely guarded |
| **M2** | `unwrap_or(true)` → `unwrap_or(false)` in `compile_access_log_filter` (`envoy-http1/src/hcm.rs:1806`) | `compile_access_log_filter_builds_metadata_arm_with_wrapper_default ... **FAILED**` | the in-process pin catches it — **but see I-3: NO differential fixture does** |
| **M3** | make the validator REJECT a matcher-less `metadata_filter` | `matcher_less_metadata_filter_is_accepted ... **FAILED**` | the load-parity pin that "must NEVER go red" is **NON-VACUOUS** despite having passed from its first run |

**M2's negative half, verified separately on the real fixtures:** `0081`'s only
textual `match_if_key_not_found` occurrence is a **comment** (`envoy.yaml:26`) — the
field is genuinely absent — and BOTH its probes send `x-a`, so the key always
resolves and the `unwrap_or` default is never consulted; `0082` pins the field to
explicit `false`, so `unwrap_or` never yields its default either. **Neither
committed differential fixture reads the wrapper default.** That confirms the
fixture reviewer's Important-1 analytically as well as by mutation.

---

## Handoff-mandated verification of the two state-3 deviations, and the invariants

| check | command | result |
|---|---|---|
| T3's transient `#[allow(clippy::only_used_in_recursion)]` genuinely removed | `grep -rn only_used_in_recursion crates/` | **0 hits** — added in T3 `796450d`, removed in T4 `cd5a675` (2 `+` / 2 `−` lines). Genuinely transient, exactly as `PROGRESS.md` claims |
| T6's `bool` vs `Option<bool>` slip resolved | `grep -n "match_if_key_not_found: Some(false)"` | present at `envoy-http1/src/hcm.rs:4941` and `:4979` |
| the `unreachable!` lockstep guard NOT weakened | `git diff …\|grep -E "^[+-].*unreachable!"` | **neither added nor removed** — byte-identical to its pre-phase form; exactly 1 occurrence in the file |
| no pre-existing test deleted or weakened | `git diff …\|grep -c "^-.*#\[test\]"` | **0** |
| `known-failures.txt` / conformance untouched | `git diff --stat … -- tests/conformance/` | **empty** |
| landed ADRs not edited; no ADR-0156 | `git diff --stat … -- DECISIONS.md`; `grep -c "^## ADR-0156"` | **empty**; **0** — ledger head stays ADR-0155, the §6.1 split reservation UNFIRED |
| ADR-0150 seam | `envoy-accesslog/Cargo.toml` `[dependencies]` | `tokio`, `bytes`, `tracing`, `thiserror` — **ZERO workspace crates** |
| `LogFilter` derives | `filter.rs:67` | `#[derive(Debug, Clone)]` — no `Eq`, no `PartialEq` |
| D-3.8 | `grep -l "forbid(unsafe_code)"` over every `members` entry | **22 of 22** workspace member roots (PROGRESS/STATE say "14 of 14", which is correct for `crates/` but UNDERSTATES the real coverage — the workspace has 22 members incl. `tests/differential`, `tests/conformance/h2spec` and the six helpers) |
| N73-R1 consumed | `grep -c "THREE oneof arms"` | **0** — the stale doc really was fixed |
| ROADMAP row `74` | `awk -F'\|'` | **6 cells**, status `in-progress` (correctly NOT flipped) |
| `STATE.md` counts | direct count | **82** fixture dirs ✅, **36** fixtures carrying an `access_log` stanza ✅, **63** tracked corpus seeds ✅. The "34 by directory name" alternate figure also reconciles (33 `*accesslog*` + `0012-access-log-file-sink`) — **no defect** |

---

## Findings

### Critical

**NONE.** Across five review dimensions, 12 live cross-proxy probes and 3 mutation
checks, no input was found that produces a wrong verdict, a panic, or a broken
load-bearing invariant.

### Important (MUST-FIX — these define the §5.2 state-3 re-entry)

**I-1 — The wrapped `BoolValue` spelling is boot-fatal here but accepted AND honored upstream, and `BEHAVIOR_CONTRACT.md` currently implies otherwise.**
`crates/envoy-config/src/bootstrap.rs:790` models `match_if_key_not_found` as
`Option<bool>`, which cannot deserialize a YAML mapping. MEASURED (probe group 3):
`match_if_key_not_found: { value: false }` → upstream `configuration OK`, boots, and
DROPS the key-absent record; envoy-rust exits 1 with `invalid type: map, expected a
boolean`. A `{ bogus: false }` control is rejected upstream naming
`google.protobuf.BoolValue`, proving the field is genuinely wrapper-typed.
**Why it matters:** `BEHAVIOR_CONTRACT.md` §A states the wrapper fact as a shared
property (`"{ value: true } is accepted alongside a bare true … modelled as
Option<bool>"`), and §D's three-item "where envoy-rust is STRICTER" list
(`invert`, multi-segment `path`, unmodelled `ValueMatcher` arms) omits it. A reader
would conclude the wrapped form works. Project invariant §4.5 / D-3.3 requires the
contract be corrected rather than left silently wrong.
**Fix (documentation + one pin — NOT a code change):** add the wrapper-spelling
rejection to §D; open it as a carry-forward (**CF-74-6**, owner = a future
wrapper-parity phase); add
`assert!(serde_yaml::from_str::<MetadataFilter>("match_if_key_not_found: {value: true}").is_err())`
to `metadata_filter_deserialize_round_trip_and_defaults`. **Do NOT change
`Option<bool>` to a custom deserializer in this re-entry** — the model correctly
preserves absent-vs-explicit-`false`, and the house precedent is bare-only
(`UInt32Value`, ADR-0063, pinned by
`cidr_range_rejects_unknown_field_and_wrapper_prefix_len`).

**I-2 — The H2 emit gate's metadata threading has ZERO coverage, and two doc comments now describe it backwards.**
`crates/envoy-http2/src/hcm.rs:1142` passes `&record.dynamic_metadata`. Mutating
that one argument to `&Default::default()` fails **no test in the workspace**: no H2
fixture carries any access-log filter (`0076`–`0082` are all H1) and the H2
in-process filter tests build only `StatusCode`/`ResponseFlag`/`Header` arms, none of
which read the 4th argument. **I measured the gate at full parity (probe group 2),
so this is an undefended line, not a broken one** — but the consequence of a future
regression is silent and severe (every H2 `metadata_filter` would see an empty
store, logging everything or nothing).

**I-2b — and the aggravating half is an UNDOCUMENTED DEVIATION.** T3's balanced-paren
fan-out script appended the 4th argument to **prose it should have skipped**.
VERIFIED on disk: the only two lines `796450d` touched in `envoy-http2/src/hcm.rs`
are both `///` doc comments —

```
+    /// &envoy_req.headers, &Default::default())` gate (phase 72 added the header slice) end-to-end;
+    /// `should_log(status, flags, headers, &Default::default())` gate (hcm.rs ~1138); this test
```

— and `grep "should_log(" | grep -v "///"` over that file returns **exactly 1** line,
the real gate, which passes `&record.dynamic_metadata`. So the two doc comments now
assert the production gate feeds an EMPTY metadata store: precisely backwards, and
precisely the misreading that would motivate a wrong "fix" to the gate (D-3.4). It is
undocumented because `PROGRESS.md:143-147` counts them as code: *"**80 call sites
patched** — filter.rs 36, file_sink.rs 4, envoy-http1/hcm.rs 38, **envoy-http2/hcm.rs
2**"*. ADR-0155's own PV-5 census already established that `envoy-http2` has 4
`should_log` hits of which only **1 is a call and 3 are doc comments**, so the true
figure is **78 call sites + 2 doc-comment edits**. `cargo fmt` does not reflow doc
comments, so `:3559` also now overruns the file's wrap width — which is why the fmt
pass did not surface it.
**Fix:** add the H2 sibling of `compile_access_log_filter_builds_metadata_arm_with_wrapper_default`,
modelled on the phase-72 precedent `h2_header_filter_…` in the same file (phases
70/71/72 each added exactly such a test); restore `&record.dynamic_metadata` in both
doc comments and re-wrap `:3559`; and correct `PROGRESS.md:143-147` to distinguish the
78 call sites from the 2 doc-comment edits.

**I-3 — No committed fixture exercises the `match_if_key_not_found` default-`true` branch — the phase's headline claim.**
Confirmed by mutation M2 plus direct inspection: `0081` omits the field but every
probe sets `x-a` (key always resolves), and `0082` pins it to explicit `false`. So
the `None → true` KEEP branch — the observable the phase repeatedly stresses
`--mode validate` provably cannot reach — is pinned **only by envoy-rust asserting
against itself**. I measured it cross-proxy (probe groups 1-S6 and 2), and
`PROGRESS.md:271-283` records the state-3 session measuring it live too, but neither
measurement became a regression fixture.
**Fix (cheap, no new fixture):** add a THIRD probe to
`tests/fixtures/0081-accesslog-metadata-filter/expectations.yaml` — `GET /x` with
**no** `x-a`, `expected_status: 200`, `expect_logged: true` — placed SECOND so the
kept-LAST convention (ADR-0147) and the cheap 2 s `CF70_3_SETTLE` are preserved.
`expected_logged_count` becomes 2 and the two kept lines are byte-distinct
(`…M=-` then `…M=1`), which pins ordering too. Update `0081/README.md` accordingly
(it currently states every probe resolves the key).

**I-4 — A `SafeRegex` metadata value is compiled but never EVALUATED by any test.**
`crates/envoy-config/src/matcher.rs:139-143` carries
`.expect("validator ensured StringMatcher SafeRegex compiled")` — a request-time
panic on an uncompiled regex. `bootstrap.rs:13663` proves the top-level validator
sets `compiled`, but nothing ever runs `MetadataMatcher::matches` with a
`SafeRegex` value, so the panic path was unexercised on the metadata route.
Relatedly `reuses_the_value_matcher_engine_verbatim` (`matcher.rs:577`) covers
Exact/Prefix/Suffix/Contains/ignore_case/present but **skips SafeRegex**, while its
own comment claims "Every modelled StringMatcher mode routes through
ValueMatcher::matches" — false as written.
**I live-probed it clean (probe group 4), including at depth-2 nesting**, so there
is no bug — but the pin is missing.
**Fix:** add a `SafeRegex` case to `matcher.rs:577` and correct that comment.

### Minor (record; fold opportunistically, not required for the re-entry)

- **M74-1** — stale arm-count doc comments, the same defect class N73-R1 was opened
  for: `bootstrap.rs:728` ("Mutually exclusive with `status_code_filter`"), `:731-732`
  ("the other **two** arms"), `:737` and `:741` ("the other **four** arms"), `:798-799`
  (the `HeaderFilter` type doc), and the test comment at `:13272` ("all **FIVE**
  arms"). T1 correctly future-proofed the type-level doc and the new field's doc
  ("the other arms"); apply that phrasing to these six so arm #7 need not touch them.
- **M74-2** — `bootstrap.rs:773` overclaims: "A matcher-less filter keeps every
  record." True only when `match_if_key_not_found` is absent or `true`;
  `metadata_filter: { match_if_key_not_found: false }` with no matcher drops every
  record (MEASURED at parity, probe S7). The adjacent `hcm.rs` comment gets it right
  ("every record takes the not-found policy"); mirror that wording.
- **M74-3** — weak `detail` discriminators: `bootstrap.rs:13616` and `:13630` both
  assert `detail.contains("segment")`, so neither distinguishes empty-path from
  multi-segment. `contains("got 0")` / `contains("got 2")` is free and discriminating.
  (I verified every asserted substring does appear, so nothing is false-green today.)
- **M74-4** — `envoy-http1/src/hcm.rs:4967-4995` case (d): both `or_filter` children
  are matcher-less with policies `false` and `true`, so the OR is unconditionally
  true regardless of the store and of whether child 1 compiled correctly. Give child
  2 a real matcher and assert both a keep and a drop.
- **M74-5** — the validator-recursion pin (`bootstrap.rs:13717`) descends only
  through `or_filter`; the `and_filter` branch (a copy-paste sibling) and
  nested-`SafeRegex` compile-in-place have no in-process pin. Both measured at parity
  by probe groups 1 and 4; one extra `and_filter` variant would close it.
- **M74-6** — `crates/envoy-accesslog/src/filter.rs:1-4` module header still stops at
  phase 72; phases 73 and 74 are absent though every other doc in the file was updated.
- **M74-7** — the metadata store type is spelled out inline in five places
  (`filter.rs:57`, `:114`, `file_sink.rs:108-111`, `matcher.rs:96-99`, `record.rs:111`).
  A `pub type DynamicMetadata = BTreeMap<String, BTreeMap<String, String>>` exported
  from `envoy-accesslog` would make CF-74-2's eventual re-typing a one-line change.
  (Considered and rejected at PV-4 for churn reasons — recorded, not re-litigated.)
- **M74-8** — `0081/README.md:62-63` claims the `-` rendering for an absent
  namespace/key is witnessed "on both proxies", but neither fixture exercises it
  (0081's probes all resolve; 0082's absent-key record is dropped before rendering).
  It is real (fixture `0042` measures it) — attribute it, or adopt I-3's fix, which
  makes `0081` witness it directly.
- **M74-9** — the fuzz seed
  (`crates/envoy-config/fuzz/corpus/parse_bootstrap/metadata_filter.yaml`) seeds only
  the explicit-`false` shape; the matcher-less form that `bootstrap.rs:5365` explicitly
  documents as accepted-and-skipped is not seeded. libFuzzer reaches it by line
  deletion, so this is genuinely nice-to-have.
- **M74-10** — `/config_dump` gains a sixth `"metadata_filter": null`
  (`AccessLogFilter` derives `Serialize` with no `skip_serializing_if`); upstream omits
  unset oneof arms. Pre-existing family-wide pattern widened by one — this is
  **M70-R4's** surface, already carried forward.
- **M74-11** — `AccessLogMetadataMatcherInvalid` carries no listener locator, unlike
  the RBAC analogue. Consistent with its access-log siblings
  (`AmbiguousAccessLogFilter`, `UnknownResponseFlag`) — a pre-existing family-wide
  diagnostic gap, not a phase-74 regression.
- **M74-12** — the two metadata-matcher validators now duplicate their first two
  checks near-verbatim (`bootstrap.rs:5388-5396` vs `:4879-4887`). Separating the
  *error variants* is justified; the *shape check* need not be duplicated. This is
  exactly the drift CF-74-4 already describes — fold when CF-74-4 is closed.
- **M74-13** — two quoted RED multiplicities in `PROGRESS.md` do not reconcile with the
  tests that produced them. T3 (`:136`) quotes `error[E0061] … 4 arguments were supplied`
  **×6**, but the pin test (`filter.rs:327-346`) contains **eleven** four-argument
  `should_log` calls; T5 (`:189-190`) quotes `error[E0599] … no method named 'matches'`
  **×6**, but `mod metadata_match_tests` has **ten** `.matches(...)` sites. The plan's own
  commands pipe through `tail -20`/`tail -30`, so the most likely cause is a truncated
  transcription — and the RED unquestionably occurred (the widening cannot compile against
  3-arg call sites), with T1's and T2's figures reconciling exactly. Fix: quote the
  `error: could not compile … due to N previous errors` summary for every RED, as T1/T2 do.
  This is also a live instance of memory `never-pipe-verification-runs-through-tail`
  reaching the per-task logs, not just the gate.
- **M74-14** — T1's RED (`PROGRESS.md:57-58`) quotes the SAME message under two different
  codes: `error[E0425]: cannot find type 'MetadataFilter'` ×3 and `error[E0433]: cannot
  find type …` ×3. rustc emits `E0412` for a missing type in annotation position and
  `E0433` for an unresolved path; `E0425` is "cannot find **value**". One code cannot be
  what rustc printed. The 3+3+3=9 breakdown and the quoted "due to 9 previous errors"
  summary are internally consistent, so this is cosmetic.
- **M74-15** — three counting imprecisions carried into both `PROGRESS.md` and `STATE.md`:
  (a) "`cargo test --workspace` built and ran **159 test binaries**" — 159 is the number of
  `test result:` lines, which includes doc-test lines, so it over-counts binaries;
  (b) clippy "re-analysed **all 15 workspace crates**" — `[workspace] members` lists **22**,
  and 15 is the count re-checked after touching four, so "all 15" reads as if 15 were the
  workspace size; (c) "`#![forbid(unsafe_code)]` in **14 of 14** workspace crate roots" —
  right for `crates/`, but the workspace has 22 members and I verified the attribute is
  present at **all 22**, so it understates rather than overclaims. Each is a one-word fix
  and none changes a verdict.
- **M74-16** — `PLAN.md:2100`'s Self-Review "Type consistency … consistent across T1→T6"
  over-claims: `PLAN.md:1400` is the one config-side literal that wrote
  `match_if_key_not_found: false` against an `Option<bool>` field (the T6 slip). Every
  other occurrence in the plan (lines 465, 962, 966, 999, 1003, 1017, 1341, 1362) is
  correctly typed. A defect in the PLAN, not the implementation — recorded so a future
  PLAN-write treats "type consistency" as a claim to be checked by compiling the literals,
  not by reading them (memory `plan-md-example-code-trips-clippy` is the same family).

---

## Strengths

- **The tri-state `Option<bool>` seam is the right design and is tested at the right
  altitude.** `matcher.rs`'s `resolution_contract_is_option_bool` asserts `Some(false)`
  for a resolved mismatch and `None` for missing-key / missing-namespace / empty-store
  as *distinct* values — real equality on the tri-state, not `matches!`. That kills the
  most attractive wrong implementation (reusing `ValueMatcher::matches_resolved`,
  which collapses unresolved into `false` and would drop every key-absent record). The
  rejection is documented in-code with its measured justification.
- **`filter.rs:155-163` implements the measured rule in the only correct order.**
  `Some(false)` reaches `unwrap_or` as `false` and is not replaced by the policy; only
  `None` is. That is the one place the two could have been conflated, and it isn't.
- **The ADR-0150 cycle seam held exactly.** `envoy-accesslog` still has ZERO workspace
  deps; resolution lives entirely on the `envoy-config` side; only the verdict crosses;
  `LogFilter` still derives only `Debug, Clone`. The `MetadataMatch` trait is a faithful
  application of the phase-72 `HeaderMatch` precedent.
- **The matcher impl is total.** `path.first()?` + two chained `?` on `BTreeMap::get` —
  no indexing, no `unwrap`/`expect`, no arithmetic. Missing namespace and missing key
  are *structurally* the same `None`, which is exactly the measured rule.
- **The 6-tuple match is correct.** I checked each arm's `Some` position against its
  body — no transposition — and the `unreachable!` lockstep guard is byte-identical to
  its pre-phase form, still sound because the validator destructures all six fields with
  no `..` and recurses in lockstep with the compile step.
- **PV-3's insight was real and the guard it demanded works.** The `set_arms` array is
  compiler-unchecked; the phase both identified that and pinned it with a test that
  validates each arm ALONE — so both an omission and a transposition fail. M1 confirms.
- **`0082` was mutation-verified during implementation** (`PROGRESS.md:265`), including
  a restore-and-re-diff. That is discipline this review usually has to ask for.
- **The `0082` `on_header_missing` omission is a caught near-vacuity.** Cloning fixture
  `0042`'s producer block would have made the fixture pass while testing the *value* path
  instead of the *not-found* path, silently vacating its entire witness. Identifying,
  documenting and verifying that trap (PV-6, README, `grep`) is high-quality work.
- **The fixtures render the gating value.** Because `%DYNAMIC_METADATA(ns:key)%` is not
  `REQ_ALLOW_LIST`-gated, the kept line is `…M=1` while a wrongly-kept line would read
  `…M=2` / `…M=-`. In `0079`/`0080` the two probes' lines are indistinguishable, so a
  "logged the wrong probe" bug survives the count assert; here it does not. A real
  strengthening over the phase-73 precedent.
- **Per-side fixture configs are exactly the four permitted divergences** — confirmed by
  full `diff` on both fixtures, with the `metadata_filter` and `header_to_metadata`
  stanzas byte-identical across sides. The two proxies are provably asked the same question.
- **The RBAC path is untouched** — `validate_metadata_matcher` byte-identical,
  `envoy-filter/src/rbac.rs` absent from the diffstat, `RbacMetadataMatcherInvalid`
  unchanged. The decision to add a separate access-log-scoped validator (rather than
  refactor the RBAC one across six coupled tests) is sound and correctly justified in-code.
- **The mechanical widening weakened nothing.** All ~86 inserted `&Default::default()`
  sites are in tests whose filter arms provably ignore the 4th argument; both production
  call sites pass the real store. `existing_arms_ignore_the_dynamic_metadata_argument`
  is the right regression pin and feeds a *populated* store to every pre-74 arm.
- **`PROGRESS.md` is honest about its two deviations**, and both check out on disk —
  including `git log -S"only_used_in_recursion"` showing the allow introduced at T3
  `796450d` and removed at T4 `cd5a675`, in the very commit that adds the consuming arm.
- **T1–T10 track `PLAN.md`'s literal code essentially verbatim.** A commit-by-commit
  diff against the plan text found the struct + doc, the `ConfigError` variant and
  let-chain validator, the `should_log` widening and both emit gates, the trait/variant/arm,
  the `matcher.rs` impl, the 6-tuple compile, both fixture config pairs, the fuzz seed and
  the contract subsection all byte-for-byte the plan's blocks modulo `cargo fmt` re-wrapping.
  The ONLY undocumented deviation found across the whole range is I-2b.
- **The state-4 arithmetic is coherent and the RED adjudication holds.** `2089 + 5 = 2094`
  == CI's `2094 passed`, 159 `test result:` lines on both sides; all five REDs are genuinely
  differential fixture tests and none sets an `AccessLogFilter` of any arm.
- **The T8 mutation check measured a suspicion instead of explaining it away.** When the
  mutation's per-side counts read `envoy=1` — superficially contradicting SPEC R-0.4 — the
  state-3 session did not rationalise it; it ran two standalone port-mapped upstream probes
  with graceful-stop flush and quoted the resulting table, independently re-deriving the
  wrapper default and correctly attributing `envoy=1` to a flush-timing artifact
  (memory `differential-per-side-counts-mid-settle-are-flush-artifacts`). My probe group 1
  S6 reproduces that conclusion a third time.
- **Docs are purely additive and structurally sound.** `--numstat` shows BEHAVIOR_CONTRACT
  `94/0`, STATE_HISTORY `69/0`, PROGRESS `600/0`; the T10 `---` separator repair is correct
  at both boundaries with no doubled blank lines.

---

## Carry-forward disposition (after this review)

- **CONSUMED:** N73-R1 (verified: 0 occurrences of the stale "THREE oneof arms" doc).
- **CF-74-5 — CLOSABLE.** `present_match` on the RESOLVED branch was the one item
  `BEHAVIOR_CONTRACT.md` §G labels "derived, not separately measured". Probe group 1
  (S4/S5) measured it cross-proxy in both polarities. **Recommend the state-3 re-entry
  upgrade §G to MEASURED and close CF-74-5**, quoting the S4/S5 table.
- **OPENED by this review:** **CF-74-6** — the wrapped `BoolValue` spelling
  `match_if_key_not_found: { value: <bool> }` is accepted+honored upstream and
  boot-fatal here (I-1). Owner = a future wrapper-spelling-parity phase, which should
  also survey the other `Option<bool>`/`Option<u32>` wrapper fields for the same gap.
- **STILL OPEN, unchanged:** CF-74-1 (`matcher.invert` accepted-but-INERT upstream,
  boot-fatal here — verified still boot-fatal and pinned at `bootstrap.rs:13468`),
  CF-74-2 (multi-segment `path`), CF-74-3 (unmodelled `ValueMatcher` arms), CF-74-4
  (the RBAC validator's missing empty-segment-`key` check).
- **NOT consumed:** M73-R2 is **partially advanced** — probe groups 1 and 4 measured the
  mixed-leaf / depth-2 / composition cases cross-proxy, but M73-R2 asks for a committed
  FIXTURE, so it stays open. M71-3, M71-6 (this review measured the H2 gate live but
  I-2's ask is an in-process test, so the standalone H2 *differential* stays open),
  M71-7/8, M70-R4/R9, CF-72-1/CF-72-2 (still the strongest NEXT candidate), CF-73-1,
  N73-R2, M73-R1, M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7, the older Minors
  and the HTTP-filters-family (1)–(4).

---

## Next state

**§5.2 state-3 RE-ENTRY (step 3, NOT step 4) — a SEPARATE session.** Scope = I-1
through I-4 (incl. I-2b's doc-comment restoration and the `PROGRESS.md` call-site
recount), plus the CF-74-5 §G upgrade; fold Minors opportunistically — M74-1, M74-2 and
M74-15 are one-word edits on files the re-entry already touches. Per memory
`state3-reentry-fixes-are-characterization-pins-red-via-mutation`, all four fixes pin
ALREADY-CORRECT code (this review measured every one at parity), so the new tests will
pass immediately — **honor TDD's RED with a mutation check** (break the engine, watch it
go RED, revert), preferring the exact mistake each finding warns about:

- I-2 → mutate `envoy-http2/src/hcm.rs:1142` to `&Default::default()` and watch the new
  H2 test go RED.
- I-3 → mutate `unwrap_or(true)` → `unwrap_or(false)` and watch fixture `0081` go RED
  (it currently does not — that is the whole point of the finding).
- I-4 → clear the `compiled` field and watch the new `SafeRegex` case panic/RED.
- I-1 → the new serde pin is RED before the doc/carry-forward work by construction only
  if the model changes; instead record the mutation as "the pin fails if `Option<bool>`
  is replaced by a wrapper-accepting deserializer", i.e. it pins the DELIBERATE posture.

Record the RED evidence in `PROGRESS.md`. Then state-4 re-verification, then a state-5
re-review. ROADMAP row `74` stays `in-progress` until the state-6 close-out.

---

# §5 state-5 RE-REVIEW (the SECOND review — this is the phase's CURRENT verdict)

> `superpowers:requesting-code-review`, run in its OWN fresh session per §5.1 /
> ADR-0127 (the context that wrote an artifact must not grade it). This section
> supersedes the verdict at the top of this file; that verdict's reasoning is
> preserved verbatim above per D-3.5 and must not be re-litigated.
>
> **What is graded here:** the §5.2 state-3 re-entry's diff
> `git diff 93ec7393c2648751ac8323e1e02cc6d09b15f2e8..cab381d2784e1497aa46fd5054c1faa08c6c5d97`
> (10 files, +965/−90) against the four Important findings it claims to close,
> PLUS the state-4 RE-VERIFICATION commit `2fb0b456404199f6b5db5b0bde1a7e7341e9e56e`
> (docs-only, 3 files) whose §7.5 gate verdict this review CONSUMES.
> Session HEAD = `2fb0b456…`; `git status --porcelain` clean; `git fetch origin
> --prune` showed `origin/main` at the SAME SHA (no sibling workstream had
> advanced). CI on `2fb0b456…` was already `completed`/`success` (run
> `30105721394`, both jobs green with full step counts 15 and 13).
>
> **The §7.5 gate (a)–(e) was NOT re-run** — it was re-run and adjudicated GREEN
> at the state-4 RE-VERIFICATION and its evidence is quoted in `PROGRESS.md`
> `## §7.5 gate (state-4 RE-VERIFICATION)`. Gate **(f) IS this review**.
>
> **Method** (memory `state5-must-probe-untested-compositions`): five FRESH
> zero-context read-only reviewers fanned out across independent dimensions (the
> new H2 test; the `envoy-config` test/doc changes; the fixture change; the
> contract + carry-forward docs; the PROGRESS/STATE narrative audited against
> disk), each forbidden to write and forbidden to run `cargo`. The MAIN session
> then made the decisive **MUTATION** measurements ITSELF in a scratch
> `git worktree` (memory `mutation-checks-collide-with-parallel-subagents`), and
> independently re-derived every load-bearing disk fact. **A green gate proves the
> code does what its tests ASK, not that the tests ask the right question.**

---

## Verdict: **APPROVED WITH MUST-FIX — 3 Important, 0 Critical. Next = §5.2 state-3 RE-ENTRY (step 3, NOT step 4).**

**All four original Important findings (I-1, I-2 + I-2b, I-3, I-4) are genuinely
and correctly CLOSED.** No behavioral defect. No Critical. The re-entry changed
**no production behavior** — re-derived on disk, not taken from the handoff:

```
$ git diff 93ec7393..cab381d2 -- crates/envoy-http1/src/hcm.rs crates/envoy-accesslog/src/filter.rs | wc -l
0
```

Both engine files are byte-unchanged, so the behavior state-4 and the first
state-5 already adjudicated cross-proxy is exactly the behavior that ships, and no
behavioral re-adjudication is owed. An independent audit against disk found the
written record **HONEST** — every load-bearing figure checks out, including the
ones easiest to fudge.

The three MUST-FIX items are **all documentation-accuracy defects, none
behavioral**, and they share one root cause: *the re-entry changed things but did
not carry the change into every place that describes them.* Each fix is a
one-to-few-line edit; **no code change and no new test is required**, so the
re-entry is purely editorial.

Two of the three are the SAME defect class this phase has already been through
once — I-1 was made MUST-FIX precisely because a document asserted something
untrue about measured behavior. Consistency with the phase's own established bar
is why these block rather than merely being recorded.

ROADMAP row `74` stays `in-progress` — no flip until the state-6 close-out.

---

## The four original findings — CONFIRMED CLOSED

I re-ran the two cheapest mutations myself rather than taking the re-entry's
transcript on trust, and then re-ran the most load-bearing one too. All three
reproduce the recorded RED **verbatim**.

| finding | independent verification | verdict |
|---|---|---|
| **I-1** wrapped `BoolValue` contract defect | §A's untrue "accepted alongside a bare `true`" parenthetical is DELETED; §A now says the two spellings are **NOT at parity**; §D carries the divergence as its fourth strictness item with the full measured table INCLUDING the decisive `{ bogus: false }` control; the correction propagated to the production doc comment (`bootstrap.rs:782-785`); the serde pin loops BOTH wrapped polarities (`bootstrap.rs:13509-13519`). `Option<bool>` was left alone as directed — no `deserialize_with`, no untagged shim; the absent-vs-explicit-`false` distinction is intact end-to-end (`:13469` `Some(false)` vs `:13481` `None` → `unwrap_or(true)` at `envoy-http1/src/hcm.rs:1806`). | **CLOSED** |
| **I-2** H2 emit gate undefended | The new test exercises the **real H2 production path**, not a direct `should_log` call: `spawn_h2_hcm` → `serve_h2_connection` → `h2::client::handshake` → `finalize_h2_stream` → the gate at `envoy-http2/src/hcm.rs:1138`, with the metadata produced by the **real** `header_to_metadata` filter rather than injected. So it pins *threading*, which is what the finding asked for — strictly stronger than the prescribed fix, whose named H1 model is only a compile+engine test. | **CLOSED** |
| **I-2b** two doc comments describing the gate backwards | Exactly two `///` lines were corrupted by `796450d` (re-derived: `--numstat` = `3 2`, the three `+` lines being ONE production argument and TWO `///`); both now describe the gate correctly; the only surviving `&Default::default()` in prose (`:3673`) correctly describes the *mutation*, not the gate. `grep "should_log(" \| grep -v "///"` → exactly **1**, the real gate. No H1 or `envoy-accesslog` prose describes the gate backwards. | **CLOSED** |
| **I-3** no fixture read the wrapper default | Fixture `0081`'s third probe is a genuine **CROSS-PROXY** witness, not a one-sided one: the probe omits `extra_headers` entirely, `match_if_key_not_found` is absent from **both** side configs, and the driver asserts `expected_lines == 2` **independently on the envoy side** (`tests/differential/src/lib.rs:6415-6422`) as well as on envoy-rust (`:6423-6430`), then compares the two files **positionally** via `zip` (`tests/differential/src/access_log.rs:316`) — so ordering is genuinely pinned, and the two kept lines are byte-distinct (`M=-` then `M=1`). Kept-LAST holds, so the cheap 2 s `CF70_3_SETTLE` is preserved. | **CLOSED** |
| **I-4** `SafeRegex` compiled but never evaluated | The added case compiles via the **same** `ValueMatcher::compile_safe_regexes` the production validator calls (`bootstrap.rs:5408`), so the pin is representative rather than hand-constructing a compiled regex. It asserts all three outcomes including the load-bearing `Some(false)` ≠ `None` — a regex REJECTION must not fall back to `match_if_key_not_found`; only an unresolved PATH may. The corrected comment is now TRUE against the five-variant `StringMatcherMode`. | **CLOSED** |

### My own mutation re-runs (scratch `git worktree`, main tree never mutated)

Every run shows `Compiling <crate>`, so none is a stale-binary false pass (memory
`mutation-check-needs-forced-rebuild`; note `cargo clippy` prints `Checking`, not
`Compiling` — memory `clippy-prints-checking-not-compiling`).

| # | mutation | result | conclusion |
|---|---|---|---|
| **A** (I-4) | drop the validator-equivalent `compile_safe_regexes()` from the new `safe_regex` helper | `panicked at crates/envoy-config/src/matcher.rs:142:18: validator ensured StringMatcher SafeRegex compiled` · `2 passed; 1 failed` | RED at the **exact** `.expect()` I-4 names, reached from the metadata route. Reproduces the record. |
| **B** (I-2) | `&record.dynamic_metadata` → `&Default::default()` at the H2 gate (`hcm.rs:1142`) | `panicked at …/hcm.rs:3817:9` · `left: ""` / `right: "200\n"` · `0 passed; 1 failed` | Reproduces the record verbatim. |
| **B2** (NEW — not measured by the re-entry) | mutation B **plus** the first assertion neutralized, to test whether the SECOND half is also load-bearing | `panicked at …/hcm.rs:3819:9` · `left: "200\n200\n200\n"` / `right: "200\n200\n"` | **Both halves ARE load-bearing.** The test's own doc claim at `:3683-3685` ("an empty store would make the `false` sink drop everything AND the default-`true` sink keep everything (3 lines, not 2)") is now MEASURED. The re-entry asserted this but never observed it — its run aborted at the first assert. |
| **C** (I-3) | `unwrap_or(true)` → `unwrap_or(false)` at `envoy-http1/src/hcm.rs:1806`, with `cargo build -p envoy-bin` in the worktree first (memory `differential-harness-uses-debug-envoy-bin`) | `fixture green: envoy-rust emitted 1 access-log lines but 2 were expected to be logged; lines: ["STATUS=200 PATH=/x M=1"]` · `0 passed; 1 failed` (18.32 s) | RED for exactly the right reason — the missing line is `STATUS=200 PATH=/x M=-`, the wrapper-default witness. Real Envoy is unaffected and still emits both, so the assertion is genuinely cross-proxy. Reproduces the record verbatim. |

**An environmental flake was caught by a CONTROL rather than mis-reported as a
result.** Mutation C's FIRST attempt failed with `upstream Envoy never became
accept-ready … Connection refused (os error 111)` — a container-startup failure,
not the semantic assertion. Rather than record that as the RED, I ran the
**unmutated** fixture from the same worktree as a control: `1 passed; 0 failed`
in 12.80 s. That proved the worktree/bind-mount setup sound and the first failure
transient, so mutation C was retried and produced the real semantic RED above.
Docker was verified healthy (daemon 28.1.1, no stale containers, `/dev/kvm` ACL
present) and the pinned image confirmed on disk with the digest matching
`ENVOY_TARGET.md` exactly (`envoyproxy/envoy:v1.33.0`,
`sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`).

**Main tree hygiene, verified AFTER every run** (memory
`mutation-checks-collide-with-parallel-subagents` — a parallel reviewer's
`git checkout --` silently reverts an in-place mutation and produces a false green
a `Compiling` grep does NOT catch): `git status --porcelain` → **0** lines
throughout; `unwrap_or(true),` ×1, `&record.dynamic_metadata,` ×1 and
`compile_safe_regexes().expect("pattern compiles")` ×2 all intact in the main
tree; the mutation was re-grepped as still PRESENT in the worktree after each run;
the scratch worktree was removed; **0** leftover probe containers. (Four
`.claude/worktrees/agent-*` worktrees exist on their own branches — a concurrent
session's artifacts, not mine and not removed.)

---

## An untested composition that does NOT need probing — and why

The two fixture/test reviewers independently flagged "`metadata_filter` nested in
`and_filter`/`or_filter` **on H2**" as an uncovered cross-product cell. I checked
whether it is genuinely reachable before spending a live probe on it, and it is
**closed by construction**:

- `crates/envoy-accesslog/src/filter.rs:144-149` — the `And`/`Or` arms re-thread
  `dynamic_metadata` **verbatim** into the recursive `should_log`.
- `crates/envoy-accesslog/Cargo.toml` has **ZERO workspace dependencies**, so
  there is exactly **one** `should_log` implementation in the workspace, shared
  identically by both HCMs. Nothing about composition is codec-specific.
- The only codec-specific fact is *which store is passed into that one function* —
  and that is precisely what the re-entry's new H2 test now pins (mutations B/B2),
  on top of the first state-5 review's live cross-proxy H2 measurement.

So (H2 threading ✅ pinned + measured) × (composition recursion ✅ measured
cross-proxy on H1 at probe groups 1/4) covers the cell; a separate H2-composition
probe would re-measure shared code through a second front door. Recorded as
reasoning, not asserted as parity. **M73-R2 / M71-6 still stand** for the
committed-FIXTURE asks, which this does not satisfy.

---

## The §7.5 gate — CONSUMED, with its most falsifiable claim independently checked

The gate verdict `(a) ✅ (b) ✅ (c) ✅ (d) ✅ (e) ✅` stands and is consumed. I did
not re-run it. I did independently verify the **structural half** of gate (b)'s
arithmetic, which is the part a re-review can check without re-running anything:

- The claim is `local 2090 + 5 = 2095 == CI 2095` over **159** `test result:`
  lines on both sides, with the `+1` over the prior `2089 + 5 = 2094` baseline
  being exactly ONE new test fn landing in an EXISTING binary.
- Measured: `git diff 93ec7393..cab381d2 | grep -cE '^\+\s*#\[(tokio::)?test'`
  → **1** — `h2_metadata_filter_gate_reads_the_threaded_dynamic_metadata`, inside
  the existing `mod tests` of `crates/envoy-http2/src/hcm.rs`. No `Cargo.toml`
  and no `tests/*.rs` target was added.

That is precisely what a `+1` with an unchanged 159-line count requires, so the
arithmetic is internally coherent. Independently re-derived on disk this session:
the state-4 commit is genuinely **docs-only** (3 files, no `crates/`, `tests/`,
`ci.yml`, `DECISIONS.md` or `ROADMAP.md`); **82** fixture directories; **36**
fixtures carrying an `access_log` stanza (and the alternate **34**-by-directory-name
figure also reconciles — both are right under their own stated rule, no defect);
**63** tracked corpus seeds; `known-failures.txt` still **21** lines last touched
by `dac3f8b` (phase 05.2) and NOT trimmed; `#![forbid(unsafe_code)]` at **22 of 22**
workspace member roots; ROADMAP row `74` **6 cells** / `in-progress` (correctly
NOT flipped); ledger head **ADR-0155** with **0** occurrences of `## ADR-0156`.
The `unreachable!` three-part reconciliation also holds (`grep -c` → 2; `:1750` a
doc comment, `:1808` the guard; **0** such lines added or removed phase-wide).

---

## Findings

### Critical

**NONE.** Across five review dimensions, four mutation measurements and a
full disk audit, no input was found that produces a wrong verdict, a panic, or a
broken load-bearing invariant.

### Important (MUST-FIX — these define the second §5.2 state-3 re-entry)

**I-5 — `BEHAVIOR_CONTRACT.md` §G over-reads its own S4/S5 table, re-creating the
"derived presented as measured" pattern inside the section written to retire it.**
`docs/envoy-rust/BEHAVIOR_CONTRACT.md:2570-2574` states: *"The derived rule is
confirmed: with the key RESOLVED, `present_match: true` KEEPS and
`present_match: false` DROPS; **with the key ABSENT both defer to
`match_if_key_not_found`** (which is why S4, whose policy is `false`, drops exactly
the requests S5, whose policy is `true`, keeps)."*

The absent-branch half is **not isolated by that table**. S4 and S5 flip TWO
variables at once — the `present_match` polarity AND the `match_if_key_not_found`
policy. I worked the alternative through by hand. With `header_to_metadata`
mapping `x-a` → `com.example:k`, the key resolves exactly for {r1, r2, r5, r6} and
is absent for {r3, r4, r7}. Now consider a competing rule in which the matcher
returns `Some(present == want)` on **both** branches and `match_if_key_not_found`
is never consulted at all:

- S4 (`want = true`): resolved → `true == true` → KEEP {r1,r2,r5,r6}; absent →
  `false == true` → DROP. **Predicts exactly the observed row.**
- S5 (`want = false`): resolved → `true == false` → DROP; absent →
  `false == false` → KEEP {r3,r4,r7}. **Predicts exactly the observed row.**

The two hypotheses are observationally identical on this table, so it cannot
confirm the absent-branch attribution, and the parenthetical offers the S4/S5
complementarity as though it were the evidence. The discriminating probe — hold
`present_match` constant and flip `match_if_key_not_found` — was not run in probe
group 1.

**Why it matters:** §G's headline is now "**MEASURED cross-proxy**", so a future
reader will treat every clause beneath it as measured-and-isolated. This is
exactly the defect class I-1 was raised for, appearing in the very section written
to retire it. **The underlying fact is true and IS measured — just elsewhere:**
§B's R-0.4 polarity flip isolates it properly (it holds the value matcher and the
key's absence constant while flipping only the policy), and probe group 1's
matcher-less S6/S7 pair isolates the policy with no value matcher present at all.
**CF-74-5's closure is NOT affected** — CF-74-5 was scoped to `present_match` on
the RESOLVED branch, both hypotheses agree there, and the table does isolate it in
both polarities on both proxies.
**Fix (one sentence, no re-probing):** scope the confirmation to the resolved
branch and cite the isolating evidence for the other half — e.g. *"…
`present_match: false` DROPS. The ABSENT branch's deferral to
`match_if_key_not_found` is not isolated by S4/S5, which flip both variables; it is
measured separately by §B's R-0.4 polarity flip and by probe group 1's matcher-less
S6/S7."*

**I-6 — fixture `0081` was reshaped but three of the four places that describe it
were not carried forward — and the worst of them actively invites the edit that
would silently vacate the witness the re-entry just bought.**
The re-entry correctly updated `0081/README.md`. It did not update:

- **(a) the fixture's OWN config comments — the one that matters.**
  `tests/fixtures/0081-accesslog-metadata-filter/envoy.yaml:26-29` and verbatim at
  `envoy-rust.yaml:24-27` still read: *"`match_if_key_not_found` is ABSENT here —
  its MEASURED default is `true`, but **every probe in this fixture sets `x-a`, so
  the key always resolves and the not-found path is never taken** (fixture 0082
  covers it)."* Every clause after the dash is now false: probe 2 sends **no**
  `x-a`, the key does **not** always resolve, the not-found path **is** taken, and
  `0082` is no longer the only fixture covering it. Compounding it, `0081`'s
  configs carry **zero** mention of `on_header_missing`, whereas `0082`'s configs
  carry the ADR-0155 PV-6 warning **inline** (`envoy.yaml:27`,
  `envoy-rust.yaml:25`); `0081`'s warning lives only in its README — not the file
  an editor edits. So a future editor reading "every probe sets `x-a`" concludes
  the `on_header_missing` omission is inert here and adds it — which makes the key
  RESOLVE and silently vacates the brand-new default-`true` witness **while the
  fixture stays green**. This is the same failure mode `0082`'s omission was
  celebrated for catching.
- **(b) the test entrypoint's module doc.**
  `tests/differential/tests/access_log_metadata_filter.rs:15-19` still says *"**Two
  probes**, kept-LAST (ADR-0147)"* and *"Each side's file holds **EXACTLY ONE**
  byte-identical line `STATUS=200 PATH=/x M=1`"*. It is three probes and two lines,
  and the doc never mentions the `match_if_key_not_found` default branch the
  fixture now witnesses. This is the file `cargo test` failure output points at,
  and the sibling `access_log_metadata_filter_key_not_found.rs:20-23` enumerates
  its own probes correctly, so the asymmetry is conspicuous.
- **(c) `BEHAVIOR_CONTRACT.md` §H.** `:2579-2591` still describes `0081` as
  `x-a: 2` → DROPPED, `x-a: 1` → KEPT, "**one line** `STATUS=200 PATH=/x M=1`",
  and still scopes the `on_header_missing` load-bearing note to **`0082`** alone
  even though `STATE.md:28` now explicitly records that *"**The SAME trap now
  applies to `0081`**"*. This is the living contract, not a phase artifact, so it
  propagates. Note §F immediately above it WAS rewritten in the same commit, so
  the authoring session updated the contract and stopped one subsection short.

**Fix:** rewrite the comment in both `0081` YAMLs to describe probe 2 and add the
inline PV-6 `on_header_missing` warning mirroring `0082`; update the entrypoint
module doc to three probes / two byte-distinct lines / the default-`true` branch;
update §H's `0081` sentence and generalise its PV-6 note to both fixtures.

**I-7 — `STATE.md` — the cold-start source of truth — states the workspace has
"exactly ONE fuzz target"; it has FIVE.**
`docs/envoy-rust/STATE.md` asserts *"exactly ONE fuzz target, so no `ci.yml` edit"*
and *"63 seeds, one target"*; `PROGRESS.md:1362`'s invariant bullet repeats
*"exactly **one** fuzz target"* unscoped. Measured on disk:

```
crates/envoy-accesslog/fuzz/fuzz_targets/accesslog_format_parse.rs
crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs
crates/envoy-filter/fuzz/fuzz_targets/cdn_loop_parse.rs
crates/envoy-http2/fuzz/fuzz_targets/grpc_health_decode.rs
crates/envoy-jwt/fuzz/fuzz_targets/jwt_parse.rs
```

— five targets, each wired into `.github/workflows/ci.yml` at lines 107, 113, 120,
127 and 134. (The CI fuzz job name quoted in this phase's own handoff lists all
five, so the record contradicts itself.) The gate-(d) body is correctly **scoped**
— *"`crates/envoy-config/fuzz/fuzz_targets/` still holds exactly ONE target"* — and
that is true, so **the gate verdict stands**: §7.5(d) requires only that any *new*
fuzzer run clean, and the phase added none.
**Why it matters:** `STATE.md` is read in full at every cold start and is the
project's single source of truth. A future session would conclude the workspace has
one fuzz target and that the "new target needs a `ci.yml` step" check (memory
`new-fuzz-target-needs-a-ci-yml-step`) is a one-directory grep — which would let a
genuinely new target land unwired.
**Fix — PARTLY DONE ALREADY; only `PROGRESS.md` remains.** This review session
rewrote `STATE.md`'s four top-section blocks anyway, so it corrected the two
`STATE.md` phrasings in place: `STATE.md` now names all five targets and their
`ci.yml` lines, and the old claim survives only as a quoted defect immediately
followed by its correction (`grep -c "exactly ONE fuzz target, so no"` → **0**).
The re-entry therefore has exactly ONE site left: **`PROGRESS.md:1362`**'s
invariant bullet, *"**63** tracked `parse_bootstrap` corpus seeds; exactly **one**
fuzz target;"* — narrow it to name the directory, e.g. "exactly **one** fuzz target
**under `crates/envoy-config/fuzz/`** (the workspace has five, all already wired
into `ci.yml`)". Do NOT rewrite the gate-(d) body at `:1274-1277`; it is already
correctly scoped.

### Minor (record; fold opportunistically, NOT required for the re-entry)

- **M74-17** — the new H2 test's format is `%RESPONSE_CODE%\n`
  (`envoy-http2/src/hcm.rs:3718`) and all three probes return 200, so every emitted
  line is the byte-identical `"200\n"`. Its `assert_eq!` on exact file contents
  therefore pins **how many** records were emitted, not **which**: a bug that
  mis-associates metadata with requests while preserving the per-sink counts of 1
  and 2 would pass (e.g. swapping probe 1's and probe 3's stores preserves both).
  Ironically the phase's OWN fixture `0081` adopts the better practice — it renders
  `%DYNAMIC_METADATA(com.example:k)%` so its kept lines are byte-DISTINCT, the very
  strengthening the first review praised. Fix: render the gating value in the H2
  test's format too.
- **M74-18** — the new H2 test does not assert `access_logs_total`, though the
  phase-72 precedent it is modelled on asserts it on both legs in the same file
  (`:3648-3652`, `:3662-3666`) precisely because the counter lives INSIDE the gated
  branch (`:1152`). Expected value here is 3.
- **M74-19** — the H2 **production** emit-gate comment (`envoy-http2/src/hcm.rs:1132-1137`)
  still stops at "Phase 72", while its H1 sibling carries the Phase 74 note
  (`envoy-http1/src/hcm.rs:1515-1517`, "thread the record's dynamic-metadata store
  for the `metadata_filter` arm"). The last H1/H2 doc asymmetry on the exact
  argument this phase added — and the phase never touched that block.
- **M74-20** — the I-1 serde pin asserts `err.to_string().contains("expected a
  boolean")` (`bootstrap.rs:13516`), which is the *expected-scalar* half of
  `invalid type: map, expected a boolean`; the half naming the wrapper SHAPE is
  `invalid type: map`. `PROGRESS.md:699-701` and §D both say the pin "names the
  wrapper shape" — slightly stronger than the code. Harmless today (a fixed YAML
  mapping can only produce that message via a bool-typed field), and the
  `Option<bool>`→bare-`bool` regression is caught anyway by `assert_eq!(empty
  .match_if_key_not_found, None)` at `:13481`. Fix: assert both substrings, or trim
  the prose.
- **M74-21** — `matcher.rs:654-664`'s comment claims the absent-key short-circuit
  protects the `.expect()` panic path, but that case compiles the regex first
  (`:663`), so it cannot panic either way; it only pins the tri-state `None`. A
  milder instance of the I-4 class (comment asserting more than the code checks).
  Fix: add a case leaving `compiled: None`, the only construction under which the
  `?`-short-circuit at `:101-102` is load-bearing.
- **M74-22** — `matcher.rs:578-580`'s new "All FIVE modelled variants" claim is
  true today but not compiler-enforced (`StringMatcherMode` is not
  `#[non_exhaustive]` and the test dispatches through a closure), so a sixth
  variant would silently stale it — re-importing into `matcher.rs` the staleness
  class M74-1 just cleaned out of `bootstrap.rs`. Fix: mirror the PV-3
  `six_arm_cardinality_counts_every_arm` pattern with an exhaustive `match`.
- **M74-23** — the house-precedent citation conflates two ADRs.
  `BEHAVIOR_CONTRACT.md:2533-2534` (and `SPEC.md:676`, `bootstrap.rs:13503`) cites
  "`UInt32Value`, **ADR-0063**, pinned by
  `cidr_range_rejects_unknown_field_and_wrapper_prefix_len`", but that test pins
  `CidrRange.prefix_len`, whose own comment attributes it to **ADR-0133**
  (`bootstrap.rs:6742-6743`); ADR-0063 is the buffer-filter ADR and establishes the
  posture via `Buffer::max_request_bytes`. The substance is true and doubly
  measured; only the pairing is wrong.
- **M74-24** — §G's table is not self-interpretable: requests are labelled by
  HEADERS (`r1 x-a:1` … `r7 x-c:1`) while the sinks gate on METADATA, and the probe
  config's `header_to_metadata` mapping is never stated in §G, so a reader cannot
  check why r4/r7 count as key-absent. One clause fixes it.
- **M74-25** — `BEHAVIOR_CONTRACT.md:2467-2468`'s "Every fixture and example in
  this project therefore writes the bare form" is imprecise: `0081` does not write
  the field at all — the omission is deliberate and load-bearing. Fix: "writes the
  bare form **or omits the field entirely**".
- **M74-26** — three of the four first-state-5 probe groups' newly measured
  cross-proxy facts never entered `BEHAVIOR_CONTRACT.md`, though doctrine makes
  that file the canonical record of what was measured: the H2 emit gate in both
  wrapper polarities; a `SafeRegex` metadata value at DEPTH 2 with no panic;
  `Metadata` nested under `and_filter`/`or_filter` at depth 2; and the matcher-less
  explicit-`false` polarity (S7) that drops every record (§A carries only the KEEP
  half). Nothing stated is false — the gap is of record. This is also the cheapest
  source of the isolating citation I-5 needs.
- **M74-27** — `SPEC.md:95-99` (§0 R-0.2) still presents the wrapper acceptance
  without a CF-74-6 pointer. Not untrue (it is scoped to the upstream
  `--mode validate` recon) but it is the same phrasing that misled in §A. Per
  append-mostly doctrine add a bracketed forward note rather than rewriting.
- **M74-28** — labelling imprecisions in the state-4 narrative, none of which
  weakens its evidence: "all four re-entry-touched crates" (the re-entry touched
  **two** — `envoy-config` and `envoy-http2`; four is the count for the whole
  phase); "the five changed `.rs` files were `touch`ed" (the re-entry changed
  **three**; no counting rule yields five); "and four docs files" (the diff has
  **five** `docs/` markdown files, correct only if the file the sentence is written
  in is silently excluded); "every engine file is byte-unchanged" —
  `envoy-http2/src/hcm.rs` IS changed (+168/−2), true only in substance because
  every hunk sits inside `mod tests`; "`0081` has **0** hits of any kind" for
  `on_header_missing` (0 in its YAML, but **3** in its README, added by that same
  re-entry — correct only under the bullet's config-key scope); and ":3559
  overran the file's wrap width" reconciles with neither a 100-col rule (`:3559` is
  96 chars, `:3457` is 100) nor an ~80-col doc convention (both overran).
- **M74-29** — no committed fixture nests `metadata_filter` inside
  `and_filter`/`or_filter` (measured live at probe groups 1/4, never pinned) and
  none exercises "namespace PRESENT but key absent" cross-proxy — the second `?` at
  `matcher.rs:102`. Both are engine-covered in-process; the former is **M73-R2**'s
  surface and stays open there.

---

## Strengths

- **The re-entry did the hard version of TDD's RED.** All four findings pinned
  ALREADY-CORRECT code, so "watch it fail" was impossible by construction. Rather
  than skip RED, it mutated the engine in exactly the way each finding warned about
  and recorded the failure — and **all three mutations I re-ran reproduce verbatim,
  down to the panic line and the literal `left`/`right` values.** That is the
  discipline memory `state3-reentry-fixes-are-characterization-pins-red-via-mutation`
  asks for, executed faithfully.
- **I-3's RED was a TWO-PART demonstration, which is the right shape for a
  coverage finding.** Showing the mutation passes GREEN against the PRE-FIX 2-probe
  fixture *reproduces the finding* (the fixture was blind), and RED against the new
  3-probe fixture *proves the fix closes it*. Most re-entries would have shown only
  the second half.
- **The new H2 test is strictly stronger than the fix that was prescribed.** The
  prescription named an H1 model that is only a compile+engine test; the re-entry
  instead drove a real H2 handshake through the production HCM with metadata
  produced by the real `header_to_metadata` filter, and added the
  `match_if_key_not_found` polarity flip nobody asked for. Both halves are
  load-bearing (my mutation B2).
- **An unmeasured claim was written and then deliberately DELETED before commit**
  (`PROGRESS.md:900-909`): a draft `SafeRegex` assertion captioned with a claim
  about upstream's full-match semantics that this project never measured. Catching
  that in oneself — the exact I-1 defect class the same session was fixing — and
  removing it rather than shipping it is the single best piece of judgment in the
  re-entry. `matcher.rs:648-653` now says anchoring is out of scope instead.
- **`Option<bool>` was left alone under pressure.** The obvious "fix" for I-1 is to
  make the field wrapper-accepting; that would have destroyed the
  absent-vs-explicit-`false` distinction `unwrap_or(true)` depends on. The re-entry
  correctly treated the divergence as a documented posture and pinned it instead.
- **The record is HONEST.** An independent audit against disk checked every
  load-bearing figure — the governing 0-line diff, the docs-only 3-file state-4
  commit, 82 / 36 / 63 / 21, 22-of-22 `#![forbid(unsafe_code)]`, the dual 36-vs-34
  counting rules, all four per-file numbers of the `78 + 2` recount, the
  `unreachable!` three-part reconciliation, ROADMAP row 74, ADR-0155 with no
  ADR-0156, and both mutation targets intact at the precise lines claimed — and
  found no materially false claim. "NO production behavior changed" verifies
  line-by-line: **zero executable production lines changed**, every `.rs` hunk
  sitting inside `mod tests` or a `///` doc comment.
- **The `78 call sites + 2 doc-comment edits` recount was re-derived on disk, not
  transcribed** from the review that requested it — and it is right to the file.
- **`0081`'s third probe was placed SECOND on purpose**, preserving kept-LAST
  (ADR-0147) so the fixture keeps paying the cheap 2 s settle instead of the 12 s
  one, and making the two kept lines byte-distinct so ORDER is pinned as well as
  count. That is a fixture-design detail most sessions would have gotten wrong by
  appending.
- **The ADR-0150 seam held under a genuine temptation.** The new H2 test needs a
  real `MetadataMatcher`, which lives on the `envoy-config` side; it constructs it
  there and boxes it through `Arc<dyn MetadataMatch>` exactly as
  `compile_access_log_filter` does, adding no reverse edge. `envoy-accesslog` still
  has ZERO workspace deps and `LogFilter` still derives only `Debug, Clone`.
- **CF-74-5's closure is honest and correctly scoped.** The S4/S5 rows do isolate
  what CF-74-5 actually claimed (`present_match` on the RESOLVED branch, both
  polarities, both proxies, with a byte-level md5 witness); I-5 is about one extra
  clause bolted onto that table, not about the closure.
- **CF-74-6 is opened consistently in both required places** with the same
  measurement, the same `{ bogus: false }` control, the same "`Option<bool>` is
  correct, do not close this field alone" rationale, the same owner, and the same
  statement that the gap is in the REJECT direction and therefore fail-loud.
- **The state-4 re-verification refused three available shortcuts** — a cached
  clippy green (`0.09s`, 0 `Checking` lines) re-run after a `touch`; a vacuous grep
  that had globbed an empty fixture name, redone against resolved names; and an
  `unreachable!` count discrepancy traced to a doc-comment line rather than waved
  through. Refusing a shortcut that nobody would have audited is the behavior this
  process exists to produce.

---

## Carry-forward disposition (after this re-review)

- **CONSUMED earlier in the phase:** N73-R1.
- **CLOSED:** **CF-74-5** — confirmed correctly closed and correctly scoped
  (see Strengths); I-5 does not reopen it.
- **OPEN, correctly recorded:** **CF-74-6** (the wrapped `BoolValue` spelling).
- **FOLDED at the re-entry, verified:** M74-1 (grep over `crates/` → 0 hits),
  M74-2, M74-15.
- **OPENED by this re-review:** **M74-17 … M74-29** (all Minor, listed above).
  The three Important items I-5/I-6/I-7 are MUST-FIX for the next re-entry, not
  carry-forwards.
- **STILL OPEN, unchanged:** CF-74-1 (`matcher.invert` accepted-but-INERT upstream,
  boot-fatal here — do NOT "implement" it), CF-74-2 (multi-segment `path`),
  CF-74-3 (unmodelled `ValueMatcher` arms), CF-74-4 (the RBAC validator's missing
  empty-segment-`key` check), M74-3…M74-14, M74-16, M73-R2 (still asks for a
  committed FIXTURE — probe groups 1/4 and this review's structural argument
  advance but do not satisfy it), M71-3, M71-6 (the standalone H2 *differential*
  stays deferred), M71-7/8, M70-R4/R9, **CF-72-1/CF-72-2** (still the strongest
  NEXT candidate), CF-73-1, N73-R2, M73-R1, M69-A..I, CF-69-1/2/3/5, M68-1, M-1,
  CF-67-3/5/6/7, the older Minors and the HTTP-filters-family (1)–(4).

---

## Next state

**§5.2 state-3 RE-ENTRY (step 3, NOT step 4) — a SEPARATE session.** Scope = I-5,
I-6 (all three sub-items) and I-7's ONE remaining site. **Every fix is
documentary** — a contract sentence (§G), fixture `0081`'s two config comment
blocks plus the inline PV-6 warning, one test module doc, contract §H, and the
single `PROGRESS.md:1362` fuzz-target phrasing (I-7's two `STATE.md` phrasings were
already corrected by this review session, which rewrote those blocks anyway). **No
code change, no new test and no new fixture is required, and none should be added.**

Because nothing executable changes, there is no engine to mutate for TDD's RED and
none should be manufactured: per memory
`state3-reentry-fixes-are-characterization-pins-red-via-mutation` the RED
obligation attaches to *behavioral* pins, and these are prose corrections whose
verification is a re-read plus a grep. Record for each fix the before/after text
and the grep proving the stale phrasing is gone (e.g. `grep -c "every probe in
this fixture sets"` → 0; `grep -c "Two probes"` over the `0081` entrypoint → 0).

Do **not** re-run the §7.5 gate in that session — but note the following state-4
re-verification WILL need to re-run it, because I-6(a) edits two fixture config
files, and any fixture-config edit must be re-proven cross-proxy (`cargo build -p
envoy-bin` first, memory `differential-harness-uses-debug-envoy-bin`). Comment-only
edits inside the YAML do not change semantics, but that must be *shown*, not
assumed.

Then: state-4 re-verification → state-5 re-review → the state-6 close-out.
ROADMAP row `74` stays `in-progress` throughout. Do **not** flip it, and do **not**
create a `stop` file — the `MISSION.md` §9 feature families remain largely unbuilt.
