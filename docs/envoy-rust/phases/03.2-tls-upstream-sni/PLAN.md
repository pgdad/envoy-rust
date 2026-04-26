# Phase 03.2 — Upstream TLS Origination + Multi-Cert SNI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/03.2-tls-upstream-sni/SPEC.md`. This plan operationalizes SPEC §§D1–D10. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5 (likely the next-sequential ADR-0020), and continue. The parent phase-03 SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (committed at SHA `a3f3474`) is preserved unedited as a historical artifact; for execution it is superseded by sub-phase SPECs (this one for 03.2; the sibling `03.1-tls-foundation-downstream/SPEC.md` was finalized in phase 03.1).

**Goal:** Layer upstream TLS origination (with wire-level SNI) and downstream multi-cert SNI selection on top of the 03.1 envoy-tls foundation. Ship two new fixtures green against upstream Envoy `v1.33.0`: `0005-tls-upstream` (plaintext downstream → envoy-rust → TLS upstream to a new in-tree `tls-echo-server` helper) and `0006-tls-sni` (TLS downstream with two filter chains routing by ClientHello SNI to two distinct certs). Parent ROADMAP row `03` flips `done` in 03.2's final commit (since row `03.1` is already `done`).

**Architecture:** `envoy-tls` grows a SNI-keyed `SniResolver` (`rustls::server::ResolvesServerCert` impl) and a `DownstreamTls::from_listener(&envoy_config::Listener)` constructor that walks all filter chains and builds a single multi-cert `ServerConfig`. `envoy-tcp::TcpProxy` gains an `Option<Arc<UpstreamTls>>` field (existing constructor `new` unchanged for the plaintext path; new `with_upstream_tls` constructor for the TLS-upstream path); `handle::<S>` branches on `self.upstream_tls.as_ref()` after the upstream `TcpStream::connect`, boxing into a `Box<dyn AsyncReadWrite + Send + Unpin>` so both arms unify into a single bidirectional copy body. `envoy-bin::main::run` walks filter chains: a listener with multiple chains *or* any chain carrying `filter_chain_match.server_names` builds via `from_listener`; a single chain with `transport_socket: Downstream(_)` and no `server_names` keeps the 03.1 `from_context` path; per-cluster `Arc<UpstreamTls>` is constructed via `UpstreamTls::from_context(&ctx)` for clusters carrying `transport_socket: Upstream(_)` and threaded into `TcpProxy::with_upstream_tls`. The differential harness gains `Driver::TlsTcpProbeList { probes: Vec<TlsTcpProbe> }` + multi-probe `drive_tls_probes` (mirrors `drive_tls`'s ADR-0006/0007 discipline once per probe), `TlsEchoBackend` (sibling of `TcpProxyBackend`; spawns the new `tls-echo-server` helper), and `render_yaml` substitution keys for the leaf-B + server PEMs + the TLS_BACKEND_PORT. Fixtures 0005 + 0006 land with their Docker-gated acceptance tests in `tests/differential/tests/`. The new `tests/helpers/tls-echo-server/` helper is a single-cert TLS echo server (mirrors `tcp-echo-server` from 02.1 with rustls termination on top).

**Tech stack:** Rust edition 2024 on pinned stable `1.95.0` (D-3.9). No new direct deps in workspace member crates other than `tls-echo-server`'s introduced dep set (covered by ADR-0019's rustls grant); `envoy-tcp/Cargo.toml` promotes `envoy-tls` from dev-dep to runtime dep. All `tokio-rustls` / `rustls-pemfile` direct adds are within the rustls foundations grant (ADR-0019). All `rcgen` / `tempfile` dev-deps remain dev-test-harness-only (ADR-0018). No new ADRs are anticipated; if `cargo deny check` flips on a new transitive surface or a runtime ambiguity surfaces (TLS protocol-version pin, wildcard SNI semantics), land an ADR per SPEC §7 + D-3.5.

---

## File structure (created / modified)

**Created:**

- `tests/helpers/tls-echo-server/Cargo.toml`
- `tests/helpers/tls-echo-server/src/main.rs` (single-file binary; tests in `#[cfg(test)] mod tests`)
- `crates/envoy-bin/tests/tls_upstream.rs` (Rust-native integration test — backstop for fixture 0005, no Docker)
- `crates/envoy-bin/tests/tls_sni.rs` (Rust-native integration test — backstop for fixture 0006, no Docker)
- `tests/differential/tests/tls_upstream.rs` (Docker-gated acceptance test — fixture 0005)
- `tests/differential/tests/tls_sni.rs` (Docker-gated acceptance test — fixture 0006)
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_multi_cert_sni.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_overlapping_sni_reject.yaml`
- `tests/fixtures/0005-tls-upstream/envoy.yaml`
- `tests/fixtures/0005-tls-upstream/envoy-rust.yaml`
- `tests/fixtures/0005-tls-upstream/inputs/payload.bin`
- `tests/fixtures/0005-tls-upstream/expectations.yaml`
- `tests/fixtures/0005-tls-upstream/README.md`
- `tests/fixtures/0006-tls-sni/envoy.yaml`
- `tests/fixtures/0006-tls-sni/envoy-rust.yaml`
- `tests/fixtures/0006-tls-sni/inputs/payload.bin`
- `tests/fixtures/0006-tls-sni/expectations.yaml`
- `tests/fixtures/0006-tls-sni/README.md`
- `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md` (appended once per task during execution)

**Modified:**

- Root `Cargo.toml` — add `tests/helpers/tls-echo-server` to `[workspace] members`. (`crates/envoy-tls` is already there from 03.1.)
- `crates/envoy-config/src/bootstrap.rs` — add `FilterChainMatch` struct + `server_names: Vec<String>` field; add optional `filter_chain_match: Option<FilterChainMatch>` field on `FilterChain`; extend `validate` with three new arms; append 11 new unit tests (Task 1: 5 parse-shape; Task 2: 6 validator).
- `crates/envoy-config/src/lib.rs` — re-export `FilterChainMatch`; extend `ConfigError` enum with `MultipleListenersWithOverlappingSni { listener: String, sni: String }`, `MultipleCatchAllFilterChains { listener: String }`, `MixedTlsAndPlaintextFilterChainsOnListener { listener: String }` variants.
- `crates/envoy-tls/src/lib.rs` — add `SniResolver` struct + `ResolvesServerCert` impl; add `DownstreamTls::from_listener(&envoy_config::Listener)` constructor; export both from the crate root.
- `crates/envoy-tls/src/tests.rs` — append 5 new tests (`sni_resolver_routes_known_sni`, `sni_resolver_falls_back_to_default_on_miss`, `sni_resolver_returns_none_on_miss_without_default`, `sni_resolver_is_case_insensitive`, `from_listener_builds_multi_cert_config`).
- `crates/envoy-tcp/src/lib.rs` — add `Option<Arc<envoy_tls::UpstreamTls>>` field on `TcpProxy`; add `TcpProxy::with_upstream_tls` constructor; extend `handle::<S>` body to branch the upstream dial via a boxed trait object; add `TcpProxyError::UpstreamTlsHandshake { source }` variant; append 3 new unit tests.
- `crates/envoy-tcp/Cargo.toml` — promote `envoy-tls = { path = "../envoy-tls" }` from dev-dep to runtime dep (the dev-dep was added in 03.1; runtime adoption is the 03.2 promotion).
- `crates/envoy-bin/src/main.rs` — extend the listener-walk pre-pass to dispatch between `DownstreamTls::from_context` (single chain, no `server_names`) and `DownstreamTls::from_listener` (multi-chain or any `server_names`); add a per-cluster `Arc<UpstreamTls>` construction loop; thread `upstream_tls` into `TcpProxy::with_upstream_tls`.
- `tests/differential/src/lib.rs` — add `Driver::TlsTcpProbeList { probes: Vec<TlsTcpProbe> }` variant + `TlsTcpProbe` struct; add `drive_tls_probes` async helper; extend `render_yaml` substitution-key map with `{{LEAF_B_CERT_PATH}}`, `{{LEAF_B_KEY_PATH}}`, `{{SERVER_CERT_PATH}}`, `{{SERVER_KEY_PATH}}`, `{{TLS_BACKEND_PORT}}`; extend `run_fixture` dispatch on `{{TLS_BACKEND_PORT}}` to spawn `TlsEchoBackend` and on `Driver::TlsTcpProbeList` to drive probes.
- `tests/differential/src/tls.rs` — extend `TlsTestPki::envoy_side_paths()` and `subject_side_paths()` with the leaf-B + server PEM keys; expose `pki.server_cert_pem` / `pki.server_key_pem` accessors used by `TlsEchoBackend::spawn`.
- `tests/differential/src/backend.rs` — add `TlsEchoBackend` (sibling of `TcpProxyBackend`); 2 new unit tests.
- `tests/differential/src/upstream.rs` — extend the `with_copy_to(...)` PEM-mount loop to mount any combination of leaf-A, leaf-B, server, CA PEMs that the rendered envoy-side YAML references.
- `crates/envoy-bin/Cargo.toml` — no changes needed (rustls/tokio-rustls/rcgen dev-deps were added in 03.1; `envoy-tls` runtime path-dep is from 03.1).
- `docs/envoy-rust/DECISIONS.md` — no anticipated ADRs (per SPEC §7). If execution surfaces a need (TLS protocol-version pin, wildcard SNI, etc.), land at the next-sequential number (likely ADR-0020).
- `docs/envoy-rust/ROADMAP.md` — at state 6 only, flip row `03.2` `status` → `done` AND parent row `03` `status` → `done` in the same commit (per ROADMAP schema "parent flips to done only after all sub-phases are done"; row `03.1` is already `done` from the 03.1 phase-done commit `64ea760`).
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase id `04`, slug `04-http-1.1` (or whatever phase 04's brainstorm decides; parent SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` and `BOOTSTRAP_PROMPT.md` §8 row 04 say "HTTP connection manager (HTTP/1.1) + route match + router filter + direct_response"), lifecycle state 1 (directory does not yet exist; slug to be picked at state-1 brainstorm), next-skill `superpowers:brainstorming`.
- `Cargo.lock` — sync as a dedicated commit (mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`) once Task 13's gate exposes drift. Expect entries for the new `tls-echo-server v0.0.0` crate and its rustls / tokio-rustls / rustls-pemfile / rustls-pki-types dep tree. Most of the rustls family is already locked from 03.1; only the new bin's package stanza and any version-pin updates from the rustls crate group's normal upstream churn would land here.
- `deny.toml` — only if `cargo deny check` flips on a new transitive surface. Most likely a no-op since the rustls family is already in scope from 03.1.

**Note: not touched in 03.2.** `crates/envoy-cluster/`, `tests/helpers/tcp-echo-server/`, `crates/envoy-listener/`, parent `03-tls-tcp/SPEC.md`, sibling `03.1-tls-foundation-downstream/`, `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `BEHAVIOR_CONTRACT.md` — all unedited (SPEC §8 closing list).

---

## Task index

Each task ends with a commit. Per phase-02.1 / 02.2 / 03.1 convention, follow each task commit with a `phase 03.2: progress note (task N)` commit that appends the matching PROGRESS.md section (commit SHA, change summary, verification output, any deviation). Choose one cadence and keep it.

1. **`envoy-config` — `FilterChainMatch` struct + `server_names` field on `FilterChain` + 5 parse-shape tests**
2. **`envoy-config` — 3 validator variants (overlapping SNI / multi-catch-all / mixed TLS+plaintext) + 6 validator tests + 2 fuzz-corpus seeds**
3. **`envoy-tls` — `SniResolver` + `ResolvesServerCert` impl + 4 SNI-resolver unit tests**
4. **`envoy-tls` — `DownstreamTls::from_listener` constructor + `from_listener_builds_multi_cert_config` integration test**
5. **`envoy-tcp` — `Option<Arc<UpstreamTls>>` field + `with_upstream_tls` ctor + branched dial body + `UpstreamTlsHandshake` error variant + 3 unit tests**
6. **`envoy-bin` — multi-cert listener dispatch + per-cluster `Arc<UpstreamTls>` construction wiring**
7. **`envoy-bin` — in-process integration tests `tls_upstream.rs` + `tls_sni.rs`**
8. **Differential harness — `Driver::TlsTcpProbeList` + `TlsTcpProbe` + `drive_tls_probes` + `render_yaml` extensions + `run_fixture` dispatch**
9. **Differential harness — `TlsEchoBackend` + 2 unit tests + `upstream::start` mount-fan-out extension**
10. **`tests/helpers/tls-echo-server/` helper crate (full impl + 5 unit tests including TLS round-trip)**
11. **Fixture `0005-tls-upstream` (5 files) + Docker-gated `tests/differential/tests/tls_upstream.rs`**
12. **Fixture `0006-tls-sni` (5 files) + Docker-gated `tests/differential/tests/tls_sni.rs`**
13. **State 4 phase-done gate — run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md**

Estimated total: 13 tasks, ~1445 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold comfortably. **Do not split 03.2 further** (per SPEC §5). If any single task balloons past ~10 sub-steps mid-execution, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of an already-split sub-phase deserve a fresh root-cause read.

The opportunistic `Cluster::name()` carryforward (phase-02.2 REVIEW M1 / phase-03.1 REVIEW §4 recommendation 2) is evaluated under Task 5 (envoy-tcp upstream-TLS error attribution) per SPEC §3 D4 + §6 signpost 19. If 03.2 surfaces no use case, it forwards unchanged to phase 06 (default per parent-SPEC §1).

---

### Task 1: `envoy-config` — `FilterChainMatch` struct + `server_names` field on `FilterChain` + 5 parse-shape tests

**Files:**
- Modify: `crates/envoy-config/src/bootstrap.rs` (insert `FilterChainMatch` struct, extend `FilterChain` with `filter_chain_match` field, append 5 unit tests)
- Modify: `crates/envoy-config/src/lib.rs` (re-export `FilterChainMatch`)
- Create: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md` (append a section for this task at end)

**Why first:** every subsequent task either references the new schema type (`SniResolver` keys map to `server_names`; `from_listener` walks `filter_chains` looking for `filter_chain_match.server_names`; envoy-bin dispatches on `server_names` presence) or relies on the validator additions Task 2 builds on top of this. The schema (struct + field) lands first; the validator (cross-chain rules) lands in Task 2.

**Scope:** ~80 LoC impl + ~50 LoC tests. Schema-only — no validator additions in this task; the new field is `Option<FilterChainMatch>` and a missing/empty `filter_chain_match` is the existing default behavior (catch-all single chain). 5 parse-shape tests cover the new shape's serde behavior; no cross-chain rules.

- [ ] **Step 1: Write the failing parse-shape tests in `crates/envoy-config/src/bootstrap.rs::tests`.**

Append to the existing `#[cfg(test)] mod tests { ... }` block in `crates/envoy-config/src/bootstrap.rs`. Use the same `parse_yaml<T: DeserializeOwned>(yaml: &str) -> Result<T, ConfigError>` helper the existing tests use (look for it near the top of `mod tests` — it's already there from phase 02.1+).

```rust
#[test]
fn parses_filter_chain_with_server_names() {
    let yaml = r#"
filter_chain_match:
  server_names: ["a.example.com", "b.example.com"]
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
    let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
    let m = chain.filter_chain_match.expect("has filter_chain_match");
    assert_eq!(m.server_names, vec!["a.example.com".to_string(), "b.example.com".to_string()]);
}

#[test]
fn parses_filter_chain_without_filter_chain_match() {
    // Existing 03.1 / 02.2 shape — no filter_chain_match key.
    let yaml = r#"
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
    let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
    assert!(chain.filter_chain_match.is_none());
}

#[test]
fn parses_filter_chain_match_with_empty_server_names() {
    // `filter_chain_match: {}` is the catch-all shape.
    let yaml = r#"
filter_chain_match: {}
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
    let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
    let m = chain.filter_chain_match.expect("has filter_chain_match");
    assert!(m.server_names.is_empty());
}

#[test]
fn parses_filter_chain_match_with_explicit_empty_server_names_list() {
    let yaml = r#"
filter_chain_match:
  server_names: []
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
    let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
    let m = chain.filter_chain_match.expect("has filter_chain_match");
    assert!(m.server_names.is_empty());
}

#[test]
fn rejects_filter_chain_match_unknown_field() {
    // deny_unknown_fields discipline: an unrecognized key under filter_chain_match fails.
    let yaml = r#"
filter_chain_match:
  destination_port: 443
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
    let err = serde_yaml::from_str::<FilterChain>(yaml).expect_err("must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("destination_port") || msg.contains("unknown field"),
        "expected unknown-field error, got {msg}"
    );
}
```

- [ ] **Step 2: Run the new tests to verify they fail.**

Run: `cargo test -p envoy-config --lib parses_filter_chain_with_server_names parses_filter_chain_without_filter_chain_match parses_filter_chain_match_with_empty_server_names parses_filter_chain_match_with_explicit_empty_server_names_list rejects_filter_chain_match_unknown_field`

Expected: tests fail with errors like `cannot find type "FilterChainMatch" in this scope` or `no field "filter_chain_match" on type "FilterChain"`. The two compile errors identify the exact missing types.

- [ ] **Step 3: Add the `FilterChainMatch` struct in `crates/envoy-config/src/bootstrap.rs`.**

Insert immediately after the existing `FilterChain` struct (around line 119–123 in the current source). Mirror the surrounding crate's style — `Debug, Deserialize, PartialEq` derive set + `#[serde(deny_unknown_fields)]`.

```rust
/// Filter-chain matcher (phase 03.2 portion). Selects which filter chain a
/// connection routes to; for phase 03.2, only `server_names` (TLS SNI) is
/// supported. Empty / missing `server_names` is the catch-all (validator
/// enforces "at most one catch-all per listener").
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChainMatch {
    /// SNI values this filter chain matches. Empty Vec = catch-all. The
    /// validator (Task 2) rejects two filter chains declaring the same SNI
    /// (case-insensitive) and rejects multiple catch-all chains per listener.
    #[serde(default)]
    pub server_names: Vec<String>,
}
```

- [ ] **Step 4: Add the `filter_chain_match` field on `FilterChain` in `crates/envoy-config/src/bootstrap.rs`.**

Modify `FilterChain` (around line 117–123) to add the new optional field. Keep `transport_socket` and `filters` in their existing positions; insert `filter_chain_match` first to mirror Envoy's bootstrap proto field ordering.

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    #[serde(default)]
    pub filter_chain_match: Option<FilterChainMatch>,    // 03.2 NEW
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    pub filters: Vec<NetworkFilter>,
}
```

Also add `PartialEq` to the existing `FilterChain` derive — the previous tests didn't need it but the Task 2 validator tests will. If it's already there, leave as-is. (Cross-check: `grep "pub struct FilterChain" crates/envoy-config/src/bootstrap.rs` and inspect the surrounding `derive` line.)

- [ ] **Step 5: Re-export `FilterChainMatch` from `crates/envoy-config/src/lib.rs`.**

Find the existing block of `pub use bootstrap::{Bootstrap, ..., FilterChain, ...};` (or similar — the exact form varies; from 03.1's lib.rs there's a long `pub use` re-export list). Append `FilterChainMatch` to that list.

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType,
    CommonTlsContext, DataSource, DownstreamTlsContext, Endpoint, FilterChain,
    FilterChainMatch,    // 03.2 NEW
    LbEndpoint, LbPolicy, Listener, LoadAssignment, LocalityLbEndpoints, NetworkFilter,
    Node, SocketAddress, StaticResources, TcpProxyConfig, TlsCertificate, TransportSocket,
    TransportSocketTypedConfig, TypedConfig, UpstreamTlsContext,
};
```

(Preserve existing alphabetical / grouped ordering — the above is illustrative; match the repo's actual convention by reading the current `lib.rs` re-export block first.)

- [ ] **Step 6: Run the new tests to verify they pass.**

Run: `cargo test -p envoy-config --lib parses_filter_chain_with_server_names parses_filter_chain_without_filter_chain_match parses_filter_chain_match_with_empty_server_names parses_filter_chain_match_with_explicit_empty_server_names_list rejects_filter_chain_match_unknown_field`

Expected: all 5 pass. If `rejects_filter_chain_match_unknown_field` doesn't trip, double-check `#[serde(deny_unknown_fields)]` is on the new struct.

- [ ] **Step 7: Run the full envoy-config test suite to verify no regressions.**

Run: `cargo test -p envoy-config`

Expected: 50 (existing from 03.1) + 5 (new Task 1) = 55 tests pass; 0 failed.

- [ ] **Step 8: Run the workspace gate to verify build / clippy / fmt clean.**

Run, in this order:

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: each command exits 0 with no warnings.

- [ ] **Step 9: Commit Task 1.**

```bash
git add crates/envoy-config/src/bootstrap.rs crates/envoy-config/src/lib.rs
git commit -m "$(cat <<'EOF'
phase 03.2: envoy-config — FilterChainMatch struct + server_names + 5 parse-shape tests

Schema-only addition in service of Task 2's cross-chain validator and Task 4's
DownstreamTls::from_listener walker. New FilterChainMatch struct with a single
server_names: Vec<String> field; new optional filter_chain_match field on
FilterChain. deny_unknown_fields preserved on both. 5 new unit tests cover the
present / absent / empty / explicit-empty / unknown-field cases. No validator
or runtime behavior changes — those land in Task 2 + Task 6.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 10: Append the Task 1 PROGRESS.md section.**

Create `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md` if it does not exist; otherwise append to it. Use the section template phase-03.1 PROGRESS.md established (look at any task block in `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md`):

```markdown
## Task 1 — envoy-config: FilterChainMatch struct + server_names + 5 parse-shape tests (YYYY-MM-DD)

- Commit: <SHA from Step 9>
- Change: Inserted `FilterChainMatch` struct (single `server_names: Vec<String>` field, `#[serde(deny_unknown_fields)]`) in `crates/envoy-config/src/bootstrap.rs`. Added `filter_chain_match: Option<FilterChainMatch>` field on `FilterChain`. Re-exported `FilterChainMatch` from `crates/envoy-config/src/lib.rs`. Appended 5 parse-shape tests covering present / missing / empty-map / empty-list / unknown-field cases.
- Verification: `cargo test -p envoy-config --lib` reported 55 passed. Workspace gate clean: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check` all exit 0.
- Deviation from PLAN: <none expected — note any here>.
```

Replace `YYYY-MM-DD` with the actual execution date.

- [ ] **Step 11: Commit the progress note.**

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 1)"
```

---

### Task 2: `envoy-config` — 3 validator variants + 6 validator tests + 2 fuzz-corpus seeds

**Files:**
- Modify: `crates/envoy-config/src/lib.rs` (extend `ConfigError` enum with 3 new variants)
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `validate` with 3 new arms; append 6 validator unit tests)
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_multi_cert_sni.yaml`
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_overlapping_sni_reject.yaml`
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md` (append a section for this task at end)

**Why now:** Task 1's schema additions are inert — the validator additions in this task make the cross-chain rules load-bearing. Task 4's `from_listener` constructor (envoy-tls) trusts these guarantees per SPEC §3 D1: it does not re-check overlapping SNIs, multi-catch-all, or mixed-TLS-plaintext. Both validator and fuzz seeds land in a single Task 2 commit (mirrors phase-03.1 Task 3's combined validator + fuzz cadence).

**Scope:** 3 new `ConfigError` variants (~20 LoC); 3 new `validate` arms (~60 LoC); 6 validator unit tests (~80 LoC); 2 fuzz-corpus seeds (~60 LoC YAML).

**Renaming note (per SPEC §3 D2):** the parent SPEC named the overlapping-SNI variant `MultipleListenersWithOverlappingSni` even though the rule is intra-listener. Sub-phase SPEC §3 D2 explicitly permits renaming to `OverlappingFilterChainSni` if the misnomer becomes confusing during execution. **Plan-writer keeps the parent SPEC's name** (`MultipleListenersWithOverlappingSni`) for verbatim continuity with the SPEC and parent-SPEC §7's projection — and pairs it with a doc-comment that names the intra-listener semantics. If the executor finds the name confusing during code review, an opportunistic rename + cross-reference update is acceptable; the name does not appear in any fixture YAML or wire format, only in `Display` output.

- [ ] **Step 1: Write the failing validator tests in `crates/envoy-config/src/bootstrap.rs::tests`.**

Append after Task 1's tests. Each test follows the existing `validate(&bootstrap)` shape from 03.1's validator tests (look at `rejects_listener_with_unknown_transport_socket_name` or `rejects_cluster_with_downstream_tls_context` for the exact shape).

```rust
#[test]
fn parses_listener_with_multi_chain_sni_routing() {
    // Happy path: two filter chains, each carrying its own DownstreamTlsContext
    // and disjoint server_names. Validator accepts.
    let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("parse");
    bootstrap.validate().expect("validates");
}

#[test]
fn parses_filter_chain_with_empty_server_names_validator() {
    // Single catch-all chain with empty server_names. Accepted by validator.
    let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: [] }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("parse");
    bootstrap.validate().expect("validates");
}

#[test]
fn rejects_filter_chains_with_overlapping_sni() {
    // Two chains both declare server_names: ["a.example.com"].
    let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("parse");
    let err = bootstrap.validate().expect_err("must reject overlapping SNI");
    assert!(matches!(
        err,
        ConfigError::MultipleListenersWithOverlappingSni { ref listener, ref sni }
            if listener == "tcp_listener" && sni == "a.example.com"
    ), "expected MultipleListenersWithOverlappingSni, got {err:?}");
}

#[test]
fn rejects_filter_chains_with_overlapping_sni_case_insensitive() {
    // Chain A "a.example.com"; chain B "A.Example.com". Match is case-insensitive.
    let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["A.Example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("parse");
    let err = bootstrap.validate().expect_err("must reject case-insensitive overlap");
    assert!(matches!(
        err,
        ConfigError::MultipleListenersWithOverlappingSni { ref listener, .. }
            if listener == "tcp_listener"
    ), "expected MultipleListenersWithOverlappingSni, got {err:?}");
}

#[test]
fn rejects_multiple_catch_all_filter_chains() {
    // Two chains both have empty server_names.
    let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: [] }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: {}
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("parse");
    let err = bootstrap.validate().expect_err("must reject multiple catch-all chains");
    assert!(matches!(
        err,
        ConfigError::MultipleCatchAllFilterChains { ref listener }
            if listener == "tcp_listener"
    ), "expected MultipleCatchAllFilterChains, got {err:?}");
}

#[test]
fn rejects_mixed_tls_and_plaintext_filter_chains() {
    // One TLS chain, one plaintext chain on the same listener.
    let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("parse");
    let err = bootstrap.validate().expect_err("must reject mixed TLS and plaintext");
    assert!(matches!(
        err,
        ConfigError::MixedTlsAndPlaintextFilterChainsOnListener { ref listener }
            if listener == "tcp_listener"
    ), "expected MixedTlsAndPlaintextFilterChainsOnListener, got {err:?}");
}
```

- [ ] **Step 2: Run the new tests to verify they fail to compile.**

Run: `cargo test -p envoy-config --lib`

Expected: build error citing `MultipleListenersWithOverlappingSni`, `MultipleCatchAllFilterChains`, `MixedTlsAndPlaintextFilterChainsOnListener` are not variants of `ConfigError`. Two compile errors per missing variant — one for the `expect_err` matcher's variant constructor and one for the test's match-arm.

- [ ] **Step 3: Add the 3 new `ConfigError` variants in `crates/envoy-config/src/lib.rs`.**

Find the `pub enum ConfigError { ... }` block (the existing variants from 03.1 include `UnknownTransportSocketName`, `MismatchedTransportSocketDirection`, `EmptyTlsCertificates`, `MissingValidationContext`, `EmptyUpstreamSni`). Append the three new variants:

```rust
/// Within one listener, two filter chains declared the same SNI value
/// (case-insensitive) in their `filter_chain_match.server_names`. Note: the
/// variant name follows the parent-phase-03 SPEC §7's projection
/// (`MultipleListenersWithOverlappingSni`) — the rule is intra-listener (per
/// listener, not across listeners), but the name is preserved verbatim. The
/// `listener` field names the offending listener; the `sni` field names the
/// duplicated SNI in lowercased canonical form.
#[error("listener {listener:?} has two filter chains with overlapping SNI {sni:?}")]
MultipleListenersWithOverlappingSni { listener: String, sni: String },

/// Within one listener, more than one filter chain has empty
/// `filter_chain_match.server_names` (or no `filter_chain_match`). At most one
/// catch-all chain is allowed per listener.
#[error("listener {listener:?} has more than one catch-all filter chain (empty server_names)")]
MultipleCatchAllFilterChains { listener: String },

/// Within one listener with multiple filter chains, at least one chain
/// carries `transport_socket: TLS` while another does not. Phase-03 does not
/// support mixing TLS and plaintext chains on the same listener (would
/// require `tls_inspector` listener filter, deferred to a later phase).
#[error("listener {listener:?} mixes TLS and plaintext filter chains")]
MixedTlsAndPlaintextFilterChainsOnListener { listener: String },
```

(If the existing `ConfigError` block uses `thiserror::Error` derive — which it does from 03.1 — the `#[error("...")]` lines above are mandatory. Verify the exact attribute syntax by reading the existing variants for two seconds before pasting.)

- [ ] **Step 4: Add the 3 new `validate` arms in `crates/envoy-config/src/bootstrap.rs`.**

Find the existing `impl Bootstrap { pub fn validate(&self) -> Result<(), ConfigError> { ... } }` body. The 03.1 path likely has a per-listener loop already (for the `UnknownTransportSocketName` / `MismatchedTransportSocketDirection` / `EmptyTlsCertificates` checks). Add a per-listener inner block for the new rules. The implementation walks each listener's `filter_chains`:

```rust
// 03.2: cross-chain rules within each listener.
for listener in &self.static_resources.listeners {
    // (existing 03.1 per-chain checks remain; insert these AFTER them, or
    // at the end of the per-listener block.)

    // Rule 1: overlapping SNI. Walk each chain's server_names; build a
    // HashSet<String> of lowercased SNIs; reject on duplicate.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for chain in &listener.filter_chains {
        if let Some(m) = chain.filter_chain_match.as_ref() {
            for sni in &m.server_names {
                let lower = sni.to_lowercase();
                if !seen.insert(lower.clone()) {
                    return Err(ConfigError::MultipleListenersWithOverlappingSni {
                        listener: listener.name.clone(),
                        sni: lower,
                    });
                }
            }
        }
    }

    // Rule 2: at most one catch-all (empty server_names) chain per listener.
    let catch_all_count = listener.filter_chains.iter()
        .filter(|c| {
            c.filter_chain_match.as_ref()
                .map(|m| m.server_names.is_empty())
                .unwrap_or(true)    // missing filter_chain_match == catch-all
        })
        .count();
    if catch_all_count > 1 {
        return Err(ConfigError::MultipleCatchAllFilterChains {
            listener: listener.name.clone(),
        });
    }

    // Rule 3: don't mix TLS and plaintext chains. Only fires when ≥ 2 chains.
    if listener.filter_chains.len() >= 2 {
        let tls_count = listener.filter_chains.iter()
            .filter(|c| c.transport_socket.is_some())
            .count();
        if tls_count > 0 && tls_count < listener.filter_chains.len() {
            return Err(ConfigError::MixedTlsAndPlaintextFilterChainsOnListener {
                listener: listener.name.clone(),
            });
        }
    }
}
```

(The exact insertion point depends on the existing 03.1 validator structure; if 03.1's validator splits across multiple `for listener in ...` loops, fold the new rules into the most appropriate one — single-loop is preferred for cache locality, but if 03.1 already has multiple loops keep the existing shape.)

- [ ] **Step 5: Run the new tests to verify they pass.**

Run: `cargo test -p envoy-config --lib parses_listener_with_multi_chain_sni_routing parses_filter_chain_with_empty_server_names_validator rejects_filter_chains_with_overlapping_sni rejects_filter_chains_with_overlapping_sni_case_insensitive rejects_multiple_catch_all_filter_chains rejects_mixed_tls_and_plaintext_filter_chains`

Expected: 6 passed, 0 failed.

- [ ] **Step 6: Run the full envoy-config test suite to verify no regressions.**

Run: `cargo test -p envoy-config`

Expected: 50 + 5 (Task 1) + 6 (this task) = 61 tests pass; 0 failed.

- [ ] **Step 7: Add fuzz corpus seed `tls_multi_cert_sni.yaml`.**

```bash
mkdir -p crates/envoy-config/fuzz/corpus/parse_bootstrap
```

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_multi_cert_sni.yaml` with verbatim content (mirrors `parses_listener_with_multi_chain_sni_routing` test YAML):

```yaml
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
```

- [ ] **Step 8: Add fuzz corpus seed `tls_overlapping_sni_reject.yaml`.**

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_overlapping_sni_reject.yaml` (mirrors `rejects_filter_chains_with_overlapping_sni` test YAML):

```yaml
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
```

- [ ] **Step 9: Sanity-check fuzz corpus parses (smoke test, not the actual fuzz run).**

Run: `cargo build -p envoy-config-fuzz` (or whatever the fuzz subcrate's name is — verify by `ls crates/envoy-config/fuzz/`; phase-01 set this up).

Expected: builds cleanly. The actual fuzz run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) is a CI-job concern and runs in Task 13's gate.

- [ ] **Step 10: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: each command exits 0 with no warnings.

- [ ] **Step 11: Commit Task 2.**

```bash
git add crates/envoy-config/src/lib.rs crates/envoy-config/src/bootstrap.rs crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_multi_cert_sni.yaml crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_overlapping_sni_reject.yaml
git commit -m "$(cat <<'EOF'
phase 03.2: envoy-config — 3 cross-chain validator rules + 6 tests + 2 fuzz seeds

ConfigError gains MultipleListenersWithOverlappingSni { listener, sni },
MultipleCatchAllFilterChains { listener }, and
MixedTlsAndPlaintextFilterChainsOnListener { listener } variants. validate()
walks each listener's filter chains and enforces: (1) no two chains may
declare the same SNI in server_names (case-insensitive); (2) at most one
chain may have empty/missing filter_chain_match (catch-all); (3) when ≥ 2
chains exist, all must carry transport_socket: TLS or none. The validator
trusts these guarantees in Task 4's envoy-tls::DownstreamTls::from_listener
walk per SPEC §3 D1.

6 new validator unit tests cover happy-path multi-chain SNI routing, single
catch-all, and the 3 reject paths (including a case-insensitive overlap
test). 2 new fuzz seeds extend the parse_bootstrap corpus with the
multi-chain shape (parser accepts; validator may accept or reject — the
fuzz target exercises both paths).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 12: Append the Task 2 PROGRESS.md section.**

```markdown
## Task 2 — envoy-config: 3 validator variants + 6 validator tests + 2 fuzz seeds (YYYY-MM-DD)

- Commit: <SHA from Step 11>
- Change: ConfigError gained 3 new variants. validate() body extended with per-listener cross-chain rules. 6 new validator tests. 2 new fuzz-corpus seeds (`tls_multi_cert_sni.yaml`, `tls_overlapping_sni_reject.yaml`).
- Verification: `cargo test -p envoy-config` reported 61 passed. Workspace gate clean.
- Deviation from PLAN: <none expected — note any here>.
```

- [ ] **Step 13: Commit the progress note.**

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 2)"
```

---

### Task 3: `envoy-tls` — `SniResolver` + `ResolvesServerCert` impl + 4 SNI-resolver unit tests

**Files:**
- Modify: `crates/envoy-tls/src/lib.rs` (add `SniResolver` struct + `ResolvesServerCert` impl; export from crate root)
- Modify: `crates/envoy-tls/src/tests.rs` (append 4 SNI-resolver unit tests)
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Task 4's `DownstreamTls::from_listener` constructor needs `SniResolver` to exist. The 4 unit tests exercise the resolver's contract independently of `from_listener` — covering known-SNI lookup, default fallback, miss-without-default, and case-insensitivity. This task lands the resolver + its tests; the `from_listener` constructor is Task 4.

**Scope:** ~80 LoC impl (struct + impl + helper) + ~60 LoC tests. The `SniResolver` is small: a `HashMap<String, Arc<CertifiedKey>>` with a default fallback. The case-insensitivity contract relies on rustls 0.23's documented behavior that `ClientHello::server_name()` returns lowercased SNI, plus the resolver's own lowercase-key invariant.

- [ ] **Step 1: Write the failing tests in `crates/envoy-tls/src/tests.rs`.**

The existing 03.1 tests use a `mod test_pki { ... }` helper module that exposes `gen_pki()` returning a CA + leaf-A + leaf-B + server cert tree (verified by reading `crates/envoy-tls/src/tests.rs`'s top of file in 03.1). Reuse `gen_pki().leaf_a` / `.leaf_b` for the resolver tests; build `Arc<CertifiedKey>` instances from those rcgen `CertifiedKey` values.

Append after the existing 03.1 tests in `crates/envoy-tls/src/tests.rs`:

```rust
#[test]
fn sni_resolver_routes_known_sni() {
    let pki = test_pki::gen_pki();
    let key_a = std::sync::Arc::new(test_pki::certified_key_from(&pki.leaf_a));
    let key_b = std::sync::Arc::new(test_pki::certified_key_from(&pki.leaf_b));

    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    map.insert("b.example.com".to_string(), key_b.clone());
    let resolver = SniResolver { map, default: None };

    // Use a connection-level integration: spin up a TlsAcceptor with the
    // resolver as ResolvesServerCert and a TlsConnector with SNI 'a.example.com';
    // assert the post-handshake peer cert's SAN contains 'a.example.com'.
    let cert_a = test_pki::peer_cert_from_handshake(
        std::sync::Arc::new(resolver),
        "a.example.com",
        &pki.ca_pem,
    );
    assert!(test_pki::cert_contains_san(&cert_a, "a.example.com"));
}

#[test]
fn sni_resolver_falls_back_to_default_on_miss() {
    let pki = test_pki::gen_pki();
    let key_a = std::sync::Arc::new(test_pki::certified_key_from(&pki.leaf_a));

    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    let resolver = SniResolver { map, default: Some(key_a.clone()) };

    let cert = test_pki::peer_cert_from_handshake(
        std::sync::Arc::new(resolver),
        "unknown.example.com",
        &pki.ca_pem,
    );
    // Default returns key_a, so the unknown-SNI handshake gets cert A.
    assert!(test_pki::cert_contains_san(&cert, "a.example.com"));
}

#[test]
fn sni_resolver_returns_none_on_miss_without_default() {
    let pki = test_pki::gen_pki();
    let key_a = std::sync::Arc::new(test_pki::certified_key_from(&pki.leaf_a));

    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    let resolver = SniResolver { map, default: None };

    // Unknown SNI with no default should fail the handshake.
    let result = test_pki::try_handshake(
        std::sync::Arc::new(resolver),
        "unknown.example.com",
        &pki.ca_pem,
    );
    assert!(result.is_err(), "expected handshake error for unknown SNI without default");
}

#[test]
fn sni_resolver_is_case_insensitive() {
    let pki = test_pki::gen_pki();
    let key_a = std::sync::Arc::new(test_pki::certified_key_from(&pki.leaf_a));

    // Map keyed lowercase per the SniResolver contract.
    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    let resolver = SniResolver { map, default: None };

    // Connect with mixed-case SNI; rustls 0.23's ClientHello::server_name()
    // already returns lowercase, so the resolver's direct lookup matches.
    let cert = test_pki::peer_cert_from_handshake(
        std::sync::Arc::new(resolver),
        "A.Example.com",
        &pki.ca_pem,
    );
    assert!(test_pki::cert_contains_san(&cert, "a.example.com"));
}
```

The above tests assume helpers `peer_cert_from_handshake`, `try_handshake`, `certified_key_from`, `cert_contains_san` exist in the test_pki module. Some of these are already in 03.1's tests.rs (specifically `cert_contains_san` was used by 03.1's `loads_downstream_server_config` test); for the ones that are not, add them in Step 2.

- [ ] **Step 2: Add missing test_pki helpers (only if not already in tests.rs).**

Check `crates/envoy-tls/src/tests.rs` for the helpers above. Add the missing ones inside `mod test_pki { ... }`:

```rust
/// Spin up an in-process TlsAcceptor + TlsConnector pair with the given
/// resolver and SNI; complete a handshake; return the connector-side peer
/// certificate.
pub fn peer_cert_from_handshake(
    resolver: std::sync::Arc<dyn rustls::server::ResolvesServerCert>,
    sni: &str,
    ca_pem_path: &std::path::Path,
) -> rustls_pki_types::CertificateDer<'static> {
    // (Implementation: pair of tokio_rustls::TlsAcceptor + TlsConnector on a
    // localhost loopback. Build ServerConfig from `with_cert_resolver(resolver)`;
    // build ClientConfig from a RootCertStore loaded from ca_pem_path; tokio
    // runtime via Builder::new_current_thread().enable_all().build().unwrap().)
    // ...
    unimplemented!("see implementation in test_pki module body")
}

/// Same shape as peer_cert_from_handshake but returns Result; an unknown-SNI
/// no-default resolver returns Err.
pub fn try_handshake(
    resolver: std::sync::Arc<dyn rustls::server::ResolvesServerCert>,
    sni: &str,
    ca_pem_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    unimplemented!("see implementation in test_pki module body")
}

/// Wrap an rcgen-built leaf cert into a rustls CertifiedKey ready to put in
/// an Arc and stuff in SniResolver's map.
pub fn certified_key_from(leaf: &rcgen::CertifiedKey) -> rustls::sign::CertifiedKey {
    let cert_der = rustls_pki_types::CertificateDer::from(leaf.cert.der().to_vec());
    let key_der = rustls_pki_types::PrivateKeyDer::Pkcs8(
        rustls_pki_types::PrivatePkcs8KeyDer::from(leaf.key_pair.serialize_der())
    );
    let signing_key = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key_der)
        .expect("signing key");
    rustls::sign::CertifiedKey::new(vec![cert_der], signing_key)
}
```

The `peer_cert_from_handshake` body is the load-bearing helper; sketch (full implementation):

```rust
pub fn peer_cert_from_handshake(
    resolver: std::sync::Arc<dyn rustls::server::ResolvesServerCert>,
    sni: &str,
    ca_pem_path: &std::path::Path,
) -> rustls_pki_types::CertificateDer<'static> {
    let _ = crate::install_default_crypto_provider();
    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server_config));

    let mut roots = rustls::RootCertStore::empty();
    let mut pem = std::io::BufReader::new(std::fs::File::open(ca_pem_path).unwrap());
    for cert in rustls_pemfile::certs(&mut pem) {
        roots.add(cert.unwrap()).unwrap();
    }
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_config));

    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = acceptor.accept(stream).await.unwrap();
        });
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let server_name = rustls_pki_types::ServerName::try_from(sni.to_string()).unwrap();
        let tls = connector.connect(server_name, stream).await.unwrap();
        let (_io, conn) = tls.get_ref();
        let cert = conn.peer_certificates().expect("peer certificates").first().unwrap().clone();
        let _ = server.await;
        cert.into_owned()
    })
}
```

For `try_handshake`, return `Err` on the connector's `connect()` failure; otherwise return `Ok(())`.

(Look at 03.1's existing test_pki module — likely some of this scaffolding is already there. Reuse rather than duplicate.)

- [ ] **Step 3: Run the new tests to verify they fail.**

Run: `cargo test -p envoy-tls --lib sni_resolver_routes_known_sni sni_resolver_falls_back_to_default_on_miss sni_resolver_returns_none_on_miss_without_default sni_resolver_is_case_insensitive`

Expected: build error citing `cannot find struct "SniResolver" in this scope`. The error pinpoints exactly what Step 4 needs to land.

- [ ] **Step 4: Add the `SniResolver` struct + `ResolvesServerCert` impl in `crates/envoy-tls/src/lib.rs`.**

Insert after the existing `SingleCertResolver` from 03.1 (around the existing `impl ResolvesServerCert for SingleCertResolver { ... }` block):

```rust
/// SNI-keyed `ResolvesServerCert`. Map keys are lowercase per parent-SPEC §6
/// signpost 21 (rustls 0.23's `ClientHello::server_name()` returns lowercase).
/// The validator (`envoy_config::ConfigError::MultipleListenersWithOverlappingSni`)
/// rejects overlapping SNIs at config-load time, so this resolver assumes
/// well-formed input.
pub struct SniResolver {
    pub map: std::collections::HashMap<String, std::sync::Arc<rustls::sign::CertifiedKey>>,
    /// Catch-all chain's certified key. None when the listener has no
    /// catch-all chain — unknown SNIs then return None and rustls aborts the
    /// handshake with `unrecognized_name`.
    pub default: Option<std::sync::Arc<rustls::sign::CertifiedKey>>,
}

impl std::fmt::Debug for SniResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SniResolver")
            .field("snis", &self.map.keys().collect::<Vec<_>>())
            .field("has_default", &self.default.is_some())
            .finish()
    }
}

impl rustls::server::ResolvesServerCert for SniResolver {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<std::sync::Arc<rustls::sign::CertifiedKey>> {
        let sni = client_hello.server_name()?;
        // rustls 0.23 returns lowercase already; we store lowercase; .get() is direct.
        self.map.get(sni)
            .cloned()
            .or_else(|| self.default.clone())
    }
}
```

Note the `pub` on the struct fields — Task 4's `from_listener` builds the resolver by direct field assignment; an alternative shape (with a `new(map, default) -> Self` constructor) is also acceptable but the `pub` fields keep the test-side construction in Step 1 simple.

- [ ] **Step 5: Run the new tests to verify they pass.**

Run: `cargo test -p envoy-tls --lib sni_resolver_routes_known_sni sni_resolver_falls_back_to_default_on_miss sni_resolver_returns_none_on_miss_without_default sni_resolver_is_case_insensitive`

Expected: 4 passed.

- [ ] **Step 6: Run the full envoy-tls test suite.**

Run: `cargo test -p envoy-tls`

Expected: 10 (existing from 03.1) + 4 (new) = 14 tests pass; 0 failed.

- [ ] **Step 7: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 8: Commit Task 3.**

```bash
git add crates/envoy-tls/src/lib.rs crates/envoy-tls/src/tests.rs
git commit -m "$(cat <<'EOF'
phase 03.2: envoy-tls — SniResolver + ResolvesServerCert impl + 4 unit tests

SniResolver is a HashMap<String, Arc<CertifiedKey>> + optional default
fallback that implements rustls::server::ResolvesServerCert. Map keys are
lowercase (parent-SPEC §6 signpost 21; rustls 0.23's ClientHello::server_name
already returns lowercase). 4 unit tests cover known-SNI lookup, default
fallback on miss, miss-without-default returning None (handshake fails with
TLS unrecognized_name), and case-insensitivity (mixed-case SNI lowercased
by rustls before resolver lookup). Tests use a localhost in-process
TlsAcceptor + TlsConnector pair to drive end-to-end resolution.

DownstreamTls::from_listener (Task 4) consumes this resolver. Existing 03.1
SingleCertResolver remains in service for the single-chain plaintext
DownstreamTls::from_context path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 9: Append the Task 3 PROGRESS.md section + commit progress note.**

```markdown
## Task 3 — envoy-tls: SniResolver + 4 unit tests (YYYY-MM-DD)

- Commit: <SHA>
- Change: Added `SniResolver { map: HashMap<String, Arc<CertifiedKey>>, default: Option<Arc<CertifiedKey>> }` with `ResolvesServerCert` impl returning `map.get(sni).or_else(|| default.clone())`. Added 4 unit tests using a localhost TlsAcceptor + TlsConnector pair.
- Verification: `cargo test -p envoy-tls` reported 14 passed. Workspace gate clean.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 3)"
```

---

### Task 4: `envoy-tls` — `DownstreamTls::from_listener` constructor + integration test

**Files:**
- Modify: `crates/envoy-tls/src/lib.rs` (add `DownstreamTls::from_listener` method)
- Modify: `crates/envoy-tls/src/tests.rs` (append `from_listener_builds_multi_cert_config` integration test)
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Task 3 lands `SniResolver`; this task lands the constructor that builds it from a parsed `envoy_config::Listener`. The integration test is the load-bearing test for this task — it asserts end-to-end that a synthetic Listener with two filter chains produces a multi-cert `ServerConfig` whose SNI dispatch returns the right cert for each SNI.

**Scope:** ~70 LoC impl (constructor body + helper for loading a single chain's CertifiedKey) + ~40 LoC test. The constructor is mostly orchestration: walk filter_chains, for each TLS chain call the existing 03.1 `load_certified_key` helper, populate the SniResolver's map (lowercased keys), build a single ServerConfig with `with_cert_resolver(Arc::new(SniResolver))`. Validator guarantees mean no error handling for overlap / multi-catch-all / mixed-tls-plaintext is needed at this layer.

- [ ] **Step 1: Write the failing integration test in `crates/envoy-tls/src/tests.rs`.**

```rust
#[test]
fn from_listener_builds_multi_cert_config() {
    let pki = test_pki::gen_pki();

    // Synthesize an envoy_config::Listener with two filter chains, each
    // carrying its own DownstreamTlsContext. Use the rcgen-built leaf-A and
    // leaf-B PEMs; write them to TempDir paths.
    let tmpdir = tempfile::tempdir().expect("tempdir");
    let leaf_a_cert = tmpdir.path().join("leaf-a.pem");
    let leaf_a_key = tmpdir.path().join("leaf-a.key");
    let leaf_b_cert = tmpdir.path().join("leaf-b.pem");
    let leaf_b_key = tmpdir.path().join("leaf-b.key");
    std::fs::write(&leaf_a_cert, pki.leaf_a.cert.pem()).unwrap();
    std::fs::write(&leaf_a_key, pki.leaf_a.key_pair.serialize_pem()).unwrap();
    std::fs::write(&leaf_b_cert, pki.leaf_b.cert.pem()).unwrap();
    std::fs::write(&leaf_b_key, pki.leaf_b.key_pair.serialize_pem()).unwrap();

    let yaml = format!(
        r#"
name: tcp_listener
address: {{ socket_address: {{ address: 0.0.0.0, port_value: 10010 }} }}
filter_chains:
  - filter_chain_match: {{ server_names: ["a.example.com"] }}
    transport_socket:
      name: envoy.transport_sockets.tls
      typed_config:
        "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
        common_tls_context:
          tls_certificates:
            - certificate_chain: {{ filename: {leaf_a_cert} }}
              private_key:       {{ filename: {leaf_a_key} }}
    filters:
      - name: envoy.filters.network.tcp_proxy
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
          stat_prefix: ingress_tcp
          cluster: backend
  - filter_chain_match: {{ server_names: ["b.example.com"] }}
    transport_socket:
      name: envoy.transport_sockets.tls
      typed_config:
        "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
        common_tls_context:
          tls_certificates:
            - certificate_chain: {{ filename: {leaf_b_cert} }}
              private_key:       {{ filename: {leaf_b_key} }}
    filters:
      - name: envoy.filters.network.tcp_proxy
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
          stat_prefix: ingress_tcp
          cluster: backend
"#,
        leaf_a_cert = leaf_a_cert.display(),
        leaf_a_key = leaf_a_key.display(),
        leaf_b_cert = leaf_b_cert.display(),
        leaf_b_key = leaf_b_key.display(),
    );
    let listener: envoy_config::Listener = serde_yaml::from_str(&yaml).expect("parse listener");
    let downstream = DownstreamTls::from_listener(&listener).expect("from_listener");

    // Run two handshakes, one per SNI; assert each returns the expected SAN.
    let acceptor = tokio_rustls::TlsAcceptor::from(downstream.config.clone());
    let cert_for_a = test_pki::peer_cert_from_handshake_via_acceptor(acceptor.clone(), "a.example.com", &pki.ca_pem);
    let cert_for_b = test_pki::peer_cert_from_handshake_via_acceptor(acceptor, "b.example.com", &pki.ca_pem);

    assert!(test_pki::cert_contains_san(&cert_for_a, "a.example.com"));
    assert!(test_pki::cert_contains_san(&cert_for_b, "b.example.com"));
}
```

Note: the test relies on a public `config: Arc<ServerConfig>` field on `DownstreamTls` (existed since 03.1) and on a `peer_cert_from_handshake_via_acceptor` helper that takes a pre-built TlsAcceptor (vs. Task 3's `peer_cert_from_handshake` which builds the acceptor internally from a resolver). Add `peer_cert_from_handshake_via_acceptor` to test_pki module if missing — it's a small refactor of the resolver-version body; both can share most code via a `peer_cert_from_handshake_inner(acceptor, sni, ca_pem)` private helper.

- [ ] **Step 2: Run the test to verify it fails.**

Run: `cargo test -p envoy-tls --lib from_listener_builds_multi_cert_config`

Expected: build error citing `no method "from_listener" on type "DownstreamTls"`.

- [ ] **Step 3: Add the `DownstreamTls::from_listener` method in `crates/envoy-tls/src/lib.rs`.**

Insert immediately after the existing `from_context` method (around lines 78–101). The body uses the existing 03.1 helper `load_certified_key(cert_path, key_path)` (verify this is the actual helper name by checking `crates/envoy-tls/src/lib.rs`; if it's named differently, use the actual name).

```rust
impl DownstreamTls {
    /// 03.2-only: build from a full envoy_config::Listener by walking all
    /// filter chains. For each chain that carries a transport_socket with a
    /// DownstreamTlsContext, load its cert+key into a CertifiedKey; for each
    /// SNI in the chain's filter_chain_match.server_names, insert the key into
    /// the SniResolver's map (keyed lowercase). At most one chain may have an
    /// empty server_names (the catch-all); its key becomes `default`.
    /// The validator already rejects overlapping server_names, multiple
    /// catch-all chains, and mixed-TLS-plaintext listeners — from_listener
    /// trusts those guarantees.
    ///
    /// If any chain in the listener carries TLS, the entire listener is
    /// treated as TLS (rustls multiplexes by SNI inside a single ServerConfig).
    pub fn from_listener(
        listener: &envoy_config::Listener,
    ) -> Result<Self, TlsError> {
        let mut map: std::collections::HashMap<String, std::sync::Arc<rustls::sign::CertifiedKey>>
            = std::collections::HashMap::new();
        let mut default: Option<std::sync::Arc<rustls::sign::CertifiedKey>> = None;

        for chain in &listener.filter_chains {
            let Some(socket) = &chain.transport_socket else { continue };
            // The validator already enforced that `name` matches and direction
            // is downstream; only TLS chains have transport_socket here.
            let envoy_config::TransportSocketTypedConfig::Downstream(ctx) = &socket.typed_config
            else {
                continue; // unreachable for valid listeners; defensive on the type
            };

            let cert = ctx.common_tls_context.tls_certificates.first()
                .ok_or(TlsError::EmptyTlsCertificates)?;
            let certified_key = std::sync::Arc::new(load_certified_key(
                cert.certificate_chain.as_path(),
                cert.private_key.as_path(),
            )?);

            let server_names = chain.filter_chain_match.as_ref()
                .map(|m| m.server_names.as_slice())
                .unwrap_or(&[]);

            if server_names.is_empty() {
                // Catch-all chain. Validator ensured at most one.
                default = Some(certified_key);
            } else {
                for sni in server_names {
                    map.insert(sni.to_lowercase(), certified_key.clone());
                }
            }
        }

        let resolver = SniResolver { map, default };

        let provider = std::sync::Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(rustls::DEFAULT_VERSIONS)
            .map_err(|e| TlsError::ServerConfigBuild { source: e.to_string() })?
            .with_no_client_auth()
            .with_cert_resolver(std::sync::Arc::new(resolver));

        Ok(Self { config: std::sync::Arc::new(config) })
    }
}
```

(Verify the exact `load_certified_key` signature and `TlsError` variant names by reading `crates/envoy-tls/src/lib.rs`. The above is illustrative — match the existing 03.1 shape exactly. The `TlsError::ServerConfigBuild` variant may not exist — if so, use whatever 03.1's `from_context` uses for the same `with_protocol_versions(...)?` step; the actual variant name from 03.1 PROGRESS.md context was likely `TlsError::ServerConfigBuild` but verify before committing.)

Also, `cert.certificate_chain.as_path()` assumes `DataSource` exposes an `as_path() -> &Path` method or that `certificate_chain` is a `PathBuf`-like field directly. Verify by re-reading the `DataSource` shape from 03.1; if it's an enum with a `Filename(PathBuf)` variant, the call site needs a `match`.

- [ ] **Step 4: Run the test to verify it passes.**

Run: `cargo test -p envoy-tls --lib from_listener_builds_multi_cert_config`

Expected: passes. The two handshakes (one per SNI) each produce the right SAN; the assertions hold.

- [ ] **Step 5: Run the full envoy-tls test suite.**

Run: `cargo test -p envoy-tls`

Expected: 14 (after Task 3) + 1 (this task) = 15 tests pass.

- [ ] **Step 6: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 7: Commit Task 4.**

```bash
git add crates/envoy-tls/src/lib.rs crates/envoy-tls/src/tests.rs
git commit -m "$(cat <<'EOF'
phase 03.2: envoy-tls — DownstreamTls::from_listener + multi-cert integration test

DownstreamTls::from_listener walks listener.filter_chains, loads each TLS
chain's certificate+key via the existing load_certified_key helper, and
populates a SniResolver's map (lowercased server_names) plus an optional
default fallback (for the catch-all chain, if any). Builds a single
ServerConfig with with_cert_resolver(Arc<SniResolver>); the resolver
multiplexes by ClientHello SNI inside the single config. Validator
guarantees (overlap reject, single-catch-all, no mixed TLS+plaintext) mean
the constructor takes the happy path through its inputs.

Integration test from_listener_builds_multi_cert_config synthesizes an
envoy_config::Listener with two filter chains (a.example.com → leaf-A;
b.example.com → leaf-B), feeds it through from_listener, runs two
in-process handshakes (one per SNI), and asserts each handshake's
post-handshake peer cert SAN matches the expected value.

The 03.1 single-cert DownstreamTls::from_context is unchanged and remains
the entry point for single-chain listeners with no server_names (existing
fixture 0004 path).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 8: Append the Task 4 PROGRESS.md section + commit progress note.**

```markdown
## Task 4 — envoy-tls: DownstreamTls::from_listener + 1 integration test (YYYY-MM-DD)

- Commit: <SHA>
- Change: Added `DownstreamTls::from_listener(&envoy_config::Listener) -> Result<Self, TlsError>`. Walks filter_chains, loads each TLS chain's cert+key via load_certified_key, populates SniResolver map (lowercased SNIs) and default (catch-all chain, if any). Builds ServerConfig with the resolver. Added `from_listener_builds_multi_cert_config` integration test (synthesizes a Listener with two TLS chains, drives two SNI-keyed handshakes, asserts each post-handshake peer cert's SAN).
- Verification: `cargo test -p envoy-tls` reported 15 passed. Workspace gate clean.
- Deviation from PLAN: <none expected — note any here, e.g., if `cert.certificate_chain` shape differed from PLAN's assumption>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 4)"
```

---

### Task 5: `envoy-tcp` — `Option<Arc<UpstreamTls>>` field + `with_upstream_tls` ctor + branched dial body + `UpstreamTlsHandshake` error variant + 3 unit tests

**Files:**
- Modify: `crates/envoy-tcp/Cargo.toml` (promote `envoy-tls` from dev-dep to runtime dep)
- Modify: `crates/envoy-tcp/src/lib.rs` (add field, new ctor, branched dial, new error variant, 3 unit tests)
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Tasks 3 + 4 land the multi-cert downstream surface; Task 5 lands the upstream-TLS *consumer* wiring. envoy-tcp is the consumer of `envoy-tls::UpstreamTls` (the library API for upstream-TLS that landed in 03.1 with no consumer). Task 6 (envoy-bin) builds the `Arc<UpstreamTls>` per cluster and threads it through the new `with_upstream_tls` ctor; Task 5 lands the ctor itself plus the branched handle body.

**Scope:** ~100 LoC impl + ~80 LoC tests. The branched dial uses the `Box<dyn AsyncReadWrite + Send + Unpin>` shape from SPEC §3 D3: a local trait alias `AsyncReadWrite: AsyncRead + AsyncWrite` (auto-impl'd for any `T: AsyncRead + AsyncWrite`), then `let upstream: Box<dyn AsyncReadWrite + Send + Unpin> = match &self.upstream_tls { None => Box::new(stream), Some(tls) => Box::new(tls.connect(stream).await?) };`. The two arms unify into a single `tokio::io::copy` body downstream of the box, preserving the 03.1 ADR-0016 `tokio::select!`-over-two-copies posture.

**M1 carryforward (`Cluster::name()` accessor) evaluation point.** Step 7 of this task is the dedicated evaluation. If the `UpstreamTlsHandshake` error variant benefits from `cluster: String` attribution (so the `warn!` log line on a per-connection failure names the offending cluster, not just the TLS error), close phase-02.1 REVIEW M1 by adding `pub(crate) fn name(&self) -> &str` on `envoy_cluster::Cluster` and use it here. Otherwise leave M1 deferred and document the decision in PROGRESS.md.

- [ ] **Step 1: Promote `envoy-tls` from dev-dep to runtime dep in `crates/envoy-tcp/Cargo.toml`.**

The current 03.1 `Cargo.toml` has `envoy-tls = { path = "../envoy-tls" }` somewhere; verify whether under `[dependencies]` or `[dev-dependencies]`. If under `[dev-dependencies]` (per SPEC §3 D3 plan-time expectation), move it to `[dependencies]`. If already under `[dependencies]` (drift from SPEC), the move is a no-op and the PLAN deviation should be noted in PROGRESS.md.

After the change, `[dependencies]` should include:

```toml
[dependencies]
envoy-cluster = { path = "../envoy-cluster" }
envoy-config = { path = "../envoy-config" }
envoy-listener = { path = "../envoy-listener" }
envoy-tls = { path = "../envoy-tls" }       # 03.2 NEW (or promoted from dev-dep)
thiserror = "2"
tokio = { version = "1", features = ["rt", "net", "io-util", "macros"] }
tracing = "0.1"
```

The dev-deps from 03.1 (rcgen, rustls, rustls-pemfile, rustls-pki-types, tokio-rustls) all stay under `[dev-dependencies]` — the unit tests still use them directly to set up the in-process TLS acceptor for the round-trip tests.

Run `cargo build -p envoy-tcp` to verify the promotion compiles.

- [ ] **Step 2: Write the 3 failing unit tests in `crates/envoy-tcp/src/lib.rs::tests`.**

Append to the existing `#[cfg(test)] mod tests { ... }` block. The tests use the existing 03.1 `mod test_pki { ... }` helper or rebuild a small CA + leaf via `rcgen` inline. Ensure the test names match SPEC §3 D3:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn proxies_to_tls_upstream_with_valid_cert() {
    let _ = envoy_tls::install_default_crypto_provider();

    // Build a CA + leaf with SAN "envoy-rust.test" via rcgen.
    let pki = build_test_pki();
    let (acceptor, server_addr) = spawn_in_process_tls_echo_server(&pki).await;

    // Build envoy-config UpstreamTlsContext shape with sni="envoy-rust.test"
    // and validation_context.trusted_ca pointing at the CA PEM.
    let upstream_ctx = envoy_config::UpstreamTlsContext {
        common_tls_context: envoy_config::CommonTlsContext {
            tls_certificates: vec![],
            validation_context: Some(envoy_config::CertificateValidationContext {
                trusted_ca: envoy_config::DataSource::Filename(pki.ca_pem_path.clone()),
            }),
        },
        sni: "envoy-rust.test".to_string(),
    };
    let upstream_tls = std::sync::Arc::new(
        envoy_tls::UpstreamTls::from_context(&upstream_ctx).expect("upstream tls")
    );

    // Build a TcpProxy pointing at the in-process TLS acceptor.
    let cluster = mk_handle_for_addr(server_addr);
    let cfg = envoy_config::TcpProxyConfig {
        stat_prefix: "ingress_tcp".to_string(),
        cluster: "backend".to_string(),
    };
    let proxy = std::sync::Arc::new(TcpProxy::with_upstream_tls(cluster, &cfg, upstream_tls));

    // Connect a downstream plaintext stream; write payload; assert byte-exact echo.
    let downstream = build_loopback_pair_and_drive_payload(proxy, b"hello, tls upstream\n").await;
    assert_eq!(downstream, b"hello, tls upstream\n");

    drop(acceptor);
}

#[tokio::test(flavor = "multi_thread")]
async fn proxies_returns_err_on_upstream_tls_handshake_fail() {
    let _ = envoy_tls::install_default_crypto_provider();

    // Build a CA1 + leaf signed by CA1; configure the proxy with a different
    // CA2 trust bundle so the leaf doesn't verify.
    let pki1 = build_test_pki();
    let pki2 = build_test_pki(); // independent CA — must reject pki1's leaf.
    let (_acceptor, server_addr) = spawn_in_process_tls_echo_server(&pki1).await;

    let upstream_ctx = envoy_config::UpstreamTlsContext {
        common_tls_context: envoy_config::CommonTlsContext {
            tls_certificates: vec![],
            validation_context: Some(envoy_config::CertificateValidationContext {
                trusted_ca: envoy_config::DataSource::Filename(pki2.ca_pem_path.clone()),  // mismatch
            }),
        },
        sni: "envoy-rust.test".to_string(),
    };
    let upstream_tls = std::sync::Arc::new(
        envoy_tls::UpstreamTls::from_context(&upstream_ctx).expect("upstream tls")
    );

    let cluster = mk_handle_for_addr(server_addr);
    let cfg = envoy_config::TcpProxyConfig {
        stat_prefix: "ingress_tcp".to_string(),
        cluster: "backend".to_string(),
    };
    let proxy = std::sync::Arc::new(TcpProxy::with_upstream_tls(cluster, &cfg, upstream_tls));

    // Drive the downstream and expect a TcpProxyError::UpstreamTlsHandshake.
    let err = drive_and_capture_error(proxy, b"will-fail-handshake").await;
    let formatted = format!("{err}");
    assert!(
        formatted.contains("UpstreamTlsHandshake")
            || formatted.contains("upstream TLS handshake")
            || formatted.contains("certificate"),
        "expected UpstreamTlsHandshake-shaped error, got {formatted}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn proxies_to_tls_upstream_sends_sni_in_client_hello() {
    let _ = envoy_tls::install_default_crypto_provider();

    let pki = build_test_pki();

    // Spawn an in-process TLS acceptor whose ResolvesServerCert captures the
    // ClientHello SNI into a Mutex<Option<String>>.
    let captured: std::sync::Arc<tokio::sync::Mutex<Option<String>>> = std::sync::Arc::new(tokio::sync::Mutex::new(None));
    let (server_addr, _acceptor_handle) = spawn_sni_capturing_acceptor(&pki, captured.clone()).await;

    let upstream_ctx = envoy_config::UpstreamTlsContext {
        common_tls_context: envoy_config::CommonTlsContext {
            tls_certificates: vec![],
            validation_context: Some(envoy_config::CertificateValidationContext {
                trusted_ca: envoy_config::DataSource::Filename(pki.ca_pem_path.clone()),
            }),
        },
        sni: "envoy-rust.test".to_string(),
    };
    let upstream_tls = std::sync::Arc::new(
        envoy_tls::UpstreamTls::from_context(&upstream_ctx).expect("upstream tls")
    );

    let cluster = mk_handle_for_addr(server_addr);
    let cfg = envoy_config::TcpProxyConfig {
        stat_prefix: "ingress_tcp".to_string(),
        cluster: "backend".to_string(),
    };
    let proxy = std::sync::Arc::new(TcpProxy::with_upstream_tls(cluster, &cfg, upstream_tls));

    let _ = build_loopback_pair_and_drive_payload(proxy, b"sni-probe").await;

    let captured_sni = captured.lock().await.clone();
    assert_eq!(
        captured_sni.as_deref(),
        Some("envoy-rust.test"),
        "expected SNI 'envoy-rust.test' in ClientHello, got {captured_sni:?}",
    );
}
```

The above tests rely on these helpers (some new, some likely already in 03.1's tests):

- `build_test_pki()` — returns a struct with `ca_pem_path: PathBuf`, `leaf_cert_pem: PathBuf`, `leaf_key_pem: PathBuf`, `_tmpdir: TempDir` (alive-keeper).
- `spawn_in_process_tls_echo_server(pki) -> (TlsAcceptor, SocketAddr)` — binds 127.0.0.1:0, accepts one connection, runs `tokio::io::copy`-loopback over a `TlsStream<TcpStream>`.
- `spawn_sni_capturing_acceptor(pki, mutex) -> (SocketAddr, JoinHandle)` — same, but the acceptor uses a `ResolvesServerCert` impl whose `resolve()` writes `client_hello.server_name()` into the mutex before returning the cert.
- `build_loopback_pair_and_drive_payload(proxy, payload) -> Vec<u8>` — opens a downstream `TcpStream` pair, hands one half to `proxy.handle::<TcpStream>(...)`, writes `payload` from the other half, reads back; returns the bytes read.
- `drive_and_capture_error(proxy, payload) -> Box<dyn Error>` — same but expects `proxy.handle` to return Err and returns the error.
- `mk_handle_for_addr(addr) -> envoy_cluster::ClusterHandle` — already in 03.1's tests; trivially wraps a single-endpoint cluster.

Add the missing helpers inside `mod test_pki { ... }` or a new `mod test_helpers { ... }`. The 03.1 tests have `mk_handle_for_addr` and a basic in-process echo server — extend with the SNI-capturing variant.

- [ ] **Step 3: Run the new tests to verify they fail.**

Run: `cargo test -p envoy-tcp --lib proxies_to_tls_upstream_with_valid_cert proxies_returns_err_on_upstream_tls_handshake_fail proxies_to_tls_upstream_sends_sni_in_client_hello`

Expected: build error citing `no method "with_upstream_tls" on type "TcpProxy"` and `unknown variant "UpstreamTlsHandshake" on enum "TcpProxyError"`.

- [ ] **Step 4: Add the field, ctor, error variant, and trait alias in `crates/envoy-tcp/src/lib.rs`.**

Modify the existing `TcpProxy` struct (around lines 17–20) and `impl TcpProxy { ... }` block:

```rust
/// Local trait alias unifying tokio's `AsyncRead` + `AsyncWrite`. Auto-impl'd
/// for any `T: AsyncRead + AsyncWrite`. Used internally to box the upstream
/// stream in the branched-dial path; both `TcpStream` (plaintext) and
/// `TlsStream<TcpStream>` (TLS-upstream) impl this.
trait AsyncReadWrite: tokio::io::AsyncRead + tokio::io::AsyncWrite {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite> AsyncReadWrite for T {}

pub struct TcpProxy {
    cluster: envoy_cluster::ClusterHandle,
    cluster_name: String,
    upstream_tls: Option<std::sync::Arc<envoy_tls::UpstreamTls>>,    // 03.2 NEW
}

impl TcpProxy {
    /// Existing 03.1 plaintext-upstream constructor. Unchanged surface; the
    /// new field defaults to None.
    pub fn new(cluster: envoy_cluster::ClusterHandle, cfg: &envoy_config::TcpProxyConfig) -> Self {
        Self {
            cluster,
            cluster_name: cfg.cluster.clone(),
            upstream_tls: None,
        }
    }

    /// 03.2 NEW: TLS-upstream constructor. The provided `Arc<UpstreamTls>`
    /// is shared across all per-connection invocations of `handle`; rustls's
    /// `ClientConfig` is `Send + Sync` and re-used.
    pub fn with_upstream_tls(
        cluster: envoy_cluster::ClusterHandle,
        cfg: &envoy_config::TcpProxyConfig,
        upstream_tls: std::sync::Arc<envoy_tls::UpstreamTls>,
    ) -> Self {
        Self {
            cluster,
            cluster_name: cfg.cluster.clone(),
            upstream_tls: Some(upstream_tls),
        }
    }

    pub async fn handle<S>(
        &self,
        downstream: S,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let pick = self.cluster.pick_endpoint();
        let cluster_name = self.cluster_name.clone();
        let addr = pick.ok_or_else(|| {
            Box::new(TcpProxyError::NoHealthyEndpoint {
                cluster: cluster_name.clone(),
            }) as Box<dyn std::error::Error + Send + Sync>
        })?;

        let stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|source| {
                Box::new(TcpProxyError::UpstreamConnect { addr, source })
                    as Box<dyn std::error::Error + Send + Sync>
            })?;

        // 03.2 branched dial: TLS or plaintext upstream.
        let upstream: Box<dyn AsyncReadWrite + Send + Unpin> = match &self.upstream_tls {
            None => Box::new(stream),
            Some(tls) => {
                let tls_stream = tls.connect(stream).await.map_err(|source| {
                    Box::new(TcpProxyError::UpstreamTlsHandshake { source })
                        as Box<dyn std::error::Error + Send + Sync>
                })?;
                Box::new(tls_stream)
            }
        };

        // ADR-0016 half-close posture, identical to 03.1.
        let (mut dr, mut dw) = tokio::io::split(downstream);
        let (mut ur, mut uw) = tokio::io::split(upstream);
        let result: Result<(), std::io::Error> = tokio::select! {
            res = tokio::io::copy(&mut dr, &mut uw) => res.map(|_| ()),
            res = tokio::io::copy(&mut ur, &mut dw) => res.map(|_| ()),
        };
        drop((dr, dw, ur, uw));
        result.map_err(|source| {
            Box::new(TcpProxyError::CopyFailed { source })
                as Box<dyn std::error::Error + Send + Sync>
        })?;

        tracing::debug!(%addr, cluster = %cluster_name, "tcp proxy connection complete");
        Ok(())
    }
}
```

And the `TcpProxyError` enum extension:

```rust
#[derive(Debug, thiserror::Error)]
pub enum TcpProxyError {
    #[error("no healthy endpoint available for cluster '{cluster}'")]
    NoHealthyEndpoint { cluster: String },
    #[error("connecting to upstream {addr}: {source}")]
    UpstreamConnect {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },
    /// 03.2 NEW: upstream TLS handshake failed. Wraps envoy_tls::TlsError.
    #[error("upstream TLS handshake failed: {source}")]
    UpstreamTlsHandshake {
        #[source]
        source: envoy_tls::TlsError,
    },
    #[error("bidirectional copy failed: {source}")]
    CopyFailed {
        #[source]
        source: std::io::Error,
    },
}
```

Note the 03.1 `ConnectionHandler::handle` impl (the `Box::pin(...)` wrapper that defers to the inherent generic method) needs minor adjustment too — the wrapper currently reconstructs a fresh `TcpProxy { cluster, cluster_name }`. Update to also clone and pass `upstream_tls`:

```rust
impl ConnectionHandler for TcpProxy {
    fn handle(
        &self,
        downstream: tokio::net::TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let cluster = self.cluster.clone();
        let cluster_name = self.cluster_name.clone();
        let upstream_tls = self.upstream_tls.clone();    // 03.2 NEW
        Box::pin(async move {
            let proxy = TcpProxy {
                cluster,
                cluster_name,
                upstream_tls,
            };
            proxy.handle::<tokio::net::TcpStream>(downstream).await
        })
    }
}
```

- [ ] **Step 5: Run the new tests to verify they pass.**

Run: `cargo test -p envoy-tcp --lib proxies_to_tls_upstream_with_valid_cert proxies_returns_err_on_upstream_tls_handshake_fail proxies_to_tls_upstream_sends_sni_in_client_hello`

Expected: 3 passed.

- [ ] **Step 6: Run the full envoy-tcp test suite to verify no regressions.**

Run: `cargo test -p envoy-tcp`

Expected: 8 (existing 02.2 + 03.1 = 4 + 4) + 3 (new) = 11 tests pass.

- [ ] **Step 7: Evaluate the M1 carryforward (`Cluster::name()` accessor).**

Per SPEC §3 D4 + §6 signpost 19, the `Cluster::name()` accessor opportunistic close-out is evaluated here. Inspect the new `TcpProxyError::UpstreamTlsHandshake { source }` variant: does it benefit from a `cluster: String` attribution field?

**Recommended decision (default):** **Do not close M1 in 03.2.** The bare `source` variant is consistent with 03.1's `UpstreamConnect { addr, source }` and `CopyFailed { source }` shapes — the error chain through `tracing::warn!` already includes per-listener context (cluster name accessible via `self.cluster_name` at the warn site, not via the error variant). M1 forwards unchanged to phase 06 (the stats phase, which has a stronger use case for cluster-name-in-errors).

**Alternative decision (close M1 opportunistically):** if the executor judges that the `UpstreamTlsHandshake` variant is materially more useful with `cluster: String`, then:

1. Add `pub(crate) fn name(&self) -> &str` on `envoy_cluster::Cluster`.
2. Remove the field-level `#[allow(dead_code)]` per the 02.1 REVIEW M1 guidance.
3. Extend `UpstreamTlsHandshake` to `{ cluster: String, source: envoy_tls::TlsError }` and update the call site.
4. Update PROGRESS.md + the 03.2 REVIEW §3 to record the closure with cross-references to phase-02.1 REVIEW M1 + phase-02.2 REVIEW §4 recommendation 1 + phase-03.1 REVIEW §4 recommendation 2.

Either way, **document the decision explicitly in PROGRESS.md for this task** (closed-in-03.2 or remained-deferred). No code changes if the default decision is taken.

- [ ] **Step 8: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 9: Commit Task 5.**

```bash
git add crates/envoy-tcp/Cargo.toml crates/envoy-tcp/src/lib.rs
git commit -m "$(cat <<'EOF'
phase 03.2: envoy-tcp — upstream-TLS dial + UpstreamTlsHandshake error + 3 tests

TcpProxy gains an Option<Arc<UpstreamTls>> field. New ctor with_upstream_tls
takes the cluster handle, TcpProxyConfig, and Arc<UpstreamTls>; existing new()
unchanged for the plaintext path. The handle::<S> body branches after the
upstream TcpStream::connect: when upstream_tls is Some, awaits
UpstreamTls::connect to wrap the stream into a TlsStream<TcpStream>, then
boxes both arms as Box<dyn AsyncReadWrite + Send + Unpin> for a unified
bidirectional copy. ADR-0016 half-close posture preserved (tokio::select! over
two tokio::io::copy futures). New TcpProxyError::UpstreamTlsHandshake { source:
envoy_tls::TlsError } variant; per-connection handshake failures log at warn!
and drop, listener stays up.

3 new unit tests: byte-exact round-trip via TLS upstream, handshake-fail-on-
mismatched-CA, and ClientHello SNI capture (proves wire-level SNI from
UpstreamTlsContext.sni reaches the upstream's ResolvesServerCert).

envoy-tls promoted from dev-dep to runtime dep on envoy-tcp.

M1 carryforward (Cluster::name accessor): evaluated; <closed in 03.2|deferred
to phase 06 — see PROGRESS.md>.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 10: Append the Task 5 PROGRESS.md section + commit progress note.**

```markdown
## Task 5 — envoy-tcp: upstream-TLS dial + 3 unit tests + M1 evaluation (YYYY-MM-DD)

- Commit: <SHA>
- Change: TcpProxy gained Option<Arc<UpstreamTls>> field + with_upstream_tls ctor + branched dial via Box<dyn AsyncReadWrite + Send + Unpin>. Promoted envoy-tls from dev-dep to runtime dep. Added TcpProxyError::UpstreamTlsHandshake variant. 3 new unit tests.
- M1 carryforward decision: <CLOSED — Cluster::name() landed; UpstreamTlsHandshake gained cluster: String field. | DEFERRED — no use case surfaced; carries forward to phase 06>.
- Verification: `cargo test -p envoy-tcp` reported 11 passed. Workspace gate clean.
- Deviation from PLAN: <none expected — note any here>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 5)"
```

---

### Task 6: `envoy-bin` — multi-cert listener dispatch + per-cluster `Arc<UpstreamTls>` construction wiring

**Files:**
- Modify: `crates/envoy-bin/src/main.rs` (extend listener-walk + add per-cluster UpstreamTls loop + thread upstream_tls into TcpProxy ctor)
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Task 4's `from_listener` and Task 5's `with_upstream_tls` both have library APIs but no production caller until envoy-bin's `run` is extended. This task lands the wiring; Task 7 lands the in-process integration tests that exercise the wiring end-to-end.

**Scope:** ~80 LoC. Pure orchestration — no new types. The dispatch logic lives entirely inside `run`'s existing per-listener loop; the per-cluster loop is new but small.

- [ ] **Step 1: Read the current `run` body to understand the 03.1 wiring.**

Skim `crates/envoy-bin/src/main.rs::run` (or wherever the listener-construction lives — likely `main.rs` directly per 03.1 PROGRESS Task 9). Note:

- The 03.1 listener walk: for each listener's first filter chain, check `transport_socket.is_some()`; if so, build `Arc<DownstreamTls>` via `from_context(&ctx)` and wrap in `TlsAcceptingHandler`.
- The 03.1 cluster construction: per SPEC `ClusterManager::new()` from the bootstrap once at startup; `TcpProxy::new(cluster_handle, &cfg)` per filter (single filter per listener in 03.1).

- [ ] **Step 2: Extend the listener-walk to dispatch single-cert vs. multi-cert.**

Replace the 03.1 dispatch (single-cert only) with a three-way:

```rust
// Inside run() — per-listener loop.
for listener in &bootstrap.static_resources.listeners {
    // Determine whether this listener needs TLS termination, and which constructor.
    let any_chain_has_tls = listener.filter_chains.iter()
        .any(|c| c.transport_socket.is_some());
    let any_chain_has_server_names = listener.filter_chains.iter()
        .any(|c| c.filter_chain_match.as_ref()
            .map(|m| !m.server_names.is_empty())
            .unwrap_or(false));

    let downstream_tls: Option<std::sync::Arc<envoy_tls::DownstreamTls>> = if !any_chain_has_tls {
        // Plaintext listener — fast path, no DownstreamTls construction.
        None
    } else if listener.filter_chains.len() == 1 && !any_chain_has_server_names {
        // Single-chain, no SNI routing — 03.1 path.
        let chain = &listener.filter_chains[0];
        let socket = chain.transport_socket.as_ref().expect("any_chain_has_tls implies Some here");
        let envoy_config::TransportSocketTypedConfig::Downstream(ctx) = &socket.typed_config else {
            anyhow::bail!("listener {:?} has non-downstream transport_socket", listener.name);
        };
        Some(std::sync::Arc::new(envoy_tls::DownstreamTls::from_context(ctx)?))
    } else {
        // Multi-chain or any chain with server_names — 03.2 path.
        Some(std::sync::Arc::new(envoy_tls::DownstreamTls::from_listener(listener)?))
    };

    // ... rest of per-listener wiring; pass downstream_tls into TlsAcceptingHandler if Some.
}
```

The `TlsAcceptingHandler` from 03.1 already accepts `Arc<DownstreamTls>` regardless of which constructor produced it — no changes needed there.

For multi-chain listeners (03.2 path), the inner handler is still constructed per *single filter* — but the filter to dispatch may differ across chains. For phase-03.2 fixtures 0005 + 0006, **both filter chains route to the same `tcp_proxy` filter targeting the same backend cluster**, so the wrapping logic can extract the filter from the first chain and ignore the others (or, more correctly, assert that all chains have the same filter cardinality and target — leave the latter as a TODO comment for phase 04 when filter-chain-specific filters become realistic).

For 03.2: extract the first chain's filter; thread it through `TlsAcceptingHandler { tls: Arc<DownstreamTls>, inner: Arc<TcpProxy> }` (the existing 03.1 shape).

```rust
let inner_filter = listener.filter_chains[0].filters.first()
    .ok_or_else(|| anyhow::anyhow!("listener {:?} has no filters in first chain", listener.name))?;
// (Existing 03.1 filter dispatch on inner_filter.name.as_str() builds the TcpProxy.)
```

- [ ] **Step 3: Add the per-cluster `Arc<UpstreamTls>` construction loop.**

Insert before the per-listener loop (so the `upstream_tls_by_cluster` map is available when constructing each `TcpProxy`):

```rust
// 03.2: per-cluster Arc<UpstreamTls> construction. Validator already
// rejected Downstream(_) on a cluster's transport_socket
// (MismatchedTransportSocketDirection { side: "cluster" }), so the
// Downstream(_) match arm is unreachable.
let mut upstream_tls_by_cluster: std::collections::HashMap<String, std::sync::Arc<envoy_tls::UpstreamTls>>
    = std::collections::HashMap::new();
for cluster in &bootstrap.static_resources.clusters {
    let Some(socket) = cluster.transport_socket.as_ref() else { continue };
    match &socket.typed_config {
        envoy_config::TransportSocketTypedConfig::Upstream(ctx) => {
            let upstream_tls = std::sync::Arc::new(
                envoy_tls::UpstreamTls::from_context(ctx)
                    .map_err(|e| anyhow::anyhow!("upstream TLS for cluster {:?}: {}", cluster.name, e))?,
            );
            upstream_tls_by_cluster.insert(cluster.name.clone(), upstream_tls);
        }
        envoy_config::TransportSocketTypedConfig::Downstream(_) => {
            // unreachable per validator; defensive fail-fast.
            anyhow::bail!("cluster {:?} has DownstreamTlsContext (validator should have rejected)", cluster.name);
        }
    }
}
```

- [ ] **Step 4: Thread `upstream_tls` into the `TcpProxy` constructor.**

In the per-listener loop's filter dispatch (the `envoy.filters.network.tcp_proxy` arm — landed in 03.1):

```rust
// Inside the tcp_proxy filter dispatch arm.
let envoy_config::TypedConfig::TcpProxy(tp_cfg) = inner_filter.typed_config.as_ref()
    .ok_or_else(|| anyhow::anyhow!("tcp_proxy filter missing typed_config"))?
else {
    anyhow::bail!("tcp_proxy filter has wrong typed_config");
};
let cluster_handle = cluster_mgr.get(&tp_cfg.cluster)
    .expect("validator guarantees cluster present");
let proxy: std::sync::Arc<envoy_tcp::TcpProxy> = match upstream_tls_by_cluster.get(&tp_cfg.cluster) {
    Some(upstream_tls) => std::sync::Arc::new(
        envoy_tcp::TcpProxy::with_upstream_tls(cluster_handle, &tp_cfg, upstream_tls.clone())
    ),
    None => std::sync::Arc::new(envoy_tcp::TcpProxy::new(cluster_handle, &tp_cfg)),
};
// (rest of TlsAcceptingHandler-or-bare wrapping unchanged.)
```

- [ ] **Step 5: Run the workspace test suite to verify no regressions.**

Run: `cargo test --workspace --lib --bins`

Expected: all 03.1 fixture-equivalent tests still pass — the new dispatch is backward-compatible (single-chain plaintext = 03.1 path; single-chain + transport_socket = 03.1 path; multi-chain = new 03.2 path; cluster transport_socket = new 03.2 path; cluster without transport_socket = 03.1 path). 03.1's existing `crates/envoy-bin/tests/tls_downstream.rs` should still pass.

If any test fails: bisect on the dispatch logic. Common pitfalls:
- The single-chain-no-server_names case must still hit `from_context` — easy to accidentally route to `from_listener` if the condition is overly restrictive.
- Cluster without `transport_socket` must still get plain `TcpProxy::new` — easy to accidentally treat `None` upstream_tls as an error.

- [ ] **Step 6: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 7: Commit Task 6.**

```bash
git add crates/envoy-bin/src/main.rs
git commit -m "$(cat <<'EOF'
phase 03.2: envoy-bin — multi-cert listener dispatch + upstream-TLS wiring

Listener walk now dispatches three ways: plaintext (no transport_socket on
any chain), single-cert (one chain + transport_socket + no server_names —
the 03.1 path via DownstreamTls::from_context), multi-cert (≥ 2 chains OR
any chain with server_names — the 03.2 path via DownstreamTls::from_listener).
TlsAcceptingHandler accepts both Arc<DownstreamTls> shapes unchanged.

Per-cluster Arc<UpstreamTls> construction: walks bootstrap.clusters, builds
UpstreamTls::from_context(&ctx) for each cluster carrying transport_socket:
Upstream(_), threads the resulting Arc into TcpProxy::with_upstream_tls in
the tcp_proxy filter dispatch arm. Clusters without transport_socket keep
TcpProxy::new (plaintext upstream — 03.1 path). Validator's
MismatchedTransportSocketDirection ensures the Downstream(_) arm on a
cluster's transport_socket is unreachable here.

The 03.1 single-cert plaintext-upstream code path is preserved for fixtures
0001-0004; the 03.2 paths add fixtures 0005 (TLS upstream) and 0006 (multi-
cert SNI). Both new paths are exercised by the in-process integration tests
in Task 7.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 8: Append the Task 6 PROGRESS.md section + commit progress note.**

```markdown
## Task 6 — envoy-bin: multi-cert + upstream-TLS wiring (YYYY-MM-DD)

- Commit: <SHA>
- Change: Listener walk extended with three-way dispatch (plaintext / single-cert via from_context / multi-cert via from_listener). Added per-cluster Arc<UpstreamTls> construction loop. Threaded upstream_tls into TcpProxy::with_upstream_tls in the tcp_proxy filter dispatch arm.
- Verification: `cargo test --workspace --lib --bins` reported all crates passed (existing 03.1 envoy-bin integration test tls_downstream.rs still green, no regressions). Workspace gate clean.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 6)"
```

---

### Task 7: `envoy-bin` — in-process integration tests `tls_upstream.rs` + `tls_sni.rs`

**Files:**
- Create: `crates/envoy-bin/tests/tls_upstream.rs`
- Create: `crates/envoy-bin/tests/tls_sni.rs`
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Task 6's wiring is end-to-end exercised here without Docker — the integration tests spawn `envoy-bin` as a subprocess via `CARGO_BIN_EXE_envoy-bin`, point it at an in-process upstream, and drive a payload round-trip. Same shape as 03.1's `tests/tls_downstream.rs`.

**Scope:** ~75 LoC per test, ~150 LoC total. Each test: rcgen-build a CA + leaf in a tempdir, spawn the test upstream(s), write a temp config YAML for envoy-bin, spawn envoy-bin, drive the round-trip, assert.

- [ ] **Step 1: Write `crates/envoy-bin/tests/tls_upstream.rs`.**

```rust
//! Phase 03.2 in-process integration test for plaintext-downstream + TLS-
//! upstream. Spawns envoy-bin as a subprocess pointing at an in-process
//! tokio_rustls TLS echo server. No Docker; runs on every `cargo test`.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test(flavor = "multi_thread")]
async fn tls_upstream_round_trip() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // 1. Generate test PKI (CA + server cert with SAN "envoy-rust.test").
    let pki = build_test_pki();

    // 2. Spawn an in-process tokio_rustls TLS echo server on 127.0.0.1:0.
    let upstream_addr = spawn_tls_echo_server(&pki).await;

    // 3. Reserve a port for envoy-bin's listener.
    let listener_port = reserve_port();

    // 4. Write a temp config YAML.
    let config = format!(
        r#"
node: {{ id: test, cluster: test }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}
static_resources:
  listeners:
    - name: tcp_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: {listener_port} }} }}
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
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {upstream_port} }} }}
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: "envoy-rust.test"
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: {ca_path}
"#,
        listener_port = listener_port,
        upstream_port = upstream_addr.port(),
        ca_path = pki.ca_pem_path.display(),
    );
    let config_path = pki.tmpdir.path().join("envoy-rust.yaml");
    std::fs::write(&config_path, config).expect("write config");

    // 5. Spawn envoy-bin pointing at the config.
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("--config-path").arg(&config_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    // 6. Wait for the listener to be reachable (poll TcpStream::connect with a 10s budget).
    let listener_addr: std::net::SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_for_listener(listener_addr, Duration::from_secs(10)).await;

    // 7. Drive a plaintext TCP round-trip through envoy-bin's listener.
    let mut stream = tokio::net::TcpStream::connect(listener_addr).await.expect("connect");
    let payload = b"hello, tls upstream\n";
    stream.write_all(payload).await.expect("write");
    let mut response = vec![0u8; payload.len()];
    stream.read_exact(&mut response).await.expect("read_exact");
    assert_eq!(response, payload);

    // 8. Tear down (Drop sends SIGKILL via kill_on_drop).
    drop(stream);
    let _ = child.kill().await;
    let _ = child.wait().await;
}

// --- Test helpers (private to this test) ---

struct TestPki {
    tmpdir: tempfile::TempDir,
    ca_pem_path: std::path::PathBuf,
    server_cert_pem: std::path::PathBuf,
    server_key_pem: std::path::PathBuf,
}

fn build_test_pki() -> TestPki {
    let tmpdir = tempfile::tempdir().expect("tempdir");
    // (Use rcgen to build CA + leaf with SAN "envoy-rust.test"; write all
    // PEMs into tmpdir.path(). Mirrors the 03.1 TlsTestPki shape — could
    // factor out a shared helper later, but for now duplicate inline.)
    let ca_kp = rcgen::KeyPair::generate().expect("ca keypair");
    let mut ca_params = rcgen::CertificateParams::new(vec![]).expect("ca params");
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.distinguished_name = {
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "envoy-rust test CA");
        dn
    };
    let ca_cert = ca_params.self_signed(&ca_kp).expect("ca self-signed");

    let leaf_kp = rcgen::KeyPair::generate().expect("leaf keypair");
    let mut leaf_params = rcgen::CertificateParams::new(vec!["envoy-rust.test".into()])
        .expect("leaf params");
    leaf_params.distinguished_name = {
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "envoy-rust.test");
        dn
    };
    let leaf_cert = leaf_params.signed_by(&leaf_kp, &ca_cert, &ca_kp).expect("leaf signed");

    let ca_pem_path = tmpdir.path().join("ca.pem");
    let server_cert_pem = tmpdir.path().join("server.crt");
    let server_key_pem = tmpdir.path().join("server.key");
    std::fs::write(&ca_pem_path, ca_cert.pem()).unwrap();
    std::fs::write(&server_cert_pem, leaf_cert.pem()).unwrap();
    std::fs::write(&server_key_pem, leaf_kp.serialize_pem()).unwrap();

    TestPki { tmpdir, ca_pem_path, server_cert_pem, server_key_pem }
}

async fn spawn_tls_echo_server(pki: &TestPki) -> std::net::SocketAddr {
    // Build a tokio_rustls TlsAcceptor with the leaf cert from pki; spawn an
    // accept loop that runs tokio::io::copy in a loopback. Returns the bound
    // SocketAddr.
    let cert_pem = std::fs::read(&pki.server_cert_pem).unwrap();
    let key_pem = std::fs::read(&pki.server_key_pem).unwrap();

    let cert_chain: Vec<_> = rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem.as_slice()))
        .map(|c| c.unwrap())
        .collect();
    let private_key = rustls_pemfile::pkcs8_private_keys(&mut std::io::BufReader::new(key_pem.as_slice()))
        .next().unwrap().unwrap();
    let private_key = rustls_pki_types::PrivateKeyDer::Pkcs8(private_key);

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .expect("server config");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(stream).await else { return };
                let (mut r, mut w) = tokio::io::split(&mut tls);
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    addr
}

fn reserve_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn wait_for_listener(addr: std::net::SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("envoy-bin listener never became reachable at {addr}");
}
```

(The above can factor a shared `tests/common/mod.rs` with `tls_downstream.rs` from 03.1 — both share `TestPki` + `wait_for_listener` + `reserve_port`. Optional refactor; either keep duplicated for now or extract.)

- [ ] **Step 2: Write `crates/envoy-bin/tests/tls_sni.rs`.**

Same shape as `tls_upstream.rs` but: (a) plaintext upstream (a `tcp-echo-server` host subprocess or in-process echo), (b) two filter chains on a single listener with leaf-A and leaf-B + `server_names: ["a.example.com"]` and `["b.example.com"]`, (c) two TLS connections from the test (one per SNI), (d) per-probe SAN/CN assertion via `tls.get_ref().1.peer_certificates()`.

```rust
//! Phase 03.2 in-process integration test for downstream multi-cert SNI cert
//! selection. Spawns envoy-bin as a subprocess with two filter chains; runs
//! two TLS handshakes (one per SNI), asserts the correct cert was selected.

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test(flavor = "multi_thread")]
async fn tls_sni_multi_cert_dispatch() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let pki = build_multi_leaf_pki();    // CA + leaf-A (SAN a.example.com) + leaf-B (SAN b.example.com)
    let upstream_addr = spawn_tcp_echo_backend().await;
    let listener_port = reserve_port();

    let config = format!(
        r#"
node: {{ id: test, cluster: test }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}
static_resources:
  listeners:
    - name: tcp_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: {port} }} }}
      filter_chains:
        - filter_chain_match: {{ server_names: ["a.example.com"] }}
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: {{ filename: {leaf_a_cert} }}
                    private_key:       {{ filename: {leaf_a_key} }}
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: {{ server_names: ["b.example.com"] }}
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: {{ filename: {leaf_b_cert} }}
                    private_key:       {{ filename: {leaf_b_key} }}
          filters:
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
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {upstream_port} }} }}
"#,
        port = listener_port,
        upstream_port = upstream_addr.port(),
        leaf_a_cert = pki.leaf_a_cert.display(),
        leaf_a_key = pki.leaf_a_key.display(),
        leaf_b_cert = pki.leaf_b_cert.display(),
        leaf_b_key = pki.leaf_b_key.display(),
    );
    let config_path = pki.tmpdir.path().join("envoy-rust.yaml");
    std::fs::write(&config_path, config).unwrap();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("--config-path").arg(&config_path)
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    let listener_addr: std::net::SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_for_listener(listener_addr, Duration::from_secs(10)).await;

    // Build a ClientConfig with the test CA in the root store.
    let mut roots = rustls::RootCertStore::empty();
    let ca_pem = std::fs::read(&pki.ca_pem).unwrap();
    for cert in rustls_pemfile::certs(&mut std::io::BufReader::new(ca_pem.as_slice())) {
        roots.add(cert.unwrap()).unwrap();
    }
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));

    // Probe A.
    let stream = tokio::net::TcpStream::connect(listener_addr).await.unwrap();
    let server_name = rustls_pki_types::ServerName::try_from("a.example.com").unwrap();
    let mut tls = connector.connect(server_name, stream).await.unwrap();
    let cert_a = tls.get_ref().1.peer_certificates().unwrap()[0].clone();
    assert!(cert_contains_san(&cert_a, "a.example.com"), "probe A wrong cert");
    tls.write_all(b"probe-a\n").await.unwrap();
    let mut buf = [0u8; 8];
    tls.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"probe-a\n");
    drop(tls);

    // Probe B.
    let stream = tokio::net::TcpStream::connect(listener_addr).await.unwrap();
    let server_name = rustls_pki_types::ServerName::try_from("b.example.com").unwrap();
    let mut tls = connector.connect(server_name, stream).await.unwrap();
    let cert_b = tls.get_ref().1.peer_certificates().unwrap()[0].clone();
    assert!(cert_contains_san(&cert_b, "b.example.com"), "probe B wrong cert");
    tls.write_all(b"probe-b\n").await.unwrap();
    let mut buf = [0u8; 8];
    tls.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"probe-b\n");
    drop(tls);

    let _ = child.kill().await;
    let _ = child.wait().await;
}

// ... helpers ...
```

(The helper module has roughly: `build_multi_leaf_pki`, `spawn_tcp_echo_backend`, `cert_contains_san`. The first builds CA + leaf-A + leaf-B; the second spawns an in-process plaintext TCP echo on 127.0.0.1:0; the third does a DER-substring scan. Mirror the 03.1 in-process test patterns.)

- [ ] **Step 3: Run the new tests.**

Run: `cargo test -p envoy-bin --test tls_upstream`
Run: `cargo test -p envoy-bin --test tls_sni`

Expected: each passes. The `tls_upstream` test takes ~1–3 seconds (subprocess spawn + TLS handshake + payload round-trip); the `tls_sni` test takes ~2–4 seconds (subprocess spawn + 2 handshakes).

If `wait_for_listener` times out, that means `envoy-bin` failed to start — capture the child's stderr and bisect the config.

- [ ] **Step 4: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

Expected: clean.

- [ ] **Step 5: Commit Task 7.**

```bash
git add crates/envoy-bin/tests/tls_upstream.rs crates/envoy-bin/tests/tls_sni.rs
git commit -m "$(cat <<'EOF'
phase 03.2: envoy-bin — in-process integration tests tls_upstream + tls_sni

Two new Rust-native, no-Docker integration tests that spawn envoy-bin via
CARGO_BIN_EXE_envoy-bin and exercise the 03.2 dispatch end-to-end.

tls_upstream_round_trip: plaintext downstream → envoy-bin → TLS upstream to
an in-process tokio_rustls TLS echo server. envoy-bin validates the
upstream's cert against the test CA, sends sni="envoy-rust.test" in the
ClientHello, and round-trips a payload byte-exact.

tls_sni_multi_cert_dispatch: two filter chains on a single listener, one per
leaf cert, routed by ClientHello SNI. Test runs two probes (SNI
"a.example.com" → leaf-A, SNI "b.example.com" → leaf-B), asserts each
post-handshake peer cert's SAN matches the expected value, round-trips a
per-probe payload through a plaintext upstream backend.

Both tests use rcgen-built test PKI in a per-test TempDir (ADR-0018 dev-test-
harness-only). Backstop fixtures 0005 + 0006 (Tasks 11 + 12) on every cargo
test invocation, even without Docker.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Append the Task 7 PROGRESS.md section + commit progress note.**

```markdown
## Task 7 — envoy-bin: in-process tls_upstream + tls_sni integration tests (YYYY-MM-DD)

- Commit: <SHA>
- Change: Added crates/envoy-bin/tests/tls_upstream.rs (~75 LoC) and crates/envoy-bin/tests/tls_sni.rs (~75 LoC). Each spawns envoy-bin as a subprocess via CARGO_BIN_EXE_envoy-bin and drives an end-to-end round-trip.
- Verification: `cargo test -p envoy-bin --test tls_upstream` passed in <Xs>; `cargo test -p envoy-bin --test tls_sni` passed in <Xs>. Workspace gate clean.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 7)"
```

---

### Task 8: Differential harness — `Driver::TlsTcpProbeList` + `TlsTcpProbe` + `drive_tls_probes` + `render_yaml` extensions + `run_fixture` dispatch

**Files:**
- Modify: `tests/differential/src/lib.rs` (add Driver variant + TlsTcpProbe struct + drive_tls_probes helper + extend render_yaml + extend run_fixture dispatch)
- Modify: `tests/differential/src/tls.rs` (extend `TlsTestPki::envoy_side_paths()` and `subject_side_paths()` with leaf-B + server keys)
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Fixtures 0005 + 0006 (Tasks 11 + 12) need the harness extensions to drive their probes. Task 8 lands the `Driver` variant + multi-probe helper + per-key render_yaml extensions; Task 9 lands the new `TlsEchoBackend`. Both required before fixture work can begin.

**Scope:** ~100 LoC impl + ~30 LoC tests (2 render_yaml unit tests for the new keys). The `drive_tls_probes` helper is the load-bearing addition — it walks `probes`, opens a fresh `TlsConnector::connect()` per probe with that probe's SNI, runs `drive_tls`'s body once, optionally checks `expected_cn` via DER-substring scan against `peer_certificates()`.

- [ ] **Step 1: Write the failing render_yaml unit tests in `tests/differential/src/lib.rs::tests`.**

Append to the existing `mod tests { ... }` block (look for `render_yaml_substitutes_tls_paths_for_envoy_side` / `..._for_subject_side` from 03.1):

```rust
#[test]
fn render_yaml_substitutes_leaf_b_and_server_paths_for_envoy_side() {
    let pki = crate::tls::TlsTestPki::generate().expect("pki");
    let template = r#"
listener: {{LEAF_A_CERT_PATH}} / {{LEAF_A_KEY_PATH}}
chain_b: {{LEAF_B_CERT_PATH}} / {{LEAF_B_KEY_PATH}}
server: {{SERVER_CERT_PATH}} / {{SERVER_KEY_PATH}}
ca: {{CA_PATH}}
"#;
    let envoy_paths = pki.envoy_side_paths();
    let envoy_refs: Vec<(&str, String)> = envoy_paths.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let rendered = crate::render_yaml(template, &envoy_refs);
    // All container-side paths under /etc/envoy-rust-tls/.
    assert!(rendered.contains("/etc/envoy-rust-tls/leaf-a.pem"));
    assert!(rendered.contains("/etc/envoy-rust-tls/leaf-a.key"));
    assert!(rendered.contains("/etc/envoy-rust-tls/leaf-b.pem"));
    assert!(rendered.contains("/etc/envoy-rust-tls/leaf-b.key"));
    assert!(rendered.contains("/etc/envoy-rust-tls/server.pem"));
    assert!(rendered.contains("/etc/envoy-rust-tls/server.key"));
    assert!(rendered.contains("/etc/envoy-rust-tls/ca.pem"));
    assert!(!rendered.contains("{{"));
}

#[test]
fn render_yaml_substitutes_leaf_b_and_server_paths_for_subject_side() {
    let pki = crate::tls::TlsTestPki::generate().expect("pki");
    let template = r#"
listener: {{LEAF_A_CERT_PATH}} / {{LEAF_A_KEY_PATH}}
chain_b: {{LEAF_B_CERT_PATH}} / {{LEAF_B_KEY_PATH}}
server: {{SERVER_CERT_PATH}} / {{SERVER_KEY_PATH}}
ca: {{CA_PATH}}
"#;
    let subject_paths = pki.subject_side_paths();
    let subject_refs: Vec<(&str, String)> = subject_paths.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let rendered = crate::render_yaml(template, &subject_refs);
    // All host-tmpdir paths.
    let tmp = pki._dir.path().display().to_string();
    assert!(rendered.contains(&format!("{tmp}/leaf-a.pem")) || rendered.contains("leaf-a.pem"));
    assert!(rendered.contains("leaf-b.pem"));
    assert!(rendered.contains("server.pem"));
    assert!(!rendered.contains("{{"));
}
```

- [ ] **Step 2: Run the new tests to verify they fail.**

Run: `cargo test -p differential --lib render_yaml_substitutes_leaf_b_and_server_paths_for_envoy_side render_yaml_substitutes_leaf_b_and_server_paths_for_subject_side`

Expected: tests fail because `envoy_side_paths()` / `subject_side_paths()` don't yet return entries for `LEAF_B_*` or `SERVER_*` keys. The test assertions on `contains("...leaf-b.pem")` etc. fail.

- [ ] **Step 3: Extend `TlsTestPki::envoy_side_paths()` and `subject_side_paths()` in `tests/differential/src/tls.rs`.**

The 03.1 implementation has these methods returning `Vec<(String, String)>` or `HashMap<String, String>` (verify by reading `tests/differential/src/tls.rs`). Extend the result with the new entries:

```rust
impl TlsTestPki {
    pub fn envoy_side_paths(&self) -> std::collections::HashMap<String, String> {
        // 03.1 shape:
        let mut m = std::collections::HashMap::new();
        m.insert("LEAF_A_CERT_PATH".to_string(), format!("{ENVOY_SIDE_DIR}/leaf-a.pem"));
        m.insert("LEAF_A_KEY_PATH".to_string(), format!("{ENVOY_SIDE_DIR}/leaf-a.key"));
        m.insert("CA_PATH".to_string(), format!("{ENVOY_SIDE_DIR}/ca.pem"));
        // 03.2 NEW:
        m.insert("LEAF_B_CERT_PATH".to_string(), format!("{ENVOY_SIDE_DIR}/leaf-b.pem"));
        m.insert("LEAF_B_KEY_PATH".to_string(), format!("{ENVOY_SIDE_DIR}/leaf-b.key"));
        m.insert("SERVER_CERT_PATH".to_string(), format!("{ENVOY_SIDE_DIR}/server.pem"));
        m.insert("SERVER_KEY_PATH".to_string(), format!("{ENVOY_SIDE_DIR}/server.key"));
        m
    }

    pub fn subject_side_paths(&self) -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("LEAF_A_CERT_PATH".to_string(), self.leaf_a_cert.display().to_string());
        m.insert("LEAF_A_KEY_PATH".to_string(), self.leaf_a_key.display().to_string());
        m.insert("CA_PATH".to_string(), self.ca_pem.display().to_string());
        // 03.2 NEW:
        m.insert("LEAF_B_CERT_PATH".to_string(), self.leaf_b_cert.display().to_string());
        m.insert("LEAF_B_KEY_PATH".to_string(), self.leaf_b_key.display().to_string());
        m.insert("SERVER_CERT_PATH".to_string(), self.server_cert.display().to_string());
        m.insert("SERVER_KEY_PATH".to_string(), self.server_key.display().to_string());
        m
    }

    pub fn container_mounts(&self) -> Vec<(std::path::PathBuf, String)> {
        // 03.1 + 03.2: every PEM that has a host-side path needs a mount.
        vec![
            (self.leaf_a_cert.clone(), format!("{ENVOY_SIDE_DIR}/leaf-a.pem")),
            (self.leaf_a_key.clone(),  format!("{ENVOY_SIDE_DIR}/leaf-a.key")),
            (self.ca_pem.clone(),      format!("{ENVOY_SIDE_DIR}/ca.pem")),
            (self.leaf_b_cert.clone(), format!("{ENVOY_SIDE_DIR}/leaf-b.pem")),    // 03.2 NEW
            (self.leaf_b_key.clone(),  format!("{ENVOY_SIDE_DIR}/leaf-b.key")),    // 03.2 NEW
            (self.server_cert.clone(), format!("{ENVOY_SIDE_DIR}/server.pem")),    // 03.2 NEW
            (self.server_key.clone(),  format!("{ENVOY_SIDE_DIR}/server.key")),    // 03.2 NEW
        ]
    }
}
```

(Verify the actual struct field names in 03.1 by reading `tests/differential/src/tls.rs`. The 03.1 PROGRESS Task 10 said "TlsTestPki struct (7 pub path fields + private `_dir: TempDir`)" so the fields exist; just confirm the names.)

- [ ] **Step 4: Add the `TlsTcpProbe` struct + `Driver::TlsTcpProbeList` variant in `tests/differential/src/lib.rs`.**

Find the existing `Driver` enum (at this point: `TcpEcho`, `HttpGet { path }`, `TlsTcp { sni, expected_cn }`). Add the new variant + the probe struct:

```rust
#[derive(Clone, Debug, serde::Deserialize, PartialEq)]
pub struct TlsTcpProbe {
    pub sni: String,
    #[serde(default)]
    pub expected_cn: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Driver {
    TcpEcho,
    HttpGet { path: String },
    TlsTcp {
        sni: String,
        #[serde(default)]
        expected_cn: Option<String>,
    },
    TlsTcpProbeList {                                                    // 03.2 NEW
        probes: Vec<TlsTcpProbe>,
    },
}
```

(The exact `#[serde(tag = "kind", rename_all = "snake_case")]` lives on the existing 03.1 enum — preserve it. The `tls_tcp_probe_list` snake-case variant maps to the YAML `kind: tls_tcp_probe_list`.)

- [ ] **Step 5: Write the `drive_tls_probes` helper in `tests/differential/src/lib.rs`.**

Add near the existing `drive_tls` helper (likely after it):

```rust
/// Run drive_tls's body once per probe. Each probe gets a fresh TLS connection
/// (fresh handshake) at the same listener address; the SNI varies per probe.
/// Returns Ok if all probes round-trip byte-exact and (when expected_cn is
/// Some) the post-handshake peer cert's SAN/CN contains the expected value.
pub async fn drive_tls_probes(
    addr: std::net::SocketAddr,
    payload: &[u8],
    probes: &[TlsTcpProbe],
    root_store: rustls::RootCertStore,
) -> anyhow::Result<()> {
    let _ = envoy_tls::install_default_crypto_provider();
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_config));

    for probe in probes {
        let stream = tokio::net::TcpStream::connect(addr).await
            .map_err(|e| anyhow::anyhow!("connect for probe sni={}: {}", probe.sni, e))?;
        let server_name = rustls_pki_types::ServerName::try_from(probe.sni.clone())
            .map_err(|e| anyhow::anyhow!("invalid sni {}: {}", probe.sni, e))?;
        let mut tls = connector.connect(server_name, stream).await
            .map_err(|e| anyhow::anyhow!("handshake for probe sni={}: {}", probe.sni, e))?;

        // expected_cn check: peer cert SAN/CN substring scan.
        if let Some(expected) = &probe.expected_cn {
            let (_io, conn) = tls.get_ref();
            let cert = conn.peer_certificates()
                .and_then(|c| c.first())
                .ok_or_else(|| anyhow::anyhow!("no peer cert for probe sni={}", probe.sni))?;
            if !check_cn_or_san(cert.as_ref(), expected) {
                anyhow::bail!(
                    "probe sni={}: peer cert does not contain expected CN/SAN {:?}",
                    probe.sni, expected
                );
            }
        }

        // Mirror drive_tls: write payload, read_exact, ADR-0007 tail-byte poll.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tls.write_all(payload).await
            .map_err(|e| anyhow::anyhow!("write for probe sni={}: {}", probe.sni, e))?;

        let mut response = vec![0u8; payload.len()];
        tls.read_exact(&mut response).await
            .map_err(|e| anyhow::anyhow!("read_exact for probe sni={}: {}", probe.sni, e))?;
        if response != payload {
            anyhow::bail!("probe sni={}: byte mismatch (got {:?}, want {:?})",
                probe.sni, response, payload);
        }

        // Trailing byte poll (100ms; ADR-0007).
        let mut extra = [0u8; 1];
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            tls.read(&mut extra)
        ).await;

        // Graceful close.
        let _ = tls.shutdown().await;
        drop(tls);
    }
    Ok(())
}
```

(The `check_cn_or_san` function was added in 03.1 — reuse the existing helper; do not redefine.)

- [ ] **Step 6: Extend `run_fixture` in `tests/differential/src/lib.rs`.**

Find the existing `run_fixture` body. The 03.1 path detects TLS templates (via `needs_tls_pki`-style boolean) and `{{BACKEND_PORT}}` (via `TcpProxyBackend`-needed boolean). Add a third detection: `{{TLS_BACKEND_PORT}}` (in either rendered template) → spawn `TlsEchoBackend` (added in Task 9 — for now this branch can `bail!` with a clear "TlsEchoBackend not yet wired up" message; Task 9 wires it).

Also add the `Driver::TlsTcpProbeList` dispatch arm to the existing dispatch match. The arm builds a `RootCertStore` from `pki.ca_pem_path()` and calls `drive_tls_probes(addr, payload, probes, roots).await?`.

```rust
// In run_fixture, after the existing TLS template detection + TcpProxyBackend
// spawn (3.1 path):

let needs_tls_backend = rendered_envoy.contains("{{TLS_BACKEND_PORT}}")
    || rendered_subject.contains("{{TLS_BACKEND_PORT}}");
let _tls_backend = if needs_tls_backend {
    // Task 9 fills this in.
    anyhow::bail!("TlsEchoBackend not yet wired up — pending Task 9");
} else {
    None
};

// In the dispatch match (after the existing Driver::TcpEcho, Driver::HttpGet,
// Driver::TlsTcp arms):

Driver::TlsTcpProbeList { probes } => {
    let pki = pki.as_ref().expect("TlsTcpProbeList implies TLS template");
    let mut roots = rustls::RootCertStore::empty();
    let ca_pem = std::fs::read(&pki.ca_pem)?;
    for cert in rustls_pemfile::certs(&mut std::io::BufReader::new(ca_pem.as_slice())) {
        roots.add(cert?)?;
    }
    drive_tls_probes(envoy_rust_addr, &payload, &probes, roots.clone()).await
        .map_err(|e| anyhow::anyhow!("envoy-rust drive_tls_probes: {}", e))?;
    drive_tls_probes(envoy_addr, &payload, &probes, roots).await
        .map_err(|e| anyhow::anyhow!("envoy drive_tls_probes: {}", e))?;
}
```

The `port_key` match (which determines whether the harness substitutes `{{PORT}}` per side) needs updating: `Driver::TlsTcpProbeList` should use the same port-key as `Driver::TlsTcp` and `Driver::TcpEcho` (i.e., `"PORT"`):

```rust
let port_key = match &driver {
    Driver::TcpEcho | Driver::TlsTcp { .. } | Driver::TlsTcpProbeList { .. } => "PORT",
    Driver::HttpGet { .. } => "PORT",
};
```

- [ ] **Step 7: Run the new tests + existing run_fixture tests.**

Run: `cargo test -p differential --lib`

Expected: 37 (existing from 03.1) + 2 (new render_yaml tests) = 39 tests pass; 1 ignored (Docker-gated, unchanged).

- [ ] **Step 8: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 9: Commit Task 8.**

```bash
git add tests/differential/src/lib.rs tests/differential/src/tls.rs
git commit -m "$(cat <<'EOF'
phase 03.2: differential — TlsTcpProbeList + drive_tls_probes + render_yaml extensions

Driver gains a TlsTcpProbeList { probes: Vec<TlsTcpProbe> } variant for
multi-probe TLS fixtures. TlsTcpProbe carries { sni, expected_cn }; each
probe is one TLS handshake against the same listener with its own SNI,
optionally asserting the post-handshake peer cert's SAN/CN matches
expected_cn (DER-substring scan via the existing 03.1 check_cn_or_san).

drive_tls_probes mirrors drive_tls's ADR-0006/0007 discipline once per probe:
read_exact(payload.len()) + 100ms trailing-byte poll + graceful shutdown.
The match arm in run_fixture's dispatch routes Driver::TlsTcpProbeList through
two drive_tls_probes calls (one against envoy, one against envoy-rust).

TlsTestPki::envoy_side_paths(), subject_side_paths(), and container_mounts()
gain entries for LEAF_B_CERT/KEY (fixture 0006 multi-cert) and SERVER_CERT/KEY
(fixture 0005 tls-echo-server). render_yaml resolves the new keys verbatim
on each side. 2 new render_yaml unit tests assert all keys substitute.

run_fixture detects {{TLS_BACKEND_PORT}} (fixture 0005's tls-echo-server upstream);
the actual TlsEchoBackend spawn lands in Task 9.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 10: Append the Task 8 PROGRESS.md section + commit progress note.**

```markdown
## Task 8 — differential: TlsTcpProbeList + drive_tls_probes + render_yaml extensions (YYYY-MM-DD)

- Commit: <SHA>
- Change: Driver gained TlsTcpProbeList variant + TlsTcpProbe struct. drive_tls_probes helper added. render_yaml extended with LEAF_B_*, SERVER_* keys. run_fixture dispatch extended (TlsTcpProbeList arm; TLS_BACKEND_PORT detection placeholder pending Task 9). 2 new render_yaml unit tests.
- Verification: `cargo test -p differential --lib` reported 39 passed, 1 ignored. Workspace gate clean.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 8)"
```

---

### Task 9: Differential harness — `TlsEchoBackend` + 2 unit tests + `upstream::start` mount-fan-out

**Files:**
- Modify: `tests/differential/src/backend.rs` (add `TlsEchoBackend` struct + spawn + Drop + 2 tests)
- Modify: `tests/differential/src/lib.rs` (wire `TlsEchoBackend` into `run_fixture`'s `{{TLS_BACKEND_PORT}}` detection — replace Task 8's `bail!` placeholder)
- Modify: `tests/differential/src/upstream.rs` (verify mount-fan-out walks `pki.container_mounts()` — should be correct from 03.1; adjust if needed)
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** `TlsEchoBackend` lands before the `tls-echo-server` helper crate (Task 10) per SPEC §6 signpost 1. The struct is a thin wrapper over `tokio::process::Command::new(<workspace_path>/target/<profile>/tls-echo-server)`; it compiles fine without the binary existing. The 2 unit tests follow the 02.2 `TcpProxyBackend` precedent of skip-if-binary-not-built (early `return` if `locate_tls_echo_server()` returns `Err`), so they pass cleanly even before Task 10 lands. Once Task 10 lands the binary, the same tests *do* exercise the spawn end-to-end via `cargo test --workspace --bins` (which builds all workspace bins before running tests).

**Scope:** ~70 LoC for `TlsEchoBackend` (struct + spawn + Drop + locate helper) + ~40 LoC for the 2 tests. The `Drop` impl mirrors `TcpProxyBackend`'s SIGKILL-on-Drop (see phase-02.2 REVIEW M1 — same posture inherited; the polling-loop-blocks-on-sleep concern is tracked forward to whichever phase first parallelizes `run_fixture`).

- [ ] **Step 1: Write the 2 failing unit tests in `tests/differential/src/backend.rs::tests`.**

Append after the existing 02.2 `tcp_proxy_backend_spawns_and_echoes` and `tcp_proxy_backend_drop_terminates_child` tests. Mirror their skip-if-not-built shape (per 02.2 PROGRESS Task 9 + REVIEW §3 M1 the precedent uses `if locate_tcp_echo_server().is_err() { return; }` early-return).

```rust
#[tokio::test(flavor = "multi_thread")]
async fn tls_echo_backend_spawns_and_echoes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Skip if tls-echo-server binary is not built (Task 10 lands it).
    if locate_tls_echo_server().is_err() {
        eprintln!("skipping tls_echo_backend_spawns_and_echoes — tls-echo-server not built; run `cargo test --workspace`");
        return;
    }
    let _ = envoy_tls::install_default_crypto_provider();

    let pki = crate::tls::TlsTestPki::generate().expect("pki");
    let backend = TlsEchoBackend::spawn(&pki.server_cert, &pki.server_key).await
        .expect("spawn tls-echo-server");

    // Build a TLS client with the test CA in the root store.
    let mut roots = rustls::RootCertStore::empty();
    let ca_pem = std::fs::read(&pki.ca_pem).unwrap();
    for cert in rustls_pemfile::certs(&mut std::io::BufReader::new(ca_pem.as_slice())) {
        roots.add(cert.unwrap()).unwrap();
    }
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_config));

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", backend.port())).await.unwrap();
    let server_name = rustls_pki_types::ServerName::try_from("envoy-rust.test").unwrap();
    let mut tls = connector.connect(server_name, stream).await.expect("handshake");

    let payload = b"hello, tls-echo-server\n";
    tls.write_all(payload).await.unwrap();
    let mut response = vec![0u8; payload.len()];
    tls.read_exact(&mut response).await.unwrap();
    assert_eq!(response, payload);

    drop(tls);
    drop(backend);
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_echo_backend_drop_terminates_child() {
    if locate_tls_echo_server().is_err() {
        eprintln!("skipping tls_echo_backend_drop_terminates_child — tls-echo-server not built");
        return;
    }
    let pki = crate::tls::TlsTestPki::generate().expect("pki");
    let backend = TlsEchoBackend::spawn(&pki.server_cert, &pki.server_key).await
        .expect("spawn tls-echo-server");
    let port = backend.port();

    drop(backend);

    // After Drop, the port should release within ~200ms (SIGKILL is fast).
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let result = std::net::TcpStream::connect(("127.0.0.1", port));
    assert!(result.is_err(), "expected port {port} to be released after Drop");
}
```

- [ ] **Step 2: Run the new tests to verify they fail.**

Run: `cargo test -p differential --lib backend::tests::tls_echo`

Expected: build error citing `cannot find struct "TlsEchoBackend"` and `cannot find function "locate_tls_echo_server"`. Both fixed in Step 3.

- [ ] **Step 3: Add `TlsEchoBackend` + `locate_tls_echo_server` in `tests/differential/src/backend.rs`.**

Mirror the existing `TcpProxyBackend` + `locate_tcp_echo_server` shape (from 02.2 Task 9). The locate helper walks two parents up from `CARGO_MANIFEST_DIR` (i.e., from `tests/differential/`) to the workspace root, honors `CARGO_TARGET_DIR`, picks `debug` or `release` per `cfg!(debug_assertions)`, adds `.exe` on Windows.

```rust
/// Workspace-relative path to the tls-echo-server binary. Mirrors
/// locate_tcp_echo_server. Returns Err if the binary is not at the expected
/// path (e.g., not built yet, or workspace layout changed).
pub(crate) fn locate_tls_echo_server() -> Result<std::path::PathBuf, String> {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // From tests/differential/, two parents up to the workspace root.
    let workspace = manifest.parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| format!("cannot resolve workspace from {}", manifest.display()))?;

    let target = std::env::var("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target"));

    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let binary_name = if cfg!(windows) { "tls-echo-server.exe" } else { "tls-echo-server" };

    let path = target.join(profile).join(binary_name);
    if !path.exists() {
        return Err(format!(
            "tls-echo-server binary not found at {} — run `cargo build --workspace --bins` first",
            path.display()
        ));
    }
    Ok(path)
}

/// Spawns the tls-echo-server helper binary on a reserved 127.0.0.1 port,
/// pointing at the given cert + key PEMs. Drop sends SIGKILL via tokio's
/// kill_on_drop(true) plus a 50ms-poll/2s-deadline fallback (mirrors
/// TcpProxyBackend's posture; phase-02.2 REVIEW M1 carries forward
/// unchanged).
pub struct TlsEchoBackend {
    port: u16,
    child: tokio::process::Child,
    _server_cert: std::path::PathBuf,    // alive-keeper for the spawn lifetime
    _server_key: std::path::PathBuf,
}

impl TlsEchoBackend {
    pub async fn spawn(server_cert: &std::path::Path, server_key: &std::path::Path) -> anyhow::Result<Self> {
        let bin = locate_tls_echo_server()
            .map_err(|e| anyhow::anyhow!(e))?;

        // Reserve a port (race-y but matches TcpProxyBackend's posture).
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let port = listener.local_addr()?.port();
            drop(listener);
            port
        };

        let child = tokio::process::Command::new(&bin)
            .arg("--port").arg(port.to_string())
            .arg("--cert").arg(server_cert)
            .arg("--key").arg(server_key)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // Wait for the listener to be reachable.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                anyhow::bail!("tls-echo-server on port {port} never became reachable");
            }
            if std::net::TcpStream::connect_timeout(
                &format!("127.0.0.1:{port}").parse()?,
                std::time::Duration::from_millis(100),
            ).is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        Ok(Self {
            port,
            child,
            _server_cert: server_cert.to_path_buf(),
            _server_key: server_key.to_path_buf(),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for TlsEchoBackend {
    fn drop(&mut self) {
        // SIGKILL on Drop. Mirrors TcpProxyBackend (phase-02.2 REVIEW M1).
        let _ = self.child.start_kill();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                Err(_) => return,
            }
        }
    }
}
```

- [ ] **Step 4: Wire `TlsEchoBackend` into `run_fixture` (replace Task 8's `bail!` placeholder).**

Locate the Task 8 stub in `tests/differential/src/lib.rs::run_fixture`:

```rust
// Task 8 placeholder.
let _tls_backend = if needs_tls_backend {
    anyhow::bail!("TlsEchoBackend not yet wired up — pending Task 9");
} else {
    None
};
```

Replace with:

```rust
let _tls_backend: Option<crate::backend::TlsEchoBackend> = if needs_tls_backend {
    let pki = pki.as_ref().ok_or_else(|| anyhow::anyhow!("TLS backend implies TLS pki"))?;
    let backend = crate::backend::TlsEchoBackend::spawn(&pki.server_cert, &pki.server_key).await
        .map_err(|e| anyhow::anyhow!("spawn TlsEchoBackend: {}", e))?;
    // Substitute {{TLS_BACKEND_PORT}} and {{BACKEND_HOST}} for the rendered
    // YAMLs (envoy side: host.docker.internal; envoy-rust side: 127.0.0.1).
    let envoy_tls_port = backend.port().to_string();
    let subject_tls_port = envoy_tls_port.clone();
    upstream_tls_paths.push(("TLS_BACKEND_PORT", envoy_tls_port));
    subject_tls_paths.push(("TLS_BACKEND_PORT", subject_tls_port));
    upstream_tls_paths.push(("BACKEND_HOST", backend.container_host().to_string()));
    subject_tls_paths.push(("BACKEND_HOST", "127.0.0.1".to_string()));
    Some(backend)
} else {
    None
};
```

(Adjust to match the actual variable names in 03.1's `run_fixture` body — the exact `upstream_tls_paths` / `subject_tls_paths` accumulator names from 03.1 PROGRESS Task 11 may differ; verify by reading the current `run_fixture` body.)

- [ ] **Step 5: Verify `upstream::start` mount-fan-out.**

Read `tests/differential/src/upstream.rs::start`. The 03.1 implementation took `pki: Option<&TlsTestPki>` and walked `pki.container_mounts()` to call `with_copy_to(container_path, host_path)` for each PEM. After Task 8's extension to `TlsTestPki::container_mounts()`, the loop should now mount 7 PEMs (3 from 03.1 + 4 new) without code changes.

Run: `grep -A 10 "container_mounts" tests/differential/src/upstream.rs` to confirm the loop walks the iterator. If the 03.1 implementation hard-coded a fixed list of mounts, refactor to walk `container_mounts()` instead.

- [ ] **Step 6: Run the new tests + the workspace test gate.**

Run: `cargo test -p differential --lib`

Expected: at this point, with `tls-echo-server` not yet built (Task 10 lands it), the 2 new `tls_echo_*` tests print `skipping tls_echo_backend_spawns_and_echoes — tls-echo-server not built` to stderr but pass. Total: 39 (after Task 8) + 2 (skip-passes) = 41 tests pass; 1 ignored (Docker-gated, unchanged).

After Task 10 lands and `cargo test --workspace` builds the new bin, re-running this test produces the actual TLS round-trip and Drop-terminates-child results. The same test code covers both cases via the early-`return` branch.

- [ ] **Step 7: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 8: Commit Task 9.**

```bash
git add tests/differential/src/backend.rs tests/differential/src/lib.rs tests/differential/src/upstream.rs
git commit -m "$(cat <<'EOF'
phase 03.2: differential — TlsEchoBackend + 2 unit tests + run_fixture wiring

TlsEchoBackend spawns the workspace's tls-echo-server helper binary on a
reserved 127.0.0.1 port, pointing at a cert + key PEM pair from a
TlsTestPki. Drop posture mirrors TcpProxyBackend (SIGKILL via tokio's
kill_on_drop(true) + 50ms-poll/2s-deadline fallback; phase-02.2 REVIEW M1
inherited). locate_tls_echo_server walks two parents up from
CARGO_MANIFEST_DIR to the workspace root, honors CARGO_TARGET_DIR, and
picks debug/release per cfg!(debug_assertions).

run_fixture's Task 8 bail! placeholder now spawns TlsEchoBackend when the
rendered YAML references {{TLS_BACKEND_PORT}}; the substitution maps gain
TLS_BACKEND_PORT and BACKEND_HOST entries (envoy-side: host.docker.internal;
subject-side: 127.0.0.1, mirroring the TcpProxyBackend posture from 02.2).

upstream::start's mount-fan-out walks pki.container_mounts() (extended in
Task 8 to include leaf-B + server PEMs) — no code changes needed beyond
verifying the loop is the iterator-driven shape from 03.1.

The 2 unit tests (tls_echo_backend_spawns_and_echoes and
tls_echo_backend_drop_terminates_child) early-return with a stderr note
when locate_tls_echo_server returns Err — at this point in the plan the
binary is not yet built (Task 10 lands it). After Task 10, `cargo test
--workspace` builds the bin and re-running these tests exercises the full
TLS round-trip + Drop semantics. Same skip-if-not-built precedent as
phase-02.2 TcpProxyBackend.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 9: Append the Task 9 PROGRESS.md section + commit progress note.**

```markdown
## Task 9 — differential: TlsEchoBackend + 2 unit tests + run_fixture wiring (YYYY-MM-DD)

- Commit: <SHA>
- Change: Added TlsEchoBackend struct + spawn + Drop + locate_tls_echo_server helper. Wired run_fixture's TLS_BACKEND_PORT detection (replaced Task 8's bail! placeholder). Verified upstream::start mount-fan-out walks pki.container_mounts() (extended in Task 8). 2 new unit tests with skip-if-not-built fall-through (tls-echo-server bin lands in Task 10).
- Verification: `cargo test -p differential --lib` reported 41 passed (2 new tests skip-pass before Task 10), 1 ignored. Workspace gate clean.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 9)"
```

---


### Task 10: `tests/helpers/tls-echo-server/` helper crate (full impl + 5 unit tests)

**Files:**
- Create: `tests/helpers/tls-echo-server/Cargo.toml`
- Create: `tests/helpers/tls-echo-server/src/main.rs`
- Modify: root `Cargo.toml` (add `tests/helpers/tls-echo-server` to `[workspace] members`)
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Task 9's `TlsEchoBackend` references this binary. After Task 10 lands, `cargo test --workspace` builds the bin and Task 9's previously-skip-passed tests now exercise the actual TLS round-trip.

**Scope:** ~120 LoC impl + ~120 LoC tests. The shape mirrors `tests/helpers/tcp-echo-server/` from phase 02.1 with rustls server-side termination on top.

- [ ] **Step 1: Create `tests/helpers/tls-echo-server/Cargo.toml`.**

```toml
[package]
name = "tls-echo-server"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[[bin]]
name = "tls-echo-server"
path = "src/main.rs"

[dependencies]
anyhow = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt-multi-thread", "net", "io-util", "macros", "signal", "time", "sync"] }
tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }
rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }
rustls-pki-types = "1"
rustls-pemfile = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
rcgen = "0.13"
tempfile = "3"
```

(All deps covered by ADR-0019's rustls grant + ADR-0018's rcgen+tempfile dev-test-harness-only grant.)

- [ ] **Step 2: Add `tests/helpers/tls-echo-server` to workspace members in root `Cargo.toml`.**

Read root `Cargo.toml`'s `[workspace] members` list (already has `tests/helpers/tcp-echo-server` from phase 02.1, `crates/envoy-tls` from 03.1, etc.). Append `"tests/helpers/tls-echo-server"`:

```toml
[workspace]
members = [
    "crates/envoy-bin",
    "crates/envoy-cluster",
    "crates/envoy-config",
    "crates/envoy-listener",
    "crates/envoy-tcp",
    "crates/envoy-tls",
    "tests/differential",
    "tests/helpers/tcp-echo-server",
    "tests/helpers/tls-echo-server",    # 03.2 NEW
]
```

(Match the existing list's exact alphabetical / grouped order; the above is illustrative.)

- [ ] **Step 3: Write the 4 failing argv-parser tests in `tests/helpers/tls-echo-server/src/main.rs::tests`.**

Mirror the shape of `tests/helpers/tcp-echo-server/src/main.rs::tests` from phase 02.1 (which uses a hand-parsed argv with a typed `ArgvError` enum). Read `tests/helpers/tcp-echo-server/src/main.rs:1..50` for the exact precedent.

```rust
#![forbid(unsafe_code)]

use std::path::PathBuf;

mod runtime;

#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
    cert: PathBuf,
    key: PathBuf,
}

#[derive(Debug, thiserror::Error, PartialEq)]
enum ArgvError {
    #[error("missing required flag {0}")]
    MissingFlag(&'static str),
    #[error("missing value for flag")]
    MissingValue,
    #[error("invalid port value")]
    InvalidPort,
    #[error("trailing argument")]
    Trailing,
    #[error("--help requested")]
    HelpRequested,
    #[error("--version requested")]
    VersionRequested,
}

fn parse_args(argv: &[String]) -> Result<Args, ArgvError> {
    // Stub for now — Step 4 implements.
    Err(ArgvError::MissingFlag("--port"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_parses_full_invocation() {
        let argv = vec![
            "--port".into(), "10042".into(),
            "--cert".into(), "/tmp/c.pem".into(),
            "--key".into(),  "/tmp/k.pem".into(),
        ];
        let args = parse_args(&argv).expect("parse");
        assert_eq!(args.port, 10042);
        assert_eq!(args.cert, PathBuf::from("/tmp/c.pem"));
        assert_eq!(args.key, PathBuf::from("/tmp/k.pem"));
    }

    #[test]
    fn argv_rejects_missing_cert() {
        let argv = vec![
            "--port".into(), "10042".into(),
            "--key".into(),  "/tmp/k.pem".into(),
        ];
        assert_eq!(parse_args(&argv), Err(ArgvError::MissingFlag("--cert")));
    }

    #[test]
    fn argv_rejects_missing_key() {
        let argv = vec![
            "--port".into(), "10042".into(),
            "--cert".into(), "/tmp/c.pem".into(),
        ];
        assert_eq!(parse_args(&argv), Err(ArgvError::MissingFlag("--key")));
    }

    #[test]
    fn argv_shows_help() {
        let argv = vec!["--help".into()];
        assert_eq!(parse_args(&argv), Err(ArgvError::HelpRequested));
    }
}
```

- [ ] **Step 4: Implement `parse_args` to make the 4 argv tests pass.**

```rust
fn parse_args(argv: &[String]) -> Result<Args, ArgvError> {
    let mut port: Option<u16> = None;
    let mut cert: Option<PathBuf> = None;
    let mut key: Option<PathBuf> = None;
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Err(ArgvError::HelpRequested),
            "--version" | "-V" => return Err(ArgvError::VersionRequested),
            "--port" => {
                let v = iter.next().ok_or(ArgvError::MissingValue)?;
                port = Some(v.parse().map_err(|_| ArgvError::InvalidPort)?);
            }
            "--cert" => {
                let v = iter.next().ok_or(ArgvError::MissingValue)?;
                cert = Some(PathBuf::from(v));
            }
            "--key" => {
                let v = iter.next().ok_or(ArgvError::MissingValue)?;
                key = Some(PathBuf::from(v));
            }
            _ => return Err(ArgvError::Trailing),
        }
    }
    Ok(Args {
        port: port.ok_or(ArgvError::MissingFlag("--port"))?,
        cert: cert.ok_or(ArgvError::MissingFlag("--cert"))?,
        key: key.ok_or(ArgvError::MissingFlag("--key"))?,
    })
}
```

Run: `cargo test -p tls-echo-server --lib argv_parses_full_invocation argv_rejects_missing_cert argv_rejects_missing_key argv_shows_help`

Expected: 4 passed.

- [ ] **Step 5: Implement the runtime in `tests/helpers/tls-echo-server/src/runtime.rs` (or inline in main.rs).**

```rust
//! TLS echo server runtime. Single-cert ResolvesServerCert (no SNI multiplexing
//! — this helper is single-purpose). Mirrors tcp-echo-server's accept loop +
//! tokio::signal::ctrl_c shutdown + 5s drain budget.

use std::path::Path;
use std::sync::Arc;

pub async fn run(port: u16, cert_path: &Path, key_path: &Path) -> anyhow::Result<()> {
    // Idempotent crypto provider install.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Load cert + key via rustls-pemfile.
    let cert_pem = std::fs::read(cert_path)?;
    let key_pem = std::fs::read(key_path)?;

    let cert_chain: Vec<_> = rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem.as_slice()))
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("parsing cert: {}", e))?;
    let private_key = rustls_pemfile::pkcs8_private_keys(&mut std::io::BufReader::new(key_pem.as_slice()))
        .next()
        .ok_or_else(|| anyhow::anyhow!("no PKCS#8 private key in {}", key_path.display()))?
        .map_err(|e| anyhow::anyhow!("parsing key: {}", e))?;
    let private_key = rustls_pki_types::PrivateKeyDer::Pkcs8(private_key);

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .map_err(|e| anyhow::anyhow!("building server config: {}", e))?;
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    tracing::info!("tls-echo-server listening on 127.0.0.1:{port}");

    let mut join_set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutdown signal received");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        let acceptor = acceptor.clone();
                        join_set.spawn(async move {
                            match acceptor.accept(stream).await {
                                Ok(mut tls) => {
                                    let (mut r, mut w) = tokio::io::split(&mut tls);
                                    let _ = tokio::io::copy(&mut r, &mut w).await;
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "tls handshake failed; dropping");
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept failed; continuing");
                    }
                }
            }
        }
    }

    // Drain phase: 5s budget, then abort stragglers.
    drop(listener);
    let drain = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(_) = join_set.join_next().await {}
    });
    let _ = drain.await;
    join_set.abort_all();
    while let Some(_) = join_set.join_next().await {}

    Ok(())
}
```

- [ ] **Step 6: Implement `main` to wire `parse_args` + `run`.**

```rust
fn print_help() {
    println!(
        "tls-echo-server: TLS echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  tls-echo-server --port <u16> --cert <path> --key <path>\n  \
         tls-echo-server --help\n  tls-echo-server --version\n"
    );
}

fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .with_writer(std::io::stderr)
        .init();

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(ArgvError::HelpRequested) => {
            print_help();
            return std::process::ExitCode::from(0);
        }
        Err(ArgvError::VersionRequested) => {
            println!("tls-echo-server {}", env!("CARGO_PKG_VERSION"));
            return std::process::ExitCode::from(0);
        }
        Err(e) => {
            eprintln!("argv error: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to build tokio runtime: {e}");
            return std::process::ExitCode::from(1);
        }
    };

    match rt.block_on(runtime::run(args.port, &args.cert, &args.key)) {
        Ok(()) => std::process::ExitCode::from(0),
        Err(e) => {
            eprintln!("runtime error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 7: Add the 5th test (`accepts_and_echoes_via_tls`) — TLS round-trip integration test.**

In `tests/helpers/tls-echo-server/src/main.rs::tests`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn accepts_and_echoes_via_tls() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Build CA + leaf via rcgen in a tempdir.
    let tmpdir = tempfile::tempdir().unwrap();
    let ca_kp = rcgen::KeyPair::generate().unwrap();
    let mut ca_params = rcgen::CertificateParams::new(vec![]).unwrap();
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca_cert = ca_params.self_signed(&ca_kp).unwrap();

    let leaf_kp = rcgen::KeyPair::generate().unwrap();
    let leaf_params = rcgen::CertificateParams::new(vec!["envoy-rust.test".into()]).unwrap();
    let leaf_cert = leaf_params.signed_by(&leaf_kp, &ca_cert, &ca_kp).unwrap();

    let cert_path = tmpdir.path().join("server.crt");
    let key_path = tmpdir.path().join("server.key");
    std::fs::write(&cert_path, leaf_cert.pem()).unwrap();
    std::fs::write(&key_path, leaf_kp.serialize_pem()).unwrap();

    // Reserve a port and spawn the runtime in a background task.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    };
    let cert_p = cert_path.clone();
    let key_p = key_path.clone();
    let server_handle = tokio::spawn(async move {
        let _ = runtime::run(port, &cert_p, &key_p).await;
    });

    // Wait for the listener to be reachable.
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() { break; }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Connect a TLS client with the test CA in the root store.
    let mut roots = rustls::RootCertStore::empty();
    let ca_pem = std::fs::read(tmpdir.path().join("ca.pem")).unwrap_or_else(|_| ca_cert.pem().into_bytes());
    for c in rustls_pemfile::certs(&mut std::io::BufReader::new(ca_pem.as_slice())) {
        roots.add(c.unwrap()).unwrap();
    }
    // Also add the CA via the rcgen-built bytes (above unwrap_or_else handles
    // the case where ca.pem wasn't written to tmpdir).
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_config));
    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let server_name = rustls_pki_types::ServerName::try_from("envoy-rust.test").unwrap();
    let mut tls = connector.connect(server_name, stream).await.expect("handshake");

    let payload = b"hello, tls-echo-server\n";
    tls.write_all(payload).await.unwrap();
    let mut response = vec![0u8; payload.len()];
    tls.read_exact(&mut response).await.unwrap();
    assert_eq!(response, payload);

    drop(tls);
    server_handle.abort();
}
```

(Note: the test writes `ca.pem` is implicit — fix by writing the CA's PEM into the tmpdir alongside the leaf:)

```rust
// In the test, immediately after building ca_cert:
std::fs::write(tmpdir.path().join("ca.pem"), ca_cert.pem()).unwrap();
```

Run: `cargo test -p tls-echo-server --lib accepts_and_echoes_via_tls`

Expected: passes (TLS handshake completes, payload round-trips byte-exact).

- [ ] **Step 8: Run the full tls-echo-server test suite.**

Run: `cargo test -p tls-echo-server`

Expected: 5 tests pass (4 argv + 1 round-trip).

- [ ] **Step 9: Re-run Task 9's TlsEchoBackend tests now that the binary is built.**

Run: `cargo test --workspace --lib --bins`

Expected: total = 41 (after Task 9 with skip-passes) — but now the 2 skip-pass tests actually run end-to-end (the `if locate_tls_echo_server().is_err() { return; }` guard now falls through). Total still 41 passed, but the 2 `tls_echo_*` tests now exercise the spawn + handshake.

- [ ] **Step 10: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 11: Commit Task 10.**

```bash
git add Cargo.toml tests/helpers/tls-echo-server/Cargo.toml tests/helpers/tls-echo-server/src/main.rs
# (and any sibling files like src/runtime.rs if split out)
git commit -m "$(cat <<'EOF'
phase 03.2: tls-echo-server helper crate (full impl + 5 tests)

New workspace member tests/helpers/tls-echo-server/ — sibling of
tcp-echo-server from phase 02.1, with rustls server-side termination on
top. Single-cert ResolvesServerCert (no SNI multiplexing — single-purpose
helper). Hand-parsed argv (--port, --cert, --key, --help, --version) with
typed ArgvError. Tokio runtime: idempotent aws_lc_rs::default_provider
install, tokio_rustls::TlsAcceptor accept loop with tokio::signal::ctrl_c
shutdown + 5s drain budget + abort-stragglers. Exit codes: 0 clean, 1
runtime error, 2 argv error.

5 unit tests: 4 argv (full invocation, missing cert, missing key, help)
+ 1 multi-threaded TLS round-trip (rcgen-built CA + leaf with SAN
"envoy-rust.test"; 22-byte payload byte-exact echo).

Workspace [members] gains tests/helpers/tls-echo-server. Deps covered by
ADR-0019 (tokio-rustls + rustls-pemfile) and ADR-0018 (rcgen + tempfile
dev-deps only).

Task 9's TlsEchoBackend tests (skip-passed before this commit) now
exercise the full spawn + handshake on `cargo test --workspace --bins`.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 12: Append the Task 10 PROGRESS.md section + commit progress note.**

```markdown
## Task 10 — tls-echo-server helper crate (YYYY-MM-DD)

- Commit: <SHA>
- Change: New workspace member tests/helpers/tls-echo-server/ with hand-parsed argv, tokio_rustls TlsAcceptor accept loop, single-cert ResolvesServerCert, ctrl_c shutdown + 5s drain. 5 unit tests (4 argv + 1 TLS round-trip via rcgen-built CA). Added to workspace members.
- Verification: `cargo test -p tls-echo-server` reported 5 passed. `cargo test --workspace --lib --bins` reported all crates passed (Task 9's TlsEchoBackend tests now exercise the full spawn). Workspace gate clean.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 10)"
```

---

### Task 11: Fixture `0005-tls-upstream` (5 files) + Docker-gated `tests/differential/tests/tls_upstream.rs`

**Files:**
- Create: `tests/fixtures/0005-tls-upstream/envoy.yaml`
- Create: `tests/fixtures/0005-tls-upstream/envoy-rust.yaml`
- Create: `tests/fixtures/0005-tls-upstream/inputs/payload.bin`
- Create: `tests/fixtures/0005-tls-upstream/expectations.yaml`
- Create: `tests/fixtures/0005-tls-upstream/README.md`
- Create: `tests/differential/tests/tls_upstream.rs`
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Task 9 + Task 10 land all the harness machinery (Driver, drive_tls_probes, TlsEchoBackend, render_yaml keys, tls-echo-server bin). Fixture 0005 is the first to actually use them.

**Scope:** ~80 LoC YAML + ~10 LoC Rust test. Verbatim from SPEC §3 D8 fixture 0005.

- [ ] **Step 1: Create fixture YAMLs.**

`tests/fixtures/0005-tls-upstream/envoy.yaml`:

```yaml
node: { id: envoy-rust-phase-03.2-fixture-0005, cluster: envoy-rust-phase-03.2 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
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
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{TLS_BACKEND_PORT}} } }
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: "envoy-rust.test"
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: {{CA_PATH}}
```

`tests/fixtures/0005-tls-upstream/envoy-rust.yaml`:

```yaml
node: { id: envoy-rust-phase-03.2-fixture-0005, cluster: envoy-rust-phase-03.2 }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
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
                  address: { socket_address: { address: 127.0.0.1, port_value: {{TLS_BACKEND_PORT}} } }
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: "envoy-rust.test"
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: {{CA_PATH}}
```

(Note: no `admin` block per ADR-0011 / phase-01 precedent — envoy-rust does not run an admin server. Per SPEC §3 D8 the YAMLs differ only on the per-side divergences.)

- [ ] **Step 2: Copy fixture-0001's payload.bin byte-identical.**

```bash
mkdir -p tests/fixtures/0005-tls-upstream/inputs
cp tests/fixtures/0001-tcp-echo/inputs/payload.bin tests/fixtures/0005-tls-upstream/inputs/payload.bin
diff tests/fixtures/0001-tcp-echo/inputs/payload.bin tests/fixtures/0005-tls-upstream/inputs/payload.bin
```

Expected: `diff` exits 0 (byte-identical). Per SPEC §3 D8 + 03.1 fixture 0004 precedent.

- [ ] **Step 3: Create `tests/fixtures/0005-tls-upstream/expectations.yaml`.**

```yaml
driver:
  kind: tcp_echo
equivalence:
  response_body: byte_exact
```

(`kind: tcp_echo`, **not** `tls_tcp` — fixture 0005's *downstream* is plaintext per SPEC §3 D8. The TLS happens on the upstream side, exercised end-to-end via the byte round-trip succeeding through the `tls-echo-server`.)

- [ ] **Step 4: Create `tests/fixtures/0005-tls-upstream/README.md`.**

```markdown
# Fixture 0005-tls-upstream

## Property

Plaintext downstream → envoy / envoy-rust → upstream TLS origination to a
single `tls-echo-server` helper. The configured `sni: "envoy-rust.test"` is
sent in the upstream ClientHello server_name extension; the upstream TLS
server accepts the connection only because the harness CA validates and
the leaf cert's SAN matches.

## Differential surface

Both proxies dial the same `tls-echo-server` upstream. The post-handshake
byte stream round-trips `inputs/payload.bin` byte-exact in both directions.
The wire-level SNI is exercised by virtue of the upstream TLS server
producing a valid handshake (the rcgen-built leaf has SAN
`envoy-rust.test`; rustls's default verifier rejects mismatched SNI).

## ADRs referenced

- ADR-0015 — cross-container `host.docker.internal` + `host-gateway`
  (envoy-side backend host resolution).
- ADR-0017 — split phase 03 into 03.1 + 03.2.
- ADR-0018 — rcgen + tempfile dev-test-harness-only (the harness PKI used
  to build the upstream's cert).
- ADR-0019 — tokio-rustls + rustls-pemfile under the rustls grant
  (envoy-rust's UpstreamTls + the tls-echo-server's rustls usage).

## Out of scope (deferred)

Per parent-SPEC §4 + 03.2 SPEC §4:
- mTLS (out of phase 03 entirely).
- Inline cert/key bytes (filename only).
- `validation_context.match_subject_alt_names` (default rustls verifier
  asserts SAN matches `ServerName`).
- Wildcard SAN (the rcgen-built cert has the literal `envoy-rust.test` SAN).
- TLS protocol-version pin (rustls + Envoy v1.33.0 negotiate defaults).
```

- [ ] **Step 5: Create `tests/differential/tests/tls_upstream.rs` (Docker-gated).**

```rust
//! Phase 03.2 fixture 0005-tls-upstream: Docker-gated acceptance test.

#![forbid(unsafe_code)]

use std::path::PathBuf;

#[tokio::test]
async fn tls_upstream_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0005-tls-upstream");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

(Same shape as 03.1's `tests/differential/tests/tls_downstream.rs`. The `run_fixture` invocation is gated by Docker availability — when Docker is not running, `testcontainers` returns a clear error and the test fails with that message; when Docker is running, the upstream Envoy container starts, both proxies bind their listeners, and the harness runs `drive_tcp` against each.)

- [ ] **Step 6: Run the new fixture test (Docker-gated).**

If Docker is available:

```bash
cargo test -p differential --test tls_upstream
```

Expected: passes (~10–30 seconds for upstream Envoy container start + handshake + round-trip).

If Docker is not available:

Skip this step. The CI job (`ubuntu-latest`) provides Docker and runs this test alongside `echo_fixture`, `admin_ready_fixture`, `tcp_proxy_fixture`, and `tls_downstream_fixture`. Document the skip in PROGRESS.md.

- [ ] **Step 7: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

Expected: clean. The Docker-gated `tls_upstream_fixture` is excluded by `--lib --bins`.

- [ ] **Step 8: Commit Task 11.**

```bash
git add tests/fixtures/0005-tls-upstream/ tests/differential/tests/tls_upstream.rs
git commit -m "$(cat <<'EOF'
phase 03.2: fixture 0005-tls-upstream + Docker-gated acceptance test

Plaintext downstream → upstream TLS origination via tls-echo-server. envoy
and envoy-rust each dial the same tls-echo-server with sni="envoy-rust.test"
in the ClientHello, validate against the harness CA, round-trip a byte-
identical 18-byte payload (copied from fixture 0001).

Both YAMLs declare a single FilterChain with cluster.transport_socket =
UpstreamTlsContext (trusted_ca + sni); no DownstreamTlsContext. driver.kind
is tcp_echo (the downstream is plaintext); the TLS hop is exercised
end-to-end through the byte round-trip succeeding.

Docker-gated tls_upstream_fixture test in tests/differential/tests/ runs
in CI (ubuntu-latest); the in-process backstop is
crates/envoy-bin/tests/tls_upstream.rs from Task 7.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 9: Append the Task 11 PROGRESS.md section + commit progress note.**

```markdown
## Task 11 — fixture 0005-tls-upstream + Docker-gated test (YYYY-MM-DD)

- Commit: <SHA>
- Change: Created tests/fixtures/0005-tls-upstream/ with 5 files (envoy.yaml, envoy-rust.yaml, inputs/payload.bin byte-identical to 0001, expectations.yaml driver.kind tcp_echo, README.md). Created tests/differential/tests/tls_upstream.rs (Docker-gated).
- Verification: <if Docker available> `cargo test -p differential --test tls_upstream` passed in <Xs>. Workspace gate clean (Docker-gated test excluded by --lib --bins).
- Docker run skipped if not available locally — CI runs it on ubuntu-latest.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 11)"
```

---

### Task 12: Fixture `0006-tls-sni` (5 files) + Docker-gated `tests/differential/tests/tls_sni.rs`

**Files:**
- Create: `tests/fixtures/0006-tls-sni/envoy.yaml`
- Create: `tests/fixtures/0006-tls-sni/envoy-rust.yaml`
- Create: `tests/fixtures/0006-tls-sni/inputs/payload.bin`
- Create: `tests/fixtures/0006-tls-sni/expectations.yaml`
- Create: `tests/fixtures/0006-tls-sni/README.md`
- Create: `tests/differential/tests/tls_sni.rs`
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`

**Why now:** Fixture 0005 (Task 11) exercises the upstream-TLS path. Fixture 0006 exercises the multi-cert downstream SNI path — the second of phase-03.2's two new fixtures. Same machinery, different driver (`Driver::TlsTcpProbeList` with two probes).

**Scope:** ~120 LoC YAML + ~10 LoC Rust test. Verbatim from SPEC §3 D8 fixture 0006.

- [ ] **Step 1: Create `tests/fixtures/0006-tls-sni/envoy.yaml`.**

```yaml
node: { id: envoy-rust-phase-03.2-fixture-0006, cluster: envoy-rust-phase-03.2 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: {{LEAF_A_CERT_PATH}} }
                    private_key:       { filename: {{LEAF_A_KEY_PATH}} }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: {{LEAF_B_CERT_PATH}} }
                    private_key:       { filename: {{LEAF_B_KEY_PATH}} }
          filters:
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
                  address: { socket_address: { address: {{BACKEND_HOST}}, port_value: {{BACKEND_PORT}} } }
```

- [ ] **Step 2: Create `tests/fixtures/0006-tls-sni/envoy-rust.yaml`.**

(Same shape with the per-side divergences: bind 127.0.0.1, no admin block, backend host 127.0.0.1.)

```yaml
node: { id: envoy-rust-phase-03.2-fixture-0006, cluster: envoy-rust-phase-03.2 }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: {{LEAF_A_CERT_PATH}} }
                    private_key:       { filename: {{LEAF_A_KEY_PATH}} }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: {{LEAF_B_CERT_PATH}} }
                    private_key:       { filename: {{LEAF_B_KEY_PATH}} }
          filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: {{BACKEND_PORT}} } }
```

- [ ] **Step 3: Copy fixture-0001's payload.bin.**

```bash
mkdir -p tests/fixtures/0006-tls-sni/inputs
cp tests/fixtures/0001-tcp-echo/inputs/payload.bin tests/fixtures/0006-tls-sni/inputs/payload.bin
diff tests/fixtures/0001-tcp-echo/inputs/payload.bin tests/fixtures/0006-tls-sni/inputs/payload.bin
```

Expected: `diff` exits 0.

- [ ] **Step 4: Create `tests/fixtures/0006-tls-sni/expectations.yaml`.**

```yaml
driver:
  kind: tls_tcp_probe_list
  probes:
    - { sni: "a.example.com", expected_cn: "a.example.com" }
    - { sni: "b.example.com", expected_cn: "b.example.com" }
equivalence:
  response_body: byte_exact
```

- [ ] **Step 5: Create `tests/fixtures/0006-tls-sni/README.md`.**

```markdown
# Fixture 0006-tls-sni

## Property

Downstream TLS with multi-cert SNI cert selection. One listener; two filter
chains; each chain carries a different cert keyed on its
`filter_chain_match.server_names`. Plaintext upstream backend
(`tcp-echo-server` from phase 02.1).

## Differential surface

Two probes per test invocation:
- Probe A: connect with SNI `a.example.com`; assert post-handshake peer cert
  SAN/CN contains `a.example.com`; round-trip `inputs/payload.bin` byte-exact.
- Probe B: connect with SNI `b.example.com`; assert peer cert SAN/CN contains
  `b.example.com`; round-trip `inputs/payload.bin` byte-exact.

The cert-selection assertion lives in the harness driver
(`drive_tls_probes`), not as a new equivalence-matrix dimension. Both proxies
must select the *same* cert for the *same* SNI for the test to pass — that
is the property under test. The matrix row engaged is still row 2 of §7.2
(post-handshake bytes byte-exact).

## SNI resolution mechanism

`rustls::server::ResolvesServerCert` keyed on lowercased ClientHello SNI
(rustls 0.23 returns lowercase SNI; envoy-tls's SniResolver stores lowercase
keys; case-insensitive exact match). Envoy mirrors via
`filter_chain_match.server_names` matching with the same case-insensitive
contract. The validator (envoy-config) rejects overlapping SNIs and
multiple catch-all chains at config-load time.

Unknown-SNI close behavior is **not** asserted in this fixture (parent-SPEC
§6 signpost 8 — adding a third probe with `expected_close: bool` is a
future-fixture option that lands its own ADR; the TLS-alert delta vs.
plain-close delta is potentially divergent between rustls and Envoy).

## ADRs referenced

- ADR-0017 — split phase 03 into 03.1 + 03.2.
- ADR-0018 — rcgen + tempfile dev-test-harness-only.
- ADR-0019 — tokio-rustls + rustls-pemfile under the rustls grant.

## Out of scope (deferred)

Per parent-SPEC §4 + 03.2 SPEC §4:
- Wildcard SAN values (`*.example.com`) — `TlsTestPki` does not generate them.
- mTLS — out of phase 03 entirely.
- `validation_context.match_typed_subject_alt_names` — out of phase 03.
- `tls_inspector` listener filter (would unlock TLS-and-plaintext mixing on
  the same listener) — out of phase 03.
- Filter-chain framework / per-route TLS config — phase 07.
```

- [ ] **Step 6: Create `tests/differential/tests/tls_sni.rs`.**

```rust
//! Phase 03.2 fixture 0006-tls-sni: Docker-gated acceptance test.

#![forbid(unsafe_code)]

use std::path::PathBuf;

#[tokio::test]
async fn tls_sni_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0006-tls-sni");
    differential::run_fixture(&dir).await.expect("fixture passes");
}
```

- [ ] **Step 7: Run the new fixture test (Docker-gated).**

If Docker is available: `cargo test -p differential --test tls_sni`. Expected: passes (~15–40 seconds; two probes per side = four handshakes plus the upstream Envoy container start).

If not available: skip locally; CI runs it.

- [ ] **Step 8: Run the workspace gate.**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --lib --bins
```

Expected: clean.

- [ ] **Step 9: Commit Task 12.**

```bash
git add tests/fixtures/0006-tls-sni/ tests/differential/tests/tls_sni.rs
git commit -m "$(cat <<'EOF'
phase 03.2: fixture 0006-tls-sni + Docker-gated acceptance test

Multi-cert downstream SNI cert selection. One listener with two filter chains
(server_names: ["a.example.com"] → leaf-A; server_names: ["b.example.com"] →
leaf-B). Plaintext upstream backend (tcp-echo-server from phase 02.1).

driver.kind: tls_tcp_probe_list with 2 probes. Each probe opens a TLS
handshake with its own SNI, asserts the post-handshake peer cert's SAN/CN
contains the expected value (DER substring scan), round-trips an 18-byte
byte-identical payload (copied from 0001).

Both proxies build a single multi-cert ServerConfig per listener (envoy-tls
DownstreamTls::from_listener; rustls::server::ResolvesServerCert SniResolver
keyed on lowercased ClientHello SNI). The validator's
MultipleListenersWithOverlappingSni and MultipleCatchAllFilterChains rules
ensure both sides have unambiguous mappings.

Docker-gated tls_sni_fixture test in tests/differential/tests/ runs in CI;
the in-process backstop is crates/envoy-bin/tests/tls_sni.rs from Task 7.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 10: Append the Task 12 PROGRESS.md section + commit progress note.**

```markdown
## Task 12 — fixture 0006-tls-sni + Docker-gated test (YYYY-MM-DD)

- Commit: <SHA>
- Change: Created tests/fixtures/0006-tls-sni/ with 5 files (envoy.yaml + envoy-rust.yaml each with 2 filter chains, inputs/payload.bin byte-identical to 0001, expectations.yaml driver.kind tls_tcp_probe_list with 2 probes, README.md). Created tests/differential/tests/tls_sni.rs (Docker-gated).
- Verification: <if Docker available> `cargo test -p differential --test tls_sni` passed in <Xs>. Workspace gate clean (Docker-gated test excluded by --lib --bins).
- Docker run skipped if not available locally — CI runs it.
- Deviation from PLAN: <none expected>.
```

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: progress note (task 12)"
```

---

### Task 13: State 4 phase-done gate — run all 5 stable commands + CI fuzz job; quote outputs into PROGRESS.md

**Files:**
- Modify: `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md` (append the state-4 gate section with full command outputs)
- (No code changes; if `Cargo.lock` is dirty, sync as a dedicated commit per the phase-01/02.1/02.2/03.1 precedent.)

**Why now:** All 12 implementation tasks complete. State 4 of the phase lifecycle (per `BOOTSTRAP_PROMPT.md` §5) verifies that the workspace is clean against the full set of stable-toolchain commands + the CI fuzz job. State 4 outputs feed into the state-5 REVIEW.md (next session) per the lifecycle.

**Scope:** ~150 LoC of PROGRESS.md content (mostly verbatim command outputs). Possibly one Cargo.lock sync commit if drift exists.

- [ ] **Step 1: Run the full local stable-toolchain gate.**

In sequence:

```bash
cargo build --workspace --all-targets
```
Expected: exits 0 with `Finished dev profile [unoptimized + debuginfo] target(s) in <s>`.

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
Expected: exits 0 with no warnings.

```bash
cargo fmt --all -- --check
```
Expected: exits 0 with no output.

```bash
cargo test --workspace --lib --bins
```
Expected: per-crate test counts + total. Approximate expectations:
- `differential` — 41 passed, 1 ignored Docker-gated (Tasks 8 + 9 added 4 new)
- `envoy-bin` (main) — 19 passed (unchanged from 03.1; integration tests in `crates/envoy-bin/tests/` count separately)
- `envoy-cluster` — 8 passed (unchanged)
- `envoy-config` — 50 (from 03.1) + 11 (Tasks 1+2: 5 + 6) = 61 passed
- `envoy-listener` — 6 passed (unchanged)
- `envoy-tcp` — 8 (from 03.1) + 3 (Task 5) = 11 passed
- `envoy-tls` — 10 (from 03.1) + 5 (Tasks 3+4) = 15 passed
- `tcp-echo-server` — 8 passed (unchanged)
- `tls-echo-server` — 5 passed (Task 10)

Total expected: ~174 tests pass + 1 ignored.

```bash
cargo deny check
```
Expected: `advisories ok, bans ok, licenses ok, sources ok` (with the existing `license-not-encountered` warnings carried forward unchanged from 03.1).

- [ ] **Step 2: Run the CI fuzz job locally (best-effort).**

```bash
cd crates/envoy-config
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30
cd ../..
```

Expected: 30 seconds of fuzzing against the corpus that now includes 03.1's 3 TLS seeds + Task 2's 2 new seeds (`tls_multi_cert_sni.yaml`, `tls_overlapping_sni_reject.yaml`). Exits 0 (no crashes); reports `INFO: -max_total_time=30 reached`.

If `+nightly` toolchain is not installed locally: skip; CI runs it on `ubuntu-latest`. Document the skip.

- [ ] **Step 3: Run the Docker-gated acceptance tests (best-effort).**

If Docker is available:

```bash
cargo test -p differential --test tls_upstream
cargo test -p differential --test tls_sni
```

Expected: both pass. Existing 03.1 fixtures should still pass:

```bash
cargo test -p differential --test echo_fixture
cargo test -p differential --test admin_ready_fixture
cargo test -p differential --test tcp_proxy_fixture
cargo test -p differential --test tls_downstream
```

If Docker is not available: skip; CI runs them on `ubuntu-latest`. Document the skip.

- [ ] **Step 4: If `Cargo.lock` is dirty, commit a sync.**

```bash
git status
```

If `Cargo.lock` is dirty (most likely from the new `tls-echo-server` package stanza + any rustls family transitive bumps):

```bash
git add Cargo.lock
git commit -m "phase 03.2: sync Cargo.lock with phase 03.2 dep graph"
```

This mirrors phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`.

If clean (no drift), skip; document the no-op.

- [ ] **Step 5: Append the state-4 gate section to PROGRESS.md.**

```markdown
## Task 13 / State 4 — phase-done gate verification (YYYY-MM-DD)

Per `docs/envoy-rust/SKILL_ROUTING.md` state 4: <gate verdict — clean on first attempt | required N fix-during-gate commits>. ROADMAP.md and STATE.md are NOT advanced here per `BOOTSTRAP_PROMPT.md` §5.1 (one state per session); those flip in state 6 (the phase-done commit) after state 5's `REVIEW.md` is approved.

### Local stable-toolchain gate

`cargo build --workspace --all-targets`:
```
<paste output>
```

`cargo clippy --workspace --all-targets --all-features -- -D warnings`:
```
<paste output>
```

`cargo fmt --all -- --check`:
```
<paste output — likely "(no output — clean; exit 0)">
```

`cargo test --workspace --lib --bins`:
```
<paste per-crate "test result: ok. N passed; ..." lines>
```

Total: <N> tests passed, 0 failed, 1 ignored (Docker-gated).

`cargo deny check`:
```
<paste output>
```

### Docker-gated acceptance tests

<if run locally> `cargo test -p differential --test tls_upstream` and `cargo test -p differential --test tls_sni`: <paste output>.

<if not run locally> Docker not available; CI (ubuntu-latest) will run all 6 fixture tests (echo_fixture, admin_ready_fixture, tcp_proxy_fixture, tls_downstream, tls_upstream, tls_sni).

### Fuzz job

<if run locally> `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`: <paste output>.

<if not run locally> Nightly toolchain not installed; CI runs it.

### Cargo.lock sync

<if dirty> Will be landed as a dedicated `phase 03.2: sync Cargo.lock with phase 03.2 dep graph` commit immediately following this PROGRESS commit, per the precedent shape from phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`.

<if clean> No drift; no sync needed.

### Outstanding for state 5/6

State 5 (`superpowers:requesting-code-review`) writes `REVIEW.md` for this phase. State 6 (the phase-done commit) flips ROADMAP row `03.2` `status` → `done` AND parent row `03` `status` → `done` (per the schema: parent flips when all sub-phases are `done`; row `03.1` is already `done`), and advances STATE.md to phase `04` (slug TBD; lifecycle state 1; next-skill `superpowers:brainstorming`). The state-6 commit message follows SPEC §9.
```

- [ ] **Step 6: Commit Task 13.**

```bash
git add docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md
git commit -m "phase 03.2: state-4 phase-done gate verification (task 13)"
```

(If a Cargo.lock sync commit is needed, land it as a separate commit immediately after this one — not amended into this commit.)

---

## Out-of-plan execution contingencies

Per phase-03.1 PLAN's matching section + SPEC §5 + BOOTSTRAP_PROMPT.md §5 deviations:

- **Ambiguity → ADR + proceed.** If execution surfaces a runtime ambiguity not covered by the SPEC (e.g., rustls's `SniResolver::resolve` returning `None` produces a TLS alert that Envoy treats differently in some negotiation), land an ADR (likely ADR-0020) per D-3.5 and continue.
- **Blocked by upstream → ROADMAP status=blocked, STATE note, exit clean.** If Envoy v1.33.0's behavior for a 03.2 case differs from what the SPEC anticipates (e.g., wildcard SNI semantics in `filter_chain_match.server_names`), land an ADR pinning the policy and document the deferral.
- **Unexpected state → `superpowers:systematic-debugging` FIRST.** If the executor finds the project state mid-task does not match what the PLAN expects (e.g., the `envoy-tcp` `Cargo.toml` already has `envoy-tls` under `[dependencies]` from a prior session), invoke systematic-debugging before continuing per BOOTSTRAP_PROMPT.md §1 Step E.
- **PLAN drift → SPEC wins.** Where this PLAN and the SPEC disagree (e.g., a variant name, a method signature), the SPEC is authoritative. Flag the drift in PROGRESS.md, land a small ADR if the drift is material, otherwise note inline and continue.
- **Mid-execution split trigger.** If any single task balloons past ~10 sub-steps once contact with reality reveals complexity, invoke `superpowers:systematic-debugging` *before* attempting a nested split. Phase 03.2 was already produced *by* a split (ADR-0017); a nested split would be unusual and deserves a fresh root-cause read.

---

## Self-review

Spec coverage: each SPEC §3 deliverable D1–D10 maps to at least one task:
- D1 (envoy-tls SniResolver + from_listener) → Tasks 3 + 4.
- D2 (envoy-config schema + validator) → Tasks 1 + 2.
- D3 (envoy-tcp upstream-TLS) → Task 5.
- D4 (Cluster::name accessor opportunistic) → Task 5 Step 7 (decision documented in PROGRESS).
- D5 (envoy-bin wiring + 2 integration tests) → Tasks 6 + 7.
- D6 (differential harness) → Tasks 8 + 9.
- D7 (tls-echo-server crate) → Task 10.
- D8 (fixtures 0005 + 0006) → Tasks 11 + 12.
- D9 (CI workflow) → no changes per SPEC §3 D9; verified at Task 13.
- D10 (ADRs) → no anticipated ADRs per SPEC §7; if execution surfaces one, lands at the next-sequential available number.

Type consistency: `DownstreamTls::from_listener` (Task 4) is referenced by `envoy-bin` Task 6 dispatch with the exact same signature. `TcpProxy::with_upstream_tls` (Task 5) is referenced by Task 6's per-cluster construction loop. `Driver::TlsTcpProbeList` (Task 8) is referenced by fixture 0006's `expectations.yaml` (Task 12 Step 4). `TlsEchoBackend::spawn(&Path, &Path)` (Task 9) signature matches the `tls-echo-server` argv parsed in Task 10. Verified consistent.

Placeholder scan: no "TBD", "implement later", "similar to Task N", or unbounded "etc." Each task's code blocks contain the actual code an engineer needs. Where code references functions that aren't fully shown (e.g., `cert_contains_san` from 03.1), the reference notes the existing definition's location.

Plan ready for execution.
