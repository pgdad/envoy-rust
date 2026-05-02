# Phase 05.4 — Progress

Per-task running log. Append-only during execution; one section per task.
Source of truth: `SPEC.md` (D1–D7) + `PLAN.md` (Tasks 1–7).

---

## Task 1 — `envoy-config` `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum + ADR-0024

- **Commit:** _(pending — fill in via post-hoc `phase 05.4: progress note (task 1)` if SHA needed cross-task)_
- **Deliverables:** SPEC §3 D1.
- **ADR landed:** ADR-0024 (`Cluster.dns_lookup_family` field + `DnsLookupFamily` enum, parse-only).
- **Files modified:**
  - `docs/envoy-rust/DECISIONS.md` (ADR-0024 appended after ADR-0023).
  - `crates/envoy-config/src/bootstrap.rs` (`DnsLookupFamily` enum; `Cluster.dns_lookup_family` field; `parses_cluster_with_dns_lookup_family_v4_only` parse test).
  - `crates/envoy-config/src/lib.rs` (re-export `DnsLookupFamily`).
  - `crates/envoy-cluster/src/cluster.rs` (2 hand-written `Cluster` initialisers gain `dns_lookup_family: None`; planner-confirmed count).
- **LoC:** ~85 (5 field + 10 enum + 1 re-export + 25 parse test + 2 initialiser updates + 13 ADR + 30 PROGRESS narrative).
- **Verification:**
  - `cargo test -p envoy-config parses_cluster_with_dns_lookup_family_v4_only` — `test result: ok. 1 passed`.
  - `cargo test -p envoy-config` — `test result: ok. 146 passed; 0 failed` (existing tests unchanged; +1 new).
  - `cargo test -p envoy-cluster` — `test result: ok. 14 passed; 0 failed` (existing tests unchanged after the 2 initialiser updates).
  - `cargo clippy -p envoy-config -p envoy-cluster --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean (after a `cargo fmt --all` apply: the new `DnsLookupFamily` re-export pushed the line past width; `cargo fmt` re-flowed the `pub use bootstrap::{ ... }` block; semantically equivalent — no symbol added/dropped beyond the new re-export).
- **Deviations from PLAN:**
  - **Step 1 grep semantics:** the `^## ADR-` count returned `24`, not the projected `23`, because the count includes the `## ADR-NNNN: <title>` template marker at line 10 of DECISIONS.md (not a real ADR). The numeric ADR head before this commit was ADR-0023 as expected — no unexpected ADRs landed beyond ADR-0023. Proceeded.
  - **Step 1 `cluster.rs` initialiser grep:** the planner's `envoy_config::Cluster {\|^        Cluster {$` regex returned 0 matches (the actual lines use `clusters: vec![Cluster {` at line 426 and `let mk_cluster = || Cluster {` at line 456 — both inside `mod tests`, both bringing `Cluster` into scope via `use envoy_config::{… Cluster …}`). The plan's projected count of **2** is correct; only the regex was imprecise. Both confirmed schema-`Cluster` literals updated; the other 2 `Cluster {` matches in the file (lines 240, 503) build the local runtime `Cluster` struct in `cluster.rs` itself (different type — has `name`/`endpoints`/`cursor`, not `cluster_type`/`load_assignment`/etc.) and are correctly NOT touched.
  - **Step 3 test YAML path correction:** the plan's verbatim test snippet uses `bootstrap.clusters.len()` and `bootstrap.clusters[0]`, but the actual struct hierarchy is `Bootstrap.static_resources.clusters`. The path was corrected to `bootstrap.static_resources.clusters[…]` to match the existing analogous test (`validates_strict_dns_cluster_does_not_require_literal_ip_endpoints` at line 4483). Test intent (assert the new field parses) is preserved verbatim.
  - **Step 3 test YAML missing `admin:` block:** the plan's verbatim YAML omits an `admin:` block, which causes `parse_bootstrap` to fail with `ConfigError::NoRuntime` (validator rejects a bootstrap with no admin endpoint AND no listeners). An `admin:` block matching the existing analogous test was added to the YAML. The new field's parse-shape check (`Some(DnsLookupFamily::V4Only)`) is unchanged and still the test's load-bearing assertion.
  - **Step 10 fmt re-flow:** adding `DnsLookupFamily` to the alphabetic re-export list pushed `FilterChain` past the line width; `cargo fmt --all` re-flowed lines 12-14 of `lib.rs` (token-equivalent; no symbols added or dropped beyond the new re-export). Per PLAN Step 10 ("If fmt fails, run `cargo fmt --all` and re-stage"), this is the prescribed remediation.
- **Carryforward note:** None — Task 1 is mechanically scoped per SPEC §3 D1.
- **Fuzz seed:** Not added (per PLAN signpost H — optional, deferred to PLAN discretion; planner elected NOT to add).
