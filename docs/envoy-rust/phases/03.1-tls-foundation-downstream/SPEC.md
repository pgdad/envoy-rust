# Phase 03.1 — `envoy-tls` foundation + downstream TLS termination + fixture 0004

- **Phase id:** `03.1`
- **Parent phase:** `03-tls-tcp` (split per ADR-0017)
- **Title:** `envoy-tls` foundation + downstream TLS termination (single cert) + fixture 0004
- **Depends on:** `02` (done as of commit `f04e21a`). Both phase-02 sub-phases `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`) are landed.
- **Differential surface when done:** one new fixture green against upstream `envoyproxy/envoy:v1.33.0` — `tests/fixtures/0004-tls-downstream/` (single-cert downstream TLS termination, plaintext upstream backend). Pre-existing fixtures `0001-tcp-echo`, `0002-static-admin-ready`, and `0003-tcp-proxy` remain green.
- **Seeded by:** `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (parent, committed at SHA `a3f3474`) §§D1 (03.1 portion), D2 (03.1 portion), D3, D4 (03.1 portion), D5 (03.1 portion), D6 (03.1 portion), D8 (fixture 0004), D10 (ADR-0018, ADR-0019); split decision at ADR-0017.

This SPEC is the design contract for sub-phase 03.1. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) must be able to execute it without consulting the parent `03-tls-tcp/SPEC.md`.

---

## 1. Goal and acceptance signal

**Goal.** Land the first TLS-aware data-plane path in the project: a listener whose filter chain declares `transport_socket: envoy.transport_sockets.tls (DownstreamTlsContext)` accepts a TLS connection, completes the rustls server handshake against a configured single cert/key pair, and hands the decrypted byte stream to the chain's `tcp_proxy` filter (which then routes plaintext to a static-cluster backend, exactly as in phase 02.2). The new `crates/envoy-tls/` library crate owns every workspace dependency on `rustls` + `aws-lc-rs` + `rustls-pki-types` + `tokio-rustls` + `rustls-pemfile`, mediated by `DownstreamTls::from_context` (cert/key loader + `ServerConfig` builder w/ a single-cert `ResolvesServerCert`) and `DownstreamTls::accept` (post-handshake `tokio_rustls::server::TlsStream<TcpStream>` returner). The companion `UpstreamTls` library API (`from_context` + `connect`) lands in 03.1 as well — its consumers wire up in 03.2, but the library code + unit tests are scoped here so 03.1 ships a complete, separately-reviewable envoy-tls crate.

`envoy-listener::ConnectionHandler` stays concrete on `tokio::net::TcpStream` per parent-SPEC §6 signpost 3 option α; the TLS hop lives in a new `TlsAcceptingHandler` adapter inside `envoy-bin`'s wiring that wraps an inner `Arc<TcpProxy>` and runs `DownstreamTls::accept` before delegating. `envoy-tcp::TcpProxy::handle` is generalized over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static` (replacing its phase-02.2 concrete-on-`TcpStream` shape) so the adapter can pass either the post-handshake `TlsStream<TcpStream>` (TLS chains) or a plaintext `TcpStream` (plaintext chains) into the same proxy code.

The harness gains a one-shot rcgen-driven test PKI (`TlsTestPki::generate` — CA + leafs `a.example.com` + `b.example.com` + `envoy-rust.test`, written to a per-fixture `TempDir`); a new `Driver::TlsTcp { sni, expected_cn }` variant; a `drive_tls` helper that mirrors `drive_tcp`'s ADR-0006/0007 read-exact + 100ms trailing-byte poll discipline on top of a `tokio_rustls::TlsConnector` handshake; and per-side `render_yaml` substitution keys for cert/CA file paths (`{{LEAF_A_CERT_PATH}}`, `{{LEAF_A_KEY_PATH}}`, `{{CA_PATH}}`).

Sub-phase 03.1 does **not** ship the SNI multi-cert resolver (`SniResolver`), the upstream-TLS consumer wiring (`UpstreamTls` consumer in `envoy-tcp` / `envoy-bin`), the `tls-echo-server` helper, fixture 0005 (upstream TLS), or fixture 0006 (multi-cert SNI). Those land in 03.2.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 03.1's feature surface:

- (a) the new differential fixture `tests/fixtures/0004-tls-downstream/` is green;
- (b) the pre-existing differential fixtures `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, and `tests/fixtures/0003-tcp-proxy/` remain green;
- (c) no conformance suites run this sub-phase (first one — `h2spec` — attaches in phase 05);
- (d) the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against an extended corpus that now includes 3 new TLS-shaped seeds (downstream-TLS happy path; malformed `@type`; `UpstreamTlsContext` with `validation_context`); no new fuzz target ships this sub-phase;
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- (f) `REVIEW.md` for this sub-phase is approved.

**Scope shape (inherited from parent-phase brainstorm).** Of the four scope-shape forks resolved during the parent-phase-03 state-1 brainstorm, the three that bind on 03.1 are:

1. **Cert provisioning — rcgen + new ADR.** Test certificates are generated at harness time by a new `TlsTestPki` module backed by the `rcgen` crate (added to the D-3.2 permitted-foundations list as **dev-test-harness-only** via this sub-phase's ADR-0018, renumbered from parent-SPEC §7's projected ADR-0017). PEMs land in a per-fixture `TempDir`; both proxies reference the same paths via `render_yaml` substitution. No PEMs are committed to the repo. `tempfile = "3"` rolls under the same dev-test-harness-only umbrella in ADR-0018.
2. **Crate layout — new `envoy-tls` library crate.** All TLS-specific code lives in `crates/envoy-tls/`. `envoy-listener` and `envoy-cluster` do *not* depend on rustls directly. Matches the §4 "one crate per primitive" pattern.
3. **Fixture distribution.** Fixture 0004 (single-cert downstream TLS, plaintext upstream) lands in 03.1.

The remaining brainstorm fork (multi-cert SNI cert selection on the downstream + wire-level SNI on the upstream) binds on 03.2 only.

---

## 2. Behavior-contract scope for sub-phase 03.1

Sub-phase 03.1 exercises only **row 2** of the `BEHAVIOR_CONTRACT.md` §7.2 equivalence matrix:

- **Response body — Byte-exact for deterministic handlers.** Fixture 0004 uses a TCP-echo data plane: Envoy's `tcp_proxy` filter routes the post-handshake decrypted byte stream to the host-local `tcp-echo-server` helper (landed in phase 02.1). The new `drive_tls` helper inherits ADR-0006/0007's `read_exact(payload.len())` + 100ms trailing-byte poll discipline; the post-handshake byte stream is the differential surface. The TLS handshake itself is exercised end-to-end by virtue of the connection succeeding, but no byte of the TLS record layer is asserted directly.

No other dimension is engaged. No response status (TCP, no HTTP). No access logs (phase 06). No stats (phase 06). No headers (phase 04 for HTTP; TCP has none). No xDS (§9 family).

**No `BEHAVIOR_CONTRACT.md` edits in 03.1.** The currently-empty subsections (`Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances`) remain empty.

---

## 3. Deliverables

### D1 — New library crate `crates/envoy-tls/`

Added to the root `Cargo.toml` `[workspace] members`. Owns all TLS-specific code; the only crate in the workspace that depends on `rustls`, `tokio-rustls`, `rustls-pki-types`, `rustls-pemfile`, or aws-lc-rs.

- `crates/envoy-tls/Cargo.toml`. `edition = "2024"`, `publish = false`, `license = "Apache-2.0"`. Dependencies from D-3.2 + ADR-0019 (renumbered from parent-SPEC ADR-0018) only:
  - `envoy-config = { path = "../envoy-config" }`
  - `rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }`
  - `rustls-pki-types = "1"`
  - `rustls-pemfile = "2"` (covered by ADR-0019)
  - `tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }` (covered by ADR-0019)
  - `tokio = { version = "1", features = ["net", "io-util", "macros", "sync"] }`
  - `thiserror = "2"`
  - `tracing = "0.1"`

  Dev-deps: `tokio` adds `rt-multi-thread` for tests; `rcgen = "0.13"` for unit-test cert generation (covered by ADR-0018, dev-test-harness-only); `tempfile = "3"` (covered by ADR-0018).

  The `aws-lc-rs` crypto provider is selected via the `tokio-rustls` `aws-lc-rs` feature; the `ring` provider is **not** brought in. Plan-writer verifies feature names against the actual `tokio-rustls` 0.26.x API at execution time.

- `crates/envoy-tls/src/lib.rs` starts with `#![forbid(unsafe_code)]` per D-3.8. Public surface:

    ```rust
    pub struct DownstreamTls {
        config: std::sync::Arc<rustls::ServerConfig>,
    }

    impl DownstreamTls {
        /// Build from a parsed envoy_config::DownstreamTlsContext.
        ///
        /// 03.1 (this sub-phase): single-cert path. Loads cert+key PEMs from the
        /// configured filenames; constructs a SingleCertResolver wrapping the
        /// resulting CertifiedKey. Rejects empty `tls_certificates` with
        /// TlsError::DownstreamRequiresCert.
        ///
        /// 03.2 will add a SNI-keyed ResolvesServerCert via a separate
        /// `from_listener` constructor; this `from_context` constructor remains
        /// the single-cert entry point.
        pub fn from_context(cfg: &envoy_config::DownstreamTlsContext)
            -> Result<Self, TlsError>;

        /// Hands a connected downstream TcpStream through the rustls server
        /// handshake; returns the post-handshake stream. On handshake failure
        /// returns TlsError::Handshake; the listener's accept loop logs at warn!
        /// and drops the connection per the same posture as phase 02.2's
        /// per-connection error handling.
        pub async fn accept(
            &self,
            downstream: tokio::net::TcpStream,
        ) -> Result<tokio_rustls::server::TlsStream<tokio::net::TcpStream>, TlsError>;
    }

    pub struct UpstreamTls {
        config: std::sync::Arc<rustls::ClientConfig>,
        server_name: rustls::pki_types::ServerName<'static>,
    }

    impl UpstreamTls {
        /// Build from a parsed envoy_config::UpstreamTlsContext. Loads the CA
        /// PEM from `validation_context.trusted_ca.filename` into a
        /// rustls::RootCertStore; builds a ClientConfig with that root store,
        /// no client auth (mTLS deferred), default cipher suites/protocols.
        /// Parses `cfg.sni` into a ServerName::DnsName via
        /// rustls-pki-types::ServerName::try_from; rejects IP literals
        /// (Envoy's UpstreamTlsContext.sni is documented DNS-name-only).
        pub fn from_context(cfg: &envoy_config::UpstreamTlsContext)
            -> Result<Self, TlsError>;

        /// Hands a connected upstream TcpStream through the rustls client
        /// handshake; returns the post-handshake stream.
        ///
        /// 03.1 ships the implementation + unit tests; 03.2 wires consumers
        /// (envoy-tcp's TcpProxy::handle gains an Option<Arc<UpstreamTls>>
        /// field; envoy-bin builds the Arc<UpstreamTls> per cluster with
        /// transport_socket: Upstream(...)).
        pub async fn connect(
            &self,
            upstream: tokio::net::TcpStream,
        ) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>, TlsError>;
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

- **Cert/key loader.** A helper `load_certified_key(cert_path, key_path) -> Result<rustls::sign::CertifiedKey, TlsError>` reads both PEMs via `std::fs::read`, extracts `Vec<rustls_pki_types::CertificateDer>` with `rustls_pemfile::certs`, extracts the private key with `rustls_pemfile::private_key`, and builds a `rustls::sign::CertifiedKey` using the `aws-lc-rs` provider's `any_supported_type` signing-key constructor. Returns `TlsError::CertParse` / `TlsError::KeyParse` on empty or malformed parses, `TlsError::FileRead` on I/O failures.

- **Single-cert `ResolvesServerCert` (03.1).** `struct SingleCertResolver(Arc<CertifiedKey>);` impl of `rustls::server::ResolvesServerCert` returns the wrapped `CertifiedKey` for any ClientHello regardless of SNI. The `ServerConfig` is built with `.with_cert_resolver(Arc::new(SingleCertResolver(key)))` (rather than the simpler `.with_single_cert(...)`) — this keeps the 03.2 SNI extension drop-in: the `ServerConfig`-building seam is already a resolver from day one.

- **`ClientConfig` builder (03.1, consumers in 03.2).** Builds with `.with_root_certificates(roots)` where `roots: rustls::RootCertStore` is populated from the configured `validation_context.trusted_ca` PEM (the loader extracts each `CertificateDer` and calls `RootCertStore::add` per cert). No system-roots fallback in phase 03 (deferred to a later phase that needs it). `.with_no_client_auth()` (mTLS deferred).

- **rustls crypto provider initialization.** `envoy-tls` does **not** call `install_default()` itself (it's a library; libraries must not unilaterally `install_default`). The call is `rustls::crypto::aws_lc_rs::default_provider().install_default()` and lives in the binary crates' `main` (`envoy-bin` in 03.1; `tls-echo-server` in 03.2). The `install_default` API returns `Err(rustls::crypto::CryptoProvider)` on the second-or-later call; the binary crates always ignore the `Err` return — it indicates the no-op second call. SPEC §6 signpost 4 documents this contract.

- **Unit tests in `crates/envoy-tls/src/lib.rs::tests`** (10 tests; covers both `DownstreamTls` 03.1 surface and `UpstreamTls` library API since the impl ships in 03.1 even though consumer wiring lands in 03.2):

  - `loads_single_cert_server_config` — rcgen-built PEMs in tmpdir; `DownstreamTls::from_context` returns `Ok`; `accept` against a connected pair (in-process `TcpListener::bind(("127.0.0.1", 0))` + `TcpStream::connect`) completes the handshake; both peers see a TLS version ≥ 1.2.
  - `rejects_empty_tls_certificates` — `DownstreamTlsContext` with empty `tls_certificates` → `TlsError::DownstreamRequiresCert`.
  - `rejects_malformed_cert_pem` — `tls_certificates[0].certificate_chain.filename` points at a file with no PEM headers → `TlsError::CertParse`.
  - `rejects_missing_key_pem` — file does not exist → `TlsError::FileRead`.
  - `loads_upstream_client_config` — `UpstreamTls::from_context` returns `Ok`; the produced `ClientConfig`'s root store contains the harness CA; `connect` against a TLS-listening counterpart (in-test `tokio_rustls::TlsAcceptor` with an rcgen-built server cert signed by the same CA) completes the handshake.
  - `upstream_rejects_invalid_sni` — `sni: "127.0.0.1"` (an IP literal, which `rustls-pki-types::ServerName::try_from` accepts as `ServerName::IpAddress` but Envoy's `UpstreamTlsContext.sni` is DNS-name-only, so envoy-rust rejects it) → `TlsError::InvalidServerName`.
  - `upstream_rejects_untrusted_cert` — server cert signed by a CA not in the configured trust bundle → handshake fails with `TlsError::Handshake { source: ... }` (the `source` is a `std::io::Error` carrying a `rustls::Error::InvalidCertificate(...)` inside, per `tokio-rustls`'s error mapping).
  - `single_cert_resolver_returns_same_cert_regardless_of_sni` — three different SNIs (`a.example.com`, `b.example.com`, `unknown.example.com`); resolver returns the same `Arc<CertifiedKey>` for each.
  - `crypto_provider_install_is_idempotent` — calling `aws_lc_rs::default_provider().install_default()` twice in the same test process: first call returns `Ok(())`, second returns `Err(_)` (documented and ignored). Test asserts no panic.
  - `accept_returns_handshake_error_on_garbage_input` — connect with a plain-TCP client that writes `b"GET / HTTP/1.1\r\n\r\n"` instead of a ClientHello → `TlsError::Handshake`.

### D2 — `envoy-config` schema extensions (03.1 portion)

`crates/envoy-config/src/bootstrap.rs` gains the `transport_socket` envelope, both TLS context types, the data-source struct, and the optional `transport_socket` field on `Cluster`. The `Node` open-schema asymmetry from phase 01 is **not** widened.

The `FilterChainMatch` struct + `server_names` field land in 03.2.

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    // 03.2 adds: filter_chain_match: Option<FilterChainMatch>
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
    /// Server Name sent in the ClientHello server_name extension. Phase 03
    /// requires this on every UpstreamTlsContext (no auto_sni). The validator
    /// rejects an empty string.
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

// On Cluster — schema in 03.1 (consumers in 03.2):
pub struct Cluster {
    // … existing fields from phase 02.1 …
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
}
```

**Validator extensions** in `envoy-config::bootstrap::validate` — new `ConfigError` variants (03.1 portion):

- `UnknownTransportSocketName(String)` — only `"envoy.transport_sockets.tls"` accepted in phase 03.
- `MismatchedTransportSocketDirection { side: &'static str, got: &'static str }` — `DownstreamTlsContext` is invalid as a cluster's upstream `transport_socket` (`side: "cluster", got: "DownstreamTlsContext"`); `UpstreamTlsContext` is invalid as a filter-chain's downstream `transport_socket` (`side: "listener", got: "UpstreamTlsContext"`).
- `EmptyTlsCertificates` — `DownstreamTlsContext.common_tls_context.tls_certificates` must be ≥ 1 (downstream side); upstream `CommonTlsContext.tls_certificates` must be 0 (no client cert in phase 03 — mTLS deferred). Variant carries `side: &'static str` to disambiguate.
- `MissingValidationContext` — `UpstreamTlsContext` requires a `validation_context.trusted_ca` (no insecure-skip in phase 03).
- `EmptyUpstreamSni` — `UpstreamTlsContext.sni` must be a non-empty string.

The 03.2 sub-phase adds `MultipleListenersWithOverlappingSni { listener: String, sni: String }` and `MultipleCatchAllFilterChains { listener: String }`.

**Validator unit tests appended to `crates/envoy-config/src/bootstrap.rs::tests` (10 tests):**

- `parses_listener_with_downstream_tls_context` — full happy-path fixture (listener with one filter chain carrying `transport_socket: envoy.transport_sockets.tls (DownstreamTlsContext)` + `tls_certificates[0]` referencing two filename data sources + `tcp_proxy → backend` filter; cluster `backend` plaintext STATIC).
- `parses_cluster_with_upstream_tls_context` — cluster carrying `transport_socket: envoy.transport_sockets.tls (UpstreamTlsContext)` with `sni: "envoy-rust.test"` + `validation_context.trusted_ca`.
- `rejects_unknown_transport_socket_name` — `name: "envoy.transport_sockets.raw_buffer"` → `ConfigError::UnknownTransportSocketName`.
- `rejects_downstream_tls_context_on_cluster` — cluster's `transport_socket.typed_config.@type` is `DownstreamTlsContext` → `ConfigError::MismatchedTransportSocketDirection { side: "cluster", got: "DownstreamTlsContext" }`.
- `rejects_upstream_tls_context_on_listener` — filter chain's `transport_socket.typed_config.@type` is `UpstreamTlsContext` → `ConfigError::MismatchedTransportSocketDirection { side: "listener", got: "UpstreamTlsContext" }`.
- `rejects_downstream_with_empty_tls_certificates` — downstream context with `tls_certificates: []` → `ConfigError::EmptyTlsCertificates { side: "listener" }`.
- `rejects_upstream_with_tls_certificates` — upstream context with non-empty `tls_certificates` → `ConfigError::EmptyTlsCertificates { side: "cluster" }` (variant naming asymmetric: "Empty" on downstream means too-few; on upstream means too-many — keep the variant name and use the `side` field to disambiguate; the error message reflects the side-specific meaning via the `Display` impl).
- `rejects_upstream_without_validation_context` — upstream context with no `validation_context` → `ConfigError::MissingValidationContext`.
- `rejects_upstream_with_empty_sni` — upstream context with `sni: ""` → `ConfigError::EmptyUpstreamSni`.
- `rejects_unknown_field_in_downstream_tls_context` — `deny_unknown_fields` regression on `DownstreamTlsContext` (e.g. `require_client_certificate: false` is rejected — that field is mTLS-shaped and out of phase 03 per parent-SPEC §4).

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 3 new TLS-shaped seeds (03.1):

- `tls_downstream_single_cert.yaml` — a full bootstrap with listener → filter chain with `DownstreamTlsContext` + single `tls_certificate` + `tcp_proxy → backend`; cluster `backend` plaintext STATIC, one endpoint. Seed paths use plausible-but-irrelevant file paths (`/tmp/cert.pem`, `/tmp/key.pem`); the fuzzer never opens them — this seed exercises only the parse path, not the loader.
- `tls_malformed_at_type.yaml` — same shape as the happy-path seed but the `@type` URL is `type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UnknownContext`, a deliberately invalid value. Exercises serde's tagged-enum default rejection on the `TransportSocketTypedConfig` enum.
- `tls_upstream_validation_context.yaml` — listener plaintext (no `transport_socket`); cluster carries `transport_socket: UpstreamTlsContext` with `sni: "envoy-rust.test"` + `validation_context.trusted_ca.filename`. Exercises the upstream-direction parse path.

The existing `parse_bootstrap` target picks them up automatically; no new fuzz target ships. The fuzz job's `-max_total_time=30` budget (per ADR-0010) is unchanged.

### D3 — `envoy-listener` TLS dispatch (full)

`crates/envoy-listener/src/lib.rs` does **not** directly depend on `envoy-tls` (avoids leaking rustls types into the listener crate). The existing `ConnectionHandler` trait is the seam: `envoy-bin` constructs a `TlsAcceptingHandler` adapter that wraps the inner `Arc<TcpProxy>` with a TLS-accept hop. The listener crate's diff in 03.1 is **zero** — `ConnectionHandler::handle` stays concrete on `tokio::net::TcpStream`, just as in phase 02.2.

Per parent-SPEC §6 signpost 3: this is option α (keep `ConnectionHandler` concrete on `TcpStream`; wrap at envoy-bin). The alternative (β: generalize the trait over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static`) is bigger and reverberates through `Listener::serve`'s `JoinSet` typing for limited 03.1 benefit — defer to phase 04 or 07 if a richer extension registry warrants it.

The `TlsAcceptingHandler` itself lives in `envoy-bin` (D5 below) — keeping it adjacent to the wiring code that constructs the inner `Arc<TcpProxy>`, the cluster manager, and the `Arc<DownstreamTls>` it needs. envoy-tls exports `DownstreamTls` and `accept`; envoy-tcp's generic `TcpProxy::handle` (D4) accepts the post-handshake stream type. envoy-bin glues them.

**No new tests in envoy-listener for 03.1.** The existing 6 tests (phase 02.2) remain unchanged — they exercise the plaintext-only `ConnectionHandler::handle(TcpStream)` path, which is preserved verbatim.

### D4 — `envoy-tcp` generic-stream lift (03.1 portion)

`crates/envoy-tcp/src/lib.rs::TcpProxy::handle` is generalized: instead of taking `&self, downstream: tokio::net::TcpStream`, it becomes generic over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static`. The body is unchanged — `tokio::io::copy` already accepts the trait — but the generic shape is the seam that 03.1 (downstream TLS) and 03.2 (upstream TLS) both exploit.

```rust
impl TcpProxy {
    /// 03.1: generalize over any AsyncRead+AsyncWrite stream so the listener
    /// can pass either TcpStream (plaintext) or TlsStream<TcpStream>
    /// (post-handshake) into it. The proxy logic itself does not care.
    pub async fn handle<S>(&self, downstream: S)
        -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        // existing body verbatim from phase 02.2: pick endpoint, connect upstream,
        // tokio::io::copy bidirectional, propagate errors via try_join!
        // … unchanged …
    }
}
```

The `ConnectionHandler::handle(TcpStream) -> BoxFuture<...>` impl on `TcpProxy` (which `envoy-listener` calls) becomes a thin wrapper that boxes the future of `self.handle::<TcpStream>(downstream)`. The `TlsAcceptingHandler` (envoy-bin, D5) implements `ConnectionHandler::handle(TcpStream)` by first running `DownstreamTls::accept(stream).await?` then calling `self.inner.handle::<TlsStream<TcpStream>>(post_handshake).await` directly (not through the trait — it has the concrete `Arc<TcpProxy>` in hand and can use the inherent generic method). This sidesteps trait-object unsupported-generic-method limitations: `ConnectionHandler` stays object-safe (the boxed-future return type the trait already uses) while the inherent `TcpProxy::handle` method is generic.

The `TcpProxy` struct itself does **not** gain an `Option<Arc<UpstreamTls>>` field in 03.1 — that lands in 03.2's D4 portion alongside the upstream-TLS dial.

**Unit tests appended to `crates/envoy-tcp/src/lib.rs::tests` (4 new tests):**

- `proxies_payload_through_tls_downstream_stream` — set up an in-process `tokio_rustls::TlsAcceptor` + `TlsConnector` pair (rcgen-built server cert with SAN `localhost`; both sides trust the same CA); on the server side, after the handshake, hand the `TlsStream<TcpStream>` to `TcpProxy::handle::<TlsStream<TcpStream>>`; on the client side, write a payload and `read_exact(payload.len())`. Asserts byte-equality. Proves the generic shape works end-to-end at the proxy boundary without involving envoy-listener / envoy-bin / envoy-tls.
- `proxies_payload_with_plaintext_stream_unchanged` — regression: existing phase-02.2 plaintext path still works through the now-generic `handle` (call site type-resolves to `TcpStream`).
- `tls_downstream_proxy_closes_upstream_on_downstream_close` — TLS downstream half-closes (drops TlsStream); upstream sees EOF and closes cleanly. Same property as phase 02.2's `proxies_closes_upstream_on_downstream_close` test, lifted to the TLS path.
- `tls_downstream_proxy_returns_err_on_upstream_connect_refused` — same shape as phase 02.2's connect-refused test, but the downstream is a `TlsStream<TcpStream>`. Asserts the error type is `TcpProxyError::UpstreamConnect` (unchanged from phase 02.2 — TLS termination doesn't introduce new upstream errors when the upstream is plaintext).

The four pre-existing phase-02.2 unit tests in `envoy-tcp::tests` remain unchanged — they exercise the plaintext path, which the new generic shape preserves.

### D5 — `envoy-bin` wiring (03.1 portion)

`crates/envoy-bin/src/main.rs::run` gains downstream-TLS dispatch:

1. **Crypto provider install.** Near the top of `run`, before any TLS-touching code: `let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();`. The `let _ =` discards the `Err` return (second-call no-op per parent-SPEC §6 signpost 4). One-line comment names the second-call-Err contract.

2. **Per-listener filter-chain pre-pass.** For the listener's first (and only — per phase-02.1's `listeners.len() ∈ {0, 1}` cap and 03.1's "one filter chain per listener" implicit cap; multi-filter-chain support lands in 03.2 with `filter_chain_match`) filter chain:
   - If `filter_chain.transport_socket` is `None` → plaintext path; build `TcpProxy::new(cluster, &tcp_proxy_cfg)`; `Arc::new(tcp_proxy) as Arc<dyn ConnectionHandler>`. Unchanged from phase 02.2.
   - If `filter_chain.transport_socket` is `Some(TransportSocket { name: "envoy.transport_sockets.tls", typed_config: TransportSocketTypedConfig::Downstream(ctx) })` → build `Arc<DownstreamTls>` once via `envoy_tls::DownstreamTls::from_context(&ctx)?`; build `TcpProxy::new(cluster, &tcp_proxy_cfg)`; wrap as `Arc::new(TlsAcceptingHandler { tls: Arc::new(downstream_tls), inner: Arc::new(tcp_proxy) }) as Arc<dyn ConnectionHandler>`. Validator already rejected mismatched directions, so the `Upstream(...)` arm is unreachable here.

3. **`TlsAcceptingHandler` adapter** (new module `crates/envoy-bin/src/tls_handler.rs`):

    ```rust
    use std::sync::Arc;
    use envoy_listener::{BoxFuture, ConnectionHandler};
    use envoy_tls::DownstreamTls;
    use envoy_tcp::TcpProxy;

    pub struct TlsAcceptingHandler {
        pub tls: Arc<DownstreamTls>,
        pub inner: Arc<TcpProxy>,
    }

    impl ConnectionHandler for TlsAcceptingHandler {
        fn handle(&self, downstream: tokio::net::TcpStream)
            -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>
        {
            let tls = self.tls.clone();
            let inner = self.inner.clone();
            Box::pin(async move {
                let post_handshake = tls.accept(downstream).await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                inner.handle(post_handshake).await
            })
        }
    }
    ```

  Per parent-SPEC §6 signpost 19: TLS handshake errors are dropped, not propagated; the listener's accept loop logs at `warn!` and stays up. The `?` above propagates the error to the boxed future's `Err` arm, which the listener's accept loop treats per its existing posture (`Some(done) = join_set.join_next() => if let Err(e) = done { tracing::warn!(%e, "connection task failed"); }`).

4. **Validator-already-rejects guarantees consumed.** envoy-bin assumes — and matches — the schema validator's rejections from D2: `UnknownTransportSocketName`, `MismatchedTransportSocketDirection`, `EmptyTlsCertificates`. The `let Some(ts) = filter_chain.transport_socket else { plaintext }; let TransportSocketTypedConfig::Downstream(ctx) = ts.typed_config else { unreachable!("validator rejects upstream on listener") };` shape is acceptable (mirrors phase-02.2's `cluster_mgr.get(&tcp_proxy_cfg.cluster).expect("validator ensured present")` precedent).

`crates/envoy-bin/Cargo.toml` adds: `envoy-tls = { path = "../envoy-tls" }`. Dev-deps add: `tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs"] }`, `rustls = { version = "0.23", default-features = false, features = ["std", "tls12"] }`, `rustls-pki-types = "1"`, `rcgen = "0.13"`, `tempfile = "3"` for the in-process integration test (D5 step 5 below). No new transitive runtime deps outside D-3.2 + ADR-0018 + ADR-0019 foundations.

5. **Integration test** `crates/envoy-bin/tests/tls_downstream.rs` (backstop to fixture 0004, in-process, no Docker): builds an rcgen test PKI in a per-test `tempfile::TempDir`; spawns `envoy-bin` as a subprocess (per phase-02.2's `tcp_proxy.rs` precedent: locate the binary via `env!("CARGO_BIN_EXE_envoy-bin")`) with a config that points at an in-process plaintext echo server on a reserved port; opens a TLS connection via `tokio_rustls::TlsConnector` configured with the test CA in its root store; writes the payload; `read_exact(payload.len())`; asserts byte-equality. Mirrors the shape of phase-02.2's `crates/envoy-bin/tests/tcp_proxy.rs`. The cross-crate `CARGO_BIN_EXE_envoy-bin` path is available because this integration test lives *in the same package* as `envoy-bin`'s binary target. (Cross-package `CARGO_BIN_EXE_*` was the phase-02.2 precedent's blocker — that scenario is reserved for the differential harness's `tcp-echo-server` lookup, not in scope for envoy-bin's own integration tests.)

### D6 — Differential harness extensions (03.1 portion)

`tests/differential/Cargo.toml` adds dev-deps: `rcgen = "0.13"`, `tempfile = "3"`, `tokio-rustls = "0.26"` (default-features=false, features=["aws-lc-rs"]), `rustls = "0.23"` (default-features=false, features=["std","tls12"]), `rustls-pki-types = "1"`, `rustls-pemfile = "2"`. All covered by ADR-0018 (rcgen + tempfile dev-test-harness-only) and ADR-0019 (tokio-rustls + rustls-pemfile under the rustls grant).

- **New module `tests/differential/src/tls.rs`** owning the test PKI:

    ```rust
    use std::path::PathBuf;
    use tempfile::TempDir;

    pub struct TlsTestPki {
        pub ca_pem_path:    PathBuf,
        pub leaf_a_cert:    PathBuf,
        pub leaf_a_key:     PathBuf,
        pub leaf_b_cert:    PathBuf,    // 03.2 use; 03.1 generates but no fixture references
        pub leaf_b_key:     PathBuf,
        pub server_cert:    PathBuf,    // 03.2 use; for tls-echo-server
        pub server_key:     PathBuf,
        _tmpdir: TempDir,                 // dropped last; removes all PEMs
    }

    impl TlsTestPki {
        pub fn generate() -> anyhow::Result<Self>;

        /// Returns the {{LEAF_A_CERT_PATH}} / {{LEAF_A_KEY_PATH}} / {{CA_PATH}}
        /// values for the envoy-side YAML (container-mounted paths under
        /// /etc/envoy-rust-tls/).
        pub fn envoy_side_paths(&self) -> std::collections::HashMap<&'static str, String>;

        /// Returns the same keys for the envoy-rust-side YAML (host tmpdir paths
        /// — the actual on-disk locations).
        pub fn subject_side_paths(&self) -> std::collections::HashMap<&'static str, String>;
    }
    ```

  The CA is a self-signed cert with `BasicConstraint::ca: true`, generated by rcgen at construction time using `KeyPair::generate(&PKCS_ECDSA_P256_SHA256)` (parent-SPEC §6 signpost 6 — ECDSA P-256 keeps the key size small and TLS 1.3 happy; `rcgen` 0.13.x's API for this is stable). Each leaf is signed by that CA with the appropriate Subject Alternative Names: `a.example.com` for `leaf_a`, `b.example.com` for `leaf_b`, `envoy-rust.test` for `server`. All three are generated at construction even though 03.1 only uses `leaf_a` + `ca`; `leaf_b` and `server` PEMs lying unused on disk in the per-fixture tmpdir are cheap and avoid having to extend `TlsTestPki` later. PEMs are written into `_tmpdir` and the `*_path` fields point at the on-disk locations. `Drop` on `_tmpdir` removes the entire directory after the fixture run completes.

- **Driver grammar.** New tagged variant on `Driver` (in `tests/differential/src/lib.rs`):

    ```rust
    pub enum Driver {
        TcpEcho,                                                        // unchanged
        HttpGet { path: String },                                       // unchanged
        TlsTcp { sni: String, expected_cn: Option<String> },            // 03.1 NEW
        // Driver::TlsTcpProbeList { probes: Vec<TlsTcpProbe> }         // 03.2 will add
    }
    ```

- **`drive_tls(addr, payload, sni, root_store, expected_cn) -> anyhow::Result<()>`** — opens a `tokio::net::TcpStream::connect(addr)`, builds a `tokio_rustls::TlsConnector` from a `rustls::ClientConfig` configured with `root_store` and the `aws-lc-rs` provider (via `default_provider().install_default()` once per process — same idempotent-ignore-Err pattern), calls `connector.connect(server_name, stream).await?` (where `server_name = ServerName::try_from(sni.as_str())?.to_owned()`), asserts the handshake succeeded, optionally walks the post-handshake `tls_stream.get_ref().1.peer_certificates()` for the leaf and asserts `expected_cn` is present in either the SAN-DNS list or the CommonName (parent-SPEC §6 signpost 17 — case-insensitive exact match on SAN/CN), writes payload, `read_exact(payload.len())`, asserts byte-equality, runs the ADR-0007 100ms trailing-byte poll (timeout-based; any non-zero read returns `Err`), gracefully shuts down the TLS stream's write side, drops the stream.

- **`render_yaml` per-driver substitution.** New keys for `Driver::TlsTcp`:
  - `{{LEAF_A_CERT_PATH}}`, `{{LEAF_A_KEY_PATH}}`, `{{CA_PATH}}` — substituted from `TlsTestPki::envoy_side_paths()` (container-mounted, e.g., `/etc/envoy-rust-tls/leaf-a-cert.pem`) for `envoy.yaml`, and from `subject_side_paths()` (host tmpdir) for `envoy-rust.yaml`.
  - `{{PORT}}` — unchanged from phase-02.2.
  - `{{BACKEND_PORT}}`, `{{BACKEND_HOST}}` — fixture 0004 reuses these from phase-02.2 (the upstream is plaintext `tcp-echo-server`).

  The 03.2 sub-phase adds `{{LEAF_B_CERT_PATH}}` / `{{LEAF_B_KEY_PATH}}` / `{{SERVER_CERT_PATH}}` / `{{SERVER_KEY_PATH}}` / `{{TLS_BACKEND_PORT}}`.

  Detection is mechanical (string-contains on the template body before substitution) — same pattern phase 02.2 used for `{{BACKEND_PORT}}`. No new `Driver` machinery for the substitution itself.

- **Upstream container mount.** `tests/differential/src/upstream.rs::start` gains a `tls_pki: Option<&TlsTestPki>` parameter. When `Some`, the testcontainers image is configured with `with_copy_to_container(host_path, "/etc/envoy-rust-tls/<filename>.pem")` for each PEM the rendered envoy-side YAML references. Verify against `testcontainers = "0.23.x"`'s API at execution time; if `with_copy_to_container` requires a per-file call (not a per-directory call), iterate over the expected file list. Existing `host_gateway: bool` gating from phase 02.2 is unchanged. Parent-SPEC §6 signpost 7: prefer `with_copy_to_container` over `with_mount` (bind-mounts may struggle on Docker Desktop on macOS).

- **`run_fixture` dispatch.** Detection cascade extended:
  1. If either rendered template references `{{CA_PATH}}` or `{{LEAF_*_PATH}}`, build `TlsTestPki::generate()?` and substitute the per-side keys via `envoy_side_paths()` / `subject_side_paths()`.
  2. Existing `{{BACKEND_PORT}}` gating from phase 02.2 still spawns `TcpProxyBackend` (fixture 0004 uses the plaintext echo backend, so this still fires).
  3. Pass `tls_pki: Option<&TlsTestPki>` into `upstream::start` so the container mount happens before the upstream Envoy container boots.

  The 03.2 sub-phase adds: detect `{{TLS_BACKEND_PORT}}` → spawn a new `TlsEchoBackend`; multi-probe `Driver::TlsTcpProbeList` dispatch.

- **Harness unit tests** in `tests/differential/src/{tls,lib}.rs::tests` (4 new tests, 03.1):
  - `tls_test_pki_generates_valid_chain` — generate PKI, parse all PEMs back via `rustls-pemfile`, assert ≥ 1 cert in each chain, walk the leaf's `Issuer` and assert it matches the CA's `Subject` (the leafs are CA-signed).
  - `tls_test_pki_drop_removes_tmpdir` — generate, capture `ca_pem_path` (or any contained path), drop the `TlsTestPki`, assert the captured path no longer exists.
  - `render_yaml_substitutes_tls_paths_for_envoy_side` — unit test the envoy-side container-mounted path substitution: a template containing `{{CA_PATH}}` is rendered to `/etc/envoy-rust-tls/ca.pem` (or whatever the canonical mount target is per parent-SPEC §6 signpost 12).
  - `render_yaml_substitutes_tls_paths_for_subject_side` — unit test the host path substitution: same template renders to the actual host tmpdir path.

- **Integration test** `tests/differential/tests/tls_downstream.rs` — Docker-gated, same `#[ignore]`-unless-`DOCKER=1` pattern as `admin_ready.rs` (phase 01) and `tcp_proxy.rs` (phase 02.2). Calls `run_fixture("0004-tls-downstream")`.

### D7 — Fixture `tests/fixtures/0004-tls-downstream/`

**Property.** Single-cert downstream TLS termination; plaintext upstream backend (`tcp-echo-server` from phase 02.1).

Files:

- `envoy.yaml` — listener bound on `0.0.0.0:{{PORT}}` with one filter chain carrying:

    ```yaml
    transport_socket:
      name: envoy.transport_sockets.tls
      typed_config:
        "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
        common_tls_context:
          tls_certificates:
            - certificate_chain:
                filename: {{LEAF_A_CERT_PATH}}
              private_key:
                filename: {{LEAF_A_KEY_PATH}}
    ```

  Filters: `envoy.filters.network.tcp_proxy` → cluster `backend`. Cluster `backend` is a STATIC, single-endpoint cluster pointing at `{{BACKEND_HOST}}:{{BACKEND_PORT}}` (templates to `host.docker.internal` on the container side, `127.0.0.1` on the subject side per ADR-0015). Admin block matches fixture 0003's pattern (port 0 → ephemeral; if v1.33.0 rejects 0 on this fixture, fall back to a templated `{{ENVOY_ADMIN_PORT}}` reserved by the harness — same possible workaround phase-02.2 SPEC §D5 anticipated; not anticipated to trip).

- `envoy-rust.yaml` — same shape with the per-side divergences from fixture 0003 (bind `127.0.0.1`, no admin block, backend host `127.0.0.1`, leaf-A paths from `subject_side_paths()`).

- `inputs/payload.bin` — copy of fixture 0001/0003's payload (deterministic non-zero blob, ≥ 1 byte). Reuse byte-identically; the exact bytes don't matter for the 1:1 echo contract.

- `expectations.yaml`:

    ```yaml
    driver:
      kind: tls_tcp
      sni: "a.example.com"
    equivalence:
      response_body: byte_exact
    ```

- `README.md` — one paragraph naming the property; the cert-loading mechanics (rcgen-generated PEMs in a per-fixture tmpdir, mounted into the upstream container at `/etc/envoy-rust-tls/`); the absence of ALPN, multi-cert SNI, upstream TLS, mTLS as out-of-fixture (each tied to a later fixture or phase); ADR references (ADR-0015 cross-container-host reachability, ADR-0016 enable_half_close default, ADR-0017 split decision, ADR-0018 rcgen+tempfile dev-test-harness, ADR-0019 tokio-rustls+rustls-pemfile under rustls grant).

### D8 — Phase-02.2 REVIEW carryforwards (status check; no action in 03.1)

Per parent-SPEC §1's baked-in defaults and §5's carryforward enumeration:

- **M1 (`TcpProxyBackend::Drop` polling loop)** — tracked forward to whichever phase first parallelizes `run_fixture` across worker threads. Phase 03.1 does not parallelize fixtures (each is a single `cargo test` invocation). No action.
- **M2 (`proxies_returns_err_on_upstream_connect_refused` formatted-string assertion)** — awareness-only; no action in 03.1.
- **M3 (`proxies_closes_downstream_on_upstream_close` implicit timing)** — awareness-only; no action.
- **M4 (`Listener::serve` JoinSet type alias)** — phase 03.1 does not introduce a richer filter trait (the `TlsAcceptingHandler` is concrete; `ConnectionHandler` shape unchanged). No action.
- **REVIEW §4 recommendation 1 (`Cluster::name()` accessor)** — default-deferred to phase 06 per parent-SPEC §1 baked-in defaults. envoy-tls's TLS-specific paths in 03.1 do not need cluster-name attribution (the `TlsAcceptingHandler` carries no cluster-name surface; envoy-tcp already carries `cluster_name: String` separately). No action.
- **REVIEW §4 recommendation 2 (ADR numbering provisional)** — explicitly heeded throughout this SPEC. ADR-0018 (rcgen+tempfile) and ADR-0019 (tokio-rustls+rustls-pemfile) lands at 03.1 task 1; ADR-0017 (split decision) already landed at the state-2 plan-writer commit (this commit if read in-tree). Sub-phase 03.2 will continue this discipline.

### D9 — CI workflow

`.github/workflows/ci.yml` changes: **none** in 03.1. The existing `build` job runs `cargo test --workspace`, which picks up the new `envoy-tls` crate automatically. The existing `fuzz` job exercises the extended `parse_bootstrap` corpus via the same `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` invocation (now covering 3 new TLS-shaped seeds).

The Docker-gated integration test `tests/differential/tests/tls_downstream.rs` runs under the same `#[ignore]`-unless-`DOCKER=1` gating pattern as `admin_ready.rs` (phase 01) and `tcp_proxy.rs` (phase 02.2).

### D10 — ADRs to land during execution

Two ADRs land during 03.1 execution, appended to `docs/envoy-rust/DECISIONS.md` in order. Numbering reflects ADR-0017's renumbering scheme: parent-SPEC §7's projected ADR-0017 (`rcgen` + `tempfile` permitted as dev-test-harness-only) becomes ADR-0018; parent-SPEC §7's projected ADR-0018 (`tokio-rustls` + `rustls-pemfile` covered by the rustls foundations grant) becomes ADR-0019. See §7 of this SPEC for the ADR texts.

Both land at task 1 of the 03.1 plan, alongside the workspace-membership commit that adds `crates/envoy-tls` to `[workspace] members` and the `Cargo.toml` for the new crate.

Additional ADRs may be required during execution per D-3.5 if:

- `cargo deny check` flips red on any new transitive license from the rustls / aws-lc-rs / tokio-rustls / rustls-pemfile / rcgen / tempfile chain. Most likely a no-op (the rustls organization's license posture is well-established Apache-2.0 + MIT + ISC, all on the existing allow-list); a non-trivial extension lands its own ADR (likely ADR-0020) at the time it trips.
- TLS protocol-version negotiation drifts between rustls and Envoy v1.33.0 in a way the differential harness catches. The fix is `tls_params { tls_minimum_protocol_version: TLSv1_3, tls_maximum_protocol_version: TLSv1_3 }` on both sides + a rustls `ClientConfig` / `ServerConfig` built with `with_protocol_versions(&[&rustls::version::TLS13])`. Land under a new ADR if it trips. Not anticipated.

---

## 4. Non-goals (deferred to 03.2 or later phases)

Deferred explicitly to sub-phase 03.2:

- **SNI multi-cert resolver (`SniResolver`)** — sub-phase 03.2's D1 portion.
- **`DownstreamTls::from_listener` constructor** — sub-phase 03.2's D1 portion.
- **`UpstreamTls` consumer wiring** in `envoy-tcp::TcpProxy` (`Option<Arc<UpstreamTls>>` field + upstream-TLS dial in `handle`) — sub-phase 03.2's D4 portion.
- **`envoy-bin` cluster-side TLS wiring** (build `Arc<UpstreamTls>` per cluster with `transport_socket: Upstream(...)`; thread into `TcpProxy::new`) — sub-phase 03.2's D5 portion.
- **`envoy-bin` multi-filter-chain dispatch** — sub-phase 03.2's D5 portion (envoy-bin walks all filter chains, builds a single multi-cert `DownstreamTls::from_listener(...)` when the listener has multiple TLS-carrying chains).
- **`FilterChainMatch` schema + `server_names` field + overlap rules + catch-all rules** — sub-phase 03.2's D2 portion.
- **`Driver::TlsTcpProbeList` + `drive_tls_probes` + `TlsEchoBackend`** — sub-phase 03.2's D6 portion.
- **`tls-echo-server` helper crate** — sub-phase 03.2's D7.
- **Fixtures `0005-tls-upstream` and `0006-tls-sni`** — sub-phase 03.2's D8.
- **Phase-03 parent ROADMAP row flips to `done`** — happens at sub-phase 03.2's final commit, not 03.1's.

Deferred to later phases (unchanged from parent-SPEC §4):

- **HTTP-over-TLS** — phase 04 (HCM HTTP/1.1) ships the first ALPN-aware fixture; phase 05 (HTTP/2) makes ALPN load-bearing.
- **mTLS** (`require_client_certificate`, `validation_context.trust_chain_verification`, client cert presentation on upstream) — out of phase 03.
- **Inline cert / key bytes** (`inline_string`, `inline_bytes`, `environment_variable` data sources) — phase 03 supports `filename` only.
- **`tls_params`** (cipher list, min/max TLS version, ECDH curves, signature algorithms) — fixture YAMLs omit; rely on rustls + Envoy defaults.
- **`auto_sni`, `auto_san_validation`, `allow_renegotiation`, `key_rotation`, `session_timeout`, `session_tickets`, `validation_context.match_typed_subject_alt_names`** — out of phase 03.
- **OCSP stapling, signed certificate timestamps, certificate transparency** — out of MVP trunk.
- **xDS-driven SDS** (Secret Discovery Service) — §9 family.
- **`Cluster::name()` accessor** — phase-02.2 REVIEW M1 carryforward; default-deferred to phase 06.
- **`envoy.filters.network.sni_cluster`** (the network filter that routes to a cluster *named* by ClientHello SNI) — §9 network-filters family.
- **Distribution-equivalence on round-robin LB** — parent-brainstorm Q1 still unit-test-only.
- **Listener filters (`listener_filters`)** — out of phase 03.
- **Filter chain framework / extension registry / per-route TLS config** — phase 07.
- **Stats subsystem, access logs, Prometheus** — phase 06.
- **Admin endpoints beyond phase 01's `/ready`** — phase 08.
- **`type: LOGICAL_DNS`, `type: STRICT_DNS`, `type: EDS`** — phase 03 still accepts only `STATIC` per phase-02.1's validator.
- **`lb_policy` variants beyond `ROUND_ROBIN`** — §9 load-balancing family.

---

## 5. Splitting guidance for the planner

Estimated scope:

| Surface | Net LoC (impl + tests) |
|---|---|
| envoy-tls core (cert/key loader + `ServerConfig` builder w/ single-cert resolver + `ClientConfig` builder + crypto-provider install discipline + 10 unit tests) | ~280 + ~150 |
| envoy-config schema (transport_socket envelope + DownstreamTlsContext + UpstreamTlsContext + CommonTlsContext + TlsCertificate + CertificateValidationContext + DataSource + optional `transport_socket` on Cluster + 10 validator tests) | ~200 + ~100 |
| envoy-listener TLS dispatch (no diff) + envoy-tcp generic-stream lift + 4 envoy-tcp tests | ~80 + ~50 |
| envoy-bin wiring (TlsAcceptingHandler + filter-chain dispatch + crypto-provider install) + integration test `tls_downstream.rs` | ~100 + ~80 |
| Harness `tls.rs` (TlsTestPki + envoy/subject path getters) + `Driver::TlsTcp` + `drive_tls` + render_yaml extensions + run_fixture dispatch + 4 unit tests + Docker-gated integration test | ~180 + ~100 |
| Fuzz corpus seeds (3 new TLS-shaped YAML fixtures) | ~80 |
| Fixture 0004 (5 files) | ~80 |
| ADRs 0018 + 0019 (docs) | ~0 |
| **Total** | **~1400 LoC; ~13 tasks** |

Both `BOOTSTRAP_PROMPT.md` §6.1 gates (> ~25 tasks OR > ~1500 LoC) hold comfortably at ~13 tasks / ~1400 LoC. **Do not split 03.1 further**. If the plan as actually written crosses either gate mid-write, invoke `superpowers:systematic-debugging` before attempting a nested split — nested splits of an already-split sub-phase were not anticipated at the parent-phase brainstorm and deserve a fresh root-cause analysis (scope creep vs. planner overdecomposition). Parent-SPEC §5's identical guidance applies here verbatim.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the planner resolves them in-plan rather than mid-execution.

1. **Task ordering for 03.1.** ADR-0018 + ADR-0019 (task 1) + workspace-membership add for `crates/envoy-tls` → envoy-config schema additions (D2) + 10 validator tests + 3 fuzz-corpus seeds → envoy-tls scaffold (D1: cert/key loader, ServerConfig builder, single-cert resolver, ClientConfig builder, crypto-provider install discipline, ~10 unit tests covering both `DownstreamTls` and `UpstreamTls` library API) → envoy-tcp generic-stream lift (D4) + 4 new envoy-tcp tests touching the generic shape → envoy-bin TlsAcceptingHandler module (D5) + filter-chain dispatch + crypto-provider install + integration test `crates/envoy-bin/tests/tls_downstream.rs` → harness `tls.rs` + `Driver::TlsTcp` + `drive_tls` + render_yaml keys + run_fixture dispatch + 4 harness tests → fixture 0004 (5 files) + Docker-gated integration test `tests/differential/tests/tls_downstream.rs` → state-4 phase-done gate.

2. **`envoy-listener::ConnectionHandler` trait shape — keep concrete on `TcpStream` in 03.1.** Per parent-SPEC §6 signpost 3 option α: envoy-listener's trait stays as in phase 02.2 (`fn handle(&self, downstream: TcpStream) -> BoxFuture<...>`). The TLS hop lives in `envoy-bin`'s `TlsAcceptingHandler` adapter. envoy-tcp's `TcpProxy::handle` is generalized over `S: AsyncRead + AsyncWrite + Unpin + Send + 'static` (a smaller, contained edit) so the adapter can pass either `TcpStream` (plaintext) or `TlsStream<TcpStream>` (post-handshake) into it. This minimizes the envoy-listener diff in 03.1 and keeps the adapter's TLS knowledge isolated to envoy-bin's wiring. If a future phase (04 or 07) builds a richer extension registry that needs trait-level genericity, that phase lands a new trait shape under its own ADR.

3. **Generic `TcpProxy::handle` is an inherent method, not a trait method.** Trait methods can't be generic (in object-safe traits — `ConnectionHandler` is `dyn`-object-safe by virtue of returning a `BoxFuture`). The plan-writer keeps `TcpProxy::handle::<S>` as an inherent generic method and writes the `ConnectionHandler::handle` trait impl as a thin wrapper that boxes the future of `self.handle::<TcpStream>(downstream)`. The `TlsAcceptingHandler` calls `self.inner.handle::<TlsStream<TcpStream>>(post_handshake)` directly on its `Arc<TcpProxy>`, sidestepping the trait. Phase-02.2's existing `ConnectionHandler` impl on `TcpProxy` is preserved; the new generic inherent method is the additive change.

4. **Crypto provider initialization is per-process, idempotent, and `install_default()` returns `Err` on second call.** `rustls::crypto::aws_lc_rs::default_provider().install_default()` returns `Err(rustls::crypto::CryptoProvider)` if a provider has already been installed in the process. envoy-bin's `main` calls it once early; the harness's `run_fixture` (via `drive_tls`'s lazy initialization) calls it once per process (which is once per `cargo test` invocation, since each test process is fresh). **Always discard the `Err` return** — it indicates the second-or-later call, which is the desired no-op. Document the call site with a one-line comment naming the second-call-Err contract.

5. **`tokio-rustls` version pinning.** Plan-writer verifies `tokio-rustls` 0.26.x is the latest stable line at execution time and pins accordingly. The `aws-lc-rs` feature is selected via `default-features = false, features = ["aws-lc-rs"]`. If 0.26's API differs materially from this SPEC's expectation, no new ADR is needed unless a fresh exemption surfaces (in which case ADR-0019 is amended at landing time, not edited post-hoc).

6. **rcgen API version.** rcgen 0.13.x is the latest at planning time. The `CertificateParams` builder shape is stable across 0.12–0.13; `KeyPair::generate(&PKCS_ECDSA_P256_SHA256)` is the canonical key generator (ECDSA P-256 keeps the key size small and TLS 1.3 happy). If rcgen 0.13's API differs, the plan-writer adjusts and notes in PROGRESS.md. Not an ADR surface.

7. **Container-side cert mount via `testcontainers`.** v0.23 exposes `with_copy_to_container` (preferred — copies a host file/dir into the container at startup) and `with_mount` (bind-mounts; may be problematic on Docker Desktop on macOS due to the gRPC FUSE layer). Plan-writer picks `with_copy_to_container` first. The container path is `/etc/envoy-rust-tls/`; fixture 0004's envoy.yaml references paths under that directory.

8. **`#![forbid(unsafe_code)]` is mandatory** at every new crate's `lib.rs` / `main.rs`: `crates/envoy-tls/src/lib.rs`. aws-lc-rs's internal unsafe is shielded behind its crate's allowlist; no envoy-rust-owned code carries unsafe.

9. **Workspace membership.** Root `Cargo.toml` `[workspace] members` grows by `crates/envoy-tls` (03.1). The `tests/helpers/tls-echo-server` add lands in 03.2.

10. **Half-close posture (ADR-0016) carries forward unchanged.** `enable_half_close: false` is Envoy's v1.33.0 default for `tcp_proxy`. Fixture 0004 does not include the key; envoy-rust's `TcpProxy::handle` (now generic over the stream type) preserves the `tokio::select!`-over-two-`tokio::io::copy`-futures shape from phase 02.2. TLS does not propagate half-close any differently than plaintext for the byte-exact contract the harness asserts.

11. **`expected_cn` matching policy in `drive_tls`.** Walk both `subject_alt_name` (DNS entries) and CommonName; case-insensitive exact match. Wildcard SAN values (`*.example.com`) are not generated by `TlsTestPki` in phase 03, so no wildcard-match policy is needed. If a future phase needs wildcards on the harness side, that phase extends `drive_tls` and lands an ADR.

12. **TLS handshake errors in `Listener::serve` and `TcpProxy::handle` are dropped, not propagated.** Per the phase-02.2 posture (per-connection errors → `tracing::warn!` and drop the connection; listener stays up), TLS handshake failures in `TlsAcceptingHandler::handle` log at `warn!` and return `Err(_)` boxed into the `ConnectionHandler` trait return type — the listener's accept loop's `if let Err(e) = done { tracing::warn!(...); }` posture drops the connection and the listener stays up. The integration tests do **not** assert on log content; they only assert end-state successful handshakes complete byte round-trips.

13. **rustls-pki-types `ServerName::try_from` semantics.** `ServerName::try_from("envoy-rust.test")` returns `Ok(ServerName::DnsName(...))`. `ServerName::try_from("127.0.0.1")` returns `Ok(ServerName::IpAddress(...))` — but Envoy's `UpstreamTlsContext.sni` is documented to be a DNS name only (Envoy rejects IPs in `sni`). To match Envoy, `UpstreamTls::from_context` only accepts `ServerName::DnsName`; an IP literal in `sni` returns `TlsError::InvalidServerName`. The validator + the unit-test enumeration codify this.

14. **ALPN absence.** Fixture YAMLs **do not** include `alpn_protocols`. envoy-tls's `ServerConfig` and `ClientConfig` builders **do not** call `with_alpn_protocols`. Phase 04 (HCM HTTP/1.1) is the first phase to add ALPN; phase 05 (HTTP/2) makes it load-bearing. Review should flag any phase-03.1 PR that "defensively" adds an ALPN list.

15. **`UpstreamTls` library API ships in 03.1 even though no fixture consumes it.** Library code with its own unit tests sits cleanly in 03.1; consumer wiring (envoy-tcp `Option<Arc<UpstreamTls>>` field; envoy-bin per-cluster construction) lands in 03.2 alongside fixture 0005. Splitting consumer-from-library across the sub-phase boundary is intentional: 03.1's envoy-tls is reviewable as a complete primitive; 03.2 just uses it. Mirrors the phase-02 pattern where `envoy-cluster::ClusterManager` shipped in 02.1 with no `envoy-bin` consumer (it was wired up in 02.2).

16. **Fuzz corpus seeds are static YAML files, not generated.** The 3 new seeds in `crates/envoy-config/fuzz/corpus/parse_bootstrap/` are committed verbatim; the fuzzer mutates them but never opens the cert/key paths they reference (the parse_bootstrap target only exercises serde, not the loader). Plan-writer picks plausible-but-irrelevant filename strings (`/tmp/cert.pem` etc.).

17. **In-process integration test PKI lifetime.** `crates/envoy-bin/tests/tls_downstream.rs` builds its PKI in a per-test `tempfile::TempDir`. The TempDir must outlive the spawned `envoy-bin` subprocess; `_tmpdir` is held in scope by the test fn until the subprocess is killed (mirrors the harness's `TlsTestPki._tmpdir` pattern in §D6). The test does not set `tempfile::TempDir::keep` — drop on test exit cleans up.

18. **rustls `RootCertStore::add` returns a `Result`.** Empty CA PEMs or malformed CA PEMs return errors at `add` time; `UpstreamTls::from_context` collects these into `TlsError::CaParse` (no leaf cert found at the path) or `TlsError::FileRead` (I/O), depending on which layer fails. The unit test `loads_upstream_client_config` exercises the happy path; an analogous "rejects empty CA PEM" test sits in the 03.2 plan if it surfaces a needed coverage gap.

19. **`anyhow` boundary at envoy-bin's integration tests.** `crates/envoy-bin/tests/tls_downstream.rs` is in the binary crate's package and may use `anyhow` (D-3.2 permits `anyhow` only in `envoy-bin`). The `tests/differential/` crate (workspace-separate) cannot use `anyhow` — it's a library crate per workspace membership rules. The harness's `drive_tls` returns `anyhow::Result<()>` because `tests/differential` is dev-test-harness only and `anyhow` was already part of its established posture from phase 00 onward.

20. **`tls_params` absence in fixture 0004.** Fixture YAMLs do not include `tls_params { tls_minimum_protocol_version: ..., tls_maximum_protocol_version: ... }`. Both rustls + aws-lc-rs and Envoy v1.33.0 default to TLS 1.2 + TLS 1.3 with rustls-default cipher list. If execution surfaces version-negotiation drift between the two proxies, an ADR (likely ADR-0020) pins the floor — not anticipated.

---

## 7. ADRs expected from this sub-phase

Two ADRs land during 03.1 execution, in `docs/envoy-rust/DECISIONS.md`, in order. Numbering reflects ADR-0017's renumbering scheme: parent-SPEC §7's projected ADR-0017 (rcgen + tempfile dev-test-harness-only) becomes ADR-0018; parent-SPEC §7's projected ADR-0018 (tokio-rustls + rustls-pemfile under the rustls grant) becomes ADR-0019.

Both land at task 1 of the 03.1 plan, alongside the workspace-membership commit that adds `crates/envoy-tls` to `[workspace] members`.

### ADR-0018 — `rcgen` and `tempfile` permitted as dev-test-harness-only foundations

- Context: Phase 03 is the first phase to need test certificates. TLS test infrastructure recurs across phases 03–08+ (HTTP/1.1 over TLS, H2 over TLS, mTLS, etc.). Static in-tree PEMs were considered and rejected per the parent-phase brainstorm Q2 decision (poor refresh ergonomics, expiry concerns, multi-leaf cert generation gets unwieldy). `rcgen` is the maintained Rust-native cert generator; `tempfile` is the canonical per-test-run tmpdir manager. Neither is on the D-3.2 permitted-foundations list at phase-02.2 close.
- Options considered: (i) static in-tree PEMs (rejected, parent-brainstorm Q2); (ii) `rcgen` + `tempfile` on the permitted list as **dev-test-harness-only** (decision); (iii) script-generated PEMs committed to the repo (rejected, parent-brainstorm Q2: worst-of-both-worlds — refresh friction *and* in-tree drift).
- Decision: add `rcgen = "0.13"` and `tempfile = "3"` to the permitted-foundations list with the **dev-test-harness-only** annotation. Mirrors ADR-0009's posture for `cargo-fuzz` + `libfuzzer-sys`. Never a transitive of `envoy-bin` or any non-test workspace crate. Restricted to: `tests/differential/` dev-deps; `tests/helpers/tls-echo-server/` dev-deps (lands in 03.2); `crates/envoy-tls/` dev-deps (for unit-test PKI); `crates/envoy-bin/` dev-deps (for the in-process integration test).
- Rationale: one-time foundations grant beats per-phase ADR churn; rcgen is the Rust-ecosystem default; tempfile is ubiquitous test-infra. Test-only restriction preserves D-3.2's spirit for runtime code.
- Consequences: future TLS-cert-using phases (04 HCM-over-TLS, 05 H2-over-TLS, mTLS phases, etc.) reuse this decision without per-phase ADRs. `cargo deny check` may flag the rcgen license (Apache-2.0 OR MIT — both on the deny.toml allow-list) or its transitive deps; if so, the deny.toml is updated alongside ADR-0018's landing. If a future phase needs cert generation in *runtime* code (e.g., hot-restart cert rotation), that phase lands a new ADR superseding the dev-test-harness-only restriction.

### ADR-0019 — `tokio-rustls` and `rustls-pemfile` covered by the rustls foundations grant

- Context: D-3.2 lists `rustls`, `webpki`, `rustls-pki-types`, and "`aws-lc-rs` permitted as the crypto provider," but does not name `tokio-rustls` or `rustls-pemfile` explicitly. Both are mechanically necessary to use rustls inside a tokio runtime / load PEMs from disk; both ship from the rustls org.
- Options considered: (i) treat both as covered implicitly by the rustls grant — risks ambiguity for downstream phases; (ii) land an ADR formalizing the extension (decision); (iii) hand-roll the async glue and PEM parser — reinvents wheels D-3.2 explicitly tells us not to.
- Decision: extend D-3.2's "rustls + aws-lc-rs permitted as the crypto provider" grant to cover `tokio-rustls = "0.26"` and `rustls-pemfile = "2"`. Both are runtime-permitted (not dev-only); rcgen + tempfile from ADR-0018 stay dev-only.
- Rationale: removes ambiguity for downstream phases. Both crates are first-party in the rustls ecosystem; treating them as part of the same foundation is the cheapest, most honest formalization.
- Consequences: envoy-tls's `Cargo.toml` lists both as direct deps. `tls-echo-server`'s `Cargo.toml` (lands in 03.2) lists both. Neither is allowed in `envoy-listener` or `envoy-cluster` — those crates remain rustls-free per D1's "envoy-tls is the only crate with rustls deps" architectural rule.

---

## 8. Artifacts this sub-phase produces

Created during execution (relative to repo root):

- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PLAN.md`
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md`
- `docs/envoy-rust/phases/03.1-tls-foundation-downstream/REVIEW.md`
- `crates/envoy-tls/Cargo.toml`
- `crates/envoy-tls/src/lib.rs`
- `crates/envoy-bin/src/tls_handler.rs`
- `crates/envoy-bin/tests/tls_downstream.rs`
- `tests/differential/src/tls.rs`
- `tests/differential/tests/tls_downstream.rs`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml`
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml`
- `tests/fixtures/0004-tls-downstream/envoy.yaml`
- `tests/fixtures/0004-tls-downstream/envoy-rust.yaml`
- `tests/fixtures/0004-tls-downstream/inputs/payload.bin`
- `tests/fixtures/0004-tls-downstream/expectations.yaml`
- `tests/fixtures/0004-tls-downstream/README.md`

Amended during execution:

- Root `Cargo.toml` — add `crates/envoy-tls` to `[workspace] members`. (`tests/helpers/tls-echo-server` lands in 03.2.)
- `crates/envoy-bin/Cargo.toml` — add `envoy-tls` path-dep; add dev-deps on `tokio-rustls`, `rustls`, `rustls-pki-types`, `rcgen`, `tempfile` for the in-process integration test.
- `crates/envoy-bin/src/main.rs` — install aws-lc-rs default crypto provider; per-filter-chain pre-pass that constructs `Arc<DownstreamTls>` when `transport_socket` is present and wraps the inner `TcpProxy` in a `TlsAcceptingHandler`.
- `crates/envoy-config/src/bootstrap.rs` — add `TransportSocket` envelope, `TransportSocketTypedConfig`, `DownstreamTlsContext`, `UpstreamTlsContext`, `CommonTlsContext`, `TlsCertificate`, `CertificateValidationContext`, `DataSource`, optional `transport_socket` field on `Cluster` and on `FilterChain`; extend `validate` with `UnknownTransportSocketName`, `MismatchedTransportSocketDirection`, `EmptyTlsCertificates`, `MissingValidationContext`, `EmptyUpstreamSni` `ConfigError` variants; 10 new validator unit tests.
- `crates/envoy-config/src/lib.rs` — re-export new public types (`TransportSocket`, `TransportSocketTypedConfig`, `DownstreamTlsContext`, `UpstreamTlsContext`, `CommonTlsContext`, `TlsCertificate`, `CertificateValidationContext`, `DataSource`); extend `ConfigError` enum.
- `crates/envoy-tcp/src/lib.rs` — generalize `TcpProxy::handle` to `<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(&self, downstream: S)`; add 4 new unit tests that touch the generic shape via `TlsStream<TcpStream>`.
- `crates/envoy-tcp/Cargo.toml` — add dev-deps on `tokio-rustls`, `rustls`, `rcgen`, `tempfile` for the new TLS-flavored unit tests.
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — 3 new TLS-shaped seeds (listed under "Created" above).
- `tests/differential/Cargo.toml` — add `rcgen`, `tempfile`, `tokio-rustls`, `rustls`, `rustls-pki-types`, `rustls-pemfile` as dev-deps.
- `tests/differential/src/lib.rs` — add `Driver::TlsTcp` variant; add `drive_tls` helper; extend `render_yaml` substitution-key map; extend `run_fixture` dispatch to detect TLS-path templates and build `TlsTestPki`; thread `tls_pki` into `upstream::start`.
- `tests/differential/src/upstream.rs` — extend `start` signature with `tls_pki: Option<&TlsTestPki>` and add `with_copy_to_container` calls per PEM when `Some`.
- `docs/envoy-rust/DECISIONS.md` — ADR-0018 + ADR-0019 appended.
- `docs/envoy-rust/ROADMAP.md` — row 03.1 `status` → `done` in the final commit.
- `docs/envoy-rust/STATE.md` — active → `03.2-tls-upstream-sni`, next-skill → `superpowers:writing-plans`, state → 2 (SPEC.md exists, PLAN.md does not; 03.2's SPEC landed alongside this one during the ADR-0017 split session).
- `deny.toml` — only if `cargo deny check` flags new licenses or transitive surfaces from the rustls / aws-lc-rs / tokio-rustls / rustls-pemfile / rcgen / tempfile chain. Most likely a no-op; a non-trivial extension lands its own ADR.

Not touched in 03.1 (belong to 03.2 or are frozen):

- `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `a3f3474`.
- `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md`, `phases/02.1-config-cluster/`, `phases/02.2-listener-tcp-proxy/` — closed in phase 02.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/` — unedited; their fixtures must remain green at phase-03.1 state-4 gate.
- `crates/envoy-cluster/src/lib.rs` — untouched in 03.1 (cluster-side TLS plumbing belongs to 03.2's D4 portion; the parent-SPEC D4 alternative routes through envoy-bin orchestration anyway, leaving envoy-cluster TLS-free).
- `tests/helpers/tcp-echo-server/` — finalized in phase 02.1; fixture 0004's plaintext upstream backend is exactly this helper. Unchanged in 03.1.
- `tests/helpers/tls-echo-server/` — does not exist yet; lands in 03.2.
- `BEHAVIOR_CONTRACT.md` — no edits in phase 03 per parent-SPEC §1's baked-in defaults.

---

## 9. Final commit message format (for state 6 of the 03.1 lifecycle)

```
phase 03.1: envoy-tls foundation + downstream TLS termination + fixture 0004 [ADR-0018, ADR-0019]

New library crate envoy-tls owns rustls server/client config construction:
DownstreamTls::from_context loads cert+key PEMs via rustls-pemfile and builds
a ServerConfig with a single-cert ResolvesServerCert; UpstreamTls library API
ships its from_context + connect with consumer wiring deferred to 03.2.
envoy-config grows the transport_socket envelope (DownstreamTlsContext +
UpstreamTlsContext + CommonTlsContext + TlsCertificate +
CertificateValidationContext + DataSource) with 10 new validator tests and
3 fuzz-corpus seeds. envoy-tcp::TcpProxy::handle generalizes over
AsyncRead+AsyncWrite+Unpin+Send+'static; envoy-bin's new TlsAcceptingHandler
wraps an Arc<TcpProxy> with a downstream-TLS handshake hop and dispatches
into the generic handle. New differential harness PKI (rcgen-driven) and
Driver::TlsTcp; fixture 0004-tls-downstream lands green end-to-end.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (unchanged);
  tests/fixtures/0004-tls-downstream green (single-cert downstream TLS,
  plaintext upstream).
Conformance: none.
```
