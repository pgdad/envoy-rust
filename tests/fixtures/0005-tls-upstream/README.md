# Fixture 0005-tls-upstream

## Property

Plaintext downstream → envoy / envoy-rust → upstream TLS origination to a
single `tls-echo-server` helper. The configured `sni: "envoy-rust.test"` is
sent in the upstream ClientHello server_name extension; the upstream TLS
server accepts the connection only because the harness CA validates and
the leaf cert's SAN matches.

## Differential surface

Both proxies dial the same `tls-echo-server` upstream. The post-handshake
byte stream round-trips `inputs/payload.bin` byte-exact in both directions.
The wire-level SNI is exercised by virtue of the upstream TLS server
producing a valid handshake (the rcgen-built leaf has SAN
`envoy-rust.test`; rustls's default verifier rejects mismatched SNI).

## ADRs referenced

- ADR-0015 — cross-container `host.docker.internal` + `host-gateway`
  (envoy-side backend host resolution).
- ADR-0017 — split phase 03 into 03.1 + 03.2.
- ADR-0018 — rcgen + tempfile dev-test-harness-only (the harness PKI used
  to build the upstream's cert).
- ADR-0019 — tokio-rustls + rustls-pemfile under the rustls grant
  (envoy-rust's UpstreamTls + the tls-echo-server's rustls usage).

## Out of scope (deferred)

Per parent-SPEC §4 + 03.2 SPEC §4:
- mTLS (out of phase 03 entirely).
- Inline cert/key bytes (filename only).
- `validation_context.match_subject_alt_names` (default rustls verifier
  asserts SAN matches `ServerName`).
- Wildcard SAN (the rcgen-built cert has the literal `envoy-rust.test` SAN).
- TLS protocol-version pin (rustls + Envoy v1.33.0 negotiate defaults).
