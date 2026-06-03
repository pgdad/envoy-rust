# Fixture 0026 — `xds-file-based-cds`

**Phase:** 18 (`18-xds-file-based-cds`; ADR-0048 SPEC / ADR-0049 PLAN).
**Differential surface:** file-based CDS — the `dynamic_resources.cds_config.
path_config_source.path` initial-load path. The xDS-family opener.

## What it exercises

This is the differential payoff of phase 18: it proves that envoy-rust's
file-based-CDS load (Tasks 1–5) and real Envoy v1.33.0 produce identical
observable behaviour given identical configs. ONE GET `/` over a single H1
keep-alive connection routes through cluster `dynamic_backend`, which exists
**only** because each proxy loaded its CDS file at boot:

| path | routed cluster | source of cluster | result |
|------|----------------|-------------------|--------|
| `/`  | `dynamic_backend` | the CDS file (NOT `static_resources`) | backend **200** |

### The data-plane discriminator (L6)

The route targets `dynamic_backend`, a cluster that appears **only** in the CDS
file — `static_resources` carries no `clusters:` key at all (L7). A proxy that
parsed the bootstrap but **ignored `dynamic_resources`** would have no
`dynamic_backend` cluster and would **503** the route. A successful **200**
therefore proves the CDS file was loaded and the cluster installed. The dynamic
cluster is structurally identical to fixture 0008's cluster shape (the same
STRICT_DNS/ROUND_ROBIN/V4_ONLY cluster moved into the CDS file and renamed). The
per-request `expected_body` in `expectations.yaml` asserts the echoed body
byte-exact on **each** side independently; combined with identical request
inputs (the `request_headers_to_remove` machinery strips the proxy-injected
headers so both sides forward the same request to the echo helper) this pins the
data-plane wire shape. The 200-vs-would-be-503 status is the CDS-load
discriminator.

## Topology

- **Listener:** one H1 HCM listener (`ingress_http1` stat_prefix), copied
  verbatim from fixture 0008's proven wire shape, with two route_config
  changes only: `validate_clusters: false` (L12b) and the route target
  `dynamic_backend`.
- **CDS:** `dynamic_resources.cds_config` with `resource_api_version: V3` and a
  `path_config_source.path` pointing at the rendered `cds.yaml`.
- **`cds.yaml`:** the bare `resources:` envelope (L1) with a single
  `@type`-tagged `…cluster.v3.Cluster` payload (`dynamic_backend`, STRICT_DNS,
  `lb_policy: ROUND_ROBIN`, `dns_lookup_family: V4_ONLY`).
- **Backend:** the http1-echo-server helper, spawned by the harness keyed on
  the `{{HTTP1_BACKEND_PORT}}` marker (which fixture 0026 places ONLY in
  `cds.yaml`; the harness's backend-launch detection scans the CDS template
  too). The upstream rendition is mounted into the Envoy container at
  `/etc/envoy-cds/cds.yaml`; the subject reads a host temp path.

## Envoy prerequisites (why `node:` + `validate_clusters: false` are present)

Both lock-ins were empirically verified against Envoy v1.33 (ADR-0049):

- **L12a — `node: { id, cluster }` is REQUIRED when CDS is configured.** Without
  it Envoy exits at startup. Hence the `node:` block with id
  `envoy-rust-phase-18-fixture-0026` / cluster `envoy-rust-phase-18`.
- **L12b — `validate_clusters: false` is REQUIRED on the route_config.** With
  CDS configured the route's `dynamic_backend` cluster is not present at
  config-load time; Envoy's default route-config validation would reject the
  bootstrap with "route: unknown cluster". Disabling it lets the route
  reference a CDS-supplied cluster.

## The CDS file envelope (L1) + `.yaml`-extension constraint

The CDS file is the bare `resources:` envelope — a top-level `resources:` list
of `@type`-tagged messages, each
`type.googleapis.com/envoy.config.cluster.v3.Cluster`. Envoy's `PathConfigSource`
infers the wire format from the file **extension**: the file must end in `.yaml`
(or `.json`/`.pb`/`.pb_text`); the harness mounts it at
`/etc/envoy-cds/cds.yaml` precisely so the extension survives.

## Stats (L3)

`expected_stats` asserts the six conditional `cluster_manager.*` names that only
register when `dynamic_resources` is configured, with values for a single
dynamic cluster + zero static clusters:

| stat | value |
|------|-------|
| `cluster_manager.cds.update_attempt`  | 1 |
| `cluster_manager.cds.update_success`  | 1 |
| `cluster_manager.cds.update_failure`  | 0 |
| `cluster_manager.cds.update_rejected` | 0 |
| `cluster_manager.cluster_added`       | 1 |
| `cluster_manager.active_clusters`     | 1 |

plus the data-plane `cluster.dynamic_backend.upstream_{rq,cx}_total` (1/1) and
the HCM `http.ingress_http1.downstream_rq_{total,2xx}` (1/1) counters.

### Envoy-only stat enumeration (not asserted)

Envoy v1.33 also emits ~12 other `cluster_manager.*` names that envoy-rust does
not emit at this scope (e.g. `cluster_manager.cds.version`,
`cluster_manager.cds.control_plane.{connected_state,pending_requests,
rq_total,…}`, `cluster_manager.warming_clusters`,
`cluster_manager.cluster_modified`, `cluster_manager.cluster_removed`,
`cluster_manager.cluster_updated`, `cluster_manager.cluster_updated_via_merge`,
`cluster_manager.update_merge_cancelled`, `cluster_manager.update_out_of_merge_
window`). The named-stat scrape asserts only the six lock-in #L3 names and does
no set-diff, so the Envoy-only family is ignored rather than allow-listed.

## `/config_dump` (L5)

The `admin_scrapes` `/config_dump` json_shape sub-case asserts the conditional
`ClustersConfigDump` lands at `configs[1]` on BOTH sides and its first
`dynamic_active_clusters` entry's `cluster.name == "dynamic_backend"`. The
`required_subtree` rule asserts both sides equal the expected value AND that the
two sides agree.

## Negative paths (L4 — backstop-only)

The negative-path CDS divergences (missing file, malformed envelope, unknown
`@type`, static/dynamic name collision, etc.) are covered by the in-process
backstop test (Task 8), NOT by this Docker-gated differential fixture: Envoy
v1.33 **exits the process** on a fatal CDS load error, which the differential
harness cannot observe as a data-plane response. Per ADR-0049 the negative
paths are backstop-only.
