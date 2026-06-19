# Fixture 0036 — `lb-ring-hash`

**Phase:** 28 (`28-lb-ring-hash`; ADR-0070 the locked algorithm).
**Differential surface:** RING_HASH consistent-hashing load balancing —
cross-proxy **identical backend selection per hash key**. The STRONG
differential target of the phase.

## What it proves

This proves — BILATERALLY (upstream `envoyproxy/envoy:v1.33.0` vs the subject
envoy-rust process) — that envoy-rust's RING_HASH LB selects the **SAME backend
as upstream Envoy for each hash key**. A single STATIC H1 listener routes `/`
to a STATIC `lb_policy: RING_HASH` cluster `ring_cluster` with TWO
distinguishable echo backends. The route action carries a `hash_policy` keyed
on the `x-hash-key` request header, so the per-request hash is
`xxh64(x-hash-key value)` and the ring lookup selects a backend. Each response
body's leading `backend: <marker>\n` line names which backend was selected
(backends spawned `--body-marker backend_1` / `--body-marker backend_2`).

The `Http1HashSweep` driver sweeps 16 distinct `x-hash-key` values against BOTH
proxies and asserts three properties:

| property | assertion |
|----------|-----------|
| **STRONG** (the core differential) | for EACH key, the marker chosen by envoy-rust is IDENTICAL to the one chosen by upstream Envoy — cross-proxy identical RING_HASH selection |
| **SPREAD** | over the full sweep, BOTH `backend_1` AND `backend_2` are selected on EACH side (the ring actually distributes; a sweep that collapses to one backend fails) |
| **STABILITY** | each key is probed twice; a repeated key hits the SAME backend on each proxy (same-key → same-backend) |

## The locked algorithm (ADR-0070, validated 36/36 vs live Envoy)

xxHash64 seed 0; per-host ring key `"{ip:port}_{i}"` (the `_` separator is
load-bearing); replicas per host = `minimum_ring_size / num_hosts`; sorted ring;
request hash = `xxh64(x-hash-key header value)`; first-entry-≥ lookup with wrap.
envoy-rust already reproduces this (the Task-5 oracle passes); this fixture
proves it end-to-end against the real Envoy.

## Sweep keys

Includes the §6.2 oracle keys (`key-0`, `key-2`, `user-alice`, `1.2.3.4`) plus
others chosen for spread: `key-1`, `key-3`, `key-4`, `key-5`, `user-bob`,
`user-carol`, `10.0.0.1`, `session-abcdef`, `session-123456`, `tenant-acme`,
`tenant-globex`, `cart-99`. The §6.2 recon saw a ~14/13 split over 27 keys, so
16 keys reliably hit both backends on both sides.

## Topology

- **Listener:** ONE STATIC H1 HCM listener (`ingress_http` stat_prefix) with an
  INLINE `route_config` routing `/` → `ring_cluster`, the route action carrying
  `hash_policy: [{ header: { header_name: "x-hash-key" } }]`.
- **Cluster:** `ring_cluster` — `type: STATIC`, `lb_policy: RING_HASH`, two
  `lb_endpoints` (backend_1 at `{{HTTP1_BACKEND_1_PORT}}`, backend_2 at
  `{{HTTP1_BACKEND_2_PORT}}`). A **PLAIN** cluster (the MVP): NO health check,
  NO outlier detection. `ring_hash_lb_config.minimum_ring_size: 1024` is set
  explicitly (it equals the default; 1024 / 2 hosts = 512 replicas/host).
- **No `dynamic_resources`** (a STATIC cluster).

## Config divergence

**None (behavioral).** `envoy.yaml` and `envoy-rust.yaml` are IDENTICAL config
except the listener bind address (`0.0.0.0` upstream container vs `127.0.0.1`
subject) — the standard harness per-side convention, not a behavioral
divergence.

### The shared `{{BACKEND_IP}}` — load-bearing for the STRONG target

The RING_HASH ring key is `xxh64("{ip:port}_{i}")` (ADR-0070), so cross-proxy
identical selection holds **only if both proxies build their ring from the
identical endpoint `ip:port` strings**. Unlike the EDS fixtures (0029/0035),
which render their endpoint IP per-side (`{{EDS_BACKEND_IP}}` → host-gateway IP
upstream / `127.0.0.1` subject), this fixture renders `{{BACKEND_IP}}` to ONE
SHARED address on BOTH sides: the host's primary non-loopback LAN IPv4
(discovered route-based by the harness, no packets sent). The subject (a host
process) reaches the `0.0.0.0`-bound echo backends via this IP directly, and the
upstream container reaches the same host backends via the Docker bridge / Docker
Desktop VM NAT (verified reachable from both). A STATIC cluster rejects
hostnames in its inline endpoints (`malformed IP address` at boot), hence a
numeric IP. The two echo-backend ports (`{{HTTP1_BACKEND_1_PORT}}` /
`{{HTTP1_BACKEND_2_PORT}}`) are the same on both sides (one backend process
each, shared). No `response_mutations` / header-stripping knobs are needed
because the driver extracts only the leading `backend: <marker>` line, not the
byte-exact body.

## LOCALLY observable (vs phases 26/27)

Unlike the file-based RDS/EDS hot-reload fixtures (0034/0035), this differential
is a **plain request/response with NO file-watch/reload trigger**, so it is
locally observable and the Docker test runs and is authoritative on any host
with a Docker daemon (no native-Linux-CI caveat).

Docker-gated by the differential harness at the cluster level (no per-test
`cfg` gate; the harness skips when `DOCKER_HOST` is unavailable).
