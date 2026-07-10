# Fixture 0072 — `envoy.filters.network.rbac`, `action: DENY`

Phase 67.1 (ADR-0128 / ADR-0129 / ADR-0130 / ADR-0131). Chain: `[rbac(action: DENY, any), echo]`.

The probe (`Driver::TcpWithStats`, `probe: write_then_read_to_eof`) connects, writes
`inputs/payload.bin`, and reads to EOF. Both proxies write **zero bytes** and close with a
**clean EOF, never an RST**, discarding what the client sent; the terminal `echo` never runs.

**The write is required (ADR-0131).** Upstream Envoy evaluates network RBAC on the **first
downstream byte** (`ONE_TIME_ON_FIRST_BYTE`), not at connection establishment. A probe that
sends nothing is never evaluated: the connection simply stays open and no counter ticks. This
fixture's first draft used a `read_to_eof` (send-nothing) probe and hung against upstream Envoy
for the full 5 s deadline — which is how the divergence was found.

## Which assertion is the witness, and which are not

**`rbac_deny.rbac.denied == 1` is the only real witness here.**

`assert_body_rule`'s `ByteExact` is a bare `if envoy_body != rust_body { bail! }`. A fixture
asserting only *"both proxies returned zero bytes"* therefore **passes vacuously** against an
envoy-rust that never implemented RBAC at all and simply failed to write. That is why this
fixture carries `expected_stats` — and why phase 67.1 had to extend the raw-TCP driver family
to support them (phase-67 SPEC R-8).

The remaining three assertions are **consistency checks, not witnesses**:

- `rbac_deny.rbac.allowed == 0`
- `rbac_deny.rbac.shadow_allowed == 0`
- `rbac_deny.rbac.shadow_denied == 0`

`scrape_admin_stat` returns `Ok(0)` for a stat name the proxy never registered, so every
`value: 0` assertion passes vacuously on an **absent** name. They confirm the counter did not
tick; they cannot confirm it exists.

## Why `any: true` only

Locked by ADR-0128 decision (iv). `direct_remote_ip` would see the Docker bridge address
(`192.168.65.2` on the dev host, something else on CI) and `destination_port` would see a
`{{PORT}}` that **differs between the two proxies** by construction — upstream Envoy listens on
`CONTAINER_PORT = 10000` inside its container while the subject listens on a host-reserved
ephemeral port. Neither is host-deterministic under the Docker harness. Every IP/port matcher is
covered **in-process** in phase `67.2`, bound to `127.0.0.1` with a known port.

`action: ALLOW` / `action: DENY` over `any: true` completely witnesses both decision paths, both
counters, and the whole iteration protocol. Nothing in `67.1` is a stub.

## The byte-less connection has no differential observable here

A client that connects and never sends a byte is never evaluated by either proxy, and a client
that half-closes without sending gets a clean EOF from both (ADR-0131, measured). Neither is
exercised by this fixture's driver; both are pinned in-process by
`envoy_listener::chain_handler_skips_filters_when_client_closes_without_sending`.

## No differential observable for the post-EOF write

Upstream Envoy accepts a client write issued **after** the client observes EOF (ADR-0124's drain,
applied here to the DENY close). This fixture's driver never writes after EOF, so that clause has
**no differential observable**. It is pinned in-process by
`envoy_listener::close_with_drain_sends_clean_eof_and_accepts_post_eof_writes` and by
`crates/envoy-bin/tests/network_filter_rbac.rs::deny_post_eof_client_write_is_accepted_not_reset`.

## Why the two sides' `echo` filters differ

`envoy.yaml` gives `echo` a `typed_config`; `envoy-rust.yaml` does not. Upstream Envoy **requires**
it; envoy-rust rejects it (`UnexpectedTypedConfig`). This is the pre-existing ADR-0014 YAML shim
behind fixture `0001`, not a `67.1` divergence.
