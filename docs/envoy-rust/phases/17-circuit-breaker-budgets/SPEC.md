# Phase 17 (`17-circuit-breaker-budgets`) — SPEC

- **Phase id:** `17`
- **Slug:** `17-circuit-breaker-budgets`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `8dd1aa7f6`, the phase-16 state-6 deterministic close-out commit; the "Upstream robustness family" §9 table at that HEAD carries rows `12`/`12.1`/`12.2`/`13`/`13.1`/`13.2`/`14`/`14.1`/`14.2`/`15`/`16`, all `status: done` — no row exists yet for circuit-breaker budgets). **This SPEC's landing commit adds the SIXTH concrete row beneath the "Upstream robustness family" heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — active health checks (HTTP/TCP/gRPC/custom), outlier detection variants, **circuit breakers**, **retries** + hedging, per-protocol connection pooling."* This phase lands the **circuit-breaker budgets**: the `max_retries` retry breaker (caps concurrent retries cluster-wide; overflow → `upstream_rq_retry_overflow` + the response surfaces un-retried) and the `max_requests` request breaker (caps concurrent requests cluster-wide; overflow → the 503 overflow local reply), plus the `circuit_breakers.default.{rq_retry_open, rq_open}` breaker-state gauges and the `track_remaining: true` → `remaining_*` gauge family. This is the **promised owner phase** of the deferred surface enumerated in BOTH the phase-15 rollovers (*"`max_requests`/`max_retries`/`track_remaining`/multi-priority — the same budget phase"*) and the phase-16 rollovers (*"the `max_retries` circuit breaker … the circuit-breaker-budget expansion phase, which phase 16's retries now UNBLOCK"*).
- **Position in the project:** the **ninth post-MVP-trunk feature-family phase** and the **sixth concrete Upstream-robustness-family phase** (after parent-12 active HTTP health checking closed at `3ec7fb9`, parent-13 connection pooling closed at `96630f9`, parent-14 outlier detection closed at `b575bdc35`, phase-15 circuit-breaker observability closed at `4dad7c4ae`, and phase-16 HTTP retries closed at `8dd1aa7f6`). The MVP trunk 00→08 + the three HTTP-filter-family phases (09/10/11) + 12/13/14/15/16 all stand `done`. The **24-Docker-gated-fixture regression baseline** established at phase-16 close (`0001-tcp-echo` through `0024-upstream-retry-on-5xx`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b). **After phase 17 lands, the Upstream-robustness family is complete in minimum-viable form** (every §9 charter member has a landed minimum-viable phase), and the next brainstorm can deliberately open a new §9 family.
- **depends-on:** `04 05 06 13 15 16` — phase `04`/`05` (the `envoy-http1`/`envoy-http2` router-proxy dispatch arms whose entry the `max_requests` breaker gates), phase `06` (the `envoy-stats` foundation: `StatsRegistry` + `Counter`/`Gauge` primitives), phase `13` (the connection pools + the `Thresholds` schema the new fields extend), phase `15` (the `circuit_breakers.default.*` gauge namespace + the conditional-registration discipline + the 81-byte overflow-503 local reply + fixture 0023 — the structural template), and phase `16` (the H1/H2 retry loops whose retry-decision points the `max_retries` breaker gates + the `upstream_rq_retry*` counter family the new `upstream_rq_retry_overflow` joins + the stateful fail-then-succeed backend + fixture 0024 — the structural template).
- **Brainstorm narrative:** see the "Phase-17 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + feature-pick rationale, the non-obvious **zero-cap-always-trips deterministic-fixture finding** (§0) that makes the budget breakers differentially testable without concurrency, and the alternatives weighed (per_try_timeout; pool hardening; TCP-proxy pooling; opening a new §9 family). The scoping decision is ratified in **ADR-0046** (landed at this brainstorm commit).

---

## 0. Critical scoping finding (READ FIRST) — zero-cap breakers trip deterministically; no concurrency-based fixtures needed

A circuit-breaker *budget* is inherently a **concurrency** feature: `max_retries` caps retries *in flight at the same instant*, `max_requests` caps requests *in flight at the same instant*. A naive differential fixture would need concurrent downstream load + a slow backend to deterministically hold N requests in flight — exactly the timing-fragile shape phase 15 explicitly dropped (its SPEC's `Driver::Http1Concurrent`/`--hold-ms` machinery was abandoned at the ADR-0043 re-scope because concurrent cross-proxy assertions are a flakiness vector).

The state-1 brainstorm identified a deterministic alternative anchored on an **empirical phase-15 precedent**:

- **Phase 15's §6.2 Docker verification (ADR-0043) empirically established that Envoy treats an explicitly-configured `0` threshold as an always-open breaker** — `max_pending_requests: 0` rejects EVERY request (not "unlimited", not "use default"): the very first request, with zero prior load, is rejected with the 503 overflow local reply + `upstream_rq_pending_overflow: 1`. Fixture 0023 asserts this bilaterally today with a single sequential GET.
- **By the same resource-manager semantic, `max_retries: 0` and `max_requests: 0` are projected to be always-open breakers**: with `max_retries: 0`, the FIRST would-be retry overflows (no concurrency needed — `upstream_rq_retry_overflow: 1`, the would-be-retried response surfaces un-retried); with `max_requests: 0`, the FIRST request overflows (the 503 overflow local reply, backend never contacted). **A single sequential GET deterministically trips each breaker.** (§6.2 item 1/2 verifies this projection empirically before the PLAN locks it.)

Two further seam findings make the phase surgical:

- **The enforcement points already exist as single-site seams.** The `max_retries` check is a ~10-line insertion at the phase-16 retry-decision points (H1 `crates/envoy-http1/src/hcm.rs:776-792`, H2 `crates/envoy-http2/src/hcm.rs` retry-decision mirror) — the loop already centralizes "should I retry?"; the budget is one more conjunct. The `max_requests` check is an insertion at the dispatch-arm entries (before pool acquire), reusing the phase-15 `synth_overflow` 503 helpers (H1 `hcm.rs:1245-1268`, H2 `hcm.rs:857-875`) for the reject path.
- **Real Envoy already emits the full breaker-state gauge family** — fixture 0023's expectations note records that Envoy v1.33 emits ~10 `circuit_breakers.{default,high}.{cx_open, cx_pool_open, rq_open, rq_pending_open, rq_retry_open}` gauges regardless of config (envoy-rust emits only `default.cx_open` at phase-15 scope), and fixture 0024's notes list `circuit_breakers.*.rq_retry_open` + `upstream_rq_retry_overflow` among the Envoy-only names envoy-rust does not yet emit. Phase 17 moves exactly those names from "Envoy-only, unasserted" to **value-exact bilateral assertions** — the differential surface is already sitting on the other side of the diff waiting to be matched.

**Consequence:** phase 17 needs **NO concurrency-driven differential machinery and no new harness driver** — the zero-cap breakers + the existing `Driver::Http1KeepAlive` sequential driver give deterministic, timing-robust bilateral coverage. Budget caps **above** zero (the genuinely concurrent regime) are covered by the in-process backstop only (where timing is controllable), exactly as phase 15 covered `cx_open`'s both-edges in-process.

This finding is ratified in **ADR-0046** (landed at this brainstorm commit) and is the reason a minimum-viable budget phase is tractable as a single un-split phase rather than a concurrency-harness sub-project.

---

## 1. Goal and acceptance signal

Phase 17 makes the **cluster-scoped circuit-breaker budgets enforceable and observable**. When a cluster configures `circuit_breakers.thresholds[0].max_retries` and/or `max_requests` (and optionally `track_remaining: true`), both upstream Envoy and envoy-rust:

- **gate every would-be retry** on the retry budget: a retry proceeds only if the cluster's concurrent-retry count is below `max_retries`; otherwise the retry is abandoned, `cluster.<name>.upstream_rq_retry_overflow` ticks, and the would-be-retried response is returned downstream verbatim (NOT a synth local reply),
- **gate every request dispatch** on the request budget: a request proceeds only if the cluster's concurrent-request count is below `max_requests`; otherwise the request is rejected with the 503 overflow local reply (the phase-15 81-byte `…reset reason: overflow` body + `x-envoy-overloaded: true`) and the §6.2-verified overflow counter ticks,
- **expose the breaker state**: `cluster.<name>.circuit_breakers.default.rq_retry_open` / `.rq_open` gauges (1 when the breaker is open, 0 otherwise), and — when `track_remaining: true` — the `remaining_retries` / `remaining_rq` (+ §6.2-verified siblings) gauges.

**Differential surface added by phase 17:**

- **Fixture `0025-upstream-circuit-breaker-retry-budget`** — bilateral assertion that both proxies, given identical bootstraps configuring three single-endpoint H1 clusters, produce on three sequential GETs (one `Driver::Http1KeepAlive` probe list — timing-robust, no concurrency):
  1. **`/budget-blocked`** → cluster `budget_zero` (`retry_policy: {retry_on: "5xx", num_retries: 1}` + `circuit_breakers: {thresholds: [{max_retries: 0, track_remaining: true}]}`; always-503 backend path): final **503** + the backend's real 503 body (the first attempt's response surfaces verbatim — the retry is budget-blocked, NOT retried, NOT replaced by a synth), `upstream_rq_retry: 0`, `upstream_rq_retry_overflow: 1`, `upstream_rq_total: 1` (one attempt only), `circuit_breakers.default.rq_retry_open: 1`, `circuit_breakers.default.remaining_retries: 0`.
  2. **`/budget-allowed`** → cluster `budget_default` (same `retry_policy`, NO `max_retries` configured — the default budget; fail-once-then-succeed backend path): final **200** (the phase-16 retried-to-200 path — proves the budget machinery does NOT block retries when the budget has room), `upstream_rq_retry: 1`, `upstream_rq_retry_success: 1`, `upstream_rq_retry_overflow: 0`.
  3. **`/rq-blocked`** → cluster `rq_zero` (`circuit_breakers: {thresholds: [{max_requests: 0}]}`, no retry_policy; backend never contacted): final **503** + the 81-byte overflow body + `x-envoy-overloaded` present, `upstream_cx_total: 0`, the §6.2-verified overflow counter `= 1`, `circuit_breakers.default.rq_open: 1`.

  The discriminating differential observables are the **breaker-overflow counters + the breaker-state gauges + the un-retried-response wire shape** — without the budget config, probe 1 would retry to a different outcome and probe 3 would reach the backend. Exact stat names/values and the rq_open/rq_retry_open initial-vs-edge semantics are §6.2-verified projections.

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0025-upstream-circuit-breaker-retry-budget` green at Docker-gated CI.
- **(b)** All **24 pre-existing differential fixtures** (`0001` through `0024`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The budget machinery is inert when `max_retries`/`max_requests`/`track_remaining` are unconfigured (no existing fixture configures them; clusters without `circuit_breakers` see zero new registered stats per the phase-15 conditional-registration discipline; clusters WITH `circuit_breakers` but without the new fields — fixtures 0020/0023 — get Envoy-default budget caps that a sequential single-attempt workload never approaches). **State-2 PLAN-writer empirically confirms** no existing fixture's `expectations.yaml` asserts the new stat names (fixtures 0023/0024's notes confirm they are currently UNASSERTED Envoy-only names — the `Http1KeepAlive` named-stat scrape ignores unasserted names, so adding envoy-rust-side emission cannot break them).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 17 does not touch H2 framing; the H2-side budget gates wrap post-dispatch logic only.
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (the existing `cluster_circuit_breakers.yaml` seed is extended in place OR a new `cluster_circuit_breaker_budgets.yaml` seed lands; corpus 28 → 29 if new — PLAN-writer's call; if a NEW seed file: edit the fuzz `.gitignore` allow-list AND the `fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array together, the 09→16 atomic-edit lesson).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (run PER TASK in the state-3 arc, per `project_state3_arc_skips_clippy`), `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent).

> **NOTE — single phase projected (see §6.1).** Phase 17's surface (schema + validator + cluster-scoped budget primitives + H1/H2 retry-budget gates + H1/H2 request-budget gates + the gauge family + fixture 0025 + in-process backstop + fuzz seed + BEHAVIOR_CONTRACT rows) is projected at **~1250–1550 LoC / ~11–14 tasks** — NEAR but under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task split gate (the same "genuinely close" posture phases 15 and 16 carried; both landed single). The recommended split seam if the §6.2-refined estimate fires the gate: **`17.1`** (schema + validator + cluster budget primitives + the `max_retries` retry breaker + fixture 0025 probes 1–2) / **`17.2`** (the `max_requests` request breaker + the `track_remaining` gauge family + fixture 0025 probe 3 + parent-17 close). The split ADR would be ADR-0048 (§7).

---

## 2. Behavior-contract scope for phase 17

Phase 17 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x→16 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — circuit-breaker budget subset (projected; §6.2-verified)

New rows, mirroring upstream Envoy v1.33's documented stat tree. **Minimum-viable subset** (the 14.1/15/16 namespace-subset precedent):

| Stat name | Kind | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|---|
| `cluster.<name>.upstream_rq_retry_overflow` | counter | value-exact | +1 per retry abandoned because the retry budget (`max_retries`) is exhausted. Under fixture-0025 probe 1, exactly `1`. Joins the phase-16 `upstream_rq_retry{,_success,_limit_exceeded}` family (single source of truth at the retry-decision budget gate). |
| `cluster.<name>.circuit_breakers.default.rq_retry_open` | gauge | value-exact | 1 when the retry breaker is open (concurrent retries ≥ `max_retries`), else 0. With `max_retries: 0` the breaker is open from construction (§6.2 item 4 verifies the at-rest value). |
| `cluster.<name>.circuit_breakers.default.rq_open` | gauge | value-exact | 1 when the request breaker is open (concurrent requests ≥ `max_requests`), else 0. With `max_requests: 0` open from construction (§6.2 item 4). |
| `cluster.<name>.circuit_breakers.default.remaining_retries` | gauge | value-exact (only when `track_remaining: true`) | Remaining retry-budget slots (`max_retries − active_retries`, floored at 0). §6.2 item 8 verifies absent-vs-0 when `track_remaining` is unset. |
| `cluster.<name>.circuit_breakers.default.remaining_rq` | gauge | value-exact (only when `track_remaining: true`) | Remaining request-budget slots. Same conditionality. |
| max_requests-overflow counter (projected: `cluster.<name>.upstream_rq_pending_overflow`) | counter | value-exact | Envoy's documented stat description (*"Total requests that overflowed connection pool **or requests circuit breaking** and were failed"*) projects that `max_requests` overflow ticks the SAME `upstream_rq_pending_overflow` counter phase 15 wired for `max_pending_requests`. **§6.2 item 3 verifies which counter ticks** — this is the highest-divergence-risk projection (reserved trigger for ADR-0047). |

**Deferred sibling names** (`circuit_breakers.default.cx_pool_open`, `.rq_pending_open`, `circuit_breakers.high.*`, `remaining_cx`, `remaining_pending`, `remaining_cx_pools`) are NOT emitted at phase-17 minimum-viable scope unless §6.2 item 4 finds them load-bearing for fixture 0025 — they remain Envoy-only unasserted names (the named-stat scrape ignores them; recorded in BEHAVIOR_CONTRACT). **§6.2 item 4 enumerates the exact Envoy-side gauge set + at-rest values** so the contract rows are complete.

### 2.2 "Response wire shape" — budget-blocked retry vs request-breaker overflow (projected; §6.2-verified)

- **Budget-blocked retry (probe 1):** the downstream response is the **would-be-retried attempt's real upstream response verbatim** (the backend's 503 + its body) — NOT a synth local reply, NOT the 81-byte overflow body. This mirrors the phase-16 limit-exceeded wire shape (the last real response surfaces). **§6.2 item 6 verifies** the access-log `%RESPONSE_FLAGS%` for this case (projected: no overflow flag on the response itself; possibly `URX`-adjacent — access-log-only either way) and confirms no new response header appears.
- **Request-breaker overflow (probe 3):** the downstream response is the **same 503 overflow local reply phase 15 verified** for pending overflow — the 81-byte `upstream connect error or disconnect/reset before headers. reset reason: overflow` body + `x-envoy-overloaded: true` (+ Envoy's standard local-reply headers). envoy-rust reuses the existing `synth_overflow`/`synth_h2_overflow` helpers verbatim. **§6.2 item 2 verifies** the body byte-equality for the `max_requests`-overflow case specifically (projected identical to the pending-overflow case).

### 2.3 DECISIONS.md amendment at SPEC time — ADR-0046 (the scoping ADR)

Like phases 15 (ADR-0042) and 16 (ADR-0044), phase 17's brainstorm DOES land an ADR: **ADR-0046** records (a) the non-obvious **zero-cap-always-trips deterministic-fixture finding** (§0 — the projection from phase-15's empirical `max_pending_requests: 0` finding that makes budget breakers differentially testable without concurrency), (b) the **cluster-scoped budget-ownership architecture decision** (the budget atomics + new stat handles live in `envoy-cluster::Cluster`, NOT in the per-protocol pools — see §5.4), and (c) the minimum-viable scope boundary — deliver `max_retries` + `max_requests` + `track_remaining` + the 2-counter/4-gauge observability subset + fixture 0025; defer the `max_pending_requests > 0` pending QUEUE, `retry_budget` percent-based budgets, `max_connection_pools`, multi-priority (HIGH) thresholds, and the concurrency-regime differential fixtures. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 17's scope is enumerated as deliverables `D1`–`D8` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (projected NOT to fire). Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — `envoy-config` schema extension (`Thresholds` budget fields)

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `Thresholds` struct (`bootstrap.rs:1320-1328`, currently `priority` + `max_connections` + `max_pending_requests`, `#[serde(deny_unknown_fields)]`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<RoutingPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pending_requests: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests: Option<u32>,             // NEW (phase 17)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,              // NEW (phase 17)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_remaining: Option<bool>,         // NEW (phase 17)
}
```

`deny_unknown_fields` continues to reject the still-deferred fields (`max_connection_pools`, `retry_budget`). The exact proto field names/types are §6.2-verified against the Envoy v1.33 `CircuitBreakers.Thresholds` proto (the PLAN-writer confirms `track_remaining` is a plain bool and the Envoy defaults: `max_retries` default 3, `max_requests` default 1024 — §6.2 item 5).

### D2 — `envoy-config` validator update

Extend `validate_circuit_breakers` (`bootstrap.rs:2729-2767`): `max_requests: 0` and `max_retries: 0` are **ACCEPTED** (they are the always-open-breaker configurations the fixture relies on — §0; this contrasts with the existing `max_connections == 0 → InvalidMaxConnections` rejection, whose phase-13 rationale the PLAN-writer re-confirms and documents in the validator comments). The phase-15 rejections stand: multiple thresholds → `UnsupportedMultipleCircuitBreakerThresholds`; non-DEFAULT priority → `UnsupportedCircuitBreakerPriority`; `max_pending_requests > 0` → `UnsupportedNonZeroMaxPendingRequests` (the pending QUEUE remains deferred per §4). ~0–2 new `ConfigError` variants (PLAN-writer's count; possibly zero — the new fields may need no semantic rejection). Positive + negative parse-path unit tests per the 13.1/15/16 validator-test cadence.

### D3 — Cluster-scoped budget primitives (`envoy-cluster`)

The architectural core. `envoy_cluster::Cluster` (`crates/envoy-cluster/src/cluster.rs`) gains:

- **Budget config:** `max_retries: u32` / `max_requests: u32` resolved from `circuit_breakers` at `from_bootstrap` (Envoy defaults when the threshold field is unset — §6.2 item 5; effectively-unlimited sentinel when `circuit_breakers` itself is absent).
- **Budget state:** `active_retries: AtomicI64` + `active_requests: AtomicI64` (or `envoy_stats::Gauge` reuse — PLAN-writer's call; the gauge already wraps an `AtomicI64`).
- **RAII acquisition API:** `try_acquire_retry() -> Option<RetryBudgetGuard>` and `try_acquire_request() -> Option<RequestBudgetGuard>` — compare-and-increment under the cap, decrement on guard drop (the 13.x `PoolGuard`/`ConnGaugeGuard` RAII discipline). With a `0` cap, acquisition always fails (the §0 always-open semantic).
- **Stat handles:** `upstream_rq_retry_overflow` (counter) + `rq_retry_open`/`rq_open` (gauges) + `remaining_retries`/`remaining_rq` (gauges, only when `track_remaining: true`), registered in `from_bootstrap` (next to the phase-16 retry counters at `cluster.rs:742-809`) **conditionally on `circuit_breakers` being configured** (the phase-15 conditional-registration discipline — clusters without `circuit_breakers` register none of these, keeping the 24 existing fixtures' stat surfaces untouched). Gauge updates happen inside the acquire/release paths (single source of truth).

Note the registration-site asymmetry this creates: phase-15's `cx_open`/`upstream_cx_overflow`/`upstream_rq_pending_overflow` live in the per-protocol POOLS; phase-17's budget stats live in the CLUSTER. This is deliberate (§5.4 records why); the PLAN-writer documents the seam in BEHAVIOR_CONTRACT.

### D4 — H1 + H2 retry-budget gate (`max_retries`)

At the phase-16 retry-decision points (H1 `crates/envoy-http1/src/hcm.rs:776-792`; H2 `crates/envoy-http2/src/hcm.rs` mirror at the `:670-672` region): a retriable outcome with `attempts <= num_retries` budget remaining additionally requires `cluster.try_acquire_retry()` to succeed. On success: hold the `RetryBudgetGuard` for the duration of the retry attempt (drop after the attempt completes), tick `upstream_rq_retry` as today, back-off, loop. On failure (budget exhausted): tick `upstream_rq_retry_overflow`, do NOT tick `upstream_rq_retry` (the retry never happens), do NOT tick `upstream_rq_retry_limit_exceeded` (§6.2 item 7 verifies Envoy's counter exclusivity here), break the loop, and surface the would-be-retried response verbatim (§2.2). H1/H2 sibling parity per the 13.x→16 discipline (the phase-16 review's named focus).

### D5 — H1 + H2 request-budget gate (`max_requests`)

At the dispatch-arm entries (H1 `hcm.rs` proxy arm before pool acquire; H2 `hcm.rs` mirror): `cluster.try_acquire_request()` gates the entire dispatch (including all retry attempts — the guard spans the retry loop, §6.2 item 9 verifies whether Envoy counts a retrying request as 1 or N against `max_requests`). On failure: respond with the existing `synth_overflow`/`synth_h2_overflow` 503 (the phase-15 81-byte body + `x-envoy-overloaded`), tick the §6.2-item-3-verified overflow counter, set the `rq_open` gauge edge, and never contact the pool/backend (`upstream_cx_total` stays 0 — the fixture-0023 precedent).

### D6 — `track_remaining` gauge family

When `track_remaining: true`: register + maintain `remaining_retries` / `remaining_rq` (values = cap − active, floored at 0, updated at every acquire/release — single source of truth inside the D3 guard methods). When unset/false: the gauges are NOT registered (absent from `/stats`, not present-at-0 — §6.2 item 8 verifies Envoy's absent-vs-0 posture; the conditional-registration machinery from D3 makes either trivially implementable). The phase-15-deferred `remaining_cx`/`remaining_pending` siblings: §6.2 item 4 enumerates whether Envoy emits them with `track_remaining: true` even when only budget fields are configured; if yes, the PLAN-writer decides minimum-viable inclusion (they are 2 more conditional gauges over already-tracked quantities — cheap) vs Envoy-only allowlisting (PLAN-writer's call; SPEC projects inclusion of `remaining_retries`/`remaining_rq` ONLY).

### D7 — Fixture 0025 + Docker wrapper + in-process backstop + fuzz seed

- **D7.1 — Fixture `tests/fixtures/0025-upstream-circuit-breaker-retry-budget/`.** The §1 three-probe topology: three single-endpoint STRICT_DNS H1 clusters (`budget_zero`, `budget_default`, `rq_zero`) + the stateful backend (always-503 `--per-path` path for probe 1; fail-once-then-succeed `--retry-script` path for probe 2; probe 3's backend is never contacted). `dns_lookup_family: V4_ONLY` (the ADR-0024/phase-16-L11 macOS-Docker posture). Driven via `Driver::Http1KeepAlive` (sequential; per-request status + byte-exact body + header-presence assertions + named cumulative `expected_stats`). Stats asserted per §1 — including BOTH the new names (value-exact) AND the inert-0 assertions on names that must NOT tick (`upstream_rq_retry: 0` on probe 1's cluster; `upstream_rq_retry_overflow: 0` on probe 2's cluster).
- **D7.2 — `tests/differential/tests/upstream_circuit_breaker_budgets.rs`** Docker-gated wrapper mirroring the 13.1/14.2/15/16 shape.
- **D7.3 — In-process backstop at `crates/envoy-bin/tests/upstream_circuit_breaker_budgets.rs`**, mirroring the 15/16 backstop shape. Exercises (i) the budget-blocked-retry path (probe-1 equivalent: counters + gauge + the un-retried response), (ii) the budget-allowed-retry path (probe-2 equivalent), (iii) the request-breaker-overflow path (probe-3 equivalent: 503 body + counter + gauge + zero upstream contact), AND (iv) **the above-zero-cap concurrency regime that the differential fixture cannot cover deterministically** (§0): `max_requests: 1` + two concurrent in-process requests against a slow in-process backend → exactly one 503 overflow; `max_retries: 1` + two concurrent retrying requests → exactly one `upstream_rq_retry_overflow`. The in-process backstop is the ONLY place the >0-cap regime is asserted (timing is controllable in-process — the 14.2/15/16 both-paths discipline extended to the concurrency regime).
- **D7.4 — Fuzz corpus seed.** Extend `crates/envoy-config/fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml` in place with the new fields, OR add `cluster_circuit_breaker_budgets.yaml` (corpus 28 → 29) — PLAN-writer's call (if a NEW seed file: the §1(d) atomic-edit discipline).

### D8 — BEHAVIOR_CONTRACT extensions

Land the §2.1 stat rows + the §2.2 wire-shape rows + the D3 registration-site-seam note at the task where each is first empirically exercised (the 06.x→16 contract-extension cadence).

---

## 4. Out of scope (deferred non-goals)

Phase 17 explicitly does NOT land:

- **The `max_pending_requests > 0` pending QUEUE.** A real pending queue (requests park waiting for a pool slot instead of being rejected) is a fundamentally different architectural lift — an async wait-queue inside the pools with FIFO fairness + timeout interaction. The phase-15 `max_pending_requests: 0`-only carve-out (reject-all) STANDS; `validate_circuit_breakers` continues to reject `max_pending_requests > 0`. **Owner: a future pool-hardening + pending-queue phase** (which also owns the phase-16 pool-liveness carryforwards — the `Connection: close` re-pooling gap + the H2 send-failure no-invalidate asymmetry).
- **`retry_budget` (percent-based retry budgets: `budget_percent` + `min_retry_concurrency`).** The modern alternative to `max_retries` (budget as a % of active requests). The schema rejects the field (deny_unknown_fields). Defers — `max_retries` is the minimum-viable retry breaker.
- **`max_connection_pools` + `cx_pool_open` + `remaining_cx_pools`.** envoy-rust has exactly one pool per cluster per protocol; the connection-pool breaker is meaningless at current architecture. Defers.
- **Multi-priority (HIGH) thresholds + `circuit_breakers.high.*`.** The phase-15 `UnsupportedCircuitBreakerPriority` rejection stands. Defers until a routing-priority phase exists.
- **Concurrency-regime DIFFERENTIAL fixtures** (>0 caps tripped by concurrent load). Covered in-process only (D7.3 (iv)); the differential surface is the deterministic zero-cap regime (§0). A future phase MAY add a concurrent differential driver if cross-proxy timing-robustness is ever solved (the phase-15 dropped-`Http1Concurrent` lesson says: do not attempt it casually).
- **`per_try_timeout`** (still deferred from phase 16 — a retry-hardening follow-up, not a budget feature).
- **`x-envoy-overloaded` request-side / `x-envoy-retry-on` overrides / vhost-level retry_policy / hedging / gRPC retry** (all still deferred from phase 16).

---

## 5. Architectural invariants

Phase 17 honors and extends the established cross-crate invariants:

### 5.1 No new crate, no new top-level Cargo dep

All work lands inside existing crates: `envoy-config` (schema + validator), `envoy-cluster` (the budget primitives + stat handles — the architectural core), `envoy-http1` + `envoy-http2` (the two budget gates at the existing seams), `tests/differential` + `tests/helpers` + `tests/fixtures` (0025), `crates/envoy-bin/tests` (backstop). **No new workspace member; no new top-level Cargo dep.**

### 5.2 Inert-when-unconfigured (the foundation-slice discipline)

Clusters without `circuit_breakers` config: zero new registered stats, zero behavior change (the budget caps resolve to effectively-unlimited sentinels; `try_acquire_*` never fails). Clusters with `circuit_breakers` but without the new fields (fixtures 0020/0023): the new fields resolve to Envoy defaults (`max_retries` 3 / `max_requests` 1024) that a sequential single-attempt workload never approaches, AND the new stat registration is gated on the new fields being present (PLAN-writer locks the exact conditionality — the safest posture keeping 0020/0023 byte-identical; §6.2 item 4's at-rest gauge values inform whether registration-when-configured-but-unused changes the Envoy-side stat surface those fixtures scrape — it does not, since the named-stat scrape ignores unasserted names in BOTH directions). The 24 existing fixtures see byte-identical behavior.

### 5.3 One-source-of-truth stat sites (the 06.x→16 discipline)

`upstream_rq_retry_overflow` increments at exactly ONE logical site per protocol (the D4 budget-gate failure arm). The breaker gauges + `remaining_*` gauges update ONLY inside the D3 acquire/release methods — the H1/H2 callers never touch gauge values directly.

### 5.4 Cluster-scoped budget ownership (the new architecture boundary; ADR-0046)

The budget state + new stat handles live in **`envoy-cluster::Cluster`**, NOT in the per-protocol pools — because (a) `max_retries`/`max_requests` are cluster-wide concepts spanning BOTH protocol pools (a cluster with H2 upstream protocol still dispatches H1-protocol attempts through the same budget), (b) the retry loop already holds a `&ClusterHandle` at the gate sites (no new plumbing), and (c) the phase-15 pool-owned stats (`cx_*`, pending) are connection-lifecycle concepts that genuinely belong to pools. The two registration sites for the `circuit_breakers.default.*` namespace (pools: `cx_open`; cluster: `rq_open`/`rq_retry_open`/`remaining_*`) are documented in BEHAVIOR_CONTRACT (D8).

### 5.5 Budget gates compose with — never replace — existing logic

The retry-budget gate (D4) is a CONJUNCT added to the phase-16 retry decision (`retriable AND attempts <= num_retries AND budget-acquired`); it does not alter classification, back-off, or the limit-exceeded path. The request-budget gate (D5) wraps the dispatch BEFORE the phase-15 pool gates (pending-reject, max_connections); the gate ordering (request breaker → pending gate → connection cap) is §6.2-verified (item 9: which breaker fires first when multiple are at 0) and locked in the PLAN.

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (split projected NOT to fire)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 17's surface estimate at SPEC time:

- D1 — schema (3 fields) (~30 LoC + ~70 LoC tests).
- D2 — validator update (~30 LoC + ~60 LoC tests).
- D3 — cluster budget primitives + RAII guards + conditional stat registration (~150 LoC + ~140 LoC tests).
- D4 — H1 + H2 retry-budget gate (~70 LoC + ~100 LoC tests).
- D5 — H1 + H2 request-budget gate (~90 LoC + ~100 LoC tests).
- D6 — track_remaining gauges (~40 LoC + ~60 LoC tests; mostly inside D3's structure).
- D7.1 — fixture 0025 (3 clusters, 3 probes) (~160 LoC YAML).
- D7.2 — Docker-gated wrapper (~50 LoC).
- D7.3 — in-process backstop (4 paths incl. the concurrency regime) (~280 LoC).
- D7.4 — fuzz seed (~25 LoC + ≤2 file edits).
- D8 — BEHAVIOR_CONTRACT rows (~70 LoC docs).
- State-4 verification + STATE-advance (~docs).

**SPEC-time projection: ~11–14 tasks; ~1250–1550 LoC** (production ~410, tests ~530, fixture/harness/backstop ~515, docs ~70). Single un-split phase projected (the same posture as phases 15/16, both of which landed single); the split seam if the §6.2-refined estimate fires: **`17.1`** (D1+D2+D3+D4+D7.1 probes 1–2+D7.4) / **`17.2`** (D5+D6+D7.1 probe 3+D7.2+D7.3 + parent close). The split ADR would be **ADR-0048** (§7).

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

Per the phase-10→16-ratified verify-at-PLAN-write process: **the state-2 PLAN-writer empirically verifies the upstream behavior shapes BEFORE locking PLAN lock-ins.** Run `envoyproxy/envoy:v1.33.0` Docker with the fixture-0025 three-cluster topology (a `max_retries: 0` + retry_policy cluster; a default-budget retry cluster; a `max_requests: 0` cluster) + the stateful/always-503 backends + a `%RESPONSE_FLAGS%` access log + admin `/stats`, and verify:

1. **`max_retries: 0` semantics (THE §0 anchor):** does an explicitly-configured 0 make the retry breaker always-open (the first would-be retry overflows: `upstream_rq_retry_overflow: 1`, `upstream_rq_retry: 0`, response = the un-retried 503) — or does 0 mean unlimited/default? (Projection: always-open, per the phase-15 `max_pending_requests: 0` empirical precedent. If this diverges → ADR-0047 + the fixture re-anchors on the in-process-only concurrency regime, a major re-scope.)
2. **`max_requests: 0` semantics:** does the first request overflow (503 + the 81-byte overflow body + `x-envoy-overloaded` + backend never contacted)? Byte-compare the local-reply body against the phase-15 pending-overflow body (projected identical).
3. **The `max_requests`-overflow counter:** which counter ticks — `upstream_rq_pending_overflow` (the §2.1 projection per Envoy's stat documentation) or a distinct name? (The highest-divergence-risk projection; ADR-0047 trigger.)
4. **The breaker-gauge family:** exact at-rest + tripped values of `circuit_breakers.default.{rq_retry_open, rq_open}` for the three clusters (does `rq_retry_open` read 1 at rest with `max_retries: 0`, or only after the first overflow?); enumerate the FULL Envoy-side `circuit_breakers.*` gauge + `remaining_*` set emitted for each cluster config (for BEHAVIOR_CONTRACT completeness + the D6 inclusion call).
5. **Defaults:** `max_retries` default (3?) and `max_requests` default (1024?) when `circuit_breakers` is configured but the field is unset — verify via the `budget_default` cluster's `remaining_retries` reading under `track_remaining: true` (set `track_remaining: true` on that cluster in the verification config for this purpose).
6. **Budget-blocked-retry wire shape:** the response is the would-be-retried attempt's real 503 verbatim (body byte-compare against the backend's body); `%RESPONSE_FLAGS%` value for this case; no new response headers.
7. **Counter exclusivity on budget-blocked retry:** `upstream_rq_retry_overflow: 1` with `upstream_rq_retry: 0` AND `upstream_rq_retry_limit_exceeded: 0` (the overflow is not also a limit-exceeded)? And `upstream_rq_total: 1` (the blocked retry never produces a second attempt)?
8. **`track_remaining` absent-vs-0:** with `track_remaining` unset, are `remaining_*` gauges absent from `/stats` or present-at-0? (Decides the D6 conditional-registration shape.)
9. **Gate ordering + retry-vs-request-budget interaction:** with `max_requests: 0` AND a retry_policy, which breaker fires (projected: the request breaker — the request never dispatches, so the retry budget is never consulted)? Does a retrying request count once or per-attempt against `max_requests` (the D5 guard-spans-the-loop projection)?
10. **Inertness of the new fields on fixtures 0020/0023:** confirm adding `max_requests`/`max_retries` SUPPORT (not config) changes nothing in those fixtures' asserted stat sets (they configure only `max_connections`/`max_pending_requests`).
11. **(Opportunistic — the M16-3 carryforward.)** The phase-16 review flagged M16-3 (`x-envoy-attempt-count` emission on synth paths under `include_attempt_count_in_response: true` is an unverified extrapolation vs real Envoy) for "the next retry-adjacent §6.2 empirical verification" — phase 17 IS retry-adjacent. While the Docker harness is up: drive a synth-path request (e.g. the budget-blocked-retry probe with `include_attempt_count_in_response: true` on the vhost) and record whether Envoy emits `x-envoy-attempt-count` on it. This item informs a possible M16-3 disposition but does NOT gate any phase-17 deliverable (no fixture-0025 assertion depends on it; if it diverges, file the finding in PROGRESS as a carryforward update, not an ADR).

Each finding lands as a PLAN lock-in. **If finding 1, 2, or 3 differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §2.x revision via an inline ADR at the state-2 PLAN-write commit** (mirrors ADR-0037/0041/0043/0045). The reserved number is **ADR-0047** (§7). **Finding 3 (which counter ticks on `max_requests` overflow) is the most likely trigger; finding 1 (zero-cap semantics) is the most consequential.**

### 6.3 In-process backstop assertions (heeds the 14.2/15/16 both-paths lesson)

D7.3 exercises all four paths (§3 D7.3 (i)–(iv)) — including the >0-cap concurrency regime that the differential fixture deliberately omits (§0). The backstop is the only place concurrency-regime budget behavior is asserted.

### 6.4 The 06.x stats convention + the inert-when-unconfigured discipline

Conditional registration per §5.2; single-source-of-truth sites per §5.3; the D3 guard methods own all gauge updates.

### 6.5 Pre-state-4 fmt + clippy discipline (heeds `project_state3_arc_skips_clippy`)

`cargo clippy --workspace --all-targets --all-features -- -D warnings` runs PER TASK in the state-3 arc (the phase-16 process note: zero lints reached state-4 under this discipline, vs phase 15's 8 deferred `collapsible_if` lints). The D4/D5 gate insertions add nesting to already-nested retry/dispatch control flow — `collapsible_if` candidates.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 16 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (5 stable-toolchain gates + each Docker-gated fixture + h2spec gate + parse_bootstrap fuzz iteration count).

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot`)

Phase 17 touches `envoy-config`, `envoy-cluster`, `envoy-http1`, `envoy-http2`. **The state-4 verification MUST run `cargo build -p envoy-config`, `-p envoy-cluster`, `-p envoy-http1`, `-p envoy-http2` STANDALONE** (in addition to the workspace build). Quote each standalone build in PROGRESS.

### 6.8 Cargo.lock cadence

Phase 17 adds zero new top-level Cargo deps; no Cargo.lock churn expected beyond version-bump noise (none should occur).

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

Per the 06.2 → … → 16 cadence. State-2 PLAN-write lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit (or, on split, the sub-phase SPECs + ROADMAP + STATE advance + ADR-0048).

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

State 3 implementation is subagent-driven (`superpowers:subagent-driven-development`), implementers dispatched SERIALLY (`feedback_serial_subagent_dispatch`) — not parallel (they race on `main`). Not engaged at this state-1 brainstorm.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0046 (LANDED at this brainstorm commit) — phase-17 minimum-viable circuit-breaker-budget scope + the zero-cap-always-trips deterministic-fixture finding + the cluster-scoped budget-ownership boundary.** Records: the §0 finding (zero-cap breakers trip deterministically per the phase-15 empirical precedent → no concurrency-based differential fixtures needed); the §5.4 architecture boundary (budget state + stats live in `envoy-cluster::Cluster`, not the pools); phase 17 delivers `max_retries` + `max_requests` + `track_remaining` + the §2.1 observability subset + fixture 0025; defers the pending QUEUE, `retry_budget`, `max_connection_pools`, multi-priority, and concurrency-regime differential fixtures. (The cadence mirrors ADR-0042/ADR-0044 — justified by the non-obvious feasibility finding + the architecture boundary decision.)
- **Conditional ADR-0047 (PLAUSIBLE) — §6.2 empirical-verification revision.** Fires if §6.2 finding 1 (zero-cap semantics), finding 2 (`max_requests: 0` wire shape), or finding 3 (the max_requests-overflow counter identity) diverges materially from the §2.x projection. Mirrors ADR-0037/0041/0043/0045. Lands at the state-2 PLAN-write commit if it fires. **Finding 3 is the most likely trigger; finding 1 is the most consequential.**
- **Conditional ADR-0048 (POSSIBLE) — phase split.** Fires if the state-2 LoC estimate exceeds ~1500 (§6.1). Seam: `17.1` (schema + validator + cluster budget primitives + retry breaker + fixture probes 1–2) / `17.2` (request breaker + track_remaining gauges + fixture probe 3 + parent close).

**ADR ledger at SPEC time:** DECISIONS.md head is ADR-0045 (count 46); this SPEC's commit lands **ADR-0046** (count 47; next available ADR-0047). **ADR-0028** (H1-listener × H2-cluster dispatch deferral) REMAINS OPEN — phase 17 does not engage it.

---

## 8. Summary

Phase 17 is the sixth and final minimum-viable Upstream-robustness-family phase. It lands the **circuit-breaker budgets**: cluster-scoped `max_retries` (gates the phase-16 retry loop; overflow → `upstream_rq_retry_overflow` + the un-retried response surfaces verbatim) and `max_requests` (gates dispatch entry; overflow → the phase-15 81-byte 503 overflow local reply), plus the `circuit_breakers.default.{rq_retry_open, rq_open}` breaker-state gauges and the `track_remaining: true` → `remaining_{retries,rq}` gauge family. The budget state + stat handles live in `envoy-cluster::Cluster` (RAII acquire/release guards); the H1/H2 gates are surgical insertions at the existing phase-16 retry-decision and dispatch-entry seams. Fixture 0025 proves the budget-blocked-retry path, the budget-allowed control path, and the request-breaker-overflow path bilaterally on a single timing-robust sequential driver — made possible by the zero-cap-always-trips finding (§0) projected from phase-15's empirical `max_pending_requests: 0` precedent; the >0-cap concurrency regime is covered in-process only. The scope boundary + the §0 finding + the §5.4 ownership boundary are ratified in ADR-0046 at this brainstorm commit; the §6.2 verification (especially zero-cap semantics + the max_requests-overflow counter identity) + the conditional reconciliation/split ADRs are reserved for the next session's PLAN-write. Projected single un-split phase (~1250–1550 LoC). After phase 17, every §9 Upstream-robustness charter member has a landed minimum-viable phase.
