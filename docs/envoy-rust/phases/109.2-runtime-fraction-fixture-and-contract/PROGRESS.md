# Sub-phase 109.2 — PROGRESS (§5 state-3 implementation)

> Running log, appended per PLAN task (BOOTSTRAP_PROMPT.md §5 state 3). The
> execution authority is `PLAN.md` (5 tasks); the design authority `SPEC.md`.
> Session started at `e458765` (clean tree, level with `origin/main`; detection
> rule re-verified on disk: `109.2/` held `SPEC.md` + `PLAN.md` only and NO
> `PROGRESS.md`; ROADMAP **113** rows / **111** `done` / **1** `in-progress`
> (parent `109`) / **1** `planned` (`109.2`); ADR head **ADR-0176**, next free
> **ADR-0177** UNRESERVED; `STATE.md` **199** lines with the Standing-traps line
> MEASURED **125473** characters; `STATE_HISTORY.md` **15843** lines;
> `runtime.rs` **888** lines; `BEHAVIOR_CONTRACT.md` **3927** lines).
>
> **This plan is UNEDITABLE once execution started (D-3.5). Every deviation is
> RECORDED here instead**, and the ledger's completeness is itself reviewed at
> state 5 — an unrecorded deviation is a finding.

## Task 1 — the three banked cascade-guard witness rows in `runtime.rs` ✅

- **Site located by TEXT, not by the inherited number.** The PLAN cites the
  anchor row `"edge: empty runtime_key is not consulted -> default 0 -> Never"`
  at `:752-756`; MEASURED on disk at `:751-756` (label line `:752`) — the
  anchor text resolved to exactly one site, so the drift is cosmetic. The
  guards the three rows pin were re-read on disk and match the PLAN's mutation
  table exactly: `route_fraction_gate` at `:163-211`, the empty-key filter at
  `:167`, `&& v.is_finite()` at `:181`, `p.numerator == p.denominator.value()`
  at `:203`.
- **Edit:** three tuples appended INSIDE the existing `ok_cells` `vec!` in the
  existing test fn `route_fraction_gate_pins_every_measured_cell`, immediately
  after the anchor row and before the closing `];`. **No new test function and
  no new public item** — the workspace test COUNT does not move (measured
  below).
- **`cargo fmt --all` then `--check`:** exit 0, silent.
  `git diff --numstat crates/envoy-config/src/runtime.rs` → **`22 0`**, exactly
  the PLAN's MEASURED figure (and 3.7× the 109.1 REVIEW §8 estimate of "~6
  lines" — the fourth confirmation of the LoC-calibration memory, and the first
  on a test-DATA patch). File **888 → 910** lines.
- **GREEN (characterization pins pass on arrival):**
  `cargo test -p envoy-config --lib route_fraction_gate_pins_every_measured_cell`
  → `test runtime::tests::route_fraction_gate_pins_every_measured_cell ... ok`,
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 708 filtered out`.
  **`1 passed` asserted literally, never the exit code.** The filter was used
  WITHOUT `-- --exact` per the PLAN's Global Constraint — the plan-write
  pre-flight measured `--exact` on the short name returning the false green
  `ok. 0 passed; 709 filtered out`, exit 0. The `708 filtered out + 1 passed =
  709` identity is unmoved from the plan-write's 709, which is the mechanical
  confirmation that this task added no test function.

### TDD RED — the three mutation checks (scratch worktree, unmutated control first)

| # | Mutation | Guard removed | Result | `Compiling envoy-config` |
|---|---|---|---|---|
| — | *(unmutated control)* | — | `ok. 1 passed; 0 failed` | 1 |
| M1 | `rf.runtime_key.as_deref().filter(\|k\| !k.is_empty())` → `rf.runtime_key.as_deref()` | the empty-key filter (`:167`) | `FAILED. 0 passed; 1 failed`, panic naming **`M-1: empty runtime_key is NOT consulted — a `.`-prefixed snapshot entry discriminates …`** | 1 |
| M2 | delete the line `                && v.is_finite()` (`:181`) | the non-finite guard | `FAILED. 0 passed; 1 failed`, panic naming **`M-2: `inf` paired with default 0 — the non-masking direction of the is_finite guard`** | 1 |
| M3 | `p.numerator == p.denominator.value()` → `p.numerator == 100` (`:203`) | the denominator consultation | `FAILED. 0 passed; 1 failed`, panic naming **`M-3: default 1_000_000/MILLION with no key pins the denominator.value() consultation`** | 1 |

Every run printed a real `test result:` line (an exit code with no such line is
a compile error and proves nothing), and every run showed a **non-zero
`Compiling envoy-config` count** (a stale test binary is a FALSE PASS). Each
mutation was applied ONE AT A TIME by a script that asserts the target string
occurs **exactly once** before replacing it, and the file was restored from a
pre-mutation backup (`/tmp/rt.bak`) between mutations. Each panic named its OWN
row — the rows are discriminating, not merely present.

**Post-run:** `git worktree remove --force` succeeded; `git worktree list` shows
the scratch tree gone (the four `.claude/worktrees/agent-*` siblings are a
concurrent workstream's and were LEFT ALONE); `git status --porcelain` in the
main tree shows exactly `M crates/envoy-config/src/runtime.rs`; and the main
tree's `runtime.rs` md5 (`61b18068d2af02171a12a3a35a028313`) is byte-identical
to the pre-mutation backup — the main tree was never mutated.

### DEVIATION 1 (RECORDED) — the scratch worktree was SEEDED, not created at bare `HEAD`

PLAN Task 1 Step 4 prescribes `git worktree add --detach /tmp/109_2-wt HEAD`
and then running the control and the three mutations there. Taken literally at
that point in the task order, **the worktree would predate this task's own
rows**: Step 4 runs BEFORE Step 5's commit, so `HEAD` is still `e458765` and
the worktree's `ok_cells` table is the pre-109.2 one. The M1 mutation against
that table is precisely the case the 109.1 REVIEW M-1 finding says survives
undetected — it would have returned a GREEN and been read as "the mutation is
misaimed" when in fact the rows under test were absent.

**What was done instead:** the worktree was created `--detach` at `HEAD` as
written, and then this task's `crates/envoy-config/src/runtime.rs` was COPIED
into it (`git diff --numstat` inside the worktree confirmed `22 0`, and md5
equality with the main tree's file was asserted). The main tree was still never
mutated — the standing constraint the step exists to enforce — and the control
+ three mutations ran against exactly the bytes this task lands. The
alternative (commit first, then create the worktree at the new commit, per
memory `mutation-checks-collide-with-parallel-subagents`) would have been
equally sound but splits Task 1 across two commits.

### Task 1 gate

- `cargo build --workspace --all-targets` → exit 0, **13** `Compiling` lines
  (non-zero: not a cached no-op).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit
  0, **13** `Checking` lines (non-zero: not a cached no-op — the build cache and
  the clippy cache are independent and were measured BOTH cold-no-op at once at
  109.1 state-4, so both counts are gated separately).
- `cargo fmt --all -- --check` → exit 0, silent.

---

## Task 2 — fixture `0088-runtime-fraction-route-gating` + its differential entrypoint ✅

- **TDD RED first.** `tests/differential/tests/runtime_fraction_route_gating.rs` was written
  BEFORE any fixture file existed. `cargo test -p differential --test runtime_fraction_route_gating`
  → exit 101 with a real `test result: FAILED. 0 passed; 1 failed` line and the panic naming the
  missing file: `fixture green: reading …/tests/fixtures/0088-runtime-fraction-route-gating/expectations.yaml`.
  (A compile error would NOT have been a RED — the `test result:` line's existence was asserted.)
- **Harness facts re-verified on disk before transcribing** (a pre-flight is a CLAIM, never an
  inheritance): `driver_needs_admin_port` (`tests/differential/src/lib.rs:3066-3074`) matches only
  `AdminScrape`/`Http1KeepAlive`/`Http2KeepAlive`/`TcpWithStats` — **`Http1ProbeList` is absent**,
  confirming the PLAN's X-1 refutation of the SPEC's `{{ADMIN_PORT}}` spelling; `Http1ProbeList`
  IS listed in `port_key_for` (`:3011`). Fixture census RE-DERIVED: **87** dirs
  (`git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l`), **87** differential test
  files, highest `0087-runtime-static-layer` — so `0088` was still the next free number.
  `docker image inspect` confirmed the local `envoyproxy/envoy:v1.33.0` digest is
  `sha256:56da5afd7df364350ff92de4fb49a9b09957c17295f2899f0a31cd12c28770c2`, byte-matching the
  `ENVOY_TARGET.md` pin.
- **Both YAMLs and `expectations.yaml` were EXTRACTED PROGRAMMATICALLY from `PLAN.md`'s fenced
  blocks** rather than retyped, so transcription fidelity is structural rather than hopeful.
  `envoy.yaml` = **126** lines (exactly the PLAN's MEASURED figure), `expectations.yaml` = 124
  lines. `expectations.yaml` was PARSE-CHECKED with a YAML loader before any run (the 109.1 lesson
  that a plan's YAML can omit a required field and land twice): 10 probes, `kind: http1_probe_list`,
  every `path`/`expected_body`/`expected_status` matching the SPEC §1 table row-for-row.
- **BYTE-IDENTICAL configs proved, not asserted:** `cmp` silent, both **126** lines, both md5
  `d205936b0390260855f19258dd02f51a`.
- **GREEN cross-proxy on first contact:** `test result: ok. 1 passed; 0 failed`, **1.28 s** cold in
  this session's already-warm image cache, **1.02-1.07 s** warm across four further runs.

### The byte-identical-pair census, RE-DERIVED (never inherited)

Of the **87** pre-existing fixture pairs carrying both YAMLs, exactly **ONE** is byte-identical:
`0027-xds-file-based-lds`. So `0088` is the **SECOND**, as the PLAN predicted — but the number was
re-measured by `cmp`-ing all 87 pairs, not taken from the PLAN. `tests/fixtures/` now holds **88**
directories.

### FINDING — a suspiciously-fast green was audited, and THE PROBE WAS THE DEFECT

A ~1 s cross-proxy green invites the "silent skip" suspicion, so `docker ps` was polled during a
re-run. **The first poll reported ZERO containers** — which looked exactly like a fixture going
green without ever starting upstream Envoy. It was not: the poll used
`docker ps --format '{{.ID}} {{.ImageID}} …'`, and **`.ImageID` is not a valid `docker ps` field**,
so every one of the 40 poll lines was `failed to execute template: … can't evaluate field ImageID`
— the probe produced NOTHING and its emptiness read as a clean census. This is the
`uniform-md5-can-be-the-EMPTY-file-md5` class in a new guise: **a probe that fails to execute
returns a believable zero.**

Re-run with a valid format, the upstream container is plainly there — **7 sightings** of
`e3a0fb318032 envoyproxy/envoy:v1.33.0 Up Less than a second 0.0.0.0:55002->10000/tcp` — port-mapped
(`-p`, never `--network host`) and resolved by container/image ID rather than by the first matching
line. The ~1 s warm figure is consistent with the 108.2 record for `0087` (7.88 s cold /
1.15-1.24 s warm). **The green is real; the alarm was an artifact of the instrument.**

### Step 7 — the two vacuity mutations

| # | mutation | measured result |
|---|---|---|
| V1 | `override_layer`'s `gate.layered: 0` → `100`, in BOTH yamls (targeted as the file's LAST non-empty line, asserted equal to the expected text before rewriting) | **RED** — `probe p8-two-layer-last-wins: upstream body != expected`, `expected: [67, 65, 84, 67, 72]` (= `CATCH`). The fixture witnesses last-layer-wins precedence, not merely the base layer. p8 is the 8th probe and the driver aborts at the FIRST failure, so p1-p7 necessarily passed under the mutation. |
| V2a | p9's `denominator: MILLION` → `HUNDRED` | **GREEN**, exactly as the PLAN predicted — the cell stops discriminating the denominator reading while the probe still passes (the consulted `100` gates either way). Recorded because a GREEN mutation is a finding, not a non-event. |
| V2b | p9's `runtime_key` → the ABSENT key `gate.absent.p9`, `MILLION` default restored, so the default decides | **RED** — `probe p9-integer-is-numerator-over-hundred`, `expected: [80, 57, 45, 71, 65, 84, 69, 68]` (= `P9-GATED`). Proves p9's witness comes from the CONSULTED value, not from the default. |

### DEVIATION 2 (RECORDED) — the PLAN's revert check is a FALSE CLEAN on an uncommitted fixture

PLAN Task 2 Step 7 prescribes reverting each mutation with
`git checkout -- tests/fixtures/0088-runtime-fraction-route-gating/ && git diff --stat` "must be
EMPTY". **At this point in the task order the fixture is still UNTRACKED** (it is created and
committed in this same task), so `git checkout --` is a **NO-OP** — it silently leaves the mutated
bytes in place — and `git diff --stat` is empty **for the wrong reason**: untracked files never
appear in `git diff` at all. Measured live: after V1 the checkout ran, `git status --porcelain`
still showed only `?? tests/fixtures/0088-runtime-fraction-route-gating/`, and the yamls' md5 was
still the MUTATED `ddcc8e79f8ec612b8a2227960c82167c`.

**What was done instead:** each mutation was reverted by RE-EXTRACTING the yaml from `PLAN.md`'s
fenced block — the same byte source the files were created from — and the revert was adjudicated
by **md5 equality with the pre-mutation value** (`d205936b0390260855f19258dd02f51a`), plus a fresh
`cmp` byte-identity check and a re-run to GREEN. This is strictly stronger than the prescribed
check and does not depend on the file's tracked/untracked state. The same trap applies to any
future fixture-adding task that mutates its own fixture before committing it.

### Task 2 gate

- `cargo build --workspace --all-targets` → exit 0, **1** `Compiling` line (non-zero; the dirty set
  is exactly the new differential test target).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → exit 0, **1** `Checking`
  line (non-zero — a `Checking` count measures the cache's dirty set, and 1 is the correct dirty set
  here, not a cached no-op which would be 0).
- `cargo fmt --all -- --check` → exit 0, silent.
- `cargo test -p differential --test runtime_fraction_route_gating` → `test result: ok. 1 passed; 0 failed`.
- `tests/differential/src/lib.rs` was **NOT** modified — this fixture needs zero harness change, as
  PLAN-VERIFY X-1 predicted and as the untouched `git status` confirms.

---

## Task 3 — the `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection ✅

- **Section boundaries RE-MEASURED, not inherited:** `## Runtime` at `:3162`, next `## ` heading
  `## xDS wire state machine` at `:3241` (the PLAN's figures, unmoved — Tasks 1 and 2 touched no
  doc). The section carries **ZERO `### ` subheadings** — confirmed by reading it, it is organised
  entirely as bold-lead paragraphs — so the new material uses a bold lead-in and adds no heading.
- **Insertion point located by TEXT:** immediately BEFORE the paragraph
  ``**The nine `runtime.*` stats** — registered unconditionally on both sides`` (whole-file count
  of that anchor = **1**), i.e. directly after the `` **`GET /runtime`** `` shape table. The
  insertion script asserted the anchor's uniqueness AND asserted that the block itself contains no
  line starting `## ` or `### ` before writing.
- **Transcribed from the SOURCES, per X-5 — not from `109.2/SPEC.md`'s summary:**
  `docs/envoy-rust/phases/109-runtime-fraction-route-gating/SPEC.md` §1.1 (the **13**-cell pick
  matrix, 30 probes each, cells 1/3/9/13 re-run 40/40 at the split) and
  `docs/envoy-rust/phases/109.1-runtime-fraction-config-and-gate/SPEC.md` §1.2 (the **10**-cell V-8
  closure matrix, 40 probes each) + §1.3 (the evaluation cascade). **13 + 10 = 23**, matching
  `route_fraction_gate`'s own doc comment. The probe counts are carried verbatim, including
  cell 5's `GATED 27 / FALLBACK 33 over n=60` and cell F4's `GATED 1/40`.
- **The `edge:` rows were deliberately EXCLUDED and the exclusion is stated in the text.** The
  landed unit table pins MORE than 23 tuples; the extras are SPEC §1.3-DERIVED, upstream-unmeasured,
  and claiming them as measured would be precisely the "a doc claim is an inherited census" failure
  this project keeps re-learning. The subsection says so explicitly so a later reader cannot
  re-inflate the number.
- **Content:** the five items the PLAN enumerates, in order — the bold lead-in naming the consumer
  and its process-lifetime-constant property; the 23-cell table (verified: exactly **23** matching
  rows by an id-anchored count); the §1.3 cascade with both load-bearing readings called out
  (numerator over HUNDRED not over the default's denominator; unparseable → default in BOTH
  directions); CF-109-1 (WIDENED) / CF-109-2 (the SNAPSHOT-PREFIX rule) / CF-109-3 each with its
  **unblock condition**; and the fixture-`0088` pointer naming which cells it pins and why the
  nondeterministic and reject-direction cells can never appear in a fixture.
- **Step 2 structural verification (all three assertions PASS):** `## Runtime` still immediately
  precedes `## xDS wire state machine` (now at `:3354`); the file's `## ` count is UNCHANGED at
  **15** and its `### ` count UNCHANGED at **24**; `git diff --numstat` = **`113 0`** — insertions
  only, **zero deletions**, so no existing contract text was overwritten. File **3927 → 4040** lines.
- **LoC note:** `113` actual against the PLAN's ≈**80** estimate (**+41%**), consistent with the
  standing calibration (76.1 +50%, 109.1 +46%). The overrun is entirely in the 23-row table, whose
  `reading` column carries the verbatim source phrasing rather than a compressed paraphrase.

### NOTE — the handoff's "T1/T3/T4 touch DISJOINT files" claim is FALSE for T3/T4

`next-prompt.txt`'s parallelism block states that T1, T3 and T4 "touch DISJOINT files and can run as
parallel implementation subagents in their OWN worktrees". **T3 and T4 both edit
`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** (T3 inserts the consumer subsection; T4 rewrites two
sentences in the same file, one of them at `:3181-3182`, INSIDE the same `## Runtime` section T3
just extended). Running them as concurrent worktree subagents would have produced two divergent
copies of that file and a merge the main session would have had to arbitrate by hand. They were
therefore executed SEQUENTIALLY in the main session, T3 first — which also means T4's line-number
anchors have drifted by T3's +113 lines and MUST be re-located by text (they were).

---

## Task 4 — the decided-in 108.2-M-1 correction (three texts) + the stale `RuntimeFractionalPercent` doc ✅

All four sites were located BY TEXT, each resolving to a whole-file count of exactly **1** before
any edit. Two of the PLAN's four line-number citations had drifted (Task 3's +113 lines moved
nothing above `:3197`, but the citations were re-derived rather than trusted):

| # | site | PLAN's line | MEASURED line | anchor text (count) |
|---|---|---|---|---|
| 1 | `BEHAVIOR_CONTRACT.md` `## Admin endpoint body shapes` `/runtime` row | `:1379` | `:1379` | ``allow: GET` bilaterally`` (1) |
| 2 | `BEHAVIOR_CONTRACT.md` `## Runtime` `GET /runtime` paragraph | `:3181-3182` | `:3181` | `GET-only (POST` (1) |
| 3 | `crates/envoy-admin/src/endpoint.rs` test doc | `:3319-3320` | `:3319` | `GET-only on BOTH` (1) |
| 4 | `crates/envoy-config/src/bootstrap.rs` `RuntimeFractionalPercent` doc | `:1497-1501` | `:1500` | ``a present `runtime_key` is rejected`` (1) |

- **Sites 1-3** now record the TRUE **asymmetry** rather than a bilateral rule: envoy-rust answers
  non-GET with 405 `allow: GET` (the deliberate 06.1/08 house convention), while upstream v1.33.0
  serves `POST /runtime` and `DELETE /runtime` with **200 and the full body**, method-restricting
  NO read-only admin endpoint (MEASURED at the 108.2 state-5 review; the discriminating control
  `GET /runtime_modify` → 405 reproduces). Each states that the divergence is reject-direction,
  tree-wide, PRE-EXISTING and fixture-unwitnessed — every fixture speaks the matching method, so
  nothing goes red.
- **Site 3's test body and assertions were NOT touched** — they pin envoy-rust's own dispatch and
  are correct. Only the `///` doc above them was reworded, to say what the test actually proves.
  `cargo test -p envoy-admin --lib runtime_post_is_method_not_allowed` → `test result: ok. 1 passed;
  0 failed` (a doc-only edit changes no behaviour).
- **Site 4 (the flagged ADDITION to SPEC D4)** narrows the stale claim rather than deleting it: the
  doc now says the two consumers DIFFER — CSRF still rejects a present `runtime_key` (ADR-0061 L6),
  the ROUTE consumer HONORS it under the deterministic 109.1 cascade. The CSRF validator, the test
  `runtime_key_is_rtds_inert` and every other consumer are untouched.
- **The near-miss was NOT edited.** The `/runtime_modify` sentence (CF-108-2: upstream POST-only,
  405 on GET, envoy-rust 404s it) describes a DIFFERENT endpoint, is correct, and is explicitly
  non-bilateral. Proved byte-identical, not merely eyeballed (below).
- **Step 5 old-wording sweep:**
  `git grep -n 'GET-only on BOTH\|GET-only (POST\|allow: GET` bilaterally' -- docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/`
  → **ZERO hits** (exit 1).
- **Gate:** `cargo build --workspace --all-targets` exit 0 / **13** `Compiling`;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` exit 0 / **13** `Checking`;
  `cargo fmt --all -- --check` exit 0. `git diff --numstat`: `endpoint.rs` `8 2`,
  `bootstrap.rs` `10 3`, `BEHAVIOR_CONTRACT.md` `9 4` — exactly the four named sites in three files.

### DEVIATION 3 (RECORDED) — the PLAN's `runtime_modify` count check moved 3 → 4, and the count is the WRONG instrument

PLAN Task 4 Step 5 expects "the `/runtime_modify` mentions are unchanged in count". **MEASURED
3 → 4.** This is not a violation of the property the check guards, and the check as written cannot
distinguish the two cases — it is the standing "a grep can legitimately return >0 because a record
QUOTES the thing it supersedes; adjudicate by LINE and by FILE, never by COUNT" trap, firing on the
PLAN's own recipe. The PLAN is in fact self-inconsistent here: Step 1 explicitly requires citing
"control `GET /runtime_modify` → 405" as the measurement's discriminator, which ADDS a mention,
while Step 5 expects the count not to move.

**Adjudicated by line instead**, with the four current mentions enumerated:

- `:1379` — the corrected row. It already carried a `/runtime_modify` clause; **contribution
  unchanged**.
- `:3186` — **the new one**, my Task-4 text naming the discriminating control. This is the entire
  delta.
- `:3210` — inside Task 3's own block ("no RTDS, no `/runtime_modify`, no disk layer").
- `:3351` — the CF-108-2 paragraph, the one that must not move.

And proved mechanically rather than by reading `+`/`-` diff lines (which the
`fmt-only-check-needs-a-whole-file-compare` lesson says false-positive): comparing the old and new
files in Python, (a) the `:1379` row's PREFIX before the corrected sentence is **byte-identical**,
(b) its trailing CF-108-2 clause is **byte-identical**, (c) the false sentence
``POST → 405 `allow: GET` bilaterally.`` is present in the OLD row and **absent** from the new one,
and (d) the standalone CF-108-2 paragraph line is **byte-identical** (single match, equal strings).
Only the false sentence changed.
