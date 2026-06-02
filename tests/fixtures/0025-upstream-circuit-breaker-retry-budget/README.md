# Fixture 0025 — `upstream-circuit-breaker-retry-budget`

**Phase:** 17 (`17-circuit-breaker-budgets`; ADR-0046 SPEC / ADR-0047 PLAN).
**Differential surface:** cluster circuit-breaker budgets — the retry budget
(`max_retries`) and the request budget (`max_requests`), plus the
`track_remaining` gauge family, on an H1 listener.

## What it exercises

This is the differential payoff of phase 17: it proves that envoy-rust's budget
gates (Tasks 4/5) and real Envoy v1.33.0 produce identical observable behaviour
given identical configs. Three sequential GETs over one keep-alive connection
drive the three budget outcomes:

| # | path | cluster | budget | attempt 1 | retry | final | `x-envoy-attempt-count` | `x-envoy-overloaded` |
|---|------|---------|--------|-----------|-------|-------|--------------------------|----------------------|
| 1 | `/budget-blocked` | `budget_zero` | `max_retries: 0` | backend 503 `service unavailable\n` | **blocked by budget** | **503** `service unavailable\n` (verbatim, L6) | `1` | absent |
| 2 | `/budget-allowed` | `budget_default` | defaults (3/1024) | backend 503 `fail\n` | admitted -> 200 `ok\n` | **200** `ok\n` (L10) | `2` | absent |
| 3 | `/rq-blocked` | `rq_zero` | `max_requests: 0` | (rejected before connect) | — | **503** `...reset reason: overflow` (L2) | `1` | **present** |

The discriminating contrast is probe 1 vs probe 3: both are 503, but probe 1 is
a **real upstream 503** returned verbatim with the retry **budget-blocked** (no
`x-envoy-overloaded`, `x-envoy-attempt-count: 1`), while probe 3 is a **synth
local-reply 503** from the request-budget gate that rejects before any upstream
connect (`x-envoy-overloaded: true`, the byte-exact "...reset reason: overflow"
body). Probe 2 is the L10 control: the budget gate does **not** block a
within-cap retry, so the retry proceeds and recovers.

## Topology

One HTTP/1.1 HCM listener (`stat_prefix: ingress_http`) routes the three paths
to three single-endpoint `STRICT_DNS` clusters (`dns_lookup_family: V4_ONLY`,
L11 — the macOS Docker IPv6 trap) that **all point at the same backend
host:port**:

- `budget_zero`: `circuit_breakers.thresholds: [{priority: DEFAULT,
  max_retries: 0, track_remaining: true}]` + route `retry_policy: {retry_on:
  "5xx", num_retries: 1}`.
- `budget_default`: `circuit_breakers.thresholds: [{priority: DEFAULT,
  track_remaining: true}]` (no caps — Envoy's defaults 3 retries / 1024
  requests) + the same `retry_policy`.
- `rq_zero`: `circuit_breakers.thresholds: [{priority: DEFAULT,
  max_requests: 0}]`, no `retry_policy`, no `track_remaining`.

The virtual host sets `include_attempt_count_in_response: true` (L11) so the
final response on every path carries `x-envoy-attempt-count`.

The backend is the `health-aware-http1-backend` helper, started by the harness
(keyed on the fixture directory name) with:

```
--per-path /budget-blocked=503      # always 503 "service unavailable\n" (20 bytes)
--retry-script /budget-allowed=fail:1  # 503 "fail\n" first, then 200 "ok\n" (cyclic)
```

`/rq-blocked` needs no backend mapping — its request-budget gate rejects before
any upstream connect, so the backend is never contacted on that path.

### Standing cyclic retry-script caution (verbatim from fixture 0024)

The retry-script counter is a **single global per-path cyclic window** (fail:1
-> `503,200,503,200,...`), **not** source-IP keyed. macOS Docker Desktop NATs
every container -> host connection to source IP `127.0.0.1` — identical to
envoy-rust's source IP — so per-source keying is not viable (both proxies
collapse into one bucket). Cyclic windows are NAT-immune: the harness drives the
two proxies sequentially and each proxy's two upstream attempts for one
downstream request are consecutive, so each proxy's `/budget-allowed` retry pair
lands in its own fresh window and observes the same fail-then-succeed sequence
over the single shared host backend (Envoy-in-Docker via `host.docker.internal`,
envoy-rust via `127.0.0.1`).

> **Latent fragility:** the cyclic design RELIES on the harness driving the two
> proxies sequentially. If the keep-alive driver is ever refactored to drive
> them in parallel (e.g. `tokio::join`), the windows would interleave and this
> fixture would silently flake. Fixture 0025 reuses the same stateful backend as
> 0024, so the caution applies verbatim.

## Discriminating observable

Driver `http1_keep_alive` (per fixtures 0023/0024). One keep-alive connection
issues the three GETs; after a 200ms settle the cumulative per-cluster +
HCM-downstream counters/gauges are asserted bilaterally:

- `cluster.budget_zero.upstream_rq_retry_overflow == 1`, with the sibling retry
  counters (`upstream_rq_retry`, `_success`, `_limit_exceeded`) all `0` — the
  L1/L7 exclusivity: a budget-blocked retry ticks **only** `retry_overflow`, not
  the dispatch/outcome counters. `upstream_rq_total == 1` (one attempt only).
- `cluster.budget_default.upstream_rq_retry == 1` + `_success == 1` +
  `retry_overflow == 0`, `upstream_rq_total == 2` — the L10 within-cap control.
- `cluster.rq_zero.upstream_rq_pending_overflow == 1`,
  `upstream_rq_5xx == 1` (L3 / ADR-0047), `upstream_rq_total == 0`,
  `upstream_rq_retry_overflow == 0` (L9a exclusivity — the request-budget gate
  is not a retry-budget event).
- HCM downstream: `downstream_rq_2xx == 1`, `downstream_rq_5xx == 2`,
  `downstream_rq_total == 3`.

### Gauge re-anchoring (L3/L4, ADR-0047)

The `circuit_breakers.default.{rq_retry_open, rq_open}` gauges are **momentary**:
they reflect breaker-open state only while a request is actually in-flight at the
gate. Every post-settle sequential scrape happens with no in-flight request, so
all three breaker-open gauges read `0` — they are asserted at `0` precisely to
pin that momentary semantic (a non-zero read would mean a leaked/stuck guard).

The `remaining_retries` / `remaining_rq` gauges (emitted **only** when
`track_remaining: true`) are `cap - active`; at rest `active == 0`, so they read
the full cap: `budget_zero.remaining_retries == 0` (cap 0), and
`budget_default.{remaining_retries == 3, remaining_rq == 1024}` (Envoy's L5
defaults). These confirm the conditional gauge registration and the default-cap
resolution.

### `upstream_rq_5xx` on rq_zero (L3, ADR-0047)

Per ADR-0047 L3, the request-budget synth-overflow 503 is **class-counted** —
`cluster.rq_zero.upstream_rq_5xx` ticks alongside `upstream_rq_pending_overflow`,
even though no upstream response was ever received. Both proxies emit this, so it
is asserted bilaterally.

## Not asserted (intentional)

- **`cluster.rq_zero.upstream_cx_total`** — the L3/ADR-0047 **known divergence**.
  Envoy's connection pool **prefetches** a connection (`upstream_cx_total == 1`)
  for the cluster even though the request-budget gate rejects the request before
  dispatch; envoy-rust never connects (`upstream_cx_total == 0`). This is a
  pool-prefetch implementation difference, not a budget-semantics difference, so
  the stat is left **unasserted** rather than loosened — the budget behaviour
  itself (the overflow 503, the counters) is identical.
- **`cluster.budget_zero.upstream_rq_5xx`** — left unasserted to keep probe-1's
  set focused on the L7 retry-counter exclusivity (the verbatim upstream 503's
  per-class accounting is orthogonal to the budget-block being proven here).
- **`remaining_cx` / `remaining_pending` / `remaining_cx_pools`,
  `circuit_breakers.high.*`, `circuit_breakers.default.{cx_open, cx_pool_open,
  rq_pending_open}`** — Envoy-only at this scope; the `http1_keep_alive` stat
  path asserts only the named `expected_stats` (no full set-diff), so these are
  simply ignored (there is no `allowlist_envoy_only` key on the keep-alive
  driver — it belongs to the prometheus-set-diff `BodyRule`, and the `Driver`
  enum's `deny_unknown_fields` rejects it here; mirrors 0022/0023/0024).

## Reuse

- **17 H1 retry-budget gate (Task 4)** — `max_retries` enforcement +
  `upstream_rq_retry_overflow`.
- **17 H1 request-budget gate (Task 5)** — `max_requests` enforcement +
  `upstream_rq_pending_overflow` + the synth-overflow 503.
- **17 BudgetState + track_remaining gauges (Tasks 2/3)** — the conditional
  `remaining_retries` / `remaining_rq` registration.
- **16 H1 retry loop (Task 4) + retry counters (Task 3)** — the underlying
  `retry_on 5xx` / `num_retries` machinery the budgets gate.
- **16 stateful backend knobs (Task 6, amended)** — `--retry-script PATH=fail:N`
  + `--per-path PATH=STATUS`.
- **T6-extended keep-alive driver** + the 16 Task 7 `require_header_value` field.

## Docker-gated

The wrapper test (`tests/differential/tests/upstream_circuit_breaker_budgets.rs`)
runs under the differential harness, which self-skips when Docker
(`envoyproxy/envoy:v1.33.0`) is unavailable.
