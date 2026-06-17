# Fixture 0034 — `xds-rds-hot-reload`

**Phase:** 26 (`26-xds-rds-hot-reload`; ADR-0065 SPEC / ADR-0066 PLAN).
**Differential surface:** file-based RDS **HOT-RELOAD** — the watched
`rds.config_source.path_config_source.path` is atomically rewritten at runtime,
and each proxy re-reads it and hot-swaps the route table. The runtime
continuation of phase 20's RDS *initial-load* surface (fixture 0028).

## What it exercises

This proves — BILATERALLY (upstream `envoyproxy/envoy:v1.33.0` vs the subject
envoy-rust process) — that a file-based RDS route table is **hot-reloaded** when
its file is atomically rewritten. The HCM carries **NO inline `route_config`**;
the route table exists only because each proxy loaded its RDS file at boot, and
it *changes* only because each proxy re-reads the file after an atomic-rename
rewrite.

The driver (`Driver::Http1RdsReload`) runs a THREE-PHASE bilateral sequence:

| phase | action | observable |
|-------|--------|------------|
| **pre** | `GET /probe` (before reload) | `200` body `rds-v1\n` (initial table) |
| **reload** | atomic-rename `rds-reload.yaml` over the watched RDS path on BOTH sides, then poll the discriminator until convergence | discriminator `GET /probe` returns `rds-v2\n` within `settle_budget_ms` |
| **post** | `GET /probe` (after convergence) | `200` body `rds-v2\n` (NEW table) |

The SAME path (`/probe`) returns a DIFFERENT body after the rewrite — the clean
bilateral proof that the route table was re-read and atomically swapped.

## Why `direct_response` instead of clusters/backends

The single observable is a route's **`direct_response` body** (`rds-v1` →
`rds-v2`), **not** a routed cluster. Rationale:

- The harness spawns a **single echo backend** and the `Http1RdsReload`
  discriminator converges on **status/body** — so two clusters could not be
  distinguished in a response. A `direct_response` body change IS a genuine
  route-table reload.
- It is **byte-exact and identical on both sides** with **zero upstream-header
  noise**: there is no upstream and no proxy-injected upstream headers, so
  (unlike fixture 0028) NO header-stripping knobs are needed in `envoy.yaml`.
  The bodies are byte-identical bilaterally natively.

This fixture therefore needs **NO clusters and NO backend**
(`static_resources.clusters: []` on both sides).

The cluster-routing / stat-counter / `config_dump` reload proofs (e.g. the
`rds.local_route.config_reload` counter ticking `1 → 2`, the new
`route_config.version`, the cluster a reloaded route now targets) live in the
separate **in-process backstop**
`crates/envoy-bin/tests/xds_rds_hot_reload.rs`, NOT here — those are
process-internal observables the Docker differential cannot see as a data-plane
response.

## Topology

- **Listener:** ONE STATIC H1 HCM listener (`ingress_http1` stat_prefix) in
  `static_resources.listeners`. Its HCM has NO inline `route_config`; instead it
  carries `rds: { route_config_name: local_route, config_source: {
  resource_api_version: V3, path_config_source: { path: {{RDS_PATH}} } } }`.
- **RDS (initial):** the SHARED `rds.yaml` — one `RouteConfiguration` named
  `local_route`, vh `domains: ["*"]`, one route `match: { prefix: "/probe" }` →
  `direct_response: { status: 200, body: { inline_string: "rds-v1\n" } }`.
- **RDS (post-reload):** the SHARED `rds-reload.yaml` — IDENTICAL shape, same
  `local_route` / `/probe`, but `direct_response` body `rds-v2\n`. The harness
  atomic-renames it over the watched `{{RDS_PATH}}` on both sides.
- **No clusters, no backend.**

## The SHARED RDS files

As in fixture 0028, the RDS file is delivered as a **SINGLE SHARED** template
(here TWO: `rds.yaml` initial + `rds-reload.yaml` post-reload), each rendered
per-side through that side's kv map. Both therefore carry **NO Envoy-only
fields**: envoy-rust's `RouteConfiguration` / `VirtualHost` use
`deny_unknown_fields`. Because the observable is a pure `direct_response` (no
upstream), neither side needs any header-stripping — the difference from
fixture 0028, which proxied to an echo backend and had to strip proxy-injected
upstream headers in its MAIN config.

## The atomic-rename reload (ADR-0066)

The harness rewrites the watched RDS file via a same-dir temp-sibling +
`rename` (an atomic swap on the same filesystem) — the ONLY rewrite that
triggers Envoy's **default file-watch** (`MovedTo`/`Create` inotify events; an
in-place truncate+write does NOT reliably fire the watch). Each side then
re-reads the file and hot-swaps the route table. The discriminator probe is
polled (each side independently) until it reflects the new table
(`rds-v2\n`), bounded by `settle_budget_ms: 5000` (generous slack over the
~50 ms settle latency).

## PathConfigSource `.yaml`-extension constraint

Envoy's `PathConfigSource` infers the wire format from the file **extension**:
the watched RDS file must end in `.yaml`. The harness mounts it at a `.yaml`
container path precisely so the extension survives the atomic rename.

## NATIVE-LINUX-CI-AUTHORITATIVE (critical)

This differential is **NOT observable under macOS / Docker-Desktop virtiofs**:
the host bind-mount inotify does **not** propagate into the container, so the
upstream Envoy never sees the reload locally (the file changes on the host, but
no inotify event reaches the containerized Envoy's file-watch). The differential
therefore runs and is authoritative **only on a native-Linux CI runner** with a
real Docker daemon over a native filesystem.

- **Local verification** = the in-process backstop
  `crates/envoy-bin/tests/xds_rds_hot_reload.rs` (runs without Docker; exercises
  the subject's own file-watch + reload pipeline directly, including the
  cluster/counter/config_dump observables this fixture omits).
- **Subject-side config validity** is also locally checkable by booting
  `envoy-bin` against the rendered `envoy-rust.yaml` + a real `rds.yaml` (see
  the Task 8 verification notes).

Docker-gated by the differential harness at the cluster level (no per-test
`cfg` gate; the harness skips when `DOCKER_HOST` is unavailable).
