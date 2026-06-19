# Fixture 0037 — `lb-maglev`

**Phase:** 29 (`29-lb-maglev`; ADR-0072 the §6.2-LOCKED Maglev algorithm).
**Differential surface:** MAGLEV consistent-hashing load balancing —
cross-proxy **identical backend selection per hash key**. The STRONG
differential target of the phase.

## What it proves

This proves — BILATERALLY (upstream `envoyproxy/envoy:v1.33.0` vs the subject
envoy-rust process) — that envoy-rust's MAGLEV LB selects the **SAME backend
as upstream Envoy for each hash key**. A single STATIC H1 listener routes `/`
to a STATIC `lb_policy: MAGLEV` cluster `maglev_cluster` with TWO
distinguishable echo backends. The route action carries a `hash_policy` keyed
on the `x-hash-key` request header, so the per-request hash is
`xxh64(x-hash-key value)` and the table lookup `table[hash % table_size]`
selects a backend. Each response body's leading `backend: <marker>\n` line
names which backend was selected (backends spawned `--body-marker backend_1` /
`--body-marker backend_2`).

The `Http1HashSweep` driver sweeps 16 distinct `x-hash-key` values against BOTH
proxies and asserts three properties:

| property | assertion |
|----------|-----------|
| **STRONG** (the core differential) | for EACH key, the marker chosen by envoy-rust is IDENTICAL to the one chosen by upstream Envoy — cross-proxy identical MAGLEV selection |
| **SPREAD** | over the full sweep, BOTH `backend_1` AND `backend_2` are selected on EACH side (the table actually distributes; a sweep that collapses to one backend fails) |
| **STABILITY** | each key is probed twice; a repeated key hits the SAME backend on each proxy (same-key → same-backend) |

## The §6.2-LOCKED algorithm (ADR-0072, validated 24/24 vs live Envoy)

xxHash64 per-host permutation from TWO seeds (`offset = xxh64(ip:port, seed 0)
% M`, `skip = xxh64(ip:port, seed 1) % (M-1) + 1`); table populated by the
standard Maglev permutation fill; lookup = `table[request_hash % M]` where
`request_hash = xxh64(x-hash-key header value)` and `M = table_size` (prime,
default 65537). envoy-rust already reproduces this (the Task-4 pinned oracle
passes 24/24); this fixture proves it end-to-end against the real Envoy.

## Sweep keys

The same 16 keys as fixture-0036 (RING_HASH): `key-0`..`key-5`, `user-alice`,
`user-bob`, `user-carol`, `1.2.3.4`, `10.0.0.1`, `session-abcdef`,
`session-123456`, `tenant-acme`, `tenant-globex`, `cart-99`. Unlike the §6.2
pinned oracle (fixed `ip:port`), the live table is built from the
runtime-ephemeral `{{BACKEND_IP}}:port` endpoints, so the per-key host mapping
is not predictable from the oracle — but a 2-host Maglev table is ~50/50
(near-perfect distribution at M=65537), so 16 keys reliably hit both backends
on both sides.

## Topology

- **Listener:** ONE STATIC H1 HCM listener (`ingress_http` stat_prefix) with an
  INLINE `route_config` routing `/` → `maglev_cluster`, the route action
  carrying `hash_policy: [{ header: { header_name: "x-hash-key" } }]`.
- **Cluster:** `maglev_cluster` — `type: STATIC`, `lb_policy: MAGLEV`, two
  `lb_endpoints` (backend_1 at `{{HTTP1_BACKEND_1_PORT}}`, backend_2 at
  `{{HTTP1_BACKEND_2_PORT}}`). A **PLAIN** cluster (the MVP): NO health check,
  NO outlier detection. `maglev_lb_config.table_size: 65537` is set explicitly
  (it equals the Envoy proto default).
- **No `dynamic_resources`** (a STATIC cluster).

## Config divergence

**None (behavioral).** `envoy.yaml` and `envoy-rust.yaml` are IDENTICAL config
except the listener bind address (`0.0.0.0` upstream container vs `127.0.0.1`
subject) — the standard harness per-side convention, not a behavioral
divergence.

### The shared `{{BACKEND_IP}}` — load-bearing for the STRONG target

The Maglev table is built from per-host `xxh64("{ip:port}", seed)` permutations
(ADR-0072), so cross-proxy identical selection holds **only if both proxies
build their table from the identical endpoint `ip:port` strings**. Unlike the
EDS fixtures (0029/0035), which render their endpoint IP per-side
(`{{EDS_BACKEND_IP}}` → host-gateway IP upstream / `127.0.0.1` subject), this
fixture renders `{{BACKEND_IP}}` to ONE SHARED address on BOTH sides: the
host's primary non-loopback LAN IPv4 (discovered route-based by the harness via
`discover_host_lan_ip`, no packets sent). The subject (a host process) reaches
the `0.0.0.0`-bound echo backends via this IP directly, and the upstream
container reaches the same host backends via the Docker bridge / Docker Desktop
VM NAT. A STATIC cluster rejects hostnames in its inline endpoints
(`malformed IP address` at boot), hence a numeric IP. The two echo-backend
ports (`{{HTTP1_BACKEND_1_PORT}}` / `{{HTTP1_BACKEND_2_PORT}}`) are the same on
both sides (one backend process each, shared). No `response_mutations` /
header-stripping knobs are needed because the driver extracts only the leading
`backend: <marker>` line, not the byte-exact body.

This is the project-memory invariant
`consistent-hash-lb-differential-needs-identical-endpoint-strings`: a per-side
IP split would defeat the STRONG target by making the two proxies build
different tables.

## LOCALLY observable (vs phases 26/27)

Unlike the file-based RDS/EDS hot-reload fixtures (0034/0035), this differential
is a **plain request/response with NO file-watch/reload trigger**, so it is
locally observable and the Docker test runs and is authoritative on any host
with a Docker daemon (no native-Linux-CI caveat).

Docker-gated by the differential harness at the cluster level (no per-test
`cfg` gate; the harness skips when `DOCKER_HOST` is unavailable).

## CI portability

Like fixture-0036, the upstream Envoy container reaches the host backends by
**bridge routing to the host LAN IP** (`{{BACKEND_IP}}` via
`discover_host_lan_ip`), NOT via the `host.docker.internal` /
`--add-host=host-gateway` mapping the other fixtures use. This is REQUIRED
because both proxies must build the Maglev table from an IDENTICAL endpoint
address string reachable from BOTH the host subject process AND the upstream
container, and the host LAN IP is the only address satisfying that on both the
Docker-Desktop (local) and Linux-CI topologies.

The authoritative §7.5 differential gate is the state-4 Linux CI run (cf. memory
"State-4 = CI's first real execution").

**Failure signature if the path is blocked on a runner:** all sweep keys return
upstream-side non-200 (503 / timeout) → surfaces as a STRONG / status assertion
failure. **First diagnostic:** check host-LAN-IP reachability from inside the
pinned `envoyproxy/envoy:v1.33.0` image.
