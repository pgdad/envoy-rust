//! Docker-gated differential test for fixture `0090-h2-response-trailers`.
//!
//! Phase 111: HTTP/2 response TRAILER forwarding, upstream -> downstream — the
//! first of the gRPC family's two blocking prerequisites (ADR-0048, re-affirmed
//! by ADR-0177; scoped by ADR-0181, planned by ADR-0182).
//!
//! The trailer-emitting backend (`http2-echo-server --trailers`) answers with
//! one ANNOUNCED trailer (`x-trail-a`, named in the RFC 7230 §4.4 `trailer:`
//! response header) and one UNANNOUNCED trailer (`x-trail-b`). Upstream Envoy
//! forwards BOTH, so this fixture witnesses "forward the block", not "forward
//! what was announced".
//!
//! First exercise of the `Response trailers` row of the equivalence matrix in
//! `docs/envoy-rust/BEHAVIOR_CONTRACT.md`, unwitnessed since phase 00 seeded
//! it. Neither `x-trail-a` nor `x-trail-b` is on the harness's 3-entry
//! `HEADER_ALLOW_LIST`, so `diff_headers` compares both VALUE-EXACT across the
//! two proxies — that comparison is this fixture's entire witness.
//!
//! The fixture probes exactly ONE cell. Five measured cells are deliberately
//! EXCLUDED, each on a measurement recorded in `PLAN.md` §1: a connection-
//! specific trailer name (CF-111-5, envoy-rust 503s today, inside the `h2`
//! codec and pre-existing), a pseudo-header in the block (CF-111-6, a
//! divergence this phase would CREATE), duplicate trailer names (CF-111-8,
//! unassertable under the set-based `diff_headers`), trailer wire order
//! (CF-111-9, doubly invisible), and any stat assertion (CF-111-7, upstream's
//! `http2.trailers` stats exist and stay 0). A fixture that goes RED for the
//! wrong reason is worse than no fixture.
//!
//! The no-trailers regression witness is fixture `0010-http2-router-upstream`,
//! whose topology this one copies verbatim and which stays untouched.

use std::path::PathBuf;

#[tokio::test]
async fn h2_response_trailers() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0090-h2-response-trailers");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
