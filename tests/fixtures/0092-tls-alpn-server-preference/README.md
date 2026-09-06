# Fixture 0092-tls-alpn-server-preference

Cell 5 of the phase-112 ALPN cell table: the selection-order witness.

Identical to `0091-tls-alpn` except that the listener's list is **reversed** to
`alpn_protocols: ["http/1.1", "h2"]` while the client still offers
`h2, http/1.1`. Because the two lists now disagree on order, the selected
protocol discriminates server preference from client preference — it is
`http/1.1` iff selection follows the **server's** order.

MEASURED `http/1.1`, 5/5 runs, on upstream Envoy v1.33.0 at the phase-112
PLAN-write session. `rustls` agrees by construction: its selection loop
(`rustls-0.23.39/src/server/hs.rs`) iterates the server's list in the outer
position and only scans the client's with `.any()`, and
`ServerConfig::alpn_protocols` is documented "most preferred first".

**This fixture needs its own directory rather than a fifth probe in `0091`**
because ALPN is a `rustls::ServerConfig` property and a fixture may carry
exactly one listener (`ConfigError::TooManyListeners`; the merged static +
dynamic cap is one). One server ALPN list per fixture is therefore forced.

The cell is expected GREEN on both proxies; its value is that it would catch a
silent inversion of the preference rule on either side.
