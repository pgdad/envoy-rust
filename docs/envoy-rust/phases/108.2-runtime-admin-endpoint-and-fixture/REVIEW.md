# Sub-phase 108.2 — admin `GET /runtime` + the nine `runtime.*` stats + fixture `0087` — CODE REVIEW

**Verdict: APPROVED.**

Per `BOOTSTRAP_PROMPT.md` §7.5, an approved `REVIEW.md` closes gate **(f)** — the only
gate still open. Gates (a)-(e) were run and adjudicated FRESH by the §5 state-4
verification session and are recorded, with actual command outputs, in `PROGRESS.md`'s
state-4 section. **This review did not re-run them and does not re-adjudicate them**
(§5.1; ADR-0127 — the context that ran the gate must not grade it, and the context that
grades it must not fix it). It re-confirmed CI on this HEAD independently (§0.3) and
probed two untested compositions against the pinned upstream image (§0.5) — new
measurements a state-5 session is entitled to make, not gate re-runs.

**Zero Issues. Two Minors, six Nits.** Not one changes an accept/reject verdict, alters
wire behaviour, weakens a fixture, or leaves a validator unwired. The sharper Minor
(M-1) is a measured-FALSE claim in the new contract text — found by probing the one
upstream cell the record asserted without measuring. Per §6.3 and ADR-0165 **nothing
was fixed by this session**; the findings are banked for the state-6 close-out to carry
and for the next runtime-family slice to weigh.

---

## §0 — How this review was conducted

### §0.1 — Scope

The unit of review is the non-`docs/` diff `d1760b0..42fb9d7` — **nine files,
+855 / −1, net 854 LoC** (matches `PROGRESS.md`'s recorded census):

| file | + | − |
|---|---:|---:|
| `crates/envoy-admin/src/endpoint.rs` | 280 | 1 |
| `crates/envoy-bin/src/runtime_stats.rs` (NEW) | 164 | 0 |
| `crates/envoy-bin/src/main.rs` | 5 | 0 |
| `tests/differential/src/lib.rs` | 72 | 0 |
| `tests/differential/tests/runtime_static_layer.rs` (NEW) | 18 | 0 |
| `tests/fixtures/0087-runtime-static-layer/envoy.yaml` (NEW) | 64 | 0 |
| `tests/fixtures/0087-runtime-static-layer/envoy-rust.yaml` (NEW) | 65 | 0 |
| `tests/fixtures/0087-runtime-static-layer/expectations.yaml` (NEW) | 121 | 0 |
| `tests/fixtures/0087-runtime-static-layer/README.md` (NEW) | 66 | 0 |

`docs/` changes in the same range are `BEHAVIOR_CONTRACT.md` (+87 — a phase deliverable,
reviewed in §1/§3), `PROGRESS.md` (+390), `STATE.md` (30/15) and `STATE_HISTORY.md`
(+80) — state ledger, not code under review.

### §0.2 — Method

Four read-only review dimensions were fanned out in parallel (SPEC/PLAN conformance;
the three code surfaces; fixture-data integrity; contract-text accuracy + the M-5/M-6
dispositions), each instructed not to write, not to run `cargo`, and to read the banked
`108.1` findings before grading. **Every finding below was re-verified on disk by the
main session**, every line number re-derived at this commit, and every decisive
measurement (the §0.5 upstream probes, the M-5 grep, the CI census, the fixture entry
derivation) made by the main session itself. Both Minors were reached by two
independent routes (a dimension and the main session), per the standing
weight-by-independent-routes rule.

### §0.3 — CI re-confirmed independently on this HEAD

Not inherited from the handoff. Run **`31288441844`** on the full 40-char SHA
`42fb9d7bf6c99cbb57ef30eaf12c96e9b93dc910`, `conclusion=success`, attempt 1, two jobs
on REAL runners (`GitHub Actions 1000005120` / `1000005119`) with step counts **15**
and **13**:

```
$ grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' <job log> \
    | awk '{b++; p+=$4; f+=$6} END{print "binaries="b" passed="p" failed="f}'
binaries=164 passed=2180 failed=0

$ grep -c 'test result: FAILED' <job log>
0
```

Job-scoped log **406 417** bytes — asserted only to be in the hundreds of KB (a
jobs-API log's byte size is not the run log's; the standing rule). A method trap fired
and is worth banking: the first `gh run list --commit` returned `[]` because the
retyped SHA had silently LOST one character (39 chars) — the same failure shape as a
short SHA. Re-derived with `git rev-parse HEAD`, never retyped.

### §0.4 — The test-count arithmetic identity CLOSES EXACTLY

| quantity | value | source |
|---|---:|---|
| CI `passed` at the phase base | **2170** | run `31260569093` (`ced6802` lineage), 163 binaries |
| new test functions in the diff | **+10** | enumerated by name below |
| CI `passed` on `42fb9d7` | **2180** | run `31288441844`, 164 binaries |

**2170 + 10 = 2180**, binaries 163 → **164** (+1, the new `runtime_static_layer`
binary). The ten, enumerated: `runtime_renders_empty_snapshot_for_absent_block`,
`runtime_renders_one_empty_string_layer_for_an_empty_block`,
`runtime_renders_the_measured_two_layer_snapshot`,
`runtime_query_string_still_dispatches`, `runtime_post_is_method_not_allowed`,
`config_dump_serializes_layered_runtime_positively` (6, envoy-admin);
`registers_all_nine_runtime_stats_with_measured_values_and_kinds`,
`absent_and_empty_blocks_differ_in_num_layers_only` (2, envoy-bin);
`fixture_0087_expectations_parses_as_admin_scrape_with_expected_stats` (1,
differential lib); `runtime_static_layer` (1, the fixture binary). **A count trap
fired while deriving this:** `grep -c '^+.*#\[test\]'` over the diff returns **9** —
the tenth is `#[tokio::test]`, which the pattern misses. Enumeration, not the count,
closes the identity.

### §0.5 — Two upstream compositions probed by the main session (pinned image, digest-verified)

State 4 owns the gate; state 5 probes what the tests did not ask (the standing
green-gate-proves-the-tests-pass rule). Both probes ran against
`envoyproxy/envoy:v1.33.0@sha256:56da5afd…70c2`, port-mapped, with controls:

1. **`POST /runtime` → `200 OK` with the full runtime body.** So did `DELETE
   /runtime`, and so did `POST /config_dump` — upstream v1.33.0 does not
   method-restrict its read-only admin endpoints at all. Control: `GET
   /runtime_modify` → `405 Method Not Allowed` reproduced (the probe distinguishes;
   the recorded CF-108-2 fact is confirmed). **This refutes the contract's "POST → 405
   bilaterally" claim — finding M-1.**
2. **The absent-vs-empty table reproduces exactly.** No `layered_runtime` block →
   `{"entries":{},"layers":[]}` (33 bytes; also re-confirms the no-pre-population
   fact — zero built-in flags surface). `layered_runtime: {}` →
   `{"layers":[""],"entries":{}}` with stats `num_keys 0 / num_layers 1 /
   load_success 1 / override_dir_not_exists 1` and five zeros — the contract's table
   row and the `runtime_stats` test expectations are upstream-true, re-measured.
   `?format=text` → the same JSON 200, re-confirmed.

### §0.6 — Standing censuses re-derived at this commit

**87** fixture dirs (highest `0087`), **87** differential test files, **164** workspace
test binaries (CI census §0.3), **3** pre-existing `kind: admin_scrape` fixtures
(0011/0014/0015 — see N-2), `#![forbid(unsafe_code)]` intact in all three touched
crates, ADR head **ADR-0174** / next free **ADR-0175**, ROADMAP **110 rows / 108 done /
1 in-progress (parent `108`) / 1 planned (`108.2`)** — a state-5 commit flips no cell.

---

## §1 — Strengths

1. **The fixture's expected-entry table is derivably correct, twice over.** The
   reviewing dimension and the main session independently re-derived all **14**
   flattened entries from the two `layered_runtime` blocks (13 base leaves + 1
   override-only key) and matched `expectations.yaml` cell-for-cell — every
   `layer_values` slot, every `final_value`, every quoting decision. The two blocks are
   byte-identical across `envoy.yaml` and `envoy-rust.yaml`; the sole config divergence
   is the echo filter spelling (typed vs name-only), the documented fixture-0001
   precedent, justified in a comment at the site.

2. **The fixture's `num_keys: 14` is a real discriminator where the unit test's `4` is
   not.** The fixture's flattened-leaf count (14) differs from its top-level declared
   union (13), so a wrong counting rule REDs differentially — and the state-3 mutation
   evidence (`expected 13 got 14` on the flipped gauge) proves the assertion is live.
   This is what saves M-2 from mattering.

3. **The `empty.in.override` cell does the precedence work.** Both the in-process
   two-layer test (`endpoint.rs`, `runtime_renders_the_measured_two_layer_snapshot`)
   and the fixture carry the one cell where "last wins" and "last NON-EMPTY wins"
   disagree — the discriminating design the 108.1 review praised, carried through to
   the observer surface.

4. **The M-6 closure is genuine and complete.**
   `config_dump_serializes_layered_runtime_positively` (`endpoint.rs:3346-3370`)
   serializes a POPULATED block through the real `/config_dump` cascade, pins the
   float cell as a JSON **number** via `serde_json::Value` equality (which
   distinguishes `1.5` from `"1.5"` — so the recorded M-4 divergence is pinned as
   ours, not papered over), and pins the absent-direction elision that keeps the 86
   pre-existing `/config_dump` fixtures byte-identical. Exactly the test whose absence
   M-6 flagged.

5. **The M-5 route-around is verifiable by grep and holds.** Zero `from_layers` call
   sites outside `crates/envoy-config` — the three non-docs hits
   (`endpoint.rs:63`, `:3018`, `runtime_stats.rs:32`) are doc comments explaining the
   route-around itself. Both consumers call `RuntimeSnapshot::from_bootstrap`
   (`endpoint.rs:982`, `runtime_stats.rs:37`).

6. **The harness extension is additive-safe by construction and structurally guarded.**
   `expected_stats` carries `#[serde(default)]`; the dispatch destructure is
   exhaustive (no `..`), so a future field cannot be silently dropped; STEP 3.5 sits
   exactly between the scrape loop and `post_admin_assertions`
   (`lib.rs:6896-6905`), preserving the documented step ordering; and the vacuous-pass
   trap is restated at the field site, in the fixture, and in the README, with the
   four real witnesses enumerated in all three.

7. **The in-process stats tests are immune to the trap the harness carries.**
   `handle_for` (`runtime_stats.rs:87-94`) panics on an absent name rather than
   returning 0, and every lookup asserts the KIND with a panic on the wrong arm — so
   the five zero-valued stats' presence and kinds are genuinely pinned in-process,
   which is precisely the obligation the fixture's witness ledger delegates to them.

8. **Deliverables are complete and non-goals held.** All four D4 dispatch surfaces
   (both compile-forcing sites plus the two convention-test rows — the GET-count
   assertion correctly moved 7 → 8); the nine stat names byte-exact with measured
   kinds and values, registered unconditionally; the two-scrape fixture design exactly
   as specified (single-segment subtree anchors only, empty allow-lists); the contract
   section, admin row and stat-mapping block all landed at the promised locations.
   No `Cargo.toml`, no `ci.yml`, no fuzz change, no `envoy-config` edit, fixture
   `0011` untouched, and no forbidden content in the fixture (only float `1.5`, no
   unquoted YAML-1.1 booleans, no `numerator`, no `reloadable_features` prefix).

9. **The deviation ledger is complete — and errs only in the safe direction.** A
   line-level comparison of every landed code file against `PLAN.md`'s literals found
   nothing unrecorded: the rustfmt reflows, the Task-2 RED-shape correction and the
   Task-3 count-vs-total reading are all in `PROGRESS.md`. The one discrepancy runs
   the other way (N-6: a recorded reflow that corresponds to no landed deviation).

10. **The contract's `## Runtime` section is measurement-faithful on every cell this
    review could check against the record — except the one cell the record never
    measured** (M-1). The absent-vs-empty table, the float source-text rule, the
    stringification examples, the nondeterminism disposition, the 0011 prose
    correction, and the NOT-MEASURED list (M-1/M-7/N-5/N-4 from the 108.1 bank, as
    promised) all verify; §0.5's probes re-measured two of its rows upstream-true.

---

## §2 — Issues (Must Fix)

**None.**

No finding changes an accept/reject verdict, alters wire behaviour, introduces a
reachable panic or abort, weakens a fixture, or leaves a validator unwired.

---

## §3 — Minor

### M-1 — The contract asserts "POST → 405 `allow: GET`" bilaterally for `/runtime`; MEASURED FALSE on the upstream side — upstream serves 200

Sites: `BEHAVIOR_CONTRACT.md:1379` ("POST → 405 `allow: GET` bilaterally"),
`:3180-3181` ("GET-only (POST → 405 `allow: GET` on both sides)"),
`crates/envoy-admin/src/endpoint.rs:3318-3319` (test doc: "`/runtime` itself is
GET-only on BOTH sides"). Origin: `PLAN.md:417` / `:1247` (landed, uneditable) — no
measurement of `POST /runtime` exists in the SPEC, the PLAN, the parent SPEC, or any
ADR; the only method measurement on the surface is `GET /runtime_modify` → 405.

**Measured this review** (§0.5, pinned image, controls quoted there):

```
POST /runtime   → HTTP/1.1 200 OK, content-type: application/json,
                  body = the full runtime dump ({"entries":{},"layers":[]})
DELETE /runtime → 200 OK (same body)
POST /config_dump → 200 OK            (upstream restricts NO read endpoint)
GET /runtime_modify → 405 Method Not Allowed   (control: probe distinguishes)
```

envoy-rust's own behaviour is fine and deliberately pinned
(`runtime_post_is_method_not_allowed`: POST → 405 `allow: GET`), and is consistent
with the house method-strict dispatch every GET endpoint has had since 06.1/08. The
defect is the contract TEXT: a section whose preamble promises "Everything here is
MEASURED … unless marked otherwise" states an equivalence that does not hold. The true
state is a **reject-direction divergence**: envoy-rust 405s a wrong-method request
where upstream v1.33.0 serves it — and the §0.5 `POST /config_dump` control shows this
divergence is not `/runtime`-specific but tree-wide and PRE-EXISTING (unwitnessed by
any fixture, since every fixture speaks the matching method; not introduced by 108.2,
which merely wrote the first false claim about it).

**Why Minor and not an Issue:** the house Issue definition is mechanical (§2) and none
of its clauses is met — no fixture exercises a wrong-method admin request on either
side, so no verdict changes and gate (b) is untouched. The direct precedents are 76.2
M-2 (a contract cell claiming coverage that measurement refuted — graded Minor) and
108.1 M-7 (an unmeasured cell asserted under a MEASURED citation — graded Minor); this
finding is the same class with the measurement now done and embedded. The honest fix —
for the next session that legitimately touches the contract or the runtime surface —
is to reword the two contract lines and the test doc comment to state the measured
truth (envoy-rust 405 pinned; upstream 200, method-unrestricted; recorded
reject-direction divergence), NOT to change envoy-rust's dispatch (the 405 is a
deliberate, RFC-clean house convention a later phase may revisit with upstream parity
in scope). Severity dissent recorded in §7.

**Reached independently by review dimension 4 (as "unmeasured") and by the main
session (as "measured false").**

### M-2 — `runtime_stats.rs`'s discrimination claim for its own test is false as written: the `4` cannot distinguish flattened-leaf counting from top-level counting

`crates/envoy-bin/src/runtime_stats.rs:96-100` (test doc: "`nested.deep` proves
`num_keys` counts FLATTENED LEAVES: 3 base leaves + 1 override-only key = 4").

The claim does not hold: `TWO_LAYER_YAML`'s nested map has a SINGLE leaf, so the
top-level declared union (`shared.key`, `only.in.base`, `nested`, `only.in.override`)
is **also 4**. A non-recursing implementation that kept `nested` as one entry would
pass this test at `num_keys == 4`. The property IS covered elsewhere — 108.1's
`flatten_layer` tests assert the dotted keys directly, and fixture `0087`'s
`num_keys: 14` differs from its top-level union of 13, with the state-3 mutation
evidence proving that assertion live (§1 item 2) — so no coverage gap exists at the
phase level; the defect is a false statement of a test's discriminating power at the
code site, the exact axis the 108.1 review praised the sibling for recording honestly
(its §1 item 2). A two-leaf nested map (making 5 ≠ 4) would make the claim true at the
cost of one line, for whichever session next touches the file.

**Reached independently by review dimension 2 and by the main session.**

---

## §4 — Nit

**N-1** — `README.md:34` and `expectations.yaml:104` cite the vacuous-pass rule at
"lib.rs:4504-4507"; the sentences now sit at **lib.rs:4518-4521**, shifted by the
14-line `expected_stats` field doc the SAME commit (`22df3a1`) added above them — a
commit stale-dating its own citation. The CLAIM is byte-true at the new location; a
drifted citation is a Nit (the 108.1 N-10 precedent, same mechanism). `PLAN.md:1076` /
`:1366` carry the same numbers (landed, uneditable).

**N-2** — `PLAN.md:200` (DD-4) says the `#[serde(default)]` keeps "all six existing
AdminScrape fixtures" parsing unchanged; the tracked census is **3** (0011, 0014,
0015 — likely conflated with SPEC §4's "seven fixtures touch the admin listener via
scrape machinery", which counts other driver kinds). The additive-safety property
holds identically at 3; recorded because an inherited census is an inherited census.

**N-3** — `README.md:35-36`'s "14 (flattened LEAVES: 13 declared + 1 override-only —
NOT the 13 top-level declared keys)" uses two DIFFERENT quantities that coincide at
13 (base-layer flattened leaves; distinct declared names across both layers). The
arithmetic is right; the prose invites a wrong equation. Inherited verbatim from
`PLAN.md:87`.

**N-4** — `BEHAVIOR_CONTRACT.md:3217-3218` attributes the quote "RTDS runtime layer.
Deferred to the xDS family." to both `0011` files; `expectations.yaml:35` actually
reads "(defers to xDS family)" — `README.md:55` has the quoted wording. Substance
identical; conflation inherited from SPEC §3.

**N-5** — The `entries` key order ("BTreeMap-canonical") is a code property with no
test: `serde_json::Value` object equality is order-insensitive, so
`runtime_renders_the_measured_two_layer_snapshot` cannot pin it. Deliberate and
honestly documented at `endpoint.rs:977-979` (the differential comparison is
order-insensitive by design); `layers`, where order IS semantic, is an array and is
order-asserted. Recorded for completeness.

**N-6** — `PROGRESS.md:115-118` records a Task-4 rustfmt reflow of the
`run_fixture(...)` chain in `runtime_static_layer.rs`, but the landed file is
byte-identical to the PLAN's literal (already split); the recorded deviation
corresponds to no landed difference. The ledger overstates in the safe direction.

---

## §5 — Deliberate decisions verified, and compositions probed clean — not filed

1. **DD-2 / M-5**: verified by main-session grep — zero `from_layers` call sites
   outside the store (§1 item 5).
2. **DD-3 (the echo listener)**: both YAMLs carry the `{{PORT}}` listener and
   `{{ADMIN_PORT}}` admin block; the harness's unconditional accept-ready wait and
   `driver_needs_admin_port` arms confirm both are required. The name-only envoy-rust
   spelling matches the fixture-0001 precedent exactly.
3. **DD-5 / M-6**: the positive pin is real and two-directional (§1 item 4).
4. **DD-7**: the `#[allow(clippy::too_many_arguments)]` sits on the widened arm with
   its justification comment, per the `AdminHandler::new` precedent.
5. **DD-8**: both subtree anchors are single-segment (`entries`, `layers`); no path
   points into a dotted key.
6. **The absent-vs-empty table and the no-pre-population fact re-measured
   upstream-TRUE** (§0.5 probe 2), including all nine stat values on the empty-block
   spelling. `?format=text` ignore parity re-confirmed.
7. **No new panic path**: `json_pretty_200`'s serialization expect is unreachable for
   `RuntimeBody` (derive-only over String/Vec/BTreeMap); the gauge sets saturate via
   `unwrap_or(i64::MAX)`; `register_runtime_stats` propagates errors and `main.rs`
   contexts them. The one `ConfigError` catch-all `unreachable!()`
   (`rds_watcher.rs`) is untouched by a phase that adds no `ConfigError` variant.
8. **Convention-test coverage moved with the surface**: `get_known_path_returns_endpoint`
   and `each_endpoint_declares_its_allowed_method` both gained the `/runtime` row;
   the POST-side test is untouched at exactly 3, as the SPEC required.

---

## §6 — Status of already-banked findings — read BEFORE grading, and NOT re-issued

All four prior reviews were read before any grading (`76.1`, `76.2` round 1 —
CHANGES-REQUESTED, frozen — `76.2` round 2, `108.1`). No banked item is re-issued.
The `108.1` bank (M-1..M-7, N-1..N-12) is this phase's direct upstream; disposition
status, verified on disk:

| banked | 108.2 disposition | verified |
|---|---|---|
| M-1 (flattened-key collision) | NOT-MEASURED list, contract + README | ✓ both sites |
| M-2 (int-outside-i64 doc claim) | out of scope (108.1 code untouched) | ✓ untouched |
| M-3 (non-finite floats) | recorded in the contract's divergence paragraph; kept out of the fixture | ✓ `BEHAVIOR_CONTRACT.md:3231-3234` |
| M-4 (`/config_dump` float shape) | measured (ADR-0174 axis ii), recorded, OUR shape pinned by the M-6 test | ✓ §1 item 4 |
| M-5 (`from_layers` invariant) | routed around (DD-2), zero call sites | ✓ §1 item 5 |
| M-6 (no positive Serialize pin) | **CLOSED** by `config_dump_serializes_layered_runtime_positively` | ✓ §1 item 4 |
| M-7 (empty nested map) | NOT-MEASURED list | ✓ |
| N-4 / N-5 | NOT-MEASURED list | ✓ |
| N-10 (citation drift) | consumed as a lesson; anchors re-derived by text at every task | ✓ (and recurs as this review's N-1, a NEW instance on NEW text) |
| others (N-1..N-3, N-6..N-9, N-11, N-12) | banked, surfaces untouched | ✓ |

The five 108.1 REVIEW §8 obligations: ob.1 (CF-108-5 measured before any float) —
DISCHARGED, ADR-0174; ob.2 (no YAML-1.1 booleans in `0087`) — held; ob.3 (M-1/M-7/
N-5/N-4 into the NOT-MEASURED list) — done in contract and README; ob.4 (M-5
disposition) — DD-2; ob.5 (M-6 pin) — landed. **CF-108-5 CLOSED** (ADR-0174);
**CF-108-1/2/3 pass through OPEN**; CF-108-4 remains closed-as-recorded (ADR-0173).

---

## §7 — Severity dissent, recorded rather than silently resolved

1. **M-1 — Minor vs Issue.** The case for Issue: `BEHAVIOR_CONTRACT.md` is the
   canonical equivalence reference (D-3.3: "the contract is the contract"), and this
   review MEASURED one of its cells false — leaving it banked leaves the canonical
   document actively wrong, which is qualitatively worse than the unmeasured-cell
   precedents. The case for Minor, which carried: the Issue definition is mechanical
   and unmet (no fixture, no verdict, no wire change — the false cell describes a
   composition nothing exercises); the direct precedents (76.2 M-2, 108.1 M-4/M-7)
   all graded doc-accuracy defects Minor even when refuted; the underlying behaviour
   divergence is pre-existing and tree-wide, so a re-entry that "fixed" only
   `/runtime`'s text would misrepresent the scope anyway; and the corrected fact is
   now fully measured and embedded here, making the eventual fix free. **Graded
   Minor. If a future phase puts wrong-method admin behaviour on a differential wire,
   or the contract section is next edited, this must be consumed first.**
2. **M-2 — Minor vs Nit.** The case for Nit: coverage exists at the phase level and
   the defect is one doc sentence. The case for Minor, which carried: the sentence
   claims a discriminating power the test does not have, on the exact surface
   (`num_keys` semantics) the fixture exists to witness — the 76.2 "check the
   constructor, not the type" class, where a false coverage claim is worse than
   silence. Both routes that found it graded it Minor independently.
3. **N-1 vs Minor.** Dimension 3 argued Minor because the commit invalidated its own
   citation (not later drift). The 108.1 N-10 precedent is directly on point — six
   citations drifted by the phase's own insertions, graded Nit because the CLAIM
   held — and controls here: Nit.

---

## §8 — Carry-forwards for the state-6 close-out to bank

- **M-1's measured facts, stated for reuse:** upstream v1.33.0 serves its read-only
  admin endpoints on ANY method (`POST /runtime` → 200, `DELETE /runtime` → 200,
  `POST /config_dump` → 200; `GET /runtime_modify` → 405 remains true); envoy-rust
  405s every wrong-method request by design (06.1/08 house convention). A
  reject-direction divergence, tree-wide, unwitnessed by any fixture, now measured
  and banked here. The two contract lines (`:1379`, `:3180-3181`) and one test doc
  (`endpoint.rs:3318-3319`) state the opposite and should be reworded by the next
  session that legitimately touches them.
- **M-2**: one-line fix (a second nested leaf) whenever `runtime_stats.rs` is next
  edited.
- **N-1..N-6** bank as recorded; none blocks anything.
- **CF-108-1, CF-108-2, CF-108-3 remain OPEN** for the future consumer/RTDS slice;
  CF-108-4/5 stay CLOSED-as-recorded. **CF-76-1, CF-75-2/3/4/5/6** and the older
  banks pass through untouched (§6.3).
- **Method notes worth carrying:** a retyped 40-char SHA that silently lost a
  character returns `[]` exactly like a short SHA — re-derive, never retype; and
  `grep -c '#\[test\]'` under-counts `#[tokio::test]` — close identities by
  enumeration.

---

## §9 — Assessment

`108.2` is a disciplined observer slice: it renders a reviewed store through one
documented entry point, pins every silent-failure cell the record identified
(`empty.in.override`, the absent-vs-empty three-way, the M-6 serialization seam), and
lands a fixture whose every expected cell this review re-derived independently and
whose decisive stat witness (`14` vs `13`) genuinely discriminates. All five 108.1
obligations were discharged or correctly routed around, and the deviation ledger
proved complete under a line-level diff.

The two Minors share a single shape, and it is the inverse of 108.1's: where the
sibling's findings were questions the phase did not ask, both of this phase's are
**answers stated more confidently than the record supports** — a bilateral claim
asserted where only one side was measured (M-1), and a discriminating power claimed
that the test does not possess (M-2). Both were caught by measuring rather than
reading; neither changes what the code does; both are now measured, recorded, and
free to fix at next contact.

**Gate (f) is CLOSED. All six §7.5 gates are now GREEN. Sub-phase `108.2` is approved
to land.**

**Next state: §5 state 6 — the close-out** (ROADMAP rows `108.2` AND parent `108`
flip `done` TOGETHER, the `76`/`76.2` precedent — status cells only, no ADR, no Notes
subsection), a **separate session** per §5.1 and ADR-0127 — a reviewer must not close
out what it graded. This review **fixed nothing**, as ADR-0165 requires.
