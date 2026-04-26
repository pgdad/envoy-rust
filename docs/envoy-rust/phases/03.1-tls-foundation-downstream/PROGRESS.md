# Phase 03.1 Progress

## Task 1 — ADRs 0018 + 0019 (2026-04-26)

- Commit: f93a062
- Change: appended ADR-0018 (rcgen + tempfile permitted as dev-test-harness-only foundations) and ADR-0019 (tokio-rustls + rustls-pemfile covered by the rustls foundations grant) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 19 (ADR-0001 through ADR-0019).

## Task 2 — envoy-config — TransportSocket envelope + TLS context types (2026-04-26)

- Commit: db52844
- Change: Added 8 new types to `bootstrap.rs` (TransportSocket, TransportSocketTypedConfig, DownstreamTlsContext, UpstreamTlsContext, CommonTlsContext, TlsCertificate, CertificateValidationContext, DataSource — all with deny_unknown_fields); added `transport_socket: Option<TransportSocket>` to FilterChain and Cluster; extended `pub use` re-exports and added `TLS_TRANSPORT_SOCKET` constant in `lib.rs`; updated `ConfigError::Yaml` Display to include source message; fixed envoy-cluster test fixtures for new Cluster field. Landed 5 parse-shape tests (2 happy-path, 3 deny_unknown_fields regressions).
- Verification: `cargo test -p envoy-config bootstrap::tests` → 43 passed (38 pre-existing + 5 new); `cargo test -p envoy-cluster` → 8 passed; `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check` all exit 0. Cargo.lock unchanged.
