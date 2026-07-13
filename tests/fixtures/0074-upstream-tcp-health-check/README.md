# Fixture 0074 — `upstream-tcp-health-check`

**Phase:** 68 (ADR-0136 / ADR-0137).
**Differential surface:** post-convergence active **TCP**-HC steady state on an H1 listener.

After ~3.5s settle, both proxies have attempted a connection-only
`tcp_health_check` against `{{DEAD_BACKEND_PORT}}` ≥2 times, observed the
connection refusal (ECONNREFUSED — no listener is ever bound to that port),
transitioned the sole endpoint to Unhealthy after `unhealthy_threshold: 2`
consecutive failures, and (with `healthy_panic_threshold: { value: 0 }`
disabling panic) make `pick()` return `None`. The H1 HCM `hcm.rs:582` arm fires
synth-503 with body `no healthy upstream` (19 bytes per ADR-0037 /
`synth_no_healthy_upstream`).

The discriminating bilateral observable is IDENTICAL to fixture 0019 (the
HTTP-HC ejection): **status 503 + body byte-exact + the 5 standard HTTP/1.1
headers** via `set-equal-modulo-allow-list`. Only the *cause* of ejection
differs: a connection-only `tcp_health_check` (an L4 connect probe) rather than
an HTTP `/healthz` probe. The TCP checker witnesses the same
`cluster.<n>.health_check.*` + `membership_*` stat tree phase 12 established.

**No backend process is spawned.** The `{{DEAD_BACKEND_PORT}}` harness marker
(`tests/differential/src/lib.rs`) reserves an ephemeral port via `reserve_port()`
and binds NO listener, so the reserved port refuses every probe for the test's
duration (ADR-0137 PV-2). This makes the ejection deterministic with no timeout
race — the connect refusal is an immediate `failure`, not an `active_hc_timeout`.
(If a host firewall drops rather than refuses the probe, the probe still fails
within `timeout: 1s` and the endpoint still ejects, so the 503 observable holds
either way.)

Integer-second durations (`1s`/`1s`) per §6.2 item-6 — the only duration form
both proxy parsers accept. The `receive`-scan path is not exercised
differentially (it is covered in-process in `crates/envoy-health/src/probe.rs`);
this fixture pins the connection-only ejection consequence (ADR-0137 PV-3).
