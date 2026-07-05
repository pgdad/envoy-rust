# DRAFT ADR for phase 62 — to be slotted into `docs/envoy-rust/DECISIONS.md`

> Standalone draft. `DECISIONS.md` is append-only, newest-first; the maintainer places
> this at the canonical position and confirms the number. **ADR-0119** is next-available:
> the ledger head on `main` is ADR-0115 (phase 58); phases 59/60/61 draft ADR-0116/0117/
> 0118. If a sibling session lands 0116–0118 first, this stays 0119; else renumber to the
> then-next-available.

---

## ADR-0119: Phase-62 pick + scope — **make the upstream H1 pool's idle timeout configurable via `Cluster.common_http_protocol_options.idle_timeout` (default 60s preserved byte-for-byte); a regression-equivalence phase, NO new fixture**

- Date: 2026-07-05
- Status: accepted
- Context: Perf-plan item **#3 ("upstream connection reuse")** asked whether the proxy
  churns upstream connections. Live measurement on the real k3s cluster (rust-gateway-api
  data plane, STRICT_DNS echo backend, `fortio` load) is unambiguous: under both sustained
  keep-alive load AND adversarial downstream connection churn, the upstream side holds a
  **stable** set of pooled keep-alive connections with **0 TIME_WAIT** — the phase-13
  `H1Pool` (`crates/envoy-http1/src/pool.rs`) is already optimal and needs no rebuild. The
  one residual gap the plan named is that the pool's **idle timeout is hard-coded**
  (`DEFAULT_IDLE_TIMEOUT = 60s`, `pool.rs:27`) — an operator cannot tune upstream
  connection reuse (raise it to hold connections warm across bursty idle gaps; lower it to
  shed them faster). Envoy exposes exactly this as
  `Cluster.common_http_protocol_options.idle_timeout`.
- Options considered:
  - **Do nothing / close #3 as a no-op** — honest (the pool is optimal), but leaves the
    named hard-coded-timeout gap and ships no operator-facing capability. Rejected: the
    configurability is a real, Envoy-faithful, zero-risk improvement.
  - **Rebuild/tune the pool (idle floor, per-endpoint warmth, STRICT_DNS re-resolve)** —
    rejected: the measurement shows no churn to fix; these would be speculative complexity
    (kept as an optional carry-forward note).
  - **Field location: top-level `Cluster.common_http_protocol_options`** (Envoy's
    still-valid field) **vs. the `typed_extension_protocol_options` map**
    (`envoy.extensions.upstreams.http.v3.HttpProtocolOptions`). Chose the top-level field:
    a single opt-in struct, `deny_unknown_fields`-safe, minimal schema surface; the typed-
    extension map is a larger `@type`-keyed change for no phase-62 benefit (noted as a
    future alignment if needed).
  - **Parse the Duration at deserialize vs. at pool-build** — chose pool-build via the
    existing `parse_duration`, matching the schema's convention (`HealthCheck.timeout`),
    so the stored shape is a scalar and no custom serde is added.
  - **Fail-closed vs. lenient on a malformed value** — chose fail-closed
    (`ConfigError::InvalidClusterIdleTimeout` at `validate_cluster`), mirroring Envoy's
    Duration validation and the `InvalidHealthCheckTiming` precedent, rather than silently
    defaulting to 60s.
- Decision: Land §A–§D of the phase-62 SPEC. (§A) `envoy-config`
  `CommonHttpProtocolOptions { idle_timeout: Option<String> }` + opt-in
  `Cluster.common_http_protocol_options`. (§B) `validate_cluster` rejects a present-but-
  non-positive-`parse_duration` `idle_timeout` via new
  `ConfigError::InvalidClusterIdleTimeout { cluster }`. (§C) `H1PoolManager::for_bootstrap`
  sources each cluster's `idle_timeout` (parse → filter non-zero → `unwrap_or(
  DEFAULT_IDLE_TIMEOUT)`) into the existing `H1Pool::new(idle_timeout)` param. (§D) config
  parse/round-trip/omitted/malformed tests + a pool test (configured 30s reaches the pool;
  unconfigured sibling keeps 60s). No new fixture / `Op` / schema-output / runtime crate /
  dependency / fuzz target.
- Rationale: The default path (field absent — every existing bootstrap) passes
  `DEFAULT_IDLE_TIMEOUT` exactly as before, so the change is byte-neutral and the
  regression suite is untouched by construction (a phase-59-style regression-equivalence
  leaf). The only new behavior is on a *newly-expressible* input. Timing is not compared by
  the differential suite, so no fixture is warranted. `parse_duration` + the sweeper's
  positive-Duration clamp are reused, so no new parser and no sweeper change.
- Consequences: `crates/envoy-config` (one struct + one field + one `ConfigError` variant +
  validation + 3 tests), `crates/envoy-http1/src/pool.rs` (a 6-line source-the-value block +
  1 test). No H1 wire / H2 / codec / `parse_bootstrap`-output change → h2spec + parse fuzz
  unaffected. Local evidence this session: `envoy-config` 538 tests + `envoy-http1` 157
  tests green (incl. the new idle_timeout tests + all pool/drain tests); fmt/clippy clean;
  `#![forbid(unsafe_code)]` PRESERVED. Real-cluster validation: with the perf data plane
  the upstream pool measured 50 stable ESTAB / 0 TIME_WAIT under load (reuse already
  optimal) and the perf sweep is same-or-better vs. baseline. OPENS an optional `perf`
  carry-forward (config `max_requests_per_connection`, STRICT_DNS periodic re-resolution);
  CONSUMES none. Independent of phases 59/60/61; rebases onto `main` cleanly. DECISIONS.md
  ledger head after this ADR: **ADR-0119**. ADR-0014 in force; `#![forbid(unsafe_code)]`
  PRESERVED. The state-2 PLAN-write is the next session.
