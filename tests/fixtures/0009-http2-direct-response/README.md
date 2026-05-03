# Fixture 0009 — HTTP/2 cleartext (H2C) direct_response

Phase 05.2 differential fixture. The first H2C surface in the project's history.

## Surface

- Listener bound on a single TCP port (`{{PORT}}` substituted by the harness).
- HCM filter chain with `codec_type: HTTP2`.
- Single virtual host `domains: ["*"]`.
- Single route `prefix: "/"` with action `direct_response { status: 200, body: { inline_string: "ok\n" } }`.
- No upstream cluster (`clusters: []`).

## What this fixture exercises

- HCM-on-H2 dispatch path in `envoy-bin/src/main.rs:207` (Task 10).
- `envoy-http2::HCM::handle` per-connection driver (Task 9).
- H2 prior-knowledge handshake via `h2::server::handshake`.
- Per-stream `tokio::spawn` task running through the existing 04.x route-walk
  + `BuildOutcome::Synth` arm (the `direct_response` happy path).
- `:authority` → `Host:` synthesis in `request.rs::http_to_envoy_request` (the
  driver's `host: envoy-rust.test` becomes the H2 `:authority` pseudo-header
  which becomes the synthesized `Host:` row that the route-walk reads).

## Cross-references

- Phase 05.2 SPEC §3 D6: `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md`.
- Architectural rules: parent-05 SPEC §3 cross-sub-phase rules 1–7.
- Sibling H1 fixture (same shape, different codec): `tests/fixtures/0007-http1-direct-response/`.
