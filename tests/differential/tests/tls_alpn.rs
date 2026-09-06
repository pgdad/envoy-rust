//! Phase 112.2 differential acceptance tests: TLS ALPN negotiation across a
//! `tcp_proxy` listener that terminates downstream TLS, against upstream
//! Envoy v1.33.0 and envoy-rust. Docker-gated.
//!
//! `0091-tls-alpn` carries cells 1-4 of the phase-112 cell table as four
//! probes against one server list `["h2", "http/1.1"]`; `0092` carries cell 5,
//! the server-preference witness, with the list reversed. Cell 6 (the
//! no-ALPN control) rides on the pre-existing `0004-tls-downstream`.

use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures")
        .join(name)
}

#[tokio::test]
async fn tls_alpn_fixture() {
    differential::run_fixture(&fixture("0091-tls-alpn"))
        .await
        .expect("fixture passes");
}
