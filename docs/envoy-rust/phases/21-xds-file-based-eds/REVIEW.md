# Phase 21 (`21-xds-file-based-eds`) — Code Review

> **Lifecycle state 5** (`BOOTSTRAP_PROMPT.md` §5 — verified, not reviewed → `superpowers:requesting-code-review` → REVIEW.md). This review covers the phase-21 state-3 execution arc (Tasks 1–10 code) + the Task-11 state-4 verification. **Verdict: APPROVED.**

**Reviewed range:** `38f9470d2..1a3dad14f` (10 task code commits + 10 PROGRESS commits; HEAD `9abeeb6ba` adds only the Task-11 state-4 docs). Working tree clean.

**Method (the phase-16/17/18/19/20 state-5 precedent):** four read-only review subagents dispatched **SERIALLY** (`feedback_serial_subagent_dispatch`) over concern-clusters — A: schema+parser+merge (T1–T3); B: stats+config_dump (T4–T5); C: harness+fixture 0029+backstop (T6–T8); D: fuzz seed+BEHAVIOR_CONTRACT (T9–T10). Each re-ran its relevant non-Docker suites + targeted clippy. The controller then independently spot-verified the load-bearing claims by direct grep at HEAD before accepting cluster verdicts.

---

## Verdict

**APPROVED — 0 Critical / 0 Important / 7 Minor.** All four concern-clusters CLEAN; all five named state-5 focus items verified PASS. The single reviewer-rated "Important" (cluster A) was **downgraded to a non-gating Minor (M21-1) by controller spot-verification** (see below) — it is confined to the explicitly-DEFERRED CDS+EDS composition path (SPEC §4 / ADR-0053), is continuous with pre-existing `validate()` structure untouched by phase 21, and affects no shipping or tested path. This is the fifth consecutive clean state-5 (after 17, 18, 19, 20).

Per `BOOTSTRAP_PROMPT.md` §5.2 the re-enter-state-3 trigger is a Critical or Important finding; there are none after adjudication. The state-6 deterministic close-out (flip ROADMAP row `21` `in-progress → done`; STATE → AWAITING NEXT PLANNING) is the NEXT session.

---

## Named focus-item verdicts (all PASS)

1. **§5.7 merge-ordering soundness — PASS.** The EDS pass (`lib.rs:799-852`) runs AFTER the CDS+LDS+RDS merges and BEFORE the single post-merge `validate()` (`lib.rs:865-871`); the validate-gate AND the EDS-pass trigger both extend to any cluster with `cluster_type == Eds` regardless of `dynamic_resources` (`|| had_eds_cluster`, `lib.rs:868`) — fixture 0029 is a static-but-EDS bootstrap with NO `dynamic_resources` (C16). `check_endpoint_sources` is re-run over the merged static+CDS set (`lib.rs:749`), before the EDS pass populates anything. The two-field `&mut` split-borrow is scoped (`lib.rs:812-852`) so it ends before `validate(bootstrap)`. Tested: `load_dynamic_eds_validate_gate_fires_static_only` (`bootstrap.rs:7775`) + `load_dynamic_eds_empty_endpoints_is_fatal_post_merge` (`:7707`).
2. **D1 `load_assignment`→`Option` migration completeness — PASS.** Only three production `.load_assignment` reads remain (`cluster.rs:738`, `bootstrap.rs:4152`, `endpoint.rs:605`), each immediately `.as_ref()`. `cargo build --workspace --all-targets` is green (the ~17-site struct-literal sweep is complete). The endpoint-build `.expect("load_assignment populated post-load — §5.3 invariant")` (`cluster.rs:740`) cannot fire for an EDS cluster: the EDS pass populates `Some` before `ClusterManager::from_bootstrap` runs, and the post-merge validate re-checks.
3. **The exactly-one-of-and-consistent validator placement — PASS.** `MissingEdsClusterConfig`/`MissingLoadAssignment`/`EdsConfigOnNonEdsCluster` live in `validate_cluster` (`bootstrap.rs:2304-2321`) — these never false-positive (the merge never adds an `eds_cluster_config` or removes a `load_assignment`). The parse-time-only `LoadAssignmentOnEdsCluster` lives in `check_endpoint_sources` (`bootstrap.rs:2601-2610`), NOT in `validate_cluster`, so it does not false-positive the post-merge `(Eds, Some, Some)` loaded state (which `validate_cluster`'s `_ => {}` arm tolerates). The `!is_eds` name-mismatch carve (`bootstrap.rs:2333`) prevents falsely rejecting an EDS cluster whose populated CLA `cluster_name` equals `service_name` (L8), not the cluster name. Distinction tested both ways.
4. **§5.2 inertness — PASS.** (a) the `cluster_type == Eds` gate on the `cluster.<name>.update_*` registration (`cluster.rs:965`) — STATIC/STRICT_DNS register none (test `eds_stats_not_registered_for_non_eds_clusters`, incl. a CDS-configured cluster); (b) the `EndpointsConfigDump` push is guarded by `if !static_endpoint_configs.is_empty()` (`endpoint.rs:616`) — no EDS cluster → no entry (test `non_eds_clusters_emit_no_endpoints_config_dump`); (c) the harness `{{EDS_PATH}}`-only `needs_eds` gating leaves fixtures 0001–0028 fully inert (no Docker call); fixtures 0014/0026/0027/0028 `configs[]` indices NOT displaced. The in-process backstop's inertness witness (case viii) positively asserts absence.
5. **The §6.2 reconciliation soundness (ADR-0054) — PASS.** (a) `discover_host_gateway_ip()` (`tests/differential/src/lib.rs:1014`) uses `getent ahostsv4` (the IPv4-only DB — the implementer's documented swap from the PLAN's `getent hosts` after finding it returns only IPv6 on macOS Docker Desktop), gated to `needs_eds` (`:2367`); (b) the per-side `{{EDS_BACKEND_IP}}` marker is the discovered numeric gateway IP (upstream, `:2665`) vs `127.0.0.1` (subject, `:2723`) — grep confirms NO `host.docker.internal` reaches the EDS endpoint address (L1, EDS rejects hostnames); (c) the `/config_dump?include_eds` scrape + the per-side `JsonSubtreeRule` index reconciliation (Envoy `configs.2` / envoy-rust `configs.1`) reuses the ADR-0052 mechanism — no new harness JSON code; the admin query-string strip (`endpoint.rs:99`) routes `?include_eds` to the ConfigDump handler.

---

## Strengths

- **Schema surgery is exact and idiomatic.** `EdsClusterConfig` (`bootstrap.rs:130-138`) reuses `ConfigSource` verbatim with `deny_unknown_fields`; `load_assignment: Option<LoadAssignment>` + `eds_cluster_config: Option<…>` both carry `#[serde(default, skip_serializing_if = "Option::is_none")]`; `ClusterType::Eds` relies on the existing `rename_all = "SCREAMING_SNAKE_CASE"` (no per-variant rename).
- **`eds.rs` faithfully mirrors `cds.rs`** (`@type`-tagged envelope, `ClusterLoadAssignment` → the existing `LoadAssignment` struct reused verbatim). The in-arc deviation from the PLAN sketch (dropping the explicit `version_info` field) correctly matches the real `CdsFile`/`LdsFile`/`RdsFile` idiom — a justified consistency improvement, not a gap.
- **The EDS `update_*` family** register-and-sets directly (1/1/0/0; `cluster.rs:965-975`) with no handle threading and no `main.rs` call site, exactly the L3 simplification; the membership-gauge narrowing (no `membership_total`; `membership_healthy` stays HC-gated at `cluster.rs:936`) is respected and the pre-existing inertness test is intact and unweakened.
- **The `EndpointsConfigDump` borrows** (`endpoint.rs:408-430`) reuse the same `Vec<LocalityLbEndpoints>` type the canonical `LoadAssignment` holds, so endpoint serialization is byte-identical to the inline-CLA shape; no `Clone` cascade.
- **The harness reconciliation is clean** — discovery gated to EDS fixtures, the EDS rendition added to BOTH the `scan_needs_marker` backend-port sources AND the `uses_host_gateway` sources (closing the phase-18 scan-miss bug-class), the per-side `JsonSubtreeRule` reused with zero new JSON code.
- **The backstop is thorough** (`crates/envoy-bin/tests/xds_file_based_eds.rs`): all 8 cases; the 6 negative paths each assert a non-zero process exit AND the specific error needle AND that the listener never accepts; membership gauges never asserted.
- **BEHAVIOR_CONTRACT additions are an accurate, thorough projection of ADR-0054's L1–L11** — every load-bearing number and posture (stat values 1/1/0/0; `static_endpoint_configs` not `dynamic_endpoint_configs`; `?include_eds`-gating; configs[2]/[1]; all-fatal posture; 6a/6b/6c; L8; numeric-IP; membership narrowing with exact `cluster.rs:926`/`:2227` citations) cross-checks clean.
- **The fuzz seed parses clean** through the exact `parse_bootstrap` entry point; corpus/.gitignore atomically consistent at 32 (`connect_timeout` correctly omitted — not an envoy-rust `Cluster` field).

---

## Issues (all non-gating)

### Critical (Must Fix)
None.

### Important (Should Fix)
None (after controller adjudication — see M21-1 below).

### Minor (Nice to Have)

- **M21-1 (cluster A; reviewer-rated Important, controller-downgraded to non-gating).** `validate()`'s post-merge per-cluster loop (`bootstrap.rs:2096-2098`) iterates only `bootstrap.static_resources.clusters`; dynamic CDS clusters get `validate_cluster` at *parse* time inside `cds::parse_cds_file` (`cds.rs:69`), where a CDS-supplied `type: EDS` cluster has `load_assignment: None` and passes. The EDS pass then populates that cluster's `load_assignment`, but the post-merge `validate()` skips it — so a cluster that is **both CDS-supplied and `type: EDS`** with, e.g., an empty-endpoints CLA would escape `EmptyClusterEndpoints` post-merge.
  - **Why non-gating:** (1) the CDS-supplied-EDS composition is **explicitly DEFERRED** (ADR-0053 / SPEC §4: "the EDS-cluster-supplied-by-CDS composition showcase … phase 21 anchors EDS on a static cluster; the D3 walk covers `all_clusters()` so it is composition-ready") — no shipping or tested path exercises it; (2) the shipping fixture 0029 is a *static* EDS cluster, which IS in `static_resources.clusters` and IS fully re-validated post-merge (tests `load_dynamic_eds_empty_endpoints_is_fatal_post_merge` + `load_dynamic_eds_validate_gate_fires_static_only`); (3) the loop at `:2096` was **NOT touched by phase 21** (confirmed by `git log -L` over the range) — it is pre-existing `validate()` structure (only static clusters get the post-merge per-cluster gauntlet; CDS clusters are validated at parse time, which suffices for inline-endpoint clusters), and `effective_clusters` is used for route-reference resolution so route refs to such a cluster still resolve.
  - **Recommended fix (for the future CDS+EDS composition phase):** change the loop at `bootstrap.rs:2096` to iterate the `effective_clusters` snapshot (already collected at `:2085-2090`) instead of `&bootstrap.static_resources.clusters`, OR have the EDS pass call `validate_cluster` on each cluster it populates; add a test driving a valid CDS EDS cluster with an empty-endpoints CLA. Owner: the deferred CDS+EDS composition phase.
- **M21-2 (cluster C).** `discover_host_gateway_ip()` (`tests/differential/src/lib.rs:998`) does not check `out.status` after `docker run`; on a container-start failure it surfaces the generic "no numeric IPv4" error rather than docker stderr. Still fails fast with a diagnostic. Fix: on `!out.status.success()`, bail including `String::from_utf8_lossy(&out.stderr)`.
- **M21-3 (cluster C).** The backstop helper block (`reserve_port`/`wait_ready`/`http1_oneshot`/`spawn_envoy_bin`/…, `crates/envoy-bin/tests/xds_file_based_eds.rs:20-28`) is copied verbatim from `xds_file_based_rds.rs` — the M18-9 extract-a-test-support-crate item, now at **N≥5** backstops. Recorded future-hardening item.
- **M21-4 (cluster C).** The unit test `eds_marker_scan_detects_eds_path_token` (`tests/differential/src/lib.rs:7465`) re-implements the `contains("{{EDS_PATH}}")` disjunction inline rather than binding the real `run_fixture` gate (the real gate is exercised by the Docker fixture). Cosmetic.
- **M21-5 (cluster D).** The BEHAVIOR_CONTRACT L4 table (`BEHAVIOR_CONTRACT.md:124`) adds a 5th row ("unknown field inside a resource → envoy-rust FATAL `deny_unknown_fields`") beyond ADR-0054's L4 (a)–(d) enumeration. It is accurate and consistent with ADR-0054 Decision 4's all-fatal posture — an enrichment, not a contradiction. Optionally annotate it as a contract-author extension.
- **M21-6 (cluster B).** `mk("update_failure")?;` / `mk("update_empty")?;` (`cluster.rs:965-975`) produce registered-but-unheld `Arc<Counter>` handles (the registry owns them); correct and matches the CDS template idiom, intent clear via the `// registers at 0 (L4)` comment. No change needed.
- **M21-7 (cluster A).** No test exercises a CDS-supplied cluster that is itself `type: EDS` getting populated (only the rejection path is covered). Adding one would document and guard M21-1. Owner: the deferred CDS+EDS composition phase.

---

## Carried future-hardening items (NOT phase-21 blockers)

- **The `parse_xds_file<T>` parser consolidation (M19-1)** + **the dynamic-file-render-helper (M20-T6-a)** are now at **N=4** copy-paste siblings (`cds.rs`/`lds.rs`/`rds.rs`/`eds.rs` + the four harness render blocks) — DEFERRED by deliberate risk-managed decision (PLAN C18: consolidating touches currently-green modules + the green harness for a cleanliness win, against the D-3.6 every-phase-green doctrine).
- **The extract-a-shared-test-support-crate item (M18-9)** is now at **N≥5** backstops (M21-3).
- **The CDS+EDS post-merge re-validation hole (M21-1) + its missing test (M21-7)** — owned by the future CDS+EDS composition phase.

---

## Verification posture (state-4 evidence relied upon)

The §7.5 phase-done gate evidence was captured at the Task-11 state-4 verification (`PROGRESS.md` Task-11 block):
- **(e)** `cargo build`/`clippy`/`fmt`/`test --workspace` + `cargo deny check` + the 4 standalone-crate builds (`project_isolated_crate_build_blindspot`) green locally; the review re-ran the per-cluster suites (envoy-config 392 passed; envoy-cluster 91; envoy-admin 95; differential lib 127 passed / 2 Docker-ignored; envoy-bin EDS backstop 8/8) + targeted clippy clean.
- **(d)** the 200k-run fuzz clean on the 32-seed corpus + the CI fuzz job green.
- **(a)+(b)+(c)** the **Docker-gated CI anchor `27043160204`** (HEAD `1a3dad14f`, ubuntu-latest, `conclusion=success`) — **fixture 0029 ran + PASSED on Linux** (`xds_file_based_eds_fixture ... ok`) alongside all pre-existing fixtures incl. the CDS/LDS/RDS triad + the h2spec ≥95% gate. The first CI attempt's sole failure was the documented unrelated `access_log_file_sink` access-log-race flake (`project_flaky_access_log_fixture_0012`), cleared by `gh run rerun --failed` — not a regression.

`#![forbid(unsafe_code)]` intact across all touched crates; no `unsafe`; `thiserror` in library crates (no `anyhow` leakage).

---

## Assessment

**Ready to merge: Yes.** Phase 21 implements file-based EDS exactly per the PLAN/ADR-0053/ADR-0054 lock-ins with strong TDD coverage, clean clippy, and bilateral Docker-gated CI evidence on Linux. All five named focus items pass under independent controller spot-verification; the only above-Minor finding was a re-validation gap on the explicitly-deferred CDS+EDS composition path, continuous with pre-existing `validate()` structure and affecting no shipping path — recorded as non-gating M21-1 for the future composition phase. No Critical or Important issues remain; the §5.2 re-enter-state-3 trigger does not fire. Proceed to the state-6 close-out.
