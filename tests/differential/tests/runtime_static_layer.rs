//! Docker-gated differential test for fixture 0087-runtime-static-layer.
//! Sub-phase 108.2 D6 — the runtime family's first differential: the admin
//! `GET /runtime` snapshot (entries + layers) and the four non-zero
//! `runtime.*` stat witnesses, asserted bilaterally against upstream Envoy
//! v1.33.0 over `tests/fixtures/0087-runtime-static-layer/`.

use std::path::PathBuf;

#[tokio::test]
async fn runtime_static_layer() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0087-runtime-static-layer");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
