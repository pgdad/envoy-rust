# Phase 16 (`16-http-retries`) — SPEC

- **Phase id:** `16`
- **Slug:** `16-http-retries`
- **Status before this SPEC lands:** _not yet in ROADMAP.md_ (per `docs/envoy-rust/ROADMAP.md` at HEAD `4dad7c4ae`, the phase-15 state-6 deterministic close-out commit; the "Upstream robustness family" §9 table at that HEAD carries rows `12`/`12.1`/`12.2`/`13`/`13.1`/`13.2`/`14`/`14.1`/`14.2`/`15`, all `status: done` — no row exists yet for retries). **This SPEC's landing commit adds the FIFTH concrete row beneath the "Upstream robustness family" heading**, with `status: planned`.
- **Charter source:** `BOOTSTRAP_PROMPT.md` §9 — *"Upstream robustness family — active health checks (HTTP/TCP/gRPC/custom), outlier detection variants, circuit breakers, **retries + hedging**, per-protocol connection pooling."* This phase lands the **HTTP retry policy** — the router-arm retry loop that re-dispatches a failed upstream attempt (on a configured `retry_on` condition) up to `num_retries` times, plus the `upstream_rq_retry*` observability stats and the `x-envoy-attempt-count` response header. **Hedging** (concurrent speculative attempts) is a distinct, more complex feature deferred per §4. Other deferrals (`per_try_timeout`, custom retry back-off intervals, retry budgets, retry-host-predicate/priority, request-header overrides, vhost-level policy, gRPC retry) are enumerated in §4.
- **Position in the project:** the **eighth post-MVP-trunk feature-family phase** and the **fifth concrete Upstream-robustness-family phase** (after parent-12 active HTTP health checking closed at `3ec7fb9`, parent-13 connection pooling closed at `96630f9`, parent-14 outlier detection closed at `b575bdc35`, and phase-15 circuit-breaker observability closed at `4dad7c4ae`). The MVP trunk 00→08 + the three HTTP-filter-family phases (09 `local_ratelimit`, 10 `rbac`, 11 `fault`) + 12/13/14/15 all stand `done`. The **23-Docker-gated-fixture regression baseline** established at phase-15 close (`0001-tcp-echo` through `0023-upstream-circuit-breaker-max-pending-requests`) carries forward unchanged per `BOOTSTRAP_PROMPT.md` §7.5 (b).
- **depends-on:** `04 05 06 13 14` — phase `04` (the `envoy-http1` H1 router-proxy arm at `crates/envoy-http1/src/hcm.rs:465-706` — the pick→acquire→send→receive dispatch seam the retry loop wraps) and phase `05` (the `envoy-http2` H2 router-proxy arm at `crates/envoy-http2/src/hcm.rs:238-481` — the analogous seam, with its H1/H2 protocol dispatch) are the dispatch seams being made retriable. Phase `06` (the `envoy-stats` foundation: `StatsRegistry` + `Counter` primitives) is load-bearing for the new `upstream_rq_retry*` subset. Phase `13` landed the connection pools the retry loop re-acquires from (`H1Pool`/`H2Pool::acquire`). Phase `14` landed the `Cluster::record_response` response-classification hook (`crates/envoy-cluster/src/cluster.rs:281-321`; called H1 `hcm.rs:706` / H2 `hcm.rs:481`) and the health/ejection-aware `pick_endpoint()` seam — the retry loop's re-pick honors both 12.x health-exclusion and 14.x ejection-exclusion, and each attempt's response continues to feed `record_response`.
- **Brainstorm narrative:** see the "Phase-16 state-1 brainstorm" subsection of `docs/envoy-rust/STATE.md` for the family-pick + feature-pick rationale, the non-obvious **body-replay feasibility finding** (H1 sends an empty body / H2 buffers-and-clones → retries are body-replay-safe with no new buffering machinery) that makes a minimum-viable retry phase tractable, and the alternatives weighed (open a new family — HTTP/3+QUIC, gRPC, xDS, Observability; OR another Upstream-robustness member — circuit-breaker-budget expansion `max_retries`/`track_remaining`/pending-queue, TCP-proxy connection pooling). The scoping decision is ratified in **ADR-0044** (landed at this brainstorm commit).

---

## 0. Critical scoping finding (READ FIRST) — body-replay is already safe; no new buffering machinery

A retry feature is only feasible if a failed request can be **re-dispatched** — which requires the request (headers + body) to be replayable. In a streaming proxy this is the load-bearing constraint (Envoy buffers the request body up to a limit and refuses to retry beyond it). The state-1 brainstorm verified the envoy-rust request-body handling directly:

- **H1 sends an EMPTY body upstream.** `crates/envoy-http1/src/hcm.rs` drains the downstream request body into `/dev/null` (`:401-420`, 4 KiB at a time) and dispatches `body: Some(Bytes::new())` (`:488`) — a deliberate phase-04.3 scope choice (chunked request-body forwarding is a non-goal; chunked bodies are rejected with a synth-501 at `:375-381`). A request is therefore **trivially replayable** on H1 — every attempt re-sends the same headers + empty body.
- **H2 BUFFERS the body in memory and clones it per dispatch.** `crates/envoy-http2/src/hcm.rs` drains the H2 DATA frames into an in-memory `body_bytes` (`:153-162`) and dispatches `body: envoy_req.body.clone()` (`:290`). A request is therefore **replayable** on H2 by re-cloning the already-buffered `Bytes`.

**Consequence:** phase 16 needs **NO new request-body buffering machinery** — the existing H1 (empty-body) and H2 (buffered-clone) postures already make every retriable request replayable. The retry loop re-sends the same captured request shape on each attempt. (The H1 empty-body posture means H1 retries are body-agnostic; the H2 buffered-body posture means H2 retries faithfully replay the buffered body. Both are bilaterally correct for bodyless/small-body requests — the differential fixture uses bodyless GETs.)

This finding is ratified in **ADR-0044** (landed at this brainstorm commit) and is the reason a minimum-viable retry phase is tractable as a single un-split (or thin-2-way-split) phase rather than a multi-phase body-buffering sub-project.

---

## 1. Goal and acceptance signal

Phase 16 makes the **HTTP router arm retry a failed upstream attempt** when the route configures a `retry_policy`. When a route configures `retry_policy.retry_on` (a comma-separated condition list) + `num_retries: N`, and an upstream attempt produces a retriable outcome (a matching response status, or a connect-failure/stream-reset matched by `retry_on`), both upstream Envoy and envoy-rust:

- **re-dispatch** the request (re-pick an endpoint honoring 12.x health-exclusion + 14.x ejection-exclusion; re-acquire from the 13.x pool) up to `N` additional times, then
- return the **first non-retriable response** (success, or a non-matching status), OR the **last attempt's response** if all `N` retries are exhausted, and
- emit `cluster.<name>.upstream_rq_retry` (counter; +1 per retry attempted), `cluster.<name>.upstream_rq_retry_success` (counter; +1 when a retried request ultimately succeeds), and `cluster.<name>.upstream_rq_retry_limit_exceeded` (counter; +1 when `num_retries` is exhausted without a non-retriable outcome), plus the `x-envoy-attempt-count` downstream-response header (value = total attempts).

**Differential surface added by phase 16:**

- **Fixture `0024-upstream-retry-on-5xx`** — bilateral assertion that both proxies, given identical bootstraps configuring an H1 upstream cluster routed with `retry_policy: { retry_on: "5xx", num_retries: 1 }`, and a **stateful synthetic backend** that returns `503` on its first request to a path and `200` on the second (the new harness primitive — see §3 D6), produce on a **single downstream GET**: a final **200** response (the first attempt's 503 is retried away), with `cluster.<name>.upstream_rq_retry = 1`, `cluster.<name>.upstream_rq_retry_success = 1`, and `x-envoy-attempt-count: 2`. The discriminating differential observable is the **retried-to-200 result + the retry counters** — without the retry policy both proxies would surface the backend's 503. The fixture ALSO covers the **retry-limit-exceeded** path (a second backend path that returns `503` on every request → both proxies surface a final `503` after `num_retries: 1` is exhausted, with `upstream_rq_retry = 1` + `upstream_rq_retry_limit_exceeded = 1`), driven sequentially via `Driver::Http1ProbeList` (timing-robust — no concurrency). **§6.2 / state-2 PLAN-writer decides** whether the limit-exceeded path rides in fixture 0024 (two paths, one `Http1ProbeList`) or splits to a sibling fixture 0025 (PLAN-writer's call; the SPEC projects a single fixture 0024 carrying both paths).

**Acceptance signal (a)–(f), per `BOOTSTRAP_PROMPT.md` §7.5:**

- **(a)** Fixture `0024-upstream-retry-on-5xx` green at Docker-gated CI.
- **(b)** All **23 pre-existing differential fixtures** (`0001` through `0023`) **remain green simultaneously** at the same CI run (regression-equivalence per §7.5 (b)). The retry loop is inert when no route configures a `retry_policy` (the absent-`Option` path is byte-identical to today's single-attempt dispatch), and the new stats register ONLY for clusters whose routes configure retries — so the existing 23 stay byte-identical. **State-2 PLAN-writer empirically confirms** no existing fixture's `expectations.yaml` asserts the new stat names or `x-envoy-attempt-count` (the inert-when-unconfigured discipline — the 05.1/07.1/12.1/14.1/15 foundation-slice pattern applied to the retry path + the retry stat subset).
- **(c)** `h2spec` continues at ≥95% (parent-05 baseline). Phase 16 does NOT touch the H2 downstream framing nor the H2 codec; the H2 retry loop wraps the post-dispatch logic only. The gate holds (the state-4 verification re-confirms).
- **(d)** `parse_bootstrap` fuzz target clean for the short-budget CI run on the extended corpus (a new `route_retry_policy.yaml` seed exercises the `retry_policy` schema + validator; corpus 22 → 23, OR the PLAN-writer extends an existing route seed in place — PLAN-writer's call).
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` (run PER TASK in the state-3 arc this time — heeds `project_state3_arc_skips_clippy`), `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved.

A **single CI run** must light up gates (a) through (e) **simultaneously** (continues the project precedent — fixture inheritance is a regression vector).

> **NOTE — single phase projected, thin-2-way split held in reserve (see §6.1).** Phase 16's surface (retry-policy schema + validator + the H1 retry loop + the H2 retry loop + 3 observability stats + `x-envoy-attempt-count` + a default back-off + the stateful fail-then-succeed harness backend + fixture 0024 (success + limit-exceeded) + in-process backstop + fuzz seed + BEHAVIOR_CONTRACT rows) is projected at **~1300–1600 LoC / ~12–16 tasks** — NEAR the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task split gate. **Phase 16 is projected to ship as a SINGLE un-split phase, but the split gate is genuinely close** and is re-evaluated at the state-2 PLAN-write against the §6.2-refined estimate. The recommended split seam if it fires: **`16.1`** (config schema + validator + the H1 retry loop + the 3 stats + `x-envoy-attempt-count` + fixture 0024) / **`16.2`** (the H2 retry loop + the limit-exceeded coverage + per-attempt `record_response`/per-class-counter reconciliation + parent-16 close), mirroring the 13/14 H1-then-H2 split cadence. The split ADR would be ADR-0046 (§7).

---

## 2. Behavior-contract scope for phase 16

Phase 16 extends `docs/envoy-rust/BEHAVIOR_CONTRACT.md` with authored additions, landed at the tasks where each is first empirically exercised (per the established 06.x→15 doctrine — contract extensions land at empirical-engagement task time, NOT at PLAN-write time and NOT at state-1 SPEC time).

### 2.1 "Stat-name mapping" extension — retry observability subset (projected; §6.2-verified)

New rows under the cluster retry namespace, mirroring upstream Envoy v1.33's documented stat tree. **Minimum-viable subset** (the 14.1/15 namespace-subset precedent — emit the names Envoy emits for the retry behavior envoy-rust implements; the rest go on `allowlist_envoy_only`):

| Stat name | Equivalence (projected; §6.2-verified) | Rationale |
|---|---|---|
| `cluster.<name>.upstream_rq_retry` | value-exact | Counter; +1 per retry ATTEMPT (i.e. per re-dispatch beyond the first attempt). Under fixture 0024's `num_retries: 1` success path, exactly `1`. Single source of truth at the retry-loop re-dispatch site (H1 + H2). |
| `cluster.<name>.upstream_rq_retry_success` | value-exact | Counter; +1 when a request that was retried at least once ultimately produces a non-retriable (success) outcome. Under the 503→200 success path, exactly `1`. |
| `cluster.<name>.upstream_rq_retry_limit_exceeded` | value-exact | Counter; +1 when `num_retries` retries are all exhausted and the final attempt is still retriable (the limit-exceeded path → surface the last response). Under the always-503 path with `num_retries: 1`, exactly `1`. |

**Interaction with the existing `upstream_rq_total` / `upstream_rq_5xx` + per-class counters (§6.2-CRITICAL).** Envoy increments `cluster.<name>.upstream_rq_total` **per upstream ATTEMPT** (so a request with one retry ticks `upstream_rq_total += 2`), and `upstream_rq_5xx` for the intermediate retried-away 503 as well as any final 5xx. envoy-rust's existing increment sites (`crates/envoy-http1/src/router.rs:95-98`; `crates/envoy-http2/src/hcm.rs:473-476`) currently fire once per request. **The state-2 PLAN-writer empirically confirms Envoy's per-attempt vs per-request counting for `upstream_rq_total` + `upstream_rq_5xx` + the per-class `downstream_rq_*`/`upstream_rq_*xx` counters and reconciles envoy-rust to match** — this is the highest-risk reconciliation in the phase (the existing fixtures 0020/0022 assert these counters byte-exact, so the retry-attempt accounting MUST be Envoy-faithful AND keep those non-retry fixtures inert). Reserved as a §6.2 PLAN lock-in (ADR-0045 if it materially diverges).

**Deferred sibling retry stats** (`upstream_rq_retry_overflow` — retry-budget overflow; `upstream_rq_retry_backoff_exponential` / `_ratelimited`) are NOT emitted at phase-16 minimum-viable scope — they correspond to retry budgets + custom back-off (deferred per §4). They land on the fixture's `allowlist_envoy_only` (the 14.1/15 `allowlist_envoy_only`-for-deferred-names precedent). **§6.2 PLAN-writer empirically enumerates the exact Envoy-side retry-stat set** so the allow-list is complete.

### 2.2 "Response header" — `x-envoy-attempt-count` (projected; §6.2-verified)

Phase 16 adds a BEHAVIOR_CONTRACT row for the **`x-envoy-attempt-count` response header** (the existing `x-envoy-upstream-service-time` header machinery at `crates/envoy-http1/src/router.rs:54,151-155` + `crates/envoy-http2/src/hcm.rs:513-516` is the precedent for injecting an `x-envoy-*` header on the proxied response). Envoy adds `x-envoy-attempt-count` to the downstream response carrying the total number of upstream attempts (`1` if no retry, `2` after one retry). **§6.2-verifiable nuance:** whether Envoy emits the header only when a `retry_policy` is configured (vs always), and whether it ALSO injects the header on the upstream REQUEST (so the backend sees the attempt number). The minimum-viable scope emits it on the **downstream response** when a `retry_policy` is configured; the §6.2 PLAN-writer confirms the exact emit conditions + whether the request-side header is required for bilateral equivalence. The header is added to the fixture-0024 header assertions; the `server`/`date`/timing allow-list discipline (BEHAVIOR_CONTRACT "Header allow-list") continues to apply.

### 2.3 "Response body / response flag" — retried-away vs limit-exceeded wire shape (projected; §6.2-verified)

On the **success path** (503 retried to 200), the downstream response is the **second attempt's 200** verbatim — envoy-rust returns the successful attempt's response unmodified (body + headers), so no body reconciliation is needed beyond the existing proxied-response path. On the **limit-exceeded path**, envoy-rust returns the **last attempt's response** (the backend's final 503 + its body) — NOT a synthetic local reply (the retry-limit case surfaces the real upstream response, distinct from the no-healthy-upstream / overflow synth-503 paths). **§6.2-verifiable:** Envoy's access-log `%RESPONSE_FLAGS%` for the limit-exceeded case is `URX` (UpstreamRetryLimitExceeded) — confirm whether `URX` surfaces in any response header (it is primarily an access-log flag, not a response header — §6.2 confirms; if it does NOT surface as a header, no wire reconciliation is needed). A conditional reconciliation ADR is reserved as ADR-0045 (§7) if a material wire divergence is found.

### 2.4 DECISIONS.md amendment at SPEC time — ADR-0044 (the scoping ADR)

Like phase 15 (whose brainstorm landed ADR-0042 for the non-obvious "enforcement-already-landed" finding), phase 16's brainstorm DOES land an ADR: **ADR-0044** records (a) the non-obvious **body-replay feasibility finding** (H1 empty-body / H2 buffered-clone → retries are body-replay-safe with no new buffering machinery — the finding that makes a minimum-viable retry phase tractable), and (b) the minimum-viable scope boundary — deliver retry-on-status/connect-failure/reset + `num_retries` + `retriable_status_codes` + the 3-stat observability subset + `x-envoy-attempt-count` + fixture 0024; defer `per_try_timeout`, custom retry back-off intervals, retry budgets, retry-host-predicate/priority, request-header overrides, vhost-level policy, gRPC retry, and hedging. The ADR is justified because the body-replay finding is non-obvious (it determines feasibility) AND cold-readability (D-3.4) demands a future session understand WHY phase 16 needs no body-buffering machinery and WHERE the minimum-viable boundary sits. Conditional §6.2-reconciliation + split ADRs are enumerated in §7.

---

## 3. Deliverables

Phase 16's scope is enumerated as deliverables `D1`–`D9` below. **The state-2 PLAN-writer organizes deliverables into tasks AND evaluates the §6.1 split gate** (projected NOT to fire — single phase, but genuinely close). Deliverables are LISTED roughly in execution order; the SPEC constrains the surface, not the task organization.

### D1 — `envoy-config` schema extension (`RouteAction_Route.retry_policy`)

At `crates/envoy-config/src/bootstrap.rs`, extend the existing `RouteAction_Route` struct (currently `cluster: String` only, `bootstrap.rs:953-955`, with `#[serde(deny_unknown_fields)]`) with an `Option<RetryPolicy>` field, and add the `RetryPolicy` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteAction_Route {
    pub cluster: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    #[serde(default)]
    pub retry_on: String,                       // comma-separated condition tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_retries: Option<u32>,               // default 1 (Envoy default; §6.2-verified)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retriable_status_codes: Vec<u32>,       // used with the "retriable-status-codes" token
}
```

`deny_unknown_fields` continues to reject the still-deferred retry fields (`per_try_timeout`, `retry_back_off`, `rate_limited_retry_back_off`, `retry_priority`, `retry_host_predicate`, `host_selection_retry_max_attempts`, `retriable_headers`, `retriable_request_headers`, `retry_budget` (the latter lives under `circuit_breakers`)). The exact field names/types are §6.2-verified against the Envoy v1.33 `RetryPolicy` proto (the PLAN-writer confirms `num_retries` default + the `retriable_status_codes` repeated-u32 shape).

### D2 — `envoy-config` validator + `retry_on` token parsing

Add a `validate_retry_policy` (mirroring `validate_circuit_breakers` / `validate_outlier_detection`) that parses `retry_on` into a typed set of supported conditions. **Minimum-viable supported tokens:** `5xx`, `gateway-error`, `connect-failure`, `reset`, `retriable-status-codes`. **§6.2-verified posture for the UNKNOWN-token case** (Envoy silently ignores unrecognized `retry_on` tokens — confirm; the minimum-viable choice is to accept-and-ignore unknown tokens to match Envoy, OR reject with a `ConfigError` — the PLAN-writer locks the Envoy-faithful posture). New `ConfigError` variants for the rejected deferred fields are surfaced by `deny_unknown_fields`; any additional semantic rejections (e.g. `retriable-status-codes` token present but `retriable_status_codes` empty — §6.2 whether Envoy errors) get a dedicated variant. ~3–5 new `ConfigError` variants (PLAN-writer's count). Positive + negative parse-path unit tests per the 13.1/15 validator-test cadence. Exercised by the `parse_bootstrap` fuzz target (D8 extends the corpus).

### D3 — H1 retry loop (router-arm dispatch wrapping)

Wrap the H1 dispatch seam (`crates/envoy-http1/src/hcm.rs:465-706` — `pick_endpoint()` → `pool.acquire()` → `send_request()` → response receipt → `record_response()`) in a retry loop driven by the route's resolved `RetryPolicy`. Each iteration: re-`pick_endpoint()` (honors the 12.x health filter + 14.x ejection filter — a retry may land on a different healthy endpoint), re-`acquire()` from the pool, re-send the (empty-body, replayable per §0) request, receive + classify the response, and call `record_response()` (per-attempt — each attempt is a "response" for 14.x outlier purposes; §6.2 confirms Envoy records per-attempt). **Classification:** an attempt is retriable iff (the response status matches a `retry_on` condition: `5xx` → 500-599; `gateway-error` → 502/503/504; `retriable-status-codes` → the configured list) OR (a connect-failure matched `connect-failure` / a reset matched `reset`) AND (retries-so-far < `num_retries`). On a retriable outcome with budget remaining: increment `upstream_rq_retry`, apply the default back-off (D7), loop. Otherwise: return the response (success → also tick `upstream_rq_retry_success` if ≥1 retry happened; budget-exhausted-still-retriable → tick `upstream_rq_retry_limit_exceeded`). The retry state is a small `RetryState` helper (attempts counter + parsed conditions + `num_retries`). **The connect-failure synth-502 arm (`hcm.rs:666-688`) becomes a retriable outcome** when `retry_on` includes `connect-failure` (rather than immediately surfacing the synth-502).

### D4 — H2 retry loop (router-arm dispatch wrapping)

Mirror D3 on the H2 arm (`crates/envoy-http2/src/hcm.rs:238-481`). The H2 seam includes the protocol-dispatch fork (H1-protocol upstream at `:325-336` vs H2-protocol upstream at `:338-438`); the retry loop wraps the WHOLE pick→dispatch→receive arm so it is protocol-agnostic. The H2 request body is replayed via the existing buffered-`Bytes` clone (`:290`, per §0). Per-attempt `record_response()` at `:481`. Same classification + back-off + stat-tick logic as D3, factored to share the `RetryState` helper (live in a small shared module or duplicated per the H1/H2 sibling discipline — PLAN-writer's call; the 13.x/14.x H1/H2-sibling precedent).

### D5 — Retry observability stats (3-name minimum-viable subset)

Register `cluster.<name>.upstream_rq_retry`, `cluster.<name>.upstream_rq_retry_success`, `cluster.<name>.upstream_rq_retry_limit_exceeded` (the §2.1 subset) on the `Cluster` (next to the existing `upstream_rq_total` / `upstream_rq_5xx` at `crates/envoy-cluster/src/cluster.rs:106-111`), gated/inert per the 14.1/15 conditional-registration discipline (register only when any route to the cluster configures a `retry_policy`, OR register unconditionally but leave at 0 — §6.2/PLAN-writer picks the inert-discipline that keeps the 23 existing fixtures byte-identical; the safest is unconditional registration at 0 since the names are cluster-scoped and a route's retry config is not known at cluster-construct time — PLAN-writer confirms). Increment at the single-source-of-truth retry-loop sites (D3/D4). **Plus the §2.1 `upstream_rq_total`/`upstream_rq_5xx` per-attempt reconciliation** (the highest-risk item — confirm Envoy counts per-attempt and reconcile the existing increment sites without breaking the 0020/0022 byte-exact counter assertions; the non-retry fixtures must stay inert because they configure no `retry_policy` and so make exactly one attempt).

### D6 — Stateful fail-then-succeed synthetic-backend harness primitive

The new test primitive. The existing configurable-status backend (`tests/differential/src/backend.rs:265-294`, the health-aware-http1-backend with `--per-path PATH=STATUS`) is **stateless** (path → fixed status). A retry fixture needs a backend that returns `503` on the **first** request to a path and `200` on the **second** (so a single retry observably flips the outcome). Extend the backend with a stateful directive — e.g. `--retry-script PATH=fail:N` (return 503 for the first N requests to PATH, then 200) and/or `--fail-first N` — backed by a per-path atomic request counter. Reuses the existing helper-process + `--per-path` plumbing (so the stateless paths still work for the always-503 limit-exceeded path). **No new `Driver` variant is needed** — the retry is internal to the proxy, driven by a single downstream GET (success path) or a small sequential `Driver::Http1ProbeList` (success + limit-exceeded paths); the existing `Http1` / `Http1ProbeList` / `Http2` drivers suffice (the seam is the BACKEND, not the driver). **§6.2-verifiable:** confirm Envoy and envoy-rust both make exactly 2 attempts (1 retry) against the fail-once-then-succeed backend under `num_retries: 1` — i.e. the stateful backend's request count is 2 on the success path.

### D7 — Default retry back-off

Implement Envoy's default retry back-off between attempts (exponential with a base interval — §6.2-verified, Envoy default base is ~25ms with jitter). **Timing is NOT differentially asserted** (BEHAVIOR_CONTRACT "Timing tolerances" default: no opt-in), so a faithful-but-modest default back-off keeps fixture 0024 timing-robust (a single retry incurs ~25ms — negligible). The back-off is implemented for fidelity (not hammering the upstream) but the differential asserts only the final status + the retry counters, never the inter-attempt delay. **§6.2-verifiable:** the exact default base interval + whether jitter matters for any asserted observable (it should not, since timing is not compared). Custom `retry_back_off` config intervals are DEFERRED per §4 (the schema rejects the field).

### D8 — Fixture 0024 + Docker wrapper + in-process backstop + fuzz seed

- **D8.1 — Fixture `tests/fixtures/0024-upstream-retry-on-5xx/`.** Configures: an H1 upstream cluster (single-endpoint STRICT_DNS, the 04.3/13.x posture) with two routes (or one route + two backend paths): `/retry-success` → the stateful fail-once-then-succeed backend path; `/retry-exhausted` → an always-503 backend path. Route `retry_policy: { retry_on: "5xx", num_retries: 1 }`. Assertions — **success path:** single GET `/retry-success` → final `200` + body + `x-envoy-attempt-count: 2` + `upstream_rq_retry: 1` + `upstream_rq_retry_success: 1`; **limit-exceeded path:** GET `/retry-exhausted` → final `503` + the backend's 503 body + `upstream_rq_retry: 1` (cumulative `2` if both paths share a cluster — PLAN-writer reconciles the cumulative-counter arithmetic) + `upstream_rq_retry_limit_exceeded: 1`. Driven via `Driver::Http1ProbeList` (sequential, timing-robust). `allowlist_envoy_only` for the deferred Envoy-side `upstream_rq_retry_overflow`/`_backoff_*` names per §2.1.
- **D8.2 — `tests/differential/tests/upstream_retry.rs`** Docker-gated wrapper mirroring the 13.1/14.2/15 shape.
- **D8.3 — In-process backstop at `crates/envoy-bin/tests/upstream_retry.rs`**, mirroring the 13.1/14.2/15 backstop shape. Boots `envoy-bin` with a synthesized bootstrap + in-process stateful backend; exercises BOTH the success path (retry → 200, counters) AND the limit-exceeded path (retries exhausted → final 503, counters) — the 14.2/15 both-paths backstop discipline. Asserts `x-envoy-attempt-count` presence + the retry-counter values directly (timing-robust; no cross-proxy fragility).
- **D8.4 — Fuzz corpus seed.** Add `crates/envoy-config/fuzz/corpus/parse_bootstrap/route_retry_policy.yaml` (corpus 22 → 23) exercising the `retry_policy` schema + validator, OR extend an existing route seed in place — PLAN-writer's call. If a NEW seed file: edit `crates/envoy-config/fuzz/.gitignore` allow-list AND the `bootstrap.rs::tests::fuzz_corpus_seeds_parse_or_reject_cleanly` SUCCESS-array together (the 09/10/11/12.2/13.1/15 atomic-edit lesson).

### D9 — BEHAVIOR_CONTRACT extensions

Land the §2.1 stat rows (`upstream_rq_retry{,_success,_limit_exceeded}` + the `upstream_rq_total`/`upstream_rq_5xx` per-attempt-counting clarification) + the §2.2 `x-envoy-attempt-count` header row + the §2.3 retried-away/limit-exceeded wire-shape row at the task where each is first empirically exercised (the 06.x→15 contract-extension cadence — at engagement task time, NOT at PLAN-write, NOT at SPEC time).

---

## 4. Out of scope (deferred non-goals)

Phase 16 explicitly does NOT land:

- **`per_try_timeout` (per-attempt timeout).** A per-attempt deadline that converts a slow attempt into a retriable failure. Introduces timeout machinery + timing-sensitive fixtures; deferred to a follow-up retry phase. The schema rejects `per_try_timeout` (deny_unknown_fields). **Prime split-seam / follow-up candidate.**
- **Custom `retry_back_off` / `rate_limited_retry_back_off` intervals.** Phase 16 implements only Envoy's DEFAULT back-off (D7); operator-configured custom intervals defer. The schema rejects these fields.
- **Retry budgets (`retry_budget` under `circuit_breakers`) + `upstream_rq_retry_overflow`.** The retry circuit breaker (cap concurrent retries as a fraction of active requests). Ties into the deferred circuit-breaker-budget expansion (phase-15 deferral). Defers. (Phase 16 implements `num_retries` per-request only, no cross-request budget.)
- **`retry_priority` / `retry_host_predicate` / `host_selection_retry_max_attempts`.** Retry-time host-selection predicates (avoid the previously-tried host, priority shifting). Phase 16's re-pick uses the standard health/ejection-aware `pick_endpoint()` with no retry-specific host avoidance. Defers.
- **`x-envoy-retry-on` / `x-envoy-max-retries` / `x-envoy-retry-grpc-on` request-header overrides.** Per-request retry config via downstream request headers. Phase 16 is operator-config (route `retry_policy`) ONLY. Defers.
- **vhost-level `retry_policy` (vs route-level).** Envoy supports `retry_policy` at the virtual-host level inherited by routes. Phase 16 supports route-level only. Defers (the envoy-rust route model is route-level today).
- **`retriable_headers` / `retriable_request_headers` (header-match retry conditions).** Retry based on response/request header matches (reusing the 04.2 HeaderMatcher). Defers (phase 16's `retry_on` is status/connect/reset-based only).
- **gRPC retry (`retry_on: cancelled,deadline-exceeded,...` + `x-envoy-retry-grpc-on`).** Tied to the gRPC family. Defers.
- **Hedging (`hedge_policy`, `hedge_on_per_try_timeout`).** Concurrent speculative attempts. A distinct, more complex feature (needs concurrent in-flight attempts + first-wins racing). Defers — the §9 charter pairs "retries + hedging" but hedging is its own phase.

---

## 5. Architectural invariants

Phase 16 honors and extends the established cross-crate invariants:

### 5.1 No new crate, no new top-level Cargo dep

All work lands inside existing crates: `envoy-config` (schema + validator), `envoy-http1` + `envoy-http2` (the retry loop wrapping the existing `hcm.rs` dispatch seams + the `RetryState` helper), `envoy-cluster` (the 3 retry-stat handles next to the existing `upstream_rq_*`), `tests/differential` (the stateful backend extension + the fixture wrapper), `tests/helpers` (the stateful fail-then-succeed backend knob), `tests/fixtures` (0024), `crates/envoy-bin/tests` (backstop). **No new workspace member; no new top-level Cargo dep.** The back-off uses tokio primitives already pulled.

### 5.2 Inert-when-unconfigured (the foundation-slice discipline applied to the retry path)

The retry loop is a NO-OP when the route's `retry_policy` is absent: the loop makes exactly ONE attempt (byte-identical to today's single-attempt dispatch), and the 3 retry stats stay at 0 (or unregistered). The 23 existing fixtures (none configures a `retry_policy`) see byte-identical behavior and zero retry stats. Regression-equivalence (acceptance gate (b)) holds because no existing fixture asserts the retry names or `x-envoy-attempt-count`, and the per-attempt `upstream_rq_total`/`upstream_rq_5xx` reconciliation (D5) is a no-op when exactly one attempt is made.

### 5.3 One-source-of-truth stat sites (the 06.x→15 discipline)

`upstream_rq_retry` / `_success` / `_limit_exceeded` each increment at exactly ONE logical site per protocol (the retry-loop classification points). The PLAN-writer ensures no double-counting and that the per-attempt `upstream_rq_total`/`upstream_rq_5xx` reconciliation (D5) is single-source-of-truth and Envoy-faithful.

### 5.4 Body-replay posture (the §0 finding as an invariant)

Phase 16 retries are body-replay-safe BECAUSE H1 sends an empty body and H2 buffers+clones (§0). The retry loop re-sends the SAME captured request shape on each attempt — it does NOT re-read the downstream body (already drained on H1 / already buffered on H2). The fixture uses bodyless GETs (the bilaterally-safe topology). A future phase that forwards H1 request bodies (lifting the phase-04.3 empty-body scope) must revisit retry replay-ability at that time — flagged as a carryforward.

### 5.5 Re-pick honors health + ejection (the 12.x/14.x seam reuse)

Each retry iteration calls the SAME `pick_endpoint()` that excludes 12.x-unhealthy + 14.x-ejected endpoints — a retry never re-selects an endpoint the health/outlier machinery has excluded. Per-attempt `record_response()` continues to feed 14.x outlier detection (each attempt's status is recorded for the picked endpoint — §6.2 confirms Envoy's per-attempt recording). The single-endpoint fixture-0024 topology means every attempt re-picks the same endpoint (bilaterally simplest); multi-endpoint retry host-selection (the `retry_host_predicate` "avoid previous host" behavior) is DEFERRED per §4.

---

## 6. Implementation signposts for the planner

The state-2 PLAN-writer reads this section to drive PLAN structure.

### 6.1 Split-gate evaluation (READ FIRST — split projected NOT to fire, but genuinely close)

Per `BOOTSTRAP_PROMPT.md` §6.1, the state-2 PLAN-write evaluates whether the PLAN exceeds ~25 numbered tasks OR ~1500 LoC. Phase 16's surface estimate at SPEC time:

- D1 — schema (`RetryPolicy` struct + `retry_policy` field) (~40 LoC + ~60 LoC tests).
- D2 — validator + `retry_on` token parse (~80 LoC + ~120 LoC tests).
- D3 — H1 retry loop + `RetryState` helper (~140 LoC + ~120 LoC tests).
- D4 — H2 retry loop (~130 LoC + ~100 LoC tests).
- D5 — 3 retry stats + per-attempt `upstream_rq_total`/`5xx` reconciliation (~70 LoC + ~100 LoC tests).
- D6 — stateful fail-then-succeed backend knob (~80 LoC).
- D7 — default back-off (~30 LoC + ~30 LoC tests).
- D8.1 — fixture 0024 (YAML + expectations, 2 paths) (~140 LoC).
- D8.2 — Docker-gated wrapper (~50 LoC).
- D8.3 — in-process backstop (both paths) (~240 LoC).
- D8.4 — fuzz seed (~25 LoC + ≤2 file edits).
- D9 — BEHAVIOR_CONTRACT rows (~60 LoC docs).
- State-4 verification + STATE-advance (~docs).

**SPEC-time projection: ~12–16 tasks; ~1300–1600 LoC** (production ~470, tests ~600, fixture/harness/backstop ~510, docs ~120). **This is NEAR the §6.1 ~1500-LoC gate.** Phase 16 is projected SINGLE, but the gate is genuinely close — **the state-2 PLAN-writer re-estimates against the §6.2-refined surface and splits if it lands over ~1500 LoC / ~25 tasks.** The recommended seam: **`16.1`** (D1+D2+D3+D5(H1)+D7+D8.1(success path)+D8.4 — schema + validator + H1 retry loop + stats foundation + back-off + fixture 0024 success path + fuzz seed; regression-equivalence on the 23 existing fixtures via the inert-when-unconfigured pattern — the 13.1/14.1 foundation-slice precedent) / **`16.2`** (D4+D5(H2 reconciliation)+D8.1(limit-exceeded path)+D8.2+D8.3 + parent-16 close — H2 retry loop + the per-attempt counter reconciliation + the limit-exceeded coverage + the backstop). The split ADR would be **ADR-0046** (§7). **Projected single — the split is held in reserve.**

### 6.2 Empirical verification at state-2 PLAN-write (HEAVY for this phase)

Per the phase-10/11/12/13/14/15-ratified verify-at-PLAN-write process: **the state-2 PLAN-writer empirically verifies the upstream wire/behavior shapes BEFORE locking PLAN lock-ins.** Run `envoyproxy/envoy:v1.33.0` Docker with a route configuring `retry_policy: { retry_on: "5xx", num_retries: 1 }` + a fail-once-then-succeed backend + an always-503 backend + a `%RESPONSE_FLAGS%` access log + admin `/stats`, and verify:

1. **`retry_on` token semantics:** exact status sets — `5xx` (500-599?), `gateway-error` (502/503/504), `connect-failure`, `reset`; the `retriable-status-codes` + `retriable_status_codes` list interaction; the UNKNOWN-token posture (silently ignored vs error). (§3 D2.)
2. **Default `num_retries`** (Envoy default = 1?) + the exact `RetryPolicy` proto field names/types (`num_retries`, `retriable_status_codes` repeated-u32, `retry_on` string). (§3 D1.)
3. **Retry stat names + values:** `cluster.<name>.upstream_rq_retry`, `upstream_rq_retry_success`, `upstream_rq_retry_limit_exceeded` (values on the success path → expect 1/1/0; on the limit-exceeded path → 1/0/1); enumerate the FULL Envoy-side retry-stat set (`upstream_rq_retry_overflow`, `_backoff_*`) for the `allowlist_envoy_only`. (§2.1.)
4. **HIGHEST-RISK — per-attempt vs per-request counting:** does `cluster.<name>.upstream_rq_total` increment per upstream ATTEMPT (so +2 after one retry) or per downstream request (+1)? Does the intermediate retried-away 503 tick `upstream_rq_5xx` + the per-class `upstream_rq_5xx`/`downstream_rq_5xx`? Reconcile envoy-rust's existing increment sites (`router.rs:95-98` / `hcm.rs:473-476`) to match WITHOUT breaking the 0020/0022 byte-exact counter assertions (those fixtures make exactly 1 attempt → must stay inert). (§2.1 + §3 D5.)
5. **`x-envoy-attempt-count`:** emitted only when `retry_policy` configured? value = total attempts? on the downstream RESPONSE and/or the upstream REQUEST? (§2.2 / §3 D6.)
6. **Default back-off:** exact base interval (~25ms?) + whether jitter affects any asserted observable (it should not — timing not compared). (§3 D7.)
7. **Per-attempt `record_response` (14.x interaction):** does each failed attempt's status feed outlier detection (each attempt is a "response" for the picked endpoint)? Confirm envoy-rust's per-attempt `record_response` matches Envoy. (§5.5.)
8. **Limit-exceeded wire shape:** the final response on `num_retries` exhaustion is the LAST attempt's real upstream response (not a synth) — confirm; `%RESPONSE_FLAGS%` = `URX`; confirm `URX` does NOT surface as a response header (access-log only). (§2.3.)
9. **Backend attempt count:** confirm both proxies make exactly 2 attempts (1 retry) against the fail-once-then-succeed backend under `num_retries: 1` (the stateful backend's request counter reads 2). (§3 D6.)

Each finding lands as a PLAN lock-in. **If finding 1, 4, or 8 differs materially from the SPEC projection, the lock-in records the divergence + the SPEC §2.x revision via an inline ADR at the state-2 PLAN-write commit** (mirrors phase-12 ADR-0037 / phase-14 ADR-0041 / phase-15 ADR-0043). The reserved number is **ADR-0045** (§7).

### 6.3 In-process backstop assertions (heeds the 14.2/15 both-paths lesson)

D8.3 SHOULD exercise BOTH retry paths (success → retried-to-200 + counters; limit-exceeded → final-503 + counters) — the backstop observes the counters + `x-envoy-attempt-count` directly without cross-proxy timing fragility (the 14.2 both-convergence-directions / 15 both-overflow-paths discipline).

### 6.4 The 06.x stats convention + the inert-when-unconfigured discipline

StatsRegistry registration for the 3 retry counters at cluster-construct time; per-`Cluster` ownership of the Counter handles next to `upstream_rq_total`/`upstream_rq_5xx`; the increment sites single-source-of-truth (§5.3). The per-attempt `upstream_rq_total`/`5xx` reconciliation (D5) is the load-bearing inert-when-unconfigured item — a single-attempt request must count EXACTLY as today.

### 6.5 Pre-state-4 fmt + clippy discipline (heeds `project_state3_arc_skips_clippy`)

Per-task PROGRESS sections quote `cargo fmt --all -- --check` AND `cargo clippy --workspace --all-targets --all-features -- -D warnings` at every PROGRESS-task close — NOT just at state-4. The phase-15 state-3 arc ran build/test/fmt but NOT clippy, so 8 `collapsible_if` lints first surfaced at the state-4 gate (memory `project_state3_arc_skips_clippy`). Phase 16's retry-loop control flow (nested `if`/`match` on classification) is a `collapsible_if`/`match`-lint candidate — run clippy per task.

### 6.6 State-4 evidence-discipline (continues per 05.3 → … → 15 chain)

Per-gate quoted evidence in PROGRESS at the state-4 verification task: real CI run URL + HEAD SHA + completion timestamp + per-gate quoted output (5 stable-toolchain gates + each Docker-gated fixture + h2spec_pass_rate_gate + parse_bootstrap fuzz iteration count).

### 6.7 Isolated-crate build discipline (heeds `project_isolated_crate_build_blindspot` / the 14.2 state-5 I-1 finding)

`cargo build --workspace` (the §7.5 gate) can be GREEN while `cargo build -p <crate>` FAILS — feature unification across the workspace masks a missing per-crate feature. Phase 16 touches `envoy-config`, `envoy-cluster`, `envoy-http1`, `envoy-http2`. **The state-4 verification MUST run `cargo build -p envoy-config`, `-p envoy-cluster`, `-p envoy-http1`, `-p envoy-http2` STANDALONE** (in addition to the workspace build). Quote each standalone build in PROGRESS.

### 6.8 Cargo.lock cadence

The phase-04.1 REVIEW M5/M9 Cargo.lock-cadence ADR carries forward. Phase 16 adds zero new top-level Cargo deps.

### 6.9 PLAN.md + PROGRESS.md skeleton + Task 1 preamble land alongside at state-2

Per the 06.2 → … → 15 cadence. State-2 PLAN-write lands `PLAN.md` + `PROGRESS.md` skeleton + Task 1 preamble in a single standalone pre-Task-1 commit (or, on split, the sub-phase SPECs + ROADMAP + STATE advance + ADR-0046).

### 6.10 Subagent-driven execution at state 3 (per `feedback_execution_style`)

State 3 implementation is subagent-driven (`superpowers:subagent-driven-development`), implementers dispatched SERIALLY (`feedback_serial_subagent_dispatch`) — not parallel (they race on `main`). Not engaged at this state-1 brainstorm.

---

## 7. Conditional ADRs (projected; land at PLAN-write or in-execution if they fire)

- **ADR-0044 (LANDED at this brainstorm commit) — phase-16 minimum-viable retry scope + the body-replay feasibility finding.** Records: H1 sends an empty body / H2 buffers-and-clones → retries are body-replay-safe with no new buffering machinery (the feasibility finding); phase 16 delivers retry-on-status/connect-failure/reset + `num_retries` + `retriable_status_codes` + the 3-stat observability subset + `x-envoy-attempt-count` + fixture 0024; defers `per_try_timeout`, custom back-off intervals, retry budgets/`upstream_rq_retry_overflow`, retry-host-predicate/priority, request-header overrides, vhost-level policy, gRPC retry, and hedging. (This is the ONLY ADR landed at the brainstorm; the cadence mirrors phase-15's ADR-0042 — justified by the non-obvious feasibility finding.)
- **Conditional ADR-0045 (PLAUSIBLE) — §6.2 empirical-verification revision.** Fires if §6.2 finding 1 (`retry_on` token semantics / unknown-token posture), finding 4 (per-attempt vs per-request `upstream_rq_total`/`5xx` counting), or finding 8 (limit-exceeded wire shape) diverges materially from the §2.x projection. Mirrors ADR-0037 / ADR-0041 / ADR-0043. Lands at the state-2 PLAN-write commit if it fires. **Finding 4 (per-attempt counting) is the most likely trigger.**
- **Conditional ADR-0046 (POSSIBLE) — phase split.** Fires if the state-2 LoC estimate exceeds ~1500 (§6.1 — genuinely close). If it fires, the seam is `16.1` (schema + validator + H1 retry loop + stats foundation + back-off + fixture-0024 success path) / `16.2` (H2 retry loop + per-attempt counter reconciliation + limit-exceeded coverage + parent close).

**ADR ledger at SPEC time:** DECISIONS.md head is ADR-0043 (count 44); this SPEC's commit lands **ADR-0044** (count 45; next available ADR-0045). **ADR-0028** (H1-listener × H2-cluster dispatch deferral) REMAINS OPEN — phase 16 does not engage it.

---

## 8. Summary

Phase 16 is the fifth Upstream-robustness-family phase. It lands the **HTTP retry policy**: a router-arm retry loop (H1 + H2) that re-dispatches a failed upstream attempt on a configured `retry_on` condition (`5xx`/`gateway-error`/`connect-failure`/`reset`/`retriable-status-codes`) up to `num_retries` times, re-picking a health/ejection-aware endpoint each time and replaying the (already-drained-empty on H1 / already-buffered-and-cloned on H2 — §0) request; three minimum-viable retry stats (`upstream_rq_retry{,_success,_limit_exceeded}`); the `x-envoy-attempt-count` response header; a new STATEFUL fail-then-succeed synthetic-backend harness primitive; and fixture 0024 proving both the retried-to-200 success path and the retry-limit-exceeded path bilaterally on a single timing-robust sequential driver. The body-replay feasibility finding + the minimum-viable scope boundary are ratified in ADR-0044 at this brainstorm commit; the §6.2 wire-shape verification (especially the per-attempt `upstream_rq_total`/`5xx` counting reconciliation) + the conditional reconciliation/split ADRs are reserved for the next session's PLAN-write. Projected single un-split phase (~1300–1600 LoC), with a thin H1-then-H2 split (16.1/16.2) held in reserve.
