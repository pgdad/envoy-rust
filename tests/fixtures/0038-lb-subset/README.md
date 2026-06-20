# fixture 0038-lb-subset — subset LB route-selection differential (ADR-0074)

Phase 30, Task 7. The STRONG cross-proxy differential for **subset load
balancing**: a route's `metadata_match` narrows a cluster's endpoint set to the
subset whose endpoint `metadata` matches, and the request is balanced over that
subset only.

## Topology

ONE STATIC HTTP/1 HCM listener routes THREE distinct prefixes to ONE STATIC
cluster `subset_cluster`:

- `subset_cluster` uses **ROUND_ROBIN** as the within-subset balancer (stated
  explicitly: upstream Envoy defaults to it when omitted, but the envoy-rust
  parser requires `lb_policy` — stating it keeps both configs identical AND
  valid) and carries an `lb_subset_config`:
  ```yaml
  lb_subset_config:
    fallback_policy: NO_FALLBACK
    subset_selectors:
      - keys: [stage]
  ```
- Two distinguishable echo backends, each spawned by the harness with a fixed
  `--body-marker` (`backend_1` / `backend_2`); each response body's leading
  `backend: <marker>\n` line names which backend served it.
- Endpoint metadata (sibling of `endpoint`, under
  `metadata.filter_metadata.envoy.lb`):
  - **backend_1 = `{stage: prod, version: v2}`** — points at
    `{{HTTP1_BACKEND_1_PORT}}` (the harness spawns that port `--body-marker
    backend_1`).
  - **backend_2 = `{stage: canary, version: v1}`** — points at
    `{{HTTP1_BACKEND_2_PORT}}` (`--body-marker backend_2`).

## The §A regression oracle (the ground truth; live-Envoy confirmed)

Single selector `keys: [stage]`, `fallback_policy: NO_FALLBACK`:

| path     | route `metadata_match`     | result                                   |
|----------|----------------------------|------------------------------------------|
| `/prod`  | `{stage: prod}`            | backend_1 (marker `backend_1`)           |
| `/canary`| `{stage: canary}`          | backend_2 (marker `backend_2`)           |
| `/nope`  | `{stage: nonexistent}`     | **HTTP 503** body `no healthy upstream`  |

`/nope` resolves to NO subset; with `NO_FALLBACK` the cluster yields no host, so
the HCM returns the fixed 19-byte `no healthy upstream` local reply (503),
byte-identical cross-proxy.

The algorithm: build a per-selector index keyed by the ordered selector-key
values; a route's `metadata_match` resolves to the subset under
`{stage: <value>}`; balance (ROUND_ROBIN) over that subset's hosts only. A
`metadata_match` whose key/value tuple is absent from the index resolves to an
EMPTY set → NO_FALLBACK → 503.

## ip:port-independence

Unlike the maglev/ring_hash differentials (0036/0037), the subset LB selects on
**METADATA**, not on the endpoint `ip:port` string. The runtime-ephemeral
backend ports are therefore NOT the discriminator here — the **body marker** is.
We still render both endpoints from the SHARED `{{BACKEND_IP}}` (the host LAN
IPv4, identical on both sides) so the two proxies' endpoint sets are
byte-identical cross-proxy; a STATIC cluster also rejects hostnames, hence a
numeric IP.

## The driver

`Driver::Http1RouteSelect` (NOT the `Http1HashSweep` hash-sweep). For each
probe it drives `GET <path>` (Host: localhost, Connection: close) against BOTH
upstream Envoy and envoy-rust and asserts:

- BOTH sides return `expected_status`.
- 200 probes (`expected_marker: Some(m)`): extract the `backend: <marker>` body
  line from EACH side; assert `rust_marker == envoy_marker` (cross-proxy
  identical — **STRONG**) AND `envoy_marker == m` (the §A oracle).
- The 503 probe (`expected_marker: None`): assert each side's body equals
  `no healthy upstream` (the NO_FALLBACK fixed local-reply body).

## Observability / gating

LOCALLY observable — a plain request/response with NO file-watch/reload trigger,
so the Docker test (`tests/differential/tests/lb_subset.rs`) runs and is
authoritative on any host with a Docker daemon. Docker-gated at the cluster
level (the harness skips when `DOCKER_HOST` is unavailable).

## Deferral (ADR-0074)

envoy-rust emits no `cluster.subset_cluster.lb_subsets_*` stats (§A divergence
#2). This fixture does no admin stat scrape, so the divergence is inert here.
