# Phase 18 (`18-xds-file-based-cds`) — REVIEW

- **Lifecycle state:** state 5 (verified → reviewed). This document is the state-5 output.
- **Skill:** `superpowers:requesting-code-review` (per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`).
- **Review range:** `8cc5ca563..be7768617` — the Task 1–10 state-3 execution arc + the in-arc
  host-gateway fix + the Task-11 state-4 verification, atop the state-2 PLAN-write `8cc5ca563`
  (the docs-only base; its code tree is identical to the pre-phase-18 baseline `3acf7367b`).
  The CODE commits reviewed: `ce7abce5b` (T1 `dynamic_resources` schema + `validate_clusters` +
  deferred cluster-reference validation) → `33ecf55cd` (T2 CDS file parser `cds.rs`) → `e793f5609`
  (T3 `load_dynamic_resources` + effective-cluster-list merge + 7-site `all_clusters()` consumer
  migration) → `a6c0de764` (T4 conditional `cluster_manager.*` stat family) → `059257d25` (T5
  `ClustersConfigDump` conditional emission) → `d7c804679` (T6 harness `{{CDS_PATH}}`
  rendering/mounting + `Http1KeepAlive` `admin_scrapes`) → `4c6ce3c4c` (T7 fixture
  `0026-xds-file-based-cds` + Docker wrapper) → `4c7bd2aac` (T8 in-process backstop, 5 paths) →
  `78d393b4b` (T9 fuzz seed, corpus 28→29 + consistency restoration) → `62bcecce6` (T10
  BEHAVIOR_CONTRACT extensions) → `f5873902a` (in-arc fix: the CI-only host-gateway bug). The
  PROGRESS-subsection / STATE-advance commits in the range carry NO code. **No in-review fix
  commits were needed.**
- **Pre-review HEAD:** `be7768617` (== `origin/main` at review start; everything pushed).
- **Method:** 4 read-only code-review subagents, one per concern-cluster, dispatched **SERIALLY**
  (`feedback_serial_subagent_dispatch`), each reading the actual on-disk diff (`git diff` /
  `git show` + `Read` + `Grep`) and re-running the relevant non-Docker test suites
  (`cargo test -p envoy-config` 324/0, `-p envoy-cluster` 88/0, `-p envoy-admin` 79/0,
  `-p differential --lib` 119/0/1, the in-process backstop `-p envoy-bin --test
  xds_file_based_cds` 5/0, the fuzz corpus gate 1/0; per-crate clippy clean everywhere). The
  controller independently spot-verified the load-bearing claims every cluster verdict rests on
  (the migration-straggler grep, the `.expect`-to-let-chain restructure, the conditional
  registration/emission predicates, the fixtures-0020–0025 immutability, the fuzz-corpus
  arithmetic, the `uses_host_gateway` combined-source call site, the shared
  `assert_admin_scrape_case` call sites) by direct grep/read against HEAD before accepting them.
- **Verdict: APPROVED** (zero Critical / **zero Important** / 10 non-gating Minors M18-1…M18-10
  carried). The second consecutive phase (after 17) to clear state-5 review with no in-review fix
  and no Important finding.

---

## 1. The named review focus (the PLAN Task-11 / STATE.md state-5 charter — the items this session MUST verify)

### 1.1 Deferred-validation soundness — **VERIFIED: no config path reaches a runtime cluster-lookup miss**

The L12b defer-then-revalidate design holds end-to-end. (a) envoy-bin's flow cannot skip the
load: `main.rs:51` `parse_bootstrap` → `:54` `load_dynamic_resources(&mut bootstrap)?` → `:55`
`Arc::new(bootstrap)` (constructed only on success) → every consumer (ClusterManager `:127`,
pools `:143`/`:156`, health `:169`, the TLS loop `:195`) constructed strictly after; the `?` makes
any load/re-validation error a process exit. (b) A Bootstrap with deferred (unvalidated)
references cannot reach any consumer: the Arc every consumer receives is built only after a
successful load that re-ran `validate()` (`lib.rs:578`). (c) `cds_configured_but_unloaded()`
(`bootstrap.rs:49`) = `cds_config.is_some() && dynamic_clusters.is_none()` is correct in all
reachable (configured × loaded) states; the unconfigured+loaded state is unreachable
(`load_dynamic_resources` returns early when unconfigured). (d) The old panicking
`.expect("UnknownCluster check above guarantees presence")` is **gone** — the H2-from-H1 gate is
now a let-chain (`bootstrap.rs:2360-2368`, `&& let Some(cluster_ref) = clusters.iter().find(…)`)
that skips silently on deferred references and re-enforces at the Task-3 re-validation
(controller-verified by direct read; grep for the old expect string returns only the explanatory
comment at `:2359`). (e) The regression tests (`bootstrap.rs:6316`/`:6346`) prove the
no-`dynamic_resources` path still enforces `UnknownCluster` immediately;
`unresolved_route_reference_fatal_after_load` (`:6561`) proves a reference in NEITHER list fails
startup (no panic).

### 1.2 `all_clusters()` migration completeness — **VERIFIED: no stragglers**

Controller grep at HEAD for `static_resources.clusters` across all crate sources: the only
production hits are the documented-deliberate set — `validate()`'s static-only per-cluster
invariant loop (`bootstrap.rs:1989`; dynamic clusters run the identical `validate_cluster`
gauntlet inside `parse_cds_file` instead), `validate()`'s disjoint-field snapshot
(`:1973-1978`, the borrow-checker-documented shape), doc comments, and test code. The five
migrated consumers all go through `all_clusters()`: `envoy-cluster/src/cluster.rs:726`,
`envoy-http1/src/pool.rs:451`, `envoy-http2/src/pool.rs:600`, `envoy-health/src/scheduler.rs:47`,
`envoy-bin/src/main.rs:195` (the TLS loop). `OutlierManager` (`outlier.rs:116`) and the admin
clusters endpoint (`endpoint.rs:551`) iterate the already-merged `ClusterManager` (correctly
unmigrated); config_dump's static-cluster rendering (`endpoint.rs:425`) deliberately renders the
bootstrap as-parsed (SPEC §5.5).

### 1.3 §5.2 inertness — **VERIFIED: structurally airtight**

Both new observability surfaces gate behind the identical, precise predicate
`dynamic_resources.as_ref().and_then(|dr| dr.cds_config.as_ref()).is_some()` — stat registration
at `cluster.rs:1068-1072`, `ClustersConfigDump` emission at `endpoint.rs:418-423` (push at
`:448`). A full grep confirms these are the ONLY `cluster_manager.*` registration site and the
ONLY `ConfigDumpEntry::Clusters` emission site — no unconditional path exists. The predicate is
strictly narrower than `dynamic_resources.is_some()` (an empty `dynamic_resources: {}` trips
neither). Absence is proven by test on both sides
(`cluster_manager_stats_not_registered_without_dynamic_resources` scrapes the full registry
snapshot; `no_dynamic_resources_emits_single_bootstrap_entry` asserts exactly one config_dump
entry — the fixture-0014 shape). On the harness side the new `admin_scrapes` field is
`#[serde(default)]` with a round-trip test asserting empty-when-absent, and the `all_clusters()`
migration is identity-preserving when `dynamic_clusters` is `None`. The 25 pre-existing fixtures
were green in the state-4 CI anchor run.

### 1.4 The L4 fatal-error posture consistency — **VERIFIED (with one Minor doc note, M18-1)**

Only two non-test callers of `load_dynamic_resources` exist: `envoy-bin/src/main.rs:54` (exits on
`Err` via `?` BEFORE constructing the Arc — no partial-load state can ever serve) and the test
suite. The known Task-3 finding (the function is not all-or-nothing internally: `dynamic_clusters`
is set at `lib.rs:574` before the re-validation at `:578`, so an `Err` leaves the `&mut Bootstrap`
mutated) is real but has no production impact today; the function's doc comment does not yet warn
future callers of the on-error mutation — carried as **M18-1**. The backstop proves the fatal
posture process-level on both negative paths (missing file: exit non-zero + "reading CDS file"
diagnostic + port-never-accepts; malformed file: exit non-zero + "parsing CDS file" — the
recorded-divergence proof vs Envoy's warn-and-serve).

### 1.5 The harness `admin_scrapes` extension's backward compatibility — **VERIFIED: fixtures 0020–0025 byte-untouched**

Controller-verified: `git log --oneline 8cc5ca563..be7768617 -- tests/fixtures/0020*  … 0025*`
returns **ZERO commits**. Structurally: `admin_scrapes` is `#[serde(default)]`
(`tests/differential/src/lib.rs:182`), the serde round-trip test asserts `admin_scrapes.is_empty()`
on key-absent input (`:4342-4379`), and the assertion logic was extracted VERBATIM into the shared
`assert_admin_scrape_case` (`:3883`) called identically from both the `AdminScrape` arm (`:3781` —
fixtures 0011/0014's path) and the new `Http1KeepAlive` loop (`:3092`) — the two drivers cannot
drift (controller-verified both call sites by grep).

---

## 2. Cluster verdicts

| # | Concern cluster | Tasks | Verdict | Critical | Important | Minor |
|---|---|---|---|---|---|---|
| 1 | `envoy-config` schema + CDS parser + loader/merge/migration | T1, T2, T3 | **CLEAN** | 0 | 0 | 3 (M18-1, M18-5, M18-6) |
| 2 | Conditional `cluster_manager.*` stats + `ClustersConfigDump` | T4, T5 | **CLEAN** | 0 | 0 | 1 (M18-3) |
| 3 | Harness CDS support + fixture 0026 + in-process backstop + in-arc fix | T6, T7, T8, fix | **CLEAN** | 0 | 0 | 2 (M18-2, M18-8) |
| 4 | Fuzz seed + BEHAVIOR_CONTRACT extensions | T9, T10 | **CLEAN** | 0 | 0 | 1 (M18-4) |

Cluster-1 highlights: the deferred-then-revalidate invariant traced end-to-end (§1.1); the
`validate_cluster` extraction is a genuine single source of truth shared by `validate()` and
`parse_cds_file` — dynamic clusters provably run the identical per-cluster gauntlet; the
borrow-checker rationale for the disjoint-field snapshot is correct and documented; test quality
is high and behavior-driven (real tempfiles, real parse→load→assert; both fatal negative paths +
collision + the load-bearing post-merge-enforcement test). Cluster-2 highlights: the conditional
predicates are byte-identical at both sites and strictly narrower than `dynamic_resources.is_some()`;
the stat-handle lifetime argument is sound (the registry retains its own `Arc` clone — dropped
local handles cannot orphan the values fixture 0026 scrapes); the L9 collision-skip happens
upstream in the loader, so config_dump cannot double-count a collided cluster. Cluster-3
highlights: the in-arc `uses_host_gateway` fix is correct, testable, and covers both rendered
sources; the `residual_marker` guard converts any unrendered CDS marker into a named pre-launch
error; the backstop's negative-path detection has no pipe-deadlock risk (concurrent
stdout+stderr drain via `tokio::join!`) and the collision test proves static-wins on the DATA
PLANE with distinct backend bodies, not merely via config_dump; the fourth-site hunt for the
main-template-only-scan bug class found NO remaining silent site (see M18-2 for the two
fail-loud residual scans). Cluster-4 highlights: the fuzz-corpus arithmetic reconciles exactly
(29 tracked seeds = 29 allow-list entries = 25 SUCCESS + 3 REJECT + 1 minimal array refs, verified
by set-comparison); the corpus gate is not stale (missing seeds panic the test); every load-bearing
BEHAVIOR_CONTRACT claim traces 1:1 to the implementation at HEAD, the PLAN §6.2 lock-ins, fixture
0026's expectations, and ADR-0049's four reconciliations.

---

## 3. Controller verification notes

Per the phase-16/17 state-5 method, the controller did not accept cluster verdicts on faith:

1. **Migration stragglers** (§1.2): re-grepped all crate sources at HEAD — only the
   documented-deliberate sites consult `static_resources.clusters` in production code. MATCHES
   cluster 1.
2. **The `.expect` restructure** (§1.1): the cluster-1 reviewer cited "`if let Some(cluster_ref)`
   at `bootstrap.rs:2360`"; a literal grep for that string returns nothing because the actual code
   uses Rust 2024 **let-chains** (`&& let Some(cluster_ref) = …`). Controller read the gate region
   directly (`:2344-2375`): the restructure is real, the semantics match the reviewer's claim, and
   the old panicking expect string survives only in an explanatory comment. Claim ACCEPTED
   (phrasing imprecision only).
3. **Conditional registration/emission predicates** (§1.3): re-grepped both sites — exactly
   `.and_then(|dr| dr.cds_config.as_ref())` at `cluster.rs:1071` and `endpoint.rs:422`. MATCHES
   cluster 2.
4. **Fixtures 0020–0025 immutability** (§1.5): re-ran the exact `git log` — zero commits. MATCHES
   cluster 3.
5. **Fuzz-corpus arithmetic**: a naive `ls | wc -l` of the corpus directory returns ~21k files
   (cargo-fuzz's generated, gitignored entries); the correct count is the 29 **tracked** seeds
   (`git ls-files` = 29 = the `.gitignore` allow-list count). MATCHES cluster 4's
   `*.yaml`-filtered count — recorded here so a future reader doesn't misread the raw directory
   count as drift.
6. **`uses_host_gateway` + `assert_admin_scrape_case` call sites**: re-grepped — the helper takes
   `(upstream_main, upstream_cds: Option<&str>)` and is called at `lib.rs:2533` with both rendered
   sources; the scrape assertion is defined once (`:3883`) and called from both driver arms
   (`:3092`, `:3781`). MATCHES cluster 3.

---

## 4. Carryforward dispositions + Minor findings (non-gating)

### 4.1 Arc-discovered carryforwards (from PROGRESS + the state-4 inventory; reviewed + dispositioned)

1. **The fuzz-corpus 28-vs-27 pre-existing inconsistency — CLOSED.** `cluster_http2_protocol_options.yaml`
   (allow-listed since ~13.2 but missing from the SUCCESS array) was restored to the array at
   Task 9; the corpus is now fully consistent (29 = 25 + 3 + 1, controller-verified). The
   PLAN-write's state-5 inventory item is resolved.
2. **The main-template-only-scan bug class (the phase's only Criticals — both found and fixed
   in-arc).** Three sites existed: the Task-6 backend-launch detection (C1, found by the Task-6
   quality reviewer, fixed pre-push), the Task-6 `BACKEND_HOST` kv gate (fixed in the same
   commit), and the ADR-0015 `host_uses_host_gateway` flag (C2, found at the state-4 CI-evidence
   check as a deterministic Linux-CI-only 503, fixed at `f5873902a`, CI-proven at run
   `26863221247`). The state-5 fourth-site hunt (cluster 3) found NO remaining silent site; the
   two residual main-only scans are fail-loud (M18-2). **Disposition: class closed for current
   fixtures; M18-2 records the forward-looking hardening.**
3. **CI readiness-flake family 0011 + 0012 + 0022 (PRE-EXISTING; manifested during the arc).**
   The Task-7 and Task-8 pushes' CI failures were this documented family (fixture 0012's
   access-log race; fixture 0025's readiness timeout), which fail-fast-masked the real 0026
   host-gateway failure in those runs. **Disposition: carries unchanged** (memory
   `project_flaky_access_log_fixture_0012` records it); the masking effect is a known cost of
   fail-fast CI — re-runs disambiguate.
4. **Two new flake-family observations (recorded in PROGRESS + auto-memory).** (a) The
   helper-rebuild readiness-timeout class is AMPLIFIED by running clippy `--all-features`
   immediately before a workspace test (feature-fingerprint dirtying); (b) rapidly launching many
   envoy-bin subprocesses during overlapping cargo test runs can trigger macOS dyld startup
   stalls. **Disposition: both carry as standing local-environment cautions; neither is a product
   defect; all cleared on re-run.**
5. **The standing multi-phase inventory** (the phase-17 rollovers: the pool-liveness family, the
   H2-upstream-fork coverage gap, the cyclic retry-script parallel-drive fragility, the 14.1
   M-track items, M-c1/M-c2/M-c3, the §6.9 per-class extension, the Upstream-robustness deferral
   ledger, ADR-0028) — **phase 18 engages NONE of it; all carries unchanged.**

### 4.2 Minor findings (M18-1 … M18-10; none gating; carried with no named owner)

| # | Finding | File | Why non-gating |
|---|---|---|---|
| M18-1 | `load_dynamic_resources` sets `dynamic_clusters` before the post-merge re-validation, so an `Err` leaves the `&mut Bootstrap` mutated; the doc comment doesn't warn future callers | `crates/envoy-config/src/lib.rs:574-578` (doc at `:530-537`) | The only production caller (`main.rs:54`) exits on `Err` before any consumer is constructed; one doc line ("on error the bootstrap must not be reused") closes it |
| M18-2 | `needs_tls_pki` (`lib.rs:2170`) and `needs_admin_port` (`lib.rs:2160`) scan main templates only — the last two unconverted template-content scans after the bug-class fix | `tests/differential/src/lib.rs` | Both are FAIL-LOUD if ever wrong: a CDS-only TLS marker leaves the kv map without the path keys → `residual_marker` (`:2499`) bails with the named marker pre-launch; `ADMIN_PORT` is structurally main-config-only. Becomes real work only when a future fixture puts an mTLS upstream cluster in a CDS file |
| M18-3 | The Task-4 stat registration reuses `StatsRegistration { cluster, message }` passing a STAT name in the `cluster` field — an error there would render "registering cluster 'cluster_manager.cds.update_attempt' stats" | `crates/envoy-cluster/src/cluster.rs:1078-1096` | The error path is unreachable (fixed valid names, no kind conflict); cosmetic field-name mismatch in a never-rendered message |
| M18-4 | The BEHAVIOR_CONTRACT L4 negative-path table asserts FATAL for 5 fault classes but the backstop directly exercises only 2 (missing/malformed); the other 3 rest on schema-level invariants (deny_unknown_fields, EmptyClusterEndpoints, per-cluster validation) verified by unit tests elsewhere | `docs/envoy-rust/BEHAVIOR_CONTRACT.md` filesystem-transport §(c) | The claims are true and individually tested — just not all backstop-asserted; a one-line provenance note per row would make the coverage source explicit |
| M18-5 | The cds.rs negative tests couple to serde_yaml error strings (`"unknown field"`) and the remaining negative tests lack message assertions | `crates/envoy-config/src/cds.rs` test module | Brittle across serde_yaml upgrades but currently correct; the Task-2 carried polish item |
| M18-6 | Task-3 polish: re-validation recompiles route regexes (idempotent but redundant); the `MINIMAL_CDS` test const is duplicated across test modules; the intra-file collision dedup is O(n²) | `crates/envoy-config/src/lib.rs`, `bootstrap.rs` test modules | All correctness-neutral; CDS files are small; consolidation is cosmetic |
| M18-7 | envoy-admin test-helper near-duplication (`handler_from_bootstrap`/`handler_with_bootstrap`) + `CLUSTER_TYPE_URL` const placement | `crates/envoy-admin/src/endpoint.rs` test modules | Test-only cosmetics; the Task-5 carried items |
| M18-8 | Harness polish: `subject_cds_path` computed unconditionally even for non-CDS fixtures; one doc-wording nit | `tests/differential/src/lib.rs:2210-2211` | An unused `PathBuf` join on non-CDS fixtures; no behavior impact |
| M18-9 | The backstop-helper duplication (`reserve_port`/`wait_ready`/`http1_oneshot`/`scrape_admin_stats`) has crossed the phase-05.2 REVIEW M5 N≥3 threshold — now copied across ≥4 backstop files | `crates/envoy-bin/tests/*.rs` | The M5 disposition ("extract a shared test-support crate when the third consumer appears") is now triggered; extraction is a future hardening-phase task, correctly not done inside a tests-only task |
| M18-10 | Fixture-0026 README frames the 0008-equivalence as structural where the assertion set is narrower (status + body + named stats) | `tests/fixtures/0026-xds-file-based-cds/README.md` | Prose framing only; the actual assertions are correctly enumerated in expectations.yaml |

### 4.3 Standing multi-phase Minor inventory (inherited; not engaged by phase 18)

The phase-17 rollovers inventory (STATE.md `### Phase-17 rollovers`) carries forward unchanged:
the 8 phase-17 Minors M17-1…M17-8, the 6 phase-17 carryforward dispositions, the
Upstream-robustness deferred-surface ledger (the pending queue, `retry_budget`,
`max_connection_pools`, multi-priority, `per_try_timeout`, TCP/gRPC health checks), the 14.1
M-track items, M-c1/M-c2/M-c3, the §6.9 per-class extension, and **ADR-0028** (H1-listener ×
H2-cluster dispatch deferral — REMAINS OPEN; phase 18 does not engage it). Phase 18 adds its own
deferral ledger via ADR-0048 §4 (file watching/hot reload [the family's prime follow-up],
file-based LDS/RDS/EDS/SDS/RTDS, the gRPC/ADS transport + the ADR-0014 protos supersession, delta
xDS, `initial_fetch_timeout`, REST xDS).

---

## 5. §7.5 phase-done gate re-attestation

The state-4 verification (PROGRESS Task 11) ran gates (a)–(e) ALL GREEN at HEAD `f5873902a` with
CI anchor `26863221247` (`completed / success`, both jobs green). **This review produced no code
changes**, so the state-4 record stands as the phase-done evidence; the review's own
re-verification is the per-cluster local test re-runs:

| Gate | State-4 evidence (HEAD `f5873902a`, CI `26863221247`) | Review re-verification (HEAD `be7768617`, local, read-only) |
|---|---|---|
| (a) fixture 0026 green | Bilateral first-run pass locally + green on Linux in the CI anchor (`xds_file_based_cds` ok) | Unchanged code; assertion set re-traced 1:1 against the L1–L12 lock-ins + ADR-0049 (cluster 3/4) |
| (b) 25 pre-existing fixtures green | All 26 green simultaneously in the CI anchor run | Unchanged code; inertness re-verified structurally (§1.3) + fixtures 0020–0025 proven byte-untouched (§1.5) |
| (c) h2spec ≥95% | `h2spec_pass_rate_gate ... ok` (CI; phase 18 touches no H2 framing) | Unchanged |
| (d) fuzz clean | `Done 200000 runs`, 0 crashes, 29-seed corpus + CI fuzz job success | Corpus gate re-run green (1/0); arithmetic re-verified 29 = 25 + 3 + 1 (cluster 4 + controller) |
| (e) 5 stable gates | build/clippy/fmt/deny clean; workspace test 1052 passed / 1 documented-flake failure cleared on isolated re-run | `cargo test -p envoy-config` 324/0; `-p envoy-cluster` 88/0; `-p envoy-admin` 79/0; `-p differential --lib` 119/0/1; backstop 5/0; per-crate clippy clean (clusters 1–4) |
| standalone builds (`project_isolated_crate_build_blindspot`) | 4/4 clean | `cargo build -p envoy-config` re-run clean (cluster 1) |
| (f) REVIEW.md approved | — | **THIS document — APPROVED** |

Because this review lands no code, the CI run triggered by this commit's push is docs-only
(vacuous-green expected); the state-4 CI anchor `26863221247` remains the phase's differential
evidence. No §5.2 state-3 re-entry condition exists.

---

## 6. ADR projection

**No new ADR.** The review found no decision-level divergence: the implementation faithfully
realizes ADR-0048 (the family pick + the three §0 findings + the minimum-viable scope — verified:
no protos/tonic/control-plane machinery landed; `ClusterManager` remains immutable
post-construction; every deferred surface still rejects loudly via `deny_unknown_fields`) and
ADR-0049 (the four reconciliations — verified at every site: the `@type`-required always-YAML
parser, the all-fatal L4 posture, the static-wins L9 merge, the L12 defer-then-revalidate
enforcement). Ledger head stays **ADR-0049** (count 50; next available **ADR-0050**, never
consumed by the split that did not fire). **ADR-0014 remains in force** (extended, not
superseded). **ADR-0028 remains OPEN** — phase 18 does not engage it.

---

## 7. Verdict + next state

**APPROVED.** Zero Critical; zero Important; 10 non-gating Minors (M18-1…M18-10) + 5 carryforward
dispositions (including the fuzz-corpus-inconsistency CLOSURE and the main-template-only-scan
bug-class closure) are recorded above with no named owner. No in-review fix was needed — the
per-task two-stage review discipline of the state-3 arc (2 Criticals + 3 Importants + ~15 Minors
all found and fixed in-task/in-arc pre-push) left nothing gating for state 5.

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + §5.1 (one state per session), the **next session performs
the state-6 deterministic close-out**: verify the docs-only CI run covering this review's push is
green, flip ROADMAP row `18` `in-progress → done` (a non-split top-level phase flips its own row
alone), advance STATE.md to "AWAITING NEXT PLANNING", append the `### Phase-18 rollovers` Notes
subsection (recording the M18-1…M18-10 inventory + the carryforward dispositions + the xDS
family's deferred-surface ledger headed by file watching/hot reload), and land the §5.3-format
final phase commit (`phase 18: xDS family opener — file-based CDS … [ADR-0048, ADR-0049]` with the
`Differential surface:` + `Conformance:` trailer lines). **After that close-out, the xDS / dynamic
config family has its opener landed** and the next brainstorm picks the next phase (the family's
prime follow-up — file watching/hot reload — or a different §9 family, on the merits).
