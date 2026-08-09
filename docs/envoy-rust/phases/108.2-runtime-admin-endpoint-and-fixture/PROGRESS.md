# Sub-phase 108.2 — implementation progress (§5 state 3)

> One entry per PLAN.md task, appended by the state-3 implementation session
> as each task lands (TDD-first, one commit per task). PLAN.md §0's pre-flight
> numbers were treated as CLAIMS and re-established here.

## Task 1 — `AdminEndpoint::Runtime` (envoy-admin)

- **RED (verified):** with the two test-support consts
  (`RUNTIME_TWO_LAYER_BOOTSTRAP`, `RUNTIME_SCALARS_BOOTSTRAP`) and the 6-test
  `mod runtime_tests` appended, `cargo test -p envoy-admin runtime_tests`
  failed to compile with exactly the predicted forcing error, 4×:
  `error[E0599]: no variant or associated item named 'Runtime' found for enum
  'endpoint::AdminEndpoint'`.
- **GREEN:** after the variant + the three dispatch arms (`from_path`,
  `allowed_method` — first compile-forcing site, `render_with` — second) +
  `RuntimeBody`/`RuntimeEntryBody`/`render_runtime` (via `json_pretty_200`,
  no new response plumbing) + the two convention-test rows
  (`get_known_path_returns_endpoint`, `each_endpoint_declares_its_allowed_method`):
  `cargo test -p envoy-admin` → `test result: ok. 103 passed; 0 failed`
  (97 baseline + 6 new — matches the plan's stated count).
- Both new consumers call `RuntimeSnapshot::from_bootstrap` only (DD-2 / M-5);
  the M-6 positive `/config_dump` pin
  (`config_dump_serializes_layered_runtime_positively`) landed in this task.
- **Boundary gate:** `cargo fmt --all -- --check` clean *after* one
  `cargo fmt --all` pass — the PLAN's literal Rust was NOT fully fmt-canonical
  as transcribed: rustfmt reflowed 3 long lines in the new tests
  (`!resp.headers.iter().any(...)`, the `/runtime_modify` `assert_eq!`, the
  `.get("layered_runtime").is_none()` chain). Formatting-only deviation,
  behavior identical. `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` exit 0 with 2 `Checking` lines (non-zero → not a cached
  no-op).
- **Anchors:** re-derived by text pre-edit; `endpoint.rs` was 3091 lines at
  task start (identical to the plan's measurement — no drift since `ced6802`).
- **Commit:** `phase 108.2 task 1: admin GET /runtime — the eleventh endpoint`

## Task 2 — the nine `runtime.*` stats (envoy-bin)

- **RED (verified, with a shape correction):** with
  `crates/envoy-bin/src/runtime_stats.rs` created but NOT declared in
  `main.rs`, `cargo test -p envoy-bin --bin envoy-bin runtime_stats` printed
  `test result: ok. 0 passed; 0 failed; ... 37 filtered out` — NOT the plan's
  predicted `E0583`: an undeclared `.rs` file is simply never compiled, so
  there is no error, only the absence of the tests. Per the standing
  `0 passed; N filtered out` trap this is a false GREEN by exit code and the
  true RED by count: the two tests did not exist for the build.
- **GREEN:** after `mod runtime_stats;` (following `mod network_rbac;`) and
  the `register_runtime_stats(&bootstrap, &registry)` call directly after
  `register_rds_stats` in `main.rs`:
  `cargo test -p envoy-bin --bin envoy-bin runtime_stats` →
  `test result: ok. 2 passed; 0 failed` (the plan's stated count).
- Kinds pinned per DD-6 (4 gauges / 5 counters); both value tables asserted;
  `num_keys` flattened-leaf semantics witnessed via `nested.deep` (4, not 3);
  registration calls `RuntimeSnapshot::from_bootstrap` only (DD-2).
- **Boundary gate:** fmt needed one reflow pass again (the `num_layers`
  assert line in the loop — same formatting-only deviation class as Task 1);
  then `cargo fmt --all -- --check` clean; clippy exit 0, 1 `Checking` line
  (non-zero).
- **Commit:** `phase 108.2 task 2: the nine runtime.* stats`

## Task 3 — `expected_stats` harness extension + fixture `0087` data files

- **RED (verified):** with the parse test
  (`fixture_0087_expectations_parses_as_admin_scrape_with_expected_stats`)
  inserted and the three fixture data files written,
  `cargo test -p differential --lib fixture_0087` failed with exactly the
  predicted `error[E0026]: variant 'Driver::AdminScrape' does not have a
  field named 'expected_stats'`.
- **GREEN:** after the four lib.rs edits — (a) the `#[serde(default)]`
  `expected_stats: Vec<KeepAliveExpectedStat>` field, (b) the dispatch
  destructure + pass-through, (c) the widened 8-arg `run_admin_scrape_arm`
  with `#[allow(clippy::too_many_arguments)]` + justification (DD-7), (d)
  the STEP 3.5 `assert_expected_stats_bilaterally` call between the scrape
  loop and STEP 4 — `cargo test -p differential --lib` →
  `test result: ok. 162 passed; 0 failed; 2 ignored`. **Count-mismatch
  investigated and resolved:** the plan's "164" is the TOTAL (the 2 ignored
  are pre-existing Docker-gated tests); measured baseline at the task-2
  commit is `161 passed; 2 ignored` = 163 total, so the delta is exactly the
  +1 new parse test (163 → 164 total). The new test passes
  (`1 passed; 163 filtered out` on the name filter).
- Fixture `0087` data files carry the plan's measured transcript verbatim:
  14 entries (every scalar a quoted YAML string), `layers`
  `["base_layer","override_layer"]`, nine `expected_stats` with exactly the
  four non-zero witnesses first, subtree anchors at single-segment `entries`
  / `layers` only (DD-8). `envoy-rust.yaml` uses the NAME-ONLY echo spelling
  (DD-3); zero clusters, zero backends.
- **Boundary gate:** fmt clean on first check (no reflow this time); clippy
  exit 0, 1 `Checking` line (non-zero).
- **Commit:** `phase 108.2 task 3: AdminScrape expected_stats + fixture 0087 data`

## Task 4 — differential test binary + the LOCAL fixture run + mutation checks

- `tests/differential/tests/runtime_static_layer.rs` created (the 87th
  differential test file / 164th workspace test binary).
- `cargo build -p envoy-bin` FIRST (fresh debug binary — the stale-binary trap).
- **The SPEC §5 LOCAL RUN, RECORDED:**
  `cargo test -p differential --test runtime_static_layer -- --nocapture` →
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered
  out; finished in 7.88s` (first run; container start included). Subsequent
  runs 1.15-1.22 s — inside the plan's ~1-3 s window. Backend-free and
  cluster-free, so fully verifiable on this host; Docker daemon was up
  (`docker ps` clean). Transcript at scratchpad `fixture-0087-run1.log`.
- **Mutation checks (in-place data mutations, serial, no parallel subagents):**
  - (a) `shared.key.final_value` `"from_override"` → `"from_base"`: RED with
    a real `test result: FAILED. 0 passed; 1 failed` line and the failure
    text naming the witness — `required_subtree "entries" envoy != expected`
    (the UPSTREAM side rejected the mutated expectation; the assertion
    reaches a real assertion, not a compile/startup failure).
  - (b) revert (a); `runtime.num_keys` `value: 14` → `13`: RED with
    `upstream stat runtime.num_keys expected 13 got 14` — the wrong
    flattened-leaf count fails loudly (bilateral: upstream checked first).
  - (c) revert (b); `git diff --stat` EMPTY (byte-exact restore of the
    tracked expectations.yaml); rerun → GREEN `1 passed` (the unmutated
    control from the same tree).
- **Boundary gate:** rustfmt reflowed the `run_fixture(...).await.expect(...)`
  chain in the new test file (same formatting-only deviation class as Tasks
  1-2); after `cargo fmt --all`: check clean, fixture still green
  (`1 passed`, 1.15s). Clippy exit 0, 1 `Checking` line.
- **Commit:** `phase 108.2 task 4: fixture 0087 differential test + local green`

## Task 5 — `BEHAVIOR_CONTRACT.md` `## Runtime` + admin row + stat mapping + fixture README

- Three contract edits, all located BY TEXT: the `## Runtime` section
  inserted immediately before `## xDS wire state machine`; the `/runtime`
  row appended to `## Admin endpoint body shapes` after the
  `/healthcheck/ok` row; the `**108.2 entries:**` three-row block appended
  at the end of `## Stat-name mapping` (before its closing `---`). All three
  texts transcribed verbatim from PLAN.md Task 5.
- `tests/fixtures/0087-runtime-static-layer/README.md` created (the witness
  ledger, the exclusion list, and the measurement provenance).
- The fixture `0011` prose correction is RECORDED in the contract section
  (fixture NOT edited — D-3.5).
- **Insurance run:** `cargo test -p differential --test runtime_static_layer`
  still `1 passed` after the docs-only edits.
- **Commit:** `phase 108.2 task 5: BEHAVIOR_CONTRACT ## Runtime + fixture 0087 README`

## Task 6 — gate-(d) record + the full regression sweep

- **Gate (d), RECORDED EXPLICITLY (SPEC §9(d) — record, don't silently
  skip):** this slice adds NO fuzz target and NO corpus seed, and `ci.yml`
  needs NO new step. The slice touches no parser: the `layered_runtime`
  parser landed (and was fuzz-covered) in 108.1; this slice only renders and
  counts the already-parsed store. The pre-existing `parse_bootstrap`
  short-budget CI run covers the only parser surface adjacent to this work.
- **Full workspace sweep** (`cargo build --workspace --all-targets` clean,
  then `cargo test --workspace --no-fail-fast`, full redirect to
  `sweep-108-2.log`, censused by the `---- <name> stdout ----` markers and
  the `(ok|FAILED)` awk-4/6 recipe):
  - **binaries 164** (163 + `runtime_static_layer`) — matches the plan.
  - **passed=2174, failed=6, passed+failed=2180 = 2170 (CI baseline, run
    31260569093 on `ced6802`) + 10 new** (6 envoy-admin + 2 envoy-bin +
    1 differential-lib + 1 fixture binary) — the identity HOLDS.
  - Failures: the deterministic five-member host-flake core (ADR-0164:
    `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset`,
    `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`,
    `admin_config_dump_server_info`) plus ONE tail member:
    `admin_ready_returns_200_post_migration` (`WouldBlock` driving `/ready`
    — an `admin_*` name overlapping this phase's surface, so it was
    classified by TEXT and ISOLATION per the standing rule: the text is a
    socket readiness race, not a `/runtime` failure, and it PASSES in
    isolation 2/2 — the open-ended startup-race tail signature, NOT a
    regression).
  - `runtime_static_layer` (fixture 0087) passed INSIDE the full parallel
    sweep — no parallel-load flake exposure observed for the new fixture.
- **§6.1 mid-execution trigger, final adjudication: DID NOT FIRE** — no
  task's sub-steps exceeded ~10 items; net non-docs LoC measured
  `git diff --numstat d1760b0 HEAD -- . ':(exclude)docs/'` = **854** (+855
  −1) vs the ~1500 gate (the plan projected ≈905 — the README came in at 66
  lines vs the ~115 estimate).
- **No ADR fired this session** — no mid-execution decision arose: the plan
  was executed as written; the deviations (rustfmt reflows ×3 tasks, the
  Task-2 RED shape, the Task-3 count-vs-total reading) are recorded per-task
  above and none changed the plan's shape. Ledger head stays **ADR-0174**.
- **Commit:** `phase 108.2 task 6: gate-(d) record + regression sweep`

---

## CI on the state-3 head (measured after the STATE.md-advance commit)

**This is a MEASUREMENT, not a gate adjudication.** State 4 owns §7.5 and must
re-confirm it rather than inherit this.

Run **`31286236760`** on the full 40-char SHA
`7eab102a935433362651615ec8b6fd4f9220a32e` (the STATE.md-advance commit,
carrying all six task commits):

- **Attempt 1: `failure`** — real runners, steps **15**/**13**; fmt, clippy,
  build, h2spec install, image pre-pull all green; the `test` step failed on
  ONE test: `xds_rds_hot_reload::name_absent_reload_warm_rejects_and_keeps_last_good`
  panicking at `envoy-bin HCM ready: Os { code: 111, kind: ConnectionRefused }`
  — the CF-75-6 ephemeral-port fatal-startup STARTUP-RACE family signature
  (reserve-then-drop `reserve_port()`), in a test file this session never
  touched. Classified by TEXT (an HCM readiness connect, nothing
  runtime/admin-surface) and dispatched per the standing rule: RERUN THE SAME
  SHA. The fuzz job succeeded on attempt 1 (13 steps).
- **Attempt 2 (rerun --failed, same SHA): `success`** — `build + test + lint`
  success at **15** steps on a real runner (`runner_name` non-empty), fuzz
  success stands at **13** steps. Whole-run conclusion **success**.
- **CI test-count identity CONFIRMED:** the rerun log censuses
  **164 `test result` lines** (binaries 163 → 164) totalling
  **passed=2180, failed=0** — exactly `2170 (baseline run 31260569093 on
  ced6802) + 10 new #[test] fns` (6 envoy-admin + 2 envoy-bin +
  1 differential-lib + 1 fixture binary). The state-4 baseline identity is
  therefore **2180 on run `31286236760` (`7eab102`)**.

---

# Sub-phase 108.2 — §5 state-4 VERIFICATION (fresh session, solo-serial)

> The full §7.5 gate re-run FRESH at head `f1bbb2c` (tree clean, `main` in
> sync with origin) by a session that implemented none of it (ADR-0127).
> Every command quoted below ran in THIS session, serially (the cargo lock).
> The state-3 numbers were treated as CLAIMS; each is re-established here.

## Gate (e) — the five workspace commands, quoted

### 1. `cargo build --workspace --all-targets`

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s
```

Exit **0**. Fully cached — valid for build (cargo verified every target's
fingerprint against this exact tree; state-3's commits built it last).

### 2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`

First run: exit 0 but only **1** `Checking` line — a cached no-op per the
standing trap, NON-EVIDENCE. Forced a rebuild of the three phase-touched
crates (`cargo clean -p envoy-admin -p envoy-bin -p differential` — which
incidentally removed 216 683 files / 103.8 GiB of accumulated artifacts)
and re-ran:

```
    Checking envoy-admin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-admin)
    Checking differential v0.0.0 (/home/esa/git/envoy-rust/tests/differential)
    Checking envoy-bin v0.0.0 (/home/esa/git/envoy-rust/crates/envoy-bin)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.21s
CLIPPY_EXIT=0
CHECKING_COUNT=3
```

Exit **0** with **3** real `Checking` lines — exactly the three crates
108.2 touched, checked clean under `-D warnings`. The untouched crates'
clippy state is carried by CI (run `31286236760`, lint step green).

### 3. `cargo fmt --all -- --check`

Exit **0**, output EMPTY (0 lines).

### 4. `cargo test --workspace --no-fail-fast` (full redirect, 3643-line log)

Censused by the standing recipes — binary count separately, totals by
`grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed'` +
awk fields 4/6, failures by the `---- <name> stdout ----` markers:

```
binaries (test result lines) = 164
passed=2174 failed=6 sum=2180
```

**The identity HOLDS: passed + failed = 2180 over 164 binaries** — equal to
the CI baseline (run `31286236760` on `7eab102`) and to the state-3 local
sweep. The six failures, censused by marker and classified by TEXT and
ISOLATION (never by name):

- `access_log_h2_rcd_upstream_reset`, `access_log_h2_uc_upstream_reset`,
  `access_log_rcd_upstream_reset`, `access_log_rf_upstream_reset`,
  `admin_config_dump_server_info` — the deterministic five-member
  ADR-0164 host-flake core, present identically at state 3 and in every
  recent session; CI-authoritative, green on CI at this tree.
- `client::tests::send_request_maps_h2_handshake_failure_to_typed_error`
  (`envoy-http2` lib, 110 passed; 1 failed) — TEXT:
  `expected H2ClientHandshake, got Ok(ClientStream { host: "test.example", .. })`
  — the handshake unexpectedly SUCCEEDS on this host's networking, the
  known pre-existing envoy-http2 host-flake (surface untouched by 108.2:
  the crate had zero commits this phase). ISOLATION: **passes 2/2**
  (`test result: ok. 1 passed; 0 failed; ... 111 filtered out` both runs
  — count asserted, not exit code) — the open-ended tail signature, NOT a
  regression. Note the tail member MOVED since state 3
  (`admin_ready_returns_200_post_migration` then, this now) — exactly the
  run-to-run tail variance the standing rule predicts.
- `runtime_static_layer` (fixture 0087) passed INSIDE the parallel sweep.

### 5. `cargo deny check`

```
advisories ok, bans ok, licenses ok, sources ok
```

Exit **0**. (5 pre-existing `license-not-encountered` allowance warnings,
e.g. `Zlib` — warnings, not errors; unchanged from prior sessions.)

## Gate (a) — fixture `0087` LOCALLY (backend-free, fully local-verifiable)

`docker ps` clean (daemon up); `cargo build -p envoy-bin` FIRST — exit 0,
`Finished ... in 2.37s` (fresh debug binary, rebuilt this session after the
clippy force-clean). Then:

```
cargo test -p differential --test runtime_static_layer -- --nocapture
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s
```

**GREEN, 1.20 s warm** — inside the expected ~1-3 s window. The mutation
evidence (two data-mutation REDs + byte-exact revert + green control) is
banked in Task 4 and is not re-run here (state 4 verifies; it does not
re-implement).

## Gate (b) — all 86 pre-existing fixtures

The 86 pre-existing differential fixture binaries ran inside the sweep
above; every failure is in the six-member classified set (five of them the
standing ADR-0164 core — CI-authoritative on this host per the standing
host-networking records; the sixth a non-differential envoy-http2 lib
test). CI on this identical code tree (runs `31286236760` attempt 2 and
`31286913939`, both `success`, 15/13 steps, log census 164 binaries /
passed=2180 / failed=0) confirms all fixtures green. This session's own
state-4 commit gets its own CI confirmation (recorded below when polled).

## Gate (c) — h2spec, unchanged surface

No H2 codec or framing change in this slice. `known-failures.txt`
untouched at **21** lines / **1** real entry (`3.5/2`) — re-censused this
session. Local run with the self-skip made visible:

```
cargo test -p h2spec-conformance --test h2spec_runner -- --nocapture
h2spec_runner: h2spec not found — skipping locally
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The silent local self-skip is the documented behavior (visible only under
`--nocapture`); the gate is NOT vacuous in CI (ADR-0163) — CI's 15-step
job installs and runs h2spec and was green on `7eab102`/`f1bbb2c`.

## Gate (d) — fuzz

Re-verified fresh, not inherited: `git diff --stat d1760b0..HEAD --
'.github' '*fuzz*'` is **EMPTY** — the six 108.2 code commits touch no
fuzz target, no corpus, no CI step. Census: **5** fuzz targets across five
crates, `crates/envoy-config/fuzz/.gitignore` 68 lines / 65 `!` / 65
tracked seeds. The pre-existing `parse_bootstrap` short-budget CI run
(fuzz job, 13 steps) was green on both inherited runs and covers the only
parser surface adjacent to this slice.

## Adjudication — §7.5 (a)-(e)

| gate | verdict | decisive evidence |
|---|---|---|
| (a) new fixture `0087` green | **PASS** | local run quoted above, `1 passed`, 1.20 s |
| (b) 86 pre-existing fixtures green | **PASS** | sweep identity 2180/164 with all six REDs classified non-regression; CI failed=0 on the identical code tree |
| (c) conformance (h2spec) | **PASS** | surface unchanged; known-failures untouched (21/1); CI-authoritative and green |
| (d) fuzz | **PASS** | no new target — re-verified EMPTY diff; standing short-budget run green in CI |
| (e) five workspace commands | **PASS** | build 0 / clippy 0 with 3 real `Checking` lines / fmt 0 empty / test 2174+6=2180 over 164 / deny 0 |

**(f) is deliberately NOT adjudicated here** — it is state 5's `REVIEW.md`.

**No ADR fired**: every classification above follows a standing recorded
rule (ADR-0164 core, tail-by-isolation, ADR-0163 h2spec, CF-75-6 family);
no genuine new adjudication decision arose.

## Stop condition (re-measured this session)

FALSE — the twenty-fourth consecutive: ROADMAP census **110 rows / 108
`done` / 1 `in-progress` (parent `108`) / 1 `planned` (`108.2`)**; THREE
family headings still carry ZERO rows (HTTP/3 + QUIC `ROADMAP.md:122`,
gRPC `:126`, WASM host `:191`); `108.2` has states 5-6 remaining and
parent `108` closes only with it; the carry-forward set is live
(CF-108-1/2/3, CF-76-1, CF-75-2/3/4/5/6, banked Minors/Nits through the
108.1 REVIEW). NO `stop` file created.

---

## CI on the state-4 head (measured after the gate-record commit)

Run **`31288147958`** on the full 40-char SHA
`5767dfd95db9259317cd070fee2bbc0700937b9e` (the state-4 gate-record commit:
PROGRESS.md state-4 section + the STATE.md advance to §5 state 5):

- **Attempt 1: `success`** — no rerun needed. `build + test + lint` success
  at **15** steps on a real runner (`runner_name` non-empty), fuzz success
  at **13** steps. Whole-run conclusion **success**.
- **CI test-count identity CONFIRMED:** the job log censuses
  **164 `test result` lines** totalling **passed=2180, failed=0** — the
  same identity as the state-3 baseline (run `31286236760` on `7eab102`)
  and as this session's local sweep (`2174 + 6 = 2180`, the six local
  REDs being the classified host-flake set that CI does not share).
- Gate (b)'s CI leg is thereby closed on THIS session's own SHA, not
  inherited: all 87 fixtures green in CI at the state-4 tree.

---

## CI on the state-5 head (measured after the review commit)

Run **`31308413275`** on the full 40-char SHA
`483ea2f667b35a6d3ff42141f7ed3fd35ef58d6c` (the state-5 review commit:
REVIEW.md APPROVED + the STATE.md advance to §5 state 6):

- **Attempt 1: `success`** — no rerun needed. `build + test + lint` success
  at **15** steps on a real runner (`GitHub Actions 1000005134`), fuzz
  success at **13** steps (`GitHub Actions 1000005135`). Whole-run
  conclusion **success**.
- **CI test-count identity CONFIRMED:** the build-job log censuses
  **164 `test result` lines** totalling **passed=2180, failed=0** — the
  same identity as the state-3/state-4 baselines and the independent
  re-census of the reviewed HEAD (run `31288441844` on `42fb9d7…`) that
  `REVIEW.md` §0.3 records.
- Method note, banked in the traps ledger: the jobs-API log archive's
  numeric file prefixes are NOT stable across runs (this run's `0_` file
  is the FUZZ job where prior runs' `0_` was build+test) — select the job
  log by NAME, never by prefix.
