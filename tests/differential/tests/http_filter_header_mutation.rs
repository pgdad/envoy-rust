//! Phase 07.2 differential acceptance test: drive a GET / through an HCM whose
//! `http_filters` chain is `[HeaderMutation, Router]`, proxying to a host-side
//! `http1-echo-server` backend. Both proxies must produce identical (status,
//! body, header-set-modulo-allow-list). The HeaderMutation `request_mutations`
//! stamp (`x-filter-stamp: phase-07`) is echoed back in the body by the
//! backend (decode-side proof); the `response_mutations` stamp
//! (`x-filter-response-stamp: phase-07`) lands on the client-visible response
//! headers (encode-side proof). Docker-gated.

use std::path::PathBuf;

#[tokio::test]
async fn http_filter_header_mutation_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0013-http-filter-header-mutation");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
