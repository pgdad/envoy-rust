# Phase 03 — Downstream TLS termination + upstream TLS origination + SNI

- **Phase id:** `03`
- **Title:** Downstream TLS termination + upstream TLS origination + SNI (cert selection)
- **Depends on:** `02` (done as of commit `f04e21a`). Both sub-phases `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`) are landed.
- **Differential surface when done:** three new fixtures green against upstream `envoyproxy/envoy:v1.33.0`:
  - `tests/fixtures/0004-tls-downstream/` — single-cert downstream TLS termination, plaintext upstream.
  - `tests/fixtures/0005-tls-upstream/` — plaintext downstream, upstream TLS origination to a new in-tree `tls-echo-server` helper, with the configured `sni` field sent in the upstream ClientHello.
  - `tests/fixtures/0006-tls-sni/` — multi-cert SNI cert selection on the downstream listener (one listener serves cert A on `sni: a.example.com` and cert B on `sni: b.example.com`).
  Pre-existing fixtures `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy` remain green.
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 03; `ROADMAP.md` row 03; `docs/envoy-rust/STATE.md` lifecycle state 1 routing for phase 03.

This SPEC is the design contract for phase 03. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is intentionally concrete enough to be turned into a plan by a stranger with zero prior context per doctrine D-3.4.

This SPEC anticipates that state 2's plan-writer will trip `BOOTSTRAP_PROMPT.md` §6.1's LoC gate (~1500 LoC) and formally split phase 03 into sibling sub-phases `03.1` and `03.2` per §6.2 — see §5 below for the cut line, scope per sub-phase, and the LoC accounting that drives the split decision. The pattern mirrors parent phase 02's pre-split posture (parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` §5; formalized by ADR-0013 at the state-2 plan-writer session). Per the parent precedent, this SPEC remains in-tree unedited as the parent-phase historical artifact once the split is formally landed; the sub-phase SPECs are written fresh at each sub-phase's state-1 session.

---

## 1. Goal and acceptance signal

**Goal.** Land the first TLS-aware data-plane path in the project. Three properties light up across the phase:

1. **Downstream TLS termination** — a listener's filter chain may declare `transport_socket: envoy.transport_sockets.tls (DownstreamTlsContext)`; envoy-rust accepts a TLS connection on that listener, completes the rustls server handshake against the configured cert/key pair, and hands the decrypted byte stream to the chain's network filters (TCP proxy in phase 03).

2. **Upstream TLS origination** — a static cluster may declare `transport_socket: envoy.transport_sockets.tls (UpstreamTlsContext)`; envoy-rust dials each picked endpoint with a rustls client handshake against a configured trust bundle (`validation_context.trusted_ca`), sending the configured `sni` value in the ClientHello server_name extension.

3. **Multi-cert downstream SNI cert selection** — a single listener may declare two filter chains with disjoint `filter_chain_match.server_names`; the listener peeks the ClientHello's SNI extension and routes to the filter chain (and therefore the cert) that matches. envoy-rust builds a single rustls `ServerConfig` with a SNI-keyed `ResolvesServerCert` impl that maps SNI → certified key.

The phase introduces a new library crate `crates/envoy-tls/` that owns every dep on `rustls` + `aws-lc-rs` + `rustls-pki-types` + `tokio-rustls`. `envoy-listener` and `envoy-cluster` (the latter via a small accessor extension) gain transport-socket dispatch hops into envoy-tls; `envoy-tcp::TcpProxy::handle` is generalized over `tokio::io::AsyncRead + AsyncWrite + Unpin + Send` so it accepts both `TcpStream` (plaintext) and `tokio_rustls::*::TlsStream<TcpStream>` (TLS).

The harness gains: a one-shot rcgen-driven test PKI (`TlsTestPki::generate` — CA + leafs `a.example.com` / `b.example.com` / `envoy-rust.test`, written to a per-fixture `TempDir`); two new `Driver` variants (`Driver::TlsTcp { sni, expected_cn }` for fixture 0004's single-probe shape and fixture 0006's per-probe shape, plus `Driver::TlsTcpProbeList { probes }` wrapping the multi-probe form); a `TlsEchoBackend` sibling of `TcpProxyBackend`; and `render_yaml` substitution keys for cert/CA file paths.

This phase stands up TLS as its own primitive — the foundation phase 04+ inherits for HTTP-over-TLS, ALPN-driven HTTP/2, mTLS, and so on.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to phase 03's feature surface:

- (a) the new differential fixtures `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, and `tests/fixtures/0006-tls-sni/` are green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, and `tests/fixtures/0003-tcp-proxy/` remain green;
- (c) no conformance suites run this phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`-max_total_time=30`) against an extended corpus that includes 4–5 new TLS-shaped seeds (downstream-TLS happy path, malformed `@type`, missing `tls_certificates`, multi-cert + `server_names`, `UpstreamTlsContext` with `validation_context`); no new fuzz target ships this phase;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for the phase (per sub-phase, after the formal split lands) is approved.

**Scope shape (brainstorm-fixed choices).** Four forks were resolved during the state-1 brainstorm; downstream planning and any sub-phase SPECs inherit them:

1. **SNI scope — wire-level + downstream cert selection.** Phase 03 ships *both* (a) wire-level SNI on the upstream side (rustls `ClientConfig.server_name` populated from `UpstreamTlsContext.sni`) and (b) downstream cert selection by SNI (rustls `ResolvesServerCert` keyed on the ClientHello SNI extension, mapping to the matching filter-chain's `tls_certificates`). The `envoy.filters.network.sni_cluster` *network filter* (which routes to a cluster *named* by ClientHello SNI) is **not** part of phase 03; it belongs to the §9 network-filters family.

2. **Cert provisioning — rcgen + new ADR.** Test certificates are generated at harness time by a new `TlsTestPki` module backed by the `rcgen` crate (added to the D-3.2 permitted-foundations list as dev-test-harness-only via a phase-03 ADR — provisional ADR-0017; see §7). PEMs land in a per-fixture `TempDir`; both proxies reference the same paths via `render_yaml` substitution. No PEMs are committed to the repo. `tempfile` rolls under the same dev-test-harness-only umbrella.

3. **Crate layout — new `envoy-tls` library crate.** All TLS-specific code (cert loader, rustls `ServerConfig` / `ClientConfig` builders, SNI `ResolvesServerCert` impl, `TlsTransportSocket` wrapper) lives in `crates/envoy-tls/`. `envoy-listener` and `envoy-cluster` do not depend on rustls directly. Match the §4 "one crate per primitive" pattern that parent phase 02 followed.

4. **Fixture shape — three fixtures across two sub-phases.** Fixture 0004 (single-cert downstream TLS, plaintext upstream) lands in 03.1 — proves the envoy-tls scaffold works end-to-end on the smallest TLS surface. Fixtures 0005 (TLS upstream) and 0006 (multi-cert SNI on downstream) land in 03.2. Each property gets its own fixture; failures localize cleanly.

**Defaults baked into this SPEC** (no question to the planner):

- **Upstream cert validation:** validate against a fixture-provided CA bundle via `validation_context.trusted_ca: { filename: "{{CA_PATH}}" }`. No insecure-skip option in phase 03. Both proxies trust the same harness-generated CA root.
- **ALPN:** **omitted** from all phase-03 fixtures. Phase 03 ships TLS-over-TCP — no HTTP — so ALPN is irrelevant. Phase 04 (HCM HTTP/1.1 over TLS) introduces it; phase 05 (HTTP/2) makes it load-bearing.
- **TLS protocol versions and cipher suites:** rustls + aws-lc-rs defaults (TLS 1.2 + TLS 1.3 with the rustls-default cipher list). Envoy v1.33.0 also defaults to TLS 1.2 + TLS 1.3. Fixture YAMLs do **not** include `tls_params`. If execution surfaces version-negotiation drift between the two proxies, an ADR (provisional ADR-0018) pins the floor — not anticipated.
- **BEHAVIOR_CONTRACT.md edits:** **none** in phase 03. The phase exercises only row 2 (`Response body — byte-exact for deterministic handlers`), same as phases 00–02. No headers (TCP, no HTTP), no stats (phase 06), no access logs (phase 06), no xDS (§9), no opt-in to Timing tolerances (TLS handshake is not fundamentally time-sensitive at this differential surface). The currently-empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) remain empty.
- **`Cluster::name()` accessor (phase-02.2 REVIEW M1 carryforward):** **default-deferred** to phase 06 (when stats first need name attribution). Phase 03 does not need it — `envoy-tcp::TcpProxy` already carries `cluster_name: String` separately, and envoy-tls's upstream side similarly carries the configured `sni` and the cluster name where attribution is wanted. If 03.2 execution surfaces an opportunistic close (e.g., a tracing-span attribution that benefits from the accessor), the closing happens in-execution per the phase-02.2 task-11 precedent (which closed phase-02.1 REVIEW M3 opportunistically); SPEC §6 signpost 11 names this.

---

## 2. Behavior-contract scope for phase 03

Phase 03 continues to exercise only **row 2** of the `BEHAVIOR_CONTRACT.md` §7.2 equivalence matrix:

- **Response body — Byte-exact for deterministic handlers.** All three fixtures use a TCP-echo data plane (either Envoy's `tcp_proxy` filter routing to a plaintext echo backend, or the same routing to a TLS echo backend in fixture 0005). The driver helpers (`drive_tls`, `drive_tcp`) inherit ADR-0006/0007's `read_exact(payload.len())` + 100 ms trailing-byte poll discipline. The post-handshake byte stream is the differential surface; the handshake itself is exercised end-to-end by virtue of the connection succeeding, but no byte of the TLS record layer is asserted directly.

Fixture 0006 additionally asserts a structural property — the post-handshake peer certificate's SAN/CN matches the expected value for each SNI probe — but this assertion lives **inside the harness driver** (`Driver::TlsTcpProbeList` interrogates `tokio_rustls::client::TlsStream::get_ref().1.peer_certificates()` and asserts equality against `probes[i].expected_cn`). It is not a new equivalence-matrix dimension; both proxies must select the *same* cert for the *same* SNI for the test to pass, which is exactly the property under test. The matrix row engaged is still row 2 (post-handshake bytes are byte-exact).

No other dimension is engaged. **No `BEHAVIOR_CONTRACT.md` edits in phase 03.**

---

## 3. Deliverables

### D1 — New library crate `crates/envoy-tls/`

Added to root `[workspace] members`. Owns all TLS-specific code; the only crate in the workspace that depends on `rustls`, `tokio-rustls`, `rustls-pki-types`, or aws-lc-rs.

- `crates/envoy-tls/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Dependencies from D-3.2 only:
  - `envoy-config = { path = "../envoy-config" }`
  - `rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }`
  - `rustls-pki-types = "1"`
  - `tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }`
  - `tokio = { version = "1", features = ["net", "io-util", "macros", "sync"] }`
  - `thiserror = "2"`
  - `tracing = "0.1"`

  Dev-deps add `tokio` `rt-multi-thread` for tests and `rcgen = "0.13"` for unit-test fixture generation (the same dep that the differential harness pulls in). The `aws-lc-rs` crypto provider is selected via the `tokio-rustls` `aws-lc-rs` feature; the `ring` provider is **not** brought in. (Plan-writer verifies feature names against the actual `tokio-rustls` 0.26.x API at execution time.)

  `tokio-rustls`'s status under D-3.2 is mildly ambiguous — D-3.2 lists `rustls`, `webpki`, `rustls-pki-types`, and "`aws-lc-rs` permitted as the crypto provider," but does not name `tokio-rustls` explicitly. `tokio-rustls` is the canonical async glue between `tokio` and `rustls`, ships from the `rustls` org, and is mechanically necessary to use rustls with tokio (the alternative is hand-rolling the I/O loop, which is exactly the kind of foundational primitive D-3.2 tells us not to reinvent). The plan-writer lands a one-paragraph ADR (provisional ADR-0018) at task 1 that explicitly extends D-3.2's "rustls + aws-lc-rs" grant to cover `tokio-rustls`. Cost is one ADR; benefit is no ambiguity for any downstream phase.

- `crates/envoy-tls/src/lib.rs` starts with `#![forbid(unsafe_code)]` per D-3.8. Public surface (described per sub-phase scope; the planner extracts the 03.1-only subset into the 03.1 sub-phase SPEC at split time):

    ```rust
    /// Lands in 03.1.
    pub struct DownstreamTls {
        config: std::sync::Arc<rustls::ServerConfig>,
    }

    impl DownstreamTls {
        /// Build from a parsed envoy_config::DownstreamTlsContext. Loads cert+key
        /// PEMs from the configured filenames; constructs a single-cert
        /// ResolvesServerCert when exactly one tls_certificate is present, a
        /// SNI-keyed ResolvesServerCert (lands in 03.2) when more than one is
        /// present *and* the listener configures filter_chain_match.server_names.
        ///
        /// Phase 03.1: single-cert path only. Phase 03.2 extends with the
        /// SniResolver (see §3 D2 below) and adopts a new constructor variant.
        pub fn from_context(cfg: &envoy_config::DownstreamTlsContext) -> Result<Self, TlsError>;

        /// Hands a connected downstream TcpStream through the rustls server
        /// handshake; returns the post-handshake stream. On handshake failure
        /// returns TlsError::Handshake; the listener's accept loop logs and
        /// drops per the same posture as phase 02.2's TcpProxy connection
        /// errors.
        pub async fn accept(
            &self,
            downstream: tokio::net::TcpStream,
        ) -> Result<tokio_rustls::server::TlsStream<tokio::net::TcpStream>, TlsError>;

        /// Lands in 03.2 alongside the SniResolver. Allows the planner to build
        /// a multi-cert listener config from a list of (server_names, cert) pairs
        /// rather than a single DownstreamTlsContext. The exact API shape is
        /// resolved at the 03.2 SPEC; the planner picks one of:
        /// (a) `from_listener(listener: &envoy_config::Listener)` that walks all
        ///     filter chains and builds the SNI map; or
        /// (b) `from_sni_map(map: HashMap<String, CertifiedKey>)` that the caller
        ///     populates by walking filter chains.
        /// Recommendation: (a). One responsibility (build a ServerConfig from
        /// the listener config); fewer caller-side moving parts; matches Envoy's
        /// internal model of "one ServerConfig per listener, SNI-multiplexed."
        // pub fn from_listener(listener: &envoy_config::Listener) -> Result<Self, TlsError>;  // 03.2

        /// Lands in 03.1 (return-type shape; consumers are 03.2-only since 03.1
        /// has no upstream-TLS fixture). The 03.1 SPEC ships the
        /// implementation; 03.2 adds the consumer wiring and fixture 0005.
        pub struct UpstreamTls {
            config: std::sync::Arc<rustls::ClientConfig>,
            server_name: rustls::pki_types::ServerName<'static>,
        }

        impl UpstreamTls {
            pub fn from_context(cfg: &envoy_config::UpstreamTlsContext)
                -> Result<Self, TlsError>;
            pub async fn connect(
                &self,
                upstream: tokio::net::TcpStream,
            ) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>, TlsError>;
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum TlsError {
        #[error("loading cert/key file {path:?}: {source}")]
        FileRead { path: std::path::PathBuf, #[source] source: std::io::Error },
        #[error("parsing PEM at {path:?}: no leaf certificate found")]
        CertParse { path: std::path::PathBuf },
        #[error("parsing private key at {path:?}: {0}")]
        KeyParse(std::path::PathBuf, String),
        #[error("rustls config build: {0}")]
        RustlsConfig(String),
        #[error("invalid SNI {sni:?} in upstream context: {reason}")]
        InvalidServerName { sni: String, reason: String },
        #[error("TLS handshake: {source}")]
        Handshake { #[source] source: std::io::Error },
        #[error("loading trusted_ca PEM at {path:?}: no CA certificate found")]
        CaParse { path: std::path::PathBuf },
        #[error("downstream context requires at least one tls_certificate")]
        DownstreamRequiresCert,
    }
    ```

- **Cert/key loader.** A helper `load_certified_key(cert_path, key_path) -> Result<rustls::sign::CertifiedKey, TlsError>` reads both PEMs via `std::fs`, extracts `Vec<rustls_pki_types::CertificateDer>` with `rustls_pemfile::certs`, extracts the private key with `rustls_pemfile::private_key`, builds a `rustls::sign::CertifiedKey` using the `aws-lc-rs` provider's `any_supported_type` signing-key constructor. Returns `TlsError::CertParse` / `TlsError::KeyParse` on empty or malformed parses. (`rustls-pemfile` is a public utility crate that ships from the rustls org and is on D-3.2 by name in spirit; if not, ADR-0018 covers it under the same blanket the way it covers `tokio-rustls`. Plan-writer verifies.)

- **Single-cert `ResolvesServerCert` (03.1).** Trivial — the lone `CertifiedKey` is returned for any ClientHello regardless of SNI. A trivial `struct SingleCertResolver(Arc<CertifiedKey>);` impl of `ResolvesServerCert` suffices. The `ServerConfig` is built with `.with_cert_resolver(Arc::new(SingleCertResolver(key)))`. Adopting the resolver shape in 03.1 (instead of the simpler `.with_single_cert(...)`) keeps the 03.2 extension drop-in: the `ServerConfig`-building seam is already a resolver from day one.

- **SNI-keyed `ResolvesServerCert` (03.2).** New `SniResolver { map: HashMap<String, Arc<CertifiedKey>>, default: Option<Arc<CertifiedKey>> }`. `resolve` reads `ClientHello::server_name()` (the `&str` SNI from the ClientHello), looks it up in the map; on miss returns `default` (i.e., the catch-all filter chain, if any) or `None` (which causes rustls to abort the handshake with `unrecognized_name` per RFC 6066 §3 and TLS 1.3 §6.2). Phase 03.2's exact match policy mirrors Envoy v1.33.0's behavior: case-insensitive exact match on SNI; wildcards (`*.example.com`) supported on either side iff Envoy supports them at v1.33.0 — verify at execution and land an ADR if a wildcard semantics fork surfaces.

- **`ClientConfig` builder (03.1, consumer 03.2).** Builds with `.with_root_certificates(roots)` where `roots: rustls::RootCertStore` is populated from the configured `validation_context.trusted_ca` PEM. No system-roots fallback in phase 03 (deferred to a later phase that needs it). `.with_no_client_auth()` (mTLS deferred). The `ServerName` for the connection is derived from `UpstreamTlsContext.sni` via `ServerName::try_from(sni.as_str())`; on parse failure returns `TlsError::InvalidServerName`.

- **rustls crypto provider initialization.** Phase 03 explicitly installs `aws-lc-rs` as the default crypto provider once at startup of any process using `envoy-tls` (envoy-bin, the harness, and `tls-echo-server`). The call is `rustls::crypto::aws_lc_rs::default_provider().install_default()` and lives in the binary crates' `main` (not in `envoy-tls` itself, which is a library and must not unilaterally `install_default`). The `install_default` API is idempotent-on-second-call-no-op-but-Err — call it once and ignore the `Err` return on second call. SPEC §6 signpost 4 covers this.

- **Unit tests in `crates/envoy-tls/src/lib.rs::tests`.** 03.1 ships ~10 tests; 03.2 ships ~5 more. 03.1 enumeration:
  - `loads_single_cert_server_config` — rcgen-built PEMs in tmpdir; `DownstreamTls::from_context` returns `Ok`; `accept` against a connected pair (in-process `TcpListener` + `TcpStream::connect`) completes the handshake; both peers see the same negotiated TLS version (≥ 1.2).
  - `rejects_empty_tls_certificates` — `DownstreamTlsContext` with empty `tls_certificates` → `TlsError::DownstreamRequiresCert`.
  - `rejects_malformed_cert_pem` — `tls_certificates[0].certificate_chain.filename` points at a file with no PEM headers → `TlsError::CertParse`.
  - `rejects_missing_key_pem` — file does not exist → `TlsError::FileRead`.
  - `loads_upstream_client_config` — `UpstreamTlsContext::from_context` returns `Ok`; the produced `ClientConfig` has the harness CA in its root store; `connect` against a TLS-listening counterpart (in-test `tokio_rustls::TlsAcceptor` plus an `rcgen`-built server cert) completes the handshake.
  - `upstream_rejects_invalid_sni` — `sni: ""` or `sni: "0.0.0.0"` (rustls `ServerName::try_from` does not accept IP literals as DNS names; an IP must use `ServerName::IpAddress`) → `TlsError::InvalidServerName`.
  - `upstream_rejects_untrusted_cert` — server cert signed by a CA not in the configured trust bundle → handshake fails with `TlsError::Handshake { source: ... }` carrying a verifier-rejection inside.
  - `single_cert_resolver_returns_same_cert_regardless_of_sni` — three different SNIs, one cert; resolver returns the same `CertifiedKey` each time.
  - `crypto_provider_install_is_idempotent` — calling `aws_lc_rs::default_provider().install_default()` twice in the test process succeeds-or-noops (the `Err(_)` from the second call is documented and ignored).
  - `accept_returns_handshake_error_on_garbage_input` — connect with a plain-TCP client that writes `b"GET / HTTP/1.1\r\n"` instead of a ClientHello → `TlsError::Handshake`.

  03.2 adds:
  - `sni_resolver_routes_known_sni` — map `{"a.example.com" → key_a, "b.example.com" → key_b}`; `resolve(ClientHello{sni: "a.example.com"})` returns `key_a`; same for `b`.
  - `sni_resolver_falls_back_to_default_on_miss` — map populated; `default = Some(key_a)`; `resolve(ClientHello{sni: "unknown.example.com"})` returns `key_a`.
  - `sni_resolver_returns_none_on_miss_without_default` — map populated; `default = None`; `resolve` returns `None`.
  - `sni_resolver_is_case_insensitive` — map keyed lowercase; `resolve(ClientHello{sni: "A.Example.com"})` returns the lowercase entry.
  - `from_listener_builds_multi_cert_config` — synthesized `envoy_config::Listener` with two filter chains (server_names `["a.example.com"]` and `["b.example.com"]`); `DownstreamTls::from_listener` returns a `ServerConfig` whose resolver picks `key_a` for `a.example.com` and `key_b` for `b.example.com`.

### D2 — `envoy-config` schema extensions

`crates/envoy-config/src/bootstrap.rs` gains the transport_socket envelope, both TLS contexts, the data-source struct, and (in 03.2) `filter_chain_match`. The `Node` open-schema asymmetry is not widened.

**Lands in 03.1:**

```rust
// On FilterChain — optional, default-plaintext:
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    #[serde(default)]
    pub filter_chain_match: Option<FilterChainMatch>,
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    pub filters: Vec<NetworkFilter>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransportSocket {
    /// Phase 03 accepts only `"envoy.transport_sockets.tls"`; the validator
    /// rejects any other name. Future phases may add raw_buffer / quic / etc.
    pub name: String,
    pub typed_config: TransportSocketTypedConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TransportSocketTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext")]
    Downstream(DownstreamTlsContext),
    #[serde(rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext")]
    Upstream(UpstreamTlsContext),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DownstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
    /// Server Name sent in the ClientHello server_name extension.
    /// Phase 03 requires this on every UpstreamTlsContext (no auto_sni). The
    /// validator rejects an empty string.
    pub sni: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonTlsContext {
    #[serde(default)]
    pub tls_certificates: Vec<TlsCertificate>,
    #[serde(default)]
    pub validation_context: Option<CertificateValidationContext>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TlsCertificate {
    pub certificate_chain: DataSource,
    pub private_key: DataSource,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CertificateValidationContext {
    pub trusted_ca: DataSource,
}

/// Phase 03 supports `filename` only. inline_string / inline_bytes /
/// environment_variable / `secret_ref` are deferred to later phases.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    pub filename: String,
}

// On Cluster — optional, default-plaintext:
pub struct Cluster {
    // … existing fields from phase 02.1 …
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,  // schema 03.1; consumer 03.2
}
```

**Lands in 03.2:**

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChainMatch {
    /// SNI values this filter chain matches. Empty Vec = catch-all (only one
    /// catch-all filter chain per listener; validator enforces).
    #[serde(default)]
    pub server_names: Vec<String>,
}
```

**Validator extensions (`bootstrap::validate`).** New `ConfigError` variants:

- `UnknownTransportSocketName(String)` — only `"envoy.transport_sockets.tls"` accepted in phase 03.
- `MismatchedTransportSocketDirection { side: &'static str, got: &'static str }` — `DownstreamTlsContext` is invalid as a cluster's upstream `transport_socket`; `UpstreamTlsContext` is invalid as a filter-chain's downstream `transport_socket`.
- `EmptyTlsCertificates` — `DownstreamTlsContext.common_tls_context.tls_certificates` must be ≥ 1 (validator); upstream context must be 0 (no client cert in phase 03 — mTLS deferred).
- `MissingValidationContext` — `UpstreamTlsContext` requires a `validation_context.trusted_ca` (no insecure-skip in phase 03).
- `EmptyUpstreamSni` — `UpstreamTlsContext.sni` must be a non-empty string.
- `MultipleListenersWithOverlappingSni { listener: String, sni: String }` (03.2) — within one listener, two filter chains may not declare the same value in `server_names`.
- `MultipleCatchAllFilterChains { listener: String }` (03.2) — within one listener, at most one filter chain may have empty `server_names` (the catch-all).

**Validator test count.** ~10 in 03.1 (`UnknownTransportSocketName`, `MismatchedTransportSocketDirection` × 2 sides, `EmptyTlsCertificates` × 2 sides, `MissingValidationContext`, `EmptyUpstreamSni`, three downstream/upstream happy paths). ~6 additional in 03.2 (`server_names` empty/non-empty parsing, overlapping SNI rejected, catch-all-allowed-once, multi-filter-chain happy path).

**Fuzz corpus extension.** 03.1 adds 3 TLS-shaped seeds to `crates/envoy-config/fuzz/corpus/parse_bootstrap/`: a downstream-TLS happy path; a malformed `@type`; an `UpstreamTlsContext` with `validation_context`. 03.2 adds 2 more: multi-cert + `server_names`; overlapping-SNI reject case. The existing `parse_bootstrap` target picks them up automatically; no new fuzz target this phase.

### D3 — `envoy-listener` TLS dispatch (lands in 03.1)

`crates/envoy-listener/src/lib.rs` does not directly depend on `envoy-tls` (avoids leaking rustls types into the listener crate). Instead, the existing `ConnectionHandler` trait is the seam: `envoy-bin` constructs a `TlsAcceptingHandler` adapter (lives in envoy-bin) that wraps the inner `Arc<dyn ConnectionHandler>` (a `TcpProxy` in phase 03) with a TLS-accept hop.

The cleanest landing of this is to **generalize** `ConnectionHandler::handle` to be generic over a connected stream type, but the existing trait uses a concrete `tokio::net::TcpStream`. Two options for the planner:

- **(α) Keep `ConnectionHandler` concrete on `TcpStream`; wrap at envoy-bin.** envoy-bin builds a `TlsAcceptingHandler { tls: Arc<DownstreamTls>, inner: Arc<dyn TlsConnectionHandler> }` where `TlsConnectionHandler` is a *new* trait in envoy-tls (or envoy-bin) that takes `tokio_rustls::server::TlsStream<TcpStream>`. Listener still calls into a `ConnectionHandler` (which is `TlsAcceptingHandler` in TLS chains and `Arc<TcpProxy>` directly in plaintext chains). The TlsAcceptingHandler's `handle` does `tls.accept(downstream).await?; inner.handle(post_handshake_stream).await`. **Recommended.** Smallest envoy-listener diff (zero edits to `ConnectionHandler` in 03.1); concentrates the TLS knowledge in envoy-bin's wiring.

- **(β) Generalize `ConnectionHandler` over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static`.** Bigger envoy-listener edit; type-check ripples through `Listener::serve`'s `JoinSet` typing. Cleaner long-term but a larger 03.1 surface change for limited 03.1 benefit.

The 03.1 sub-phase SPEC picks **α**. envoy-tcp's `TcpProxy::handle` is in turn generalized over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static` (a smaller, contained edit) so the `TlsAcceptingHandler` can pass either the post-handshake `TlsStream` or a plaintext `TcpStream` into it. envoy-tcp's existing tests cover the plaintext path; new tests cover the TLS path via in-process `tokio_rustls::TlsAcceptor` + `TlsConnector` (no envoy-listener / envoy-tls scaffolding needed in the test).

### D4 — `envoy-tcp` generic-stream lift (lands in 03.1) + upstream TLS dial (lands in 03.2)

03.1 changes:

- `crates/envoy-tcp/src/lib.rs::TcpProxy::handle` is generalized: instead of `(&self, downstream: tokio::net::TcpStream)`, it becomes `<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(&self, downstream: S)`. The body is unchanged — `tokio::io::copy` already accepts the trait — but the generic shape is the seam that 03.2's TLS-on-TLS fixture exploits.
- `ConnectionHandler::handle` becomes a trait shape that envoy-tcp implements for `TcpStream` (plaintext path). The new `TlsConnectionHandler` (envoy-bin or envoy-tls — planner picks at split time) implements for `TlsStream<TcpStream>`. Both call into the same generic `TcpProxy::handle`.

03.2 changes:

- `TcpProxy::handle` gains an `Option<Arc<UpstreamTls>>` field so it can wrap the upstream dial in a TLS handshake. The cluster's transport_socket dictates whether this field is `Some` at construction (envoy-bin's wiring reads `cluster.transport_socket` and threads `UpstreamTls::from_context(...)?` into the `TcpProxy::new` call). When `Some`, the upstream-dial path becomes `TcpStream::connect(addr).await` → `upstream_tls.connect(stream).await` → bidirectional copy as today.
- `envoy-cluster::Cluster` gains an `upstream_tls: Option<Arc<UpstreamTls>>` field built at `from_bootstrap` time from the cluster's parsed `transport_socket`. New `Cluster::upstream_tls()` accessor returns `Option<&Arc<UpstreamTls>>`. envoy-tcp reads it via the existing `ClusterHandle::pick_endpoint` adjacency.

  *Alternative carried in this SPEC:* envoy-cluster does **not** depend on envoy-tls (avoids the TLS dep climbing into a non-TLS crate). Instead, envoy-bin's wiring builds the `Option<Arc<UpstreamTls>>` from the parsed `cluster.transport_socket` at startup, alongside the `ClusterManager`, and threads it into the per-cluster `TcpProxy` constructor. `Cluster` itself does *not* carry the TLS handle. **Recommended.** Keeps envoy-cluster's deps unchanged (no rustls leak); envoy-bin remains the orchestration site for cross-crate composition. The 03.2 SPEC picks the option at split time.

### D5 — `envoy-bin` wiring

03.1 wiring:

- `crates/envoy-bin/src/main.rs::run` gains a per-filter-chain pre-pass: for the listener's first filter chain (only one allowed in 03.1; multiple lands in 03.2 with `filter_chain_match`), if `transport_socket` is `Some(envoy.transport_sockets.tls + DownstreamTlsContext)`, build `Arc<DownstreamTls>` once via `DownstreamTls::from_context`, wrap the inner `TcpProxy` handler in a `TlsAcceptingHandler { tls, inner }`, and pass the wrapper as the `Arc<dyn ConnectionHandler>` into `Listener::bind`. Plaintext path is unchanged.
- One-time `rustls::crypto::aws_lc_rs::default_provider().install_default()` call near the top of `run`. Idempotent; second call returns `Err` (ignored — harmless when tests run multiple times in one process).
- Integration test `crates/envoy-bin/tests/tls_downstream.rs` (backstop to fixture 0004; same `#[ignore]`-unless-`DOCKER=1` not needed here — this test is in-process). Spawns envoy-bin as a subprocess with rcgen-generated PEMs in a per-test tmpdir; opens a TLS connection via `tokio_rustls::TlsConnector` configured with the same CA in its root store; writes the payload; `read_exact`; asserts byte-equality. Mirrors the shape of phase 02.2's `crates/envoy-bin/tests/tcp_proxy.rs`.

03.2 wiring:

- `run` learns to walk *all* filter chains in the listener (≥ 1 allowed) and build a single multi-cert `DownstreamTls::from_listener(listener)` when any chain carries TLS + the listener has multiple filter chains. The validator already rejects overlapping `server_names`; envoy-bin trusts that.
- For each cluster with `transport_socket: Upstream(...)`, build `Arc<UpstreamTls>` at startup; thread into the per-cluster `TcpProxy` constructor (or, if D4's alternative is taken, into the `TcpProxy` field on construction).
- Integration test `crates/envoy-bin/tests/tls_upstream.rs` (backstop to fixture 0005); tls-echo-server runs as a host subprocess started by the test.
- Integration test `crates/envoy-bin/tests/tls_sni.rs` (backstop to fixture 0006); two probes per test invocation.

`crates/envoy-bin/Cargo.toml` adds path-deps `envoy-tls = { path = "../envoy-tls" }` (03.1) and a dev-dep on `tokio-rustls` + `rcgen` for the integration tests' TLS clients.

### D6 — Differential harness extensions

03.1:

- **New module `tests/differential/src/tls.rs`** owning the test PKI:

    ```rust
    pub struct TlsTestPki {
        pub ca_pem_path:   PathBuf,
        pub leaf_a_cert:   PathBuf,
        pub leaf_a_key:    PathBuf,
        pub leaf_b_cert:   PathBuf,    // 03.2 use; 03.1 generates but no fixture references
        pub leaf_b_key:    PathBuf,
        pub server_cert:   PathBuf,    // 03.2 use; for tls-echo-server
        pub server_key:    PathBuf,
        _tmpdir: tempfile::TempDir,
    }

    impl TlsTestPki {
        pub fn generate() -> anyhow::Result<Self>;
    }
    ```

    The CA is a self-signed cert with `ca: true` BasicConstraint, generated by rcgen at construction time. Each leaf is signed by that CA with the appropriate Subject Alternative Names: `a.example.com`, `b.example.com`, `envoy-rust.test`. PEMs are written into `_tmpdir` and the `*_path` fields point at the on-disk locations. `Drop` on `_tmpdir` removes the entire directory after the fixture run completes.

- **Driver grammar.** New tagged variants on `Driver`:

    ```rust
    Driver::TlsTcp { sni: String, expected_cn: Option<String> }
    Driver::TlsTcpProbeList { probes: Vec<TlsTcpProbe> }    // 03.2

    pub struct TlsTcpProbe { pub sni: String, pub expected_cn: Option<String> }
    ```

    Existing `Driver::TcpEcho` and `Driver::HttpGet` are unchanged.

- **`drive_tls(addr, payload, sni, root_store, expected_cn) -> Result<()>`** — opens a `TcpStream::connect(addr)`, builds a `tokio_rustls::TlsConnector` from a `ClientConfig` with `root_store` and the `aws-lc-rs` provider, calls `connector.connect(server_name, stream)`, asserts the handshake succeeded, optionally asserts the post-handshake `peer_certificates()[0]` carries `expected_cn` in its SAN/CN, writes payload, `read_exact(payload.len())`, asserts byte-equality, runs the ADR-0007 100ms trailing-byte poll, gracefully shuts down. `expected_cn` semantics: if `Some(cn)`, parse the leaf via `rustls-pki-types`, walk `subject_alt_name` and CommonName, assert `cn` is present in either; if `None`, skip the check.

- **`render_yaml` per-driver substitution.** New keys: `{{CA_PATH}}`, `{{LEAF_A_CERT_PATH}}`, `{{LEAF_A_KEY_PATH}}`, `{{LEAF_B_CERT_PATH}}` / `{{LEAF_B_KEY_PATH}}` (03.2), `{{SERVER_CERT_PATH}}` / `{{SERVER_KEY_PATH}}` (03.2), `{{TLS_BACKEND_PORT}}` (03.2). Per-side substitution: for envoy-side YAMLs the path is the *container-mounted* path (e.g. `/etc/envoy-rust-tls/ca.pem`); for envoy-rust-side YAMLs the path is the *host* tmpdir path. `tls.rs` exposes both forms via two methods on `TlsTestPki`: `.envoy_side_paths()` (container-mounted) and `.subject_side_paths()` (host).

- **Upstream container mount.** `tests/differential/src/upstream.rs::start` gains a `tls_pki: Option<&TlsTestPki>` parameter. When `Some`, the testcontainers image is configured with `with_copy_to_container(host_dir, "/etc/envoy-rust-tls/")` (or the equivalent v0.23 API; verify at execution against `Cargo.lock`). Existing `host_gateway: bool` gating from phase 02.2 is unchanged.

- **`run_fixture` dispatch.** Detection cascade extended:
  1. If either rendered template references `{{CA_PATH}}`, `{{LEAF_*_PATH}}`, or `{{SERVER_*_PATH}}`, build `TlsTestPki::generate()` and substitute the per-side keys.
  2. Existing `{{BACKEND_PORT}}` gating from phase 02.2 still spawns `TcpProxyBackend`.
  3. (03.2) If either rendered template references `{{TLS_BACKEND_PORT}}`, spawn `TlsEchoBackend` (a sibling of `TcpProxyBackend` that runs the new `tls-echo-server` helper); fill the substitution.
  4. Pass `tls_pki: Option<&TlsTestPki>` into `upstream::start` so the container mount happens before the upstream Envoy container boots.

- **Harness unit tests in `tests/differential/src/{tls,lib}.rs::tests` (4 tests, 03.1):**
  - `tls_test_pki_generates_valid_chain` — generate PKI, parse all PEMs back via rustls-pemfile, assert ≥ 1 cert in each chain, assert CA-signed-leafs property (verify the leaf's `Issuer` matches the CA's `Subject`).
  - `tls_test_pki_drop_removes_tmpdir` — generate, capture path, drop, assert the path no longer exists.
  - `render_yaml_substitutes_tls_paths_for_envoy_side` — unit test the envoy-side container-mounted path substitution.
  - `render_yaml_substitutes_tls_paths_for_subject_side` — unit test the host path substitution.

  03.2 adds 2 more (`tls_echo_backend_*` mirroring the 02.2 `tcp_proxy_backend_*` pair).

03.2 additions:

- **`TlsEchoBackend` in `tests/differential/src/backend.rs`.** Sibling of `TcpProxyBackend`; spawns the new `tls-echo-server` helper binary with `--port`, `--cert`, `--key` argv. Same SIGKILL-on-Drop posture as `TcpProxyBackend` per phase 02.2.
- **Multi-probe `drive_tls`** — `drive_tls_probes(addr, payload, probes, root_store) -> Result<()>` opens one connection per probe, runs `drive_tls`'s body once per connection.

### D7 — New helper crate `tests/helpers/tls-echo-server/` (lands in 03.2)

Sibling of `tests/helpers/tcp-echo-server/`. Same skeleton, with TLS termination on top.

- `tests/helpers/tls-echo-server/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Deps: `anyhow`, `thiserror`, `tokio`, `tokio-rustls`, `rustls`, `rustls-pki-types`, `tracing`, `tracing-subscriber`. Same dep set as `tcp-echo-server` plus the `rustls` glue. The `aws-lc-rs` crypto provider is selected via `tokio-rustls`'s feature.
- `src/main.rs` starts with `#![forbid(unsafe_code)]`. argv: `--port <u16>` (required), `--cert <path>` (required, leaf cert PEM), `--key <path>` (required, private-key PEM), `--help`, `--version`. On startup: install aws-lc-rs default provider; load the cert + key via `rustls-pemfile`; build a `ServerConfig` with a single-cert `ResolvesServerCert` (no SNI multiplexing — single-purpose); accept TLS connections with `tokio_rustls::TlsAcceptor`; for each connection, post-handshake, `tokio::io::copy` from the read half to the write half (echo). Drain logic mirrors `tcp-echo-server` (5 s `DRAIN_BUDGET`).
- ~120 LoC of impl + ~5 unit tests (argv parse, version flag, help flag, accepts and echoes, handshake-failure path).

### D8 — Differential fixtures

#### Fixture `tests/fixtures/0004-tls-downstream/` (lands in 03.1)

**Property.** Downstream TLS termination with a single cert; plaintext upstream backend (`tcp-echo-server`).

Files:

- `envoy.yaml` — listener bound on `0.0.0.0:{{PORT}}` with one filter chain carrying `transport_socket: envoy.transport_sockets.tls (DownstreamTlsContext)` referencing `{{LEAF_A_CERT_PATH}}` + `{{LEAF_A_KEY_PATH}}`; chain runs `envoy.filters.network.tcp_proxy → cluster: backend`. Cluster `backend` is a STATIC, single-endpoint cluster pointing at `{{BACKEND_HOST}}:{{BACKEND_PORT}}` (templates to `host.docker.internal` on the container side, `127.0.0.1` on the subject side per ADR-0015). Admin block matches fixture 0003's pattern.

- `envoy-rust.yaml` — same shape with the per-side divergences from fixture 0003 (bind `127.0.0.1`, no admin block, backend host `127.0.0.1`).

- `inputs/payload.bin` — copy of fixture 0001/0003's payload (18 bytes; deterministic non-zero blob).

- `expectations.yaml`:

    ```yaml
    driver:
      kind: tls_tcp
      sni: "a.example.com"
    equivalence:
      response_body: byte_exact
    ```

- `README.md` — one paragraph naming the property; the cert-loading mechanics (rcgen-generated PEMs in a per-fixture tmpdir, mounted into the upstream container at `/etc/envoy-rust-tls/`); the absence of ALPN, multi-cert SNI, upstream TLS, mTLS as out-of-fixture (each tied to the later fixture or phase); ADR references (ADR-0017 rcgen, ADR-0018 tokio-rustls).

#### Fixture `tests/fixtures/0005-tls-upstream/` (lands in 03.2)

**Property.** Plaintext downstream; upstream TLS origination from envoy-rust to the new `tls-echo-server` helper. Wire-level SNI is sent to upstream (server_name in ClientHello = `UpstreamTlsContext.sni`).

Files:

- `envoy.yaml` — listener with one plaintext filter chain (`transport_socket` absent) running `tcp_proxy → cluster: backend`. Cluster `backend` carries `transport_socket: envoy.transport_sockets.tls (UpstreamTlsContext)` with `sni: "envoy-rust.test"` and `validation_context.trusted_ca: { filename: "{{CA_PATH}}" }`. The cluster's single endpoint points at `{{BACKEND_HOST}}:{{TLS_BACKEND_PORT}}`.

- `envoy-rust.yaml` — same shape with the per-side divergences.

- `inputs/payload.bin` — fixture-0001 payload, byte-identical.

- `expectations.yaml`:

    ```yaml
    driver:
      kind: tcp_echo
    equivalence:
      response_body: byte_exact
    ```

- `README.md` — names the property (upstream TLS origination, including SNI on the wire), the `tls-echo-server` helper's role, the validation-against-the-harness-CA posture, and ADR references.

#### Fixture `tests/fixtures/0006-tls-sni/` (lands in 03.2)

**Property.** Downstream TLS with multi-cert SNI cert selection. One listener; two filter chains; each chain carries a different cert keyed on its `filter_chain_match.server_names`. Plaintext upstream backend (tcp-echo-server).

Files:

- `envoy.yaml` — listener with two filter chains:
  - chain A: `filter_chain_match: { server_names: ["a.example.com"] }`, `transport_socket → DownstreamTlsContext` referencing `{{LEAF_A_CERT_PATH}}` / `{{LEAF_A_KEY_PATH}}`, filters: `tcp_proxy → backend`.
  - chain B: `filter_chain_match: { server_names: ["b.example.com"] }`, `transport_socket → DownstreamTlsContext` referencing `{{LEAF_B_CERT_PATH}}` / `{{LEAF_B_KEY_PATH}}`, filters: `tcp_proxy → backend`.
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

- `README.md` — names the property, the SNI resolution mechanism, the per-probe round-trip, the ADR references, and explicitly notes the unknown-SNI case is **not** asserted in this fixture (signpost 8 below).

### D9 — CI workflow

`.github/workflows/ci.yml` changes: **none** in 03.1 or 03.2. The existing `build` job runs `cargo test --workspace`, which picks up the new `envoy-tls` crate and the new `tls-echo-server` helper automatically. The existing `fuzz` job exercises the extended parse_bootstrap corpus via the same `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` invocation.

The Docker-gated integration tests `tests/differential/tests/tls_downstream.rs` (03.1), `tests/differential/tests/tls_upstream.rs` (03.2), and `tests/differential/tests/tls_sni.rs` (03.2) run under the same `#[ignore]`-unless-`DOCKER=1` gating pattern as `admin_ready.rs` (phase 01) and `tcp_proxy.rs` (phase 02.2).

### D10 — ADRs to land during execution

Three ADRs anticipated. Numbering provisional per phase-02.2 REVIEW recommendation #2 — interim ADRs landing between 02.2 done and 03 start (or between sub-phases within 03) shift the numbering; each sub-phase SPEC commits to the actual next-sequential numbers at task 1.

- **ADR-0017 — Add `rcgen` and `tempfile` to permitted-foundations as dev-test-harness-only.** Lands in 03.1 task 1. Mirrors ADR-0009 / ADR-0010 / ADR-0012 (cargo-fuzz tooling) on posture: dev-test-harness-only; never a transitive of `envoy-bin` or any non-test workspace crate. Justification: TLS test infra recurs across phases 03–08+ (HTTP/1.1 over TLS, H2 over TLS, mTLS, etc.); a one-time foundations grant beats per-phase ADR churn. ADR explicitly enumerates: `rcgen` as the cert generator; `tempfile` as the per-fixture tmpdir manager; both restricted to `tests/differential/`, `tests/helpers/tls-echo-server/`, and `crates/envoy-tls/`'s dev-deps.

- **ADR-0018 — Extend the rustls foundations grant to cover `tokio-rustls` (and `rustls-pemfile`).** Lands in 03.1 task 1 alongside ADR-0017. Justification: `tokio-rustls` is the canonical async glue between `tokio` and `rustls`, ships from the rustls org, and is mechanically necessary to use rustls inside a tokio runtime. `rustls-pemfile` is the canonical PEM-parsing utility from the rustls org. Both round out D-3.2's "rustls + aws-lc-rs permitted as the crypto provider" grant; the ADR formalizes the extension so no ambiguity remains for downstream phases.

- **ADR-0019 — Phase 03 split into 03.1 + 03.2.** Lands at state 2 (plan-writer time), mirroring ADR-0013's pattern. The ADR cites this SPEC §5's anticipated split, the actual PLAN.md LoC overage that triggers §6.1, and the cut along the foundation-vs-extensions boundary.

Possible additional ADRs (only land if execution proves they're needed; not anticipated):

- TLS protocol-version pin if rustls and Envoy v1.33.0 negotiate differently in a way the differential harness catches. The fix is `tls_params { tls_minimum_protocol_version: TLSv1_3, tls_maximum_protocol_version: TLSv1_3 }` on both sides + a rustls `ClientConfig` / `ServerConfig` built with `with_protocol_versions(&[&rustls::version::TLS13])`.
- `cargo deny` for `aws-lc-rs` C bindings if licensing flags. Mirrors ADR-0005's testcontainers-tree precedent.
- Wildcard SNI semantics if Envoy v1.33.0 supports `*.example.com` in `filter_chain_match.server_names` and the multi-cert resolver needs to match it.
- `testcontainers` mount API extension if v0.23's `with_copy_to_container` is too awkward for the per-fixture tmpdir.

---

## 4. Non-goals (deferred to later phases)

- **HTTP-over-TLS** — phase 04 (HCM HTTP/1.1) ships the first ALPN-aware fixture; phase 05 (HTTP/2) makes ALPN load-bearing.
- **mTLS** (`require_client_certificate`, `validation_context.trust_chain_verification`, client cert presentation on upstream) — out of phase 03; lands when a fixture demands it.
- **Inline cert / key bytes** (`inline_string`, `inline_bytes`, `environment_variable` data sources) — phase 03 supports `filename` only.
- **`tls_params`** (cipher list, min/max TLS version, ECDH curves, signature algorithms) — fixture YAMLs omit; rely on rustls + Envoy defaults. ADR pins if execution drifts.
- **`auto_sni`, `auto_san_validation`, `allow_renegotiation`, `key_rotation`, `session_timeout`, `session_tickets`, `validation_context.match_typed_subject_alt_names`** — out of phase 03.
- **OCSP stapling, signed certificate timestamps, certificate transparency** — out of MVP trunk.
- **xDS-driven SDS** (Secret Discovery Service) — §9 family (xDS / dynamic config).
- **`Cluster::name()` accessor** — phase-02.2 REVIEW M1 carryforward; default-deferred to phase 06 unless 03.2 execution surfaces a need.
- **Unknown-SNI close behavior assertion in a fixture** — signposted; not asserted in fixture 0006. A future fixture may add a third probe with `sni: "unknown.example.com"` and `Driver::TlsTcpProbeList::expected_close: bool`.
- **`envoy.filters.network.sni_cluster`** (the network filter that routes to a cluster *named* by ClientHello SNI) — §9 network-filters family. Out of phase 03 per Q1's scope answer.
- **Distribution-equivalence on round-robin LB** — parent-brainstorm Q1 still unit-test-only; carries forward unchanged.
- **Multiple upstream certificates / SNI on the upstream side per cluster** — out of phase 03; one cluster, one upstream `sni`. A future fixture with per-endpoint SNI lands its own ADR.
- **Listener filters (`listener_filters`)** — out of phase 03; lands when a phase needs a listener filter (e.g., `tls_inspector` is the first one — it makes filter_chain_match work *without* TLS termination, which is a different pattern than phase 03's "TLS termination + match on server_names").
- **Filter chain framework / extension registry / per-route TLS config** — phase 07.
- **Stats subsystem, access logs, Prometheus** — phase 06.
- **Admin endpoints beyond phase 01's `/ready`** — phase 08.
- **`type: LOGICAL_DNS`, `type: STRICT_DNS`, `type: EDS`** — phase 03 still accepts only `STATIC` per phase-02.1's validator. (TLS does not unblock new cluster types.)
- **`lb_policy` variants beyond `ROUND_ROBIN`** — §9 load-balancing family.

---

## 5. Splitting guidance for the planner

This SPEC anticipates that state 2's plan-writer will trip `BOOTSTRAP_PROMPT.md` §6.1's LoC gate (~1500 LoC) and formally split phase 03 along the foundation-vs-extensions boundary that parent phase 02 used.

**LoC accounting (rough, planner refines at PLAN.md write):**

| Surface | 03.1 LoC | 03.2 LoC |
|---|---|---|
| `envoy-tls` core (cert/key loader + `ServerConfig` builder w/ single-cert resolver + `ClientConfig` builder + crypto provider install + 10 unit tests) | ~280 + ~150 | — |
| `envoy-tls` extensions (multi-cert SNI `ResolvesServerCert` + `from_listener` constructor + 5 unit tests) | — | ~150 + ~100 |
| `envoy-config` schema (transport_socket envelope + DownstreamTlsContext + UpstreamTlsContext + CommonTlsContext + TlsCertificate + CertificateValidationContext + DataSource + 10 validator tests) | ~200 + ~100 | — |
| `envoy-config` schema additions (FilterChainMatch + server_names overlap + catch-all rules + 6 validator tests) | — | ~80 + ~60 |
| `envoy-listener` TLS dispatch + `envoy-tcp` generic-stream lift + 4 envoy-tcp tests touching the generic shape | ~80 + ~50 | — |
| `envoy-cluster` upstream-TLS plumbing (or envoy-bin orchestration of UpstreamTls per D4 alt) + `envoy-tcp` upstream-TLS dial + 3 envoy-tcp tests | — | ~100 + ~80 |
| `envoy-bin` wiring (TlsAcceptingHandler + filter-chain dispatch + crypto provider install) + integration test `tls_downstream.rs` | ~100 + ~80 | — |
| `envoy-bin` wiring (multi-cert + upstream TLS) + integration tests `tls_upstream.rs` + `tls_sni.rs` | — | ~80 + ~150 |
| Harness `tls.rs` PKI + `Driver::TlsTcp` + `drive_tls` + render_yaml extensions + run_fixture dispatch + 4 unit tests | ~180 + ~100 | — |
| Harness `Driver::TlsTcpProbeList` + multi-probe `drive_tls_probes` + `TlsEchoBackend` + `upstream::start` mount extension + 2 unit tests | — | ~120 + ~80 |
| `tls-echo-server` helper crate (~120 impl + 5 tests) | — | ~120 + ~120 |
| Fixture 0004 (5 files) | ~80 | — |
| Fixture 0005 (5 files) | — | ~80 |
| Fixture 0006 (5 files) | — | ~120 |
| Phase-02.2 REVIEW M1 (Cluster::name accessor) — opportunistic, not load-bearing | — | ~5 (only if 03.2 surfaces a need) |
| ADR-0017 + ADR-0018 (docs) | — | — |
| ADR-0019 (split decision; lands at state 2 plan-writer) | — | — |
| **Total** | **~1400 LoC; ~13 tasks** | **~1445 LoC; ~14 tasks** |

Both sub-phases comfortably under §6.1's gates (~1500 LoC / ~25 tasks). **Do not split further.** If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` per parent phase 02 SPEC §5's precedent — nested splits of an already-split phase deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition).

**Cut line.** The split runs along this boundary:

- **03.1 — `envoy-tls` foundation + downstream TLS termination (single cert) + fixture 0004.** Slug `03.1-tls-foundation-downstream`. Inherits this SPEC's §3 D1 (envoy-tls 03.1 portion), D2 (envoy-config schema 03.1 portion), D3 (envoy-listener TLS dispatch — full), D4 03.1 portion (envoy-tcp generic-stream lift), D5 03.1 portion (envoy-bin wiring + `tls_downstream.rs`), D6 03.1 portion (harness PKI + `Driver::TlsTcp` + `drive_tls` + render_yaml keys for leaf-A and CA), D8 fixture 0004, D10 ADR-0017 + ADR-0018. **Not in 03.1:** UpstreamTls *consumer* wiring (UpstreamTls *struct + impl* are fine to land in 03.1 since they're library code with their own unit tests; the state-2 planner picks based on PLAN.md size whether to ship them in 03.1 or defer wholly to 03.2). Acceptance: stable-toolchain CI green; fuzz short-budget CI green on extended corpus; fixtures 0001/0002/0003 still green; new fixture 0004 green end-to-end. Depends on phase `02` (parent done at `f04e21a`).

- **03.2 — Upstream TLS origination + multi-cert SNI cert selection + `tls-echo-server` helper + fixtures 0005 + 0006.** Slug `03.2-tls-upstream-sni`. Inherits this SPEC's §3 D1 (envoy-tls 03.2 portion: SniResolver + `from_listener`), D2 03.2 portion (FilterChainMatch + overlap rules), D4 03.2 portion (UpstreamTls consumer; envoy-cluster or envoy-bin upstream-TLS plumbing), D5 03.2 portion (envoy-bin multi-cert + upstream wiring + integration tests), D6 03.2 portion (Driver::TlsTcpProbeList + TlsEchoBackend), D7 (tls-echo-server helper crate), D8 fixtures 0005 + 0006, D10 (no new ADRs unless execution surfaces). Acceptance: stable-toolchain CI green; fuzz short-budget CI green; fixtures 0001/0002/0003/0004 still green; fixtures 0005 + 0006 green end-to-end; phase-03 parent row flips to `done` in the 03.2 final commit (ROADMAP-schema invariant: "parent flips to `done` only after all sub-phases are `done`"). Depends on `03.1`.

The state-2 plan-writer lands ADR-0019 (split decision) in the same shape as ADR-0013 (parent phase 02 split). After that commit, this SPEC remains in-tree unedited as the parent-phase historical artifact; sub-phase SPECs are written fresh at each sub-phase's state-1 session, citing ADR-0019 for the split provenance and rewriting each expected ADR with its actual landed number.

**Phase-02.2 REVIEW carryforwards (status at phase-03 entry):**

- **M1 (`TcpProxyBackend::Drop` polling loop)** — tracked forward to whichever phase first parallelizes `run_fixture` across worker threads. Phase 03 does not parallelize fixtures (each is a single `cargo test` invocation). No action.
- **M2 (`proxies_returns_err_on_upstream_connect_refused` formatted-string assertion)** — awareness-only; no action in phase 03.
- **M3 (`proxies_closes_downstream_on_upstream_close` implicit timing)** — awareness-only; no action.
- **M4 (`Listener::serve` JoinSet type alias)** — phase 03 does not introduce a richer filter trait; no action. Phase 04+ may revisit.
- **REVIEW §4 recommendation 1 (`Cluster::name()` accessor)** — default-deferred to phase 06 per §1 baked-in defaults; opportunistic close-out in 03.2 allowed if a use case surfaces.
- **REVIEW §4 recommendation 2 (ADR-0017 numbering provisional)** — explicitly heeded throughout this SPEC.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution.

1. **Task ordering for 03.1:** ADR-0017 + ADR-0018 (task 1) → envoy-config schema additions (D2 03.1 portion) → envoy-tls scaffold (D1 03.1 portion: cert loader, ServerConfig builder, ClientConfig builder, single-cert resolver, ~10 unit tests) → envoy-tcp generic-stream lift (D4) → envoy-listener TLS dispatch via TlsAcceptingHandler (D3) → harness `tls.rs` + `Driver::TlsTcp` + `drive_tls` + render_yaml keys (D6) → envoy-bin wiring (D5) + in-process integration test → fixture 0004 (D8) + Docker-gated integration test → state-4 phase-done gate.

2. **Task ordering for 03.2:** envoy-config schema additions (D2 03.2 portion) → envoy-tls multi-cert SNI resolver + from_listener (D1 03.2 portion) + 5 unit tests → envoy-tls UpstreamTls consumer wiring (D4 03.2 portion) → envoy-bin multi-cert + upstream TLS wiring (D5 03.2 portion) → harness Driver::TlsTcpProbeList + drive_tls_probes + TlsEchoBackend (D6 03.2 portion) → tls-echo-server helper crate (D7) + 5 unit tests → fixtures 0005 + 0006 (D8) + Docker-gated integration tests → state-4 phase-done gate.

3. **`envoy-listener::ConnectionHandler` trait shape — keep concrete on `TcpStream` in 03.1.** Per D3 option α: envoy-listener's trait stays as in phase 02.2 (`fn handle(&self, downstream: TcpStream) -> BoxFuture<...>`). The TLS hop lives in envoy-bin's `TlsAcceptingHandler` adapter. envoy-tcp's `TcpProxy::handle` is generalized over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static` so the adapter can pass either `TcpStream` (plaintext path) or `TlsStream<TcpStream>` (post-handshake) into it. This minimizes the envoy-listener diff in 03.1 and keeps the adapter's TLS knowledge isolated to envoy-bin's wiring. If a future phase (04 or 07) builds a richer extension registry that needs trait-level genericity, that phase lands a new trait shape under its own ADR.

4. **Crypto provider initialization is per-process, idempotent, and `install_default()` returns `Err` on second call.** `rustls::crypto::aws_lc_rs::default_provider().install_default()` returns `Err(rustls::crypto::CryptoProvider)` if a provider has already been installed in the process. envoy-bin's `main` calls it once early; the harness's `run_fixture` calls it once per process (which is once per `cargo test` invocation, since each test process is fresh); `tls-echo-server`'s `main` calls it once. **Always ignore the `Err` return** — it indicates the second-or-later call, which is the desired no-op. Document the call site with a one-line comment naming the second-call-Err contract.

5. **`tokio-rustls` version pinning.** Plan-writer verifies `tokio-rustls` 0.26.x is the latest stable line at execution time and pins accordingly. The `aws-lc-rs` feature is selected via `default-features = false, features = ["aws-lc-rs"]`. If 0.26's API differs materially from the planner's expectation, no ADR is needed unless a new exemption surfaces (in which case ADR-0018 is amended at landing time, not edited post-hoc).

6. **rcgen API version.** rcgen 0.13.x is the latest at planning time. The `CertificateParams` builder shape is stable across 0.12–0.13; `KeyPair::generate(&PKCS_ECDSA_P256_SHA256)` is the canonical key generator (ECDSA P-256 keeps the key size small and TLS 1.3 happy). If rcgen 0.13's API differs, the plan-writer adjusts and notes in PROGRESS.md. Not an ADR surface.

7. **Container-side cert mount via `testcontainers`.** v0.23 exposes either `with_copy_to_container` (preferred — copies a host file/dir into the container at startup) or `with_mount` (bind-mounts; may be problematic on Docker Desktop on macOS due to the gRPC FUSE layer). Plan-writer picks `with_copy_to_container` first; falls back to `with_mount` only if a v0.23 quirk forces it. Either way, the container path is `/etc/envoy-rust-tls/`; both fixture 0004's envoy.yaml and fixture 0006's envoy.yaml reference paths under that directory. The harness writes a tiny `cert_paths.json` sidecar into the same dir for diagnostic logging (optional; defer if not needed).

8. **Unknown-SNI handling on a multi-cert listener.** Envoy v1.33.0's behavior: with `filter_chain_match.server_names` and no catch-all chain, an unknown SNI causes the connection to be closed (filter chain selection returns no match → listener drops). rustls's behavior: an unknown SNI in `SniResolver::resolve` returning `None` causes the rustls handshake to abort with TLS alert `unrecognized_name`. Both end states are "handshake fails, connection drops" but the wire alert differs. Phase-03 fixtures **do not assert** the unknown-SNI close behavior. Adding a third probe with `expected_close: bool` to `Driver::TlsTcpProbeList` is a future option that lands its own ADR (the TLS-alert delta vs. plain-close delta is potentially divergent between rustls and Envoy — better to reach for it once a fixture genuinely needs it).

9. **`#![forbid(unsafe_code)]` is mandatory** at every new crate's `lib.rs` / `main.rs`: `crates/envoy-tls/src/lib.rs` and `tests/helpers/tls-echo-server/src/main.rs`. aws-lc-rs's internal unsafe is shielded behind its crate's allowlist; no envoy-rust-owned code carries unsafe.

10. **Workspace membership.** Root `Cargo.toml` `[workspace] members` grows by `crates/envoy-tls` (03.1) and `tests/helpers/tls-echo-server` (03.2). Do not add either under `exclude`.

11. **Phase-02.2 REVIEW M1 (`Cluster::name()` accessor) carry-forward.** Default-deferred to phase 06 per §1 baked-in defaults. If 03.2 execution finds a use case (e.g., upstream-TLS error attribution that benefits from the cluster name in the error string), the executor closes it opportunistically (parent-phase precedent: phase-02.2 task 11 closed phase-02.1 REVIEW M3 opportunistically). Document the closure in PROGRESS.md and the sub-phase REVIEW.md §3 with the `#[allow(dead_code)]` removal and the cross-reference.

12. **`testcontainers`'s container-side path.** `/etc/envoy-rust-tls/` is the canonical mount target. Do not vary it across fixtures; the substituted YAML strings are identical on the envoy side regardless of which leaf cert the fixture uses.

13. **rustls `ResolvesServerCert::resolve` is synchronous.** It does not have access to async I/O. For phase 03 this is fine — the SniResolver's map is built once at startup from already-loaded certs. If a future phase needs SDS-backed cert lookup (xDS family), the resolver becomes a `Send + Sync` smart pointer to a snapshot that an async task swaps under a `parking_lot::RwLock` or `arc-swap`; that's deferred entirely.

14. **Cert lifetime in tests.** `TlsTestPki::generate()`'s `_tmpdir: TempDir` keeps PEMs alive for `run_fixture`'s entire duration. Drop fires after both proxies tear down (the upstream container is stopped first by testcontainers' Drop, then the envoy-rust subprocess by the harness's `Subject::Drop`, then the `TlsEchoBackend` Drop in 03.2, then `TlsTestPki` Drop). Mirrors `TcpProxyBackend`'s lifetime ordering from phase 02.2.

15. **ALPN absence.** Fixture YAMLs **do not** include `alpn_protocols`. envoy-tls's `ServerConfig` and `ClientConfig` builders **do not** call `with_alpn_protocols`. Phase 04 (HCM HTTP/1.1) is the first phase to add ALPN; phase 05 (HTTP/2) makes it load-bearing. Review should flag any phase-03 PR that "defensively" adds an ALPN list.

16. **Half-close posture (ADR-0016) carries forward unchanged.** `enable_half_close: false` is Envoy's v1.33.0 default for `tcp_proxy`. Phase-03 fixtures do not include the key; envoy-rust's `TcpProxy::handle` (now generic over the stream type) preserves the `tokio::select!`-over-two-`tokio::io::copy`-futures shape from phase 02.2. TLS does not propagate half-close any differently than plaintext for the byte-exact contract the harness asserts.

17. **`expected_cn` matching policy in `drive_tls`.** Walk both `subject_alt_name` (DNS entries) and CommonName; case-insensitive exact match. Wildcard SAN values (`*.example.com`) are not generated by `TlsTestPki` in phase 03, so no wildcard-match policy is needed. If a future phase needs wildcards on the harness side, that phase extends `drive_tls` and lands an ADR.

18. **Envoy `validation_context.match_subject_alt_names` deferral.** Envoy supports SAN-equality and SAN-prefix matching against the upstream cert's SAN. envoy-rust's phase-03 `UpstreamTls` does **not** support this — rustls's default verifier validates the cert chain against the trust bundle and asserts the cert's SAN matches the configured `ServerName`. That covers Envoy's behavior when `validation_context.match_subject_alt_names` is omitted, which is fixture 0005's posture. Adding `match_subject_alt_names` is a future-phase ADR.

19. **TLS handshake errors in `Listener::serve` and `TcpProxy::handle` are dropped, not propagated.** Per the phase-02.2 posture (per-connection errors → `tracing::warn!` and drop the connection; listener stays up), TLS handshake failures in `TlsAcceptingHandler::handle` log at `warn!` and return `Ok(())` to the JoinSet (or `Err` boxed; either way the listener stays up). The integration tests do **not** assert on log content; they only assert end-state successful handshakes complete byte round-trips.

20. **rustls-pki-types `ServerName::try_from` semantics.** `ServerName::try_from("envoy-rust.test")` returns `Ok(ServerName::DnsName(...))`. `ServerName::try_from("127.0.0.1")` returns `Ok(ServerName::IpAddress(...))` — but Envoy's `UpstreamTlsContext.sni` is documented to be a DNS name only (Envoy rejects IPs in `sni`). To match Envoy, `UpstreamTls::from_context` only accepts `ServerName::DnsName`; an IP literal in `sni` returns `TlsError::InvalidServerName`. Plan-writer encodes this in the validator + the unit-test enumeration.

21. **rustls server-side ClientHello SNI access.** rustls 0.23 exposes the SNI via `ClientHello::server_name()` inside `ResolvesServerCert::resolve(&self, client_hello: ClientHello)`. The returned `&str` is the lowercased SNI. The `SniResolver`'s map is keyed lowercase; the case-insensitive match policy is mechanically the lowercase-key lookup.

22. **Listener filter chain ordering and matching (envoy v1.33.0).** Envoy walks filter chains in declaration order; first match wins. The validator should not allow two filter chains to match the *same* SNI (`MultipleListenersWithOverlappingSni`), so first-match is unambiguous. The catch-all (empty `server_names`) is at most one chain (`MultipleCatchAllFilterChains`) and matches when no preceding chain's `server_names` match. Phase-03 fixture 0006 has no catch-all chain — both chains have explicit `server_names` — so unknown-SNI fails the handshake (signpost 8).

---

## 7. ADRs expected from this phase

Three ADRs land during phase 03 execution (across both sub-phases), in `docs/envoy-rust/DECISIONS.md`, in order. Numbering provisional per phase-02.2 REVIEW recommendation #2.

### ADR-0017 (provisional) — `rcgen` and `tempfile` permitted as dev-test-harness-only foundations

- **Lands in 03.1 task 1.**
- **Context:** Phase 03 is the first phase to need test certificates. TLS test infrastructure recurs across phases 03–08+ (HTTP/1.1 over TLS, H2 over TLS, mTLS, etc.). Static in-tree PEMs were considered and rejected per the brainstorm Q2 decision (poor refresh ergonomics, expiry concerns, multi-leaf cert generation gets unwieldy). `rcgen` is the maintained Rust-native cert generator; `tempfile` is the canonical per-test-run tmpdir manager. Neither is on the D-3.2 permitted-foundations list at phase-02.2 close.
- **Options considered:** (i) static in-tree PEMs (rejected, brainstorm Q2); (ii) `rcgen` + `tempfile` on the permitted list as dev-test-harness-only (decision); (iii) script-generated PEMs committed to the repo (rejected, brainstorm Q2: worst-of-both-worlds).
- **Decision:** add `rcgen = "0.13"` and `tempfile = "3"` to the permitted-foundations list with the **dev-test-harness-only** annotation. Mirrors ADR-0009's posture for `cargo-fuzz` + `libfuzzer-sys`. Never a transitive of `envoy-bin` or any non-test workspace crate. Restricted to: `tests/differential/` dev-deps; `tests/helpers/tls-echo-server/` dev-deps; `crates/envoy-tls/` dev-deps (for unit-test PKI).
- **Rationale:** one-time foundations grant beats per-phase ADR churn; rcgen is the Rust-ecosystem default; tempfile is ubiquitous test-infra. Test-only restriction preserves D-3.2's spirit for runtime code.
- **Consequences:** future TLS-cert-using phases (04 HCM-over-TLS, 05 H2-over-TLS, mTLS phases, etc.) reuse this decision without per-phase ADRs. `cargo deny check` may flag the rcgen license (Apache-2.0 OR MIT — both on the deny.toml allow-list) or its transitive deps; if so, the deny.toml is updated alongside ADR-0017's landing. If a future phase needs cert generation in *runtime* code (e.g., hot-restart cert rotation), that phase lands a new ADR superseding the dev-test-harness-only restriction.

### ADR-0018 (provisional) — `tokio-rustls` and `rustls-pemfile` covered by the rustls foundations grant

- **Lands in 03.1 task 1, alongside ADR-0017.**
- **Context:** D-3.2 lists `rustls`, `webpki`, `rustls-pki-types`, and "`aws-lc-rs` permitted as the crypto provider," but does not name `tokio-rustls` or `rustls-pemfile` explicitly. Both are mechanically necessary to use rustls inside a tokio runtime / load PEMs from disk; both ship from the rustls org.
- **Options considered:** (i) treat both as covered implicitly by the rustls grant — risks ambiguity; (ii) land an ADR formalizing the extension (decision); (iii) hand-roll the async glue and PEM parser — reinvents wheels D-3.2 explicitly tells us not to.
- **Decision:** extend D-3.2's "rustls + aws-lc-rs permitted as the crypto provider" grant to cover `tokio-rustls` and `rustls-pemfile`. Both are runtime-permitted (not dev-only); rcgen + tempfile from ADR-0017 stay dev-only.
- **Rationale:** removes ambiguity for downstream phases. Both crates are first-party in the rustls ecosystem; treating them as part of the same foundation is the cheapest, most honest formalization.
- **Consequences:** envoy-tls's `Cargo.toml` lists both as direct deps. `tls-echo-server`'s `Cargo.toml` lists both. Neither is allowed in `envoy-listener` or `envoy-cluster` — those crates remain rustls-free per D1's "envoy-tls is the only crate with rustls deps" architectural rule.

### ADR-0019 (provisional) — Phase 03 split into 03.1 + 03.2

- **Lands at state 2 (plan-writer time).**
- **Context:** This SPEC's §5 estimates ~2845 LoC of net change across ~27 tasks for a single phase 03. Both §6.1 thresholds (~25 tasks, ~1500 LoC) are crossed (LoC ~90% over the gate; tasks marginally over).
- **Options considered:** (i) accept the overage and write a single `PLAN.md` (rejected per parent phase 02 ADR-0013's pattern: §6.1 is `triggered if either threshold is crossed`); (ii) split at a custom boundary not anticipated in this SPEC §5 (rejected: the foundation-vs-extensions cut is the natural one); (iii) split at this SPEC §5's designated boundary (decision).
- **Decision:** split at this SPEC's §5-designated boundary: 03.1 ships the envoy-tls scaffold + downstream-TLS basics + fixture 0004; 03.2 ships upstream-TLS + multi-cert SNI + tls-echo-server + fixtures 0005 + 0006. Each sub-phase has one clean direction of dependency (03.1 → 03.2); each is individually under both §6.1 gates.
- **Rationale:** mirror ADR-0013's pattern. The SPEC §5 boundary is the one the brainstorm validated.
- **Consequences:** ROADMAP.md row 03 flips `status` → `in-progress` and `sub-phases` → `03.1, 03.2`. Two new rows land with `status = planned`: row 03.1 depends-on `02`; row 03.2 depends-on `03.1`. Row 03's `status` flips to `done` only after both sub-phases land. ADR numbering for any in-execution ADRs in 03.1 / 03.2 picks up at the next-sequential available number (e.g., if no other ADR lands first, 03.1's task-1 ADRs are ADR-0017 + ADR-0018 + ADR-0019 in that order; the split ADR is last). This SPEC stays in-tree unedited as the parent-phase historical artifact, last touched at the SHA of this SPEC's commit.

---

## 8. Artifacts this phase produces

Created during execution (relative to repo root). Distribution between sub-phases follows §5's cut line.

03.1:
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/SPEC.md` (written at 03.1 state-1; cites ADR-0019 for the split)
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PLAN.md`
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md`
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/REVIEW.md`
- `crates/envoy-tls/Cargo.toml`
- `crates/envoy-tls/src/lib.rs`
- `tests/differential/src/tls.rs`
- `crates/envoy-bin/tests/tls_downstream.rs`
- `tests/differential/tests/tls_downstream.rs`
- `tests/fixtures/0004-tls-downstream/envoy.yaml`
- `tests/fixtures/0004-tls-downstream/envoy-rust.yaml`
- `tests/fixtures/0004-tls-downstream/inputs/payload.bin`
- `tests/fixtures/0004-tls-downstream/expectations.yaml`
- `tests/fixtures/0004-tls-downstream/README.md`

03.2:
- `docs/envoy-rust/phases/03.2-tls-upstream-sni/SPEC.md`
- `docs/envoy-rust/phases/03.2-tls-upstream-sni/PLAN.md`
- `docs/envoy-rust/phases/03.2-tls-upstream-sni/PROGRESS.md`
- `docs/envoy-rust/phases/03.2-tls-upstream-sni/REVIEW.md`
- `tests/helpers/tls-echo-server/Cargo.toml`
- `tests/helpers/tls-echo-server/src/main.rs`
- `crates/envoy-bin/tests/tls_upstream.rs`
- `crates/envoy-bin/tests/tls_sni.rs`
- `tests/differential/tests/tls_upstream.rs`
- `tests/differential/tests/tls_sni.rs`
- `tests/fixtures/0005-tls-upstream/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}`
- `tests/fixtures/0006-tls-sni/{envoy.yaml,envoy-rust.yaml,inputs/payload.bin,expectations.yaml,README.md}`

Amended during execution (across sub-phases):

- Root `Cargo.toml` — add `crates/envoy-tls` (03.1) and `tests/helpers/tls-echo-server` (03.2) to `[workspace] members`.
- `crates/envoy-bin/Cargo.toml` — add `envoy-tls` path-dep (03.1); add dev-deps on `tokio-rustls`, `rcgen`, `tempfile` for the integration tests (03.1 and 03.2 incrementally).
- `crates/envoy-bin/src/main.rs` — install aws-lc-rs default crypto provider (03.1); construct `Arc<DownstreamTls>` per filter chain when `transport_socket` is present (03.1); wrap in `TlsAcceptingHandler` (03.1); construct `Arc<UpstreamTls>` per cluster with TLS transport_socket (03.2); thread through to per-cluster `TcpProxy` (03.2).
- `crates/envoy-config/src/bootstrap.rs` — `TransportSocket` envelope, `TransportSocketTypedConfig`, `DownstreamTlsContext`, `UpstreamTlsContext`, `CommonTlsContext`, `TlsCertificate`, `CertificateValidationContext`, `DataSource` (03.1); `FilterChainMatch` + `server_names` (03.2); the `validate` extension with new `ConfigError` variants (split per sub-phase).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — TLS-shaped seeds (03.1 and 03.2 incrementally).
- `crates/envoy-tcp/src/lib.rs` — generalize `TcpProxy::handle` over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static` (03.1); add `Option<Arc<UpstreamTls>>` field + upstream-TLS dial (03.2). New tests in both sub-phases.
- `crates/envoy-listener/src/lib.rs` — minimal or zero edits in 03.1 (the `TlsAcceptingHandler` lives in envoy-bin; trait stays as-is per signpost 3). 03.2 may add a one-line edit to surface the per-handler TLS metadata if the planner finds a use case.
- `tests/differential/Cargo.toml` — add `rcgen`, `tempfile`, `tokio-rustls`, `rustls`, `rustls-pki-types`, `rustls-pemfile` as dev-deps (03.1).
- `tests/differential/src/lib.rs` — `Driver::TlsTcp` variant (03.1); `Driver::TlsTcpProbeList` variant (03.2); `drive_tls` helper (03.1); `drive_tls_probes` helper (03.2); `render_yaml` substitution-key extensions per sub-phase; `run_fixture` dispatch extensions per sub-phase; new unit tests in both sub-phases.
- `tests/differential/src/upstream.rs` — extend `start` signature with `tls_pki: Option<&TlsTestPki>` for container-side cert mounting (03.1).
- `tests/differential/src/backend.rs` — add `TlsEchoBackend` (03.2).
- `docs/envoy-rust/DECISIONS.md` — ADR-0017 + ADR-0018 + ADR-0019 appended in order.
- `docs/envoy-rust/ROADMAP.md` — row 03 `status` → `in-progress` and `sub-phases` → `03.1, 03.2` at the split commit (state 2 of 03.1's parent context); rows 03.1 + 03.2 added with `status = planned` at the same commit; rows flip to `done` as each sub-phase lands; row 03 flips to `done` at 03.2's final commit (per ROADMAP schema "parent flips to `done` only after all sub-phases are `done`").
- `docs/envoy-rust/STATE.md` — advanced through every state transition per BOOTSTRAP_PROMPT §5.1 ("one state per session").
- `deny.toml` — only if `cargo deny check` flags new licenses or transitive surfaces from the rustls / aws-lc-rs / tokio-rustls / rcgen / tempfile chain. Most likely a no-op; a non-trivial extension lands its own ADR.

Not touched in phase 03 (frozen):

- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent of phase 02; unedited per D-3.4 / D-3.5; last touched at SHA `50349da`).
- `docs/envoy-rust/phases/02.1-config-cluster/` and `docs/envoy-rust/phases/02.2-listener-tcp-proxy/` — closed in phase 02.
- `tests/helpers/tcp-echo-server/` — finalized in 02.1.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/` — unedited; their fixtures must remain green at phase-03 state-4 gate.
- `crates/envoy-cluster/src/lib.rs` — touched only if D4's "envoy-cluster carries `upstream_tls`" alternative is taken (recommended alternative: envoy-bin orchestrates instead, leaving envoy-cluster TLS-free).
- `BEHAVIOR_CONTRACT.md` — no edits in phase 03 per §1's baked-in defaults.

---

## 9. Final commit message format (parent row 03 `done` commit, lands at 03.2 final)

```
phase 03: TLS termination + upstream origination + SNI [ADR-0017,ADR-0018,ADR-0019]

Two new layers of the data plane: envoy-tls owns rustls server/client config
construction with single-cert and SNI-keyed cert resolvers (downstream) and
trust-bundle-validated upstream origination with SNI on the wire. envoy-bin
gains transport-socket dispatch on both sides; envoy-tcp generalizes over
AsyncRead+AsyncWrite for plaintext-or-TLS streams. New differential harness
PKI (rcgen-driven) and Driver::TlsTcp{,ProbeList}. New tls-echo-server helper
for upstream-TLS fixtures.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (single-cert downstream TLS);
  tests/fixtures/0005-tls-upstream green (upstream TLS origination + SNI);
  tests/fixtures/0006-tls-sni green (multi-cert SNI cert selection).
Conformance: none.
```

This format applies to the parent-row `done` commit, which lands at the end of 03.2 alongside ROADMAP row 03 flipping to `done`. Each sub-phase has its own SPEC §9 final-commit format committed in the sub-phase SPEC at split time.
