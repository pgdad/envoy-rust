# Fixture 0027 — `xds-file-based-lds`

**Phase:** 19 (`19-xds-file-based-lds`; ADR-0050 SPEC / PLAN).
**Differential surface:** file-based LDS — the `dynamic_resources.lds_config.
path_config_source.path` initial-load path, composed with file-based CDS (the
phase-18 surface). The xDS-family continuation.

## What it exercises

This is the differential payoff of phase 19: it proves that envoy-rust's
file-based-LDS load (Tasks 1–6) and real Envoy v1.33.0 produce identical
observable behaviour given identical configs, end-to-end through the §5.7
LDS+CDS composition. The bootstrap carries **ZERO static listeners** (L2); the
H1 HCM listener exists **only** because each proxy loaded its LDS file at boot.
TWO GETs over ONE H1 keep-alive connection:

| path | routed cluster | cluster source | result |
|------|----------------|----------------|--------|
| `/static`  | `static_backend`  | `static_resources.clusters` (a STATIC cluster) | backend **200** |
| `/dynamic` | `dynamic_backend` | the CDS file (NOT `static_resources`)          | backend **200** |

Both clusters are STRICT_DNS / ROUND_ROBIN / V4_ONLY and resolve to the same
http1-echo-server helper; the probes differ only in the `host` header they send
(which the helper echoes back, making the byte-exact body assertion the
data-plane wire-shape pin).

### The three bilateral observables

1. **Data-plane connect + route through an LDS-only listener.** The listener
   exists only because of the LDS file. A proxy that parsed the bootstrap but
   **ignored `dynamic_resources.lds_config`** would have **no listener at all**
   — the data-plane connect would be refused. A successful **200** on either
   probe therefore proves the LDS file was loaded and the listener installed.
   Probe 1 (`/static` → a STATIC cluster) discriminates LDS-loaded from
   not-loaded **independently of CDS**; probe 2 (`/dynamic` → the CDS cluster)
   proves the §5.7 composition — a request whose listener AND cluster both
   exist only in dynamic-resource files.
2. **The LDS/CDS stat subsets** (see "Stats (L3)" below).
3. **The `ListenersConfigDump` + `/listeners` admin scrapes** (see
   "`/config_dump` + `/listeners` (L5)" below).

## Topology

- **Listener:** ONE H1 HCM listener (`ingress_http1` stat_prefix), delivered
  entirely by the LDS file. The route_config carries two routes: `/static` →
  `static_backend`, `/dynamic` → `dynamic_backend`.
- **LDS:** `dynamic_resources.lds_config` with `resource_api_version: V3` and a
  `path_config_source.path` pointing at the rendered LDS file.
- **CDS:** `dynamic_resources.cds_config` with `resource_api_version: V3` and a
  `path_config_source.path` pointing at the rendered `cds.yaml` (the
  fixture-0026 shape verbatim, cluster renamed `dynamic_backend`).
- **Static cluster:** `static_backend` in `static_resources.clusters` (keeps
  probe 1 orthogonal to CDS).
- **Backend:** the http1-echo-server helper, spawned by the harness keyed on
  the `{{HTTP1_BACKEND_PORT}}` marker. The marker appears in the main config
  (`static_backend`), the LDS file, and `cds.yaml`; the harness's
  backend-launch scan covers all four scanned renditions (both per-side main
  configs, the LDS file, and `cds.yaml`) (Task 6 — same carryforward lesson as
  the phase-18 CDS scan extension), so a single backend is spawned and all
  occurrences resolve to it.

## Per-side LDS templates (Correction 5)

Like the per-side **main** configs of fixtures 0008/0026, the LDS file is
delivered as TWO per-side templates rendered through each side's kv map:

- **`lds-envoy.yaml`** (upstream): carries the Envoy-only HCM fields
  `generate_request_id: false` + `request_headers_to_remove` (stripping the
  `x-forwarded-*` / `x-request-id` / `x-envoy-*` headers Envoy v1.33 injects on
  the upstream-bound request). Binds `0.0.0.0:{{PORT}}`. The harness renders it,
  mounts the rendition into the Envoy container at a `.yaml` container path
  (`upstream::LDS_CONTAINER_PATH`), and substitutes `{{LDS_PATH}}` in the main
  config with that container path.
- **`lds-envoy-rust.yaml`** (subject): the same listener **without** the
  Envoy-only HCM fields (envoy-rust's `deny_unknown_fields` parser rejects
  them; envoy-rust injects none of those headers, so omission keeps the echoed
  bodies byte-equal). Binds `127.0.0.1:{{PORT}}` and stays at its host temp
  path.

This intentional per-side field-set divergence is exactly the
fixtures-0008/0026 pattern: the stripping/suppression on the Envoy side is what
makes both proxies forward the **same** request to the echo helper, so the
byte-exact body assertion holds bilaterally.

## L6 — no `validate_clusters`

Unlike fixture 0026's STATIC listener (whose route_config carried
`validate_clusters: false` to let an Envoy-side STATIC route reference a
not-yet-loaded CDS cluster), **LDS-delivered route_configs skip Envoy's inline
cluster validation entirely** (verified §6.2 item 6). The LDS file therefore
carries NO `validate_clusters` field at all, on either side.

## PathConfigSource `.yaml`-extension constraint

Envoy's `PathConfigSource` infers the wire format from the file **extension**:
both the LDS and CDS files must end in `.yaml`. The harness mounts each at a
`.yaml` container path precisely so the extension survives.

## Stats (L3)

`expected_stats` asserts the conditional `listener_manager.lds.*` family that
only registers when `dynamic_resources.lds_config` is configured, plus
`listener_added` / `total_listeners_active`, alongside the fixture-0026 CDS
family (note `cluster_added` / `active_clusters` are **2** here — the static
`static_backend` PLUS the CDS `dynamic_backend`):

| stat | value |
|------|-------|
| `listener_manager.lds.update_attempt`     | 1 |
| `listener_manager.lds.update_success`     | 1 |
| `listener_manager.lds.update_failure`     | 0 |
| `listener_manager.lds.update_rejected`    | 0 |
| `listener_manager.listener_added`         | 1 |
| `listener_manager.total_listeners_active` | 1 |
| `cluster_manager.cds.update_attempt`      | 1 |
| `cluster_manager.cds.update_success`      | 1 |
| `cluster_manager.cds.update_failure`      | 0 |
| `cluster_manager.cds.update_rejected`     | 0 |
| `cluster_manager.cluster_added`           | 2 |
| `cluster_manager.active_clusters`         | 2 |

plus the per-cluster `cluster.{static,dynamic}_backend.upstream_{rq,cx}_total`
(1/1 each) and the HCM `http.ingress_http1.downstream_rq_{total,2xx}` (2/2)
counters.

### Envoy-only stat enumeration (not asserted)

Envoy v1.33 emits a large `listener_manager.*` family that envoy-rust does not
emit at this scope. Most notably the **PER-WORKER**
`listener_manager.<worker>.listener_create_success` counters (one per worker
thread), plus `listener_manager.listener_in_place_*`,
`.listener_modified`, `.listener_removed`, `.listener_stopped`,
`.lds.version`, `.lds.control_plane.{connected_state,pending_requests,
rq_total,…}`, and `.total_listeners_{warming,draining}`. The named-stat scrape
asserts only the six lock-in #L3 names and does **no** set-diff, so the
Envoy-only family is ignored rather than allow-listed (the same disposition
fixture 0026 takes for the Envoy-only `cluster_manager.*` family). The
per-worker `listener_create_success` exclusion is the most prominent: envoy-rust
does not model per-worker listener creation, so those names have no analogue.

## `/config_dump` + `/listeners` (L5)

With BOTH `lds_config` AND `cds_config` configured, the config_dump order is
`Bootstrap[0]`, `Clusters[1]`, `Listeners[2]`:

- The first `/config_dump` sub-case asserts the conditional
  `ListenersConfigDump` lands at `configs[2]` on BOTH sides and its first
  `dynamic_listeners` entry's `name == "dynamic_listener"`.
- The second `/config_dump` sub-case re-checks fixture-0026 compatibility: the
  `ClustersConfigDump` still lands at `configs[1]` and its first
  `dynamic_active_clusters` entry's `cluster.name == "dynamic_backend"`.
- The `/listeners` sub-case asserts each side emits exactly one
  `<name>::<address>:<port>` line, with the `dynamic_listener::` prefix
  required on both and the per-side `0.0.0.0:` / `127.0.0.1:` address shapes in
  the per-side prefix allow-lists.

## Negative paths (L4 — backstop-only)

The negative-path LDS divergences (missing file, malformed envelope, unknown
`@type`, etc.) are covered by the in-process backstop test (Task 8), NOT by this
Docker-gated differential fixture: Envoy v1.33 **exits the process** on a fatal
LDS load error, which the differential harness cannot observe as a data-plane
response. Per ADR-0050 the negative paths are backstop-only.
