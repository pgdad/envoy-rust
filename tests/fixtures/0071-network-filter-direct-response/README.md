# 0071 — `envoy.filters.network.direct_response`

The Network-filters family's first differential fixture (phase 66, ADR-0123).

**What it asserts.** Both proxies serve a listener whose sole network filter is
`envoy.filters.network.direct_response` with an `inline_string` payload. The
`tcp_direct_response` driver connects, **sends nothing**, and reads to EOF. The
two response bodies must be **byte-exact** equal.

**Why this is deterministic** (SPEC §0 R-0.5, witnessed against
`envoyproxy/envoy:v1.33.0`): the payload is written the moment the connection is
accepted, is byte-identical across connections, is unaffected by client input or
by client read timing, and the close is a clean EOF with no RST. No allow-list,
no timing tolerance, no stats assertion is required.

**Both sides carry the identical `typed_config`.** Unlike fixture `0001`
(`echo`), where upstream Envoy REQUIRES a `typed_config` that envoy-rust forbids
(the ADR-0014 YAML shim), `direct_response` needs no shim. The only difference
between `envoy.yaml` and `envoy-rust.yaml` is the bind address.

**What this fixture CANNOT catch.** Its driver never writes after observing EOF,
so it is blind to the read-half drain that ADR-0124 requires. That behavior is
pinned in-process by `post_eof_client_write_is_accepted_not_reset` in
`crates/envoy-bin/src/direct_response.rs`, whose doc comment carries the
mutation-check instruction.
