# Fixture 0004-tls-downstream

This fixture drives an arbitrary byte payload through a listener configured with
`envoy.filters.network.tcp_proxy` whose filter chain terminates downstream TLS
via `transport_socket: envoy.transport_sockets.tls (DownstreamTlsContext)`. The
single configured cert is a leaf with SAN `a.example.com` (rcgen-generated at
fixture-run time per ADR-0018, signed by a harness-generated CA, written into a
per-fixture `TempDir`). Both upstream Envoy and envoy-rust dial the same
plaintext upstream backend (the in-tree `tcp-echo-server` helper from phase
02.1, running as a host subprocess).

The harness's `Driver::TlsTcp { sni: "a.example.com" }` opens a TLS connection
to each proxy with the test CA in its `RootCertStore`, completes the handshake,
writes the payload, reads `payload.len()` bytes, asserts byte-equality, and
runs the ADR-0007 100ms trailing-byte poll. The `drive_tls` helper inherits
`drive_tcp`'s read-exact + trailing-poll discipline (ADR-0006, ADR-0007).

Cross-container host reachability for the plaintext upstream is covered by
ADR-0015; container-to-host PEM availability is provided by testcontainers'
`with_copy_to_container` (per parent-SPEC §6 signpost 7), which copies each
PEM into the upstream Envoy container under `/etc/envoy-rust-tls/` at
container-start time. Half-close posture follows ADR-0016 (`enable_half_close`
absent from both sides).

What is *out* of this fixture (each pinned to a later fixture or phase):

- ALPN — phase 04 first uses ALPN; phase 05 makes it load-bearing.
- Multi-cert SNI cert selection on the downstream — fixture 0006 (sub-phase
  03.2).
- Upstream TLS origination — fixture 0005 (sub-phase 03.2).
- mTLS / `require_client_certificate` — out of phase 03.
- Inline cert/key data sources — phase 03 supports `filename` only.
- `tls_params` (cipher list, min/max version) — defer to a future ADR if
  rustls-vs-Envoy version negotiation drifts (SPEC §6 signpost 20).

ADR references: ADR-0015 (cross-container host reachability), ADR-0016
(`enable_half_close: false` default), ADR-0017 (phase-03 split), ADR-0018
(rcgen + tempfile dev-test-harness-only), ADR-0019 (tokio-rustls +
rustls-pemfile under the rustls grant).
