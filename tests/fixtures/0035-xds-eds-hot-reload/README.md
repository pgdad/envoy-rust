# Fixture 0035 — `xds-eds-hot-reload`

**Phase:** 27 (`27-xds-eds-hot-reload`; ADR-0067 SPEC / ADR-0068 PLAN).
**Differential surface:** file-based EDS endpoint **HOT-RELOAD** — the watched
`eds_cluster_config.eds_config.path_config_source.path` is atomically rewritten
at runtime, and each proxy re-reads it and hot-swaps the cluster's endpoint set.
The runtime continuation of phase-21's EDS *initial-load* surface (fixture
0029), mirroring how fixture 0034 (RDS hot-reload) continued fixture 0028.

## What it exercises

This proves — BILATERALLY (upstream `envoyproxy/envoy:v1.33.0` vs the subject
envoy-rust process) — that a file-based EDS endpoint set is **hot-reloaded** when
its file is atomically rewritten. The cluster `eds_backend` carries **NO inline
`load_assignment`**; its endpoint exists only because each proxy loaded its EDS
file at boot, and it *changes* only because each proxy re-reads the file after an
atomic-rename rewrite (`eds.yaml` → `eds-reload.yaml`).

The driver (`Driver::Http1EdsReload`) runs a THREE-PHASE bilateral sequence over
TWO distinguishable single-endpoint `http1-echo-server` backends (each spawned
with a distinct `--body-marker`, so a `GET /probe` response's leading
`backend: <marker>\n` line identifies WHICH backend served it):

| phase | action | observable |
|-------|--------|------------|
| **pre** | `GET /probe` (before reload) | `200` body `backend: backend_1\n…` (initial EDS endpoint) |
| **reload** | atomic-rename `eds-reload.yaml` over the watched EDS path on BOTH sides (endpoint swapped `backend_1` → `backend_2`), then poll the discriminator until convergence | discriminator `GET /probe` body marker flips to `backend: backend_2\n…` within `settle_budget_ms` |
| **post** | `GET /probe` (after convergence) | `200` body `backend: backend_2\n…` (SWAPPED EDS endpoint) |

The SAME path (`/probe`) returns a DIFFERENT body after the rewrite — the clean
bilateral proof that the endpoint set was re-read and atomically swapped.

## Topology

- **Listener:** ONE STATIC H1 HCM listener (`ingress_http` stat_prefix) with an
  INLINE `route_config` routing `/probe` → `eds_backend`.
- **Cluster:** `eds_backend` — `type: EDS`, `lb_policy: ROUND_ROBIN`, NO inline
  `load_assignment`, NO `health_checks`, NO `outlier_detection`. A **PLAIN** EDS
  cluster is the only kind that gets a reload watcher (§0 finding 3 /
  Decision-5).
- **EDS (initial):** the SHARED `eds.yaml` — one `ClusterLoadAssignment` named
  `eds_backend`, one endpoint at `{{EDS_BACKEND_IP}}:{{HTTP1_BACKEND_1_PORT}}`.
- **EDS (post-reload):** the SHARED `eds-reload.yaml` — IDENTICAL envelope/CLA
  shape, but the endpoint points at `{{HTTP1_BACKEND_2_PORT}}`. The harness
  atomic-renames it over the watched `{{EDS_PATH}}` on both sides.
- **No `dynamic_resources`** (the cluster is STATIC-but-EDS, NOT CDS-delivered).

## NUMERIC per-side IP rendering (reused from 0029)

File-based EDS **rejects hostnames** (`malformed IP address` →
`update_rejected`), so the endpoint `socket_address.address` must be a numeric
IP — and that IP differs per side. The SHARED `eds.yaml` / `eds-reload.yaml` use
the `{{EDS_BACKEND_IP}}` marker: the harness renders it to the runtime-discovered
numeric host-gateway IP on the upstream (Envoy) side and `127.0.0.1` on the
subject (envoy-rust) side. `{{HTTP1_BACKEND_1_PORT}}` / `{{HTTP1_BACKEND_2_PORT}}`
resolve to the two echo-server ports per side.

### Why `eds.yaml` references `{{HTTP1_BACKEND_2_PORT}}` in a comment

The harness scans ONLY `eds.yaml` (NOT `eds-reload.yaml`) when deciding which
echo backends to spawn and which port keys to put in the per-side kv map. The
post-reload endpoint lives in `eds-reload.yaml` as `{{HTTP1_BACKEND_2_PORT}}`, so
`eds.yaml` ALSO references that marker (in a comment only — the live initial
endpoint is backend_1). Otherwise backend_2 would never spawn and the reload
render would bail on the unsubstituted token.

## The atomic-rename reload (ADR-0066 carryforward)

The harness rewrites the watched EDS file via a same-dir temp-sibling +
`rename` (an atomic swap on the same filesystem). Each side then re-reads the
file and hot-swaps the endpoint set. The discriminator probe is polled (each
side independently) until it reflects the new backend (`backend: backend_2`),
bounded by `settle_budget_ms: 5000`.

## Why no header-stripping `response_mutations` (vs 0029)

The discriminator/probes assert the **byte-exact echoed body** but NOT
`x-envoy-upstream-service-time` presence (unlike 0029's `Http1KeepAlive`
`require_header_present`), so `envoy.yaml` keeps the request-side stripping
(`generate_request_id: false` + the header_mutation `request_mutations` +
`suppress_envoy_headers: true`) that makes the echoed body byte-identical
bilaterally, but omits the `response_mutations` re-add.

## What lives in the BACKSTOP, not here (critical)

The `Http1EdsReload` driver (schema-locked at Task 6) expresses exactly ONE
reload (pre → reload → post). The process-internal observables and the full
bad-reload taxonomy therefore live in the in-process backstop
`crates/envoy-bin/tests/xds_eds_hot_reload.rs`, NOT here:

- the §6.2-LOCKED counter taxonomy
  (`cluster.eds_backend.update_{attempt,success,failure,rejected,empty}`
  `1/1/0/0/0` → `2/2/0/0/0` on a happy reload);
- the `/config_dump?include_eds` `EndpointsConfigDump` reflecting the new
  endpoint;
- the bad-reload classes V4(a)–(e) incl. **apply-empty → 503 "no healthy
  upstream"** and empty-envelope → `update_empty` / last-good kept;
- in-flight isolation (a request that picked an endpoint completes against it
  across a reload) and cursor-bounds on a SHRINKING (2→1) endpoint set.

The malformed/bad-reload **bilateral** class (V4(a) — both proxies keep
last-good) cannot be expressed in the single-reload `Http1EdsReload` driver, so
it too is asserted in the backstop (which boots the real `envoy-bin` and drives
a malformed reload directly).

## NATIVE-LINUX-CI-AUTHORITATIVE (critical)

This differential is **NOT observable under macOS / Docker-Desktop virtiofs**:
the host bind-mount inotify does **not** propagate into the container, so the
upstream Envoy never sees the reload locally (the file changes on the host, but
no inotify event reaches the containerized Envoy's file-watch). The differential
therefore runs and is authoritative **only on a native-Linux CI runner** with a
real Docker daemon over a native filesystem.

- **Local verification** = the in-process backstop
  `crates/envoy-bin/tests/xds_eds_hot_reload.rs` (runs without Docker; exercises
  the subject's own file-watch + EDS reload pipeline directly, including the
  counter / config_dump / bad-reload observables this fixture omits).

Docker-gated by the differential harness at the cluster level (no per-test
`cfg` gate; the harness skips when `DOCKER_HOST` is unavailable).
