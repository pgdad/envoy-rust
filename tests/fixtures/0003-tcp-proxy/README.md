# Fixture 0003-tcp-proxy

This fixture drives an arbitrary byte payload through a listener configured with
`envoy.filters.network.tcp_proxy` → static cluster `backend` (one endpoint) → a
host-local `tcp-echo-server` helper process (the binary landed in phase 02.1).
Both upstream Envoy and envoy-rust dial the same backend.

The `driver.kind: tcp_echo` value refers to the harness's round-trip pattern
(write payload, read-exact, compare), not to Envoy's echo *filter* — reusing
the same `TcpEcho` driver across fixtures 0001 and 0003 proves that the harness
is data-plane-agnostic.

Cross-container host reachability is covered by ADR-0015; the
`{{BACKEND_HOST}}` divergence between `envoy.yaml` and `envoy-rust.yaml` is its
only non-harness divergence. Half-close posture is Envoy's v1.33.0 default
(`enable_half_close: false`), covered by ADR-0016.
