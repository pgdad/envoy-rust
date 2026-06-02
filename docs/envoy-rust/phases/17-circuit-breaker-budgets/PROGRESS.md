# Phase 17 (`17-circuit-breaker-budgets`) — PROGRESS

> Running log, updated by the executor on each task completion (the 06.2 → 16 cadence).
> One entry per PLAN task; quote the verifying command output. The state-3 arc runs
> `cargo clippy --workspace --all-targets --all-features -- -D warnings` PER TASK
> (NOT deferred to state-4) per `project_state3_arc_skips_clippy`.

**PLAN:** `docs/envoy-rust/phases/17-circuit-breaker-budgets/PLAN.md`
**SPEC:** `docs/envoy-rust/phases/17-circuit-breaker-budgets/SPEC.md`
**Scope ADRs:** ADR-0046 (minimum-viable budget scope + the zero-cap-always-trips finding + the cluster-scoped ownership boundary); ADR-0047 (§6.2 reconciliation — the overflow co-firing counters [`upstream_rq_pending_overflow` + `upstream_rq_5xx`; `upstream_cx_total` known divergence] + the momentary `*_open` gauge semantic [fixture asserts at 0]).

---

## State-2 PLAN-write (this commit)

- Performed the HEAVY SPEC §6.2 empirical verification against `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`; Docker; foreground general-purpose subagent; 5-cluster topology; every claim cross-checked against backend request counts). Findings L1–L12 locked into PLAN.md "§6.2 empirical lock-ins". **The §0 zero-cap-always-trips projection is CONFIRMED** (L1/L2 — the most consequential items match). Two material divergences (**L3** the overflow local reply co-fires `upstream_rq_5xx`/`upstream_rq_503`/`upstream_rq_completed` + `upstream_cx_total` prefetch while `upstream_rq_total` stays 0; **L4** the `*_open` breaker gauges are MOMENTARY — 0 at rest and after sequential probes, never latched) → **ADR-0047 landed**.
- **The opportunistic L11 item RESOLVED the M16-3 carryforward:** Envoy DOES emit `x-envoy-attempt-count` on overflow local replies (value 1) under `include_attempt_count_in_response: true` — envoy-rust's existing synth-path emission is empirically CORRECT. Fixture 0025 closes M16-3 with bilateral coverage (per-probe `x-envoy-attempt-count` value assertions).
- Performed the PLAN-time SPEC-correction pass (read-only Explore subagent) against HEAD `ee61fc744`. **ZERO drift** — all 20 SPEC §0/§3 anchors confirmed accurate (the first phase since the correction-pass discipline began with no corrections needed). Confirmations recorded in PLAN.md "PLAN-time SPEC corrections".
- Evaluated the §6.1 split gate against the §6.2-refined surface (~1450–1550 LoC / 11 tasks) → **single un-split phase; ADR-0048 does NOT fire.**
- Flipped ROADMAP row `17` `planned → in-progress`. Advanced STATE.md to `17` state-2-complete / state-3-next.

## Task 1 — `envoy-config` schema (`Thresholds` budget fields + validator acceptance)

**Preamble (read before starting):**
- **Goal:** Add `max_requests`/`max_retries`/`track_remaining` to the `Thresholds` struct (`crates/envoy-config/src/bootstrap.rs:1319-1328`). NO `validate_circuit_breakers` semantic change required — `0` is valid for both new caps (the always-open-breaker configs, L1/L2); zero new `ConfigError` variants. TDD: serde round-trip + deny_unknown_fields rejection (deferred `retry_budget`/`max_connection_pools`) + validator-accepts-zero tests first.
- **§6.2 lock-ins that bind this task:** L1/L2 (`0` caps are valid, meaningful configs — the validator must NOT reject them, in contrast to `max_connections == 0` → `InvalidMaxConnections`); L5 (defaults 3/1024 are resolved at `Cluster::from_bootstrap` [Task 3], NOT in the schema — the schema keeps `Option<u32>`).
- **Anchors (verified at HEAD `ee61fc744` — ZERO drift):** `Thresholds` `bootstrap.rs:1319-1328` (`#[serde(deny_unknown_fields)]`; fields priority/max_connections/max_pending_requests); `validate_circuit_breakers` `:2729-2767`; the 4 existing CB `ConfigError` variants in `lib.rs:445-469`.
- **Carry-forward warning:** the phase-15 pool test modules (`crates/envoy-http1/src/pool.rs` + `crates/envoy-http2/src/pool.rs` `#[cfg(test)]`) construct `Thresholds` struct literals — if exhaustive, they break on the new fields and MUST be extended with `max_requests: None, max_retries: None, track_remaining: None` in the SAME commit (the phase-16 Task-1 workspace-compile lesson).
- **Verification:** `cargo test -p envoy-config` (PASS) + `cargo build --workspace --all-targets` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` + `cargo fmt --all -- --check`.

## Task 1 — `Thresholds` budget fields (`max_requests`/`max_retries`/`track_remaining`) (commit `902a5aa48`)

**Landed.** `crates/envoy-config/src/bootstrap.rs` (single file): the 3 new `Option` fields on `Thresholds`
(`max_requests: Option<u32>` / `max_retries: Option<u32>` / `track_remaining: Option<bool>`, each
`#[serde(default, skip_serializing_if = "Option::is_none")]`; `deny_unknown_fields` retained); the
`validate_circuit_breakers` doc-comment explaining the zero-cap acceptance asymmetry (`max_requests: 0`/
`max_retries: 0` = always-open breakers [L1/L2], ACCEPTED, vs `max_connections: 0` = `InvalidMaxConnections`
[phase-13 rationale]); the `CircuitBreakers` struct doc updated to reflect the new accepted-field set.
**Zero new `ConfigError` variants; zero semantic validation changes** (PLAN lock-in). The pre-existing test
`cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields` renamed to
`..._rejects_still_deferred_threshold_fields` (its unknown-field probe switched `max_requests: 5` →
`max_connection_pools: 5` since the former is now a valid field — no coverage lost). 7 new TDD tests
(parse 0/0/true; parse 5/3/false; absent → None; `retry_budget` rejected; `max_connection_pools` rejected;
validator accepts zero caps; existing rejections still fire with budget fields present).

**Pool-literal carry-forward warning: did NOT materialize.** `grep Thresholds crates/envoy-http{1,2}/src/pool.rs`
→ no struct literals (the pools read thresholds via `.and_then()` accessors) — no fold-in needed; the commit
touches exactly 1 file.

**Verification (quoted):**
- TDD RED: 6 compile errors (`no field 'max_requests' on type '&Thresholds'`) before the fields landed.
- `cargo test -p envoy-config` → `test result: ok. 302 passed; 0 failed` (295 + 7 new).
- `cargo build --workspace --all-targets` → clean.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean (exit 0).
- `cargo fmt --all -- --check` → clean (exit 0).
- `git show --stat HEAD` → 1 file changed (`bootstrap.rs` +346/−14).

**Two-stage review:** spec-compliance **✅ compliant** (first pass; reviewer independently re-ran all gates +
verified the renamed test lost no coverage + verified the pool.rs no-literal claim); code-quality **Approved**
(zero Critical / zero Important / 3 Minors — stale `CircuitBreakers` doc + 2 test-coverage nits — **all 3
fixed in-task** before the commit was finalized [amended pre-push]).

---

_(Tasks 2–11 entries appended by the executor as each completes.)_
