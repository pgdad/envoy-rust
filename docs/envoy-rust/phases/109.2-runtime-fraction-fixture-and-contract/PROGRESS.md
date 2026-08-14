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

---

## Task 5 — state-3 exit gate ✅

**This is the state-3 EXIT BAR, not the §7.5 adjudication.** State 4 owns the formal gate in a
SEPARATE session (§5.1; ADR-0127) and must RE-RUN every command below rather than inherit these
numbers.

### Step 1 — the full gate set, run from the REPO ROOT

| command | result |
|---|---|
| `cargo build --workspace --all-targets` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0, silent |
| `cargo test --workspace --no-fail-fast` (×2) | see below |
| `cargo deny check` | exit 0, `advisories ok, bans ok, licenses ok, sources ok` |

`cargo deny` was run from the repo root (it does NOT walk up — a `docs/…` cwd errors with
`the directory … doesn't contain a Cargo.toml file` while build/test/fmt/clippy all walk up
happily). It was gated on the exit code PLUS the four-ok line, not on a loose `warning` grep, which
false-positives on its `license-not-encountered` allowance warnings. Per-task boundaries had already
gated build/clippy on non-zero `Compiling`/`Checking` counts (13/13 at T1 and T4, 1/1 at T2 — a
`Checking` count measures the cache's DIRTY SET, so 1 is correct there and only 0 would be a
cached no-op).

### Step 2 — the sweep, run TWICE, failing SET diffed, classified BY ISOLATION ONLY

Census recipe `grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed'` + awk fields
**4**/**6**, with the binary count asserted separately (the `ok`-only form makes `failed=0`
tautological, and fields `$5`/`$7` return a believable `passed=0`):

| sweep | binaries | passed | failed | passed+failed |
|---|---|---|---|---|
| 1 | **165** | **2187** | **7** | **2194** |
| 2 | **165** | **2188** | **6** | **2194** |

**The identity closes exactly on both sweeps: `2194 = 2193 + 1`** — the CI-confirmed 2193 at
`9331ce3`/`c3e6177`/`3861981` plus the ONE new test fn this slice adds. Binaries **164 → 165**.
This is the PLAN's prediction met on the nose, and it is the mechanical proof that T1's three rows
added no test function.

Failing test names were enumerated from the `---- <name> stdout ----` markers (never by
indentation, which invents phantom names from the failure BODY).

**Set diff:** sweep 1 ∖ sweep 2 = `{cursor_bounds_on_shrinking_endpoint_set}`; sweep 2 ∖ sweep 1 = ∅.

**Classification — by ISOLATION, which is the ONLY classifier:**

| test | in sweeps | ISOLATION | class |
|---|---|---|---|
| `access_log_h2_rcd_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** (ADR-0164) |
| `access_log_h2_uc_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `access_log_rcd_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `access_log_rf_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `admin_config_dump_server_info` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `client::tests::send_request_maps_h2_handshake_failure_to_typed_error` | 1, 2 | **`ok. 1 passed`** | **TAIL** |
| `cursor_bounds_on_shrinking_endpoint_set` | 1 only | **`ok. 1 passed`** | **TAIL** |

The five-member ADR-0164 deterministic core reproduced EXACTLY and unchanged — a changed member
set, not a changed tail, is what would warrant investigation. Both tail members pass alone.

**The two-sweep INTERSECTION is 6 and DISAGREES with isolation** — it contains
`send_request_maps_h2_handshake_failure_to_typed_error`, which passes in isolation and is therefore
TAIL. Using membership-in-both-sweeps as the rule would have silently promoted a tail member and
redefined the documented core from five to six. This is the 108.1-state-4 finding reproducing
exactly: **ADR-0164 leg (iii) is a SUFFICIENT flake signal, never a NECESSARY one; only isolation
classifies.** The tail's SIZE (2, then 1) carries no signal in either direction.

**Neither tail member sits on this phase's surface.** `git diff --name-only e458765 HEAD` lists ten
files and NONE is `crates/envoy-bin/tests/xds_eds_hot_reload.rs` or anything in `envoy-http2`.
`cursor_bounds_on_shrinking_endpoint_set`'s failure TEXT is
`envoy-bin HCM ready: Os { code: 111, kind: ConnectionRefused, … }` — the **CF-75-6** ephemeral-port
/ admin-ready STARTUP-RACE family, which is OPEN and whose documented remedy is to rerun the SAME
SHA, never to weaken a test. Read by TEXT and by ISOLATION, not by name.

### Step 3 — the new fixture in isolation

`cargo test -p differential --test runtime_fraction_route_gating` → `test result: ok. 1 passed; 0
failed` on every one of the six runs this session made. Timings: **1.34 s** wall after forcing a
test-binary rebuild, **1.11 s** warm (in-test 0.98-1.28 s). It also passed INSIDE both full parallel
sweeps, which is the stronger statement. A backend-free fixture in ~1-3 s is NORMAL — and this
session PROVED it rather than asserting it, by catching the upstream container live in `docker ps`
(see the Task 2 finding).

### Step 4 — measured net LoC (MEASURED, not "≈ the projection")

`git diff --numstat e458765 HEAD`: **+1001 / −9 = net +992** whole-tree; **+567 / −5 = net +562**
excluding `docs/`. Against the PLAN's **≈745** projection that is **+33%**.

| file | numstat |
|---|---|
| `tests/fixtures/0088-…/envoy.yaml` | `126 0` |
| `tests/fixtures/0088-…/envoy-rust.yaml` | `126 0` |
| `tests/fixtures/0088-…/expectations.yaml` | `124 0` |
| `tests/fixtures/0088-…/README.md` | `111 0` |
| `tests/differential/tests/runtime_fraction_route_gating.rs` | `40 0` |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | `122 4` |
| `crates/envoy-config/src/runtime.rs` | `22 0` |
| `crates/envoy-config/src/bootstrap.rs` | `10 3` |
| `crates/envoy-admin/src/endpoint.rs` | `8 2` |
| `109.2/PROGRESS.md` | (this file) |

Writing "≈ the projection" without measuring is exactly the 109.1 REVIEW M-4 finding, so the figure
is stated as a measurement with its command. **This is the fifth consecutive confirmation of the
`calibrate-loc-estimate-against-landed-phases` memory** (76.1 +50%, 109.1 +46%, now 109.2 +33%) —
and the first where the overrun lives in the DOCUMENTATION half (T3 `113 0` vs ≈80) rather than in a
mechanical call-site fan-out, which is consistent with the PLAN's own §6.1 argument that this slice
carries no T4-class fan-out.

### Deviation ledger — COMPLETE (three entries, all above)

1. **DEVIATION 1** (Task 1) — the mutation worktree was SEEDED rather than created at bare `HEAD`,
   because at that point in the task order `HEAD` predates the rows under test.
2. **DEVIATION 2** (Task 2) — the prescribed `git checkout --` revert check is a NO-OP on the
   still-untracked fixture and its `git diff --stat` is empty for the wrong reason; reverts were
   adjudicated by md5 equality instead.
3. **DEVIATION 3** (Task 4) — the prescribed `runtime_modify` count check moved 3 → 4 because the
   PLAN's own Step 1 requires adding the control citation; adjudicated by LINE, with byte-identity
   of the CF-108-2 paragraph and of the corrected row's untouched clauses proven in Python.

**No other deviation was taken.** The PLAN was not edited (D-3.5). Nothing outside the PLAN's named
files was touched: `tests/differential/src/lib.rs` unmodified, no existing fixture edited,
`HEADER_ALLOW_LIST` still 3 entries, `known-failures.txt` still 21 lines, no `ci.yml` edit (this
slice adds no fuzz target), no ROADMAP status cell flipped, no ADR written (**ADR-0177 stays
UNRESERVED**), no `REVIEW.md`.

### Stop condition — RE-MEASURED, FALSE on all three legs (the FORTIETH consecutive)

- **(i)** ROADMAP **113 rows / 111 `done` / 1 `in-progress` (parent `109`) / 1 `planned` (`109.2`)**
  — a state-3 implementation flips no cell. **FALSE.**
- **(ii)** `109.2` is implemented but NOT verified, NOT reviewed and NOT closed; parent `109` is
  still open; `RuntimeUInt32`/CSRF consumers, RTDS and hot restart remain unbuilt. **FALSE.**
- **(iii)** THREE family headings still carry ZERO rows (`### HTTP/3 + QUIC family`,
  `### gRPC family`, `### WASM host family`), re-measured by heading-slice. **FALSE.**

**No `stop` file was created and none exists.**

---

# §5 state-4 verification — the formal §7.5 gate adjudication

> **A SEPARATE session from the state-3 implementation above** (§5.1; ADR-0127 — the context that
> wrote the code must not be the one that grades it). This section is **APPEND-ONLY**: not one line
> of the state-3 sections above was edited.
>
> Every figure below was **RE-RUN in this session**, from the **repo root**, at HEAD
> `3982c89e3cd9bbe9fdabb9d2e82fd43db2178c10` — clean tree, branch `main`, level with `origin/main`
> after `git fetch origin --prune`. **Nothing was inherited from the Task-5 exit gate**; where a
> re-run disagrees with it, the disagreement is recorded as a finding rather than reconciled away.
>
> **This session GRADES and does not FIX** (ADR-0127 / ADR-0165). The twelve findings in the ledger
> below are **BANKED** for the state-5 review or for a later §5.2 state-3 re-entry. Not one was
> repaired here. No `REVIEW.md` was written (state 5's output) and no ROADMAP status cell was
> flipped (state 6's).

## Detection rule, re-verified on disk (not trusted from the handoff)

`docs/envoy-rust/phases/109.2-runtime-fraction-fixture-and-contract/` holds **`SPEC.md` + `PLAN.md` +
`PROGRESS.md` and NO `REVIEW.md`** — §5 state 4's unambiguous rule. Sibling `109.1/` holds all FOUR
artifacts (closed); parent `109/` holds `SPEC.md` ONLY (split parent, §6.2 step 1 — no `PLAN.md`
will ever exist for it). ROADMAP census **113 data rows / 111 `done` / 1 `in-progress` / 1
`planned`**, the two non-`done` rows ENUMERATED BY ID rather than inferred from a count: `109` →
`in-progress`, `109.2` → `planned`. ADR head **ADR-0176**; `grep -c '^## ADR-0177'` = **0**.

## Gate (e) — the five workspace commands

| # | command | exit | evidence gated on |
|---|---|---|---|
| e1 | `cargo build --workspace --all-targets` | **0** | **14** `Compiling` lines |
| e2 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0** | **14** `Checking` lines; **0** `^warning`, **0** `^error` |
| e3 | `cargo fmt --all -- --check` | **0** | zero bytes of output |
| e4 | `cargo test --workspace --no-fail-fast` (×2) | 101 / 101 | see the sweep table |
| e5 | `cargo deny check` | **0** | the line `advisories ok, bans ok, licenses ok, sources ok` |

**The cached-no-op trap was defeated causally, not assumed away.** `target/` was already warm (21 GB)
from the state-3 session, so a bare build would have returned exit 0 with **ZERO** `Compiling` lines —
non-evidence per the standing trap, and the build cache and the clippy cache no-op *independently*.
Before the gate chain ran, `crates/envoy-config/src/lib.rs` was `touch`ed (**mtime only** — the tree
stayed clean; `git status --porcelain` is empty at this commit). Both counts then came back at
**14**, and the 14 are exactly `envoy-config` plus its dependents — a real dirty set, not a number.
The build finished in 3.02 s and clippy in 1.94 s because rustc's incremental cache saw *identical
content* behind the refreshed mtime; the compiler demonstrably ran. **The cold, from-scratch
compile is CI's, and CI is where that half of gate (e) is authoritative** (state-4 = CI's first real
execution).

`cargo deny` was run **from the repo root** — it does NOT walk up, unlike build/clippy/fmt/test. It
emitted **5** `license-not-encountered` warnings (`0BSD`, `BSD-2-Clause`, `MPL-2.0`,
`Unicode-DFS-2016`, `Zlib` — unmatched *allowances*, not findings), so it was gated on the exit code
PLUS the four-ok line and never on a loose `warning` grep, which false-positives here.

`cargo build -p envoy-bin` (DEBUG — the differential harness runs the debug binary, never the
release one) → exit **0**, **11** `Compiling` lines, run before any local differential.

## Gate (e4) — the sweep, run TWICE, censused by the standing recipe

Recipe `grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed'` + awk fields **4**/**6**
(the `ok`-only form makes `failed=0` tautological; fields `$5`/`$7` return a believable `passed=0`),
with the binary count asserted **separately** and cross-checked against `grep -c 'test result:
FAILED'`:

| sweep | wall | binaries | passed | failed | passed+failed | `FAILED` cross-check |
|---|---|---|---|---|---|---|
| 1 | 5 m 57 s | **165** | **2184** | **10** | **2194** | 10 ✓ |
| 2 | 5 m 32 s | **165** | **2187** | **7** | **2194** | 7 ✓ |

**The identity closes exactly on BOTH sweeps: `passed + failed = 2194` over `165` binaries** — and
`2194 = 2193 + 1`, the CI-confirmed 2193 at `9331ce3`/`c3e6177`/`3861981` plus the ONE test fn this
slice adds. It also closes the cross-instrument check that matters: **local `passed + failed`
(2194) == CI `passed` (2194)** at `39e9afc`. The new fixture's binary
(`tests/runtime_fraction_route_gating.rs`) is present in both sweeps and in neither failing set —
it PASSED inside both full parallel sweeps, which is a stronger statement than any isolated run.

## Classification — BY ISOLATION ONLY, every RED, no exceptions

Failing names were enumerated from the `---- <name> stdout ----` markers (never by indentation,
which invents phantom names out of the failure BODY). Set arithmetic over the two sweeps:

- sweep 1 ∖ sweep 2 = **5** — `access_log_file_sink`, `access_log_h2_uf_connect_failure`,
  `access_log_or_filter`, `http_filter_rbac_fixture`, `set_metadata_dynamic_metadata`
- sweep 2 ∖ sweep 1 = **2** — `access_log_and_filter`, `xds_file_based_eds_fixture`
- intersection = **5**, union = **12**

All **12** were then re-run ALONE, sequentially:

| test | in sweeps | ISOLATION | class |
|---|---|---|---|
| `access_log_h2_rcd_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** (ADR-0164) |
| `access_log_h2_uc_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `access_log_rcd_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `access_log_rf_upstream_reset` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `admin_config_dump_server_info` | 1, 2 | `FAILED. 0 passed; 1 failed` | **CORE** |
| `access_log_and_filter` | 2 | `ok. 1 passed; 0 failed` | TAIL |
| `access_log_file_sink` | 1 | `ok. 1 passed; 0 failed` | TAIL |
| `access_log_h2_uf_connect_failure` | 1 | `ok. 1 passed; 0 failed` | TAIL |
| `access_log_or_filter` | 1 | `ok. 1 passed; 0 failed` | TAIL |
| `http_filter_rbac_fixture` | 1 | `ok. 1 passed; 0 failed` | TAIL |
| `set_metadata_dynamic_metadata` | 1 | `ok. 1 passed; 0 failed` | TAIL |
| `xds_file_based_eds_fixture` | 2 | `ok. 1 passed; 0 failed` | TAIL |

**The five-member ADR-0164 deterministic core reproduced EXACTLY and unchanged** — a changed member
set, not a changed tail, is what would warrant investigation. These five are LOCAL-only; CI passes
them (confirmed green at `39e9afc`, `failed=0`).

**The tail is SEVEN this session — and its size carries no signal.** It was 2 at the state-3 exit
gate and 7 here, on the same commit; membership moves run-to-run and neither number classifies
anything. Every one of the seven passes alone. Six of the seven share ONE failure text —
`fixture green: upstream Envoy never became accept-ready … Connection refused (os error 111)` —
i.e. the upstream **container** missed its 10 s readiness budget while a dozen sibling containers
were starting under full parallel load. That is the documented parallel-load differential family,
not a product regression.

**The intersection AGREED with isolation this session — and that is a coincidence, not a method.**
At the state-3 exit gate the two-sweep intersection was 6 and *disagreed* with isolation
(`send_request_maps_h2_handshake_failure_to_typed_error` sat in both sweeps yet passed alone). Both
sessions are the same lesson: ADR-0164 leg (iii) is a SUFFICIENT flake signal, never a NECESSARY
one. **Only isolation classifies**, and it was run on all 12 rather than on the 7 the intersection
would have exempted.

**No RED sits on this phase's surface.** `git diff --name-only e458765 3982c89` lists **12** files;
none is a `crates/envoy-http2/` file, none is an existing fixture, and none is
`tests/differential/src/lib.rs`.

## Gates (a) and (b) — the differential surface

**(a) the new fixture.** `cargo test -p differential --test runtime_fraction_route_gating`, run
ALONE three times: `test result: ok. 1 passed; 0 failed` on all three, in **1.11 s / 1.07 s /
1.10 s**. It ALSO passed inside both full parallel sweeps. The ~1 s figure is normal for a
backend-free, CLUSTER-FREE fixture and matches the `0087` warm record (1.15–1.24 s).

Re-derived independently of the state-3 claims: `envoy.yaml` and `envoy-rust.yaml` are
**BYTE-IDENTICAL** — `cmp` silent, both **126** lines, both md5 **`d205936b0390260855f19258dd02f51a`**
— and the fixture carries no `{{BACKEND_IP}}`, so it spawns no backend and is fully verifiable on
this host (it is NOT in the `192.168.65.2` backend-routing host-RED class). `expectations.yaml`'s ten
probes were read against the SPEC §1 table row-for-row: ten DISTINCT `path:` values (the §G
attribution rule) and ten byte-exact bodies, `P1-GATED / CATCH / CATCH / P4-GATED / P5-GATED /
CATCH / P7-GATED / CATCH / P9-GATED / CATCH`, matching the measured matrix.

**(b) the 87 pre-existing fixtures.** Green under the isolation classification above: the only
persistent REDs are the five documented LOCAL-only ADR-0164 core members, and every other RED passes
alone. **PASS, under the documented local-flake carve-out — CI is authoritative for the five.**

## Gate (c) — conformance

The workspace contains exactly ONE conformance package: `h2spec-conformance` (at
`tests/conformance/h2spec/`; the package name is NOT `conformance-h2spec`). h3spec, gRPC and
proxy-wasm suites do not exist — their families are unbuilt, consistent with the three zero-row
ROADMAP headings.

`cargo test -p h2spec-conformance -- --nocapture` → exit **0**, `3 passed`. **Run with
`--nocapture` precisely because the gate SELF-SKIPS SILENTLY**, and it did:

```
h2spec_runner: h2spec not found — skipping locally
test h2spec_pass_rate_gate ... ok
```

So the local `3 passed` is two string-parser units plus a **no-op** gate. **Recorded
SKIPPED-NOT-PASSED locally; CI is authoritative per ADR-0163** (in CI the gate cannot report `ok`
without actually executing h2spec — the known-failures structural proof). `known-failures.txt` is
**21** lines carrying exactly **ONE** real entry (`3.5/2`) and was **NOT touched** by this phase
(`git diff --name-only e458765 3982c89 -- tests/conformance/` is empty), so the declared threshold is
untouched. **This slice adds no conformance surface.**

## Gate (d) — fuzzing

**Vacuously met: this slice adds NO fuzz target.** Measured — **5** targets across **FIVE** crates
(`envoy-accesslog`, `envoy-config`, `envoy-filter`, `envoy-http2`, `envoy-jwt`), unmoved; and
`git diff --name-only e458765 3982c89 -- .github/` is **empty**, so no `ci.yml` step was needed or
added (a new target is not auto-discovered — omitting the step is how gate (d) goes silently unmet).

## §7.5 verdict

| gate | verdict |
|---|---|
| (a) new/changed differential fixtures green | **PASS** — `0088` 3/3 alone + inside both sweeps |
| (b) pre-existing differential fixtures green | **PASS** — under the documented ADR-0164 LOCAL-only carve-out; all other REDs pass in isolation |
| (c) conformance at the declared threshold | **PASS, CI-AUTHORITATIVE** — h2spec self-skips locally (ADR-0163); threshold untouched |
| (d) new fuzzer short-budget CI run | **VACUOUSLY MET** — no fuzz target added |
| (e) build / clippy / fmt / test / deny | **PASS** — all exit 0, each gated on line counts or its own ok-line, not on an exit code alone |
| (f) `REVIEW.md` approved | **NOT MET — and correctly so.** State 5 owns it; writing it here would chain 4→5 (§5.1; ADR-0127). |

**Gates (a)–(e) are MET at `3982c89`. The implementation is VERIFIED.** Gate (f) is the next
session's, by design.

## Findings — BANKED, NOT FIXED (ADR-0127 / ADR-0165)

Twelve findings. Every one was re-verified ON DISK by the main session before being written here (a
subagent finding is a CLAIM). **None was repaired.** The graded work stands as landed.

**V-1 (Minor) — the net-LoC citation is self-falsifying at the commit that carries it.** Task 5
Step 4 above and `STATE.md` both state: ``Net LoC MEASURED by `git diff --numstat e458765 HEAD`:
+1001/−9 = 992``. MEASURED now: that pair is the range **`e458765..8644fa4`** (Task 4). At
**`39e9afc`** — the commit whose `STATE.md` contains the sentence — the same command yields
**+1200/−25 = 1175**; at HEAD `3982c89`, **+1202/−25 = 1177**. The figure was correct when taken and
then re-attributed to a later SHA without re-running it. **The `excluding docs/` figure 562 IS
correct** and is stable across `8644fa4`/`39e9afc`/`3982c89` (every commit after Task 4 is
docs-only). Consequence: the "**+33%** over the PLAN's ≈745" verdict holds only for the nodocs
comparison; whole-tree it is ≈+58%. This is the same class as the 109.1 REVIEW M-4 finding that this
very section of `PROGRESS.md` cites — a session-summary arithmetic claim not re-derived at the commit
that publishes it.

**V-2 (Minor) — a PLAN byte-identity invariant was broken, and the break was not recorded.**
`PLAN.md:586` (Task 4 Step 2) requires: "The `200 application/json; body is exactly two top-level
keys` clause and the table that follows it **must be left byte-identical**." Measured
`fcad066` → `8644fa4`: the clause was BOTH prefixed and REFLOWED —
`` `allow: GET` on both sides). 200 `application/json`; body is exactly two`` + `top-level keys:`
became `answer 200 `application/json`; body is exactly two top-level keys:`, i.e. the substring
``200 `application/json`; body is exactly two\ntop-level keys:`` occurs **1×** in the old file and
**0×** in the new. **The table half of the invariant HELD** (13 rows, byte-identical). The edit is
editorially defensible — removing "on both sides" from the preceding sentence left "200 …" without a
subject — which is exactly the shape of a reality-forced departure doctrine says must be LABELLED.
It was not: the ledger describes sites 1–3 only as "now record the TRUE asymmetry". The executor
demonstrably understood the invariant class, having proven Step 1's parallel byte-identity
requirement mechanically in Python and written it up.

**V-3 (Minor) — the deviation ledger's completeness claim is falsified by its own commit.** The
ledger asserts "**No other deviation was taken.** … Nothing outside the PLAN's named files was
touched". `39e9afc` touches `docs/envoy-rust/STATE.md` (`19 16`) and
`docs/envoy-rust/STATE_HISTORY.md` (`42 0`), and **`grep -c 'STATE' PLAN.md` = 0** — the PLAN names
neither file anywhere, and its Task-5 `git add` line lists `PROGRESS.md` alone. Stated fairly: the
state-3 → state-4 advance is BOOTSTRAP_PROMPT §5 session protocol and sits outside the PLAN's
authority, the commit message announces it, and an independent audit of the whole diff found the
edits attributable to protocol rather than to scope creep. The defect is in the LEDGER, which both
omits the departure and affirmatively denies it — and it is the ledger a later reviewer reconstructs
from.

**V-4 (Minor) — the contract asserts UNMEASURED upstream behaviour for CF-109-3.**
`BEHAVIOR_CONTRACT.md:3284-3286`: "Each is boot-fatal here **where upstream accepts**" — universally
quantified over CF-109-1/2/3. Upstream acceptance is measured for CF-109-1 (cell 5) and CF-109-2
(cells 7/8); **neither source matrix contains a single jwt probe cell**, so the CF-109-3 leg of
"each" has no measurement anywhere. Exactly the "a doc claim is an inherited census" class the
subsection itself warns about elsewhere.

**V-5 (Minor) — the cascade's step 2 folds upstream-unmeasured classes into a "one row per measured
cell" enumeration.** `:3266-3267`: "Otherwise — bools, non-numeric strings, **the empty string,
non-finite spellings** — → `default_value` (cells 10, 11, B1-B3)." Cells 10/11 are `"abc"` and
B1-B3 are bools; **no cell covers the empty string or a non-finite spelling**, and `109.1/SPEC.md`
explicitly excludes those as not-measured-upstream. The adjacent empty-`runtime_key` sentence models
the correct treatment ("upstream-unmeasured; … recorded"); step 2 drops the marker.

**V-6 (Minor) — the CF-109-1 bullet contradicts the contract's own F3/S1 rows.** `:3288-3291` says
the class includes cells F3 and S1 "**because upstream samples them per request**". But row F3
(`:3244`) records `FALLBACK 40/40` with the explicit hedge "0.5% sampling and truncate-to-0 are
indistinguishable at n=40", and S1 (`:3248`) likewise records `FALLBACK 40/40` with no sampling
claim. Per-request sampling is measured only for cells 5 (n=60) and F4 (GATED 1/40). The bullet
asserts flatly what its own table hedges.

**V-7 (Nit) — the probe-count preamble is wrong for cell 5.** `:3219-3221` says cells 1-13 were
measured "**30 probes each** (cells 1/3/9/13 RE-RUN at 40/40 at the state-2 split)". Cell 5 was
**n=60** (`GATED 27 / FALLBACK 33`) per `109/SPEC.md:43` and per the contract's own row `:3230`;
cell 9 already read 40/40 at the pick. The table rows are right — only the preamble generalises.

**V-8 (Nit) — the "`edge:`-only" characterisation of the unit table's remainder is incomplete, and
incomplete about rows THIS phase added.** `:3250-3253` says the extras beyond the 23 are "labelled
`edge:`". Measured in `route_fraction_gate_pins_every_measured_cell`: **11** `edge:`-labelled rows
**plus the 3 `M-1`/`M-2`/`M-3` rows Task 1 landed**, which carry no `edge:` label and are equally
upstream-unmeasured. A reader applying the stated rule would count the M-rows as measured. (The
contract's *positive* claim — "these 23 rows are the MEASURED contract and nothing else is" — holds
exactly.)

**V-9 (Nit) — the fixture-partition paragraph reads as exhaustive but accounts for 15 of 23 cells.**
`:3311-3316`: 9 pinned by `0088` + 4 nondeterministic + 2 reject-direction = **15**. Cells **11, B1,
B2, B3, F1, F2, N1, N2** are deterministic, non-fatal, and simply absent — they COULD appear in a
fixture, and a later reader will infer the partition is complete. Separately, "the jwt surface" is
listed among "cells" but is not a cell in either matrix.

**V-10 (Nit) — "upstream also accepts `>`" has no cell.** `:3271-3272` records that divergence for
`default_value` numerator > denominator. Enumerated: **no probe in either source matrix sets a
`default_value` numerator greater than its denominator** (cell 12 probes a runtime *value* of 200,
a different quantity). Inherited from the fault/CSRF house discipline, not measured here.

**V-11 (Nit) — CF-109-1's unblock condition is narrower than the ledger's.** `:3292` gives
"*Unblocked by* a phase that lands per-request sampling"; `109/SPEC.md` §6 and ADR-0175 D5 both give
it as **a PRNG ADR *plus* a §7.2 contract-relaxation ADR** (shared with the non-deterministic-LB
candidate). Not contradictory — the second gate is simply dropped.

**V-12 (Nit) — no task-boundary gate is recorded for Task 3.** `PLAN.md:19` (Global Constraint)
requires every task boundary to run build / clippy / `fmt --check`. Tasks 1, 2 and 4 each record
theirs above; the Task 3 section records only its structural verification. Task 3 is docs-only and
Task 5's full gate covers the tree afterwards, so this is harmless in substance — but it is a
prescribed check neither evidenced nor waived.

### Two standing census recipes corrected forward

- **`grep -c '^## ADR-'` returns 173, not 172.** The extra is the schema template
  `## ADR-NNNN: <title>` at `DECISIONS.md:10`, which carries no 4-digit number. The numbered form
  `grep -c '^## ADR-[0-9]\{4\}'` = **172**, with no duplicates, against a head of **ADR-0176** — the
  numbering is sparse and 172-vs-176 needs no reconciling.
- **`ADR-0177` DOES appear once in `DECISIONS.md`**, at `:2426`, as prose inside ADR-0176's
  `**Consequences.**` paragraph ("next available **ADR-0177** (unreserved)"). The substantive claim
  is intact — `grep -c '^## ADR-0177'` = **0**, so no ADR-0177 has fired — but "does not appear
  anywhere" is false and a future session grepping the bare string will get a hit.

## Censuses RE-DERIVED at this verification (every one measured, none inherited)

**88** fixture dirs (highest `0088-runtime-fraction-route-gating`, so `0089` is next) / **88**
differential test files — via `git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l`,
since `git ls-files 'tests/fixtures/*/'` is a vacuous glob returning a clean-looking ZERO. **165**
test binaries; identity `passed + failed = 2194`. **134** `ConfigError` variants, the enum
brace-matched to `lib.rs:75-1105` (a naive `grep -cP '^    [A-Z]'` over the whole file returns
**162**). Fuzz `.gitignore` **69**/**66**/**66**, **envoy-config-SCOPED** — a workspace-wide
`git ls-files 'crates/*/fuzz/corpus/**'` returns **78** and answers a DIFFERENT question, not a
drift. **5** fuzz targets across FIVE crates. **21**-line `known-failures.txt` (ONE real entry).
**14** crates (no `envoy-runtime` — ADR-0172 D8: the snapshot store is a MODULE in `envoy-config`).
**117** phase directories. `runtime.rs` **910** lines; `bootstrap.rs` **21950**;
`BEHAVIOR_CONTRACT.md` **4045** with **15** `## ` and **24** `### ` (both held constant by the
phase); `STATE.md` **204** lines with **5** `## ` / **3** `### ` and its `**Standing traps**` line
MEASURING **130784** CHARACTERS (**131902** bytes — `wc -c` reads ~1118 high on multi-byte glyphs);
`STATE_HISTORY.md` **15885** by `wc -l` (a python `split("\n")` reads **15886**, one higher, for the
trailing newline); `109.2/PLAN.md` **656**; `109.2/PROGRESS.md` **450** before this section.

## Stop condition — RE-MEASURED, FALSE on all three legs (the FORTY-FIRST consecutive)

- **(i)** ROADMAP **113 data rows / 111 `done` / 1 `in-progress` / 1 `planned`**, the two non-`done`
  rows ENUMERATED BY ID (`109` → `in-progress`, `109.2` → `planned`) rather than inferred from a
  count. A state-4 verification flips no cell. **FALSE.**
- **(ii)** `109.2` is implemented and now VERIFIED, but NOT reviewed and NOT closed; parent `109` is
  still open; `RuntimeUInt32`/CSRF consumers, RTDS and hot restart remain unbuilt. **FALSE.**
- **(iii)** THREE family headings still carry ZERO rows — `### HTTP/3 + QUIC family`, `### gRPC
  family`, `### WASM host family` — re-measured by a heading-slice census walking all **11** `### `
  headings (10 / 5 / 3 / 14 / **0** / **0** / 6 / 29 / 6 / **0** / 13). **FALSE.**

**No `stop` file was created and none exists.** The next session is the `109.2` §5 **state-5 code
review** — a SEPARATE session (§5.1; ADR-0127), which must read `109.1/REVIEW.md` §8 BEFORE grading
or it will re-derive that round's banked findings as if new.
