# Fixture 0024 — `upstream-retry-on-5xx`

**Phase:** 16 (`16-http-retries`; ADR-0044 SPEC / ADR-0045 PLAN).
**Differential surface:** HTTP route-level retry policy (`retry_on: "5xx"`,
`num_retries: 1`) on an H1 listener — the success path (retry recovers) and the
limit-exceeded path (retry consumed, last 503 returned verbatim).

## What it exercises

This is the differential payoff of phase 16: it proves that envoy-rust's H1
retry loop (Task 4) and real Envoy v1.33.0 produce identical retry behaviour
given identical configs. Two sequential GETs over one keep-alive connection
drive the two retry outcomes of `retry_policy: {retry_on: "5xx", num_retries: 1}`:

| # | path | attempt 1 | attempt 2 (retry) | final | `x-envoy-attempt-count` |
|---|------|-----------|-------------------|-------|--------------------------|
| 1 | `/retry-success` | backend 503 `fail\n` | backend 200 `ok\n` | **200** `ok\n` | `2` |
| 2 | `/retry-exhausted` | backend 503 `service unavailable\n` | backend 503 `service unavailable\n` | **503** `service unavailable\n` (last 503 verbatim, L9) | `2` |

## Topology

One HTTP/1.1 HCM listener (`stat_prefix: ingress_http`) routes `/retry-success`
and `/retry-exhausted` to a single-endpoint `STRICT_DNS` cluster `backend`
(`dns_lookup_family: V4_ONLY`, L11 — the macOS Docker IPv6 trap). Both routes
carry `retry_policy: {retry_on: "5xx", num_retries: 1}`; the virtual host sets
`include_attempt_count_in_response: true` (L6) so the final response on both
paths carries `x-envoy-attempt-count: 2`.

The backend is the `health-aware-http1-backend` helper, started by the harness
(keyed on the fixture directory name) with:

```
--retry-script /retry-success=fail:1   # 503 "fail\n" for the first request, then 200 "ok\n"
--per-path /retry-exhausted=503        # always 503 "service unavailable\n" (20 bytes)
```

The retry-script counter is a **single global per-path cyclic window** (fail:1
-> `503,200,503,200,...`), **not** source-IP keyed. macOS Docker Desktop NATs
every container -> host connection to source IP `127.0.0.1` — identical to
envoy-rust's source IP — so per-source keying is not viable (both proxies
collapse into one bucket). Cyclic windows are NAT-immune: the harness drives
the two proxies sequentially and each proxy's two upstream attempts for one
downstream request are consecutive, so each proxy's retry pair lands in its own
fresh window and observes the same fail-then-succeed sequence over the single
shared host backend (Envoy-in-Docker via `host.docker.internal`, envoy-rust via
`127.0.0.1`). This corrects the per-source design that ADR-0045 (Task 6 review)
flagged as residual risk and that the first live run falsified.

> **Latent fragility:** the cyclic design RELIES on the harness driving the two
> proxies sequentially. If the keep-alive driver is ever refactored to drive
> them in parallel (e.g. `tokio::join`), the windows would interleave and this
> fixture would silently flake.

## Discriminating observable

Driver `http1_keep_alive` (13.1 D10; extended at 14.2 Task 6 with per-request
`expected_body` / `require_header_present`; extended at 16 Task 7 with
`require_header_value` for the value-exact `x-envoy-attempt-count: 2` check).
One keep-alive connection issues the two GETs; after a 200ms settle the
cumulative retry + per-attempt counters are asserted bilaterally:

- `cluster.backend.upstream_rq_retry == 2` (one retry per probe)
- `cluster.backend.upstream_rq_retry_success == 1` (probe 1)
- `cluster.backend.upstream_rq_retry_limit_exceeded == 1` (probe 2)
- `cluster.backend.upstream_rq_total == 4` (2 attempts × 2 probes; per-attempt, L5)
- `cluster.backend.upstream_rq_5xx == 1` (completing-only — only probe 2's final 503, L5)
- `http.ingress_http.downstream_rq_2xx == 1`
- `http.ingress_http.downstream_rq_5xx == 1`
- `http.ingress_http.downstream_rq_total == 2`

`upstream_rq_total == 4` (not 2) and `upstream_rq_5xx == 1` (not 3) are the L5
discriminators: Envoy counts `upstream_rq_total` **per attempt** but
`upstream_rq_5xx` only on the **completing** response (so the recovered probe 1
and the intermediate 503 of each probe do not tick the per-class 5xx counter).

## Reuse

- **16 H1 retry loop (Task 4)** — the per-attempt counting, back-off, and
  `x-envoy-attempt-count` header under test.
- **16 retry counters (Task 3)** — `upstream_rq_retry{,_success,_limit_exceeded}`.
- **16 stateful backend knob (Task 6, amended)** — `--retry-script PATH=fail:N`
  (the cyclic-window fail-then-succeed sequence, single global per-path counter)
  alongside the stateless `--per-path`.
- **T6-extended keep-alive driver** + the 16 Task 7 `require_header_value`
  field (value-exact header assertion).

## Deferred / Envoy-only stat names

The L10 Envoy-only retry stat family (`upstream_rq_retry_overflow`,
`upstream_rq_retry_backoff_exponential`, `upstream_rq_retry_backoff_ratelimited`,
`retry_or_shadow_abandoned`, `circuit_breakers.{default,high}.rq_retry_open`,
and the `cluster.backend.retry.upstream_rq_{503,5xx,completed}` sub-scope) is
emitted by Envoy but not by envoy-rust at this scope. The `http1_keep_alive`
stat path asserts only the named `expected_stats` (no full set-diff), so these
unasserted names are simply ignored — there is no `allowlist_envoy_only` key on
the keep-alive driver (it belongs to the prometheus-set-diff `BodyRule`, and
the `Driver` enum's `deny_unknown_fields` rejects it here). The deferred set is
catalogued in `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (Task 10).

## Docker-gated

The wrapper test (`tests/differential/tests/upstream_retry.rs`) runs under the
differential harness, which self-skips when Docker
(`envoyproxy/envoy:v1.33.0`) is unavailable.
