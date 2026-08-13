# Sub-phase 109.1 — PROGRESS (§5 state-3 implementation)

> Running log, appended per PLAN task (BOOTSTRAP_PROMPT.md §5 state 3). The
> execution authority is `PLAN.md` (7 tasks); the design authority `SPEC.md`.
> Session started at `a460b38` (clean tree, detection rule re-verified on disk:
> `109.1/` held SPEC.md + PLAN.md only; ROADMAP 113 rows / 110 done / 1
> in-progress / 2 planned; ADR head 0176; 130 `ConfigError` variants; fuzz
> `.gitignore` 68/65/65; `bootstrap.rs` 21635 lines; 87 fixture dirs; 14 crates;
> traps line MEASURED 108924 chars).

## Task 1 — `RuntimeSnapshot::route_fraction_gate` typed lookup ✅ (commit `b30be96`)

- **TDD RED:** transcribed the PLAN's two tests (`route_fraction_gate_pins_every_measured_cell`,
  `route_fraction_passes_is_total_and_maps_the_gate`) + `snap`/`rf` helpers into
  `runtime.rs` `mod tests`. `cargo test -p envoy-config --lib -- runtime::tests::route_fraction`
  → exit 101 with exactly the expected compile-error RED:
  `E0425 cannot find type FractionGate` / `E0433 FractionGateError` /
  `E0599 no method named route_fraction_gate` / `route_fraction_passes` (t1-red.log).
- **Implementation:** `FractionGate` + `FractionGateError` enums after the
  `impl RuntimeSnapshot` block; `route_fraction_gate` (SPEC §1.3 cascade:
  snapshot-prefix MapShapedKey check → finite-f64 parse with 0/≥100/between/negative
  arms → default_value sign with `NondeterministicDefault` reject) and the
  infallible `route_fraction_passes` (Err fallback = `default_value.numerator != 0`,
  deliberately NO `unreachable!()`) inside the impl.
- **GREEN:** `cargo fmt --all`, then `cargo test -p envoy-config --lib -- runtime::tests`
  → `test result: ok. 10 passed; 0 failed` (8 pre-task + 2 new; fresh `Compiling envoy-config` line).
- **Gates:** `cargo build --workspace --all-targets` exit 0;
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` exit 0
  with **13 `Checking` lines** (non-cached); `cargo fmt --all -- --check` exit 0.
- **Commit:** `b30be96` — `phase 109.1 task 1: RuntimeSnapshot::route_fraction_gate — the store's first typed lookup, pinned against all 23 measured cells` (1 file, +436).
- Note: the new-test RED was a compile error, which is the CORRECT RED shape
  here (the API did not exist — this is new-API TDD, not a characterization
  pin; no mutation check owed).

## Task 2 — wire field + 100-site fan-out + fuzz seed ✅ (commit `cb1cf26`)

- **TDD RED:** transcribed `route_match_runtime_fraction_parses_and_stays_optional`
  into the bootstrap.rs jwt-validator test mod (adjacent to the existing
  `RouteMatch` literal tests). `cargo test -p envoy-config --lib -- route_match_runtime_fraction`
  → exit 101, `error[E0609]: no field runtime_fraction on type bootstrap::RouteMatch` (t2-red.log).
- **Field added** after `headers` inside `pub struct RouteMatch` (deny_unknown_fields
  retained; `Route`'s hand-written impls untouched — its derives carry the field).
- **Fan-out:** E0063 blast confirmed; sites driven from `git grep -n 'RouteMatch {' -- crates/ tests/`
  (101 raw hits = 100 literals + struct def, matching W-1 exactly). Brace-matching
  script inserted `runtime_fraction: None,` at all 100 literals
  (2 bootstrap.rs + 1 instance.rs + 3 jwt_authn.rs + 1 types.rs + 57 http1/hcm.rs
  + 36 http2/hcm.rs); struct-update-syntax asserted absent; `cargo fmt --all`
  canonicalized. Doc-comment corruption check: the only `+` diff lines carrying
  `///` are the 8 intentionally-added ones (field doc + test doc).
- **Fuzz seed (D8):** `corpus/parse_bootstrap/runtime_fraction_route.yaml` created;
  `.gitignore` `!` line inserted after the `layered_runtime.yaml` negation.
  Proof: `.gitignore` **69** lines / **66** `!` lines; `git ls-files …/corpus/` = **66** (was 65).
- **GREEN:** named test `1 passed`. Full sweep `cargo test --workspace --no-fail-fast`
  → **passed=2178 failed=5 over 164 binaries**; identity closes:
  2178+5 = 2183 = 2180 + 3 new tests (T1's 2 + T2's 1). The 5 failures are
  EXACTLY the ADR-0164 five-member deterministic host-flake core
  (`access_log_h2_rcd/h2_uc/rcd/rf_upstream_reset` + `admin_config_dump_server_info`)
  — CI-authoritative, not a regression.
- **Gates:** clippy exit 0 with 13 `Checking` lines; `cargo fmt --all -- --check` exit 0.
- **Commit:** `cb1cf26` (8 files, +180/−2).

## Task 3 — four `ConfigError` variants + validators at boot & post-merge + jwt reject ✅ (commit `09769e1`)

- **TDD RED:** wrote `boot_rejects_nondeterministic_map_shaped_and_bad_default_runtime_fractions`
  + `jwt_rule_with_runtime_fraction_is_rejected` (jwt-validator test mod, mirroring
  `jwt_authn_validator_accepts_valid`'s cfg construction) +
  `load_dynamic_resources_rejects_lds_delivered_nondeterministic_runtime_fraction`.
  RED: exit 101, `E0599 no variant named` × all four new names (t3-red.log).
- **DEVIATION from PLAN (recorded, not edited into the plan):** (a) the PLAN's
  `runtime_fraction_bootstrap` helper yaml omits `codec_type`, which is a
  REQUIRED field on `HttpConnectionManagerConfig` (no serde default) — the
  first GREEN run failed with `missing field codec_type`; added
  `codec_type: HTTP1` (the `lds_file` helper precedent). (b) The SAME omission
  was in the PLAN's Task 2 fuzz-seed yaml — the landed seed would not parse;
  fixed the seed in this commit (the seed's value is exercising the
  runtime_fraction parse branch, which needs a parsing config). (c) The PLAN
  placed the post-merge witness "in lib.rs tests"; the existing
  `load_dynamic_resources` test pattern it says to mirror lives in
  bootstrap.rs's LDS test mod — placed it there (adjacency beats the letter).
  This is the `plan-md-example-code-trips` memory class, again.
- **Implementation:** 4 variants appended to `ConfigError` after the CSRF
  runtime variants (count 130 → **134**, re-derived); `validate_route_runtime_fraction`
  next to `validate_route_match_cardinality`; `validate_hcm` gains the
  `runtime: &RuntimeSnapshot` 6th param, called in its route walk directly
  after the cardinality check; `validate()` builds the snapshot ONCE after
  `validate_layered_runtime` (immutable borrow ends at the statement — no
  conflict with the later `&mut` walk, exactly as the PLAN priced); jwt rule
  walk gains the CF-109-3 presence reject.
- **GREEN:** `runtime_fraction` filter → `4 passed; 0 failed` (T2's serde test
  + the 3 new); whole crate `cargo test -p envoy-config --lib` →
  `706 passed; 0 failed` (703 pre-task + 3).
- **Gates:** workspace build exit 0; clippy exit 0 / 13 `Checking`; fmt --check exit 0.
- **Commit:** `09769e1` (3 files, +309).

## Task 4 — `HCMConfig.runtime` seam + `from_config` 5th param (behavior-neutral) ✅ (commit `a9e0ed6`)

- **RED (the compiler is the census):** added the field (after `pool_mgr`) + the
  5th `from_config` param + `runtime,` in its `Ok(Self { … })`. Workspace build
  → **E0063=42 / E0061=41** (t4-red.log). Census reconciliation vs W-1, re-derived:
  - the 41-literal claim decomposes as 39 `hcm.rs` grep hits + 2 `rds_watcher.rs`
    grep hits, but the constructor's own literal is spelled `Ok(Self { … })` and
    was NEVER in the `HCMConfig {` grep census — so 41 grep literals + 1 `Self`
    literal = 42 construction sites total;
  - **W-1 DEVIATION (recorded):** the `envoy-http2` hit W-1 classed as "the
    H2-wrapper literal that gains NOTHING" is at (drifted) `hcm.rs:4894` inside
    the TEST helper `synth_h2_hcm_config_with_pipeline`, which does
    `use envoy_http1::HCMConfig` and constructs the **H1** type — it DOES gain
    the field. The compiler's E0063 list, not the banked disambiguation, is the
    authority (the standing "a subagent finding is a claim" trap, hit again).
  - E0061=41 = the 44-site call census minus the 3 `envoy-bin` sites (envoy-bin
    never compiled in the RED build — its dependency failed first).
- **Fan-out:** driven from the compiler's error list (file:line), bottom-up per
  file: 42 literals gained `runtime: Arc::new(RuntimeSnapshot::default()),`,
  41 test call sites gained the arg (admin's spelled fully-qualified); ONE
  repair pass for trailing-comma multi-line call style (2 sites); test-mod
  `use envoy_config::runtime::RuntimeSnapshot;` imports added to
  http1/hcm.rs + rds_watcher.rs + http2/hcm.rs (test-mod, not crate-top: the
  production paths use fully-qualified names, avoiding an unused-import lint).
- **Production:** `main.rs` builds the snapshot ONCE after `Arc::new(bootstrap)`
  (`RuntimeSnapshot::from_bootstrap`), `Arc::clone`d at the three
  `from_config` sites (uring / per-worker / shared).
- **Census greps:** `runtime: Arc::new(RuntimeSnapshot::default())` = 39+2+1 = 42 ✓.
- **GREEN + behavior-neutrality:** workspace build exit 0; full sweep
  `passed=2181 failed=5` over **164** binaries — identity 2186 = 2180 + 6 new
  (T1:2 T2:1 T3:3), failing set = the ADR-0164 five-member core, unchanged.
- **Gates:** clippy exit 0 / 7 `Checking` (envoy-config untouched this task —
  the dirty set is the 7 downstream crates, per the clippy-cache trap this is
  a CACHE-dirty count, non-zero as required); fmt --check exit 0.
- **Commit:** `a9e0ed6` (5 files, +417/−115).

## Task 5 — RDS reparse widening + classifier extension, classifier test FIRST ✅ (commit `2d9bbbf`)

- **Step 1 (BEFORE any widening — the W-2 discipline):** wrote
  `reload_warm_rejects_nondeterministic_runtime_fraction` in rds_watcher.rs,
  on a new param'd harness `store_with_cluster_and_runtime` (the existing
  `store_with_cluster` now delegates with the default snapshot; its 7 existing
  callers unchanged). **DEVIATION (recorded):** the PLAN's test builds the
  snapshot layer via `serde_yaml::from_str`, but envoy-http1 has NO serde_yaml
  dev-dep — built the layer in code instead (`RuntimeValue::Int(50)`; the
  `snapshot_with_gate_k_50` helper) rather than adding a dependency.
- **TDD RED (the RIGHT failure):** `cargo test -p envoy-http1 --lib -- reload_warm_rejects…`
  → `panicked … got Ok(())` — the reload SUCCEEDS because nothing on the
  reparse path validates the field. A behavioral RED, not a compile error
  (compile shims — harness param, helper — were fixed first, per the plan note).
- **Widening:** `reparse_and_select_route_config` gains the 4th param
  `runtime: &RuntimeSnapshot`; `validate_route_runtime_fraction` called in the
  vh/route walk BEFORE the action match, context `rds:<path>` (the
  `validate_redirect_oneofs` convention); 8 rds.rs test call sites gained
  `&RuntimeSnapshot::default()` (paren-matching script, bottom-up).
- **Classifier:** production call passes `&target.store.runtime`; the
  `update_rejected` arm gains the THREE reparse-returnable variants; the
  "ONLY the six variants" comment now names NINE and records the DELIBERATE
  exclusion of `UnsupportedRuntimeFractionInJwtRule` (unreturnable — RDS route
  configs carry no jwt rules).
- **3 rds.rs unit tests** pin each variant through `reparse` (value-50 /
  map-shaped / bad-default snapshots).
- **GREEN:** `rds::` filter `17 passed` (14 pre + 3); `rds_watcher` filter
  `11 passed` (10 pre + 1 — the Step-1 test passes: Err + update_rejected=1 +
  live table ptr-equal + NO unreachable!() abort).
- **Gates:** workspace build exit 0; clippy exit 0 / 13 `Checking`; fmt --check exit 0.
- **Commit:** `2d9bbbf` (2 files, +260/−19).

## Task 6 — the LIVE gate at both `route_matches` call sites + the H2 witness ✅ (commit `3181291`)

- **TDD RED (behavioral, all three):** `gated_route_test_config` helper
  (mirrors `resolve_route_test_config`; TWO routes — gated "/"-prefix
  consulting gate.k above a bare catch-all; snapshot layers CODE-BUILT via
  `RuntimeValue::Str` — same serde_yaml-dev-dep deviation as Task 5, recorded)
  + `resolve_route_honors_runtime_fraction_gate` +
  `build_response_honors_runtime_fraction_gate` (H1) +
  `h2_inherits_runtime_fraction_gate_via_shared_resolver` (H2, calling the
  EXACT production path `envoy_http1::hcm::resolve_route(&inner, …)` on a
  `from_config`-built inner). RED: all three fail on the key-"0" assertions —
  `key 0 must skip the gated route` / `left: "gated"` — the gated route still
  matches because the gate is unwired. (Adaptation: `BuildOutcome::Synth(resp, _)`
  carries a `Response`, so assertions read `resp.body`, and `BuildOutcome` has
  no `Debug` — panic arms follow the existing `_other =>` house style.)
- **Wiring:** `route_matches` gains the 4th param `runtime: &RuntimeSnapshot`
  and opens with the gate (`route_fraction_passes`, evaluated FIRST — AND-order
  neutral); `resolve_route_in`/`build_response_in` gain the param;
  public `resolve_route`/`build_response` pass `&config.runtime`
  (signatures UNCHANGED — H2 zero production edits); the keep-alive loop's two
  direct `_in` sites pass `&config.runtime`; the 6 `route_matches` test call
  sites gained `&RuntimeSnapshot::default()` (the compiler enumerated exactly 6
  E0061 sites, matching the PLAN).
- **GREEN:** the 3 new tests pass; whole crates: envoy-http1 `201 passed; 0 failed`,
  envoy-http2 `112 passed; 0 failed; 1 ignored`. Full sweep:
  **passed=2187 failed=6 over 164 binaries** — identity 2193 = 2180 + 13 new
  (T1:2 T2:1 T3:3 T5:4 T6:3). Failures = the ADR-0164 five-member core + ONE
  tail member (`send_request_maps_h2_handshake_failure_to_typed_error`, the
  documented envoy-http2 h2-handshake host-flake) — **isolation-classified:
  re-run alone → `1 passed; 0 failed`**. Not a regression.
- **Gates:** clippy exit 0 / 7 `Checking` (envoy-config untouched); fmt --check exit 0.
- **Commit:** `3181291` (2 files, +265/−13).

## Task 7 — D7 absence-assertion narrowing + the state-3 exit gate ✅ (commit `9a7e7f8`)

- **Narrowed in place, all three:** the `runtime.rs` module doc ("Nothing reads
  this store yet" → the route consumer is live, the rest stays true), the
  `runtime_stats.rs` module-doc sentence (reads-vs-mutates split made explicit),
  and the ONE `BEHAVIOR_CONTRACT.md` blockquote sentence ("Nothing READS the
  runtime store for behavior yet" → 109.1 narrowing). NOT touched:
  `runtime_key_is_rtds_inert`, the CSRF rejects, fixtures, the full `## Runtime`
  consumer subsection (109.2's).
- **Old-wording census:** `git grep 'Nothing reads this store yet\|Nothing READS
  the runtime store'` → ZERO hits.
- **State-3 exit gate (state 4 owns the formal §7.5 sweep):**
  `cargo build --workspace --all-targets` exit 0; clippy exit 0 / **13 Checking**;
  `cargo fmt --all -- --check` exit 0; `cargo deny check` exit 0
  (`advisories ok, bans ok, licenses ok, sources ok`);
  `cargo test --workspace --no-fail-fast` run **2×**:
  sweep1 `passed=2187 failed=6`, sweep2 `passed=2188 failed=5`, BOTH over 164
  binaries, identity **2193 = 2180 + 13 new tests** closing both times.
  Failing-SET diff: the five-member ADR-0164 core is IDENTICAL across both
  runs; the h2-handshake tail member appears only in sweep1 (and passes in
  isolation — Task 6 log) — the tail MOVES, the core doesn't, exactly the
  documented classification.
- **Commit:** `9a7e7f8` (3 files, +19/−10).

## Session summary (state-3 COMPLETE)

All SEVEN PLAN tasks landed IN ORDER with TDD and per-task commits:
`b30be96` (T1) → `cb1cf26` (T2) → `09769e1` (T3) → `a9e0ed6` (T4) →
`2d9bbbf` (T5) → `3181291` (T6) → `9a7e7f8` (T7). Net new tests: **13**
(local identity 2193 over 164 binaries, CI to confirm). The §6.1 mid-execution
split trigger did NOT fire (no task's sub-steps passed ~10 items; running net
LoC ≈ the PLAN's ≈1180 projection); **ADR-0177 stays UNRESERVED**. PLAN
deviations (all recorded per-task above, none edited into the landed PLAN):
the `codec_type` yaml omission (T3, also fixed the T2 seed), the
lib.rs-vs-bootstrap.rs LDS-test placement (T3), the serde_yaml-dev-dep
adaptation (T5/T6), the W-1 http2-literal misclassification + the `Ok(Self {…})`
constructor literal (T4), the trailing-comma call-site repair (T4).
NEXT = the §5 state-4 verification, a SEPARATE session (§5.1; ADR-0127).

## State-4 verification (§7.5 gate set — SEPARATE session per §5.1/ADR-0127; the verifier did NOT run the implementation)

> Session start: `git fetch origin --prune`, `git status --porcelain` CLEAN,
> branch `main`, HEAD `03961e0` (the state-3 CI-record commit). Detection rule
> re-verified on disk: `109.1/` holds SPEC.md + PLAN.md + PROGRESS.md (7
> complete task entries, no state-4 section); ROADMAP census 113 rows / 110
> `done` / 1 `in-progress` / 2 `planned`; ADR head 0176 (max recipe), 0177
> free. Docker daemon UP. `superpowers:verification-before-completion` invoked.

### Gate (e) — build / clippy / fmt / deny

- **Cached-no-op guard fired first:** from the freshly-pulled green tree, both
  `cargo build --workspace --all-targets` and clippy returned exit 0 with
  **ZERO** `Compiling`/`Checking` lines — non-evidence per the standing trap.
  Applied the 108.2 recipe: `cargo clean -p envoy-admin -p envoy-bin
  -p envoy-config -p envoy-filter -p envoy-http1 -p envoy-http2` (the six
  crates the state-3 diff touched), then re-ran everything.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` →
  exit **0** with **14 `Checking` lines** (non-cached; ends
  `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 3.94s`).
- `cargo build --workspace --all-targets` → exit **0** with **14 `Compiling`
  lines** (`Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 12.41s`).
- `cargo fmt --all -- --check` → exit **0**, zero output bytes.
- `cargo deny check` → first attempt errored (`the directory …/docs/envoy-rust
  doesn't contain a Cargo.toml file` — run from a subdirectory cwd; cargo-deny
  does not walk up; recorded honestly); re-run from the repo root → exit **0**,
  final line `advisories ok, bans ok, licenses ok, sources ok` (the 5
  `license-not-encountered` warnings are the documented benign family).

### Gates (a)+(b) — the differential surface (no new fixture in 109.1) + full sweep ×2

`cargo test --workspace --no-fail-fast` run **TWICE**, full output redirected
to files (never through `tail`), totals derived by the standing recipe
(`grep -oE 'test result: (ok|FAILED)\. [0-9]+ passed; [0-9]+ failed'`, awk
fields 4/6, binary count asserted separately):

- sweep 1: **binaries=164 passed=2188 failed=5 sum=2193** (exit 101 — non-zero
  because failures are present, as expected)
- sweep 2: **binaries=164 passed=2188 failed=5 sum=2193**
- identity **2193 = 2180 + 13 new tests** closes on BOTH runs (CI-confirmed
  2193/0 on `9331ce3`, recorded in `STATE.md ## Last commit`).
- **Failing-SET diff: IDENTICAL across both sweeps** (`diff` of the sorted
  `---- <name>` marker sets = empty): the four `access_log_*_upstream_reset`
  (h2_rcd / h2_uc / rcd / rf) + `admin_config_dump_server_info` — exactly the
  ADR-0164 five-member deterministic core, with **ZERO tail members in either
  sweep** (first time measured with an empty tail; the tail's absence carries
  no signal either way, per the standing rule).
- **Isolation classification (ONLY isolation classifies):** each of the five
  re-run alone → `test result: FAILED. 0 passed; 1 failed` all five —
  **deterministic in isolation, which IS the core's environmental signature**
  (ADR-0164). Failure TEXTS match the documented families byte-for-byte in
  kind: `envoy="{\"rc\":503,\"rf\":\"UF\"}" envoy-rust="{\"rc\":503,\"rf\":\"UC\"}"`
  (TcpCloseBackend IPv6-unreachable family) and `fixture green: admin body
  rule: /clusters` (the 192.168.65.2 bridge-IP family). All five are
  LOCAL-only: CI passes them (run `31572355578` on `9331ce3`, 2193/0).
- Gate (a): 109.1 adds NO fixture (SPEC §5, the 108.1 foundation-slice
  precedent) — vacuously green. Gate (b): all 87 differential test files ran
  inside both sweeps; every RED is isolation-classified environmental above.
  **No regression; no §5.2 re-entry.**

### Gate (c) — conformance

`cargo test -p h2spec-conformance -- --nocapture` → exit 0; the runner
printed **`h2spec_runner: h2spec not found — skipping locally`** (the
documented LOUD local self-skip; `which h2spec` absent on this host) and the
two string-parser unit tests + gate reported `3 passed`. Per the ADR-0163
posture: the LOCAL gate is recorded as SKIPPED-NOT-PASSED;
`known-failures.txt` is UNCHANGED (**21 lines, exactly ONE real entry**,
untouched by the whole 109.1 diff) and the CI gate — which genuinely runs
h2spec — is green on this exact tree (`9331ce3` run `31572355578`).
NEVER trimmed on local evidence.

### Gate (d) — fuzz, all five targets at the CI 30s budget (`cargo +nightly fuzz run <t> -- -max_total_time=30`, each from its crate dir)

- `parse_bootstrap` (envoy-config): `Done 35960 runs` — exit 0 (66 tracked corpus files incl. the T2 seed)
- `jwt_parse` (envoy-jwt): `Done 4512071 runs` — exit 0
- `cdn_loop_parse` (envoy-filter): `Done 10468273 runs` — exit 0
- `accesslog_format_parse` (envoy-accesslog): `Done 3342055 runs` — exit 0
- `grpc_health_decode` (envoy-http2): `Done 41558474 runs` — exit 0

No crash, no sanitizer finding in any log. No new fuzz target in 109.1 ⇒ no
`ci.yml` edit owed.

### Disk-fact censuses (re-derived this session, not inherited)

**134** `ConfigError` variants (enum spans `lib.rs:75-1105`; counted by
`#[error` lines inside the enum span); fuzz `.gitignore` **69/66/66** with the
T2 seed in `git ls-files`; `bootstrap.rs` **21943** lines; **87** fixture dirs
(highest `0087`) / **87** differential test files / **164** test binaries;
**14** crates; `#![forbid(unsafe_code)]` present in every crate root (zero
missing); `known-failures.txt` 21 lines / 1 real entry; ADR head **0176**
(next free 0177 — unreserved, nothing new decided this session).

### Read-only recon (subagent evidence, re-verified on disk by the main session)

All **13** new test fns exist at their claimed sites (the one name deviation —
`reparse_rejects_map_shaped_runtime_key`, shorter than the handoff's spelling —
was pre-flagged in the state-3 ledger); the 23-cell table pins EVERY SPEC §1.3
cascade row (v==0 / v≥100 / 0<v<100 fatal / v<0→default / non-numeric→default /
map-prefix fatal / nondeterministic-default fatal, each with an explicit
assertion); the gate is LIVE in production (`route_matches` takes `runtime:`
and evaluates `route_fraction_passes` FIRST; both `resolve_route_in` and
`build_response_in` thread it; public wrappers pass `&config.runtime`); the
classifier's `update_rejected` arm carries the 4 pre-existing + 3 new variants
and its comment names the NINE-variant returnable set with the jwt exclusion
justified. **Handed to state-5 (observation, not a defect):** the 23-cell
table's `"empty runtime_key"` edge row (`runtime.rs:751-756`) is
NON-DISCRIMINATING as written — snapshot `gate.k: 0` under `rf(0, Hundred,
Some(""))` yields `Never` whether or not the empty key is consulted; a
discriminating pin needs a diverging default (e.g. 100). The test passes; the
pinned RULE is real; only this one row's witness power is weak.

### Verdict

Gates (a)-(e) **GREEN** (c local-skip documented, CI-authoritative). Gate (f)
is state-5's (REVIEW.md — the NEXT session; NOT written here). Stop condition
re-measured and FALSE on all three legs (113/110/1/2; 109.1 mid-family with
states 5-6 + all of 109.2 ahead; h3=0 grpc=0 wasm=0 by heading-slice at
ROADMAP lines 122/126/194). NO `stop` file created.
