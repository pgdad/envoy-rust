#![forbid(unsafe_code)]

//! Phase 03.1 TLS surface for envoy-rust. Owns rustls server/client config
//! construction, the cert/key PEM loader, and the `TlsError` typed-error enum.
//! Public surface is populated by Tasks 6 (`DownstreamTls`) and 7 (`UpstreamTls`)
//! of `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PLAN.md`.
//!
//! D-3.2 + ADR-0018 + ADR-0019: this is the only crate in the workspace that
//! depends on rustls / tokio-rustls / rustls-pki-types / rustls-pemfile /
//! aws-lc-rs. envoy-listener and envoy-cluster stay rustls-free.
