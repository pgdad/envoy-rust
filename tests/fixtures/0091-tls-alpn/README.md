# Fixture 0091-tls-alpn

Cells 1–4 of the phase-112 ALPN cell table. A `tcp_proxy` listener terminates
downstream TLS with a leaf whose SAN is `a.example.com` (rcgen-generated at
fixture-run time per ADR-0018, signed by the harness CA), and its
`common_tls_context` advertises `alpn_protocols: ["h2", "http/1.1"]`. Both
proxies dial the same plaintext echo backend.

`Driver::TlsTcpProbeList` drives four independent TLS handshakes against the
one listener, varying only the **client's** offer — which is why one fixture
covers four cells and why `TlsTcpProbeList` rather than `TlsTcp` is the right
driver. Each probe asserts the negotiated protocol on BOTH proxies through
`expected_alpn`; both sides satisfying the rule is the cross-proxy equivalence
claim (112.2 SPEC §3 E3), so this driver needs no final `assert_equivalence`.

| probe | client offers | expected |
|---|---|---|
| 1 | `h2`, `http/1.1` | `h2` — the server's first choice |
| 2 | `http/1.1` | `http/1.1` |
| 3 | `h3` | nothing negotiated, handshake SUCCEEDS |
| 4 | *(no ALPN extension)* | nothing negotiated |

Every row was MEASURED on upstream Envoy v1.33.0 at the phase-112 and 112.1
PLAN-write sessions, 45/45 runs each, against the `ENVOY_TARGET.md` digest
verified on the running container.

**Probe 3 is the load-bearing one.** `rustls` by default sends a fatal
`no_application_protocol` alert when a client's non-empty offer misses a
non-empty server list, and upstream Envoy does not. Sub-phase 112.1's D6′
`LazyConfigAcceptor` accept path exists to remove that divergence; probe 3 is
its cross-proxy witness, and it goes RED if that path regresses.

What is *out* of this fixture:

- Server-preference ordering — fixture `0092-tls-alpn-server-preference`.
- The no-ALPN control (cell 6) — it rides on `0004-tls-downstream`, because a
  second listener is illegal (`ConfigError::TooManyListeners`) and per-chain
  ALPN is inexpressible (`DownstreamTls::from_listener` builds one
  `rustls::ServerConfig` per listener; CF-112-4).
- The UPSTREAM ALPN offer — no driver can report what a backend negotiated
  (CF-112-2).
- ALPN × SNI (CF-112-3), and `Http2OverTlsNotSupported` (CF-112-1): `h2` is
  advertised here but never spoken; the filter chain is `tcp_proxy`.
