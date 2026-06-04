# Fixture 0028 — `xds-file-based-rds`

**Phase:** 20 (`20-xds-file-based-rds`; ADR-0051 SPEC / ADR-0052 PLAN).
**Differential surface:** file-based RDS — the HCM
`rds.config_source.path_config_source.path` initial-load path, composed with
file-based CDS (the phase-18 surface). The xDS-family continuation that
completes the **CDS + LDS + RDS** filesystem-dynamic-config triad.

## What it exercises

This is the differential payoff of phase 20: it proves that envoy-rust's
file-based-RDS load (Tasks 1–6) and real Envoy v1.33.0 produce identical
observable behaviour given identical configs, end-to-end through the §5.7
RDS+CDS composition. The HCM carries **NO inline `route_config`** — the route
table exists **only** because each proxy loaded its RDS file at boot. TWO GETs
over ONE H1 keep-alive connection:

| path | routed cluster | cluster source | result |
|------|----------------|----------------|--------|
| `/static`  | `static_backend`  | `static_resources.clusters` (a STATIC cluster) | backend **200** |
| `/dynamic` | `dynamic_backend` | the CDS file (NOT `static_resources`)          | backend **200** |

Both clusters are STRICT_DNS / ROUND_ROBIN / V4_ONLY and resolve to the same
http1-echo-server helper; the probes differ only in the `host` header they send
(which the helper echoes back, making the byte-exact body assertion the
data-plane wire-shape pin) and the route they hit.

### The three bilateral observables

1. **Data-plane route through an RDS-only route table.** The route table exists
   only because of the RDS file. A proxy that parsed the bootstrap but
   **ignored the HCM's `rds.config_source`** would have **no route table** and
   404 on every route. A successful **200** on either probe therefore proves
   the RDS file was loaded and the route table installed. Probe 1 (`/static` →
   a STATIC cluster) discriminates RDS-loaded from not-loaded **independently
   of CDS**; probe 2 (`/dynamic` → the CDS cluster) proves the §5.7
   composition — a request whose route table AND cluster both arrive from
   dynamic-resource files (the cluster-before-route-revalidation ordering).
2. **The RDS/CDS stat subsets** (see "Stats (L3)" below).
3. **The `RoutesConfigDump` + `ClustersConfigDump` admin scrapes** (see
   "`/config_dump` (L5)" below).

## Topology

- **Listener:** ONE STATIC H1 HCM listener (`ingress_http1` stat_prefix) in
  `static_resources.listeners`. Its HCM has NO inline `route_config`; instead it
  carries `rds: { route_config_name: local_route, config_source: {
  resource_api_version: V3, path_config_source: { path: {{RDS_PATH}} } } }`.
- **RDS:** the SHARED `rds.yaml` — one `RouteConfiguration` named `local_route`,
  vh `domains: ["*"]`, routes `/static` → `static_backend`, `/dynamic` →
  `dynamic_backend`. NO `validate_clusters` (L6).
- **CDS:** `dynamic_resources.cds_config` with `resource_api_version: V3` and a
  `path_config_source.path` pointing at the rendered `cds.yaml` (the
  fixture-0026 shape verbatim, cluster `dynamic_backend`).
- **Static cluster:** `static_backend` in `static_resources.clusters` (keeps
  probe 1 orthogonal to CDS).
- **Backend:** the http1-echo-server helper, spawned by the harness keyed on
  the `{{HTTP1_BACKEND_PORT}}` marker. The marker appears in both per-side main
  configs and in `cds.yaml`; the harness's backend-launch scan covers all
  scanned renditions so a single backend is spawned and all occurrences resolve
  to it.

## The SHARED `rds.yaml` and per-side byte-exact-body stripping

UNLIKE the per-side **main**/**LDS** templates of fixtures 0026/0027, the RDS
file is delivered as a **SINGLE SHARED** `rds.yaml` rendered per-side through
each side's kv map (the Task 6 harness supports one shared `rds.yaml`, not
per-side RDS files). The shared file therefore carries **NO Envoy-only fields**:
envoy-rust's `RouteConfiguration` / `VirtualHost` use `deny_unknown_fields` and
do **not** accept `request_headers_to_remove`, so the route-config-level header
stripping that fixtures 0026/0027 used **cannot** live in `rds.yaml`.

Instead the Envoy-side proxy-injected upstream-header stripping moves into the
Envoy-side **MAIN** config (`envoy.yaml`; `envoy-rust.yaml` omits all of it),
using HCM/router knobs that are NOT part of the route table:

- **`generate_request_id: false`** on the Envoy HCM — suppresses the per-request
  UUID `x-request-id` (envoy-rust injects none).
- an Envoy-only **`envoy.filters.http.header_mutation`** filter BEFORE the
  router, with decode-side `remove:` of `x-forwarded-for` and `x-forwarded-proto`
  — the proxy-injected upstream REQUEST headers the HCM adds BEFORE the filter
  chain (so a decode-side mutation can catch them). envoy-rust supports
  header_mutation (phase 07.2) but its side simply omits this filter.
- the router's **`suppress_envoy_headers: true`** — suppresses the `x-envoy-*`
  headers the ROUTER injects on the upstream-bound request, notably
  `x-envoy-expected-rq-timeout-ms`, which is added at upstream-forward time
  DOWNSTREAM of the decode filters, so a `header_mutation` `remove` cannot catch
  it. `suppress_envoy_headers` is the only knob that strips it.

`suppress_envoy_headers` ALSO drops the *response* header
`x-envoy-upstream-service-time` (which envoy-rust always emits and which
`require_header_present` asserts on BOTH sides), so the same `header_mutation`
filter's `response_mutations` re-adds it (`append` / `APPEND_IF_EXISTS_OR_ADD`,
value `"0"`) on the Envoy side — a presence-only assertion, and the value
legitimately differs per side. envoy-rust emits `x-envoy-upstream-service-time`
natively (so its side needs no re-add) and injects none of the stripped request
headers, so its side carries NONE of these knobs.

This intentional per-side field-set divergence is the same posture as fixtures
0008/0026/0027 — the stripping/suppression on the Envoy side is what makes both
proxies forward the **same** request to the echo helper, so the byte-exact body
assertion (`method: GET\npath: /static\nheaders:\n  host: static_backend\nbody:
\n`, only `host` surviving) holds bilaterally. envoy-rust forwards only `host`
natively, so its body is already minimal; the Envoy side is stripped down to
match.

## L6 — no `validate_clusters`

The `rds.yaml`'s `RouteConfiguration` carries NO `validate_clusters` field. The
route's `dynamic_backend` cluster is CDS-supplied (not present at config-load),
and RDS-delivered route_configs are not subject to Envoy's inline cluster
validation; both routes resolve their clusters at request time (the §5.7
cluster-before-route-revalidation).

## PathConfigSource `.yaml`-extension constraint

Envoy's `PathConfigSource` infers the wire format from the file **extension**:
the RDS and CDS files must end in `.yaml`. The harness mounts each at a `.yaml`
container path precisely so the extension survives.

## Stats (L3)

`expected_stats` asserts the conditional per-HCM
`http.ingress_http1.rds.local_route.*` family that only registers when the HCM's
`rds` is configured (the §5.2 per-HCM conditional-registration discipline;
inline-route HCMs emit no `rds.*` names), alongside the fixture-0026 CDS family
(note `cluster_added` / `active_clusters` are **2** here — the static
`static_backend` PLUS the CDS `dynamic_backend`):

| stat | value |
|------|-------|
| `http.ingress_http1.rds.local_route.update_attempt`  | 1 |
| `http.ingress_http1.rds.local_route.update_success`  | 1 |
| `http.ingress_http1.rds.local_route.update_failure`  | 0 |
| `http.ingress_http1.rds.local_route.update_rejected` | 0 |
| `http.ingress_http1.rds.local_route.config_reload`   | 1 |
| `cluster_manager.cds.update_attempt`      | 1 |
| `cluster_manager.cds.update_success`      | 1 |
| `cluster_manager.cds.update_failure`      | 0 |
| `cluster_manager.cds.update_rejected`     | 0 |
| `cluster_manager.cluster_added`           | 2 |
| `cluster_manager.active_clusters`         | 2 |

plus the per-cluster `cluster.{static,dynamic}_backend.upstream_rq_total` (1/1)
and the HCM `http.ingress_http1.downstream_rq_{total,2xx}` (2/2) counters.

### L3 Envoy-only stat enumeration (not asserted)

Envoy v1.33 emits a larger RDS scope than the five lock-in names. Most notably
the Envoy-only `http.ingress_http1.rds.local_route.{version, version_text,
update_time, config_reload_time_ms, update_empty, init_fetch_timeout,
update_duration}` names (plus `.control_plane.*`), which envoy-rust does not
emit at this scope (all RDS load failures are fatal pre-registration, so
`update_failure`/`update_rejected` register at 0 and never tick;
`config_reload` ticks 1 at initial load). The named-stat scrape asserts only the
five lock-in #L3 rds names and does **no** set-diff, so the Envoy-only family is
ignored rather than allow-listed (the same disposition fixtures 0026/0027 take
for the Envoy-only `cluster_manager.*` / `listener_manager.*` families).

## `/config_dump` (L5)

The per-side config_dump **order differs**, so the RoutesConfigDump assertion
uses the Task 6 per-side `JsonSubtreeRule` path override
(`path_envoy` / `path_envoy_rust`):

- **Envoy** emits the full bootstrap config projection: Bootstrap[0],
  Clusters[1], Listeners[2], **ScopedRoutes[3]**, **Routes[4]**. The
  RoutesConfigDump therefore lands at `configs[4]` (Envoy interposes a
  ScopedRoutesConfigDump at [3] even with no scoped routes).
- **envoy-rust** gates the ListenersConfigDump off when no `lds_config` is
  present (the §config_dump conditional-emission discipline), so its order is
  Bootstrap[0], Clusters[1], **Routes[2]**. The RoutesConfigDump lands at
  `configs[2]`.
- Both sides agree the first `dynamic_route_configs` entry's
  `route_config.name == "local_route"`.

A second `/config_dump` sub-case re-checks fixture-0026 compatibility: the
`ClustersConfigDump` still lands at `configs[1]` on BOTH sides and its first
`dynamic_active_clusters` entry's `cluster.name == "dynamic_backend"`.

## Negative paths (L4 — backstop-only)

The negative-path RDS divergences (missing file, malformed envelope, unknown
`@type`, route_config_name mismatch, etc.) are covered by the in-process
backstop test (Task 8), NOT by this Docker-gated differential fixture: Envoy
v1.33 **exits the process** on a fatal RDS load error, which the differential
harness cannot observe as a data-plane response. Per ADR-0051 the negative paths
are backstop-only.
