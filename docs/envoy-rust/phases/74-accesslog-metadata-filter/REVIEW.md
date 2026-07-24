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
