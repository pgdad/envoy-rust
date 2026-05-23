# Fixture 0019 — `upstream-active-health-check`

**Phase:** 12.2 (parent-12 D7).
**Differential surface:** post-convergence active-HC steady state on an H1 listener.

After ~3.5s settle, both proxies have probed the synthetic backend at `/healthz` ≥1
time, observed the 503, transitioned the sole endpoint to Unhealthy, and (with
`healthy_panic_threshold: { value: 0 }` disabling panic) make `pick()` return
`None`. The H1 HCM `hcm.rs:582` arm fires synth-503 with body `no healthy upstream`
(19 bytes per ADR-0037 / `synth_no_healthy_upstream`).

The discriminating bilateral observable: **status 503 + body byte-exact + the 5
standard HTTP/1.1 headers** via `set-equal-modulo-allow-list`. The `server` +
`date` header values diverge per the existing 04.1 allow-list rows.

Integer-second durations (`1s`/`1s`) per §6.2 item-6 — the only duration form
both proxy parsers accept.

The synthetic backend is launched by the harness (`HealthAwareHttp1Backend`;
12.2 D7.1 / the 06.3 REVIEW I2 down-payment).
