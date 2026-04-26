# Fixture 0006-tls-sni

## Property

Downstream TLS with multi-cert SNI cert selection. One listener; two filter
chains; each chain carries a different cert keyed on its
`filter_chain_match.server_names`. Plaintext upstream backend
(`tcp-echo-server` from phase 02.1).

## Differential surface

Two probes per test invocation:
- Probe A: connect with SNI `a.example.com`; assert post-handshake peer cert
  SAN/CN contains `a.example.com`; round-trip `inputs/payload.bin` byte-exact.
- Probe B: connect with SNI `b.example.com`; assert peer cert SAN/CN contains
  `b.example.com`; round-trip `inputs/payload.bin` byte-exact.

The cert-selection assertion lives in the harness driver
(`drive_tls_probes`), not as a new equivalence-matrix dimension. Both proxies
must select the *same* cert for the *same* SNI for the test to pass — that
is the property under test. The matrix row engaged is still row 2 of §7.2
(post-handshake bytes byte-exact).

## SNI resolution mechanism

`rustls::server::ResolvesServerCert` keyed on lowercased ClientHello SNI
(rustls 0.23 returns lowercase SNI; envoy-tls's SniResolver stores lowercase
keys; case-insensitive exact match). Envoy mirrors via
`filter_chain_match.server_names` matching with the same case-insensitive
contract. The validator (envoy-config) rejects overlapping SNIs and
multiple catch-all chains at config-load time.

Unknown-SNI close behavior is **not** asserted in this fixture (parent-SPEC
§6 signpost 8 — adding a third probe with `expected_close: bool` is a
future-fixture option that lands its own ADR; the TLS-alert delta vs.
plain-close delta is potentially divergent between rustls and Envoy).

## ADRs referenced

- ADR-0017 — split phase 03 into 03.1 + 03.2.
- ADR-0018 — rcgen + tempfile dev-test-harness-only.
- ADR-0019 — tokio-rustls + rustls-pemfile under the rustls grant.

## Out of scope (deferred)

Per parent-SPEC §4 + 03.2 SPEC §4:
- Wildcard SAN values (`*.example.com`) — `TlsTestPki` does not generate them.
- mTLS — out of phase 03 entirely.
- `validation_context.match_typed_subject_alt_names` — out of phase 03.
- `tls_inspector` listener filter (would unlock TLS-and-plaintext mixing on
  the same listener) — out of phase 03.
- Filter-chain framework / per-route TLS config — phase 07.
