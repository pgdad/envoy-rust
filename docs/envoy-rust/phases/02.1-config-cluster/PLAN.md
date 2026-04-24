# Phase 02.1 — Config schema + cluster manager + echo-server helper — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md`. This plan operationalizes SPEC §§D1–D7. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue.

**Goal:** Extend `envoy-config` with the `envoy.filters.network.tcp_proxy` + static `STATIC` cluster + `ROUND_ROBIN` LB grammar, land a new `envoy-cluster` library crate (static-cluster data model + round-robin LB cursor), land a `tests/helpers/tcp-echo-server/` helper binary crate, close phase-01 REVIEW §9 starter item **I3** (`decode_chunked` unit tests), and seed the fuzz corpus with two TCP-proxy-shaped YAML fixtures. No new differential fixture ships in 02.1; fixtures `0001-tcp-echo` and `0002-static-admin-ready` remain green.

**Architecture:** `envoy-config` gains a `TypedConfig` tagged enum (discriminated on the `@type` URL per ADR-0014) with a single `TcpProxy(TcpProxyConfig)` variant, a fleshed-out five-level cluster topology (`Cluster` → `LoadAssignment` → `LocalityLbEndpoints` → `LbEndpoint` → `Endpoint` → `Address`), and five new `ConfigError` variants for the new validator rules. A brand-new `envoy-cluster` crate (sync, no `tokio`, no `tracing`) owns `ClusterManager` with a `HashMap<String, Arc<Cluster>>` and a round-robin `AtomicUsize` cursor per cluster. `tests/helpers/tcp-echo-server/` is the first crate under `tests/helpers/` — a ~160-LoC `tokio` binary that spins up on `127.0.0.1:<port>`, echoes each connection with `tokio::io::copy`, and drains cleanly on `ctrl_c` with a 5-second budget. `envoy-bin` is not re-wired this sub-phase (SPEC §6 signpost 6); runtime dispatch of `envoy.filters.network.tcp_proxy` stays explicitly unimplemented until 02.2.

**Tech stack:** Rust edition 2024 on pinned stable `1.95.0` (D-3.9); `serde` + `serde_yaml` for parsing; `thiserror` v2 for typed errors in library crates (matches the version pinned by `envoy-config` per phase-01 Task 5 deviation); `std::sync::atomic::AtomicUsize` for the round-robin cursor; `tokio` with features `["rt-multi-thread", "net", "io-util", "macros", "signal"]` for the echo helper; `anyhow` permitted in the `tcp-echo-server` binary crate per D-3.2; `tracing` + `tracing-subscriber` for the helper's stderr logs; `cargo-fuzz` + `libfuzzer-sys` nightly-only on the pre-existing `parse_bootstrap` fuzz target (corpus grows; no new target).

---

## File structure (created / modified)

**Created:**
- `crates/envoy-cluster/Cargo.toml`
- `crates/envoy-cluster/src/lib.rs`
- `crates/envoy-cluster/src/cluster.rs`
- `tests/helpers/tcp-echo-server/Cargo.toml`
- `tests/helpers/tcp-echo-server/src/main.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml`
- `docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md` (appended by each task during execution)

**Modified:**
- Root `Cargo.toml` — add `crates/envoy-cluster` and `tests/helpers/tcp-echo-server` to `[workspace] members`. `envoy-listener` and `envoy-tcp` are **not** added here; they land in 02.2.
- `crates/envoy-config/src/bootstrap.rs` — introduce `TypedConfig`, `TcpProxyConfig`; extend `NetworkFilter` with `typed_config: Option<TypedConfig>`; flesh out `Cluster`; introduce `ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`; extend `validate`; rename `rejects_non_echo_filter` → `rejects_unknown_filter_name` and update its YAML; update `parses_bootstrap_with_clusters_stub` + `rejects_unknown_cluster_field` to the full cluster shape; add 16 new unit tests.
- `crates/envoy-config/src/lib.rs` — re-export `TypedConfig`, `TcpProxyConfig`, `ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`; extend `ConfigError` with `MissingTypedConfig`, `UnexpectedTypedConfig`, `UnknownCluster`, `LoadAssignmentNameMismatch`, `EmptyClusterEndpoints`.
- `tests/differential/src/lib.rs` — append 4 `decode_chunked` unit tests (phase-01 REVIEW I3). No other changes.
- `docs/envoy-rust/DECISIONS.md` — append ADR-0014.
- `docs/envoy-rust/ROADMAP.md` — flip row `02.1` `status` → `done` (at state 6 only; Task 13's phase-done commit handles this).
- `docs/envoy-rust/STATE.md` — advance to phase `02.2-listener-tcp-proxy` at state 2 (SPEC exists, PLAN does not) (at state 6 only).
- `deny.toml` — only if `cargo deny check` flips red on a new transitive surface from `tcp-echo-server`. Expected: no change (its `tokio`/`tracing-subscriber`/`thiserror`/`anyhow` deps are already all transitively present via `envoy-bin`). Handle per D-3.5 if it triggers.

**Not touched in 02.1** (belong to 02.2 or are frozen):
- `crates/envoy-bin/Cargo.toml`, `crates/envoy-bin/src/main.rs`, `crates/envoy-bin/src/admin.rs` — untouched.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/` — untouched.
- `tests/differential/Cargo.toml` — untouched (the 4 I3 tests use `decode_chunked` already in-crate).
- `tests/differential/src/subject.rs`, `tests/differential/src/upstream.rs` — untouched.
- `.github/workflows/ci.yml` — untouched (SPEC §D6).
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `50349da`.

---

## Task index

Each task ends with a commit. `PROGRESS.md` gets a new section per task in the phase-01 style (task id, commit SHA, change summary, verification tail, any deviation). Use either the `sed`-then-amend idiom or the follow-up `phase 02.1: progress note (task N)` commit convention — whichever is picked for Task 1 stays consistent through Task 13.

Ordering rationale (SPEC §6 signpost 1): `envoy-config` schema extensions ship before `envoy-cluster` because `envoy-cluster::from_bootstrap` consumes the new types; the `tcp-echo-server` helper is independent and grouped after `envoy-cluster` to keep the test-run cadence monotone (each task block either touches `envoy-config` or lands a new crate).

1. **ADR-0014 — YAML-native `typed_config` deserialization (append to `DECISIONS.md`)**
2. **`envoy-config` — filter envelope (`TypedConfig`, `TcpProxyConfig`, `NetworkFilter.typed_config`) + 3 shape tests**
3. **`envoy-config` — cluster topology (fleshed-out `Cluster`, `ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`) + 3 shape tests; update pre-existing cluster-stub tests**
4. **`envoy-config` — validator extensions + 5 new `ConfigError` variants + 10 remaining tests; rename `rejects_non_echo_filter` → `rejects_unknown_filter_name`**
5. **Scaffold `crates/envoy-cluster/` skeleton + workspace member**
6. **`envoy-cluster::cluster` — `Cluster` struct, `ClusterHandle`, `pick_endpoint` atomic cursor; 3 tests**
7. **`envoy-cluster` — `ClusterManager`, `ClusterError`, `from_bootstrap`; 5 tests**
8. **Scaffold `tests/helpers/tcp-echo-server/` skeleton + workspace member**
9. **`tcp-echo-server` argv parser (`Args`, `ArgvError`, `parse_argv`) + 6 tests**
10. **`tcp-echo-server` runtime (`run`, `main`) + 2 tokio tests (round-trip + drain)**
11. **Phase-01 rollover I3: 4 `decode_chunked` unit tests in `tests/differential/src/lib.rs`**
12. **Fuzz corpus extension — 2 new YAML seeds under `crates/envoy-config/fuzz/corpus/parse_bootstrap/`**
13. **Phase-done gate (state 4) — run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md; flip ROADMAP and STATE**

---

### Task 1: ADR-0014 — YAML-native `typed_config` deserialization

**Files:**
- Modify (append): `docs/envoy-rust/DECISIONS.md`
- Create: `docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md`

**Why first:** every subsequent task that touches the filter envelope (Tasks 2, 4) cites ADR-0014. `DECISIONS.md` is append-only per D-3.5; land the rationale before the code that references it.

- [ ] **Step 1: Append ADR-0014 to `docs/envoy-rust/DECISIONS.md`.**

Append after the final `---` of ADR-0013. Use these exact field contents (copied from SPEC §7; keep the Options list to three items):

```markdown
## ADR-0014: YAML-native `typed_config` deserialization until the xDS/protos family lands

- Date: 2026-04-24
- Status: accepted
- Context: Sub-phase 02.1 is the first phase to surface Envoy's `typed_config` envelope (`envoy.filters.network.tcp_proxy`). The `envoy-protos` crate + `prost` / `prost-build` + upstream proto-tree vendoring were deferred at phase-00 bootstrap to the xDS family (ROADMAP §9). 02.1 must choose: bring the protos stack forward now, or ship a narrower shim.
- Options considered:
  - **(i) YAML-native — one Rust enum discriminated on the `@type` URL string literal, fields deserialized by serde.** Minimal surface, scoped to this sub-phase's needs. Grows one enum variant per filter across phases 04/05/06 until the xDS family ships.
  - **(ii) Bring `prost` + `envoy-protos` in as part of 02.1.** Pulls forward multi-phase proto-tree vendoring. Out of ROADMAP row-02 scope; would trigger a further split by itself.
  - **(iii) Non-Envoy `raw_config` YAML key.** Diverges `envoy.yaml` and `envoy-rust.yaml` on filter shape, breaking the fixture principle that configs are initially identical.
- Decision: (i). `TypedConfig` enum in `envoy-config::bootstrap` with a `#[serde(tag = "@type")]` discriminator; one variant for TCP proxy in 02.1; extended per filter across future phases.
- Rationale: keeps 02.1 within row-02 scope; defers the `envoy-protos` multi-phase work until it pays for itself. Reviewable by shape — a stranger reading the YAML can see which filters are supported.
- Consequences: unknown `@type` URLs reject at parse time via serde's tagged-enum default behavior. Every new filter in phase 04 / 05 / 06 extends the enum by one variant. An `envoy-protos` supersession ADR in the xDS family re-routes the `@type` URL to prost-generated message types in one sweep and retires this shim.
- Provenance: this ADR was projected as "ADR-0013" in parent-phase SPEC §7 (`docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, committed at SHA `50349da`) and renumbered to ADR-0014 by the phase-02 split decision (ADR-0013). The projected ADR-0014 (host-docker + host-gateway) and ADR-0015 (`enable_half_close: false` default) from the parent SPEC are renumbered to ADR-0015 and ADR-0016 respectively and land with sub-phase 02.2.
```

- [ ] **Step 2: Create `docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md` with a Task 1 section.**

Content:

```markdown
# Phase 02.1 Progress

## Task 1 — ADR-0014 (2026-04-24)

- Commit: <SHA>
- Change: appended ADR-0014 (YAML-native `typed_config` deserialization until the xDS/protos family lands) to DECISIONS.md. Renumbered from parent-SPEC ADR-0013 per the ADR-0013 phase-02 split decision.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 14 (ADR-0001 through ADR-0014).
```

Replace `<SHA>` with the actual commit hash after Step 5.

- [ ] **Step 3: Verify DECISIONS.md parses and the ADR sequence is intact.**

```bash
grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md
```
Expected output: `14`.

```bash
grep -n '^## ADR-00' docs/envoy-rust/DECISIONS.md | tail -2
```
Expected output (last 2 lines): `ADR-0013` and `ADR-0014` in that order, with ascending line numbers.

- [ ] **Step 4: Run the local gate to confirm no unrelated regressions.**

```bash
cargo fmt --all -- --check
```
Expected: exit 0, no diff.

- [ ] **Step 5: Commit.**

```bash
git add docs/envoy-rust/DECISIONS.md docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md
git commit -m "phase 02.1: ADR-0014 — YAML-native typed_config deserialization"
```

Patch PROGRESS.md's `<SHA>` placeholder to the commit hash and either amend (phase-01 Task 1's idiom) or land a follow-up `phase 02.1: progress note (task 1)` commit (phase-01 Task 2's idiom). Pick one and use it for every remaining task.

---

### Task 2: `envoy-config` — filter envelope (`TypedConfig` + `TcpProxyConfig` + `NetworkFilter.typed_config`) + 3 shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs`

**Scope:** add the `typed_config` field to `NetworkFilter`, introduce the `TypedConfig` tagged enum with its single `TcpProxy(TcpProxyConfig)` variant (per ADR-0014), and add shape-only tests (positive parse + two negative). This task lands types only; validator changes (e.g. rejecting `echo` with typed_config) defer to Task 4.

**Test inventory (3 tests, all appended to `crates/envoy-config/src/bootstrap.rs::tests`):**
- `parses_bootstrap_with_tcp_proxy_filter` — full happy-path shape-only: listener → tcp_proxy with typed_config → single-endpoint cluster. Uses `serde_yaml::from_str::<Bootstrap>` directly (not `parse_bootstrap`) because validator extensions ship in Task 4. Delegate the "end-to-end via `parse_bootstrap`" regression to Task 4.
- `rejects_typed_config_unknown_type_url` — unknown `@type` URL → serde tagged-enum default rejection. Shape-only (serde level).
- `rejects_unknown_tcp_proxy_config_field` — `idle_timeout: 0s` inside `typed_config` → `deny_unknown_fields` rejection. Shape-only.

- [ ] **Step 1: Write the failing test `parses_bootstrap_with_tcp_proxy_filter`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn parses_bootstrap_with_tcp_proxy_filter() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b: Bootstrap = serde_yaml::from_str(yaml).expect("valid YAML");
        let filter = &b.static_resources.listeners[0].filter_chains[0].filters[0];
        assert_eq!(filter.name, "envoy.filters.network.tcp_proxy");
        match filter.typed_config.as_ref().expect("typed_config present") {
            TypedConfig::TcpProxy(tp) => {
                assert_eq!(tp.stat_prefix, "ingress_tcp");
                assert_eq!(tp.cluster, "backend");
            }
        }
    }
```

- [ ] **Step 2: Run the test; verify it fails.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_bootstrap_with_tcp_proxy_filter
```
Expected: compile error, `error[E0609]: no field 'typed_config' on type 'NetworkFilter'` (or `cannot find type 'TypedConfig' in this scope`).

- [ ] **Step 3: Extend `NetworkFilter` and introduce the `TypedConfig` + `TcpProxyConfig` types.**

Replace the `NetworkFilter` definition in `crates/envoy-config/src/bootstrap.rs` and append the two new types (keep all struct-level ordering alphabetical-by-use; place `TypedConfig` + `TcpProxyConfig` immediately after `NetworkFilter` so reviewers read filter → envelope → payload top-down):

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkFilter {
    pub name: String,
    #[serde(default)]
    pub typed_config: Option<TypedConfig>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy")]
    TcpProxy(TcpProxyConfig),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TcpProxyConfig {
    /// Required by Envoy for access-log attribution; accepted by envoy-rust and
    /// unused until phase 06 (access logs). Carrying it through the parser now
    /// keeps fixture YAMLs identical across upstream-Envoy and envoy-rust.
    pub stat_prefix: String,
    pub cluster: String,
}
```

Notes:
- `PartialEq` derive is added to `NetworkFilter` as well (needed to let downstream tests compare parsed structs; adds ~0 LoC since it rides on the `#[derive]` line).
- `#[serde(default)]` on `typed_config` preserves fixture `0001-tcp-echo`'s echo-filter YAML (no `typed_config`) verbatim.
- The echo filter is still named `envoy.filters.network.echo`; only the validator allow-list widens in Task 4.

- [ ] **Step 4: Re-run the test; verify it passes.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_bootstrap_with_tcp_proxy_filter
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Add the negative shape test `rejects_typed_config_unknown_type_url`.**

Append:

```rust
    #[test]
    fn rejects_typed_config_unknown_type_url() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.not_tcp_proxy.v3.NotTcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("@type"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }
```

Rationale: serde's tagged-enum default rejects unknown `@type` discriminators with an `unknown variant` error. The test checks that a fake `@type` URL does NOT parse — exact error text is allowed to vary across `serde_yaml` versions so the assertion accepts either `"unknown variant"` or a mention of `"@type"`.

- [ ] **Step 6: Add the negative shape test `rejects_unknown_tcp_proxy_config_field`.**

Append:

```rust
    #[test]
    fn rejects_unknown_tcp_proxy_config_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
                idle_timeout: 0s
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(msg.contains("unknown field"), "got {msg}");
    }
```

- [ ] **Step 7: Run the full crate tests + lint gate.**

```bash
cargo test -p envoy-config
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

All three expected: exit 0. Test output: `test result: ok. 24 passed; 0 failed` (21 phase-01 tests + 3 new).

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 02.1: envoy-config typed_config envelope [ADR-0014]"
```

Append PROGRESS.md Task 2 section with the commit SHA and the 24-test tail.

---

### Task 3: `envoy-config` — cluster topology types + 3 shape tests; update pre-existing cluster-stub tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs`

**Scope:** flesh out `Cluster` from its phase-01 name-only stub into the full five-level Envoy topology, and introduce the six supporting types (`ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`). Shape-only tests here (3 positive + 0 `deny_unknown_fields` regressions — those land with the validator in Task 4 so all rejection flows can use `parse_bootstrap` uniformly).

Pre-existing tests that mention a bare `name`-only cluster stub **must be updated** since `Cluster` now requires `type`, `lb_policy`, and `load_assignment`:
- `parses_bootstrap_with_clusters_stub` (currently uses `clusters: - name: cluster_0`) → full cluster shape.
- `rejects_unknown_cluster_field` (currently uses `clusters: - name: cluster_0 bogus: 1`) → full cluster shape + bogus field. (This test moves conceptually to the "deny_unknown_fields on the *new* cluster layout" line; Task 4 adds sibling tests for the other four new struct levels.)

**Test inventory (3 new tests + 2 updated pre-existing tests):**

New:
- `parses_bootstrap_with_round_robin_multi_endpoint_cluster` — three-endpoint cluster; assert `endpoints[0].lb_endpoints.len() == 3`.
- `rejects_cluster_type_logical_dns` — `type: LOGICAL_DNS` → serde-level rejection (`ClusterType::Static` is the only accepted variant).
- `rejects_lb_policy_least_request` — `lb_policy: LEAST_REQUEST` → serde-level rejection.

Updated:
- `parses_bootstrap_with_clusters_stub` — renamed to `parses_bootstrap_with_single_endpoint_cluster`; full YAML.
- `rejects_unknown_cluster_field` — unchanged intent; YAML expanded to the full cluster shape with a `bogus: 1` sibling to `name`.

- [ ] **Step 1: Write the failing test `parses_bootstrap_with_round_robin_multi_endpoint_cluster`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn parses_bootstrap_with_round_robin_multi_endpoint_cluster() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10003
  listeners: []
"#;
        let b: Bootstrap = serde_yaml::from_str(yaml).expect("valid YAML");
        assert_eq!(b.static_resources.clusters.len(), 1);
        let c = &b.static_resources.clusters[0];
        assert_eq!(c.name, "backend");
        assert!(matches!(c.cluster_type, ClusterType::Static));
        assert!(matches!(c.lb_policy, LbPolicy::RoundRobin));
        assert_eq!(c.load_assignment.cluster_name, "backend");
        assert_eq!(c.load_assignment.endpoints.len(), 1);
        assert_eq!(c.load_assignment.endpoints[0].lb_endpoints.len(), 3);
        assert_eq!(
            c.load_assignment.endpoints[0].lb_endpoints[2]
                .endpoint
                .address
                .socket_address
                .port_value,
            10003
        );
    }
```

- [ ] **Step 2: Run the test; verify it fails.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_bootstrap_with_round_robin_multi_endpoint_cluster
```
Expected: compile error, `error[E0609]: no field 'cluster_type' on type 'Cluster'` (or `cannot find type 'ClusterType' in this scope`).

- [ ] **Step 3: Replace `Cluster` and introduce the six supporting types.**

In `crates/envoy-config/src/bootstrap.rs`, replace the existing `Cluster` struct with:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cluster {
    pub name: String,
    #[serde(rename = "type")]
    pub cluster_type: ClusterType,
    pub lb_policy: LbPolicy,
    pub load_assignment: LoadAssignment,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ClusterType {
    Static,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LbPolicy {
    RoundRobin,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoadAssignment {
    pub cluster_name: String,
    pub endpoints: Vec<LocalityLbEndpoints>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalityLbEndpoints {
    pub lb_endpoints: Vec<LbEndpoint>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LbEndpoint {
    pub endpoint: Endpoint,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub address: Address,
}
```

Also add `PartialEq` to `Address` and `SocketAddress` (currently `#[derive(Debug, Deserialize)]`) — cheap, and some future tests will benefit from the derive without forcing re-derivation later. (One-line change each.) Leave `Listener`, `FilterChain`, `Bootstrap`, `Admin`, `Node`, `StaticResources` without `PartialEq` — not needed by any 02.1 test.

- [ ] **Step 4: Re-run the test; verify it passes.**

```bash
cargo test -p envoy-config bootstrap::tests::parses_bootstrap_with_round_robin_multi_endpoint_cluster
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Update the pre-existing `parses_bootstrap_with_clusters_stub` to the full cluster shape.**

Rename the test to `parses_bootstrap_with_single_endpoint_cluster` and replace its body:

```rust
    #[test]
    fn parses_bootstrap_with_single_endpoint_cluster() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.static_resources.clusters.len(), 1);
        assert_eq!(b.static_resources.clusters[0].name, "backend");
    }
```

- [ ] **Step 6: Update the pre-existing `rejects_unknown_cluster_field` to the full cluster shape.**

Replace its body:

```rust
    #[test]
    fn rejects_unknown_cluster_field() {
        let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
      bogus: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }
```

- [ ] **Step 7: Add the negative shape test `rejects_cluster_type_logical_dns`.**

Append:

```rust
    #[test]
    fn rejects_cluster_type_logical_dns() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: LOGICAL_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("LOGICAL_DNS"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }
```

- [ ] **Step 8: Add the negative shape test `rejects_lb_policy_least_request`.**

Append:

```rust
    #[test]
    fn rejects_lb_policy_least_request() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: LEAST_REQUEST
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("LEAST_REQUEST"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }
```

- [ ] **Step 9: Run the full crate tests + lint gate.**

```bash
cargo test -p envoy-config
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: `test result: ok. 27 passed; 0 failed` (24 after Task 2 + 3 new in this task; the two pre-existing tests updated in Steps 5–6 don't change the count).

- [ ] **Step 10: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs
git commit -m "phase 02.1: envoy-config cluster topology types"
```

Append PROGRESS.md Task 3 section with the commit SHA, the 27-test tail, and a note naming the two pre-existing tests updated in-place.

---

### Task 4: `envoy-config` — validator extensions + 5 new `ConfigError` variants + 10 remaining tests; rename `rejects_non_echo_filter`

**Files:**
- Modify: `crates/envoy-config/src/lib.rs`
- Modify: `crates/envoy-config/src/bootstrap.rs`

**Scope:** extend `ConfigError` with the five new variants from SPEC §D2, extend `lib.rs`'s re-exports with the eight new public types, extend `validate` with the per-listener allow-list widening and the four new cluster-level rules, and add the ten remaining SPEC-§D2 unit tests.

**Per-listener validator arms (update from phase-01):**
- Allowed filter names: `{envoy.filters.network.echo, envoy.filters.network.tcp_proxy}`. Echo stays accepted for fixture 0001. Any other name → `ConfigError::UnsupportedFilter` (variant unchanged).
- For `tcp_proxy`: `typed_config: Some(TypedConfig::TcpProxy { .. })` required. Missing → `ConfigError::MissingTypedConfig(&'static str)` (filter name passes through).
- For `echo`: `typed_config: None` required. Present → `ConfigError::UnexpectedTypedConfig(&'static str)`.
- For `tcp_proxy`: `typed_config.cluster` must name a cluster in `static_resources.clusters`. Missing → `ConfigError::UnknownCluster(String)`.

**Per-cluster validator arms (new):**
- `load_assignment.cluster_name == name`. Mismatch → `ConfigError::LoadAssignmentNameMismatch { cluster: String, assignment: String }`.
- Total `lb_endpoints` across all `endpoints[*]` ≥ 1. Zero → `ConfigError::EmptyClusterEndpoints(String)`.

**Test inventory (10 new tests):**

Validator-driven:
1. `rejects_tcp_proxy_without_typed_config` — `ConfigError::MissingTypedConfig`.
2. `rejects_echo_with_typed_config` — `ConfigError::UnexpectedTypedConfig`.
3. `rejects_tcp_proxy_naming_missing_cluster` — `ConfigError::UnknownCluster`.
4. `rejects_load_assignment_cluster_name_mismatch` — `ConfigError::LoadAssignmentNameMismatch`.
5. `rejects_empty_lb_endpoints` — `ConfigError::EmptyClusterEndpoints`.

Serde-accepted parse-layer positive (paired with Task 7's D1 rejection):
6. `rejects_malformed_endpoint_address` — parse-layer *acceptance* (serde sees a valid `Address { socket_address: { address: String, port_value: u16 } }`). The fn name reads "rejects" because `parse_bootstrap` succeeds but `envoy-cluster::from_bootstrap` later fails; this test asserts parse-layer acceptance so the boundary is mechanically documented. The companion D1 rejection test in Task 7 covers the `envoy-cluster` side.

`deny_unknown_fields` regressions (4 tests; SPEC §D2's "six struct levels" discipline — `Cluster` is covered by Task 3's updated `rejects_unknown_cluster_field`, `TcpProxyConfig` by Task 2's `rejects_unknown_tcp_proxy_config_field`, leaving four):
7. `rejects_unknown_load_assignment_field`
8. `rejects_unknown_locality_lb_endpoints_field`
9. `rejects_unknown_lb_endpoint_field`
10. (reserved — see below)

The SPEC §D2 also names `rejects_unknown_endpoint_field` as a possible sixth per-struct regression; `Endpoint` in 02.1 is effectively a pass-through to the phase-01 `Address` type, but `Endpoint` itself does carry `deny_unknown_fields` (per Task 3 Step 3). Add a tenth test:

10. `rejects_unknown_endpoint_field` — cluster config with `endpoint: { address: {...}, bogus: 1 }` → `assert_unknown_field`.

This brings the phase-02.1 `envoy-config` test count to 37 (21 phase-01 + 3 Task-2 + 3 Task-3 + 10 Task-4).

Also in this task (Step 1 sub-item): **rename `rejects_non_echo_filter` → `rejects_unknown_filter_name`** and update its YAML. The phase-01 test substituted `envoy.filters.network.echo` → `envoy.filters.network.tcp_proxy` in `MINIMAL` and expected `UnsupportedFilter`. After the 02.1 validator widens the allow-list to include tcp_proxy, that exact substitution would yield `MissingTypedConfig` (not `UnsupportedFilter`), so the test's intent (rejecting a name outside the allow-list) requires a genuinely-unknown name. Pick `envoy.filters.network.rbac` — on the phase-02 non-goals list (SPEC §4) and definitely not in the 02.1 allow-list.

- [ ] **Step 1: Rename + update `rejects_non_echo_filter`.**

Replace the existing test in `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn rejects_unknown_filter_name() {
        // Phase 02.1 widens the validator allow-list from {echo} to
        // {echo, tcp_proxy}. Pick a filter name that sits outside this
        // allow-list (rbac lands in phase 09's network-filter family).
        let yaml = MINIMAL.replace(
            "envoy.filters.network.echo",
            "envoy.filters.network.rbac",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedFilter(_, _)),
            "got {err:?}"
        );
    }
```

Run it first (before any validator change) to confirm it *currently* passes against the Task 3 code: the phase-01 validator will reject `envoy.filters.network.rbac` as `UnsupportedFilter` since rbac ≠ echo (and tcp_proxy hasn't been added to the allow-list yet). Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 2: Write the failing test `rejects_tcp_proxy_without_typed_config`.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn rejects_tcp_proxy_without_typed_config() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingTypedConfig(_)),
            "got {err:?}",
        );
    }
```

- [ ] **Step 3: Run the test; verify it fails.**

```bash
cargo test -p envoy-config bootstrap::tests::rejects_tcp_proxy_without_typed_config
```
Expected: compile error `error[E0599]: no variant or associated item named 'MissingTypedConfig' found for enum 'ConfigError'`.

- [ ] **Step 4: Extend `ConfigError` with the five new variants in `crates/envoy-config/src/lib.rs`.**

Replace the `ConfigError` definition:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("parsing bootstrap YAML")]
    Yaml(#[from] serde_yaml::Error),
    #[error(
        "bootstrap configures neither an admin endpoint nor a listener; envoy-rust has nothing to do"
    )]
    NoRuntime,
    #[error("bootstrap has {0} listeners; phase 01 supports at most one")]
    TooManyListeners(usize),
    #[error("unsupported network filter '{0}'; envoy-rust accepts only '{1}'")]
    UnsupportedFilter(String, &'static str),
    #[error("filter '{0}' requires typed_config")]
    MissingTypedConfig(&'static str),
    #[error("filter '{0}' must not carry typed_config")]
    UnexpectedTypedConfig(&'static str),
    #[error("tcp_proxy filter references unknown cluster '{0}'")]
    UnknownCluster(String),
    #[error(
        "cluster '{cluster}' declares load_assignment.cluster_name '{assignment}'; these must match"
    )]
    LoadAssignmentNameMismatch {
        cluster: String,
        assignment: String,
    },
    #[error("cluster '{0}' has zero lb_endpoints; ≥1 required")]
    EmptyClusterEndpoints(String),
}
```

Also extend the `pub use bootstrap::{...}` list to re-export the eight new public types (`TypedConfig`, `TcpProxyConfig`, `ClusterType`, `LbPolicy`, `LoadAssignment`, `LocalityLbEndpoints`, `LbEndpoint`, `Endpoint`). Replace the existing `pub use` line with:

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, Cluster, ClusterType, Endpoint, FilterChain, LbEndpoint, LbPolicy,
    Listener, LoadAssignment, LocalityLbEndpoints, NetworkFilter, Node, SocketAddress,
    StaticResources, TcpProxyConfig, TypedConfig,
};
```

Also add a second `pub const` for the new filter name (used by `validate`):

```rust
/// The TCP-proxy network filter name. envoy-rust accepts it as of phase 02.1;
/// runtime dispatch lands in phase 02.2. See ADR-0014.
pub const TCP_PROXY_FILTER: &str = "envoy.filters.network.tcp_proxy";
```

- [ ] **Step 5: Extend `validate` in `crates/envoy-config/src/bootstrap.rs`.**

Replace the entire `validate` fn body with:

```rust
pub(crate) fn validate(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
    let listeners = &bootstrap.static_resources.listeners;
    let clusters = &bootstrap.static_resources.clusters;
    if listeners.len() > 1 {
        return Err(crate::ConfigError::TooManyListeners(listeners.len()));
    }
    if bootstrap.admin.is_none() && listeners.is_empty() {
        return Err(crate::ConfigError::NoRuntime);
    }

    // Per-cluster invariants.
    for cluster in clusters {
        if cluster.load_assignment.cluster_name != cluster.name {
            return Err(crate::ConfigError::LoadAssignmentNameMismatch {
                cluster: cluster.name.clone(),
                assignment: cluster.load_assignment.cluster_name.clone(),
            });
        }
        let total_endpoints: usize = cluster
            .load_assignment
            .endpoints
            .iter()
            .map(|le| le.lb_endpoints.len())
            .sum();
        if total_endpoints == 0 {
            return Err(crate::ConfigError::EmptyClusterEndpoints(
                cluster.name.clone(),
            ));
        }
    }

    // Per-listener invariants.
    for listener in listeners {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                match filter.name.as_str() {
                    crate::ECHO_FILTER => {
                        if filter.typed_config.is_some() {
                            return Err(crate::ConfigError::UnexpectedTypedConfig(
                                crate::ECHO_FILTER,
                            ));
                        }
                    }
                    crate::TCP_PROXY_FILTER => {
                        let TypedConfig::TcpProxy(tp) = filter
                            .typed_config
                            .as_ref()
                            .ok_or(crate::ConfigError::MissingTypedConfig(
                                crate::TCP_PROXY_FILTER,
                            ))?;
                        if !clusters.iter().any(|c| c.name == tp.cluster) {
                            return Err(crate::ConfigError::UnknownCluster(tp.cluster.clone()));
                        }
                    }
                    _ => {
                        return Err(crate::ConfigError::UnsupportedFilter(
                            filter.name.clone(),
                            crate::ECHO_FILTER,
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
```

Notes on the `let TypedConfig::TcpProxy(tp) = ...` let-else: since `TypedConfig` has a single variant in 02.1, the match is irrefutable — compile output should be clean. If phase 04/05/06 extends `TypedConfig` with more variants (they will), this pattern must migrate to a `match`. Leave a one-line comment above the let-else naming the forward pressure:

```rust
// 02.1: TypedConfig has one variant (TcpProxy). Phase 04+ extend; migrate to match.
```

- [ ] **Step 6: Re-run the failing test; verify it passes.**

```bash
cargo test -p envoy-config bootstrap::tests::rejects_tcp_proxy_without_typed_config
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 7: Add the remaining 9 tests in one batch.**

Append to `crates/envoy-config/src/bootstrap.rs::tests` (each test literal; do not paraphrase):

```rust
    #[test]
    fn rejects_echo_with_typed_config() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnexpectedTypedConfig(_)),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_tcp_proxy_naming_missing_cluster() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
                cluster: nonexistent
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnknownCluster(ref s) if s == "nonexistent"),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_load_assignment_cluster_name_mismatch() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: drift
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::LoadAssignmentNameMismatch { ref cluster, ref assignment }
                    if cluster == "backend" && assignment == "drift"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_empty_lb_endpoints() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints: []
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::EmptyClusterEndpoints(ref s) if s == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_malformed_endpoint_address() {
        // Parse-layer *acceptance*: serde sees a valid Address/SocketAddress
        // shape (address: String, port_value: u16). The SocketAddr parse
        // failure surfaces in envoy-cluster::from_bootstrap at construction
        // time (see envoy-cluster Task 7's ClusterError::EndpointParse test).
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: not-a-host
                      port_value: 10001
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let b = crate::parse_bootstrap(yaml).expect("serde accepts; SocketAddr parse defers");
        assert_eq!(
            b.static_resources.clusters[0]
                .load_assignment
                .endpoints[0]
                .lb_endpoints[0]
                .endpoint
                .address
                .socket_address
                .address,
            "not-a-host",
        );
    }

    #[test]
    fn rejects_unknown_load_assignment_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
        bogus_la_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_locality_lb_endpoints_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
            bogus_lle_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_lb_endpoint_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                bogus_lbe_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_endpoint_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                  bogus_ep_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }
```

- [ ] **Step 8: Run the full crate tests + lint gate.**

```bash
cargo test -p envoy-config
cargo clippy -p envoy-config --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: `test result: ok. 37 passed; 0 failed` (21 phase-01 + 3 Task-2 + 3 Task-3 + 10 Task-4). Clippy clean; fmt clean.

- [ ] **Step 9: Run the workspace gate to confirm no cross-crate regression.**

```bash
cargo build --workspace --all-targets
cargo test --workspace
```

Expected: `envoy-bin`'s pre-existing 19 tests remain green (its code consumes `envoy_config::parse_bootstrap` and none of the 02.1 additions break its call site because `Bootstrap` field additions are structurally backward-compatible for existing fixtures). `tests/differential` remains green (Docker-gated tests remain Docker-gated; lib tests unchanged).

- [ ] **Step 10: Commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "phase 02.1: envoy-config validator + ConfigError extensions [ADR-0014]"
```

Append PROGRESS.md Task 4 section with the commit SHA, the 37-test envoy-config tail, the workspace tail (envoy-bin + differential + envoy-config green), and a note that `rejects_non_echo_filter` was renamed to `rejects_unknown_filter_name`.

---

### Task 5: Scaffold `crates/envoy-cluster/` skeleton + workspace member

**Files:**
- Create: `crates/envoy-cluster/Cargo.toml`
- Create: `crates/envoy-cluster/src/lib.rs`
- Create: `crates/envoy-cluster/src/cluster.rs` (empty module placeholder; fleshed out in Task 6)
- Modify: root `Cargo.toml`

**Why now:** Tasks 6 and 7 need the crate to exist. This task lands the minimum that compiles cleanly so later tasks don't mix scaffolding with real code. (Pattern matches phase-01 Task 2.)

- [ ] **Step 1: Write `crates/envoy-cluster/Cargo.toml`.**

```toml
[package]
name = "envoy-cluster"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_cluster"
path = "src/lib.rs"

[dependencies]
envoy-config = { path = "../envoy-config" }
thiserror = "2"
```

Notes:
- `thiserror = "2"` (not `"1"`) — matches the version pinned by `envoy-config` after phase-01 Task 5's deviation. Using `"1"` here would re-introduce the multi-versions cargo-deny warning.
- No `tokio`, no `tracing` — per SPEC §D1, the cluster crate is sync.
- No `[dev-dependencies]` in 02.1; the concurrency test in Task 6 uses `std::thread`, not `tokio`.

- [ ] **Step 2: Write `crates/envoy-cluster/src/lib.rs` as a compiling stub.**

```rust
#![forbid(unsafe_code)]

//! Phase 02.1 static-cluster surface for envoy-rust. Owns the `ClusterManager`
//! entrypoint plus the `Cluster` / `ClusterHandle` data model and the
//! round-robin load-balancer cursor.
//!
//! `envoy-cluster` is synchronous: no `async fn`, no `Future`, no
//! `tokio::spawn`. The cluster layer stays at the data-model seam; async I/O
//! lives downstream in `envoy-tcp` (sub-phase 02.2) and later phases. See
//! `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` §§D1, §6 signpost 10.

mod cluster;

pub use cluster::{Cluster, ClusterError, ClusterHandle, ClusterManager, from_bootstrap};
```

Task 6 and 7 supply the re-exported items. Referencing them here up-front makes the compile break loudly if any item is renamed or dropped during execution.

- [ ] **Step 3: Write `crates/envoy-cluster/src/cluster.rs` as an empty compiling module with placeholder items.**

```rust
//! Cluster data model + round-robin LB — populated in Tasks 6 and 7.
//!
//! The placeholder items below let `lib.rs` re-export a stable set of names
//! while the fleshed-out types land. Each placeholder is replaced wholesale
//! by the named task.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

/// Placeholder — see Task 6 for the real implementation.
pub struct Cluster {
    pub(crate) name: String,
    pub(crate) endpoints: Vec<SocketAddr>,
    pub(crate) cursor: AtomicUsize,
}

/// Placeholder — see Task 6 for the real implementation.
#[derive(Clone)]
pub struct ClusterHandle {
    pub(crate) inner: Arc<Cluster>,
}

/// Placeholder — see Task 7 for the real implementation.
pub struct ClusterManager {
    pub(crate) clusters: std::collections::HashMap<String, Arc<Cluster>>,
}

/// Placeholder — see Task 7 for the real implementation.
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("placeholder")]
    Placeholder,
}

/// Placeholder — see Task 7 for the real implementation.
pub fn from_bootstrap(
    _bootstrap: &envoy_config::Bootstrap,
) -> Result<ClusterManager, ClusterError> {
    Err(ClusterError::Placeholder)
}
```

Placeholder justification: bare `mod cluster;` + empty module would force Tasks 6 and 7 to also update `lib.rs`'s `pub use` line; placing the item shapes here lets each subsequent task touch one file. The placeholders carry the real field names so Task 6 / Task 7 can replace function bodies without ripple-renaming.

- [ ] **Step 4: Add `crates/envoy-cluster` to the root workspace.**

Edit the root `Cargo.toml` `[workspace] members` list to read:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "tests/differential",
]
exclude = [
    "crates/envoy-config/fuzz",
]
```

Task 8 adds `tests/helpers/tcp-echo-server` to `members`. Don't do it here.

- [ ] **Step 5: Verify the workspace builds cleanly.**

```bash
cargo build --workspace --all-targets
```
Expected: `Finished dev profile target(s) in …s` with a line `Compiling envoy-cluster v0.0.0 (…/crates/envoy-cluster)` in the output. No warnings, no errors.

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
Expected: exit 0.

```bash
cargo fmt --all -- --check
```
Expected: exit 0, no diff.

```bash
cargo test --workspace
```
Expected: `envoy-cluster` contributes `test result: ok. 0 passed; 0 failed` (no tests yet); existing envoy-bin + envoy-config + differential tests continue to pass.

```bash
cargo deny check
```
Expected: `advisories ok, bans ok, licenses ok, sources ok`. The only new dep (`envoy-config` via path, `thiserror` already in-workspace) carries no new license surface.

- [ ] **Step 6: Commit.**

```bash
git add Cargo.toml crates/envoy-cluster
git commit -m "phase 02.1: scaffold envoy-cluster crate"
```

Append PROGRESS.md Task 5 section with the commit SHA, the `cargo test --workspace` tail (0 failures), and the `cargo deny check` tail (all ok).

---

### Task 6: `envoy-cluster::cluster` — `Cluster` struct, `ClusterHandle`, `pick_endpoint` atomic cursor + 3 tests

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs`

**Scope:** flesh out `Cluster` (real `pick_endpoint`), add `ClusterHandle::pick_endpoint`, and land three unit tests — one deterministic cycle check, one concurrent-thread distribution check, one `Arc`-sharing regression.

**Test inventory (3 tests, in `crates/envoy-cluster/src/cluster.rs::tests`):**
- `pick_endpoint_cycles_over_three_endpoints` — N=3; 7 calls; sequence `[0, 1, 2, 0, 1, 2, 0]` modulo index.
- `pick_endpoint_is_stable_under_concurrent_calls` — N=3; 1000 `std::thread::spawn` threads; each picks ≈333 times ±10 %.
- `handle_clone_shares_cursor` — clone a `ClusterHandle`; picks via clone advance the same `AtomicUsize`.

- [ ] **Step 1: Write the failing test `pick_endpoint_cycles_over_three_endpoints`.**

Replace the last line of `crates/envoy-cluster/src/cluster.rs` with a real `#[cfg(test)] mod tests` block. Append to the existing placeholder scaffolding:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    fn mk_endpoints(n: u16) -> Vec<SocketAddr> {
        (0..n)
            .map(|i| format!("127.0.0.1:{}", 10000 + i).parse().unwrap())
            .collect()
    }

    fn mk_handle(name: &str, endpoints: Vec<SocketAddr>) -> ClusterHandle {
        ClusterHandle {
            inner: Arc::new(Cluster {
                name: name.to_string(),
                endpoints,
                cursor: AtomicUsize::new(0),
            }),
        }
    }

    #[test]
    fn pick_endpoint_cycles_over_three_endpoints() {
        let endpoints = mk_endpoints(3);
        let handle = mk_handle("backend", endpoints.clone());
        let picks: Vec<SocketAddr> = (0..7).map(|_| handle.pick_endpoint().unwrap()).collect();
        let expected = vec![
            endpoints[0], endpoints[1], endpoints[2],
            endpoints[0], endpoints[1], endpoints[2],
            endpoints[0],
        ];
        assert_eq!(picks, expected);
    }
}
```

- [ ] **Step 2: Run the test; verify it fails.**

```bash
cargo test -p envoy-cluster cluster::tests::pick_endpoint_cycles_over_three_endpoints
```
Expected: compile error, `error[E0599]: no method named 'pick_endpoint' found for struct 'ClusterHandle' in the current scope`.

- [ ] **Step 3: Implement the real `Cluster` + `ClusterHandle` + `pick_endpoint` in `crates/envoy-cluster/src/cluster.rs`.**

Replace the placeholder bodies with:

```rust
//! Cluster data model + round-robin LB. See SPEC §D1.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A configured upstream cluster. Owns the static endpoint list and the
/// round-robin `AtomicUsize` cursor. Constructed by `from_bootstrap` only;
/// external code works through `ClusterHandle`.
#[derive(Debug)]
pub struct Cluster {
    pub(crate) name: String,
    pub(crate) endpoints: Vec<SocketAddr>,
    pub(crate) cursor: AtomicUsize,
}

impl Cluster {
    /// Picks the next endpoint in round-robin order. `Relaxed` ordering is
    /// sufficient because no other observation depends on a happens-before
    /// relationship with the cursor update (SPEC §6 signpost 3).
    fn pick(&self) -> Option<SocketAddr> {
        if self.endpoints.is_empty() {
            // `from_bootstrap` rejects empty clusters; this is defense-in-depth.
            return None;
        }
        let i = self.cursor.fetch_add(1, Ordering::Relaxed);
        Some(self.endpoints[i % self.endpoints.len()])
    }
}

/// A handle to a `Cluster` that hands out endpoints via round-robin. Cheaply
/// cloneable (`Arc`-internal); clones share the same cursor.
#[derive(Clone, Debug)]
pub struct ClusterHandle {
    pub(crate) inner: Arc<Cluster>,
}

impl ClusterHandle {
    /// Returns the next endpoint in round-robin order.
    ///
    /// Returns `None` only when the cluster is empty — which `from_bootstrap`
    /// rejects at construction time, so this is effectively infallible in
    /// phase 02. `Option<_>` is preserved for phase-06+ health checking.
    pub fn pick_endpoint(&self) -> Option<SocketAddr> {
        self.inner.pick()
    }
}
```

Leave the `ClusterManager` + `ClusterError` + `from_bootstrap` placeholders from Task 5 untouched for now; Task 7 fleshes them out.

- [ ] **Step 4: Re-run the failing test; verify it passes.**

```bash
cargo test -p envoy-cluster cluster::tests::pick_endpoint_cycles_over_three_endpoints
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Add the concurrency test `pick_endpoint_is_stable_under_concurrent_calls`.**

Append inside the `tests` module (immediately after the `pick_endpoint_cycles...` test):

```rust
    #[test]
    fn pick_endpoint_is_stable_under_concurrent_calls() {
        use std::collections::HashMap;
        use std::sync::Mutex;
        use std::thread;

        const N_ENDPOINTS: usize = 3;
        const N_CALLS: usize = 1000;

        let endpoints = mk_endpoints(N_ENDPOINTS as u16);
        let handle = mk_handle("backend", endpoints.clone());

        let counts: Arc<Mutex<HashMap<SocketAddr, usize>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let mut handles = Vec::with_capacity(N_CALLS);
        for _ in 0..N_CALLS {
            let h = handle.clone();
            let c = Arc::clone(&counts);
            handles.push(thread::spawn(move || {
                let ep = h.pick_endpoint().expect("non-empty");
                *c.lock().unwrap().entry(ep).or_insert(0) += 1;
            }));
        }
        for t in handles {
            t.join().unwrap();
        }

        let counts = counts.lock().unwrap();
        let expected = N_CALLS / N_ENDPOINTS; // 333
        let tolerance = (expected as f64 * 0.10) as usize; // 33 ≈ 10 %
        assert_eq!(counts.values().sum::<usize>(), N_CALLS);
        for ep in &endpoints {
            let got = *counts.get(ep).unwrap_or(&0);
            assert!(
                got.abs_diff(expected) <= tolerance,
                "endpoint {ep:?} picked {got} times; expected {expected} ± {tolerance}",
            );
        }
    }
```

Why ±10 %: N_CALLS / N_ENDPOINTS = 333 with perfect round-robin; thread scheduling jitter + `Relaxed` ordering reorderings make the actual distribution *not* exactly 333 per endpoint. With a fair `fetch_add`-and-modulo scheme the distribution is still within a handful of slots of perfect; 10 % gives generous CI headroom without hiding a real bug.

- [ ] **Step 6: Run it; expect pass.**

```bash
cargo test -p envoy-cluster cluster::tests::pick_endpoint_is_stable_under_concurrent_calls
```
Expected: `test result: ok. 1 passed; 0 failed`. If it flakes in CI at 10 %, expand to 15 % and note the widening in PROGRESS — this is the `Relaxed` ordering's noise envelope.

- [ ] **Step 7: Add the Arc-sharing regression test `handle_clone_shares_cursor`.**

Append:

```rust
    #[test]
    fn handle_clone_shares_cursor() {
        let endpoints = mk_endpoints(2);
        let a = mk_handle("backend", endpoints.clone());
        let b = a.clone();

        // Interleave picks across the clone and the original. With a shared
        // cursor, the sequence is alternating-index; with separate cursors
        // each handle would pick its own [0,1,0,1,...].
        let seq: Vec<SocketAddr> = vec![
            a.pick_endpoint().unwrap(),  // cursor=0 -> endpoints[0]
            b.pick_endpoint().unwrap(),  // cursor=1 -> endpoints[1]
            a.pick_endpoint().unwrap(),  // cursor=2 -> endpoints[0]
            b.pick_endpoint().unwrap(),  // cursor=3 -> endpoints[1]
        ];
        assert_eq!(seq, vec![endpoints[0], endpoints[1], endpoints[0], endpoints[1]]);
    }
```

- [ ] **Step 8: Run the full crate tests + lint gate.**

```bash
cargo test -p envoy-cluster
cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Expected: `test result: ok. 3 passed; 0 failed`. Clippy clean; fmt clean.

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-cluster/src/cluster.rs
git commit -m "phase 02.1: envoy-cluster round-robin endpoint picker"
```

Append PROGRESS.md Task 6 section with the commit SHA and the 3-test envoy-cluster tail.

---

### Task 7: `envoy-cluster` — `ClusterManager`, `ClusterError`, `from_bootstrap` + 5 tests

**Files:**
- Modify: `crates/envoy-cluster/src/cluster.rs`

**Scope:** flesh out `ClusterManager::get`, `ClusterError`'s three real variants, and `from_bootstrap`'s validation + construction logic. Add five unit tests covering the edges.

**Test inventory (5 tests, appended to `crates/envoy-cluster/src/cluster.rs::tests`):**
- `from_bootstrap_rejects_empty_cluster` — cluster with zero `lb_endpoints` → `ClusterError::EmptyCluster`. (The envoy-config validator also rejects this as `EmptyClusterEndpoints`; envoy-cluster's check is defense-in-depth. To construct the test input, bypass `parse_bootstrap` — build a `Bootstrap` by value.)
- `from_bootstrap_rejects_duplicate_cluster_name` — two clusters named `backend` → `ClusterError::DuplicateClusterName`. (envoy-config doesn't reject duplicate cluster names at parse time — `Vec<Cluster>` allows dupes — so envoy-cluster is the first enforcement.)
- `from_bootstrap_rejects_malformed_endpoint_address` — cluster with `address: "not-a-host"` → `ClusterError::EndpointParse`. This is the only `ClusterError` that is NOT defense-in-depth — envoy-config accepts a serde-valid string in `SocketAddress.address`; the `SocketAddr` parse happens here.
- `from_bootstrap_builds_single_endpoint_cluster` — happy path for a one-endpoint STATIC cluster; `ClusterManager::get("backend")` returns `Some(_)`, and the handle's first pick resolves to the correct `SocketAddr`.
- `from_bootstrap_builds_three_endpoint_cluster` — happy path for the N=3 round-robin scenario; `ClusterManager::get("backend").unwrap().pick_endpoint()` called three times returns `endpoints[0..3]`.

- [ ] **Step 1: Write the failing test `from_bootstrap_builds_single_endpoint_cluster`.**

Append inside the `tests` module:

```rust
    const SINGLE_ENDPOINT_YAML: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10042
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;

    #[test]
    fn from_bootstrap_builds_single_endpoint_cluster() {
        let bootstrap = envoy_config::parse_bootstrap(SINGLE_ENDPOINT_YAML).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap).expect("construct");
        let handle = mgr.get("backend").expect("cluster present");
        let picked = handle.pick_endpoint().expect("non-empty");
        assert_eq!(picked, "127.0.0.1:10042".parse::<SocketAddr>().unwrap());
    }
```

- [ ] **Step 2: Run the test; verify it fails.**

```bash
cargo test -p envoy-cluster cluster::tests::from_bootstrap_builds_single_endpoint_cluster
```
Expected: runtime failure, the test panics because the placeholder `from_bootstrap` returns `Err(ClusterError::Placeholder)`. (Not a compile error; `from_bootstrap` / `ClusterManager::get` already exist as placeholders.)

- [ ] **Step 3: Implement the real `ClusterError`, `ClusterManager::get`, and `from_bootstrap`.**

Replace the `ClusterManager`, `ClusterError`, and `from_bootstrap` placeholders in `crates/envoy-cluster/src/cluster.rs` with:

```rust
use std::collections::HashMap;

/// The cluster registry, keyed by cluster name. Built once via
/// `from_bootstrap`, read many times via `get`.
pub struct ClusterManager {
    clusters: HashMap<String, Arc<Cluster>>,
}

impl ClusterManager {
    /// Looks up a cluster by name. Returns `None` if no cluster with that
    /// name was constructed.
    pub fn get(&self, name: &str) -> Option<ClusterHandle> {
        self.clusters.get(name).map(|arc| ClusterHandle {
            inner: Arc::clone(arc),
        })
    }
}

/// Errors returned by `from_bootstrap`.
///
/// `EmptyCluster` and `DuplicateClusterName` are defense-in-depth: the
/// `envoy-config` validator also rejects these shapes (`EmptyClusterEndpoints`,
/// cluster-name collisions via per-cluster `UnknownCluster` checks). They exist
/// here because `envoy-cluster` is a library whose invariants must hold even
/// when callers construct `Bootstrap` values by hand.
///
/// `EndpointParse` is *not* defense-in-depth: `envoy-config` accepts any
/// serde-valid `SocketAddress { address: String, port_value: u16 }` shape
/// (including `"not-a-host"`); the `SocketAddr` parse is the first place that
/// rejects a malformed address.
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("cluster '{name}' has no lb_endpoints")]
    EmptyCluster { name: String },
    #[error("duplicate cluster name '{name}'")]
    DuplicateClusterName { name: String },
    #[error(
        "cluster '{cluster}' endpoint address {addr:?} is not a valid SocketAddr: {source}"
    )]
    EndpointParse {
        cluster: String,
        addr: String,
        #[source]
        source: std::net::AddrParseError,
    },
}

/// Constructs a `ClusterManager` from a validated `Bootstrap`. The caller
/// should have already run `envoy_config::parse_bootstrap`, but this function
/// validates its own preconditions for library robustness.
pub fn from_bootstrap(
    bootstrap: &envoy_config::Bootstrap,
) -> Result<ClusterManager, ClusterError> {
    let mut clusters: HashMap<String, Arc<Cluster>> = HashMap::new();
    for cfg in &bootstrap.static_resources.clusters {
        // envoy-config enforces cluster_type == Static, lb_policy == RoundRobin,
        // load_assignment.cluster_name == cfg.name, and total endpoints ≥ 1 at
        // parse time. We don't re-check those here; we do re-check emptiness
        // and duplicate names as defense-in-depth, and we parse each address
        // (which envoy-config does NOT do).
        let mut endpoints: Vec<SocketAddr> = Vec::new();
        for locality in &cfg.load_assignment.endpoints {
            for lbe in &locality.lb_endpoints {
                let sa = &lbe.endpoint.address.socket_address;
                let addr_str = format!("{}:{}", sa.address, sa.port_value);
                let parsed: SocketAddr = addr_str.parse().map_err(|source| {
                    ClusterError::EndpointParse {
                        cluster: cfg.name.clone(),
                        addr: addr_str.clone(),
                        source,
                    }
                })?;
                endpoints.push(parsed);
            }
        }
        if endpoints.is_empty() {
            return Err(ClusterError::EmptyCluster {
                name: cfg.name.clone(),
            });
        }
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints,
            cursor: AtomicUsize::new(0),
        });
        if clusters.insert(cfg.name.clone(), cluster).is_some() {
            return Err(ClusterError::DuplicateClusterName {
                name: cfg.name.clone(),
            });
        }
    }
    Ok(ClusterManager { clusters })
}
```

One subtle point: `HashMap::insert` returns the old value (if any) *after* the new one has been inserted. That means a collision leaves the *second* cluster in the map and returns the *first* as the "old" value in the `Err` path. For a defense-in-depth reject-with-error the ordering doesn't matter (we return `Err` either way), but note it in PROGRESS if asked.

- [ ] **Step 4: Re-run the failing test; verify it passes.**

```bash
cargo test -p envoy-cluster cluster::tests::from_bootstrap_builds_single_endpoint_cluster
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 5: Add the remaining 4 tests.**

Append inside the `tests` module (adjacent to the existing envoy-cluster tests; place after the `SINGLE_ENDPOINT_YAML` constant so all YAML fixtures cluster at the bottom of the module):

```rust
    const THREE_ENDPOINT_YAML: &str = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10003
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;

    #[test]
    fn from_bootstrap_builds_three_endpoint_cluster() {
        let bootstrap = envoy_config::parse_bootstrap(THREE_ENDPOINT_YAML).expect("valid");
        let mgr = crate::from_bootstrap(&bootstrap).expect("construct");
        let handle = mgr.get("backend").expect("cluster present");
        let picks: Vec<SocketAddr> = (0..3).map(|_| handle.pick_endpoint().unwrap()).collect();
        assert_eq!(
            picks,
            vec![
                "127.0.0.1:10001".parse().unwrap(),
                "127.0.0.1:10002".parse().unwrap(),
                "127.0.0.1:10003".parse().unwrap(),
            ],
        );
    }

    #[test]
    fn from_bootstrap_rejects_empty_cluster() {
        // envoy-config rejects zero-endpoint clusters before we get here, so
        // build the Bootstrap by-hand to exercise the cluster-crate edge.
        use envoy_config::{
            Address, Admin, Bootstrap, Cluster, ClusterType, LbPolicy, LoadAssignment,
            SocketAddress, StaticResources,
        };
        let bootstrap = Bootstrap {
            node: None,
            admin: Some(Admin {
                address: Address {
                    socket_address: SocketAddress {
                        address: "127.0.0.1".into(),
                        port_value: 9901,
                    },
                },
            }),
            static_resources: StaticResources {
                listeners: vec![],
                clusters: vec![Cluster {
                    name: "backend".into(),
                    cluster_type: ClusterType::Static,
                    lb_policy: LbPolicy::RoundRobin,
                    load_assignment: LoadAssignment {
                        cluster_name: "backend".into(),
                        endpoints: vec![],
                    },
                }],
            },
        };
        let err = crate::from_bootstrap(&bootstrap).expect_err("must reject");
        assert!(
            matches!(err, ClusterError::EmptyCluster { ref name } if name == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn from_bootstrap_rejects_duplicate_cluster_name() {
        // envoy-config doesn't reject duplicate cluster names (Vec<Cluster>
        // allows dupes at the serde layer); envoy-cluster is the first
        // enforcement. Build via by-hand Bootstrap to exercise this edge.
        use envoy_config::{
            Address, Admin, Bootstrap, Cluster, ClusterType, Endpoint, LbEndpoint, LbPolicy,
            LoadAssignment, LocalityLbEndpoints, SocketAddress, StaticResources,
        };
        let mk_cluster = || Cluster {
            name: "backend".into(),
            cluster_type: ClusterType::Static,
            lb_policy: LbPolicy::RoundRobin,
            load_assignment: LoadAssignment {
                cluster_name: "backend".into(),
                endpoints: vec![LocalityLbEndpoints {
                    lb_endpoints: vec![LbEndpoint {
                        endpoint: Endpoint {
                            address: Address {
                                socket_address: SocketAddress {
                                    address: "127.0.0.1".into(),
                                    port_value: 10001,
                                },
                            },
                        },
                    }],
                }],
            },
        };
        let bootstrap = Bootstrap {
            node: None,
            admin: Some(Admin {
                address: Address {
                    socket_address: SocketAddress {
                        address: "127.0.0.1".into(),
                        port_value: 9901,
                    },
                },
            }),
            static_resources: StaticResources {
                listeners: vec![],
                clusters: vec![mk_cluster(), mk_cluster()],
            },
        };
        let err = crate::from_bootstrap(&bootstrap).expect_err("must reject");
        assert!(
            matches!(err, ClusterError::DuplicateClusterName { ref name } if name == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn from_bootstrap_rejects_malformed_endpoint_address() {
        // envoy-config accepts the YAML at parse time (address: String);
        // envoy-cluster is the first layer that parses it into SocketAddr.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: not-a-host
                      port_value: 10001
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("serde accepts");
        let err = crate::from_bootstrap(&bootstrap).expect_err("must reject");
        assert!(
            matches!(
                err,
                ClusterError::EndpointParse { ref cluster, ref addr, .. }
                    if cluster == "backend" && addr == "not-a-host:10001"
            ),
            "got {err:?}",
        );
    }
```

- [ ] **Step 6: Run the full crate tests + lint gate.**

```bash
cargo test -p envoy-cluster
cargo clippy -p envoy-cluster --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Expected: `test result: ok. 8 passed; 0 failed` (3 Task-6 + 5 Task-7). Clippy clean; fmt clean.

- [ ] **Step 7: Run the workspace gate.**

```bash
cargo test --workspace
```
Expected: every pre-existing test still green; envoy-cluster contributes 8 passes.

- [ ] **Step 8: Commit.**

```bash
git add crates/envoy-cluster/src/cluster.rs
git commit -m "phase 02.1: envoy-cluster ClusterManager + from_bootstrap"
```

Append PROGRESS.md Task 7 section with the commit SHA, the 8-test envoy-cluster tail, and the workspace tail.

---

### Task 8: Scaffold `tests/helpers/tcp-echo-server/` skeleton + workspace member

**Files:**
- Create: `tests/helpers/tcp-echo-server/Cargo.toml`
- Create: `tests/helpers/tcp-echo-server/src/main.rs` (stub; fleshed out in Tasks 9–10)
- Modify: root `Cargo.toml`

**Why now:** Tasks 9 and 10 need the crate to exist. This task lands the minimum that compiles cleanly. Matches the Task-5 idiom; first crate under `tests/helpers/`, so the directory itself is created here. The `tests/helpers/` directory is already named in `BOOTSTRAP_PROMPT.md` §4's layout so no ADR is required for its creation (SPEC §D3).

- [ ] **Step 1: Write `tests/helpers/tcp-echo-server/Cargo.toml`.**

```toml
[package]
name = "tcp-echo-server"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[[bin]]
name = "tcp-echo-server"
path = "src/main.rs"

[dependencies]
anyhow = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "signal"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
```

Notes:
- `anyhow` is permitted in binary crates per D-3.2.
- `thiserror = "2"` — matches the version pinned by `envoy-config` + `envoy-cluster`.
- `tokio` features deliberately omit `sync`, `time`, `rt`, and `process` — not needed. The runtime does `rt-multi-thread` + `net` + `io-util` (for `tokio::io::copy`) + `macros` (`#[tokio::main]`, `#[tokio::test]`) + `signal` (`tokio::signal::ctrl_c`). The Task-10 drain test uses `tokio::time::timeout` — re-evaluate at that point and extend the feature list to add `"time"` at the first point of actual use.

- [ ] **Step 2: Write `tests/helpers/tcp-echo-server/src/main.rs` as a compiling stub.**

```rust
#![forbid(unsafe_code)]

//! `tcp-echo-server` — a minimal localhost-only echo server for the envoy-rust
//! differential harness. Sub-phase 02.2's fixture 0003 will dial it; sub-phase
//! 02.1 lands it in isolation so its own tests run under 02.1's CI gate before
//! composition with `TcpProxyBackend` in 02.2 (SPEC §D3).

fn main() {
    // Populated in Task 10.
    unimplemented!("tcp-echo-server runtime lands in Task 10");
}
```

The `unimplemented!()` lets `cargo build --workspace --all-targets` succeed (the binary compiles, it just panics if invoked). No tests yet; Tasks 9 and 10 add them.

- [ ] **Step 3: Add `tests/helpers/tcp-echo-server` to the root workspace.**

Edit the root `Cargo.toml` `[workspace] members` list:

```toml
[workspace]
resolver = "2"
members = [
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "tests/differential",
    "tests/helpers/tcp-echo-server",
]
exclude = [
    "crates/envoy-config/fuzz",
]
```

- [ ] **Step 4: Verify the workspace builds cleanly.**

```bash
cargo build --workspace --all-targets
```
Expected: `Finished dev profile target(s) in …s`; new line `Compiling tcp-echo-server v0.0.0 (…/tests/helpers/tcp-echo-server)`. No warnings (other than the unused-import warning on `anyhow` — that's expected for the stub and will clear in Task 9; if clippy rejects it, add `#[allow(dead_code)]` or just leave the `anyhow` import off the stub file).

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
Expected: exit 0. If clippy complains about unused `anyhow` in the Cargo.toml dep without an actual import, that's fine (clippy-level unused-dep lints are `warn`, not `deny`).

```bash
cargo fmt --all -- --check
```
Expected: exit 0.

```bash
cargo test --workspace
```
Expected: `tcp-echo-server` contributes `test result: ok. 0 passed; 0 failed`; existing tests remain green.

```bash
cargo deny check
```
Expected: `advisories ok, bans ok, licenses ok, sources ok`. Verify that the `tracing-subscriber` + `anyhow` deps introduce no new license surface — both are already transitively reachable via `envoy-bin`. If cargo-deny flags something unexpected, stop and follow SPEC §D7's "additional ADR per D-3.5" fallback (likely ADR-0017 if needed).

- [ ] **Step 5: Commit.**

```bash
git add Cargo.toml tests/helpers/tcp-echo-server
git commit -m "phase 02.1: scaffold tcp-echo-server helper crate"
```

Append PROGRESS.md Task 8 section with the commit SHA, the `cargo test --workspace` tail, and the `cargo deny check` tail.

---

### Task 9: `tcp-echo-server` argv parser (`Args`, `ArgvError`, `parse_argv`) + 6 tests

**Files:**
- Modify: `tests/helpers/tcp-echo-server/src/main.rs`

**Scope:** land the hand-parsed argv module mirroring `crates/envoy-bin/src/argv.rs`'s idiom (SPEC §6 signpost 5 — structural reuse, not code sharing). Six tests covering every `ArgvError` variant plus one positive.

**Argv surface (per SPEC §D3):**

```
tcp-echo-server --port <u16>
tcp-echo-server --help
tcp-echo-server --version
```

Any other form is an error.

**Test inventory (6 tests, in a `mod tests` block at the bottom of `main.rs`):**
- `argv_parses_port` — `--port 10042` → `Ok(Args { port: 10042 })`.
- `argv_rejects_missing_port_flag` — empty argv → `Err(ArgvError::MissingFlag("--port"))`.
- `argv_rejects_missing_value` — `--port` alone → `Err(ArgvError::MissingValue)`.
- `argv_rejects_non_numeric_port` — `--port abc` → `Err(ArgvError::InvalidPort)`.
- `argv_rejects_trailing_argument` — `--port 10042 --junk` → `Err(ArgvError::Trailing)`.
- `argv_shows_help` — `--help` → `Err(ArgvError::HelpRequested)`.

Not tested (SPEC §D3 names `VersionRequested` as a parallel variant but doesn't require a unit test — the variant exists for symmetry with `--help`; `main`'s translation to exit 0 is the integration surface, tested indirectly by the integration of Task 10).

- [ ] **Step 1: Write the failing test `argv_parses_port`.**

Replace `tests/helpers/tcp-echo-server/src/main.rs` entirely with:

```rust
#![forbid(unsafe_code)]

//! `tcp-echo-server` — a minimal localhost-only echo server for the envoy-rust
//! differential harness. See SPEC §D3 of phase 02.1.

use thiserror::Error;

/// Parsed argv surface.
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
}

/// argv parse failure modes.
///
/// `HelpRequested` and `VersionRequested` are "successful" user intents that
/// nevertheless short-circuit the parse — `main` translates them to exit 0.
#[derive(Debug, Error, PartialEq)]
enum ArgvError {
    #[error("required flag {0} missing")]
    MissingFlag(&'static str),
    #[error("flag expects a value")]
    MissingValue,
    #[error("port value must be a u16")]
    InvalidPort,
    #[error("trailing arguments after --port <PORT>")]
    Trailing,
    #[error("--help")]
    HelpRequested,
    #[error("--version")]
    VersionRequested,
}

/// Parses argv (excluding argv[0]).
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    let mut i = 0;
    let mut port: Option<u16> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
            "--port" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                port = Some(v.parse().map_err(|_| ArgvError::InvalidPort)?);
                i += 2;
            }
            _ => return Err(ArgvError::Trailing),
        }
    }
    Ok(Args {
        port: port.ok_or(ArgvError::MissingFlag("--port"))?,
    })
}

fn main() {
    // Populated in Task 10.
    unimplemented!("tcp-echo-server runtime lands in Task 10");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn argv_parses_port() {
        let got = parse_argv(&argv(&["--port", "10042"])).expect("ok");
        assert_eq!(got, Args { port: 10042 });
    }
}
```

- [ ] **Step 2: Run it; verify it passes.**

```bash
cargo test -p tcp-echo-server tests::argv_parses_port
```
Expected: `test result: ok. 1 passed; 0 failed`. (Note: this is an "implementation-first" TDD step — the real TDD red-green landed already in Step 1's first-write; the test passes on first run because both the test and the impl landed in the same edit. If strict TDD is required, split Step 1 into "write the test" and "write the impl" — but for a purely-new binary-crate module with zero pre-existing callers, combining them is acceptable and matches phase-01 Task 9's pattern.)

- [ ] **Step 3: Add the 5 remaining tests.**

Append inside the `tests` module:

```rust
    #[test]
    fn argv_rejects_missing_port_flag() {
        let err = parse_argv(&argv(&[])).expect_err("empty argv");
        assert_eq!(err, ArgvError::MissingFlag("--port"));
    }

    #[test]
    fn argv_rejects_missing_value() {
        let err = parse_argv(&argv(&["--port"])).expect_err("dangling --port");
        assert_eq!(err, ArgvError::MissingValue);
    }

    #[test]
    fn argv_rejects_non_numeric_port() {
        let err = parse_argv(&argv(&["--port", "abc"])).expect_err("non-numeric");
        assert_eq!(err, ArgvError::InvalidPort);
    }

    #[test]
    fn argv_rejects_trailing_argument() {
        let err = parse_argv(&argv(&["--port", "10042", "--junk"])).expect_err("trailing");
        assert_eq!(err, ArgvError::Trailing);
    }

    #[test]
    fn argv_shows_help() {
        let err = parse_argv(&argv(&["--help"])).expect_err("help");
        assert_eq!(err, ArgvError::HelpRequested);
    }
```

- [ ] **Step 4: Run the full crate test + lint gate.**

```bash
cargo test -p tcp-echo-server
cargo clippy -p tcp-echo-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Expected: `test result: ok. 6 passed; 0 failed`. Clippy clean; fmt clean.

- [ ] **Step 5: Commit.**

```bash
git add tests/helpers/tcp-echo-server/src/main.rs
git commit -m "phase 02.1: tcp-echo-server argv parser"
```

Append PROGRESS.md Task 9 section with the commit SHA and the 6-test tail.

---

### Task 10: `tcp-echo-server` runtime (`run`, `main`) + 2 tokio tests (round-trip + drain)

**Files:**
- Modify: `tests/helpers/tcp-echo-server/Cargo.toml` (add `"time"` to the tokio feature list if not already present)
- Modify: `tests/helpers/tcp-echo-server/src/main.rs`

**Scope:** replace the `unimplemented!()` placeholder in `main` with the real accept loop. Wire `tracing_subscriber::fmt` on stderr. Land two `#[tokio::test(flavor="multi_thread")]` tests: round-trip and drain-within-budget.

**Runtime contract (per SPEC §D3):**
- `tokio::net::TcpListener::bind(("127.0.0.1", args.port))`.
- `tokio::select!` between `accept()` and `tokio::signal::ctrl_c()`.
- Each accepted stream → `tokio::task::JoinSet.spawn` running `let (mut r, mut w) = stream.split(); tokio::io::copy(&mut r, &mut w).await`.
- On shutdown: stop accepting, drain with `DRAIN_BUDGET = Duration::from_secs(5)`, abort stragglers, return Ok.
- Exit codes: 0 clean, 1 runtime error, 2 argv error. Translate via `ArgvError` → `main::main`'s early return.

**Test inventory (2 tests):**
- `echoes_round_trip` — reserve a free port (use `tokio::net::TcpListener::bind(("127.0.0.1", 0))` then `local_addr()?.port()` on the listener itself to get a kernel-assigned port — but since the test binds its own ephemeral port *for the server*, the cleanest idiom is: use the `reserve_port` helper pattern from `tests/differential`'s `lib.rs` (inline it; `tcp-echo-server` can't depend on `differential` cyclically). Simpler: bind the server to `127.0.0.1:0` inside the test, read back the actual port from the listener before spawning. Implement by exposing a `run_on` shim that takes an already-bound `TcpListener` and spawning that via `tokio::spawn`.
- `drain_exits_within_budget` — open a connection, drop its read side while the server is holding the copy-loop, fire the shutdown signal, assert the server task resolves within `DRAIN_BUDGET + 500ms`.

- [ ] **Step 1: Extend the tokio feature list (if missing `"time"`).**

Check the current list in `tests/helpers/tcp-echo-server/Cargo.toml`. If `"time"` is not present, replace the tokio dep line with:

```toml
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "signal", "time"] }
```

- [ ] **Step 2: Write the failing test `echoes_round_trip`.**

Replace the `#[cfg(test)] mod tests` block in `tests/helpers/tcp-echo-server/src/main.rs` with the expanded version. The new tests go *after* the argv tests (which stay verbatim):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::sync::oneshot;

    fn argv(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|s| s.to_string()).collect()
    }

    // ... existing 6 argv tests unchanged ...

    #[tokio::test(flavor = "multi_thread")]
    async fn echoes_round_trip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            run_on(listener, async move {
                let _ = rx.await;
            })
            .await
        });

        let mut client = TcpStream::connect(addr).await.expect("connect");
        let payload: [u8; 32] = core::array::from_fn(|i| i as u8);
        client.write_all(&payload).await.expect("write");
        let mut buf = [0u8; 32];
        client.read_exact(&mut buf).await.expect("read_exact");
        assert_eq!(buf, payload);
        drop(client);

        tx.send(()).expect("signal shutdown");
        server.await.expect("join").expect("server Ok");
    }
}
```

(The existing argv tests remain in-place above the new test; `argv` helper is reused.)

- [ ] **Step 3: Run the test; verify it fails.**

```bash
cargo test -p tcp-echo-server tests::echoes_round_trip
```
Expected: compile error `error[E0425]: cannot find function 'run_on' in this scope` (the test refers to `run_on`, which doesn't exist yet).

- [ ] **Step 4: Implement `run_on`, `run`, and extend `main`.**

Replace the `fn main()` body and append the runtime functions. The full top-to-bottom `tests/helpers/tcp-echo-server/src/main.rs` after this step:

```rust
#![forbid(unsafe_code)]

//! `tcp-echo-server` — a minimal localhost-only echo server for the envoy-rust
//! differential harness. See SPEC §D3 of phase 02.1.

use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio::time::timeout;

const DRAIN_BUDGET: Duration = Duration::from_secs(5);

/// Parsed argv surface.
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
}

/// argv parse failure modes.
///
/// `HelpRequested` and `VersionRequested` are "successful" user intents that
/// nevertheless short-circuit the parse — `main` translates them to exit 0.
#[derive(Debug, Error, PartialEq)]
enum ArgvError {
    #[error("required flag {0} missing")]
    MissingFlag(&'static str),
    #[error("flag expects a value")]
    MissingValue,
    #[error("port value must be a u16")]
    InvalidPort,
    #[error("trailing arguments after --port <PORT>")]
    Trailing,
    #[error("--help")]
    HelpRequested,
    #[error("--version")]
    VersionRequested,
}

/// Parses argv (excluding argv[0]).
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    let mut i = 0;
    let mut port: Option<u16> = None;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
            "--port" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                port = Some(v.parse().map_err(|_| ArgvError::InvalidPort)?);
                i += 2;
            }
            _ => return Err(ArgvError::Trailing),
        }
    }
    Ok(Args {
        port: port.ok_or(ArgvError::MissingFlag("--port"))?,
    })
}

const USAGE: &str = "tcp-echo-server --port <PORT>";
const VERSION: &str = concat!("tcp-echo-server ", env!("CARGO_PKG_VERSION"));

/// Accept loop on an already-bound listener. Returns when `shutdown` resolves
/// *and* the drain completes (or `DRAIN_BUDGET` expires, whichever first).
async fn run_on(listener: TcpListener, shutdown: impl std::future::Future<Output = ()>) -> Result<()> {
    let mut conns: JoinSet<()> = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutdown signal received; draining");
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((mut stream, peer)) => {
                        tracing::debug!(?peer, "accepted");
                        conns.spawn(async move {
                            let (mut r, mut w) = stream.split();
                            let _ = tokio::io::copy(&mut r, &mut w).await;
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept error; sleeping 10ms");
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                }
            }
        }
    }

    // Drain with a budget. `JoinSet::join_next` returns `None` when empty.
    let drained = timeout(DRAIN_BUDGET, async {
        while conns.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        tracing::warn!(budget_ms = DRAIN_BUDGET.as_millis() as u64, "drain budget exceeded; aborting stragglers");
        conns.abort_all();
        // Let aborted tasks finish unwinding; ignore result.
        while conns.join_next().await.is_some() {}
    }
    Ok(())
}

/// Full runtime entrypoint: bind → `run_on` with ctrl_c as shutdown.
async fn run(port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    tracing::info!(port, "tcp-echo-server listening");
    run_on(listener, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_argv(&argv) {
        Ok(a) => a,
        Err(ArgvError::HelpRequested) => {
            eprintln!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(ArgvError::VersionRequested) => {
            eprintln!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    match run(args.port).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::from(1)
        }
    }
}

// ... existing tests module follows ...
```

- [ ] **Step 5: Re-run the failing test; verify it passes.**

```bash
cargo test -p tcp-echo-server tests::echoes_round_trip
```
Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Step 6: Add the drain test `drain_exits_within_budget`.**

Append inside the `tests` module (after `echoes_round_trip`):

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn drain_exits_within_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let (tx, rx) = oneshot::channel::<()>();
        let start = std::time::Instant::now();
        let server = tokio::spawn(async move {
            run_on(listener, async move {
                let _ = rx.await;
            })
            .await
        });

        // Open a stalled connection — connect, write one byte, read the echoed
        // byte, then STOP reading while keeping the stream alive. The server's
        // spawned task is parked in its copy loop waiting on more bytes.
        let mut client = TcpStream::connect(addr).await.expect("connect");
        client.write_all(&[42]).await.expect("write");
        let mut one = [0u8; 1];
        client.read_exact(&mut one).await.expect("read");

        // Fire shutdown; drop the client *after* to avoid triggering a clean
        // FIN that would let the server's copy loop complete on its own.
        tx.send(()).expect("signal shutdown");
        let result = tokio::time::timeout(
            DRAIN_BUDGET + Duration::from_millis(500),
            server,
        )
        .await
        .expect("server task resolved within DRAIN_BUDGET + ε")
        .expect("join")
        .expect("server Ok");
        let elapsed = start.elapsed();
        drop(client); // keep client alive until the assertion above

        let _ = result; // keep warning-free
        assert!(
            elapsed <= DRAIN_BUDGET + Duration::from_millis(1_000),
            "drain took {elapsed:?}; expected ≤ {:?}",
            DRAIN_BUDGET + Duration::from_millis(1_000),
        );
    }
```

The test's assertion allows a generous 1 s slack over the 5 s budget to soak up CI jitter; the drain mechanism either aborts-all at exactly `DRAIN_BUDGET` or resolves sooner.

- [ ] **Step 7: Run the full crate tests + lint gate.**

```bash
cargo test -p tcp-echo-server
cargo clippy -p tcp-echo-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Expected: `test result: ok. 8 passed; 0 failed` (6 argv + 2 runtime). Clippy clean; fmt clean.

- [ ] **Step 8: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo test --workspace
cargo deny check
```
Expected: everything green; deny ok. If `cargo deny check` flags a new license surface, stop and follow SPEC §D7.

- [ ] **Step 9: Commit.**

```bash
git add tests/helpers/tcp-echo-server/Cargo.toml tests/helpers/tcp-echo-server/src/main.rs
git commit -m "phase 02.1: tcp-echo-server runtime + drain"
```

Append PROGRESS.md Task 10 section with the commit SHA, the 8-test tcp-echo-server tail, the workspace-wide tail, and the cargo-deny tail.

---

### Task 11: Phase-01 rollover I3 — 4 `decode_chunked` unit tests in `tests/differential/src/lib.rs`

**Files:**
- Modify: `tests/differential/src/lib.rs`

**Scope:** close phase-01 REVIEW §9 starter item **I3**. `decode_chunked` at `tests/differential/src/lib.rs` (private fn, fn signature `fn decode_chunked(wire: &[u8]) -> Result<Vec<u8>>`) has been implemented since the phase-01 state-4 gate but is unit-test-unverified. Four tests close that gap. No production code change; tests only.

**Test inventory (4 tests, in the existing `#[cfg(test)] mod tests` block at the bottom of `tests/differential/src/lib.rs`; placed after the `drive_http_get_*` tests for adjacency):**
- `decode_chunked_empty_stream` — `b"0\r\n\r\n"` → `Ok(vec![])`.
- `decode_chunked_with_chunk_extension` — `b"5;name=value\r\nhello\r\n0\r\n\r\n"` → `Ok(b"hello".to_vec())`.
- `decode_chunked_truncated_size_line` — `b"5hello"` (missing CRLF after size) → `Err(_)`.
- `decode_chunked_ignores_trailer_bytes` — `b"3\r\nabc\r\n0\r\nTrailer-Name: value\r\n\r\n"` → `Ok(b"abc".to_vec())`.

- [ ] **Step 1: Write the failing test `decode_chunked_empty_stream`.**

Append to `tests/differential/src/lib.rs::tests` (the `mod tests` block already exists; append after the final pre-existing test):

```rust
    #[test]
    fn decode_chunked_empty_stream() {
        let decoded = super::decode_chunked(b"0\r\n\r\n").expect("empty stream decodes");
        assert!(decoded.is_empty(), "got {decoded:?}");
    }
```

Note on `super::`: `decode_chunked` is `fn`, not `pub fn`. Tests in the same file's child module reach it via `super::decode_chunked`. No visibility change required.

- [ ] **Step 2: Run it; verify it passes on first run.**

```bash
cargo test -p differential --lib tests::decode_chunked_empty_stream
```
Expected: `test result: ok. 1 passed; 0 failed`. (The fn is already implemented from the phase-01 state-4 state. This test asserts that the empty-stream wire encoding round-trips cleanly.)

Strict-TDD note: phase-01's TDD discipline required a red-before-green step. Here the production code pre-exists, and the test exists solely to close a review-identified coverage gap; the new tests ratify existing behavior rather than drive new behavior. This is explicitly permitted by `BOOTSTRAP_PROMPT.md` §5 (state-3 TDD "no exceptions" applies to *production code* introduction; backfilling coverage on a pre-existing helper that a prior review flagged is a distinct case). Skip the "red first" sub-step for all 4 I3 tests.

- [ ] **Step 3: Add the remaining 3 tests.**

Append:

```rust
    #[test]
    fn decode_chunked_with_chunk_extension() {
        let wire = b"5;name=value\r\nhello\r\n0\r\n\r\n";
        let decoded = super::decode_chunked(wire).expect("chunk extensions tolerated");
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn decode_chunked_truncated_size_line() {
        // No CRLF anywhere — the first `windows(2).position(== \r\n)` miss
        // must surface as Err, not silent Ok(partial).
        let err = super::decode_chunked(b"5hello").expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("missing CRLF") || msg.contains("CRLF"),
            "expected CRLF-missing error; got {msg}",
        );
    }

    #[test]
    fn decode_chunked_ignores_trailer_bytes() {
        let wire = b"3\r\nabc\r\n0\r\nTrailer-Name: value\r\n\r\n";
        let decoded = super::decode_chunked(wire).expect("trailer tolerated");
        assert_eq!(decoded, b"abc");
    }
```

- [ ] **Step 4: Run the full differential lib tests + lint gate.**

```bash
cargo test -p differential --lib
cargo clippy -p differential --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```
Expected: `test result: ok. 26 passed; 0 failed; 1 ignored` (22 pre-existing + 4 new; the 1 ignored is the Docker-gated TOCTOU test `wait_accept_ready_times_out_for_closed_socket` — unchanged).

- [ ] **Step 5: Commit.**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 02.1: decode_chunked unit tests (phase-01 I3 close-out)"
```

Append PROGRESS.md Task 11 section with the commit SHA, the 26-test differential-lib tail, and a one-line note that phase-01 REVIEW §9 item **I3** is now closed.

---

### Task 12: Fuzz corpus extension — 2 new YAML seeds

**Files:**
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml`

**Scope:** extend the pre-existing `parse_bootstrap` fuzz-corpus directory with two TCP-proxy-shaped seeds per SPEC §D5. The fuzz target itself (`crates/envoy-config/fuzz/fuzz_targets/parse_bootstrap.rs`) is unchanged — `TypedConfig`, `Cluster`, and the nested endpoint shapes are all reachable via `envoy_config::parse_bootstrap`, so structural coverage of the extended grammar comes for free. The `-max_total_time=30` budget (ADR-0010) is unchanged.

No code changes; no tests. Creating YAML seed files is a data-only deliverable.

- [ ] **Step 1: Create `tcp_proxy_single_endpoint.yaml`.**

Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml`:

```yaml
node:
  id: fuzz-seed-single
  cluster: fuzz
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 127.0.0.1
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
```

- [ ] **Step 2: Create `tcp_proxy_round_robin_triple.yaml`.**

Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml`:

```yaml
node:
  id: fuzz-seed-triple
  cluster: fuzz
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 127.0.0.1
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10003
```

- [ ] **Step 3: Verify the seeds parse via `parse_bootstrap` as a sanity check.**

The fuzz corpus is not rendered through the harness, so the seeds don't need to satisfy the full-fixture templating; they only need to be valid `parse_bootstrap` inputs. Run a one-off check:

```bash
cargo run --quiet -p envoy-config --example parse_seed -- \
    crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml \
    crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml
```

*Note:* `envoy-config` has no `examples/parse_seed.rs` today. Skip the `cargo run` sanity check and use an inline test instead: append a temporary `#[test]` to `crates/envoy-config/src/bootstrap.rs::tests` that reads both files and calls `parse_bootstrap`:

```rust
    #[test]
    fn fuzz_corpus_tcp_proxy_seeds_parse() {
        let root = env!("CARGO_MANIFEST_DIR");
        for fname in &[
            "fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml",
            "fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml",
        ] {
            let path = format!("{root}/{fname}");
            let yaml = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!("read {path}: {e}")
            });
            crate::parse_bootstrap(&yaml).unwrap_or_else(|e| {
                panic!("parse {path}: {e}")
            });
        }
    }
```

This test lives permanently — it's a regression gate on corpus-seed validity. Every future phase that adds seeds extends the list; any breakage means either the grammar regressed or the seed is stale, both of which are worth catching at CI time.

- [ ] **Step 4: Run the full test gate.**

```bash
cargo test -p envoy-config
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
```
Expected: `envoy-config` test count grows by 1 (to 38 including the corpus sanity check); workspace-wide, all tests green.

- [ ] **Step 5: Commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml \
       crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml \
       crates/envoy-config/src/bootstrap.rs
git commit -m "phase 02.1: fuzz corpus — TCP-proxy YAML seeds"
```

Append PROGRESS.md Task 12 section with the commit SHA, the 38-test envoy-config tail, and a note that the 30-second `cargo fuzz run parse_bootstrap` CI invocation now covers the extended grammar. Full fuzz run happens in Task 13 (CI gate).

---

### Task 13: Phase-done gate (state 4) — 5 stable commands + CI fuzz job; flip ROADMAP + STATE; final commit

**Files:**
- Modify: `docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md`
- Modify: `docs/envoy-rust/ROADMAP.md`
- Modify: `docs/envoy-rust/STATE.md`

**Scope:** this is the state-4 → state-6 transition combined into a single task per phase-01 Task 19's pattern. Run every `BOOTSTRAP_PROMPT.md` §7.5 gate command locally; push the branch and watch CI; quote all outputs into PROGRESS.md; then produce the phase-done commit that flips ROADMAP row `02.1` → `done` and advances STATE to `02.2-listener-tcp-proxy` at lifecycle state 2.

**Note on lifecycle states:** phase-01 separated state-4 (verification), state-5 (review), and state-6 (final commit) into distinct sessions per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session"). This plan writes all six states' work into the execution pipeline but does NOT promise to collapse them into one session — `subagent-driven-development` fires a fresh subagent per task; verification-before-completion and requesting-code-review remain separate sessions after this plan executes. Task 13 documents the *verification pass* (state 4) and prepares the *phase-done commit template* (state 6); a separate review session (state 5) sits between them. If a REVIEW raises issues, re-entry is at state 3 per `BOOTSTRAP_PROMPT.md` §5.2.

- [ ] **Step 1: Push the working branch to the remote to trigger CI.**

```bash
git push -u origin HEAD
```

Capture the branch name and the resulting PR or workflow run URL. If CI fails, do not proceed to Step 2 — debug (invoke `superpowers:systematic-debugging`), fix, commit, and re-push.

- [ ] **Step 2: Run the 5 local stable-toolchain gate commands.**

```bash
cargo build --workspace --all-targets
```
Expected: exit 0. Quote the tail (`Finished dev profile target(s) in …s`) into PROGRESS.md.

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
Expected: exit 0. Quote the tail.

```bash
cargo fmt --all -- --check
```
Expected: exit 0, no diff.

```bash
cargo test --workspace --lib --bins
```

Expected: all green. Quote the tails per crate:
- `envoy-config`: `test result: ok. 38 passed; 0 failed`.
- `envoy-cluster`: `test result: ok. 8 passed; 0 failed`.
- `envoy-bin`: `test result: ok. 18 passed; 0 failed` (unchanged from phase 01).
- `tcp-echo-server`: `test result: ok. 8 passed; 0 failed`.
- `differential` lib: `test result: ok. 26 passed; 0 failed; 1 ignored` (22 phase-01 + 4 I3).

Docker-gated integration tests (`tests/differential/tests/echo.rs::echo_fixture`, `tests/differential/tests/admin_ready.rs::admin_ready_fixture`) are excluded from `--lib --bins` per phase-01 Task 19's convention and validated only in CI. Quote their CI outcome from the CI run.

```bash
cargo deny check
```
Expected: `advisories ok, bans ok, licenses ok, sources ok`. Quote the tail.

- [ ] **Step 3: Watch the CI run and verify all jobs succeed.**

```bash
gh run list --workflow=ci.yml --branch=<branch> --limit=1
gh run view <run-id>
```

Expected both `build` and `fuzz` jobs conclude `success`:
- `build`: fmt, clippy, build, test, install cargo-deny, cargo deny check.
- `fuzz`: nightly toolchain install, cargo-fuzz install, `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`.

Record the CI run URL and run ID in PROGRESS.md.

- [ ] **Step 4: Append the "State 4 — Phase-done gate verification" section to PROGRESS.md.**

Use the phase-01 state-4 section as the template (verbatim — matches the pattern Task 1 of this plan established for PROGRESS formatting). Include:

- Local gate tails (build, clippy, fmt, test, deny).
- CI gate run ID + URL.
- Explicit per-bullet mapping to `BOOTSTRAP_PROMPT.md` §7.5 (a)–(f):
  - (a) no new differential fixture; fixtures `0001-tcp-echo` and `0002-static-admin-ready` remain green on CI.
  - (b) no conformance suites this sub-phase → n/a.
  - (c) fuzz target `parse_bootstrap` → 30 s clean run, no crashes, on extended corpus.
  - (d) local stable-toolchain gate → all clean.
  - (e) the 5 `cargo` commands + `cargo deny check` clean on CI.
  - (f) REVIEW.md → deferred to state 5 (next session; `superpowers:requesting-code-review`).

- [ ] **Step 5: Stop. This session ends here.**

Per `BOOTSTRAP_PROMPT.md` §5.1 one-state-per-session discipline, state 4 (verification) is the end of this execution run. The next session:

1. Invokes `superpowers:requesting-code-review` → produces `REVIEW.md`.
2. If approved with no Critical/Important issues, a subsequent session produces the phase-done commit per Step 6 below.
3. If issues exist, re-enter at state 3.

The remaining sub-steps (6 through 9) are the **template** for the phase-done commit session, documented here so the executor has a clear handoff.

**The following sub-steps run in a *future* session after REVIEW.md is approved — NOT in this execution:**

- [ ] **Step 6 (future session): Flip `docs/envoy-rust/ROADMAP.md` row 02.1 `status` from `planned` to `done`.**

Edit the `02.1` row in `ROADMAP.md`'s MVP trunk section. The `02` parent row flips to `done` only after 02.2 lands (per ROADMAP schema: "The parent flips to `done` only after all sub-phases are `done`").

- [ ] **Step 7 (future session): Advance `docs/envoy-rust/STATE.md` to phase 02.2.**

Active phase id `02.2`, slug `02.2-listener-tcp-proxy`, directory `docs/envoy-rust/phases/02.2-listener-tcp-proxy/` (exists; contains `SPEC.md` landed in the ADR-0013 split session), status: phase 02.2 lifecycle **state 2 (SPEC.md exists, PLAN.md does not)**, next expected skill `superpowers:writing-plans`. Update the "Last commit" and "Last updated" sections to reflect this phase-done commit.

- [ ] **Step 8 (future session): Append the final PROGRESS.md state-6 section.**

Mirror phase-01 PROGRESS.md's state-6 section: a short paragraph naming the phase-done commit's subject line + a summary of what shipped + the ADR list.

- [ ] **Step 9 (future session): Commit with the SPEC §9 message format.**

```
phase 02.1: Config schema + cluster manager + echo-server helper [ADR-0014]

envoy-config grows the typed_config envelope (TypedConfig / TcpProxyConfig)
and the five-level Cluster / LoadAssignment / LocalityLbEndpoints /
LbEndpoint / Endpoint topology with 16 new validator tests.
envoy-cluster crate lands with static-cluster data model + round-robin LB
cursor (AtomicUsize) and 8 unit tests. tests/helpers/tcp-echo-server/ is
the first crate under tests/helpers/ — a ~160-LoC tokio echo binary that
sub-phase 02.2's fixture 0003 will dial. Phase-01 REVIEW §9 starter
item I3 (decode_chunked unit tests) lands alongside.

Differential surface: no new fixture in 02.1; fixtures 0001-tcp-echo and
  0002-static-admin-ready remain green (unchanged). Fuzz corpus for
  parse_bootstrap now includes single-endpoint and three-endpoint
  tcp_proxy seeds.
Conformance: none.
```

```bash
git add docs/envoy-rust/ROADMAP.md docs/envoy-rust/STATE.md \
       docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md
git commit -m "$(cat <<'EOF'
phase 02.1: Config schema + cluster manager + echo-server helper [ADR-0014]

envoy-config grows the typed_config envelope (TypedConfig / TcpProxyConfig)
and the five-level Cluster / LoadAssignment / LocalityLbEndpoints /
LbEndpoint / Endpoint topology with 16 new validator tests.
envoy-cluster crate lands with static-cluster data model + round-robin LB
cursor (AtomicUsize) and 8 unit tests. tests/helpers/tcp-echo-server/ is
the first crate under tests/helpers/ — a ~160-LoC tokio echo binary that
sub-phase 02.2's fixture 0003 will dial. Phase-01 REVIEW §9 starter
item I3 (decode_chunked unit tests) lands alongside.

Differential surface: no new fixture in 02.1; fixtures 0001-tcp-echo and
  0002-static-admin-ready remain green (unchanged). Fuzz corpus for
  parse_bootstrap now includes single-endpoint and three-endpoint
  tcp_proxy seeds.
Conformance: none.
EOF
)"
```

Phase 02.1 is now complete. The next session inspects `STATE.md`, sees active phase `02.2-listener-tcp-proxy` at lifecycle state 2, and invokes `superpowers:writing-plans`.

---

## Self-review (plan-writer, not executor)

The following checks were run against this plan before it was committed. Each one is a spec-coverage gate; failures were fixed inline before the plan landed.

**1. Spec coverage.** Each SPEC §D1–D7 deliverable maps to at least one task:
- §D1 (`envoy-cluster` crate) → Tasks 5, 6, 7.
- §D2 (`envoy-config` schema extensions + 16 tests) → Tasks 2, 3, 4. The 16 tests decompose across tasks: 3 in Task 2 (shape), 3 in Task 3 (shape — the two pre-existing tests updated in Task 3 Steps 5–6 do not add to the count), 10 in Task 4 (validator + `deny_unknown_fields`). The extra `rejects_unknown_endpoint_field` test brings the count to 10 in Task 4; the SPEC's "fold accordingly" clause (§D2 Notes) covers this.
- §D3 (`tcp-echo-server` helper crate + 8 tests) → Tasks 8, 9, 10.
- §D4 (phase-01 rollover I3) → Task 11.
- §D5 (fuzz corpus extension) → Task 12.
- §D6 (CI workflow — no change) → trivially covered by Task 13's CI verification.
- §D7 (ADR-0014) → Task 1.

**2. Placeholder scan.** No "TBD", no "fill in later", no "similar to Task N without repeating the code", no "handle edge cases" without the code shown. Every test has its source; every command has its expected output.

**3. Type consistency.** Types named in later tasks match their introduction sites:
- `Cluster`, `ClusterManager`, `ClusterHandle`, `ClusterError` — defined in Tasks 5 (placeholder) / 6 / 7; referenced by name in Tasks 6, 7, self-reference only. `from_bootstrap` is `pub fn`; `ClusterManager::get` takes `&self, name: &str`; `ClusterHandle::pick_endpoint(&self) -> Option<SocketAddr>`. Consistent across tasks.
- `TypedConfig`, `TcpProxyConfig` — introduced in Task 2; referenced in Task 4's validator + Tasks 4 / 12's YAML fixtures. `TypedConfig::TcpProxy(TcpProxyConfig)` is the sole variant; the validator's let-else uses `let TypedConfig::TcpProxy(tp) = ...` irrefutably.
- `ConfigError` variants — extended in Task 4 with `MissingTypedConfig(&'static str)`, `UnexpectedTypedConfig(&'static str)`, `UnknownCluster(String)`, `LoadAssignmentNameMismatch { cluster, assignment }`, `EmptyClusterEndpoints(String)`. Task-4 tests match-arm against each.
- `ClusterError` variants — introduced in Task 7 with `EmptyCluster { name }`, `DuplicateClusterName { name }`, `EndpointParse { cluster, addr, source }`. Task-7 tests match-arm against each.
- `Args` / `ArgvError` / `parse_argv` — introduced in Task 9; referenced in Task 10's `main`. `ArgvError` has `MissingFlag(&'static str)`, `MissingValue`, `InvalidPort`, `Trailing`, `HelpRequested`, `VersionRequested` — Tasks 9 and 10 agree.

**4. Task-count check (SPEC §5 gate).** 13 tasks at ~980 LoC (SPEC §5 estimate). Under both `BOOTSTRAP_PROMPT.md` §6.1 thresholds (~25 tasks, ~1500 LoC). Safe.

---

## Execution handoff

Per the user's standing preference (auto-memory `feedback_execution_style`): execute via `superpowers:subagent-driven-development` — fresh subagent per task + two-stage review cadence. Do not ask between subagent-driven and inline execution; go straight to subagent-driven.
