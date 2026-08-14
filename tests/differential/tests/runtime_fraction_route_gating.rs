//! Sub-phase 109.2 differential acceptance test: route `match.runtime_fraction`
//! gating over the 108-landed runtime snapshot store.
//!
//! Ten HTTP/1.1 probes at a backend-free, CLUSTER-FREE HCM listener
//! (`clusters: []`, `direct_response` routes only). Nine routes carry a
//! `match.runtime_fraction`; a two-static-layer `layered_runtime` block decides
//! their gates. Each probe has a DISTINCT `path:` (the attribution rule) and
//! each route a DISTINCT body, so the response body IS the gate's verdict —
//! a wrongly-passing gate answers `P<N>-GATED` where `CATCH` is expected.
//!
//! This is the FIRST differential witness of `runtime_fraction` in the corpus,
//! and the first fixture combining `Http1ProbeList` traffic with
//! `layered_runtime`. The ten cells it pins are the deterministic subset of the
//! 23-cell matrix MEASURED against `envoyproxy/envoy:v1.33.0` (parent
//! `109/SPEC.md` §1.1 + `109.1/SPEC.md` §1.2): absent key honours
//! `default_value` in BOTH directions; a consulted key OVERRIDES the default;
//! an integer value is the numerator over HUNDRED regardless of the default's
//! denominator (`/p-million`, a 0/MILLION default gated by the value `100`);
//! `>= 100` always passes; quoted numeric strings parse like integers; an
//! unparseable value falls back to `default_value`; and a two-layer key honours
//! last-layer-wins `final_value`. The per-request-nondeterministic cells
//! (`0 < v < 100`) are boot-fatal here under CF-109-1 and are witnessed
//! in-process by 109.1, never in a fixture.
//!
//! `envoy.yaml` and `envoy-rust.yaml` are BYTE-IDENTICAL — the second such pair
//! in the corpus. Docker-gated and backend-free (no `{{BACKEND_IP}}` marker, so
//! no backend container spawns), therefore fully verifiable on a developer host.

use std::path::PathBuf;

#[tokio::test]
async fn runtime_fraction_route_gating() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0088-runtime-fraction-route-gating");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
