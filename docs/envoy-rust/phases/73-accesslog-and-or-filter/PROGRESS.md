# Phase 73 — access-log `and_filter` / `or_filter` — §5 state-3 implementation log

> **State-3 implementation** (`superpowers:executing-plans` + `test-driven-development`).
> This session implemented `PLAN.md`'s 8 TDD tasks IN ORDER (the ordering is
> load-bearing), each RED-first. It does NOT run the full §7.5 gate (that IS the
> §5 state-4 verification, a SEPARATE later session), but it DID run each task's
> own unit tests to green and (per the PLAN T5/T6 steps) both new Docker
> differentials in isolation. `cargo fmt --all --check` + targeted `cargo clippy`
> on the three touched crates were run as a state-4 de-risk (a stray fmt diff was
> auto-fixed and committed). No ADR fired (the §6.2 reconciliation ADR-0153 was
> the state-2 output; ADR-0154 stays UNFIRED — no split).

## Execution approach

The DAG is near-linear: **T1→T2** are serial in `bootstrap.rs`; **T4** gates on
T1 (config fields) + T3 (runtime variants); **T5/T6** gate on all code + a rebuilt
`envoy-bin`; the cargo lock serializes any test run. The genuinely-independent
tasks (T3 ~25 LoC, T7 a single seed file, T8 a doc insert) were too small to
justify worktree-subagent coordination overhead, so the main session implemented
all eight directly, task-by-task, per the plan's own §5.1 guidance ("fully linear
DAG, or coordination cost > win → do not parallelize"). Line numbers in the tree
matched `PLAN.md` at start (struct ~723, destructure 5172, `validate_header_matcher`
5403, `AmbiguousAccessLogFilter` lib.rs:458, compile 1745, LogFilter enum 44, all
6 hcm construction sites) — no drift.

## STEP 0/0.5 confirmation

- `git status` clean; branch `main`; `HEAD` = the state-2 PLAN-write commit
  `23ec5aa67b283cf02e4649984991d5eeced485c3`; `git fetch origin --prune` showed no
  sibling had advanced.
- CI on `23ec5aa…` = `completed success` (run `29877696877`) — the docs-only
  PLAN-write commit.

## Per-task log (each RED-first → GREEN → commit)

### T1 — config structs + 5-arm cardinality destructure — commit `78269e2`

- **RED:** added `and_or_filter_deserialize_round_trip_and_default` +
  `and_filter_alongside_header_filter_is_ambiguous` to `bootstrap.rs` tests.
  `cargo test -p envoy-config --lib and_` → compile error (`cannot find type
  AndFilter`; `struct AccessLogFilter has no field named and_filter/or_filter`) —
  6 errors, as expected.
- **GREEN:** added `pub and_filter: Option<AndFilter>` + `pub or_filter:
  Option<OrFilter>` fields; the `AndFilter`/`OrFilter { filters: Vec<AccessLogFilter> }`
  structs (`#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
  #[serde(default, deny_unknown_fields)]`, NO `Box`, NO `Clone`); re-exported both
  from `envoy_config::{AndFilter, OrFilter}` (sorted slots in `lib.rs`); widened
  the `validate_access_logs` destructure + `set_arms` array to 5 arms (cardinality
  only); added `and_filter: None, or_filter: None` to all 6 `AccessLogFilter{…}`
  construction sites in `hcm.rs` (`4524`/`4668`/`4741`/`4772`/`4909`/`4926`).
  `cargo test -p envoy-config --lib and_` → `41 passed; 0 failed`; the two target
  tests confirmed individually (`1 passed` each). `cargo build -p envoy-config -p
  envoy-http1` → clean.

### T2 — recursive validator + `filters.len() >= 2` fail-loud — commit `7f8dfae`

- **RED:** added `file_log_with_filter`/`exact_header` helpers + 5 tests
  (`and_filter_with_one_child_is_rejected`, `empty_and_filter_is_rejected`,
  `or_filter_with_two_children_is_accepted`, `nested_bad_leaf_surfaces_through_recursion`,
  `nested_composition_cardinality_surfaces_through_recursion`). `cargo test -p
  envoy-config --lib` → compile error (`no variant InsufficientCompositeFilters`),
  3 errors.
- **GREEN:** added `ConfigError::InsufficientCompositeFilters { count: usize }`
  after `AmbiguousAccessLogFilter` (`lib.rs`); extracted the inline per-filter body
  into a recursive `&mut`-taking `validate_access_log_filter` helper (5-arm
  destructure + cardinality + the three leaf checks + `filters.len() >= 2` for
  each composition arm + a recursive descent into every child via `for child in
  af.filters.iter_mut() { validate_access_log_filter(child)? }`); replaced the
  inline body with a single `validate_access_log_filter(filter)?` call; updated the
  `validate_access_logs` doc-comment item 3 (M70-R1 no-`..` rationale now lives on
  the helper). `cargo test -p envoy-config --lib` → `636 passed; 0 failed`; all 5
  T2 tests confirmed by name.

### T3 — `LogFilter::And`/`Or` runtime variants + `should_log` all/any — commit `07e9938`

- **RED:** added `and_or_should_log_all_any_and_empty_boundary` to
  `filter.rs` tests (uses `ge`/`le` status-code children — no header stub; covers
  AND=all, OR=any, nested recursion, and the empty-vec boundary all([])=true /
  any([])=false). `cargo test -p envoy-accesslog --lib …` → compile error (`no
  variant And/Or`), 6 errors.
- **GREEN:** added `And(Vec<LogFilter>)` / `Or(Vec<LogFilter>)` variants (NO `Box`,
  NO `Eq`/`PartialEq`, NO `envoy-config` dep — ADR-0150 holds) + the two
  `should_log` arms (`filters.iter().all(…)` / `.any(…)`, recursing with the same
  `(status, response_flags, headers)`). `cargo test -p envoy-accesslog --lib` →
  `109 passed; 0 failed`.

### T4 — recursive 5-tuple `compile_access_log_filter` — commit `8a3224a`

- **RED:** added `compile_access_log_filter_builds_composition_arms_recursively`
  (a flat `and_filter{[x-a,x-b]}` → `And([Header,Header])` + a depth-2
  `or_filter{[and{[x-a,x-b]}, header{x-c}]}` → `Or([And,Header])`, asserting the
  keep/drop over concrete header slices). `cargo test -p envoy-http1 --lib …` →
  FAILED (panicked at `unreachable!("… exactly one filter arm is set")` — the
  3-tuple match ignored `and_filter`), as expected.
- **GREEN:** widened the match to the 5-tuple `(scf, rff, hf, af, of)`, adding the
  two composition arms mapping children via
  `af.filters.iter().map(compile_access_log_filter).collect()`; updated the fn
  doc-comment to "five arms ship". `cargo test -p envoy-http1 --lib` → `183 passed;
  0 failed`; the composition test + the existing arm tests confirmed green.

### T5 — differential fixture `0079-accesslog-and-filter` — commit `6007bfe`

- Created `envoy-rust.yaml` / `envoy.yaml` / `expectations.yaml` / `README.md` +
  the `access_log_and_filter.rs` entrypoint (cloned from `0078`). Format is the
  allow-listed `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%\n` (ADR-0153 PV-6 — a
  `%REQ(X-A)%` format is boot-fatal). `and_filter { [header{x-a=1},
  header{x-b=1}] }`; probes kept-LAST (`x-a:1`→DROP, `x-a:1 x-b:1`→KEEP).
- **Boot check** (`envoy-bin -c …`, {{PORT}}→19079): booted OK, no `ConfigError`
  (the recursive `and_filter` parses + validates + the listener binds).
- **Differential (isolation):** `cargo build -p envoy-bin` (rebuilt the debug
  binary the harness runs) then `cargo test -p differential --test
  access_log_and_filter` → `test access_log_and_filter ... ok` (`1 passed`,
  3.27s). One byte-identical `STATUS=200 PATH=/x` line cross-proxy.

### T6 — differential fixture `0080-accesslog-or-filter` (depth-2) — commit `de4aee3`

- Created the four fixture files + `access_log_or_filter.rs`. Filter is the
  depth-2 `or_filter { [ and_filter{[x-a=1, x-b=1]}, header_filter{x-c=1} ] }`
  (witnessing the recursion differentially, SPEC R-0.5); three probes kept-LAST
  (`x-a:1`→DROP, `x-a:1 x-b:1`→KEEP nested-AND, `x-c:1`→KEEP leaf).
- **Boot check** ({{PORT}}→19080): booted OK, no error.
- **Differential (isolation):** `cargo test -p differential --test
  access_log_or_filter` → `test access_log_or_filter ... ok` (`1 passed`, 12.65s).
  TWO byte-identical `STATUS=200 PATH=/x` lines cross-proxy.

### T7 — `parse_bootstrap` fuzz seed + `!`-un-ignore — commit `1f90b13`

- Created `crates/envoy-config/fuzz/corpus/parse_bootstrap/and_or_filter.yaml` (a
  depth-2 `or_filter{[and_filter{[…]}, header_filter{…}]}` exercising both arms +
  the recursion) and added `!corpus/parse_bootstrap/and_or_filter.yaml` to
  `crates/envoy-config/fuzz/.gitignore` (next to the sibling `header_filter.yaml`).
  NO new fuzz target / `ci.yml` edit (ADR-0137 config-sub-message precedent; PV-7
  — the existing `parse_bootstrap` CI step covers the new seed).
- **Verified tracked** (`git ls-files …/and_or_filter.yaml` prints the path) and
  **parses+validates** (`envoy-bin -c` on the seed with port→19099 booted OK, no
  error). The full 30s nightly `cargo fuzz` run is the state-4/CI gate.

### T8 — `BEHAVIOR_CONTRACT.md` subsection — commit `e8a17c2`

- Inserted the phase-73 `and_filter`/`or_filter` subsection (§A schema/min_items,
  §B decision/ADR-0150-holds/recursion, §C mutual exclusion across the 5 arms, §D
  format-allow-list note, §E authoritative fixtures 0079/0080) as a sibling to the
  phase-70/71/72 access-log-filter subsections, before the `## xDS wire state
  machine` `---`.

### fmt de-risk — commit `41c9ee7`

- `cargo fmt --all -- --check` flagged two rustfmt-canonical wraps (my T1 lib.rs
  re-export block + a T2 test `let err =` line); `cargo fmt --all` applied them;
  re-ran the three touched crates' lib tests (`109` + `636` + `183` passed).
  `cargo clippy -p envoy-config -p envoy-accesslog -p envoy-http1 --all-targets
  --all-features -- -D warnings` → clean.

## Load-bearing invariants preserved

- **ADR-0150 seam:** `envoy-accesslog` still does NOT depend on `envoy-config`;
  `LogFilter` still has NO `Eq`/`PartialEq`; the new `And`/`Or` variants recurse
  through `Vec<LogFilter>` (NO `Box`) and add no equality derive / no config dep.
- **No `Box`** at either layer (config `AccessLogFilter` and runtime `LogFilter`
  both recurse through `Vec<_>`). `AccessLogFilter` still does not derive `Clone`;
  `AndFilter`/`OrFilter` match.
- **One new `ConfigError` variant** (`InsufficientCompositeFilters`); nested leaf
  failures reuse the existing variants through the recursion.
- **Fixtures use `%REQ(:PATH)%`, never `%REQ(X-A)%`** (ADR-0153 PV-6).
- The **32** pre-existing access-log fixtures (incl. `0076`/`0077`/`0078`) and
  `known-failures.txt` are untouched; `0079`/`0080` are NEW.
- `#![forbid(unsafe_code)]` holds; no `unsafe`.

## Carry-forwards (unchanged from ADR-0153)

- **M71-3** NOT folded (needs a dedicated all-drop fixture — `0079`/`0080` both
  keep ≥1 line per the kept-LAST convention). **CF-73-1** OPENED (arbitrary nesting
  depth, no stack guard — parity with upstream, deferred non-goal). CF-72-1 /
  CF-72-2 / M71-6/7/8 / M70-R4/R9 / M69-A..I / CF-69-1/2/3/5 / M68-1 / M-1 /
  CF-67-3/5/6/7 / the older Minors + HTTP-filters-family (1)–(4) — all untouched.

## Deferred to §5 state-4 verification (a SEPARATE later session, per §7.5)

- (a) `0079`/`0080` green + (b) all `0001`–`0078` still green — the FULL Docker
  differential sweep (this session ran only `0079`/`0080` in isolation).
- (c) no new conformance suite; (d) the `parse_bootstrap` fuzz short-budget CI run;
- (e) `cargo build --workspace --all-targets` + workspace `cargo clippy` +
  `cargo fmt --all --check` + `cargo test --workspace` + `cargo deny check`;
- (f) `REVIEW.md` approved (state-5).

## Result

All 8 `PLAN.md` tasks implemented (each RED-first, unit tests green); both new
differentials pass locally in isolation; fmt + touched-crate clippy clean. Next:
the §5 state-4 verification (a SEPARATE session — do NOT chain).

## §5 state-4 verification (2026-07-22, this session)

Full §7.5 (a)-(f) gate run SOLO-SERIAL on `HEAD == f1d4c1a` (the state-3 ledger
commit; docs-only session, no code change — the gate grades the state-3 code).
Every command's raw output quoted below. **VERDICT: GREEN** — every local RED
adjudicated to a documented host-flake or a pass-in-isolation parallel-load flake.

**STEP 0/0.5 — state + CI confirmation:**
- `git status --porcelain` → clean; branch `main`; `HEAD f1d4c1a7ca51d5d821be37faa61b09a8c8ccaeb2`.
- `git fetch origin --prune` → no sibling advance; `REVIEW.md` still ABSENT (genuinely state-4).
- `gh run list --commit f1d4c1a7ca51d5d821be37faa61b09a8c8ccaeb2` →
  `completed  success  ci  main  push  29880724936  8m4s` — the authoritative
  full-workspace + Docker-differential run for the state-3 code is GREEN on CI.

**(a) `cargo fmt --all -- --check`** → exit 0 (no diff).

**(e) `cargo clippy --workspace --all-targets --all-features -- -D warnings`** →
exit 0. `Finished dev profile … in 4.10s` (all crates checked, zero warnings).

**(e) `cargo build --workspace --all-targets`** → exit 0. `Finished dev profile … in 8.57s`.

**`cargo build -p envoy-bin`** (debug binary the differential runs, memory
`differential-harness-uses-debug-envoy-bin`) → exit 0. `Finished dev profile … in 1.76s`.

**(e) `cargo deny check`** → exit 0. `advisories ok, bans ok, licenses ok, sources ok`
(3 benign `license-not-encountered` warnings for unmatched allowances — pre-existing).

**(a)(b)(c) `cargo test --workspace --no-fail-fast`** — run twice (diff-the-set,
memory `local-red-set-varies-run-to-run`). Stable total **2076** both runs:
- RUN 1: exit 101 — **2069 passed; 7 failed**. Failing set:
  `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset`,
  `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`,
  `access_log_json_nested`, `admin_config_dump_server_info`, `admin_ready_fixture`.
- RUN 2: exit 101 — **2070 passed; 6 failed**. Failing set:
  the 4× `*_upstream_reset`, `admin_config_dump_server_info`,
  `outlier_detection_ejects_then_un_ejects`
  (`access_log_json_nested`/`admin_ready_fixture` GREEN this run; `outlier_detection` NEW).

Adjudication (each RED = documented host-flake, CI-authoritative, NOT a regression):
- **4× `access_log_*_upstream_reset`** — deterministic in both runs → documented
  `tcpclosebackend-ipv6-unreachable-host-flake` (real Envoy can't reach the
  host-spawned close backend on this host; reports UF instead of a reset).
- **`admin_config_dump_server_info`** — deterministic (run1+run2+2× isolation, exit 101);
  the `/clusters` diff shows all 18 `envoy-only` backend rows keyed on
  `backend::192.168.65.2:…` → documented `differential-host-bridge-ip-192-168-65-2`
  flake (this host routes the backend via 192.168.65.2, not the allow-listed IP).
- **`access_log_json_nested`** (`--test access_log_json_nested`) → exit 0, 1 passed —
  parallel-load flake (run1 symptom "upstream Envoy never became accept-ready").
- **`admin_ready_fixture`** lives in binary `admin_ready` (NOT its own target —
  `--test admin_ready_fixture` merely lists targets, memory
  `cargo-test-p-name-false-green-filtered-out`). `--test admin_ready` → exit 0, 1 passed —
  parallel-load startup-race flake.
- **`outlier_detection_ejects_then_un_ejects`** lives in `crates/envoy-bin/tests/upstream_outlier_detection.rs`.
  `cargo test -p envoy-bin --test upstream_outlier_detection …` → exit 0, 1 passed — parallel-load flake.
- **NEW fixtures GREEN in isolation:** `--test access_log_and_filter` (0079) → exit 0, 1 passed;
  `--test access_log_or_filter` (0080) → exit 0, 1 passed.
- Cross-check (memory `local-red-set-varies-run-to-run`): total 2076 is stable; only
  the pass/fail split shifts; the deterministic core (4 reset + admin_config_dump) is
  documented-environmental; the variable tail each passes in isolation; and CI is GREEN
  on the identical state-3 code (run `29880724936`). No real regression.

**(d) `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=60`** (from
`crates/envoy-config`, memory `cargo-fuzz-runs-from-crate-dir-not-repo-root`) →
exit 0. `Done 180100 runs in 102 second(s)` — no crash. The new
`fuzz/corpus/parse_bootstrap/and_or_filter.yaml` seed is git-tracked (`git ls-files`
confirms) and exercised by the existing CI fuzz step (ADR-0137, no new target).

**(c)** no new conformance suite this phase. **(f)** `REVIEW.md` is the state-5 gate
(next session).

**Result:** §7.5 (a)-(e) VERIFIED GREEN. No §5.2 re-entry needed. Advance STATE to
phase-73 state-4 complete; next = §5 state-5 code-review (a SEPARATE session).
