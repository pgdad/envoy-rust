# 0096 — `envoy.filters.http.health_check` stats

Phase **115** (`ADR-0201`). The stat-surface sibling of `0095`, on the existing
`Driver::AdminScrape`: three HTTP/1.1 pre-requests (`GET /healthz`, `GET /other`, `GET /healthz`)
against a backend-free, cluster-free HCM listener whose chain is `[health_check (:path exact
/healthz), router]`, then a bilateral ABSOLUTE stat assertion on both admin listeners.

## What it witnesses

| stat | value | rule (MEASURED against `envoyproxy/envoy:v1.33.0`) |
|---|---:|---|
| `http.ingress_http.health_check.request_total` | 2 | ticks once per INTERCEPT, never on a fall-through |
| `http.ingress_http.health_check.ok` | 2 | the same, in non-pass-through mode |
| `http.ingress_http.downstream_rq_total` | 3 | every request is counted |
| `http.ingress_http.downstream_rq_2xx` | 1 | **an intercepted probe is NOT counted** — only the fall-through is |
| `http.ingress_http.health_check.failed` | 0 | ⚠ NOT a presence witness (see below) |

⚠ `scrape_admin_stat` returns `0` for a name a proxy never registered, so a `value: 0` entry passes
even when the stat is ABSENT. Only the four non-zero entries are witnesses. Upstream registers all
eight `health_check.*` counters at config load; their PRESENCE on envoy-rust is pinned in-process
(`registers_eight_counters_and_ticks_two_per_intercept`), not here.

**Not witnessed, and not implemented:** upstream also ticks `http.ingress_http.tracing.health_check`.
envoy-rust has no `tracing.*` stat family at all (CF-115-7).

The `scrapes:` entry is fixture `0015`'s `/server_info` sub-case, present only because the driver
requires a non-empty list.

## Byte-identical configs

`envoy.yaml` and `envoy-rust.yaml` are byte-identical (`cmp` silent). Re-derive, do not inherit.

## Running it

```bash
cargo build -p envoy-bin
cargo test -p differential --test http_filter_health_check_stats
```

## Proof it is not vacuous

| # | mutation | result |
|---|---|---|
| V4 | the H1 per-class counter gate compares against a sentinel that never matches | `subject stat http.ingress_http.downstream_rq_2xx expected 1 got 3` |
| V5 | delete `self.request_total.inc();` | `subject stat http.ingress_http.health_check.request_total expected 2 got 0` |
