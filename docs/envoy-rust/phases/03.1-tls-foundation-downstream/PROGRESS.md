# Phase 03.1 Progress

## Task 1 — ADRs 0018 + 0019 (2026-04-26)

- Commit: f93a062
- Change: appended ADR-0018 (rcgen + tempfile permitted as dev-test-harness-only foundations) and ADR-0019 (tokio-rustls + rustls-pemfile covered by the rustls foundations grant) to DECISIONS.md.
- Verification: `grep -c '^## ADR-00' docs/envoy-rust/DECISIONS.md` → 19 (ADR-0001 through ADR-0019).

## Task 2 — envoy-config — TransportSocket envelope + TLS context types (2026-04-26)

- Commit: db52844
- Change: Added 8 new types to `bootstrap.rs` (TransportSocket, TransportSocketTypedConfig, DownstreamTlsContext, UpstreamTlsContext, CommonTlsContext, TlsCertificate, CertificateValidationContext, DataSource — all with deny_unknown_fields); added `transport_socket: Option<TransportSocket>` to FilterChain and Cluster; extended `pub use` re-exports and added `TLS_TRANSPORT_SOCKET` constant in `lib.rs`; updated `ConfigError::Yaml` Display to include source message; fixed envoy-cluster test fixtures for new Cluster field. Landed 5 parse-shape tests (2 happy-path, 3 deny_unknown_fields regressions).
- Verification: `cargo test -p envoy-config bootstrap::tests` → 43 passed (38 pre-existing + 5 new); `cargo test -p envoy-cluster` → 8 passed; `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check` all exit 0. Cargo.lock unchanged.

## Task 3 — envoy-config — TLS validator extensions + 5 new ConfigError variants (2026-04-26)

- Commit: 9202f31
- Change: Extended `ConfigError` in `lib.rs` with 5 new variants (UnknownTransportSocketName, MismatchedTransportSocketDirection, EmptyTlsCertificates, MissingValidationContext, EmptyUpstreamSni). Extended `validate(...)` in `bootstrap.rs` with per-cluster and per-listener TLS arms that enforce direction, certificate cardinality, validation_context presence, and SNI non-emptiness. Landed 7 new tests (rejects_unknown_transport_socket_name, rejects_downstream_tls_context_on_cluster, rejects_upstream_tls_context_on_listener, rejects_downstream_with_empty_tls_certificates, rejects_upstream_with_tls_certificates, rejects_upstream_without_validation_context, rejects_upstream_with_empty_sni).
- Verification: `cargo test -p envoy-config --lib` → 50 passed (38 base + 5 Task 2 + 7 Task 3). Workspace gate: `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace --lib --bins` all exit 0. Per-crate counts: differential 31, envoy-bin 19, tcp-echo-server 8, envoy-config 50, envoy-listener 6, envoy-tcp 4, envoy-cluster 8 (total 126). Cargo.lock unchanged.
- Note on SPEC §D2 test-count drift: SPEC §D2 estimated 10 TLS-related tests across Tasks 2+3. Actual landed count is 12 (5 in Task 2 + 7 in Task 3). The two extras are rejects_upstream_without_validation_context and rejects_upstream_with_empty_sni — both SPEC-named validator tests that were included in the PLAN's Task 3 expansion (PLAN line 637 explains this as "reviewer-cost rounding"). Neither gate (LoC, task count) is materially affected.
