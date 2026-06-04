# Phase 20 (`20-xds-file-based-rds`) — REVIEW

- **Lifecycle state:** state 5 (verified → reviewed). This document is the state-5 output.
- **Skill:** `superpowers:requesting-code-review` (per `BOOTSTRAP_PROMPT.md` §5 state 5 + `SKILL_ROUTING.md`).
- **Review range:** `20b4d2daf..385656f21` — the Task 1–10 state-3 execution arc, atop the state-2
  PLAN-write base `20b4d2daf` (its code tree is identical to the pre-phase-20 baseline). The CODE
  commits reviewed: `ba34125f7` (T1 `rds` schema + `route_config`→`Option<RouteConfiguration>` +
  the parse-time exactly-one-of `check_route_sources` + 5 `ConfigError` variants + the D1
  11-construction-site migration sweep) → `c1219d6ff` (T2 RDS file parser `rds.rs`) → `67ab8bd7b`
  (T3 `load_dynamic_resources` RDS pass + §5.7 merge ordering + effective-`route_config` population +
  the `check_route_sources` re-run over the merged set) → `668bfbd7c` (T4 conditional per-HCM
  `http.<stat_prefix>.rds.<route_config_name>.*` stats via `register_rds_stats`) → `b1ef487da` (T5
  `RoutesConfigDump` conditional emission, pushed after Listeners) → `5a39a5c63` (T6 harness shared
  `{{RDS_PATH}}` rendering/mounting + the per-side `JsonSubtreeRule` path override) → `a4d136265`
  (T7 fixture `0028-xds-file-based-rds` + Docker wrapper) → `9eb65b26a` (T8 in-process backstop,
  happy + 6 negative + inertness) → `d19be5147` (T9 fuzz seed `hcm_rds_route_config.yaml`, corpus
  30→31) → `865a2e0ab` (T10 BEHAVIOR_CONTRACT RDS rows). The PROGRESS-subsection commits in the range
  carry NO code. **No in-review fix commits were needed.**
- **Pre-review HEAD:** `ef7c5e19a` (== `origin/main` at review start; the Task-11 state-4 verification
  + STATE advance, docs-only; everything pushed).
- **Method:** 4 read-only code-review subagents, one per concern-cluster, dispatched **SERIALLY**
  (`feedback_serial_subagent_dispatch`), each reading the actual on-disk diff
  (`git diff 20b4d2daf..385656f21` + `Read` + `Grep`) and re-running the relevant non-Docker test
  suites (`cargo test -p envoy-config` 368/0, `-p envoy-admin` 91/0, `-p envoy-listener` 36/0,
  `-p differential --lib` 126/0/1, the in-process backstop `-p envoy-bin --test xds_file_based_rds`
  8/0, the fuzz-corpus replay gate `fuzz_corpus_seeds_parse_or_reject_cleanly` green). The controller
  independently spot-verified the load-bearing claims every cluster verdict rests on (the §5.7
  CDS→LDS→`check_route_sources`-re-run→RDS-populate→single-`validate()` ordering + the `|| had_rds_hcm`
  gate; the `validate_hcm` `None`-early-return + the parse-time `check_route_sources`; the
  `register_rds_stats` `rds.is_some()` gate + the 1/1/1/0/0 deterministic values; the `RoutesConfigDump`
  `if !is_empty()` conditional push + the after-Listeners ordering; the `envoy.yaml`/`envoy-rust.yaml`
  header-hygiene-only asymmetry; the per-side `JsonSubtreeRule` accessors) by direct grep/read against
  HEAD before accepting them.
- **Verdict: APPROVED** (zero Critical / **zero Important** / non-gating Minors carried). The fourth
  consecutive phase (after 17, 18, 19) to clear state-5 review with no in-review fix and no Important
  finding.

---

## 1. The named review focus (the STATE.md state-5 charter — the items this session MUST verify)

### 1.1 §5.7 merge-ordering soundness — **VERIFIED: no config path reaches a runtime route → cluster-lookup miss**

The single-post-merge-revalidation invariant holds end-to-end. `load_dynamic_resources`
(`crates/envoy-config/src/lib.rs`) runs the CDS merge (`:659`) → the LDS merge (`:693`) →
`check_route_sources` re-run over the merged listener set (`:701`) → the RDS pass (`:715-749`:
HCM walk + `std::fs::read_to_string` → `RdsFileError`, `parse_rds_file` → `RdsParseError`,
`position(...).map(remove).ok_or(RdsRouteConfigNotFound)` name-selection, `hcm.route_config = Some(selected)`)
→ exactly ONE `bootstrap::validate(bootstrap)?` (`:761`) gated on
`dynamic_clusters.is_some() || dynamic_listeners.is_some() || had_rds_hcm`. CDS clusters merge BEFORE
the validation that re-checks route-references, so an RDS route resolves against a dynamic cluster
(the fixture-0028 composition; test `rds_route_to_cds_cluster_resolves`). The deliberate **L7
divergence** — an RDS route to a cluster in NEITHER list FAILS envoy-rust startup (vs Envoy's
runtime-503) — fails fatally and does not silently pass: the deferred route check enforces against
the merged cluster set and emits `UnknownCluster` (test `rds_unresolved_route_is_fatal` asserts
`UnknownCluster("nope")` — a real error via `matches!`, not a panic). Critically, the `|| had_rds_hcm`
disjunct makes the post-merge `validate()` fire for an **rds-only** bootstrap (cds and lds both
absent), so the populated route table is re-validated even when no other dynamic resource exists
(test `rds_empty_virtual_hosts_is_fatal_post_merge`). Controller-confirmed the gate/ordering by direct
read of `lib.rs:576,659,693,701,715-749,759-761`.

### 1.2 D1 `route_config`→`Option` migration completeness — **VERIFIED: no stragglers, no None-panic path**

Making `HttpConnectionManagerConfig.route_config` an `Option<RouteConfiguration>` is a
workspace-compile-affecting change; the migration is complete and sound. Controller grep at HEAD over
`crates/` for `.route_config` reads and `route_config:` constructions: the envoy-config-struct field
is `Option`; exactly the 11 envoy-config-struct construction sites migrated to `route_config: Some(...),
rds: None`; the 15 LOCAL `Arc::new(RouteConfiguration {...})` sites (the `HCMConfig.route_config:
Arc<RouteConfiguration>` field — `envoy-http1/src/hcm.rs:113`, the http2 alias) were correctly NOT
migrated (they are a different field of a different struct). The non-config read sites handle `None`
safely: `envoy-admin/src/endpoint.rs:595` uses `.as_ref().map(...)`; `envoy-http1/src/hcm.rs:1181`
reads the local `Arc` field, not the config `Option`. The sole production config-struct read,
`envoy-http1/src/hcm.rs:200` (`clone_route_config(cfg.route_config.as_ref().expect("route_config
populated post-load — §5.3 invariant"))`), documents a post-load invariant that genuinely holds: every
HCM resolves to `Some` after `load_dynamic_resources` — inline HCMs at parse, rds HCMs via the §1.1
RDS pass — and `from_config` only runs after that load completes. No production path reaches the
`expect` with `None`. Build green; all 368 envoy-config tests pass.

### 1.3 §5.2 inertness — **VERIFIED: structurally airtight on BOTH surfaces**

Both new observability surfaces gate behind a precise `hcm.rds.is_some()` predicate. Stat registration:
`register_rds_stats` (`crates/envoy-listener/src/lib.rs:402-431`) produces NO name unless BOTH the
HCM-filter `let-else` and `hcm.rds.as_ref()` `let-else` pass — an inline-route HCM (including one under
a `cds_config`/`lds_config` bootstrap) registers nothing (controller-verified the double `let-else`
gate at `:411-415`). config_dump emission: the `RoutesConfigDump` collection filters on
`hcm.rds.is_some()` (`crates/envoy-admin/src/endpoint.rs:594`) and the push is guarded by
`if !dynamic_route_configs.is_empty()` (`:606`), placed AFTER the Listeners push (`:580`) so a non-rds
fixture's `ClustersConfigDump` stays at `configs[1]` and does not shift. Absence is proven on both
sides and in a real process: `register_rds_stats`' test `rds_stats_not_registered_without_rds_hcm`
loops all four `(lds, cds)` combinations asserting ZERO `.rds.` names; the admin tests
`inline_route_hcm_emits_no_routes_config_dump` + `plain_bootstrap_emits_no_routes_config_dump` assert
no Routes entry; the backstop's inertness case (viii) spawns a genuine INLINE-route process and asserts
the absence of both a `RoutesConfigDump` entry and any `.rds.` stat. Fixtures 0014/0026/0027 carry zero
`rds:` keys (controller-confirmed) and therefore cannot gain either surface — the 27-pre-existing-fixture
regression-equivalence rests on this.

### 1.4 Exactly-one-of placement (C16) — **VERIFIED: the post-load both-Some state is valid, not falsely rejected**

The subtle correctness point of the phase. `check_route_sources`
(`crates/envoy-config/src/bootstrap.rs:2502`) is the SOLE cardinality gate — it returns
`MissingRouteSource` (neither) / `AmbiguousRouteSource` (both) and is called at PARSE time
(`lib.rs:576`, before any file is read) AND re-run post-LDS-merge (`lib.rs:701`) BEFORE the RDS pass
populates anything — so at both call sites an rds HCM still has `route_config: None` and the
`(true, true)` arm cannot trip on the post-load state. After the RDS pass sets `route_config = Some(...)`
an rds HCM has BOTH fields `Some`; this loaded state is NEVER re-checked for cardinality, so it stays
valid (and `rds.is_some()` remains the stats/config_dump predicate). `validate_hcm`
(`bootstrap.rs:2395-2396`) early-returns `Ok(())` on `route_config: None` (an rds HCM pre-load) with no
cardinality re-check, and is never the cardinality gate. The merged-set re-run is test-witnessed by
`lds_hcm_with_no_route_source_is_missing_route_source` (an LDS-supplied HCM with neither source caught
post-merge). Controller-confirmed by direct read of `bootstrap.rs:2395,2502` + `lib.rs:576,701`.

### 1.5 Fixture-0028 byte-exact-body Envoy-side stripping soundness — **VERIFIED: no false-equivalence**

The highest-stakes item — the first SHARED-route + byte-exact-body fixture. Controller diffed
`tests/fixtures/0028-xds-file-based-rds/envoy.yaml` vs `envoy-rust.yaml`: the ONLY substantive
differences are (a) the standard harness bind-address split (`0.0.0.0` for the in-container Envoy vs
`127.0.0.1` for the envoy-rust subprocess — the established 0008/0026/0027 pattern) and (b) the
Envoy-only header-hygiene knobs in the MAIN config (`generate_request_id: false`; an
`envoy.filters.http.header_mutation` filter removing `x-forwarded-for`/`x-forwarded-proto` on the
request and re-adding `x-envoy-upstream-service-time` on the response; the router's
`suppress_envoy_headers: true`). These are pure header hygiene that converge Envoy's forwarded-request
shape DOWN to envoy-rust's native shape — verified against source: envoy-rust's H1 router natively
injects `x-envoy-upstream-service-time` (`crates/envoy-http1/src/router.rs:159`), so the presence-only
`require_header_present` assertion holds on both sides; and envoy-rust has NO code path injecting
x-request-id / x-forwarded-* / x-envoy-expected-rq-timeout-ms onto the forwarded request, so the Envoy
strippers do not mask a behavioral divergence. No Envoy-only field alters routing or the echoed body
fields (`method`/`path`/`host`/`body`); no stripper lacks a native envoy-rust mirror. The shared
`rds.yaml` carries NO `validate_clusters` (L7) and none of the deny-unknown-rejected Envoy-only route
fields (it is rendered per-side through the kv map, identical both sides). The byte-exact body strings
are the same shape fixture 0027 already proves green for the identical HTTP1 static/dynamic echo
topology. The fixture is green for the right reason.

---

## 2. Cluster verdicts

| # | Concern cluster | Tasks | Verdict | Critical | Important | Minor |
|---|---|---|---|---|---|---|
| 1 | `envoy-config` schema + RDS parser + loader/merge/migration | T1, T2, T3 | **CLEAN** | 0 | 0 | 1 (the documented `expect`) |
| 2 | Conditional `http.*.rds.*` stats + `RoutesConfigDump` | T4, T5 | **CLEAN** | 0 | 0 | 2 (M20-S5-1, M20-S5-2) |
| 3 | Harness `{{RDS_PATH}}` + per-side `JsonSubtreeRule` + fixture 0028 + backstop | T6, T7, T8 | **CLEAN** | 0 | 0 | 2 (M20-S5-3, M20-S5-4) |
| 4 | Fuzz seed + BEHAVIOR_CONTRACT extensions | T9, T10 | **CLEAN** | 0 | 0 | 0 |

Cluster-1 highlights: the §5.7 single-validation merge ordering traced end-to-end (§1.1); the RDS pass
takes a disjoint two-field mutable borrow mirroring `all_listeners()` ordering, with `had_rds_hcm` a
`Copy` bool so nothing is held past the loop; name-selection MOVES the selected RouteConfiguration out
(no clone) and is panic-free; `rds.rs` is an idiom-faithful mirror of `lds.rs` (envelope without
`deny_unknown_fields` so `version_info` is accept-and-ignored; the single-variant `@type` enum rejects
non-RouteConfiguration and missing-`@type` loudly); 13 new envoy-config tests are real and
behavior-driven (real tempfiles; the L7 resolves-case AND the L7 UnknownCluster-not-panic case; the
post-merge empty-vhosts fatal case). Cluster-2 highlights: both inertness guards are correct-polarity
and the §5.2 invariant is structurally airtight (§1.3); the L5 `RoutesConfigDump` shape matches the
lock-in exactly (the `#[serde(flatten)]`-on-nested-`&RouteConfiguration` pattern mirroring
`TaggedCluster`/`TaggedListener`, NO `version_info` anywhere, an explicit no-`version_info` test
assertion); `register_counter` is idempotent so a duplicate name cannot crash startup; the deterministic
1/1/1/0/0 values are valid under the L4 all-fatal posture (no handle threading). Cluster-3 highlights:
the fixture asymmetry is sound with no false-equivalence (§1.5); the per-side `JsonSubtreeRule` is
backward-compatible (the shared-`path` path is bit-identical, regression-tested) and the per-side
override is proven by an active DECOY test (a `WRONG` value planted at the rust index in the Envoy body);
`{{RDS_PATH}}` is genuinely SHARED (one file rendered twice) and joins BOTH the backend and host-gateway
scans (the phase-18 "scan ALL rendered sources" lesson); the 8-case backstop uses `reserve_port()` +
`kill_on_drop(true)`, asserts non-zero exit + a specific error-text needle on every negative, and locates
`RoutesConfigDump` by SCANNING `configs[]` for the `@type` rather than a brittle hard-coded index.
Cluster-4 highlights: the fuzz seed is minimal/correct/atomic/git-tracked and IS in the
`fuzz_corpus_seeds_parse_or_reject_cleanly` replay list (regression protection matching the cds/lds
seeds); every load-bearing BEHAVIOR_CONTRACT claim traces 1:1 to the emitters at HEAD (the 5 stat
names+values vs `register_rds_stats:417-427`; the 6 `ConfigError` variant names vs `lib.rs`; the
`configs[4]`/`configs[2]` indices vs fixture 0028 `expectations.yaml` + `endpoint.rs`; the
no-`version_info` shape vs the Task-5 structs) with zero stale CDS/LDS copy-paste.

---

## 3. Controller verification notes

Per the phase-16/17/18/19 state-5 method, the controller did not accept cluster verdicts on faith:

1. **§5.7 ordering + the rds-only gate** (§1.1): read `lib.rs:576,659,693,701,715-749,759-761` — the
   order is CDS-merge → LDS-merge → `check_route_sources` re-run → RDS-populate → single `validate()`
   gated on `dynamic_clusters || dynamic_listeners || had_rds_hcm`. The `|| had_rds_hcm` disjunct fires
   the re-validation for an rds-only bootstrap. MATCHES cluster 1.
2. **The C16 exactly-one-of placement** (§1.4): read `bootstrap.rs:2395` (`validate_hcm` early-returns
   `Ok(())` on `None`, no cardinality re-check) and `:2502` (`check_route_sources` is the sole gate),
   plus its two call sites `lib.rs:576` (parse) + `:701` (post-LDS-merge, pre-RDS-populate). The
   post-load both-Some state is never re-checked. MATCHES cluster 1.
3. **The stat gate + values** (§1.3): read `register_rds_stats` (`envoy-listener/src/lib.rs:402-431`)
   — double `let-else` (`HttpConnectionManager` then `rds.as_ref()`), base
   `http.{stat_prefix}.rds.{route_config_name}`, `update_attempt`/`update_success`/`config_reload`
   `.add(1)`, `update_failure`/`update_rejected` registered at 0. MATCHES cluster 2.
4. **The config_dump conditional push + ordering** (§1.3): read `endpoint.rs:594` (filter on
   `hcm.rds.is_some()`), `:606` (`if !dynamic_route_configs.is_empty()` guard), `:580` (Listeners push)
   then `:607` (Routes push) — Routes lands after Listeners; for fixture 0028 (cds yes, lds no) Listeners
   is gated off so Routes is at `configs[2]`. MATCHES cluster 2.
5. **The fixture asymmetry** (§1.5): `diff envoy.yaml envoy-rust.yaml` — only the bind-address split +
   the Envoy-only header-hygiene knobs differ; the shared `rds.yaml` carries no `validate_clusters` and
   no deny-unknown-rejected fields. MATCHES cluster 3.
6. **The per-side `JsonSubtreeRule`** (§1.3/§1.5): read `lib.rs:615-639` — `path` is `#[serde(default)]`,
   `path_envoy`/`path_envoy_rust` default `None`, accessors `*_path()` fall back to `self.path`. Legacy
   shared-`path` rules in 0014/0026/0027 deserialize unchanged. MATCHES cluster 3.

---

## 4. Carryforward dispositions + Minor findings (non-gating)

### 4.1 Arc-discovered carryforwards (from PROGRESS + the state-4 inventory; reviewed + dispositioned)

1. **The fuzz-corpus consistency — STAYS CLOSED.** Phase 20's atomic three-way edit (seed +
   `.gitignore` allow-list line + the `fuzz_corpus_seeds_parse_or_reject_cleanly` replay entry, all in
   commit `d19be5147`) keeps the corpus consistent (30 → 31; controller-confirmed the seed is tracked,
   not-ignored, and in the replay list). No new inconsistency.
2. **The main-template-only-scan bug class (phase 18's only escaped Critical) — class stays closed.**
   Phase 20's harness extension honors the "scan ALL rendered sources" lesson: the RDS rendition joins
   both the backend-detection and host-gateway scan families (cluster 3; controller-noted the shared
   `rds.yaml` carries cluster NAMES not host/port markers, so `rds_scan`'s backend membership is
   defensive-symmetry dead weight — M20-T6-b, honestly commented). No new silent scan site.
3. **CI readiness-flake family 0011/0012/0022 + the cold-helper-compile flake (PRE-EXISTING).** The
   state-4 `cargo test --workspace` first-run surfaced the phase-13.2 `upstream_h2_connection_pooling`
   H2-pooling backstop at a backend-readiness timeout under parallel load — root-caused to the documented
   cold-helper/readiness flake (`project_flaky_access_log_fixture_0012`; phase 20 touched no
   H2-pooling/helper code), green in isolation at 2.05s. **Disposition: carries unchanged.**
4. **M18-9 — the backstop-helper / test-support duplication (extract-a-test-support-crate) — N≥4,
   carries forward.** Phase 20 copied the backstop helper block verbatim from `xds_file_based_lds.rs`
   (Task 8; the N≥4 duplication is noted in the file header `:16-23`). Direct construction of the
   `#[serde(skip)]` side-fields requires hand-built `Bootstrap` values, so per-module self-containment is
   defensible; the standing extract-a-shared-test-support-crate item remains open for a future hardening
   phase. **Disposition: carries unchanged** (correctly not done inside a tests-only task).
5. **M19-1 / C17 — the `xds_file.rs` parser consolidation — DEFERRED by deliberate decision, carries.**
   Phase 20 wrote a new `rds.rs` mirroring `lds.rs` rather than consolidating `cds.rs`+`lds.rs`+RDS into
   a resource-parametric `xds_file.rs` (PLAN C17: a risk-managed choice to keep the two green CDS/LDS
   modules untouched under the less-margin budget + the D-3.6 every-phase-green doctrine). The consolidation
   is now a 3rd-sibling item; **M20-T6-a is its harness analogue** (the CDS+LDS+RDS dynamic-file
   render/write/guard block is now TRIPLICATED → "extract a dynamic-file render helper"). Both stay open
   for a future hardening phase.
6. **The standing multi-phase inventory** (the phase-17/18/19 rollovers; the Upstream-robustness
   deferred-surface ledger; **ADR-0028** [H1-listener × H2-cluster dispatch deferral — REMAINS OPEN]) —
   **phase 20 engages NONE of it; all carries unchanged.**

### 4.2 Minor findings (none gating; carried with no named owner)

**New at this state-5 review:**

| # | Finding | File | Why non-gating |
|---|---|---|---|
| M20-S5-1 | If two HCMs share BOTH the same `stat_prefix` AND the same `route_config_name`, the idempotent `register_counter` returns the same `Arc`, so each `mk(...).add(1)` runs again → `update_attempt`/`update_success`/`config_reload` would read **2**, not 1 (upstream Envoy keys an RDS subscription by `route_config_name`, so one shared subscription / single increment would be more faithful) | `crates/envoy-listener/src/lib.rs:425-427` | Requires an unusual config (two HCMs, identical prefix AND route name); outside the stated lock-ins; the asserted fixture-0028 + backstop topology has one rds HCM. A one-line dedupe (`HashSet` on `base`) or an acknowledging comment would close it. Latent-only |
| M20-S5-2 | The config_dump collection gates on `hcm.rds.is_some()` AND `hcm.route_config.as_ref()` (`endpoint.rs:594-595`), whereas `register_rds_stats` gates on `rds.is_some()` alone — the textual divergence is structurally harmless (`load_dynamic_resources` always populates `route_config` for an rds HCM, and an unresolvable name is fatal under L4), so the `None` arm is unreachable for a running server | `crates/envoy-admin/src/endpoint.rs:594-595` | Consistent in practice; the divergence is a readability nit, not a behavior difference. No action needed |
| M20-S5-3 | The fixture-0028 `path_envoy_rust` config_dump assertion holds only because envoy-rust folds the RDS-loaded table back into `hcm.route_config` (then the dump reads it); a future refactor that stopped populating `route_config` for rds HCMs would silently break the fixture | `crates/envoy-admin/src/endpoint.rs:589-606` | Documentation/awareness only; the fold is the intended §5.3 uniform-downstream-shape design (load-bearing for §1.2 too). No action now |
| M20-S5-4 | The backstop's `/static` probe sends `Host: dynamic_backend` (it routes correctly via the vhost `domains: ["*"]` prefix match, but the Host value is misleading for the `/static` case) | `crates/envoy-bin/tests/xds_file_based_rds.rs` | Cosmetic; the route table matches on path-prefix not host, so the assertion is valid. No behavior impact |

**Carried from the state-3 two-stage per-task reviews (PROGRESS, non-gating):** M20-T1-b (tests
(f)/(g) tighten `.is_err()` to `matches!`); M20-T3-a (a shared RDS file is re-read per HCM —
acceptable at startup); M20-T3-b (the CDS/LDS-vs-RDS borrow-strategy divergence is justified but
slightly under-documented); M20-T3-c (no merged-set `AmbiguousRouteSource` test — both arms share the
`all_listeners()` traversal so risk is near-zero); M20-T4-b (the inner `mk` closure is re-declared per
loop iteration — captures `base` by ref); **M20-T6-a (notable** — the CDS+LDS+RDS dynamic-file
render-block triplication → "extract a dynamic-file render helper", the harness analogue of the M19-1
parser consolidation); M20-T6-b (`rds_scan` defensive dead weight in `backend_scan_sources`); M20-T6-c
(the differential `lib.rs` is now ~7.3k lines — pre-existing largeness); M20-T7 (the
"envoy-rust emits `x-envoy-upstream-service-time` natively" note kept); M20-T8 (static_backend +
dynamic_backend point at the same in-process backend — the per-cluster distinction is established by
fixture 0028); M20-T10 (a stat-name abbreviation style mirroring the phase-19 LDS precedent).
**CLOSED in-arc:** M20-T1-a + M20-T1-c (Task 3), M20-T4-a (Task 8), M20-T4-c (Task 5); the Task-7
Important doc-inconsistency was fixed in-task.

### 4.3 Standing multi-phase Minor inventory (inherited; not engaged by phase 20)

The phase-17/18/19 rollovers, the Upstream-robustness deferred-surface ledger, and **ADR-0028**
(H1-listener × H2-cluster dispatch deferral — REMAINS OPEN; phase 20 does not engage it) all carry
forward unchanged. Phase 20 extends the xDS family's deferred-surface ledger via ADR-0051: file
watching/hot reload for ALL THREE file-based resource types (the family's prime follow-up — ROI improved
again: one watching phase now lights up CDS+LDS+RDS; its §6.2 verification MUST run on Linux CI per
ADR-0049 Provenance), `scoped_routes`/SRDS/VHDS, the LDS+RDS composition showcase,
multiple-`rds`-HCM / multiple-RouteConfig-per-file, file-based EDS/SDS/RTDS, the gRPC/ADS transport +
the ADR-0014 protos supersession, and delta xDS.

---

## 5. §7.5 phase-done gate re-attestation

The state-4 verification (PROGRESS Task 11) ran gates (a)–(e) ALL GREEN with CI anchor `26967529584`
(HEAD `385656f21`, `conclusion=success`, both jobs green). **This review produced no code changes**, so
the state-4 record stands as the phase-done evidence; the review's own re-verification is the per-cluster
local test re-runs:

| Gate | State-4 evidence (CI `26967529584`, HEAD `385656f21`) | Review re-verification (HEAD `ef7c5e19a`, local, read-only) |
|---|---|---|
| (a) fixture 0028 green | all 28 fixtures green simultaneously on Linux (bilateral) | Unchanged code; assertion set re-traced 1:1 against the L1–L11 lock-ins + the per-side header-hygiene asymmetry (cluster 3, §1.5) |
| (b) 27 pre-existing fixtures green | all 28 (`0001`–`0028`) green simultaneously in the CI anchor | Unchanged code; inertness re-verified structurally (§1.3) — conditional stat registration + conditional `RoutesConfigDump` emission + `{{RDS_PATH}}`-only harness gating |
| (c) h2spec ≥95% | green on Linux (phase 20 touches no H2 framing) | Unchanged |
| (d) fuzz clean | `Done 200000 runs`, 0 crashes, 31-seed corpus + CI fuzz job success | Corpus replay gate re-run green; seed tracked + in the replay list (cluster 4 + controller) |
| (e) 5 stable gates | build/clippy/fmt/deny clean; workspace test green (one cold-helper flake cleared) | `cargo test -p envoy-config` 368/0; `-p envoy-admin` 91/0; `-p envoy-listener` 36/0; `-p differential --lib` 126/0/1; backstop `-p envoy-bin --test xds_file_based_rds` 8/0 (clusters 1–4) |
| standalone builds (`project_isolated_crate_build_blindspot`) | 4/4 clean (`-p envoy-config`/`-p envoy-cluster`/`-p envoy-http1`/`-p envoy-http2`) | `cargo build -p envoy-config` re-run clean (cluster 1) |
| (f) REVIEW.md approved | — | **THIS document — APPROVED** |

Because this review lands no code, the CI run triggered by this commit's push is docs-only
(vacuous-green expected); the state-4 CI anchor `26967529584` remains the phase's differential evidence.
No §5.2 state-3 re-entry condition exists.

---

## 6. ADR projection

**No new ADR.** The review found no decision-level divergence: the implementation faithfully realizes
ADR-0051 (the xDS-family-continuation pick + the four §0 findings + the minimum-viable scope — verified:
no protos/tonic/control-plane machinery landed; the `ConfigSource`/`PathConfigSource` schema is reused
verbatim by `Rds`; no HCM runtime route-table mutability/locks/watch tasks; every deferred surface still
rejects loudly via `deny_unknown_fields`) and ADR-0052 (the §6.2 reconciliation — the `configs[]`
ordering divergence is bridged by the per-side `JsonSubtreeRule` path override, verified at fixture
0028's config_dump assertion + the harness accessors; the L1/L3/L4/L6/L7/L8/L9/L10 lock-ins all realized
at their sites). The §6.1 split gate did not fire, so the reserved **ADR-0053** never fired. Ledger head
stays **ADR-0052** (count 53; **ADR-0053** free for any future use). **ADR-0014 remains in force**
(extended a THIRD time by the RDS envelope, not superseded). **ADR-0028 remains OPEN** — phase 20 does
not engage it.

---

## 7. Verdict + next state

**APPROVED.** Zero Critical; zero Important; non-gating Minors (the 4 new M20-S5-1…M20-S5-4 + the carried
state-3 Minors + the carryforward dispositions) are recorded above with no named owner. No in-review fix
was needed — the per-task two-stage review discipline of the state-3 arc (review-fixes applied in-task on
Tasks 7/9; all other findings non-gating Minors recorded in PROGRESS; M20-T1-a/M20-T1-c/M20-T4-a/M20-T4-c
closed in-arc) left nothing gating for state 5.

Per `BOOTSTRAP_PROMPT.md` §5 state 6 + §5.1 (one state per session), the **next session performs the
state-6 deterministic close-out**: verify the docs-only CI run covering this review's push is green, flip
ROADMAP row `20` `in-progress → done` (a non-split top-level phase flips its own row alone), advance
STATE.md to "AWAITING NEXT PLANNING", append the `### Phase-20 rollovers` Notes subsection (recording the
M20-S5-1…M20-S5-4 + the carried state-3 Minor inventory + the carryforward dispositions + the xDS
family's deferred-surface ledger now headed by file watching/hot reload for ALL THREE file-based resource
types CDS+LDS+RDS), and land the §5.3-format final phase commit (`phase 20: xDS family — file-based RDS …
[ADR-0051, ADR-0052]` with the `Differential surface:` + `Conformance:` trailer lines). **After that
close-out, the xDS / dynamic config family's filesystem-transport surface covers the FULL CDS+LDS+RDS
data-plane triad**, and the next brainstorm picks the next phase (the family's prime follow-up — file
watching/hot reload — or a different §9 family, on the merits).
