# Phase 05.1 Progress

## Task 1 — envoy-config: ClusterType::StrictDns + ADR-0023 + 6 validator tests + fuzz seed (2026-05-02)

**Commit:** `7a2cd6f`

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
