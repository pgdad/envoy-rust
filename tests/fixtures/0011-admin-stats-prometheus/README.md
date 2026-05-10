# Fixture 0011 — admin-stats-prometheus

**Phase:** 06.1.

**Surface:** First admin-side differential fixture in the project. Drives
the new `Driver::AdminScrape` (06.1 D6.a) which sequences HCM-side
pre-requests (so registry counters increment) followed by an admin scrape
of `/stats/prometheus`, then asserts the metric-name set is equal between
upstream Envoy and envoy-rust modulo the empirically-seeded
`allowlist_envoy_only`.

**Configuration:**

- HCM listener: `ingress_http` binds `0.0.0.0:{{PORT}}` (Envoy) /
  `127.0.0.1:{{PORT}}` (envoy-rust). HCM `codec_type: HTTP1`. Single
  virtual host (`domains: ["*"]`) with one route (`prefix: "/"`) that
  serves `direct_response` 200 `"ok\n"` (no upstream cluster, so the
  fixture is self-contained: no helper backend process to spawn).
- Admin listener: binds `0.0.0.0:{{ADMIN_PORT}}` (Envoy) /
  `127.0.0.1:{{ADMIN_PORT}}` (envoy-rust). The `{{ADMIN_PORT}}` marker is
  satisfied by `run_fixture`'s admin-port reservation (see
  `tests/differential/src/lib.rs::run_fixture`'s `needs_admin_port`
  branch, 06.1 D6.a).
- Cross-sub-phase rule 3: admin is HTTP/1.1 only in 06.1; this fixture
  does NOT set TLS, ALPN, or HTTP/2 on the admin listener.

**Test driver:** `Driver::AdminScrape { pre_requests: [GET / on
{{PORT}}], path: "/stats/prometheus", expected_status: 200,
expected_content_type: "text/plain; charset=UTF-8",
expected_body_rule: PrometheusExposition { allowlist_envoy_only:
[...empirical...], allowlist_envoy_rust_only: [] } }`.

**Backend:** none. The HCM listener serves `direct_response` so no
helper process is required.

## Empirical allow-list seeding (SPEC §6 signpost 12)

The `allowlist_envoy_only` is populated from the first run's
`envoy-only` diff and represents metrics that upstream Envoy emits but
envoy-rust does not (yet, in 06.1). Final size: **202 entries**.
Categories:

- `server.*` (29) — server-state stats (uptime, live, memory,
  hot_restart, fips_mode, version, …). envoy-rust emits no server stats
  today; deferred to a later phase.
- `http.downstream.*` (60) — HCM stats beyond `downstream_rq_total`
  (the one HCM stat envoy-rust emits in 06.1).
- `listener.*` + `listener.admin.*` (46) — auto-emitted listener stats
  for both the HCM and admin listeners. envoy-rust emits only the bare
  `downstream_cx_total` per listener.
- `listener_manager.*` (12) — listener manager book-keeping. envoy-rust
  has no listener manager surface in 06.1.
- `cluster_manager.*` (9) — cluster manager book-keeping. envoy-rust
  has no cluster manager surface in 06.1.
- `runtime.*` (9) — RTDS runtime layer. Deferred to the xDS family.
- `filesystem.*` (6) — file I/O stats. envoy-rust does not emit.
- `http.tracing.*` (5), `http.passthrough.*` (5), `http.rq.*` (5),
  `http1.*` (4), `http.no_*`/`rs.*` (3) — HCM-adjacent counters.
- `overload.*` (3), `main_thread.*` (2), `workers.*` (2),
  `thread_local.*` (2), `tcmalloc.*` (1) — runtime-overload bookkeeping.

## Prometheus name-vs-label shape divergence

`allowlist_envoy_rust_only` carries **2 entries**:
`envoy_http_ingress_http_downstream_rq_total` and
`envoy_listener_ingress_http_downstream_cx_total`. These are paired
with the corresponding upstream-Envoy bare names
(`envoy_http_downstream_rq_total` /
`envoy_listener_downstream_cx_total`) on the `allowlist_envoy_only`
side: both proxies emit the same counters; the Prometheus *shape*
differs:

- Upstream Envoy projects dynamic name segments (HCM `stat_prefix`,
  listener `name`) into Prometheus *labels*:
  `envoy_http_downstream_rq_total{envoy_http_conn_manager_prefix="ingress_http"} 1`.
- envoy-rust embeds the dynamic segment in the metric name:
  `envoy_http_ingress_http_downstream_rq_total 1`.

This is documented in BEHAVIOR_CONTRACT.md "Stat-name mapping" §06.1
under "Prometheus exposition shape divergence". Resolution defers to a
later phase that adds a `StatsTagExtractor`-equivalent; when that
lands, the paired allow-list entries drop together (no contract
loosening — the dot-tree contract `http.<stat_prefix>.downstream_rq_total`
remains value-exact).

## Cross-references

- Phase 06.1 SPEC §3 D6 — fixture deliverable.
- Phase 06.1 PLAN.md Task 13 — fixture authoring + empirical seeding.
- SPEC §6 signpost 12 — allow-list seeding doctrine.
- SPEC §6 signpost 11 — 50ms Relaxed-ordered counter visibility window
  (consumed inside `drive_admin_scrape`; not configurable here).
- ADR-0029 — phase 06 split into 06.1 / 06.2 / 06.3.
- BEHAVIOR_CONTRACT.md — admin endpoint set + content-type pin.
- Phase 04.3 fixture 0008 — `direct_response` precedent (the HCM-side
  `request_headers_to_remove` block is borrowed from there for
  cross-fixture consistency, even though direct_response responses
  don't trigger upstream-side header rewrites).

**Acceptance signal:** the fixture is green at the Docker-gated CI
level (`tests/differential/tests/admin_stats_prometheus.rs`). No
in-process backstop exists for this fixture (the differential level is
the only level that exercises the upstream-Envoy admin output).
