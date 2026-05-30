# Fixture 0022 — `upstream-outlier-detection-consecutive-5xx`

**Phase:** 14.2 (parent-14 D8.1).
**Differential surface:** consecutive-5xx passive outlier-detection ejection on
an H1 listener, plus the no-healthy-upstream synth-503 follow-on.

## What it exercises

After 3 consecutive backend 5xx responses on a single-endpoint cluster, the
`outlier_detection.consecutive_5xx` detector ejects the endpoint; with panic
disabled the now-empty healthy pool yields the 12.2 no-healthy-upstream
synthetic 503 on the next request.

## Topology

One HTTP/1.1 HCM listener routes `/fail` and `/` to a single-endpoint
`STRICT_DNS` cluster `backend_cluster`, pointed at the configurable-status
backend (`health-aware-http1-backend`, the 13.x helper). The harness starts
the backend with `--per-path /fail=500`, so `/fail` returns a backend 500
(`server error\n`, 13 bytes) and `/` returns 200. The cluster carries:

```yaml
outlier_detection:
  consecutive_5xx: 3
  base_ejection_time: 60s
  max_ejection_percent: 100
  interval: 1s
common_lb_config:
  healthy_panic_threshold: { value: 0 }
```

`healthy_panic_threshold: 0` disables panic-mode routing, so once the single
endpoint is ejected the cluster genuinely has no healthy host.

## Discriminating observable

Driver `http1_keep_alive` (13.1 D10, extended at 14.2 Task 6 with per-request
`expected_body` / `require_header_present` / `require_header_absent`). One
keep-alive connection issues 4 sequential `GET /fail`:

| # | upstream result | status | body (byte-exact) | `x-envoy-upstream-service-time` |
|---|-----------------|--------|-------------------|---------------------------------|
| 1 | backend 500 (counter 0→1) | 500 | `server error\n` (13 B) | present |
| 2 | backend 500 (counter 1→2) | 500 | `server error\n` (13 B) | present |
| 3 | backend 500 (counter 2→3 = threshold → **eject**) | 500 | `server error\n` (13 B) | present |
| 4 | no healthy endpoint | 503 | `no healthy upstream` (19 B) | absent |

After a 500ms settle the five consecutive-5xx ejection counters are asserted
bilaterally:

- `cluster.backend_cluster.outlier_detection.ejections_active == 1`
- `cluster.backend_cluster.outlier_detection.ejections_enforced_total == 1`
- `cluster.backend_cluster.outlier_detection.ejections_enforced_consecutive_5xx == 1`
- `cluster.backend_cluster.outlier_detection.ejections_detected_consecutive_5xx == 1`
- `cluster.backend_cluster.outlier_detection.ejections_overflow == 0`

`ejections_detected_consecutive_5xx == 1` reflects the single threshold
crossing. `ejections_overflow == 0` because, with one host and
`max_ejection_percent: 100`, the eject cap is `floor(1 * 100%) = 1` and the
cap-check sees `active_count == 0 < cap 1` before ejecting.

## Reuse

- **12.2 no-healthy-upstream synth-503 contract row** — request 4 reuses the
  exact `no healthy upstream` (19-byte) synthetic response from 12.2.
- **13.x `health-aware-http1-backend`** — the per-path configurable-status
  backend, driven here with `--per-path /fail=500`.
- **T6-extended keep-alive driver** — no new harness primitive; Task 6 added
  the per-request body / header-presence / header-absence assertion fields.

## Deferred Envoy-only stat names

Envoy emits the full outlier-detection stat family (success-rate,
failure-percentage, local-origin, and consecutive-gateway-failure detector
counter pairs, plus the legacy aliases `ejections_total` /
`ejections_consecutive_5xx`). envoy-rust emits only the minimum-viable
consecutive-5xx subset asserted above. The remaining 13 Envoy-only names are
NOT listed in `expectations.yaml`: this fixture's `Driver::Http1KeepAlive`
stat path asserts only the named `expected_stats` (no full prometheus
set-diff), so unasserted Envoy-only names are simply ignored — there is no
`allowlist_envoy_only` key on the keep-alive driver (that key belongs to the
prometheus-set-diff `BodyRule`, and the `Driver` enum's `deny_unknown_fields`
rejects it here).

The deferred set is catalogued instead in
`docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the 14.1 outlier-detection stat table).
The prior "14-claimed-vs-13-enumerated" prose-count discrepancy (carryforward
M8) is reconciled to **13** by phase 14.2 Task 9. The live Envoy emission is
observed and reconciled against this list at Task 10 (state-4 verification) /
in CI.

## Docker-gated

The wrapper test (`tests/differential/tests/upstream_outlier_detection.rs`)
runs under the differential harness, which self-skips when Docker
(`envoyproxy/envoy:v1.33.0`) is unavailable. The live bilateral run happens at
Task 10 / in CI.
