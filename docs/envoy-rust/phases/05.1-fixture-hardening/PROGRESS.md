# Phase 05.1 Progress

## Task 1 — envoy-config: ClusterType::StrictDns + ADR-0023 + 6 validator tests + fuzz seed (2026-05-02)

**Commit:** `bfabcb6`

**Change summary.** Lands ADR-0023 (`ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred) inline at this Task 1 commit per SPEC §7. Extends `crates/envoy-config/src/bootstrap.rs::ClusterType` from single-variant `Static` (lines 60-62 at HEAD `e626862`) to two-variant `Static | StrictDns`. Appends 6 unit tests to `bootstrap::tests` covering: (1) `STRICT_DNS` parse + variant-match; (2) `STATIC` parse-path regression-guard (unchanged); (3) `LOGICAL_DNS` rejection with `"unknown variant"` error documenting the ADR-0023 deferral; (4) unknown-tag rejection (`WEIRD_TYPE`); (5) multi-endpoint `STRICT_DNS` load-assignment shape; (6) `STRICT_DNS` cluster with a DNS-name endpoint passes the validator stage cleanly. Adds 1 new fuzz corpus seed (`crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml`; full bootstrap with one `STRICT_DNS` cluster whose endpoint resolves to `localhost`); appends one `.gitignore` allow-list entry. Total: ~145 LoC (15 schema + 80 unit tests + 25 fuzz seed YAML + 1 .gitignore + 25 ADR).

**Verification tail.**

```
$ cargo test --package envoy-config 2>&1 | tail -3
test result: ok. 145 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=15 2>&1 | tail -3
###### End of recommended dictionary. ######
Done 310910 runs in 16 second(s)
```

Test count delta: 139 → 145 (6 new tests).

**Deviations from PLAN.** Signpost A applied at Task 1 — ADR-0023's prose in DECISIONS.md uses `ClusterError::DnsResolutionFailed` (NOT `ConfigError::ClusterDnsResolutionFailed` as projected in SPEC §3 D1) per planner-time refinement. Implementation lands the variant on envoy-cluster's `ClusterError` at Task 2, not on envoy-config's `ConfigError` (which is unchanged in 05.1). Reasoning: SPEC §3 D2 pseudocode mixed both error types in the same `?` chain, which is mechanically inconsistent; SPEC §6 signpost 14 preserves envoy-cluster's typed-error chain unchanged; the simpler placement is on `ClusterError` where the DNS resolution code lives. ADR-0023's diagnostic shape (`{cluster, address, source: std::io::Error}`) is identical regardless of which enum carries the variant. PROGRESS.md Task 2 notes the variant landing location.

### Task 1 review fix — extend `fuzz_corpus_seeds_parse_or_reject_cleanly` walk-list

- Commit: `7391a4e`
- Important fix I1: appended `"fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml"` to the parse-success slice in `crates/envoy-config/src/bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` (after `route_with_header_matchers.yaml`, end-of-slice positioning matching the feature-grouped order — tcp_proxy → tls → hcm → route → cluster). Per phase-04.2 (`route_with_header_matchers.yaml`) and phase-04.3 (`hcm_route_to_cluster.yaml`) precedent: that test enumerates seeds via an explicit `&[..]` slice (not `read_dir`), so each new corpus seed must be hand-registered or the in-tree `cargo test --package envoy-config` gate silently skips it. Closes review I1 (planner-time omission — Task 1 PLAN.md Steps 8–10 covered the seed file + `.gitignore` + cargo-fuzz run, but not the in-tree walk-list entry).
- Verification: `cargo test --package envoy-config --lib -- fuzz_corpus_seeds` -> 1 passed (test now walks 9 success seeds + 3 reject seeds + minimal.yaml); `cargo test --package envoy-config` -> 145 passed (count unchanged — the test counts as one assertion regardless of slice length); `cargo build --workspace --all-targets` -> clean; `cargo clippy --workspace --all-targets --all-features -- -D warnings` -> clean; `cargo fmt --all -- --check` -> clean.
- ADRs: none.

## Task 2 — envoy-cluster: tokio dep + async from_bootstrap + ClusterError::DnsResolutionFailed + STRICT_DNS branch + 3 new tests + I3 close-out (2026-05-02)

**Commit:** `f7a555d`

**Change summary.** Promotes `crates/envoy-cluster/src/cluster.rs::from_bootstrap` from `pub fn` to `pub async fn`; adds a `STRICT_DNS` resolution branch via `tokio::net::lookup_host` (resolves once at cluster-build time per ADR-0023; results cached for cluster lifetime). Extends `ClusterError` with one new variant `DnsResolutionFailed { cluster: String, address: String, source: std::io::Error }`. Adds `tokio = { version = "1", features = ["net", "rt", "macros"] }` to envoy-cluster's `[dependencies]` (was previously absent — `tokio` is already a top-level dep on other workspace crates so no new transitive license surfaces). Adds `tokio = { version = "1", features = ["macros", "rt"] }` to `[dev-dependencies]` for `#[tokio::test]` flavor. Appends 3 new unit tests (`static_cluster_constructs_with_literal_ip` — closes phase-02.1 REVIEW I3; `strict_dns_cluster_resolves_localhost_at_build_time`; `strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain`). Updates 5 existing tests (`from_bootstrap_builds_single_endpoint_cluster`, `from_bootstrap_builds_three_endpoint_cluster`, `from_bootstrap_rejects_empty_cluster`, `from_bootstrap_rejects_duplicate_cluster_name`, `from_bootstrap_rejects_malformed_endpoint_address`) to `#[tokio::test] async fn` + `.await` on the `from_bootstrap` call (mechanical; ~5 LoC churn). Updates the single envoy-bin call site at `crates/envoy-bin/src/main.rs:83` with one `.await` token. Total: ~120 LoC core + ~25 LoC of forced cross-crate test-helper churn (see Deviations below).

**Phase-02.1 REVIEW I3 closes** at this commit. The carryforward chain (phase-02.1 → 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 → 05.1) ends here. The new positive `Static` regression guard `static_cluster_constructs_with_literal_ip` is structurally meaningful only because Task 1 added the second `ClusterType` variant.

**Verification tail.**

```
$ cargo test --package envoy-cluster --lib 2>&1 | tail -3
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

$ cargo build --workspace --all-targets 2>&1 | tail -3
[clean — Finished `dev` profile]

$ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
[clean — Finished `dev` profile]

$ cargo fmt --all -- --check
[clean — exit 0]
```

`cargo test --workspace` shows one pre-existing failure: `differential::http1_router_upstream_fixture`. The container fails to start with `malformed IP address: host.docker.internal. Consider setting resolver_name or setting cluster type to 'STRICT_DNS' or 'LOGICAL_DNS'`. Verified pre-existing on main `f3d19a6` via stash-test — this is exactly the bug Phase 05.1 is meant to fix; Task 3 (next) flips the fixture YAML files to `STRICT_DNS`. Excluding `differential`, all other workspace tests pass clean (`cargo test --workspace --exclude differential`).

Test count delta in envoy-cluster: 11 → 14 (+3 new tests). Workspace test count unchanged otherwise.

**Deviations from PLAN.** Signpost A applied (variant lands on `ClusterError::DnsResolutionFailed`, not `ConfigError::ClusterDnsResolutionFailed`). Signpost B applied (`tokio` is a NEW direct dep on envoy-cluster; was absent at HEAD `e626862`). Signpost E NOT triggered: `this-host-does-not-exist.invalid` resolved as expected (returned NotFound from `tokio::net::lookup_host`); the `address: ` (empty/space) escape hatch was not needed on this machine.

**Forced cross-crate test-helper churn (NOT in PLAN.md):** Promoting `from_bootstrap` to `async fn` forces every existing caller to `.await`. PLAN.md listed only the 5 callers in `crates/envoy-cluster/src/cluster.rs::tests` and the 1 caller in `crates/envoy-bin/src/main.rs:83`, but the workspace also contains 3 additional test-helper callers that PLAN.md did not enumerate:

- `crates/envoy-tcp/src/lib.rs::tests::mk_handle` — promoted from `fn` to `async fn`; 11 call-site `.await` updates (all inside `#[tokio::test(flavor = "multi_thread")]` async tests).
- `crates/envoy-http1/src/hcm.rs::tests::cluster_mgr_with_endpoint` — promoted from `fn` to `async fn`; 5 call-site `.await` updates.
- `crates/envoy-http1/src/hcm.rs::tests::cluster_mgr_empty` — promoted from `fn` to `async fn`; 6 call-site `.await` updates (5 direct + 1 inside helpers `hcm_config_single_route` and `build_test_config`, themselves promoted to `async fn` with their own callers updated).

Without these the workspace will not compile (`E0728: 'await' is only allowed inside async functions`). All updates are mechanical — `fn` → `async fn`, append `.await` at each call site, no behavioural changes. Files touched beyond PLAN.md's enumeration: `crates/envoy-tcp/src/lib.rs`, `crates/envoy-http1/src/hcm.rs`. `Cargo.lock` updated as expected by adding `tokio` to envoy-cluster (dep graph is otherwise unchanged — `tokio` was already pulled by envoy-tls/envoy-tcp/envoy-http1/envoy-listener/envoy-bin).
