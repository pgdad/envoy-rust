# Fixture 0029 — `xds-file-based-eds`

**Phase:** 21 (`21-xds-file-based-eds`; ADR-0053 SPEC / ADR-0054 reconciliation).
**Differential surface:** file-based EDS — a STATIC cluster declared `type: EDS`
with `eds_cluster_config.eds_config.path_config_source.path` (instead of an
inline `load_assignment`) loads its `ClusterLoadAssignment` (endpoints) from a
YAML file at startup, routes data-plane traffic to those endpoints, and exposes
the load via `cluster.<name>.update_*` + the `/config_dump` `EndpointsConfigDump`
section. The xDS-family **4th member**, completing **CDS + LDS + RDS + EDS**.

## What it exercises

This is the differential payoff of phase 21: it proves envoy-rust's file-based
EDS load (Tasks 1–6) and real Envoy v1.33.0 produce identical observable
behaviour given identical configs, end-to-end. The cluster carries **NO inline
`load_assignment`** — its endpoints exist **only** because each proxy loaded its
EDS file at boot. ONE GET over an H1 keep-alive connection:

| path | host | routed cluster | endpoint source | result |
|------|------|----------------|-----------------|--------|
| `/`  | `eds_backend` | `eds_backend` (`type: EDS`) | the SHARED eds.yaml (NOT inline) | backend **200** |

The cluster resolves to the http1-echo-server helper; the probe's `host` header
is echoed in the body, making the byte-exact body assertion the data-plane
wire-shape pin.

### The three bilateral observables

1. **Data-plane route through an EDS-supplied endpoint.** The cluster's
   endpoints exist only because of the EDS file. A proxy that parsed the
   bootstrap but **ignored the cluster's `eds_cluster_config`** would have a
   cluster with **no endpoints** and 503 (no healthy upstream). A successful
   **200** + echoed body proves the EDS file was loaded and the endpoint
   installed (L2 — endpoints active before `/ready`, no warm-up window).
2. **The per-cluster EDS stat subset** (see "Stats (L3)" below).
3. **The `EndpointsConfigDump` admin scrape** (see "`/config_dump?include_eds`
   (L5)" below).

## Topology

- **Listener:** ONE STATIC H1 HCM listener (`ingress_http` stat_prefix) in
  `static_resources.listeners`, with an **INLINE** `route_config` (`local_route`,
  vh `domains: ["*"]`, route `/` → `eds_backend`) — unlike fixture 0028's RDS,
  the route table is static here. `http_filters` ends in the router.
- **Cluster:** ONE STATIC cluster `eds_backend` in `static_resources.clusters`
  with `type: EDS` + `lb_policy: ROUND_ROBIN`, NO inline `load_assignment`, and
  `eds_cluster_config: { eds_config: { resource_api_version: V3,
  path_config_source: { path: {{EDS_PATH}} } } }`.
- **NO `dynamic_resources`:** the cluster is **static-but-EDS** — the EDS file is
  loaded by the cluster's own `eds_cluster_config`, not by CDS. (C16: the EDS
  pass + the post-merge validate-gate fire whenever ANY cluster is `type: EDS`,
  regardless of `dynamic_resources`.)
- **EDS:** the SHARED `eds.yaml` — one `ClusterLoadAssignment` named
  `eds_backend`, one locality, one `lb_endpoint` → the numeric backend IP.
- **Backend:** the http1-echo-server helper, spawned by the harness keyed on the
  `{{HTTP1_BACKEND_PORT}}` marker (present in both main configs and eds.yaml; the
  harness backend-launch scan covers all renditions so one backend is spawned).

## The SHARED `eds.yaml`, numeric IP, and the per-side `{{EDS_BACKEND_IP}}` marker (L1 / L9)

Like fixture 0028's rds.yaml, the EDS file is delivered as a **SINGLE SHARED**
`eds.yaml` rendered per-side through each side's kv map (the Task 6 harness
supports one shared `eds.yaml`). The shared file carries **NO Envoy-only
fields**.

The load-bearing reconciliation (L1 / L9 / D6): **EDS rejects hostnames.** An
endpoint `socket_address.address` that is a DNS name is rejected by Envoy
(`malformed IP address` → `update_rejected`) — EDS endpoints are **resolved
socket addresses** (STATIC semantics, NOT STRICT_DNS). So the backend address
**must be a numeric IP that differs per side**, and the file uses a NEW
`{{EDS_BACKEND_IP}}` marker (NOT `{{BACKEND_HOST}}`):

- **upstream (Envoy) side** → the **runtime-discovered numeric host-gateway IP**.
  The harness discovers it once via `getent ahostsv4 host.docker.internal` in the
  pinned Envoy image (`192.168.65.254` on this macOS Docker Desktop machine; the
  bridge IP on Linux CI), gated to EDS fixtures (the `needs_eds` `{{EDS_PATH}}`
  scan). The `--add-host=host.docker.internal:host-gateway` mapping wires
  Envoy's container to reach the host-spawned echo helper.
- **subject (envoy-rust) side** → `127.0.0.1` (envoy-rust runs in-process on the
  host, so the backend is loopback-reachable).

The per-side numeric-IP rendition joins the backend/host-gateway scans (the
phase-18 bug-class lesson: every per-side template must be scanned so a single
backend is spawned and the host-gateway mapping is applied where referenced).

## Per-side main-config field-set split

Identical posture to fixtures 0008/0026/0027/0028: the Envoy side (`envoy.yaml`)
carries the Envoy-only byte-exact-body stripping knobs in its MAIN config
(`generate_request_id: false`; a `header_mutation` filter removing
`x-forwarded-for`/`x-forwarded-proto` and re-adding the response
`x-envoy-upstream-service-time` that `suppress_envoy_headers` strips; the
router's `suppress_envoy_headers: true`). envoy-rust emits these natively, so
`envoy-rust.yaml` omits ALL of them. Both sides forward the SAME request to the
echo helper, so the byte-exact body
(`method: GET\npath: /\nheaders:\n  host: eds_backend\nbody: \n`, only `host`
surviving) holds bilaterally.

## PathConfigSource `.yaml`-extension constraint

Envoy's `PathConfigSource` infers the wire format from the file **extension**:
the EDS file must end in `.yaml`. The harness mounts it at a `.yaml` container
path precisely so the extension survives.

## Stats (L3)

`expected_stats` asserts the conditional per-cluster `cluster.eds_backend.update_*`
4-name subset that only registers when the cluster's `cluster_type == EDS` (the
§2.1 minimum-viable subset; STATIC/STRICT_DNS clusters emit no `update_*` names
in envoy-rust — L10), plus the data-plane witness and the HCM downstream
counters:

| stat | value |
|------|-------|
| `cluster.eds_backend.update_attempt`     | 1 |
| `cluster.eds_backend.update_success`     | 1 |
| `cluster.eds_backend.update_failure`     | 0 |
| `cluster.eds_backend.update_empty`       | 0 |
| `cluster.eds_backend.upstream_rq_total`  | 1 |
| `http.ingress_http.downstream_rq_total`  | 1 |
| `http.ingress_http.downstream_rq_2xx`    | 1 |

### L3 Envoy-only stat enumeration (not asserted)

**`membership_healthy` / `membership_total` are NOT asserted** — a verified
envoy-rust narrowing: `membership_healthy` registers only when `health_checks`
is configured (absent here), and `membership_total` does not exist in envoy-rust
at all. Envoy emits both for every cluster → allow-listed envoy-only (not
broadened — broadening would touch the existing "no membership_healthy gauge for
a plain cluster" inertness test and change existing-fixture stat output, out of
minimum-viable scope). Other Envoy-only / NOT asserted at this scope:
`update_no_rebuild`, `update_rejected` (structurally 0 in envoy-rust — L4
all-fatal, so it never registers as a tickable name), `update_time`,
`update_duration` (histogram), `membership_change`/`degraded`/`excluded`,
`assignment_*`, `version`/`version_text`, `warming_state`. The named-stat scrape
asserts only the seven lock-in names and does **no** set-diff.

## `/config_dump?include_eds` (L5)

The `EndpointsConfigDump` **diverges materially** from the other config-dump
sections (the ADR-0054 trigger):

1. **Envoy OMITS `EndpointsConfigDump` from the DEFAULT `/config_dump`** — only
   `/config_dump?include_eds` surfaces it. envoy-rust's admin STRIPS the query
   string (so `/config_dump?include_eds` routes to ConfigDump) and emits the
   section **unconditionally-when-EDS** (a narrowing vs Envoy's `?include_eds`
   gating). The fixture scrapes `/config_dump?include_eds`.
2. file-based EDS endpoints land under **`static_endpoint_configs[]`**, NOT
   `dynamic_endpoint_configs[]` (file-based EDS is "static" to Envoy).
3. shape: `{ "@type": ".../EndpointsConfigDump", "static_endpoint_configs": [ {
   "endpoint_config": { "@type": ".../ClusterLoadAssignment", "cluster_name":
   "eds_backend", "endpoints": [...], "policy": {...} } } ] }`.
4. **per-side `configs[]` index** (the EndpointsConfigDump is pushed after
   Clusters / before Listeners):
   - **Envoy** (`?include_eds` order): Bootstrap[0], Clusters[1],
     **Endpoints[2]**, Listeners[3], ScopedRoutes[4], Routes[5], Secrets[6]. The
     EndpointsConfigDump lands at `configs[2]`.
   - **envoy-rust**: Bootstrap[0], **Endpoints[1]** — no `cds_config` so no
     ClustersConfigDump (and no `lds_config` so no ListenersConfigDump). The
     EndpointsConfigDump lands at `configs[1]`.

The per-side index difference is handled by the Task 6 per-side `JsonSubtreeRule`
path override (`path_envoy: configs.2....` / `path_envoy_rust: configs.1....`,
REUSED from phase 20 — no new harness JSON code). Both sides agree the first
`static_endpoint_configs` entry's `endpoint_config.cluster_name == "eds_backend"`.

### C19 — BootstrapConfigDump shows the POPULATED `load_assignment`

The EDS pass mutates `load_assignment` in-place on the bootstrap. So
envoy-rust's `BootstrapConfigDump` for this static EDS cluster shows the
**populated** `load_assignment` (resolved endpoints) — a known minor divergence
vs Envoy, which shows the cluster as-configured (no resolved endpoints) in
BootstrapConfigDump. This is **NOT asserted**: the config_dump probe asserts only
the `EndpointsConfigDump` `cluster_name` subtree; the surrounding `configs` array
is `value_may_differ`. The `EndpointsConfigDump` is the faithful resolved-endpoints
surface.

## Negative paths (L4 — backstop-only)

The EDS negative-path divergences (missing file, malformed envelope, missing/
mismatched `ClusterLoadAssignment`, empty `resources: []`, the 6a/6b/6c
consistency cases) are covered by the in-process backstop test (Task 8), NOT by
this Docker-gated differential fixture. Envoy v1.33's disposition diverges per
case (warm-and-503 for content errors; hard-exit only for a missing FILE PATH),
and envoy-rust is **all-fatal** (the ADR-0049 decision-2 posture extended to
EDS) — neither observable as a clean data-plane response. Only the missing-file
path is fatal on BOTH; per ADR-0053 the negative paths are backstop-only.
