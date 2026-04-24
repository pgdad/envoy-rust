# Fixture 0002 — Static admin `/ready`

This fixture drives `GET /ready` at the admin endpoint of upstream Envoy
(`envoyproxy/envoy:v1.33.0`) and envoy-rust, asserting that both return
identical HTTP status (`200 OK`) and response body (`LIVE\n`). Header
equivalence is intentionally out of scope for phase 01 — the
`BEHAVIOR_CONTRACT.md` header allow-list is populated starting phase 04
(ADR-0011).

The `envoy.yaml` and `envoy-rust.yaml` differ only in bind address: upstream
Envoy runs inside a container and binds `0.0.0.0`; the envoy-rust subject
runs as a host subprocess and binds `127.0.0.1` (SPEC §6 signpost 3). Both
templates use the same `{{ADMIN_PORT}}` token; the harness substitutes each
side's reserved port independently.

This is the first fixture to use the `http_get` driver introduced by SPEC
§D5. Future admin fixtures (phase 08 for `/stats`, `/clusters`,
`/config_dump`, and drain) reuse the driver.

No `inputs/` directory: `http_get`'s payload (path + host) lives in
`expectations.yaml`.

## Known Envoy v1.33.0 quirks

If upstream Envoy rejects this YAML at container start (stderr shows a
validation error), add `access_log_path: /dev/null` to the `admin:` block —
some Envoy releases require that field on admin bootstraps. Record the fix
in this file's PROGRESS.md deviation note, not as an ADR.
