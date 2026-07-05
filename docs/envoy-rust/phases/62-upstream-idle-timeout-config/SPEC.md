# Phase 62 — `62-upstream-idle-timeout-config` — SPEC

**Pick (ADR-0119):** Make the upstream H1 keep-alive pool's per-connection **idle
timeout configurable** via Envoy's `Cluster.common_http_protocol_options.idle_timeout`,
replacing the hard-coded `DEFAULT_IDLE_TIMEOUT = 60s` in `envoy-http1`'s `H1Pool`. This is
a **regression-equivalence phase** (like phase 59): NO new fixture, NO `Op`/schema-output
change; the default path (field absent) keeps the 60s timeout byte-for-byte, so every
`0001`-`00NN` fixture stays byte-identical. Acceptance is (a) fixtures unchanged +
(b) unit tests for the new field (config parse + validation + pool wiring).

> **Why this phase (perf-plan item #3 — "upstream connection reuse").** The #3
> flamegraph-derived candidate was "reduce upstream connection churn." Live measurement
> on the real k3s cluster (rust-gateway-api data plane, STRICT_DNS echo backend) shows the
> phase-13 pool is **already optimal**: under sustained *and* adversarial
> connection-churn load the upstream side holds a stable set of keep-alive connections
> with **0 TIME_WAIT** — zero churn. So #3 needs **no** pool rebuild. The residual gap the
> plan named is that the pool's idle timeout is **hard-coded 60s** (`pool.rs:27`,
> deferral noted at the sweeper): an operator cannot tune upstream connection reuse for a
> bursty workload (raise it to keep connections warm across idle gaps) or a churny one
> (lower it). This phase closes exactly that gap, Envoy-faithfully, without touching the
> proven pool mechanism.

## §1 — Goal & surface

**Goal.** Thread a per-cluster, operator-set upstream idle timeout into `H1Pool`, keeping
the 60s default byte-identical when unset.

**Differential surface: UNCHANGED.** `idle_timeout` changes only pool eviction *timing*,
which the differential suite does not compare (BEHAVIOR_CONTRACT: timing is not compared
by default). No fixture drives a >60s idle gap, so no fixture output moves. No new fixture.

**Config surface (additive, Envoy-faithful).** `Cluster` gains
`common_http_protocol_options: Option<CommonHttpProtocolOptions>` (Envoy
`core.v3.HttpProtocolOptions`); phase-62 parses-and-stores only its `idle_timeout`
(Envoy `Duration` string, `parse_duration` shapes `"<N>s"`/`"<N>ms"`/`"<N>us"`).
`deny_unknown_fields`-safe (declared field). Absent → `None` → 60s default.

## §2 — Scope (minimum-viable, ADR-0119)

- **§A — `envoy-config` schema.** New `CommonHttpProtocolOptions { idle_timeout:
  Option<String> }` (duration as a scalar, parsed at use-time per the schema's convention
  — cf. `HealthCheck.timeout`). New `Cluster.common_http_protocol_options` field, opt-in.
- **§B — `envoy-config` validation (fail-closed).** `validate_cluster` rejects a present
  `idle_timeout` that is not a positive `parse_duration` scalar, via a new
  `ConfigError::InvalidClusterIdleTimeout { cluster }` — mirroring Envoy's Duration
  validation (and the existing `InvalidHealthCheckTiming` precedent). Absent/`None`
  validates trivially (the regression-equivalence path).
- **§C — `envoy-http1` pool wiring.** `H1PoolManager::for_bootstrap` reads each cluster's
  `common_http_protocol_options.idle_timeout`, parses via
  `envoy_config::bootstrap::parse_duration`, filters out zero, and passes it to the
  existing `H1Pool::new(idle_timeout)` param (previously the hard-coded
  `DEFAULT_IDLE_TIMEOUT`); `unwrap_or(DEFAULT_IDLE_TIMEOUT)` keeps the call total and the
  default byte-identical. No `H1Pool` signature change (it already took `idle_timeout`).
- **§D — tests.** `envoy-config`: parse+round-trip (`idle_timeout: 30s`), omitted→`None`,
  malformed→`InvalidClusterIdleTimeout` (fail-closed). `envoy-http1`: `for_bootstrap`
  applies the configured 30s to the tuned cluster's pool and keeps 60s
  (`DEFAULT_IDLE_TIMEOUT`) for an unconfigured sibling. NO new fixture.

**Load-bearing invariant:** all existing fixtures stay byte-identical — §A/§C are inert
when the field is absent (every current bootstrap), §B only rejects a *new*, previously
unparseable input, and the sweeper's `idle_timeout / SWEEPER_DIVISOR` clamp (`pool.rs`)
already tolerates any positive Duration.

## §3 — Acceptance (§7.5)

(a) all fixtures green + byte-identical (no output moves) + (b) the §D unit tests green
(config parse/validate + pool wiring; default cluster == 60s) + (c) h2spec unchanged (no
H2/codec change) + (d) no new fuzz target (no new parser — `parse_duration` reused) +
(e) build/clippy/fmt/test/deny clean; `#![forbid(unsafe_code)]` holds; ONE new
`ConfigError` variant (`InvalidClusterIdleTimeout`), no new runtime crate/dependency/`Op`/
`AccessLogRecord` field + (f) `REVIEW.md` approved.

## §4 — Reuse map

- `parse_duration` (`bootstrap.rs`) and the `"<N>s"` convention — reused verbatim (§A/§B/§C).
- `H1Pool::new`'s existing `idle_timeout: Duration` param + the idle sweeper's positive-
  Duration clamp — reused; §C only changes the *value* passed, not the mechanism.
- The `InvalidHealthCheckTiming` fail-closed precedent — mirrored by §B.

## §5 — Process

- **§6.1 split — does NOT fire.** ~4 files (`bootstrap.rs`, `lib.rs`, `pool.rs`, this
  SPEC), ~180 LoC incl. tests, no new harness/fixture/struct beyond one config struct +
  one error variant. Well under the gate.
- **Carry-forwards:** OPENS an optional `perf` note (config-driven
  `max_requests_per_connection`, STRICT_DNS periodic re-resolution) — deliberately OUT of
  scope (the live measurement shows no churn to justify them). CONSUMES none. Independent
  of phases 59/60/61 (different surface); rebases onto `main` cleanly.
- Pick + §A–§D locked by **ADR-0119** (next-available; ledger head after 0118 = the
  phase-61 SO_REUSEPORT draft).
