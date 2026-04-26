# Phase 03.2 — Upstream TLS origination + multi-cert SNI cert selection + fixtures 0005 + 0006

- **Phase id:** `03.2`
- **Parent phase:** `03-tls-tcp` (split per ADR-0017)
- **Title:** Upstream TLS origination + multi-cert SNI cert selection + `tls-echo-server` helper + fixtures 0005 + 0006
- **Depends on:** `03.1` (envoy-tls foundation + downstream TLS termination + fixture 0004). Sibling sub-phase 03.1 MUST be `done` (its ROADMAP row flipped to `done` in 03.1's final commit) before 03.2 enters `in-progress`.
- **Differential surface when done:** two new fixtures green against upstream `envoyproxy/envoy:v1.33.0`:
  - `tests/fixtures/0005-tls-upstream/` — plaintext downstream, upstream TLS origination from envoy-rust to a new in-tree `tls-echo-server` helper, with the configured `sni` field sent in the upstream ClientHello.
  - `tests/fixtures/0006-tls-sni/` — multi-cert SNI cert selection on a single downstream listener (chain A serves cert A on `sni: a.example.com`; chain B serves cert B on `sni: b.example.com`).
  Pre-existing fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy`, and `0004-tls-downstream` remain green.
- **Seeded by:** `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (parent, committed at SHA `a3f3474`) §§D1 (03.2 portion), D2 (03.2 portion), D4 (03.2 portion), D5 (03.2 portion), D6 (03.2 portion), D7, D8 (fixtures 0005 + 0006); split decision at ADR-0017.

This SPEC is the design contract for sub-phase 03.2. The next session — after 03.1 has landed its final commit and STATE.md has advanced to `03.2-tls-upstream-sni` — converts this into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and 03.1's final state (via `git log` and the landed `envoy-tls` / `envoy-config` / `envoy-tcp` / `envoy-bin` surface) must be able to execute it without consulting the parent `03-tls-tcp/SPEC.md`.

---

## 1. Goal and acceptance signal

**Goal.** Layer the two remaining TLS properties on top of 03.1's foundation:

1. **Upstream TLS origination with wire-level SNI.** A static cluster declaring `transport_socket: envoy.transport_sockets.tls (UpstreamTlsContext)` causes envoy-rust to dial each picked endpoint with a rustls client handshake against the configured trust bundle (`validation_context.trusted_ca`), sending the configured `sni` value in the ClientHello server_name extension. Fixture 0005 proves the post-handshake byte stream is byte-exact across both proxies; the SNI on the wire is exercised by virtue of the upstream TLS server (the new `tls-echo-server` helper) accepting the connection and producing the byte-exact echo.

2. **Multi-cert downstream SNI cert selection.** A single listener may declare two or more filter chains with disjoint `filter_chain_match.server_names`. The listener peeks the ClientHello's SNI extension and routes to the filter chain (and therefore the cert) that matches. envoy-rust builds a single rustls `ServerConfig` per listener with a SNI-keyed `ResolvesServerCert` impl that maps SNI → certified key. Fixture 0006 proves cert-selection equivalence: two probes per test, one per SNI, each asserting the post-handshake peer cert's SAN/CN matches the expected value (this assertion lives **inside the harness driver**, not as a new equivalence-matrix dimension; both proxies must select the *same* cert for the *same* SNI for the test to pass).

`crates/envoy-tls/` gains: a SNI-keyed `SniResolver` impl of `rustls::server::ResolvesServerCert`; a new `DownstreamTls::from_listener(listener: &envoy_config::Listener)` constructor that walks all filter chains and builds a single `ServerConfig` whose resolver dispatches by ClientHello SNI. The upstream-TLS *consumer* wiring lands too: `envoy-tcp::TcpProxy` gains an optional `Arc<UpstreamTls>` field; `TcpProxy::handle` wraps the upstream dial in a TLS handshake when `Some`. `envoy-bin` builds the `Arc<UpstreamTls>` per cluster with `transport_socket: Upstream(...)` (the parent-SPEC §3 D4 alternative — keep envoy-cluster rustls-free; envoy-bin orchestrates) and threads it into the `TcpProxy` constructor.

The harness gains: a `Driver::TlsTcpProbeList { probes }` variant; a multi-probe `drive_tls_probes` helper that runs `drive_tls`'s body once per probe; a `TlsEchoBackend` sibling of `TcpProxyBackend` that spawns the new `tls-echo-server` helper binary. Fixtures 0005 and 0006 land. The phase-03 parent ROADMAP row flips to `done` in 03.2's final commit.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 03.2's feature surface (= the full parent-phase-03 acceptance surface, minus the 03.1-already-done subset):

- (a) the new differential fixtures `tests/fixtures/0005-tls-upstream/` and `tests/fixtures/0006-tls-sni/` are green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, and `tests/fixtures/0004-tls-downstream/` remain green;
- (c) no conformance suites run this sub-phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 03.1 (3 TLS seeds) plus 2 new seeds in 03.2 (multi-cert + `server_names`; overlapping-SNI reject case);
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for this sub-phase is approved.

**Scope shape (inherited from parent-phase brainstorm).** Of the four scope-shape forks resolved during the parent-phase-03 state-1 brainstorm, two bind on 03.2:

1. **SNI scope — wire-level + downstream cert selection.** Phase 03.2 ships *both* (a) wire-level SNI on the upstream side (rustls `ClientConfig.server_name` populated from `UpstreamTlsContext.sni`) and (b) downstream cert selection by SNI (rustls `ResolvesServerCert` keyed on the ClientHello SNI extension, mapping to the matching filter-chain's `tls_certificates`). The `envoy.filters.network.sni_cluster` *network filter* (which routes to a cluster *named* by ClientHello SNI) is **not** part of phase 03.2; it belongs to the §9 network-filters family.
2. **Fixture distribution.** Fixtures 0005 (TLS upstream) and 0006 (multi-cert SNI on downstream) land in 03.2. Each property gets its own fixture; failures localize cleanly.

The other two forks (cert provisioning via rcgen + tempfile under ADR-0018; new `envoy-tls` library crate) bound on 03.1 and are inherited as landed.

---

## 2. Behavior-contract scope for sub-phase 03.2

Sub-phase 03.2 exercises only **row 2** of the `BEHAVIOR_CONTRACT.md` §7.2 equivalence matrix:

- **Response body — Byte-exact for deterministic handlers.** Both fixtures use a TCP-echo data plane (fixture 0005's upstream is the new `tls-echo-server`; fixture 0006's upstream is the plaintext `tcp-echo-server` from phase 02.1, with TLS termination on the downstream side). The harness driver helpers (`drive_tls`, `drive_tls_probes`) inherit ADR-0006/0007's `read_exact(payload.len())` + 100ms trailing-byte poll discipline.

Fixture 0006 additionally asserts a structural property — the post-handshake peer certificate's SAN/CN matches the expected value for each SNI probe — but this assertion lives **inside the harness driver** (`drive_tls_probes` interrogates `tokio_rustls::client::TlsStream::get_ref().1.peer_certificates()` and asserts equality against `probes[i].expected_cn`). It is not a new equivalence-matrix dimension; both proxies must select the *same* cert for the *same* SNI for the test to pass, which is exactly the property under test. The matrix row engaged is still row 2 (post-handshake bytes are byte-exact).

No other dimension is engaged. **No `BEHAVIOR_CONTRACT.md` edits in 03.2.** The currently-empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) remain empty.

---

## 3. Deliverables

### D1 — `envoy-tls` SNI multi-cert resolver + `from_listener` constructor

`crates/envoy-tls/src/lib.rs` extends with:

```rust
/// SNI-keyed ResolvesServerCert. Map keys are lowercase per parent-SPEC §6
/// signpost 21 (rustls 0.23's ClientHello::server_name() returns lowercase).
pub struct SniResolver {
    map: std::collections::HashMap<String, std::sync::Arc<rustls::sign::CertifiedKey>>,
    default: Option<std::sync::Arc<rustls::sign::CertifiedKey>>,
}

impl rustls::server::ResolvesServerCert for SniResolver {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<std::sync::Arc<rustls::sign::CertifiedKey>> {
        let sni = client_hello.server_name()?;
        // rustls returns lowercase already; we store lowercase; .get() is direct.
        self.map.get(sni)
            .cloned()
            .or_else(|| self.default.clone())
    }
}

impl DownstreamTls {
    /// 03.2-only: build from a full envoy_config::Listener by walking all filter
    /// chains. For each chain that carries a transport_socket with a
    /// DownstreamTlsContext, load its cert+key into a CertifiedKey; for each
    /// SNI in the chain's filter_chain_match.server_names, insert the key into
    /// the SniResolver's map (keyed lowercase). At most one chain may have an
    /// empty server_names (the catch-all); its key becomes `default`.
    /// The validator already rejects overlapping server_names and multiple
    /// catch-all chains (see D2 below); from_listener trusts those guarantees.
    ///
    /// If any chain in the listener carries TLS, the entire listener is treated
    /// as TLS (rustls multiplexes by SNI inside a single ServerConfig). A
    /// listener that mixes TLS and plaintext chains is rejected by the
    /// validator (a TLS chain and a plaintext chain cannot coexist on one
    /// listener in phase 03 — that pattern requires `tls_inspector` listener
    /// filter, deferred to a later phase).
    pub fn from_listener(listener: &envoy_config::Listener)
        -> Result<Self, TlsError>;
}
```

The `tests/helpers/tls-echo-server/` helper (D7 below) and 03.1's `DownstreamTls::from_context` continue to work side-by-side: `from_context` is the single-cert entry point used when the listener has exactly one filter chain with a `DownstreamTlsContext`; `from_listener` is the multi-chain entry point. envoy-bin (D5) picks based on the listener's filter-chain count.

**Unit tests appended to `crates/envoy-tls/src/lib.rs::tests` (5 new tests):**

- `sni_resolver_routes_known_sni` — map `{"a.example.com" → key_a, "b.example.com" → key_b}`; `resolve(ClientHello{sni: "a.example.com"})` returns `key_a`; same for `b`.
- `sni_resolver_falls_back_to_default_on_miss` — map populated; `default = Some(key_a)`; `resolve(ClientHello{sni: "unknown.example.com"})` returns `key_a`.
- `sni_resolver_returns_none_on_miss_without_default` — map populated; `default = None`; `resolve` returns `None` (which causes rustls to abort the handshake with `unrecognized_name` per RFC 6066 §3 / TLS 1.3 §6.2).
- `sni_resolver_is_case_insensitive` — map keyed lowercase (e.g., `a.example.com`); construct a synthetic `ClientHello` with `sni: "A.Example.com"`; resolver returns the lowercase entry. (Implementation note: rustls 0.23's `ClientHello::server_name()` already returns the lowercased SNI, so the test exercises the contract end-to-end via rustls's parser rather than a hand-rolled cap conversion. If unit-test machinery for synthesizing a `ClientHello` proves awkward, replace with a connection-level integration test that uses `tokio_rustls::TlsConnector`.)
- `from_listener_builds_multi_cert_config` — synthesize an `envoy_config::Listener` with two filter chains (`server_names: ["a.example.com"]` + leaf-A; `server_names: ["b.example.com"]` + leaf-B); call `DownstreamTls::from_listener`; assert the resulting `ServerConfig`'s resolver picks `key_a` for `a.example.com` and `key_b` for `b.example.com` (run two in-process TLS handshakes, one per SNI; assert each handshake's `peer_certificates()[0]` contains the expected SAN).

The 10 unit tests landed in 03.1 (covering `DownstreamTls::from_context`, `UpstreamTls::from_context`, the cert/key loader, single-cert resolver, crypto-provider install idempotence) remain unchanged and continue to pass.

### D2 — `envoy-config` schema extensions (03.2 portion)

`crates/envoy-config/src/bootstrap.rs` gains the `FilterChainMatch` struct and the `server_names` field; `validate` gains the SNI-overlap and catch-all rules.

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    #[serde(default)]
    pub filter_chain_match: Option<FilterChainMatch>,    // 03.2 NEW
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,       // 03.1
    pub filters: Vec<NetworkFilter>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChainMatch {
    /// SNI values this filter chain matches. Empty Vec = catch-all (only one
    /// catch-all filter chain per listener; validator enforces). The validator
    /// also rejects two filter chains declaring the same SNI in their
    /// server_names lists.
    #[serde(default)]
    pub server_names: Vec<String>,
}
```

The `Listener.filter_chains: Vec<FilterChain>` cardinality cap from phase 02.1 (`listeners.len() ∈ {0, 1}`) does **not** govern filter-chain count — that cap is on listeners. 03.1 implicitly expected one filter chain per listener (because no `filter_chain_match` existed); 03.2 lifts that to ≥ 1 with the validator below.

**Validator extensions** in `envoy-config::bootstrap::validate` — new `ConfigError` variants (03.2 portion):

- `MultipleListenersWithOverlappingSni { listener: String, sni: String }` — within one listener, two filter chains may not declare the same value (case-insensitive) in their `server_names`. The variant name is named after the parent-SPEC's `MultipleListenersWithOverlappingSni`, even though the rule is intra-listener (not cross-listener). Plan-writer may rename to `OverlappingFilterChainSni` if the misnomer becomes confusing during execution; either name is acceptable as long as the message clearly identifies the listener and the offending SNI.
- `MultipleCatchAllFilterChains { listener: String }` — within one listener, at most one filter chain may have empty `server_names` (or no `filter_chain_match`).
- `MixedTlsAndPlaintextFilterChainsOnListener { listener: String }` — a listener with multiple filter chains may not mix TLS and plaintext chains in phase 03 (that pattern requires `tls_inspector` listener filter, deferred). Either all chains carry `transport_socket: TLS` or none do. (This rule is loose-fitting: a listener with one filter chain has no mixing concern; the rule fires only when ≥ 2 chains exist and at least one but not all carries TLS.)

The validator also propagates the 03.1-landed rules (`UnknownTransportSocketName`, `MismatchedTransportSocketDirection`, `EmptyTlsCertificates`, `MissingValidationContext`, `EmptyUpstreamSni`) over the new multi-chain shape — each chain's `DownstreamTlsContext` is validated independently.

**Validator unit tests appended to `crates/envoy-config/src/bootstrap.rs::tests` (6 new tests):**

- `parses_listener_with_multi_chain_sni_routing` — full happy-path fixture (listener with two filter chains; chain A `server_names: ["a.example.com"]` + cert A; chain B `server_names: ["b.example.com"]` + cert B; both chains run `tcp_proxy → backend`).
- `parses_filter_chain_with_empty_server_names` — `filter_chain_match: { server_names: [] }` — a single catch-all chain — accepted.
- `rejects_filter_chains_with_overlapping_sni` — two chains both declare `server_names: ["a.example.com"]` → `ConfigError::MultipleListenersWithOverlappingSni`.
- `rejects_filter_chains_with_overlapping_sni_case_insensitive` — chain A declares `["a.example.com"]`, chain B declares `["A.Example.com"]` → same error (matching is case-insensitive on both sides).
- `rejects_multiple_catch_all_filter_chains` — two chains both have empty `server_names` → `ConfigError::MultipleCatchAllFilterChains`.
- `rejects_mixed_tls_and_plaintext_filter_chains` — listener with one TLS chain and one plaintext chain → `ConfigError::MixedTlsAndPlaintextFilterChainsOnListener`.

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 2 new TLS-shaped seeds (03.2):

- `tls_multi_cert_sni.yaml` — listener with two filter chains, each carrying its own `DownstreamTlsContext` and `filter_chain_match.server_names`.
- `tls_overlapping_sni_reject.yaml` — same shape but with overlapping `server_names` (the parser accepts the YAML; the validator's `MultipleListenersWithOverlappingSni` rejects it; `parse_bootstrap` exercises both paths).

### D3 — `envoy-tcp` upstream-TLS dial (03.2 portion of D4)

`crates/envoy-tcp/src/lib.rs` gains an optional upstream-TLS field on `TcpProxy`:

```rust
pub struct TcpProxy {
    cluster: envoy_cluster::ClusterHandle,
    cluster_name: String,
    upstream_tls: Option<std::sync::Arc<envoy_tls::UpstreamTls>>,    // 03.2 NEW
}

impl TcpProxy {
    /// Existing 03.1 constructor — plaintext upstream.
    pub fn new(
        cluster: envoy_cluster::ClusterHandle,
        cfg: &envoy_config::TcpProxyConfig,
    ) -> Self;

    /// 03.2 NEW — TLS upstream.
    pub fn with_upstream_tls(
        cluster: envoy_cluster::ClusterHandle,
        cfg: &envoy_config::TcpProxyConfig,
        upstream_tls: std::sync::Arc<envoy_tls::UpstreamTls>,
    ) -> Self;

    /// `handle` keeps its 03.1 generic signature. The body branches on
    /// `self.upstream_tls.as_ref()` after the upstream TcpStream::connect:
    ///
    ///   let stream = TcpStream::connect(addr).await?;
    ///   let upstream: Box<dyn AsyncReadWrite + Send + Unpin> = match &self.upstream_tls {
    ///       None => Box::new(stream),
    ///       Some(tls) => Box::new(tls.connect(stream).await?),
    ///   };
    ///   // tokio::io::copy bidirectional via try_join! — same as 03.1.
    ///
    /// Where AsyncReadWrite is a local trait alias unifying tokio::io::AsyncRead
    /// + AsyncWrite (declared in envoy-tcp). The boxing is required because the
    /// branch arms produce different concrete types; the alternative (separate
    /// monomorphic copy paths per branch) doubles the bidirectional-copy code.
}
```

`crates/envoy-tcp/Cargo.toml` adds `envoy-tls = { path = "../envoy-tls" }` as a runtime dep (it was a dev-dep only in 03.1 for the new TLS-flavored unit tests).

A new `TcpProxyError::UpstreamTlsHandshake { source }` variant captures upstream-TLS handshake failures (wraps the `envoy_tls::TlsError` returned by `UpstreamTls::connect`). Per phase-02.2's posture, per-connection errors log at `warn!` and drop; the listener stays up.

**Unit tests appended to `crates/envoy-tcp/src/lib.rs::tests` (3 new tests):**

- `proxies_to_tls_upstream_with_valid_cert` — set up an in-process `tokio_rustls::TlsAcceptor` upstream (rcgen-built cert with SAN `envoy-rust.test`; signed by the test CA). Build a `TcpProxy::with_upstream_tls(cluster_pointing_at_acceptor_addr, &cfg, Arc::new(UpstreamTls::from_context(&upstream_ctx)?))` where `upstream_ctx.sni = "envoy-rust.test"` and `validation_context.trusted_ca` points at the CA PEM. Connect a downstream plaintext `TcpStream`; write payload; assert byte-exact round-trip.
- `proxies_returns_err_on_upstream_tls_handshake_fail` — same shape but the acceptor uses a self-signed cert NOT signed by the configured trust bundle. Assert the error type is `TcpProxyError::UpstreamTlsHandshake`.
- `proxies_to_tls_upstream_sends_sni_in_client_hello` — run an in-process `tokio_rustls::TlsAcceptor` whose `ResolvesServerCert` impl captures the ClientHello's SNI value into a `Mutex<Option<String>>`. After the round-trip, assert the captured SNI equals the configured `UpstreamTlsContext.sni` value (proves wire-level SNI is sent).

The 4 unit tests landed in 03.1 (covering generic-stream + plaintext path regression + downstream-TLS via `TlsStream<TcpStream>`) remain unchanged and continue to pass. The original 4 phase-02.2 unit tests also remain unchanged.

### D4 — Phase-02.2 REVIEW M1 (`Cluster::name()` accessor) — opportunistic close-out

Parent-SPEC §1's baked-in defaults default-defer this carryforward to phase 06. 03.2 evaluates **opportunistically** during execution: if any 03.2 task introduces upstream-TLS error attribution that would benefit from the cluster name in the error string (e.g., `TcpProxyError::UpstreamTlsHandshake { cluster: String, source: ... }` would be more informative than the bare `source`-only variant), close the carryforward in-execution per phase-02.2 task-11 precedent (which closed phase-02.1 REVIEW M3 opportunistically).

If 03.2 execution finds no use case, the carryforward stays open and forwards unchanged to phase 06.

The PROGRESS.md at execution time documents the decision either way (closed-in-03.2 or remained-deferred), with the 03.2 REVIEW.md §3 cross-reference. No decision is made at SPEC time; the 03.2 plan-writer also does not commit either way — it lists the carryforward as an "evaluate during execution" item under the M1 task signpost.

### D5 — `envoy-bin` wiring (03.2 portion)

`crates/envoy-bin/src/main.rs::run` extends 03.1's per-listener pre-pass:

1. **Listener walk for multi-cert dispatch.** For each listener:
   - If `listener.filter_chains.len() == 1` and the single chain carries `transport_socket: Downstream(_)` → 03.1 path: build `DownstreamTls::from_context(&ctx)?`. Single-cert.
   - If `listener.filter_chains.len() ≥ 1` and any chain carries `transport_socket: Downstream(_)` and at least one chain has `filter_chain_match.server_names` populated → multi-cert path: build `DownstreamTls::from_listener(&listener)?`. The validator already rejected overlapping SNIs, multiple catch-alls, and TLS/plaintext mixing within a single listener.
   - If `listener.filter_chains.len() == 1` and the single chain has no `transport_socket` → 03.1 plaintext path; unchanged.
   - The `TlsAcceptingHandler` wrapping (03.1 D5) accepts an `Arc<DownstreamTls>` regardless of whether it was built via `from_context` or `from_listener` — the wrapping logic is unchanged in 03.2.

   Note: 03.1's plaintext-listener path (where the entire listener has no `transport_socket` on any chain) remains the fast path — no `DownstreamTls` is constructed.

2. **Per-cluster upstream-TLS construction.** For each cluster in `bootstrap.static_resources.clusters`:
   - If `cluster.transport_socket` is `None` → plaintext upstream. Build `TcpProxy::new(cluster_handle, &tcp_proxy_cfg)` (03.1 path; unchanged).
   - If `cluster.transport_socket` is `Some(TransportSocket { name: "envoy.transport_sockets.tls", typed_config: TransportSocketTypedConfig::Upstream(ctx) })` → build `Arc<UpstreamTls>` once via `envoy_tls::UpstreamTls::from_context(&ctx)?`. Build `TcpProxy::with_upstream_tls(cluster_handle, &tcp_proxy_cfg, upstream_tls)`.
   - The validator already rejected `Downstream(_)` on a cluster's `transport_socket` (`MismatchedTransportSocketDirection { side: "cluster" }`), so the `Downstream(_)` arm is unreachable here.

3. **Wiring composition.** envoy-bin's `run` constructs a per-cluster `Arc<TcpProxy>` (or per-cluster TLS-wrapped variant) once at startup, then per-listener constructs the `Arc<DownstreamTls>` (single or multi) and the `TlsAcceptingHandler` if TLS, and threads everything into `Listener::bind`. Match on filter-chain count + transport_socket presence drives the dispatch; the validator's guarantees keep the match arms exhaustive without falling through to runtime errors.

**Integration tests in `crates/envoy-bin/tests/`** (Docker-free, in-process backstops to fixtures 0005 and 0006; same shape as 03.1's `tls_downstream.rs`):

- `crates/envoy-bin/tests/tls_upstream.rs` — builds a `TlsTestPki`-style PKI in a per-test tempdir; spawns an in-process `tokio_rustls::TlsAcceptor` upstream that echoes; spawns `envoy-bin` as a subprocess (via `CARGO_BIN_EXE_envoy-bin`) with a config that points at the in-process upstream's address and includes `UpstreamTlsContext` referring to the test CA; opens a plaintext TCP connection to envoy-bin's listener; writes payload; `read_exact`; asserts byte-equality. Plaintext downstream; TLS upstream.
- `crates/envoy-bin/tests/tls_sni.rs` — same setup but with two filter chains on a single listener (chain A with cert A + `server_names: ["a.example.com"]`; chain B with cert B + `server_names: ["b.example.com"]`), routing to a single plaintext echo backend. Open two TLS connections: one with SNI `a.example.com`, asserting the post-handshake peer cert's SAN contains `a.example.com`; one with SNI `b.example.com`, asserting `b.example.com`. Two probes per test invocation.

### D6 — Differential harness extensions (03.2 portion)

- **`Driver::TlsTcpProbeList`** (in `tests/differential/src/lib.rs`):

    ```rust
    pub enum Driver {
        TcpEcho,                                                        // unchanged
        HttpGet { path: String },                                       // unchanged
        TlsTcp { sni: String, expected_cn: Option<String> },            // 03.1
        TlsTcpProbeList { probes: Vec<TlsTcpProbe> },                   // 03.2 NEW
    }

    #[derive(Clone, Debug, serde::Deserialize, PartialEq)]
    pub struct TlsTcpProbe {
        pub sni: String,
        pub expected_cn: Option<String>,
    }
    ```

  Existing `Driver::TcpEcho`, `Driver::HttpGet`, and `Driver::TlsTcp` are unchanged.

- **`drive_tls_probes(addr, payload, probes, root_store) -> anyhow::Result<()>`** — opens one connection per probe, runs `drive_tls`'s body once per connection (write payload, `read_exact(payload.len())`, byte-equality assertion, optional `expected_cn` SAN/CN check, ADR-0007 trailing-byte poll, graceful shutdown). All probes run sequentially against the same `addr` (the listener's port); each probe's TLS handshake brings its own SNI and gets the matching cert.

- **`TlsEchoBackend`** in `tests/differential/src/backend.rs` (sibling of `TcpProxyBackend`):

    ```rust
    pub struct TlsEchoBackend {
        port: u16,
        child: tokio::process::Child,
        _server_pem_paths: PathBufPair,    // (cert, key) — kept alive for child's lifetime
    }

    impl TlsEchoBackend {
        pub async fn spawn(server_cert: &Path, server_key: &Path) -> anyhow::Result<Self>;
        pub fn port(&self) -> u16;
        pub fn container_host(&self) -> &'static str;    // "host.docker.internal"
    }

    impl Drop for TlsEchoBackend { /* same SIGKILL-on-Drop posture as TcpProxyBackend */ }
    ```

  Same `Drop` posture as `TcpProxyBackend` per phase 02.2 (per the M1 carryforward, the polling loop blocks on `std::thread::sleep` from a tokio-runtime thread — known issue, tracked forward to whichever phase first parallelizes `run_fixture`; 03.2 inherits without changes).

- **`render_yaml` per-driver substitution.** New keys for 03.2 fixtures:
  - `{{LEAF_B_CERT_PATH}}`, `{{LEAF_B_KEY_PATH}}` — fixture 0006 multi-cert.
  - `{{SERVER_CERT_PATH}}`, `{{SERVER_KEY_PATH}}` — fixture 0005 (paths to the `tls-echo-server`'s cert/key, which are the harness PKI's `server_*` PEMs).
  - `{{TLS_BACKEND_PORT}}` — fixture 0005's TLS upstream port (separate from `{{BACKEND_PORT}}` since fixtures may use both — though 0005 only uses `TLS_BACKEND_PORT`).

  Same `envoy_side_paths()` / `subject_side_paths()` machinery from 03.1; just more keys.

- **`run_fixture` dispatch.** Detection cascade extended:
  1. If either rendered template references `{{CA_PATH}}` / `{{LEAF_*_PATH}}` / `{{SERVER_*_PATH}}`, build `TlsTestPki::generate()?` (03.1 path; existing).
  2. If either rendered template references `{{BACKEND_PORT}}`, spawn `TcpProxyBackend` (phase 02.2 path; existing).
  3. (03.2 NEW) If either rendered template references `{{TLS_BACKEND_PORT}}`, spawn `TlsEchoBackend::spawn(pki.subject_side_paths()["{{SERVER_CERT_PATH}}"], ...key)`; fill the substitution.
  4. Pass `tls_pki: Option<&TlsTestPki>` into `upstream::start` (03.1 path; unchanged in 03.2 except the path list now includes leaf-B and server PEMs when fixture 0006/0005's templates require them).

- **Harness unit tests** in `tests/differential/src/{tls,backend,lib}.rs::tests` (2 new tests, 03.2):
  - `tls_echo_backend_spawns_and_echoes` — spawn a `TlsEchoBackend`, connect via `tokio_rustls::TlsConnector` (CA in root store; SNI `envoy-rust.test`), round-trip a payload, assert byte-equality. Mirrors phase-02.2's `tcp_proxy_backend_spawns_and_echoes` test.
  - `tls_echo_backend_drop_terminates_child` — spawn, drop, assert child process exited.

  The 4 harness tests landed in 03.1 (`tls_test_pki_generates_valid_chain`, `tls_test_pki_drop_removes_tmpdir`, `render_yaml_substitutes_tls_paths_for_envoy_side`, `render_yaml_substitutes_tls_paths_for_subject_side`) remain unchanged.

- **Integration tests** `tests/differential/tests/tls_upstream.rs` and `tests/differential/tests/tls_sni.rs` — both Docker-gated, same `#[ignore]`-unless-`DOCKER=1` pattern. Each calls `run_fixture("0005-tls-upstream")` / `run_fixture("0006-tls-sni")`.

### D7 — New helper crate `tests/helpers/tls-echo-server/`

Sibling of `tests/helpers/tcp-echo-server/` (landed in phase 02.1). Same skeleton, with TLS termination on top.

- `tests/helpers/tls-echo-server/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps: `anyhow`, `thiserror`, `tokio` (features `rt-multi-thread`, `net`, `io-util`, `macros`, `signal`), `tokio-rustls` (default-features=false, features=["aws-lc-rs"]; covered by ADR-0019), `rustls = "0.23"` (default-features=false, features=["std","tls12"]), `rustls-pki-types = "1"`, `rustls-pemfile = "2"` (covered by ADR-0019), `tracing`, `tracing-subscriber`. Same dep set as `tcp-echo-server` plus the rustls glue.

  Dev-deps: none beyond the runtime deps (the unit tests below use the runtime cert-loading code with dev-only PEMs generated via `rcgen` + `tempfile`, so add `rcgen = "0.13"` and `tempfile = "3"` as dev-deps under ADR-0018).

- `tests/helpers/tls-echo-server/src/main.rs` starts with `#![forbid(unsafe_code)]`. Contract:
  - Hand-parsed argv mirroring `tcp-echo-server`'s shape from phase 02.1: `--port <u16>` (required), `--cert <path>` (required, leaf cert PEM), `--key <path>` (required, private-key PEM), `--help`, `--version`. `ArgvError` typed via `thiserror` (variants: `MissingFlag(&'static str)`, `MissingValue`, `InvalidPort`, `Trailing`, `HelpRequested`, `VersionRequested`).
  - Runtime: install `aws_lc_rs::default_provider().install_default()` once (ignore `Err` second-call return); load the cert + key via `rustls-pemfile`; build a `ServerConfig` with a single-cert `ResolvesServerCert` (no SNI multiplexing — `tls-echo-server` is single-purpose); `tokio::net::TcpListener::bind(("127.0.0.1", port))`; accept loop with `tokio::select!` between `accept()` and `tokio::signal::ctrl_c()`; for each accepted stream, spawn onto a `tokio::task::JoinSet`: run `tokio_rustls::TlsAcceptor::accept(stream).await?`; post-handshake, `let (mut r, mut w) = tokio::io::split(tls_stream); tokio::io::copy(&mut r, &mut w).await`.
  - On shutdown: stop accepting, drain with `DRAIN_BUDGET = Duration::from_secs(5)`, abort stragglers, return 0.
  - Logs on `stderr` via `tracing_subscriber::fmt`.
  - Exit codes: `0` clean, `1` runtime error, `2` argv error. Mirrors `envoy-bin`'s and `tcp-echo-server`'s argv-vs-runtime-vs-clean exit-code convention.

- ~120 LoC of impl.

- Unit tests in `tests/helpers/tls-echo-server/src/main.rs::tests` (5 tests):
  - `argv_parses_full_invocation` — `--port 10042 --cert /tmp/c.pem --key /tmp/k.pem` → `Ok(Args { port: 10042, cert: ..., key: ... })`.
  - `argv_rejects_missing_cert` — `--port 10042 --key /tmp/k.pem` → `Err(ArgvError::MissingFlag("--cert"))`.
  - `argv_rejects_missing_key` — `--port 10042 --cert /tmp/c.pem` → `Err(ArgvError::MissingFlag("--key"))`.
  - `argv_shows_help` — `--help` → `Err(ArgvError::HelpRequested)` (exit 0 path via main's translation).
  - `accepts_and_echoes_via_tls` — `#[tokio::test(flavor="multi_thread")]`: `rcgen`-build a CA + leaf in a tmpdir; spawn the server in a task on a reserved port pointing at those PEMs; connect via `tokio_rustls::TlsConnector` configured with the CA; write 32-byte payload; `read_exact` 32 bytes; assert equal.

### D8 — Differential fixtures

#### Fixture `tests/fixtures/0005-tls-upstream/`

**Property.** Plaintext downstream; upstream TLS origination from envoy-rust to the new `tls-echo-server` helper. Wire-level SNI is sent to the upstream (server_name in ClientHello = `UpstreamTlsContext.sni`).

Files:

- `envoy.yaml`:

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

- `envoy-rust.yaml` — same shape with the per-side divergences (bind `127.0.0.1`, no admin block, backend host `127.0.0.1`, CA path from `subject_side_paths()`).

- `inputs/payload.bin` — fixture-0001 payload, byte-identical.

- `expectations.yaml`:

    ```yaml
    driver:
      kind: tcp_echo
    equivalence:
      response_body: byte_exact
    ```

  Note: the driver is `tcp_echo`, not `tls_tcp` — fixture 0005's *downstream* is plaintext; the harness opens a plain TCP connection to envoy-bin's listener. The TLS happens on the *upstream* side, which the harness doesn't reach into directly; it's exercised end-to-end via the byte round-trip succeeding (the `tls-echo-server` is the only entity the upstream byte stream reaches, and it requires a successful TLS handshake to echo).

- `README.md` — names the property (upstream TLS origination, including SNI on the wire), the `tls-echo-server` helper's role, the validation-against-the-harness-CA posture, and ADR references (ADR-0015 cross-container-host reachability, ADR-0017 split decision, ADR-0018 rcgen+tempfile dev-test-harness, ADR-0019 tokio-rustls+rustls-pemfile under rustls grant).

#### Fixture `tests/fixtures/0006-tls-sni/`

**Property.** Downstream TLS with multi-cert SNI cert selection. One listener; two filter chains; each chain carries a different cert keyed on its `filter_chain_match.server_names`. Plaintext upstream backend (`tcp-echo-server` from phase 02.1).

Files:

- `envoy.yaml`:

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

  Both chains route to the same `backend` cluster — the differential property is *cert selection*, not routing.

- `envoy-rust.yaml` — same shape with per-side divergences.

- `inputs/payload.bin` — fixture-0001 payload.

- `expectations.yaml`:

    ```yaml
    driver:
      kind: tls_tcp_probe_list
      probes:
        - { sni: "a.example.com", expected_cn: "a.example.com" }
        - { sni: "b.example.com", expected_cn: "b.example.com" }
    equivalence:
      response_body: byte_exact
    ```

- `README.md` — names the property, the SNI resolution mechanism (rustls `ResolvesServerCert` keyed on lowercase SNI; case-insensitive exact match; envoy-side mirrors via `filter_chain_match.server_names`), the per-probe round-trip, the ADR references, and explicitly notes the unknown-SNI close behavior is **not** asserted in this fixture (parent-SPEC §6 signpost 8 — adding a third probe with `expected_close: bool` is a future-fixture option).

### D9 — CI workflow

`.github/workflows/ci.yml` changes: **none** in 03.2. The existing `build` job runs `cargo test --workspace`, which picks up the new `tls-echo-server` helper crate automatically. The existing `fuzz` job exercises the further-extended `parse_bootstrap` corpus via the same `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` invocation.

The Docker-gated integration tests `tests/differential/tests/tls_upstream.rs` and `tests/differential/tests/tls_sni.rs` run under the same `#[ignore]`-unless-`DOCKER=1` gating pattern.

### D10 — ADRs to land during execution

**No anticipated ADRs in 03.2.** ADR-0017 (split decision), ADR-0018 (rcgen+tempfile), and ADR-0019 (tokio-rustls+rustls-pemfile) all landed in or before 03.1.

Possible additional ADRs (only land if execution proves they're needed; not anticipated):

- **TLS protocol-version pin** if rustls and Envoy v1.33.0 negotiate differently in a way the differential harness catches on fixture 0005 or 0006. The fix is `tls_params { tls_minimum_protocol_version: TLSv1_3, tls_maximum_protocol_version: TLSv1_3 }` on both sides + a rustls `ClientConfig` / `ServerConfig` built with `with_protocol_versions(&[&rustls::version::TLS13])`.
- **Wildcard SNI semantics** if Envoy v1.33.0 supports `*.example.com` in `filter_chain_match.server_names` and the multi-cert resolver needs to match it. Land an ADR pinning the policy if it surfaces.
- **`testcontainers` mount API extension** if v0.23's `with_copy_to_container` proves awkward for the per-fixture tmpdir at the multi-PEM scale of fixtures 0005 + 0006.
- **`Cluster::name()` accessor** opportunistic close-out (per D4 above) lands as a doc cross-reference in PROGRESS.md + the 03.2 REVIEW; only an ADR if a posture decision (e.g., field naming convention, error attribution shape) is worth recording — likely not.

If `cargo deny check` flips red on any new transitive license (most likely from `tls-echo-server`'s newly-introduced `tokio-rustls` direct dep — though already covered in envoy-tls + envoy-bin's deps from 03.1, so no new transitive surface), land it under a new ADR (likely ADR-0020) at the time it trips.

---

## 4. Non-goals (deferred to later phases)

Out of phase 03 entirely (carries forward unchanged from parent-SPEC §4):

- **HTTP-over-TLS** — phase 04.
- **mTLS** — out of phase 03.
- **Inline cert / key bytes** — phase 03 supports `filename` only.
- **`tls_params`** — fixture YAMLs omit; rely on rustls + Envoy defaults.
- **`auto_sni`, `auto_san_validation`, `allow_renegotiation`, `key_rotation`, `session_timeout`, `session_tickets`, `validation_context.match_typed_subject_alt_names`** — out of phase 03.
- **OCSP stapling, signed certificate timestamps, certificate transparency** — out of MVP trunk.
- **xDS-driven SDS** — §9 family.
- **Unknown-SNI close behavior assertion in a fixture** — signposted in parent-SPEC §6 signpost 8; not asserted in fixture 0006. A future fixture may add a third probe with `expected_close: bool`.
- **`envoy.filters.network.sni_cluster`** — §9 network-filters family.
- **Distribution-equivalence on round-robin LB** — parent-brainstorm Q1 unit-test-only.
- **Multiple upstream certificates / SNI on the upstream side per cluster** — out of phase 03; one cluster, one upstream `sni`. A future fixture with per-endpoint SNI lands its own ADR.
- **Listener filters (`listener_filters`)** — out of phase 03 (e.g., `tls_inspector` for SNI-without-termination).
- **Filter chain framework / extension registry / per-route TLS config** — phase 07.
- **Stats subsystem, access logs, Prometheus** — phase 06.
- **Admin endpoints beyond phase 01's `/ready`** — phase 08.
- **`type: LOGICAL_DNS`, `type: STRICT_DNS`, `type: EDS`** — phase 03 still accepts only `STATIC`.
- **`lb_policy` variants beyond `ROUND_ROBIN`** — §9 load-balancing family.

---

## 5. Splitting guidance for the planner

Estimated scope:

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-tls extensions (multi-cert SNI `SniResolver` + `from_listener` constructor + 5 unit tests) | ~150 + ~100 |
| envoy-config schema additions (`FilterChainMatch` + `server_names` + overlap rules + catch-all rules + mixed-TLS-plaintext rule + 6 validator tests + 2 fuzz-corpus seeds) | ~80 + ~60 + ~60 |
| envoy-tcp upstream-TLS plumbing (`Option<Arc<UpstreamTls>>` field + `with_upstream_tls` ctor + branched dial in `handle` + `UpstreamTlsHandshake` error variant + 3 envoy-tcp tests) | ~100 + ~80 |
| envoy-bin wiring (multi-cert listener dispatch + per-cluster upstream-TLS construction + 2 in-process integration tests `tls_upstream.rs` + `tls_sni.rs`) | ~80 + ~150 |
| Harness `Driver::TlsTcpProbeList` + multi-probe `drive_tls_probes` + `TlsEchoBackend` + render_yaml extensions (leaf-B, server, TLS_BACKEND_PORT) + run_fixture dispatch + upstream-mount extension for 0005/0006 + 2 unit tests | ~120 + ~80 |
| `tls-echo-server` helper crate (~120 impl + 5 tests including TLS round-trip) | ~120 + ~120 |
| Fixture 0005 (5 files) | ~80 |
| Fixture 0006 (5 files) | ~120 |
| Phase-02.2 REVIEW M1 (`Cluster::name()` accessor) — opportunistic, evaluated at execution; not load-bearing | ~5 (only if 03.2 surfaces a need) |
| Docker-gated integration tests `tls_upstream.rs` + `tls_sni.rs` (in `tests/differential/tests/`) | ~40 |
| **Total** | **~1445 LoC; ~14 tasks** |

Both `BOOTSTRAP_PROMPT.md` §6.1 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably at ~14 tasks / ~1445 LoC. **Do not split 03.2 further**. If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of an already-split sub-phase were not anticipated at the parent-phase brainstorm and deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition). Parent-SPEC §5's identical guidance applies here verbatim.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution.

1. **Task ordering for 03.2.** envoy-config schema additions (D2) + 6 validator tests + 2 fuzz-corpus seeds → envoy-tls multi-cert SNI resolver + `from_listener` (D1) + 5 unit tests → envoy-tcp upstream-TLS dial (D3) + 3 unit tests → envoy-bin multi-cert + upstream TLS wiring (D5) + in-process integration tests `tls_upstream.rs` + `tls_sni.rs` → harness `Driver::TlsTcpProbeList` + `drive_tls_probes` + `TlsEchoBackend` (D6) → `tls-echo-server` helper crate (D7) + 5 unit tests → fixtures 0005 + 0006 (D8) + Docker-gated integration tests → state-4 phase-done gate. If the M1 carryforward (`Cluster::name()` accessor) opportunistically closes during envoy-tcp upstream-TLS error attribution work, fold it into that task block per phase-02.2 task-11 precedent.

2. **Crypto provider initialization is per-process.** `tls-echo-server`'s `main` calls `aws_lc_rs::default_provider().install_default()` once early, ignoring the `Err` second-call return. envoy-bin already calls it (landed in 03.1). `drive_tls_probes` (in the harness) does not need to call it because `drive_tls` already does in 03.1. Same idempotent-ignore-`Err` pattern across all three call sites.

3. **`SniResolver`'s map is keyed lowercase.** rustls 0.23's `ClientHello::server_name()` returns a lowercased `&str`; the `SniResolver`'s map is keyed lowercase; `from_listener` lowercases each `server_names` entry before insertion. The case-insensitive validator rule is mechanically the lowercase-eq rule. Tests assert this contract end-to-end.

4. **Unknown-SNI handling on a multi-cert listener.** Envoy v1.33.0's behavior: with `filter_chain_match.server_names` and no catch-all chain, an unknown SNI causes the connection to be closed (filter chain selection returns no match → listener drops). rustls's behavior: an unknown SNI in `SniResolver::resolve` returning `None` causes the rustls handshake to abort with TLS alert `unrecognized_name`. Both end states are "handshake fails, connection drops" but the wire alert differs. Phase-03.2 fixture 0006 **does not assert** the unknown-SNI close behavior. Adding a third probe with `expected_close: bool` to `Driver::TlsTcpProbeList` is a future option that lands its own ADR (the TLS-alert delta vs. plain-close delta is potentially divergent between rustls and Envoy — better to reach for it once a fixture genuinely needs it).

5. **`#![forbid(unsafe_code)]` is mandatory** at every new crate's `lib.rs` / `main.rs`: `tests/helpers/tls-echo-server/src/main.rs`. Same as 03.1's discipline for `crates/envoy-tls/src/lib.rs`.

6. **Workspace membership.** Root `Cargo.toml` `[workspace] members` grows by `tests/helpers/tls-echo-server` (03.2). `crates/envoy-tls` was added in 03.1.

7. **rustls `ResolvesServerCert::resolve` is synchronous.** It does not have access to async I/O. For phase 03.2 this is fine — the `SniResolver`'s map is built once at startup from already-loaded certs by `from_listener`. If a future phase needs SDS-backed cert lookup (xDS family), the resolver becomes a `Send + Sync` smart pointer to a snapshot that an async task swaps under a `parking_lot::RwLock` or `arc-swap`; that's deferred entirely.

8. **Cert lifetime in tests.** `TlsTestPki::generate()`'s `_tmpdir: TempDir` keeps PEMs alive for `run_fixture`'s entire duration. Drop fires after both proxies tear down (the upstream container is stopped first by testcontainers' Drop, then the envoy-rust subprocess by the harness's `Subject::Drop`, then the `TlsEchoBackend` Drop in 0005, then `TlsTestPki` Drop). Mirrors `TcpProxyBackend`'s lifetime ordering from phase 02.2.

9. **ALPN absence.** Fixture YAMLs **do not** include `alpn_protocols`. envoy-tls's `ServerConfig` and `ClientConfig` builders **do not** call `with_alpn_protocols`. Phase 04 (HCM HTTP/1.1) is the first phase to add ALPN; phase 05 (HTTP/2) makes it load-bearing. Review should flag any phase-03.2 PR that "defensively" adds an ALPN list.

10. **Half-close posture (ADR-0016) carries forward unchanged.** `enable_half_close: false` is Envoy's v1.33.0 default for `tcp_proxy`. Phase-03.2 fixtures do not include the key; envoy-rust's `TcpProxy::handle` (still generic over the stream type, with the new branched-dial body for upstream TLS) preserves the `tokio::select!`-over-two-`tokio::io::copy`-futures shape from phase 02.2.

11. **`expected_cn` matching policy in `drive_tls_probes`.** Walk both `subject_alt_name` (DNS entries) and CommonName; case-insensitive exact match. Wildcard SAN values (`*.example.com`) are not generated by `TlsTestPki` in phase 03.2, so no wildcard-match policy is needed. Same as 03.1.

12. **Envoy `validation_context.match_subject_alt_names` deferral.** Envoy supports SAN-equality and SAN-prefix matching against the upstream cert's SAN. envoy-rust's phase-03 `UpstreamTls` does **not** support this — rustls's default verifier validates the cert chain against the trust bundle and asserts the cert's SAN matches the configured `ServerName`. That covers Envoy's behavior when `validation_context.match_subject_alt_names` is omitted, which is fixture 0005's posture. Adding `match_subject_alt_names` is a future-phase ADR.

13. **TLS handshake errors in `Listener::serve` and `TcpProxy::handle` are dropped, not propagated.** Per the phase-02.2 + 03.1 posture, per-connection TLS handshake failures (downstream or upstream) log at `warn!` and drop; the listener stays up. The integration tests do **not** assert on log content; they only assert end-state successful handshakes complete byte round-trips.

14. **Listener filter chain ordering and matching (envoy v1.33.0).** Envoy walks filter chains in declaration order; first match wins. The validator does not allow two filter chains to match the *same* SNI (`MultipleListenersWithOverlappingSni`), so first-match is unambiguous. The catch-all (empty `server_names`) is at most one chain (`MultipleCatchAllFilterChains`) and matches when no preceding chain's `server_names` match. Phase-03.2 fixture 0006 has no catch-all chain — both chains have explicit `server_names` — so unknown-SNI fails the handshake (signpost 4).

15. **Multi-cert from_listener vs. single-cert from_context dispatch.** envoy-bin (D5) picks `DownstreamTls::from_listener(listener)` when the listener has multiple filter chains *or* any chain has `filter_chain_match.server_names` populated; otherwise `from_context(&ctx)` for the single-chain single-cert path. The two constructors produce equivalent `ServerConfig`s in the trivial case (single chain, no server_names) — the dispatch is for code clarity, not correctness. Reviewer should not flag the redundancy: the single-cert constructor is the 03.1-shipped entry point and stays in service for fixtures 0004 + 0005 (0005's downstream is plaintext, but the construct is unused there).

16. **`tls-echo-server` uses single-cert resolver, not SNI multiplexing.** Even though `crates/envoy-tls` ships a `SniResolver`, the `tls-echo-server` helper does not use it — it's a single-purpose echo server with one cert. Plan-writer copies the single-cert path from `DownstreamTls::from_context` into `tls-echo-server`'s `main`, or factors out a shared helper `build_single_cert_server_config(cert, key) -> Result<ServerConfig, TlsError>` in envoy-tls. The factor-out is optional — both paths work — but the helper-function shape avoids drift and is the recommended choice. The single-cert resolver impl (`SingleCertResolver`) was already factored out in 03.1, so the helper-function shape is mostly already there.

17. **`render_yaml`'s key set grows — keep it monotone-non-decreasing per fixture.** Each fixture references a subset of `{{LEAF_A_*}}, {{LEAF_B_*}}, {{SERVER_*}}, {{CA_*}}, {{TLS_BACKEND_PORT}}, {{BACKEND_*}}, {{PORT}}, {{ADMIN_PORT}}}`. The harness substitution machinery checks each key for presence in the template before resolving (mechanical string-contains); missing keys are left untouched (or substituted to a sentinel that fails the YAML parse loudly — pick one in execution). Plan-writer picks the unsubstituted-leave-in-template-then-fail-parse-loudly approach: if a fixture references `{{LEAF_B_CERT_PATH}}` and the harness fails to substitute it, the YAML parses fail with the literal `{{LEAF_B_CERT_PATH}}` in an `address:` field (or wherever) — easy to debug, harder to silent-pass. Mirrors phase-02.2's approach.

18. **Testcontainers mount fan-out for fixture 0006.** Fixture 0006 references both `{{LEAF_A_CERT_PATH}}` + `{{LEAF_A_KEY_PATH}}` and `{{LEAF_B_CERT_PATH}}` + `{{LEAF_B_KEY_PATH}}`. The harness must `with_copy_to_container` all four PEMs. The mount loop walks the substitution map; if the key set is non-empty, mount each PEM at its `envoy_side_path` in `/etc/envoy-rust-tls/<filename>.pem`.

19. **`Cluster::name()` accessor evaluation.** Per D4 above, evaluate at execution. If it surfaces a use case (likely upstream-TLS error attribution: `TcpProxyError::UpstreamTlsHandshake { cluster: String, ... }`), close the carryforward; document in PROGRESS.md + REVIEW §3 with the cross-reference to phase-02.1 REVIEW M1 + phase-02.2 REVIEW §4 recommendation 1. If it does not surface a use case, the carryforward forwards to phase 06 unchanged — no decision needs to be made at SPEC time.

20. **anyhow boundary at envoy-bin's integration tests.** `crates/envoy-bin/tests/tls_upstream.rs` and `crates/envoy-bin/tests/tls_sni.rs` are in the binary crate's package and may use `anyhow` (D-3.2 permits `anyhow` only in `envoy-bin`). The `tests/differential/` crate cannot use `anyhow` for new code beyond what was already established in phase 00 — but `drive_tls_probes` returning `anyhow::Result<()>` is consistent with 03.1's `drive_tls` posture and with phase-00's harness-wide `anyhow` usage.

---

## 7. ADRs expected from this sub-phase

**No anticipated ADRs.** ADR-0017 (split decision), ADR-0018 (rcgen+tempfile dev-test-harness-only), and ADR-0019 (tokio-rustls+rustls-pemfile under the rustls grant) all landed in 03.1 (or, for ADR-0017, at the parent-phase state-2 plan-writer commit that landed both sub-phase SPECs).

Possible additional ADRs land only if execution proves they're needed (per D-3.5 ambiguity-resolution discipline). Likely candidates if any:

- **TLS protocol-version pin** (likely ADR-0020) if rustls and Envoy v1.33.0 negotiate differently.
- **Wildcard SNI semantics** if Envoy v1.33.0 supports `*.example.com` in `server_names` and the resolver needs the policy.
- **`cargo deny` exemption** for any new transitive license surface (most likely a no-op since rustls + tokio-rustls + aws-lc-rs are already in scope from 03.1).
- **`Cluster::name()` accessor** opportunistic close-out (per D4 above) — typically lands as a doc cross-reference, not an ADR, unless a posture decision is worth recording.

If any of these fire, they take the next-sequential available ADR number at the time they land.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/03.2-tls-upstream-sni/PLAN.md`
- `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`
- `docs/envoy-rust/phases/03.2-tls-upstream-sni/REVIEW.md`
- `tests/helpers/tls-echo-server/Cargo.toml`
- `tests/helpers/tls-echo-server/src/main.rs`
- `crates/envoy-bin/tests/tls_upstream.rs`
- `crates/envoy-bin/tests/tls_sni.rs`
- `tests/differential/tests/tls_upstream.rs`
- `tests/differential/tests/tls_sni.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_multi_cert_sni.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_overlapping_sni_reject.yaml`
- `tests/fixtures/0005-tls-upstream/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}`
- `tests/fixtures/0006-tls-sni/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}`

Amended during execution:

- Root `Cargo.toml` — add `tests/helpers/tls-echo-server` to `[workspace] members`. (`crates/envoy-tls` is already there from 03.1.)
- `crates/envoy-tls/src/lib.rs` — add `SniResolver` struct + `ResolvesServerCert` impl; add `DownstreamTls::from_listener` constructor; add 5 new unit tests (`sni_resolver_routes_known_sni`, `sni_resolver_falls_back_to_default_on_miss`, `sni_resolver_returns_none_on_miss_without_default`, `sni_resolver_is_case_insensitive`, `from_listener_builds_multi_cert_config`).
- `crates/envoy-config/src/bootstrap.rs` — add `FilterChainMatch` struct + `server_names` field on `FilterChain`; extend `validate` with `MultipleListenersWithOverlappingSni`, `MultipleCatchAllFilterChains`, `MixedTlsAndPlaintextFilterChainsOnListener` `ConfigError` variants; 6 new validator unit tests.
- `crates/envoy-config/src/lib.rs` — re-export `FilterChainMatch`; extend `ConfigError` enum.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — 2 new TLS-shaped seeds (listed under "Created" above).
- `crates/envoy-tcp/src/lib.rs` — add `Option<Arc<UpstreamTls>>` field on `TcpProxy`; add `with_upstream_tls` constructor; extend `handle` body with the upstream-TLS branched dial; add `TcpProxyError::UpstreamTlsHandshake { source }` variant; 3 new unit tests.
- `crates/envoy-tcp/Cargo.toml` — promote `envoy-tls` from dev-dep to runtime dep.
- `crates/envoy-bin/src/main.rs` — extend listener-walk to dispatch between `DownstreamTls::from_context` (single chain, no server_names) and `DownstreamTls::from_listener` (multi chain or any server_names); add per-cluster `Arc<UpstreamTls>` construction loop; thread `upstream_tls` into `TcpProxy::with_upstream_tls`.
- `tests/differential/src/lib.rs` — add `Driver::TlsTcpProbeList` variant + `TlsTcpProbe` struct; add `drive_tls_probes` helper; extend `render_yaml` substitution-key map with `{{LEAF_B_*}}` / `{{SERVER_*}}` / `{{TLS_BACKEND_PORT}}`; extend `run_fixture` dispatch on `{{TLS_BACKEND_PORT}}` to spawn `TlsEchoBackend`.
- `tests/differential/src/backend.rs` — add `TlsEchoBackend` (sibling of `TcpProxyBackend`); 2 new unit tests.
- `tests/differential/src/upstream.rs` — extend testcontainers config to mount the additional PEMs (leaf-B for fixture 0006; CA + server for fixture 0005's CA reference, although the server PEM lives on the host side for the `tls-echo-server` helper, not on the upstream Envoy container).
- `docs/envoy-rust/DECISIONS.md` — no anticipated ADRs (see §7).
- `docs/envoy-rust/ROADMAP.md` — row 03.2 `status` → `done` in the final commit; *at the same commit* row 03 (parent) `status` → `done` (per the ROADMAP schema: "The parent flips to `done` only after all sub-phases are `done`.") — since 03.1 will already be `done` at 03.2 start, landing 03.2 `done` completes the parent. Update both rows in the same commit.
- `docs/envoy-rust/STATE.md` — active → `04-http-1.1` (slug consistent with §8 of `BOOTSTRAP_PROMPT.md`), next-skill → `superpowers:brainstorming` (phase 04 state 0/1), state detection: phase 04 directory does not exist yet.
- `deny.toml` — only if `cargo deny check` flags new licenses or transitive surfaces. Most likely a no-op.

Not touched in 03.2 (belong to 03.1 or earlier, or are frozen):

- `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `a3f3474`.
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/` — landed and finalized before 03.2 begins.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, `phases/02.1-config-cluster/`, `phases/02.2-listener-tcp-proxy/` — closed in phase 02.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/` — unedited; their fixtures must remain green at phase-03.2 state-4 gate.
- `crates/envoy-cluster/src/lib.rs` — untouched in 03.2 per parent-SPEC §3 D4's recommended alternative (envoy-bin orchestrates upstream-TLS construction; envoy-cluster stays rustls-free).
- `crates/envoy-listener/src/lib.rs` — untouched in 03.2 (the `TlsAcceptingHandler` adapter from 03.1 handles both single-cert and multi-cert listeners; no envoy-listener trait change needed).
- `tests/helpers/tcp-echo-server/` — finalized in phase 02.1; fixture 0006's plaintext upstream backend is exactly this helper.
- `BEHAVIOR_CONTRACT.md` — no edits in phase 03 per parent-SPEC §1's baked-in defaults.

---

## 9. Final commit message format (for state 6 of the 03.2 lifecycle, parent row 03 `done` commit)

```
phase 03.2: TLS upstream origination + multi-cert SNI [parent 03 done]

Two new layers complete the phase-03 TLS surface: envoy-tls grows a
SNI-keyed SniResolver and a DownstreamTls::from_listener constructor that
walks all filter chains and builds a single multi-cert ServerConfig.
envoy-tcp::TcpProxy gains an Option<Arc<UpstreamTls>> field and a branched
dial in handle() for upstream TLS origination with wire-level SNI.
envoy-bin orchestrates per-cluster UpstreamTls construction and per-listener
multi-cert dispatch. New differential harness Driver::TlsTcpProbeList +
drive_tls_probes; new TlsEchoBackend; new tls-echo-server helper crate.
Fixtures 0005-tls-upstream and 0006-tls-sni land green end-to-end. Parent
phase 03 ROADMAP row flips done in the same commit.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (unchanged);
  tests/fixtures/0005-tls-upstream green (upstream TLS origination + SNI on
  the wire to tls-echo-server);
  tests/fixtures/0006-tls-sni green (multi-cert SNI cert selection on a
  single downstream listener; per-probe peer-cert SAN assertion).
Conformance: none.
```
