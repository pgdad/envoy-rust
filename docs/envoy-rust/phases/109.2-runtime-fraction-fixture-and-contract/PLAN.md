# Sub-phase 109.2 Implementation Plan — differential fixture `0088-runtime-fraction-route-gating`, the `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection, the decided-in 108.2-M-1 correction, and the three banked witness rows

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every implementation subagent still does TDD (D-3.1) and gets FULL zero-context instructions (D-3.4); any tree-MUTATING subagent gets its OWN worktree reset to current `main`.

**Goal:** Land the DIFFERENTIAL witness for the `RouteMatch.runtime_fraction` route gate that sibling `109.1` made live — a new cluster-free, backend-free fixture `0088-runtime-fraction-route-gating` whose `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL — plus the `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection recording the 23-cell measured matrix, the decided-in 108.2-REVIEW-M-1 correction of the measured-false bilateral-405 claim, and the three banked cascade-guard witness rows. Sub-phase `109.2` is the last slice of parent phase `109`; its state-6 close-out flips ROADMAP rows `109.2` AND `109` to `done` together.

**Architecture:** No production code changes and no harness changes. The fixture drives the EXISTING `Driver::Http1ProbeList` (13 fixtures already use it; `tests/differential/src/lib.rs:115-121`) over ten `direct_response` routes, nine of which carry a `match.runtime_fraction` whose gate outcome is decided by a two-static-layer `layered_runtime` block. Each probe has a DISTINCT `path:` and a DISTINCT `direct_response` body, so the response body IS the gate's verdict, byte-exact. Everything else in this slice is documentation plus three rows appended to an existing table-driven unit test.

**Tech Stack:** YAML fixture data; Rust test data (three tuple rows in an existing `vec!`); Markdown. No new dependency, no new harness machinery, no PRNG, no new fuzz target (so **no `ci.yml` edit**).

**Spec:** `docs/envoy-rust/phases/109.2-runtime-fraction-fixture-and-contract/SPEC.md` (the design authority — §1 fixture contract, §2 D1-D5, §4 X-1…X-5). Sibling `docs/envoy-rust/phases/109.1-runtime-fraction-config-and-gate/` holds all FOUR landed artifacts (`SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`, verdict APPROVED); parent `docs/envoy-rust/phases/109-runtime-fraction-route-gating/` holds `SPEC.md` ONLY and is a SPLIT PARENT — **no `PLAN.md` will ever exist for it** (§6.2 step 1). ADR-0176 fixed the cut and DECIDED-IN the M-1 ride-along (DECISION 5); ADR-0175 is the parent pick.

---

## Global Constraints

- **Every literal in this plan was PRE-FLIGHTED by the plan-write session at commit `464167a`** — the fixture YAML was `--mode validate`-checked against upstream `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…70c2`, verified by `docker image inspect` before probing), SERVED by that image over three independent probe passes, and served by a debug `envoy-bin` built from this tree; the three Rust rows were compiled, run GREEN, and mutation-RED-checked in a scratch worktree. Transcribe them BYTE-FOR-BYTE. Where a step says MEASURED, that number came off this session's disk or wire, not from an inherited document.
- **Line numbers in this plan were verified at commit `464167a` and WILL drift as tasks land — locate every site by the quoted TEXT, never by the inherited number.**
- **Gate every task on `--workspace --all-targets`.** Task boundaries run `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, and the task's named test commands. **Gate clippy on a NON-ZERO `Checking` line count** — exit 0 with zero `Checking` lines is a fully cached no-op, not evidence — and gate the BUILD on a non-zero `Compiling` count for the same reason (both caches were measured cold-no-op simultaneously at 109.1 state-4). `cargo deny check` must run from the REPO ROOT (it does not walk up).
- **Assert test COUNTS, never exit codes.** `ok. 0 passed; N filtered out` is a false green. This plan's own pre-flight hit it: `cargo test -p envoy-config --lib route_fraction_gate_pins_every_measured_cell -- --exact` returned `0 passed; 709 filtered out` because the test's full path is module-qualified. **The correct filter is `route_fraction_gate_pins_every_measured_cell` WITHOUT `--exact`**, and the test reports as `runtime::tests::route_fraction_gate_pins_every_measured_cell`. Gate on the exact expected `N passed`.
- **TDD per task, no exceptions** (D-3.1). Task 1's rows are characterization pins that pass immediately — honour RED with the three mutation checks given verbatim in Task 1, run in a scratch `git worktree` created `--detach` at HEAD (never mutate the main tree; run the unmutated control from the same worktree; a run with no `test result` line is NOT evidence; prove the rebuild with a non-zero `Compiling envoy-config` count).
- **`cargo fmt --all` after EVERY Rust transcription, then `--check`** — transcription does not preserve canonicality.
- **Redirect full test output to a file; never pipe a verification run through `tail`** (it truncates the `failures:` block and hides `Compiling`).
- **Do NOT touch:** any `crates/` file except the two named in Tasks 1 and 4 (`crates/envoy-config/src/runtime.rs` — test module only; `crates/envoy-admin/src/endpoint.rs` — doc comment only; and the Task-4 optional `crates/envoy-config/src/bootstrap.rs` doc comment); fixtures `0011` and `0087` or ANY existing fixture; `HEADER_ALLOW_LIST` (3 entries — never add `location`); `known-failures.txt` (21 lines, ONE real entry — never trim); `tests/differential/src/lib.rs` (this fixture needs ZERO harness change — verified, see PLAN-VERIFY X-1); `ENVOY_TARGET.md`; `rust-toolchain.toml`; `ci.yml`; the test `runtime_key_is_rtds_inert`; the CSRF `runtime_key` rejects; `Route`'s hand-written `Serialize`/`Deserialize` impls; the jwt matcher `route_match_matches`; any landed phase artifact (D-3.5 — a landed REVIEW is NEVER edited; a hypothetical second round writes `REVIEW-2.md`).
- **Do NOT flip any ROADMAP status cell and do NOT write an ADR.** The `109.2` + parent-`109` two-row flip belongs to the state-6 close-out (SPEC D5, the 76.2/108.2 precedent), which is a SEPARATE session. No ADR is owed by this plan (§6.1 does not re-fire — see the gate verdict below); **ADR-0177 stays UNRESERVED**.
- Commits per task, message prefix `phase 109.2 task N:`. `next-prompt.txt` is gitignored (`.gitignore:13`) — never `git add` it.
- **Preserve ADR-0016 through ADR-0176.** CF-109-1 (WIDENED) / CF-109-2 / CF-109-3 stay OPEN — `109.1` landed their REJECT sides only and this slice lands none of their honoring sides. CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6 pass through untouched.

---

## PLAN-VERIFY results (SPEC §4 X-1…X-5, re-derived FRESH at this plan-write, commit `464167a`)

Every item below was re-measured this session. **Two of the SPEC's own details are REFUTED** — both in the fixture shape, both discovered by running the thing rather than reading it.

- **X-1 (byte-identical YAML feasibility) — CLOSED, ACHIEVED, and the SPEC's stated shape is REFUTED ON TWO POINTS.**
  1. **`{{ADMIN_PORT}}` is NOT substituted for an `Http1ProbeList` fixture.** SPEC §2 D1 prescribes "`admin` (`{{ADMIN_PORT}}`)". MEASURED FALSE: substitution keys are driver-gated by `driver_needs_admin_port` (`tests/differential/src/lib.rs:3066-3074`), whose `matches!` lists ONLY `AdminScrape`, `Http1KeepAlive`, `Http2KeepAlive` and `TcpWithStats` — `Http1ProbeList` is absent. `render_yaml` (`lib.rs:1312-1318`) leaves any token not in the kvs UNTOUCHED by design, so a literal `{{ADMIN_PORT}}` would survive into the config and fail to parse as an address. **The fixture uses a literal `port_value: 0` for admin** — the `0083`/`0086` convention. `{{PORT}}` IS substituted (`port_key_for`, `lib.rs:2998-3055`, lists `Http1ProbeList` at `:3011`).
  2. **A `node:` block cannot appear on both sides with the `0083`/`0086` spelling.** `node: { id: x, cluster: y }` is BOOT-FATAL upstream: YAML 1.1 booleanizes unquoted `y`, and upstream reported `invalid JSON in envoy.config.bootstrap.v3.Bootstrap @ node.cluster: string … unexpected character: 't'` with the rendered JSON showing `"cluster":true`. (This is the standing YAML-1.1-vs-1.2 divergence firing live on the exact spelling the existing fixtures use — which is precisely WHY those fixtures carry `node:` on the envoy-rust side only.) **The fixture omits `node:` entirely**, which both proxies accept.
  With those two corrections a SINGLE byte-identical file serves both sides: `--mode validate` returned `configuration '/e.yaml' OK` upstream, and the same bytes booted the debug `envoy-bin`. Fixture `0088` therefore becomes the **SECOND** byte-identical pair of the 88 (exactly **1 of 87** is byte-identical today — re-derived). No fallback to the `0086` two-spelling convention is needed.
- **X-2 (re-run the dry-run before freezing `expectations.yaml`) — CLOSED, all ten cells re-measured cross-proxy at THIS session.** Upstream `envoyproxy/envoy:v1.33.0` port-mapped (`docker run -d -p`, never `--network host`), three independent passes, byte-identical readings on every pass; envoy-rust from a debug `envoy-bin` built from this tree (a debug build is MANDATORY before any local differential, and it must post-date 109.1 or it rejects `runtime_fraction` as an unknown field). **The envoy-rust side was exercised WITH `runtime_fraction` present**, as X-2 requires — the 109.1-landed parser accepts it. Both sides produced, for `/p-default-on`, `/p-default-off`, `/p-key-zero`, `/p-key-hundred`, `/p-key-twohundred`, `/p-quoted-zero`, `/p-unparseable`, `/p-two-layer`, `/p-million`, `/p-catch` respectively: `P1-GATED`, `CATCH`, `CATCH`, `P4-GATED`, `P5-GATED`, `CATCH`, `P7-GATED`, `CATCH`, `P9-GATED`, `CATCH` — all status 200. This matches SPEC §1's table on all ten cells. **This remains a CLAIM the state-3 session re-establishes THROUGH THE HARNESS** (both proxies, one `run_fixture` call); a hand-driven dry-run is not a green fixture.
- **X-3 (fixture census) — RE-DERIVED: 87 fixture directories, highest `0087-runtime-static-layer`, so `0088` is still the next free number.** Recipe: `git ls-files 'tests/fixtures/**' | cut -d/ -f3 | sort -u | wc -l` = **87** (the naive `git ls-files 'tests/fixtures/*/'` returns a clean-looking ZERO — do not use it). `ls tests/differential/tests/*.rs | wc -l` = **87**. Test binaries **164**.
- **X-4 (locate the three M-1 texts by their WORDS) — CLOSED, all three located, all drifted from the SPEC's cited numbers.** `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1379` (the `/runtime` row of the `## Admin endpoint body shapes` table — the sentence `POST → 405 \`allow: GET\` bilaterally.`, whole-file count of that exact string = **1**); `docs/envoy-rust/BEHAVIOR_CONTRACT.md:3181-3182` (the text `GET-only (POST → 405`, count = **1**); `crates/envoy-admin/src/endpoint.rs:3319-3320` (the text `GET-only on BOTH`, count = **1**) — the SPEC cited `:3318-3319`; the doc comment spans `:3317-3320` and documents the test `runtime_post_is_method_not_allowed` at `:3322`. A near-miss that must NOT be edited: `BEHAVIOR_CONTRACT.md:3233` describes `/runtime_modify` (upstream POST-only, 405 on **GET**, envoy-rust 404s it) — that is CF-108-2, a DIFFERENT endpoint, correctly stated, and explicitly non-bilateral.
- **X-5 (transcribe the matrices from the SOURCES, not from `109.2/SPEC.md`'s summary) — SOURCES READ AND RECONCILED.** Parent `109/SPEC.md` §1.1 (heading at `:28`) holds the **13**-cell pick matrix (rows 1-13, 30-40 probes each). `109.1/SPEC.md` §1.2 (`:48-79`) holds the **10**-cell V-8 closure matrix (B1-B3, F1-F4, N1, N2, S1, 40 probes each) and §1.3 (`:80-105`) the evaluation cascade. **13 + 10 = 23**, which is the figure `route_fraction_gate`'s own doc comment states (`crates/envoy-config/src/runtime.rs:150`). Task 3 transcribes from those two sections. Note for the transcriber: the landed unit table pins **more** than 23 tuples (24 `ok_cells` + 4 boot-fatal + 3 map-shaped + 1 sibling + 2 default-edge assertions); the extra ones are SPEC §1.3-derived rows labelled `edge:` and are NOT upstream-measured. **The contract subsection records the 23 MEASURED cells; it must not claim the derived edges were measured.**
- **Additional re-derived censuses (all unmoved at `464167a`):** `crates/envoy-config/src/runtime.rs` **888** lines; `route_fraction_gate` `:163-211`, `route_fraction_passes` `:219-225`, the table test `:590-859`; the four 109.1 `ConfigError` variants at `crates/envoy-config/src/lib.rs:768-812`; **134** `ConfigError` variants total; `route_matches` at `crates/envoy-http1/src/hcm.rs:2194-2220` with the gate at `:2205-2209`; `bootstrap.rs` **21943** lines; **14** crates (no `envoy-runtime` — ADR-0172 D8: the store is a MODULE in `envoy-config`); **117** phase directories; fuzz `.gitignore` **69** lines / **5** targets / **66** tracked corpus files; ROADMAP **113** rows / **111** `done` / **1** `in-progress` (parent `109`) / **1** `planned` (`109.2`); ADR head **ADR-0176**, next free **ADR-0177** UNRESERVED; `109.1/REVIEW.md` **480** lines; `BEHAVIOR_CONTRACT.md` **3927** lines with the `## Runtime` section spanning **3162-3240** (next `## ` heading `## xDS wire state machine` at `:3241`) and carrying ZERO `### ` subheadings — it is organised entirely as bold-lead paragraphs.
- **CI identity prediction for state 4.** This slice adds exactly ONE `#[test]`/`#[tokio::test]` function (the fixture entrypoint) and ZERO new unit-test functions — Task 1's three rows go INSIDE an existing `vec!` in an existing test fn, so they move no count. Expect **binaries 164 → 165** and **passed 2193 → 2194**, `failed=0`. A different delta is a signal, not a rounding error.

### §6.1 gate verdict — re-derived BOTTOM-UP at this plan-write: **the split does NOT fire**

| Task | Deliverable | Net LoC | Basis |
|---|---|---|---|
| 1 | three witness rows in `runtime.rs` | **22** | MEASURED — the patch was written, `cargo fmt`-canonicalised and `numstat`-measured this session (`22 0`) |
| 2 | fixture `0088`: `envoy.yaml` + `envoy-rust.yaml` + `expectations.yaml` + `README.md` + differential test file | ≈ **510** | `126 + 126` MEASURED (the validated file); `expectations.yaml` ≈115 and `README.md` ≈110 priced against `0086` (133/93) and `0083` (214/219); test file ≈30 priced against `route_redirect_action.rs` (47) and `runtime_static_layer.rs` (18) |
| 3 | `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection | ≈ **80** | a 23-row table + cascade + carry-forwards, in the section's existing prose style |
| 4 | the M-1 correction (3 texts) + the stale-`RuntimeFractionalPercent`-doc narrowing | ≈ **15** | four doc edits, all in-place rewordings |
| 5 | state-3 exit gate | **0** | no code |
| — | `PROGRESS.md` appends (state 3 writes these) | ≈ **120** | `0086`'s per-task appends measured 119+54+88 across three commits |
| | **TOTAL** | **≈ 745 net LoC over 5 tasks** | |

Both thresholds are cleared with wide headroom (**~745 vs ~1500 LoC; 5 vs ~25 tasks**). Applying the standing calibration honestly: `109.1` landed **+46%** over its projection and `76.1` **+50%**, both concentrated in test/mechanical halves — but this slice's single largest line item (the 252 lines of fixture YAML) is not an estimate at all, it is a file that already exists and has been validated on both proxies, and the slice contains **no mechanical call-site fan-out of the T4 class** that the `109.1` M-4 record identifies as the ~3× offender. Even at +50% across every estimated (non-measured) component the total lands near **~1000**, still under the gate. **VERDICT: the §6.1 split does NOT fire. No ADR is written by this plan; ADR-0177 stays UNRESERVED** (the 108.2 and 109.1 PLAN-writes both landed with their reserved number unfired). If the mid-execution trigger fires anyway (any single task's sub-steps blowing past ~10 items), §6.2 applies IN FULL and ADR-0177 records it.

### Banked findings — scheduled or explicitly DECLINED (§6.3 / ADR-0165: a PLAN schedules or declines; it does not fix)

| Banked item | Origin | Disposition in this plan |
|---|---|---|
| M-1 **remedy correction** — a discriminating empty-`runtime_key` pin needs a snapshot with a literal `""` entry or a `.`-prefixed entry; the state-4-handed "diverging default (numerator 100)" remedy was TRACED FALSE | `109.1/REVIEW.md` §8 | **SCHEDULED — Task 1, row 1.** The corrected remedy is used; the refuted one is NOT. Verified discriminating by mutation this session. |
| The **three-row witness patch** (M-1 corrected snapshot / `inf` + default-**0** for M-2 / `1_000_000`/MILLION no-key Always for M-3) | `109.1/REVIEW.md` §8 / M-1, M-2, M-3 | **SCHEDULED — Task 1 in full.** All three rows written, run GREEN, and each MUTATION-RED-checked this session. |
| M-5 — the two glued `envoy-http2/src/hcm.rs:1925`/`:2045` fan-out literals (`runtime_fraction: None, },` at off-by-3 indent; rustfmt DECLINES to reflow them, so `fmt --check` is structurally blind FOREVER) | `109.1/REVIEW.md` §8 | **EXPLICITLY DECLINED.** The bank's own condition is "hand-fixable by any future task that edits that file" — **`109.2` edits no file in `envoy-http2`**, and opening that crate purely to fix two test-only indentation sites is scope the SPEC does not carry. Stays banked, unfixed, for the next slice that legitimately touches `crates/envoy-http2/src/hcm.rs`. |
| M-4 — the LoC-calibration record (T4-class mechanical fan-outs cost ~3× their naive one-line-per-site price) | `109.1/REVIEW.md` §8 | **CONSUMED AS INPUT, not fixed.** It is a record about estimation, and it is applied in the §6.1 verdict above (which is why that verdict states the no-fan-out fact explicitly rather than just quoting a number). |
| 108.2 M-1 — the measured-false bilateral-405 claim | `108.2/REVIEW.md`, **DISPOSED — decided-IN by ADR-0176 DECISION 5** | **SCHEDULED — Task 4.** This is the one banked item this slice is REQUIRED to land. |
| 108.2 M-2 + N-1…N-6; the `109.1` REVIEW's N-1…N-6; the 76.1/76.2/108.1 families | various | **STAY BANKED, UNFIXED** (§6.3; ADR-0165). None is re-issued and none is touched here. |
| **NEW at this plan-write (not previously banked):** `RuntimeFractionalPercent`'s doc comment (`crates/envoy-config/src/bootstrap.rs:1497-1501`) still reads "envoy-rust honors only the deterministic 0%/100% `default_value`; a present `runtime_key` is rejected (no RTDS runtime layer — ADR-0061 L6)". That was true when only CSRF used the type; after 109.1 the ROUTE consumer HONORS `runtime_key`. A reader arriving from `RouteMatch.runtime_fraction` reads a false statement about the field they are using. | measured this session | **SCHEDULED — Task 4, step 4**, flagged as an ADDITION to SPEC D4 rather than a silent inclusion. It is the same class as `109.1`'s Task-7 D7 narrowing (a consumer-absence sentence falsified by the landed gate) and costs ~4 lines in the same task that already corrects three doc sentences on this exact surface. If the state-5 reviewer judges it out of scope, dropping it costs nothing and breaks nothing. |

---

### Task 1: The three banked cascade-guard witness rows in `runtime.rs`

**Files:**
- Modify: `crates/envoy-config/src/runtime.rs` — the `ok_cells` vector inside `#[cfg(test)] mod tests`, immediately after the row labelled `"edge: empty runtime_key is not consulted -> default 0 -> Never"` (at `:752-756` at plan time; **locate by that label text**).

**Interfaces:**
- Consumes: the existing test helpers `snap(&[&str]) -> RuntimeSnapshot`, `one(&str)`, `rf(u32, DenominatorType, Option<&str>) -> RuntimeFractionalPercent`, and the in-scope bindings `empty` (a `RuntimeSnapshot::default()`), `Hundred`/`Million`, `Always`/`Never` — all already declared at the top of `route_fraction_gate_pins_every_measured_cell`.
- Produces: nothing new. **No new test function, no new public item** — three tuples appended to an existing `vec!`. The workspace test COUNT does not move.

**Why this task exists:** `109.1/REVIEW.md` M-1/M-2/M-3 found that the 23-cell table pins every measured cell but witnesses three of the cascade's GUARDS only from their masked side. Each row below pins one guard from the direction that EXPOSES it.

- [ ] **Step 1: Add the three rows.** Insert immediately after the closing `),` of the `"edge: empty runtime_key is not consulted -> default 0 -> Never"` tuple and immediately before the `];` that closes `ok_cells`. This is the exact fmt-canonical text (`cargo fmt --all -- --check` clean as written):

```rust
            // 109.2: the three witness rows the 109.1 REVIEW banked (M-1/M-2/M-3).
            // Each pins a cascade GUARD from the direction that EXPOSES it; the
            // pre-existing rows above witness those guards only from their
            // masked side.
            (
                "M-1: empty runtime_key is NOT consulted — a `.`-prefixed snapshot entry discriminates (the diverging-default remedy does NOT)",
                snap(&["name: l\nstatic_layer:\n  .dotted: 1\n"]),
                rf(100, Hundred, Some("")),
                Always,
            ),
            (
                "M-2: `inf` paired with default 0 — the non-masking direction of the is_finite guard",
                one("inf"),
                rf(0, Hundred, Some("gate.k")),
                Never,
            ),
            (
                "M-3: default 1_000_000/MILLION with no key pins the denominator.value() consultation",
                empty.clone(),
                rf(1_000_000, Million, None),
                Always,
            ),
```

- [ ] **Step 2: `cargo fmt --all`, then confirm the diff is exactly `22 0`**

Run: `cargo fmt --all && cargo fmt --all -- --check && git diff --numstat crates/envoy-config/src/runtime.rs`
Expected: `--check` silent (exit 0); numstat exactly `22	0	crates/envoy-config/src/runtime.rs`. A different insertion count means the block was transcribed into the wrong place.

- [ ] **Step 3: Run the test — it PASSES immediately (these are characterization pins)**

Run: `cargo test -p envoy-config --lib route_fraction_gate_pins_every_measured_cell 2>&1 | tee /tmp/109_2-t1-green.log`
Expected: `test runtime::tests::route_fraction_gate_pins_every_measured_cell ... ok` and `test result: ok. 1 passed; 0 failed`. **Assert `1 passed` literally.** Do NOT add `-- --exact` with the bare name: the test's full path is module-qualified and `--exact` on the short name yields the false green `ok. 0 passed; 709 filtered out` (measured at the plan-write pre-flight).

- [ ] **Step 4: Honour RED with three mutation checks in a SCRATCH WORKTREE (never the main tree)**

The rows pass on arrival, so the RED evidence is the mutation. Create the worktree and take a backup of the file inside it:

```bash
git worktree add --detach /tmp/109_2-wt HEAD
cd /tmp/109_2-wt
cp crates/envoy-config/src/runtime.rs /tmp/rt.bak
```

First run the UNMUTATED CONTROL from this same worktree — it must print `1 passed` (a mutation RED is not evidence without it):

```bash
cargo test -p envoy-config --lib route_fraction_gate_pins_every_measured_cell
```

Then, ONE AT A TIME — apply the mutation, run, confirm, restore with `cp /tmp/rt.bak crates/envoy-config/src/runtime.rs`:

| # | Mutation (edit inside `/tmp/109_2-wt` only) | Guard it removes | Expected result |
|---|---|---|---|
| M1 | replace `rf.runtime_key.as_deref().filter(\|k\| !k.is_empty())` with `rf.runtime_key.as_deref()` | the empty-key filter (`runtime.rs:167`) | `test result: FAILED. 0 passed; 1 failed`, panic naming the **M-1** row |
| M2 | delete the line `                && v.is_finite()` (`runtime.rs:181`) | the non-finite guard | `test result: FAILED. 0 passed; 1 failed`, panic naming the **M-2** row |
| M3 | replace `p.numerator == p.denominator.value()` with `p.numerator == 100` (`runtime.rs:203`) | the denominator consultation | `test result: FAILED. 0 passed; 1 failed`, panic naming the **M-3** row |

Each run must show a **non-zero `Compiling envoy-config` count** (a stale test binary gives a FALSE PASS) and must print an actual `test result:` line — an exit code with no `test result:` line is a compile error, which proves NOTHING. **All three were run at the plan-write pre-flight and all three REDded exactly as tabulated, each naming its OWN row** — so a GREEN here means the mutation was misapplied, not that the row is vacuous.

Then destroy the worktree and confirm the main tree is untouched:

```bash
cd /home/esa/git/envoy-rust && git worktree remove --force /tmp/109_2-wt && git status --porcelain
```
Expected: only `crates/envoy-config/src/runtime.rs` modified.

- [ ] **Step 5: Task gate + commit**

Run: `cargo build --workspace --all-targets` (non-zero `Compiling`), `cargo clippy --workspace --all-targets --all-features -- -D warnings` (non-zero `Checking`), `cargo fmt --all -- --check`.

```bash
git add crates/envoy-config/src/runtime.rs
git commit -m "phase 109.2 task 1: the three banked cascade-guard witness rows (109.1 REVIEW M-1/M-2/M-3), each mutation-RED-checked"
```

---

### Task 2: Fixture `0088-runtime-fraction-route-gating` + its differential entrypoint

**Files:**
- Create: `tests/fixtures/0088-runtime-fraction-route-gating/envoy.yaml`
- Create: `tests/fixtures/0088-runtime-fraction-route-gating/envoy-rust.yaml` (BYTE-IDENTICAL to the above)
- Create: `tests/fixtures/0088-runtime-fraction-route-gating/expectations.yaml`
- Create: `tests/fixtures/0088-runtime-fraction-route-gating/README.md`
- Create: `tests/differential/tests/runtime_fraction_route_gating.rs`
- **Do NOT modify `tests/differential/src/lib.rs`** — this fixture needs zero harness change (unlike `0087`, whose data commit had to add `AdminScrape`'s `expected_stats`). `Driver::Http1ProbeList` is already used by 13 fixtures.

**Interfaces:**
- Consumes: `Driver::Http1ProbeList { probes: Vec<Http1Probe> }` (`tests/differential/src/lib.rs:115-121`); `Http1Probe` (`:1154-1177`) whose REQUIRED fields are `name`, `method`, `path`, `host` and whose `expected_status` / `expected_body` / `expected_headers` / `extra_headers` / `body` are all `#[serde(default)]`; `Http1BodyRule::ByteExact { body }` (`:1062-1079`); `Http1HeaderRule::SetEqualModuloAllowList` (`:1081-1085`); `Expectations { driver, equivalence }` (`:28-35`); `differential::run_fixture(&Path)`.
- Produces: a green differential fixture. Nothing depends on it in later tasks except Task 3's pointer sentence.

**What the driver asserts per probe** (`run_http1_probe_list_arm`, `lib.rs:5437-5550`): cross-proxy status equality (from `equivalence.response_status: exact`), then `expected_status` against BOTH sides, then the cross-proxy body rule, then `expected_body` byte-exact against BOTH sides, then headers set-equal-modulo-allow-list. **The loop `bail!`s on the FIRST failing probe**, so one red run names exactly ONE probe — when reporting a failure, cite the named probe, and do not infer a second cell's state from a single red run.

**Why every probe has BOTH a distinct `path:` AND a distinct body:** the `BEHAVIOR_CONTRACT.md` attribution rule (the paragraph "Why every probe carries a DISTINCT `path:` — required, not cosmetic." at `:2926-2937`, sitting between the `§G` label at `:2908` and `§H` at `:2957`). Distinct paths keep the probes independently attributable. Distinct BODIES are what make the gated routes discriminating: five probes expect the catch-all body `CATCH`, and if their route bodies were also `CATCH` a wrongly-passing gate would be invisible. Each gated route answers `P<N>-GATED`, so a gate that wrongly passes returns `P<N>-GATED` where `CATCH` is expected and REDs.

- [ ] **Step 1: Write the failing test FIRST (the fixture does not exist yet)**

Create `tests/differential/tests/runtime_fraction_route_gating.rs`:

```rust
//! Sub-phase 109.2 differential acceptance test: route `match.runtime_fraction`
//! gating over the 108-landed runtime snapshot store.
//!
//! Ten HTTP/1.1 probes at a backend-free, CLUSTER-FREE HCM listener
//! (`clusters: []`, `direct_response` routes only). Nine routes carry a
//! `match.runtime_fraction`; a two-static-layer `layered_runtime` block decides
//! their gates. Each probe has a DISTINCT `path:` (the attribution rule) and
//! each route a DISTINCT body, so the response body IS the gate's verdict —
//! a wrongly-passing gate answers `P<N>-GATED` where `CATCH` is expected.
//!
//! This is the FIRST differential witness of `runtime_fraction` in the corpus,
//! and the first fixture combining `Http1ProbeList` traffic with
//! `layered_runtime`. The ten cells it pins are the deterministic subset of the
//! 23-cell matrix MEASURED against `envoyproxy/envoy:v1.33.0` (parent
//! `109/SPEC.md` §1.1 + `109.1/SPEC.md` §1.2): absent key honours
//! `default_value` in BOTH directions; a consulted key OVERRIDES the default;
//! an integer value is the numerator over HUNDRED regardless of the default's
//! denominator (`/p-million`, a 0/MILLION default gated by the value `100`);
//! `>= 100` always passes; quoted numeric strings parse like integers; an
//! unparseable value falls back to `default_value`; and a two-layer key honours
//! last-layer-wins `final_value`. The per-request-nondeterministic cells
//! (`0 < v < 100`) are boot-fatal here under CF-109-1 and are witnessed
//! in-process by 109.1, never in a fixture.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL — the second such pair
//! in the corpus. Docker-gated and backend-free (no `{{BACKEND_IP}}` marker, so
//! no backend container spawns), therefore fully verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn runtime_fraction_route_gating() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0088-runtime-fraction-route-gating");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Run it to verify it FAILS**

Run: `cargo test -p differential --test runtime_fraction_route_gating 2>&1 | tee /tmp/109_2-t2-red.log`
Expected: `test result: FAILED. 0 passed; 1 failed`, the panic naming a missing `tests/fixtures/0088-runtime-fraction-route-gating/expectations.yaml`. **Confirm a `test result:` line exists** — a compile error is not a RED.

- [ ] **Step 3: Create `envoy.yaml`** — transcribe EXACTLY. These 126 lines were `--mode validate`-checked against the pinned upstream image AND booted by a debug `envoy-bin` at the plan-write. **There is no `node:` block (upstream boot-fatal — YAML 1.1 booleanizes `cluster: y`) and admin uses a literal `port_value: 0` (`{{ADMIN_PORT}}` is NOT substituted for this driver).**

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
                        # p1 — absent key, default 100/HUNDRED -> Always.
                        - match:
                            prefix: "/p-default-on"
                            runtime_fraction:
                              default_value: { numerator: 100, denominator: HUNDRED }
                              runtime_key: gate.absent.on
                          direct_response:
                            status: 200
                            body: { inline_string: "P1-GATED" }
                        # p2 — absent key, default 0/HUNDRED -> Never.
                        - match:
                            prefix: "/p-default-off"
                            runtime_fraction:
                              default_value: { numerator: 0, denominator: HUNDRED }
                              runtime_key: gate.absent.off
                          direct_response:
                            status: 200
                            body: { inline_string: "P2-GATED" }
                        # p3 — key 0 overrides default 100 -> Never.
                        - match:
                            prefix: "/p-key-zero"
                            runtime_fraction:
                              default_value: { numerator: 100, denominator: HUNDRED }
                              runtime_key: gate.zero
                          direct_response:
                            status: 200
                            body: { inline_string: "P3-GATED" }
                        # p4 — key 100 overrides default 0 -> Always.
                        - match:
                            prefix: "/p-key-hundred"
                            runtime_fraction:
                              default_value: { numerator: 0, denominator: HUNDRED }
                              runtime_key: gate.hundred
                          direct_response:
                            status: 200
                            body: { inline_string: "P4-GATED" }
                        # p5 — key 200 >= 100 -> Always.
                        - match:
                            prefix: "/p-key-twohundred"
                            runtime_fraction:
                              default_value: { numerator: 0, denominator: HUNDRED }
                              runtime_key: gate.twohundred
                          direct_response:
                            status: 200
                            body: { inline_string: "P5-GATED" }
                        # p6 — quoted "0" parses like the integer -> Never.
                        - match:
                            prefix: "/p-quoted-zero"
                            runtime_fraction:
                              default_value: { numerator: 100, denominator: HUNDRED }
                              runtime_key: gate.quoted
                          direct_response:
                            status: 200
                            body: { inline_string: "P6-GATED" }
                        # p7 — unparseable -> default 100 -> Always.
                        - match:
                            prefix: "/p-unparseable"
                            runtime_fraction:
                              default_value: { numerator: 100, denominator: HUNDRED }
                              runtime_key: gate.abc
                          direct_response:
                            status: 200
                            body: { inline_string: "P7-GATED" }
                        # p8 — two layers, last-wins final "0" -> Never.
                        - match:
                            prefix: "/p-two-layer"
                            runtime_fraction:
                              default_value: { numerator: 100, denominator: HUNDRED }
                              runtime_key: gate.layered
                          direct_response:
                            status: 200
                            body: { inline_string: "P8-GATED" }
                        # p9 — integer value is numerator over HUNDRED, not MILLION -> Always.
                        - match:
                            prefix: "/p-million"
                            runtime_fraction:
                              default_value: { numerator: 0, denominator: MILLION }
                              runtime_key: gate.million
                          direct_response:
                            status: 200
                            body: { inline_string: "P9-GATED" }
                        # p10 — bare catch-all, no runtime_fraction.
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "CATCH" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
layered_runtime:
  layers:
    - name: base_layer
      static_layer:
        gate.zero: 0
        gate.hundred: 100
        gate.twohundred: 200
        gate.quoted: "0"
        gate.abc: abc
        gate.layered: 100
        gate.million: 100
    - name: override_layer
      static_layer:
        gate.layered: 0
```

- [ ] **Step 4: Create `envoy-rust.yaml` as a BYTE-IDENTICAL copy, and prove it**

```bash
cd tests/fixtures/0088-runtime-fraction-route-gating
cp envoy.yaml envoy-rust.yaml
cmp envoy.yaml envoy-rust.yaml && echo "BYTE-IDENTICAL"
wc -l envoy.yaml envoy-rust.yaml   # expect 126 each
```
Expected: `cmp` silent, both 126 lines. **This makes `0088` the SECOND byte-identical pair in the corpus** (exactly 1 of 87 is byte-identical today). Record that count in the README, as a per-fixture claim — it is never a tree property.

- [ ] **Step 5: Create `expectations.yaml`.** Ten probes, one per path, in route order. `method: get` is lower-case (the `0083` idiom).

```yaml
# Sub-phase 109.2 (ADR-0175/0176): ten sequential HTTP/1.1 probes against a
# backend-free, CLUSTER-FREE HCM listener (`clusters: []`, direct_response
# only) whose routes carry `match.runtime_fraction`. A two-static-layer
# `layered_runtime` block supplies the consulted values.
#
# This is the FIRST differential witness of `runtime_fraction` in the corpus
# and the first fixture combining `Http1ProbeList` traffic with
# `layered_runtime`.
#
# Each probe has a DISTINCT path (the attribution rule, BEHAVIOR_CONTRACT.md
# "Why every probe carries a DISTINCT `path:`") and each gated route answers a
# DISTINCT body `P<N>-GATED`. Nine routes are gated; the tenth is a bare
# `prefix: "/"` catch-all answering `CATCH`. So the expected body IS the gate's
# verdict: a gate that wrongly PASSES answers `P<N>-GATED` where `CATCH` is
# expected, and a gate that wrongly BLOCKS answers `CATCH` where `P<N>-GATED`
# is expected. Both directions are covered — five probes expect a gated body
# and five expect `CATCH`.
#
# THE MEASURED CELLS (parent 109/SPEC.md §1.1 + 109.1/SPEC.md §1.2, measured
# against envoyproxy/envoy:v1.33.0; re-measured cross-proxy at the 109.2
# PLAN-write over three independent passes):
#   absent key            -> default_value honoured, BOTH directions (p1, p2)
#   consulted key         -> OVERRIDES the default                  (p3, p4)
#   value >= 100          -> always passes                          (p5)
#   quoted numeric string -> parses like the integer                (p6)
#   unparseable value     -> falls back to default_value            (p7)
#   two layers            -> last-layer-wins `final_value` honoured (p8)
#   integer value         -> numerator over HUNDRED, NOT over the default's
#                            denominator — a 0/MILLION default gated by the
#                            value 100 (p9). Under the wrong reading this is a
#                            ~10^-4 event per request; no 0/100 fixture could
#                            ever catch it.
#
# NOT here, deliberately: every per-request-NONDETERMINISTIC cell
# (0 < v < 100 — integer 50, floats 0.5/1.5, the quoted "0.5") is boot-fatal
# under CF-109-1 and is witnessed by 109.1's in-process reject tests. Same for
# the map-shaped consulted key (CF-109-2) and `runtime_fraction` inside jwt
# rules (CF-109-3). A fixture cannot witness a config that refuses to boot.
driver:
  kind: http1_probe_list
  probes:
    # ---- p1: absent key, default 100/HUNDRED -> GATED --------------------
    - name: p1-absent-key-default-on
      method: get
      path: "/p-default-on"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "P1-GATED" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p2: absent key, default 0/HUNDRED -> falls through --------------
    - name: p2-absent-key-default-off
      method: get
      path: "/p-default-off"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "CATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p3: key 0 OVERRIDES default 100 -> falls through -----------------
    - name: p3-key-zero-overrides-default-on
      method: get
      path: "/p-key-zero"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "CATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p4: key 100 OVERRIDES default 0 -> GATED -------------------------
    - name: p4-key-hundred-overrides-default-off
      method: get
      path: "/p-key-hundred"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "P4-GATED" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p5: value 200 >= 100 -> GATED ------------------------------------
    - name: p5-key-twohundred-always
      method: get
      path: "/p-key-twohundred"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "P5-GATED" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p6: quoted "0" parses like the integer -> falls through ----------
    - name: p6-quoted-zero-parses-like-integer
      method: get
      path: "/p-quoted-zero"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "CATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p7: unparseable -> default 100 -> GATED --------------------------
    - name: p7-unparseable-falls-back-to-default
      method: get
      path: "/p-unparseable"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "P7-GATED" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p8: two layers, override wins, final "0" -> falls through --------
    - name: p8-two-layer-last-wins
      method: get
      path: "/p-two-layer"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "CATCH" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p9: integer value is numerator over HUNDRED, not MILLION -> GATED
    - name: p9-integer-is-numerator-over-hundred
      method: get
      path: "/p-million"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "P9-GATED" }
      expected_headers: set_equal_modulo_allow_list
    # ---- p10: the bare catch-all itself, ungated ---------------------------
    - name: p10-bare-catchall
      method: get
      path: "/p-catch"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body: { kind: byte_exact, body: "CATCH" }
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: { kind: byte_exact }
```

- [ ] **Step 6: Run the fixture — it must go GREEN cross-proxy**

Run: `cargo build -p envoy-bin` FIRST (the differential harness runs the DEBUG binary; a stale pre-109.1 binary rejects `runtime_fraction` as an unknown field), then:

`cargo test -p differential --test runtime_fraction_route_gating 2>&1 | tee /tmp/109_2-t2-green.log`
Expected: `test result: ok. 1 passed; 0 failed`. Cold run ≈ 8 s, warm ≈ 1-2 s. **A backend-free fixture completing in ~1-3 s is NORMAL, not a silent skip** — if you want to prove the containers really ran, poll `docker ps` by container/image ID during the run (a sibling session's container appears first if you match by name).

- [ ] **Step 7: Prove the fixture is not vacuous — two in-place data mutations, each reverted byte-exactly**

Run these in the MAIN tree (they touch only fixture data, and the revert is verified by an empty `git diff`), or in a scratch worktree if any subagent is running concurrently — a sibling's `git checkout` silently reverts an in-place mutation.

| # | Mutation | Expected |
|---|---|---|
| V1 | in BOTH yaml files, change `override_layer`'s `gate.layered: 0` to `gate.layered: 100` | probe `p8-two-layer-last-wins` REDs (`CATCH` expected, `P8-GATED` returned) — proves the fixture witnesses last-layer-wins precedence and not merely the base layer |
| V2 | in BOTH yaml files, change p9's `denominator: MILLION` to `denominator: HUNDRED` | the p9 cell no longer discriminates the denominator reading; confirm the probe still passes, then ALSO run the sharper form — change p9's `runtime_key` to an absent key `gate.absent.p9` so the 0/MILLION default decides: probe `p9-integer-is-numerator-over-hundred` REDs (`P9-GATED` expected, `CATCH` returned), proving p9's witness comes from the CONSULTED value and not from the default |

After each: `git checkout -- tests/fixtures/0088-runtime-fraction-route-gating/ && git diff --stat` must be EMPTY. Record each mutation's actual output in `PROGRESS.md` — including, if a mutation comes back GREEN, that fact and its interpretation (a GREEN usually means the mutation is misaimed, sometimes that the assertion is weak; both are findings).

- [ ] **Step 8: Create `README.md`.** Cover, in the house style of `tests/fixtures/0086-route-redirect-action/README.md`: what the fixture witnesses (the ten cells, in a table matching `expectations.yaml`'s comment block); that it is backend-free and CLUSTER-FREE and therefore locally runnable with no `{{BACKEND_IP}}` host-RED class; that `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL and that this makes it the second such pair of the 88 (a per-fixture claim, re-derived — never a tree property); **WHY there is no `node:` block** (upstream boot-fatal on the `cluster: y` spelling the other fixtures use, because YAML 1.1 booleanizes it — measured, with the upstream error text quoted); **WHY admin uses a literal `port_value: 0`** (`{{ADMIN_PORT}}` is not substituted for `Http1ProbeList`, per `driver_needs_admin_port`); which cells are DELIBERATELY absent and why (every `0 < v < 100` cell is boot-fatal under CF-109-1; CF-109-2/CF-109-3 likewise — witnessed in-process by 109.1, never here); and that spellings which are not Display-stable (`1e6`, `1.50`, `.nan`) must NEVER enter this or any fixture, because upstream renders runtime floats as raw SOURCE TEXT while envoy-rust renders `f64` Display (CF-108-5, ADR-0174).

- [ ] **Step 9: Task gate + commit**

Run: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (non-zero `Checking`), `cargo fmt --all -- --check`, and the fixture test once more (`1 passed`).

```bash
git add tests/fixtures/0088-runtime-fraction-route-gating/ tests/differential/tests/runtime_fraction_route_gating.rs
git commit -m "phase 109.2 task 2: differential fixture 0088 — ten runtime_fraction route-gating probes, byte-identical configs both sides"
```

---

### Task 3: The `BEHAVIOR_CONTRACT.md` `## Runtime` consumer subsection

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — insert INSIDE the existing `## Runtime` section (`:3162-3240` at plan time; the section ends where `## xDS wire state machine` begins at `:3241`), AFTER the `` **`GET /runtime`** `` table and BEFORE the paragraph beginning `**The nine \`runtime.*\` stats**`. **Locate both by that text.** The section has ZERO `### ` subheadings — it is organised as bold-lead paragraphs, so the new material must follow that convention (a bold lead-in, not a heading).

**Sources to transcribe FROM (X-5 — not from `109.2/SPEC.md`'s summary):**
- `docs/envoy-rust/phases/109-runtime-fraction-route-gating/SPEC.md` §1.1 (heading at `:28`) — the **13**-cell pick matrix.
- `docs/envoy-rust/phases/109.1-runtime-fraction-config-and-gate/SPEC.md` §1.2 (`:48-79`) — the **10**-cell V-8 closure matrix — and §1.3 (`:80-105`) — the evaluation cascade.

- [ ] **Step 1: Write the subsection.** It records five things, in this order:

  1. **A bold lead-in** naming the consumer: the route `match.runtime_fraction` gate, live since 109.1, evaluated by `RuntimeSnapshot::route_fraction_gate` inside `route_matches` at both production call sites (H2 inherits via the shared resolver), decided ONCE per lookup and process-lifetime-constant.
  2. **The 23-cell measured matrix** as ONE table with a `cell` column carrying the source labels (`1`…`13` from the parent §1.1; `B1`-`B3`, `F1`-`F4`, `N1`, `N2`, `S1` from `109.1` §1.2), a `default_value` column, a `consulted value` column, and a `measured result` column. Transcribe the readings verbatim from the two source tables, including the probe counts (30/30, 40/40, `27 GATED / 33 FALLBACK over n=60` for cell 5, `GATED 1/40` for F4). **Do NOT add the `edge:` rows from the unit test — those are SPEC §1.3-derived, not upstream-measured**, and claiming otherwise would be exactly the "a doc claim is an inherited census" failure this project keeps re-learning.
  3. **The §1.3 evaluation cascade**, verbatim in substance: parse `final_value` as `f64`; if it parses AND is finite — `v == 0` → never matches; `v >= 100` → the gate always passes and prefix/path/header matching applies unchanged; `0 < v < 100` → **boot-fatal** (CF-109-1); `v < 0` → use `default_value`. Otherwise (bools, non-numeric strings, the empty string, non-finite spellings) → `default_value`. The `default_value` itself must satisfy `FractionalPercent::selects_deterministic`: numerator `0` → never, numerator `== denominator.value()` → always, anything else boot-fatal (upstream also accepts `>` — the recorded, slightly-narrower divergence). Note the two load-bearing readings explicitly: **an integer runtime value is the numerator over HUNDRED, NOT over the default's denominator** (cell 9), and **an unparseable value falls back to `default_value` in BOTH directions** (cells 10/11).
  4. **The three reject-direction carry-forwards with their unblock conditions**, all OPEN, all landed REJECT-side-only by 109.1: **CF-109-1 (WIDENED)** — effective values strictly between 0 and 100 are boot-fatal, including non-integral floats and float-shaped strings, because upstream samples them per request and envoy-rust is deterministic-only; unblocked by a phase that lands per-request sampling. **CF-109-2** — a map-shaped value at (or beside) a CONSULTED key is boot-fatal, implemented as the SNAPSHOT-PREFIX rule (a consulted key `K` is fatal iff any snapshot entry starts with `K.`), because the store flattens maps to dotted keys so a plain lookup would silently fall back to the default where upstream honours the map (the CF-108-3 interlock); unblocked by a store that preserves map-shaped values. **CF-109-3** — `runtime_fraction` inside `jwt_authn.rules[].match` is boot-fatal because the hand-copied jwt matcher (`route_match_matches`, the CF-76-1 second matcher) never evaluates runtime gates and would silently ignore it — the ADR-0049 silent-inert class; unblocked by unifying the two matchers.
  5. **The fixture pointer**: `0088-runtime-fraction-route-gating` witnesses the deterministic subset cross-proxy (ten probes, byte-identical configs on both sides), and every nondeterministic or reject-direction cell is witnessed IN-PROCESS by 109.1 and by construction cannot appear in a fixture — a config that refuses to boot has no wire behaviour to compare.

- [ ] **Step 2: Verify the insertion did not disturb the section's neighbours**

Run:
```bash
grep -n '^## ' docs/envoy-rust/BEHAVIOR_CONTRACT.md | grep -A1 '## Runtime'
grep -c '^### ' docs/envoy-rust/BEHAVIOR_CONTRACT.md
git diff --numstat docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: `## Runtime` still immediately precedes `## xDS wire state machine`; the file's `### ` count is UNCHANGED at **24** (the new material uses bold lead-ins, not headings); the numstat shows insertions only (`N 0`) — a non-zero deletion count means existing contract text was overwritten, which this task must not do.

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 109.2 task 3: BEHAVIOR_CONTRACT ## Runtime consumer subsection — the 23-cell measured matrix, the evaluation cascade, CF-109-1/2/3 and the fixture-0088 pointer"
```

---

### Task 4: The decided-in 108.2-M-1 correction (three texts) + the stale `RuntimeFractionalPercent` doc

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — TWO sentences.
- Modify: `crates/envoy-admin/src/endpoint.rs` — ONE doc comment.
- Modify: `crates/envoy-config/src/bootstrap.rs` — ONE doc comment (the NEW finding, flagged below).

**The fact being recorded** (MEASURED at the 108.2 state-5 review, ADR-0176 DECISION 5): upstream `envoyproxy/envoy:v1.33.0` answers `POST /runtime` with **200 and the full runtime body** — and likewise `DELETE /runtime` and `POST /config_dump`. **It method-restricts NO read-only admin endpoint.** The control that makes the probe discriminating is `GET /runtime_modify` → 405, which reproduces. envoy-rust's 405-on-non-GET is the deliberate 06.1/08 house convention. So the true statement is an ASYMMETRY, not a bilateral rule: **envoy-rust 405s non-GET; upstream serves them** — a recorded, tree-wide, PRE-EXISTING and fixture-unwitnessed reject-direction divergence (every fixture speaks the matching method, so nothing goes red).

**Do NOT edit** the `/runtime_modify` sentence at `BEHAVIOR_CONTRACT.md:3233` — that describes a DIFFERENT, upstream-only endpoint (upstream POST-only, 405 on GET, envoy-rust 404s it), it is CF-108-2, and it is correct.

- [ ] **Step 1: Correct the `## Admin endpoint body shapes` table row.** In `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, locate the single line containing the text `POST → 405 \`allow: GET\` bilaterally.` (`:1379` at plan time; whole-file count of that string = **1**). Replace that ONE sentence in place — keeping every other clause of the row byte-identical — with a statement of the asymmetry: envoy-rust answers non-GET with 405 `allow: GET` (the house convention); upstream v1.33.0 serves `POST`/`DELETE /runtime` with 200 and the full body, method-restricting no read-only admin endpoint (MEASURED, 108.2 REVIEW M-1; control `GET /runtime_modify` → 405). Note that the divergence is reject-direction and fixture-unwitnessed.

- [ ] **Step 2: Correct the `## Runtime` `GET /runtime` paragraph.** Locate the text `GET-only (POST → 405` (`:3181-3182` at plan time; count = **1**) and reword to the same asymmetry, in that paragraph's voice. The `200 application/json; body is exactly two top-level keys` clause and the table that follows it must be left byte-identical.

- [ ] **Step 3: Correct the test doc in `crates/envoy-admin/src/endpoint.rs`.** Locate the text `GET-only on BOTH` (`:3319-3320` at plan time; count = **1**) — a `///` doc comment spanning `:3317-3320` on the test `runtime_post_is_method_not_allowed` (`:3322`). Reword so it says what the test actually proves: envoy-rust's OWN dispatch answers `POST /runtime` with 405 `allow: GET` and 404s `/runtime_modify`; upstream serves `POST /runtime` (200) and 405s `GET /runtime_modify`, so the 405 here is the house convention rather than a bilateral rule. **Do NOT change the test body or its assertions** — they pin envoy-rust's own behaviour and are correct.

- [ ] **Step 4 (NEW at this plan-write — an ADDITION to SPEC D4, flagged as such): narrow the stale `RuntimeFractionalPercent` doc.** In `crates/envoy-config/src/bootstrap.rs`, locate the doc comment above `pub struct RuntimeFractionalPercent` (`:1497-1501` at plan time) containing the text `a present \`runtime_key\` is rejected`. That was true when CSRF was the type's only consumer; since 109.1 the ROUTE consumer HONOURS `runtime_key` through `RuntimeSnapshot::route_fraction_gate`, so a reader arriving from `RouteMatch.runtime_fraction` reads a false statement about the field they are using. Narrow it to: the CSRF consumer still rejects a present `runtime_key` (ADR-0061 L6); the ROUTE consumer honours it, deterministically, per the 109.1 cascade. **Do NOT touch the CSRF validator, the test `runtime_key_is_rtds_inert`, or any other consumer.** This step is scope the SPEC does not name — it is included because it is the same class as 109.1's Task-7 D7 narrowing, on the same surface, in the task that already corrects three doc sentences about it. Dropping it costs nothing.

- [ ] **Step 5: Verify the old wordings are GONE and nothing else moved**

Run:
```bash
git grep -n 'GET-only on BOTH\|GET-only (POST\|allow: GET` bilaterally' -- docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/
git grep -c 'runtime_modify' docs/envoy-rust/BEHAVIOR_CONTRACT.md
cargo test -p envoy-admin --lib runtime_post_is_method_not_allowed
git diff --numstat
```
Expected: the first grep returns **ZERO** hits. The `/runtime_modify` mentions are unchanged in count. The named test still reports `1 passed` (doc-only edits change no behaviour). The numstat touches exactly the three (or four, with step 4) named files.

- [ ] **Step 6: Task gate + commit**

Run: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (non-zero `Checking`), `cargo fmt --all -- --check`.

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md crates/envoy-admin/src/endpoint.rs crates/envoy-config/src/bootstrap.rs
git commit -m "phase 109.2 task 4: correct the measured-false bilateral-405 claim (108.2 REVIEW M-1, decided-in per ADR-0176 D5) and narrow the stale RuntimeFractionalPercent doc"
```

---

### Task 5: State-3 exit gate

State 4 owns the formal §7.5 sweep in a SEPARATE session (§5.1; ADR-0127 — the context that wrote the code must not grade it). This task is the state-3 exit bar and the honest hand-off.

- [ ] **Step 1: Run the full gate set from the REPO ROOT**

```bash
cd /home/esa/git/envoy-rust
cargo build --workspace --all-targets            > /tmp/109_2-build.log 2>&1
cargo clippy --workspace --all-targets --all-features -- -D warnings > /tmp/109_2-clippy.log 2>&1
cargo fmt --all -- --check
cargo test --workspace --no-fail-fast            > /tmp/109_2-sweep.log 2>&1
cargo deny check                                 > /tmp/109_2-deny.log 2>&1
```
Never pipe these through `tail` — redirect, then inspect the file. Gate the build on a non-zero `Compiling` count and clippy on a non-zero `Checking` count (both caches were measured cold-no-op simultaneously — exit 0 alone is not evidence). Gate `cargo deny` on its exit code plus the `advisories ok, bans ok, licenses ok, sources ok` line; it emits `license-not-encountered` warnings on a fully-green run, so a loose `warning` grep false-positives.

- [ ] **Step 2: Run the sweep TWICE and diff the failing SET**

Census recipe: `grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed' /tmp/109_2-sweep.log` then awk fields **4** and **6**; assert the BINARY count separately. Enumerate the failing test names from the `---- <name> stdout ----` markers, never by indentation.

Expected: the ADR-0164 five-member deterministic LOCAL core (four `access_log_*_upstream_reset` + `admin_config_dump_server_info` — CI passes these) plus an open-ended startup-race tail. **Classify by ISOLATION only** — re-run each failure alone; passes-in-isolation ⇒ tail, fails-in-isolation ⇒ core. The tail's SIZE carries no signal in either direction, and a two-sweep intersection is NOT a classifier (it was measured disagreeing with isolation). The identity to close: `local passed + failed` must equal the CI figure, **2194** after this slice (2193 + the ONE new fixture test), over **165** binaries.

- [ ] **Step 3: Run the new fixture in isolation and record its timing**

`cargo test -p differential --test runtime_fraction_route_gating` → `1 passed`. Record cold and warm durations in `PROGRESS.md`.

- [ ] **Step 4: Append the state-3 session summary to `PROGRESS.md`** — per-task commits, every deviation from this plan RECORDED (this plan is not edited once execution starts; the deviation ledger is the record — and the ledger's completeness is itself reviewed, so an unrecorded deviation is a finding), the measured net LoC by `git diff --numstat` against the base commit (do NOT write "≈ the projection" without measuring — that exact sentence was the 109.1 M-4 finding), and the flake classification with its isolation evidence.

- [ ] **Step 5: Final commit**

```bash
git add docs/envoy-rust/phases/109.2-runtime-fraction-fixture-and-contract/PROGRESS.md
git commit -m "phase 109.2 task 5: state-3 exit gate — full sweep, fixture 0088 green in isolation, PROGRESS session summary"
```

---

## Self-review (run at plan-write, recorded)

1. **Spec coverage.** SPEC §2 D1 (fixture `0088` — four files) → Task 2 steps 3/4/5/8. D2 (`expectations.yaml`: status + body per probe, no header assertions beyond the driver defaults, no stats assertions) → Task 2 step 5 — honoured: `expected_headers` uses only the existing `set_equal_modulo_allow_list` default and there are ZERO stat assertions (the nine `runtime.*` stats are startup-set and unmoved; `0087` already witnesses them). D3 (the `## Runtime` consumer subsection: 23-cell matrix + cascade + CF-109-1/2/3 + fixture pointer) → Task 3. D4 (the M-1 correction, three texts located by WORDS) → Task 4 steps 1/2/3. D5 (parent close) → NOT this plan: it is the state-6 close-out's two-row flip, and this plan flips no status cell, by design. SPEC §3 (differential surface) → Tasks 2 + 5. SPEC §4 X-1…X-5 → all five closed in the PLAN-VERIFY section above, two of them REFUTING the SPEC. **No gap found. One deliberate addition** (Task 4 step 4), flagged in the banked-findings table rather than slipped in.
2. **Placeholder scan.** No TBD/TODO/"implement later"/"handle edge cases" remains. Every code and YAML block is complete and was executed at the plan-write, not merely inspected. Task 3 and Task 2 step 8 specify prose deliverables by their required CONTENT (five enumerated items; nine enumerated topics) rather than by transcribing ~200 lines of finished Markdown — the only two places this plan describes rather than dictates, and both are documentation whose exact wording is the executor's to write against sources this plan names by path and line.
3. **Type consistency.** The fixture directory name `0088-runtime-fraction-route-gating` matches the test file `runtime_fraction_route_gating.rs` and the `PathBuf::join` argument in it (the convention: drop the `NNNN-` prefix, hyphens → underscores). `kind: http1_probe_list`, `method: get`, `expected_body: { kind: byte_exact, body: … }`, `expected_headers: set_equal_modulo_allow_list`, `response_status: exact`, `response_body: { kind: byte_exact }` are the exact serde spellings read off `Driver`/`Http1Probe`/`Http1BodyRule`/`Http1HeaderRule`/`StatusRule` and cross-checked against the landed `0083` fixture. Task 1's three rows use only helpers (`snap`, `one`, `rf`, `empty`, `Hundred`, `Million`, `Always`, `Never`) that already exist in the enclosing test fn. Every probe `path:` in `expectations.yaml` has a matching route `prefix:` in the YAML, and every `expected_body` matches either that route's `inline_string` or the catch-all's `CATCH`.
4. **Known risks, priced.** (a) *The dry-run is not a harness run.* The ten cells were driven by hand against both proxies; `run_fixture` adds its own accept-ready wait, container lifecycle and header comparison. Task 2 steps 2/6 make the harness the authority. (b) *Header set-equality is the least pre-flighted assertion* — `expected_headers: set_equal_modulo_allow_list` was NOT exercised by the hand-driven dry-run. It is the standard setting on all 13 existing probe-list fixtures over `direct_response` routes, so the risk is low; if it REDs, the honest response is to diagnose the header diff, not to weaken the assertion, and never to add a name to the 3-entry `HEADER_ALLOW_LIST`. (c) *`0.0.0.0` as the listen address on the envoy-rust side* — this fixture uses it on BOTH sides (that is part of what makes them byte-identical), where `0083`/`0086` use `127.0.0.1` on the subject side; the debug `envoy-bin` accepted it and served all ten probes at the plan-write, but it is the one structural departure from the existing probe-list fixtures and is the first thing to suspect if the subject side fails to become accept-ready under the harness. (d) *Backend-routing host-RED does not apply* — the fixture is backend-free with no `{{BACKEND_IP}}` marker, so it is fully verifiable on a developer host; CI remains authoritative for everything else.
