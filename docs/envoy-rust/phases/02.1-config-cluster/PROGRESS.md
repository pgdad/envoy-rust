# Phase 02.1 Progress

## Task 1 — ADR-0014 (2026-04-24)

- Commit: 6d1f8d6
- Change: appended ADR-0014 (YAML-native `typed_config` deserialization until the xDS/protos family lands) to DECISIONS.md. Renumbered from parent-SPEC ADR-0013 per the ADR-0013 phase-02 split decision.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 14 (ADR-0001 through ADR-0014).

## Task 2 — typed_config envelope (2026-04-24)

- Commit: ebaa712
- Change: extended `NetworkFilter` with `typed_config: Option<TypedConfig>`; introduced `TypedConfig` tagged-enum envelope (single `TcpProxy(TcpProxyConfig)` variant per ADR-0014) and `TcpProxyConfig { stat_prefix, cluster }`. Added 3 shape tests.
- Verification: `cargo test -p envoy-config` → `test result: ok. 24 passed; 0 failed` (21 phase-01 + 3 new).
- Deviation from plan: the test YAML in `parses_bootstrap_with_tcp_proxy_filter` (PLAN.md Step 1) as-written references cluster fields (`type`, `lb_policy`, `load_assignment`) that only land in Task 3; with phase-01's `Cluster { name: String }` under `deny_unknown_fields`, serde rejected the YAML before the test's filter assertions could run. Simplified the cluster block to `clusters: [{ name: backend }]` — preserves the TCP-proxy → cluster name-reference scene-setting and matches Task 2's parse-layer semantics. Task 3's `parses_bootstrap_with_single_endpoint_cluster` exercises the full cluster shape. Drift logged per D-3.5 (plan drift, not spec drift; no ADR needed).
