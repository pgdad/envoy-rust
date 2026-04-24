# Fixture 0001-tcp-echo

This fixture drives identical bytes at upstream Envoy's
`envoy.filters.network.echo` filter and at envoy-rust's phase-00 echo listener,
asserting byte-exact response body equivalence. It is the first differential
fixture in the project and establishes the harness contract for subsequent
TCP fixtures.

- **Payload:** `inputs/payload.bin` — 18 bytes of deterministic ASCII
  (`hello, envoy-rust\n`). Kept trivially inspectable.
- **Equivalence:** body byte-exact; no header/trailer/stat/timing clauses.
- **Port:** templated via `{{PORT}}`; rendered by the harness. envoy-rust binds
  the rendered port directly; upstream Envoy binds it inside the container and
  testcontainers host-maps to a random port.

## Phase 01 migration

The `expectations.yaml` grammar acquired a tagged `driver:` discriminator in
phase 01 (SPEC §D5). The fixture's behavior is unchanged — it still drives
`inputs/payload.bin` at both proxies and asserts byte-exact bodies. The only
shape change is the new `driver: { kind: tcp_echo }` stanza.

Related ADRs: ADR-0008 (envoy-config extraction), ADR-0011 (header equivalence
deferred to phase 04).
