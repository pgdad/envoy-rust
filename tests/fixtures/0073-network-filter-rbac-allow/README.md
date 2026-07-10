# Fixture 0073 — `envoy.filters.network.rbac`, `action: ALLOW`

Phase 67.1 (ADR-0128 / ADR-0129 / ADR-0130 / ADR-0131). Chain: `[rbac(action: ALLOW, any), echo]`.

The probe (`Driver::TcpWithStats`, `probe: echo`) writes `inputs/payload.bin` (`PING-RBAC\n`),
reads exactly that many bytes back, then runs the ADR-0007 trailing-byte poll. The `rbac` filter
allows the connection and **yields to the terminal `echo`**, which round-trips the payload.

Both proxies evaluate the policy on the **first downstream byte** (`ONE_TIME_ON_FIRST_BYTE`,
ADR-0131), which this probe supplies. A connection that never sends a byte is never evaluated and
ticks no counter — on either proxy.

**This is the Network-filters family's first differential proof that a non-terminal filter runs
and then yields to the terminal filter — i.e. of the chain iteration protocol itself.**

## Which assertion is the witness, and which are not

Unlike fixture `0072`, the **body assertion here is not vacuity-prone**: a non-empty payload must
come back byte-for-byte, which an envoy-rust that failed to write could not fake.

`rbac_allow.rbac.allowed == 1` is nonetheless load-bearing for a *different* reason: it proves the
`rbac` filter **ran**, rather than being skipped. The body alone cannot distinguish "the chain
iterated through rbac and reached echo" from "the dispatcher ignored every filter but the terminal
one" — which is exactly what envoy-rust did before 67.1 (`main.rs` read `filters.first()`; SPEC
R-9). Without this assertion the fixture would pass against the old dispatch.

The remaining three assertions are **consistency checks, not witnesses**:

- `rbac_allow.rbac.denied == 0`
- `rbac_allow.rbac.shadow_allowed == 0`
- `rbac_allow.rbac.shadow_denied == 0`

`scrape_admin_stat` returns `Ok(0)` for a stat name the proxy never registered, so every `value: 0`
assertion passes vacuously on an **absent** name.

## Why `any: true` only

Locked by ADR-0128 decision (iv). `direct_remote_ip` would see the Docker bridge address
(`192.168.65.2` on the dev host, something else on CI) and `destination_port` would see a `{{PORT}}`
that **differs between the two proxies** by construction. Neither is host-deterministic under the
Docker harness. Every IP/port matcher is covered **in-process** in phase `67.2`, bound to
`127.0.0.1` with a known port.

## Why the two sides' `echo` filters differ

`envoy.yaml` gives `echo` a `typed_config`; `envoy-rust.yaml` does not. Upstream Envoy **requires**
it; envoy-rust rejects it (`UnexpectedTypedConfig`). This is the pre-existing ADR-0014 YAML shim
behind fixture `0001`, not a `67.1` divergence.
