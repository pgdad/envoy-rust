# Phase 50 — `50-accesslog-rf-overflow` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Every task is `superpowers:test-driven-development` (RED → GREEN → commit). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Differentially witness the THIRD non-`-` `%RESPONSE_FLAGS%` value — `UO` (UpstreamOverflow) — BYTE-EXACT on the circuit-breaker overflow 503 path, by discriminating the overflow outcome at envoy-rust's H1 retry-loop result-consumption site and extending the phase-48/49 `%RESPONSE_FLAGS%` derive by one arm.

**Architecture:** `%RESPONSE_FLAGS%` is derived 1:1 from the per-request `%RESPONSE_CODE_DETAILS%` at the unified H1 access-log record-build site (`crates/envoy-http1/src/hcm.rs:1237`, reading the var built across the dispatch path). Today the overflow 503 path renders `rcd:"via_upstream"` / `rf:"-"` — WRONG (the overflow path is not a real upstream response). This phase (§A) sets `response_code_details_for_log = Some("upstream_reset_before_response_started{overflow}")` for the overflow outcome at the two overflow sites, and (§B) adds the exact-string derive arm `Some("upstream_reset_before_response_started{overflow}") => "UO"`. Additive: every existing fixture `0001`-`0057` stays byte-identical.

**Tech Stack:** Rust (workspace), `tokio`, `envoy-http1` HCM, `envoy-accesslog` (FileSink + CompiledJsonFormat), `differential` test crate (testcontainers + upstream Envoy `v1.33.0`).

**Scope lock:** ADR-0107. NO new `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant. `#![forbid(unsafe_code)]` holds. Projected ~6 tasks / ~70-120 LoC → §6.1 split does NOT fire (ADR-0108 stays reserved-but-unfired).

---

## Pre-flight: state confirmation (already done at PLAN-write; re-confirm at impl start)

- `git status` clean; `HEAD` at the phase-50 state-1 brainstorm commit (or later).
- `docs/envoy-rust/phases/50-accesslog-rf-overflow/SPEC.md` present; this `PLAN.md` is the state-2 output.
- **Concurrency guard (memory `concurrent-loop-sessions-race-on-phase-pick`):** before any commit, re-run `git status --porcelain`; if a sibling already advanced STATE or wrote files, defer.

## §6.2 recon — PLAN-VERIFY results (all CONFIRMED against the tree at PLAN-write)

1. **`outcome: None` appears at EXACTLY two `AttemptResult` constructions** — `hcm.rs:439` (no-healthy, `endpoint: None`) and `hcm.rs:640` (overflow, `endpoint: Some`). Within the `if let Some(endpoint) = attempt.endpoint` branch (`:990`), the four reachable outcomes are `Some(Response)` (`:600`), `Some(Reset)` (`:620`), `Some(ConnectFailure)` (`:629`), and `None` (`:640`) → **`attempt.outcome.is_none()` ⟺ pool-overflow** within that branch. `endpoint` is `Option<SocketAddr>` (Copy) so the `if let Some(endpoint)` does NOT move `attempt`; `attempt.outcome.is_none()` is a shared borrow safe to read inside the branch and the outlier-detection block at `:1019` re-reads `attempt.endpoint` afterward.
2. **Both pool-overflow arms collapse to one outcome.** `PoolError::Overflow`/`max_connections` (`hcm.rs:503`→`:508`) and `PoolError::PendingOverflow`/`max_pending_requests` (`hcm.rs:510`→`:515`) BOTH return `AcquireOutcome::Overflow(synth_overflow(close))` → the single `endpoint:Some`/`outcome:None` `AttemptResult` at `:632-643`. So ONE discriminator at `:995` covers both. Fixture `0058` exercises the `PendingOverflow`/`:515` arm.
3. **Third `synth_overflow` call site = the pre-route request-budget arm.** `hcm.rs:923` (`BudgetAcquisition::Rejected`/`max_requests`, the `:911-934` block) calls `synth_overflow(close)` and assigns `outgoing` directly, BYPASSING the retry loop — so `response_code_details_for_log` stays at its `None` init (`:844`) → renders `null`/`"-"` today. The retry-BUDGET `Rejected` exit (`hcm.rs:~1066`) surfaces the prior attempt's response VERBATIM and does NOT call `synth_overflow` — no retry-overflow site to guard. **DECISION (resolves SPEC §3.1): tag BOTH the pool arms (via the `:995` discriminator) AND the budget arm (`:923`) for a uniformly-1:1 derive** (the SPEC's twice-stated recommended default). The budget arm is in-process-backstopped (Task 3) but NOT differentially witnessed by `0058` → its differential witness is deferred as new carry-forward **M50-C**.
4. **(state-0 recon, locked)** Live `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`) at the `0023` overflow topology + `{rc,rcd,rf}` json_format + `GET /` → byte-stable `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`. `UO` is a clean brace-free deterministic constant; the rcd brace content (`overflow`) is a fixed reset-reason enum → byte-exact cross-proxy.
5. **Driver/probe wiring** — `http1_access_log_byte_exact` already drives a `GET /` probe with `expected_status: 503` (proven by `0053`/`0057`). No harness change. **Topology for `0058`:** STATIC `backend_cluster`, single LITERAL dead endpoint `127.0.0.1:1` (NEVER dialed — the pending-gate rejects before connect), `circuit_breakers.thresholds:[{max_connections:1, max_pending_requests:0}]`. This matches the state-0 recon topology AND the `0057` "no backend spawned, byte-identical configs" pattern (a literal address, not a `{{BACKEND_*}}` marker).
6. **(M50-A folded)** §F's pooled-overflow backstop wires `pool_mgr: Some(...)` via the production harness shape at `hcm.rs:5976-6018` (`H1PoolManager::for_bootstrap` + `cluster_mgr` from the same bootstrap) with a `circuit_breakers` block; the apter unit reference for the pool reject is `acquire_rejects_with_pending_overflow_when_max_pending_requests_zero` (`crates/envoy-http1/src/pool.rs:801`), NOT `synth_overflow_emits_81_byte_body_and_x_envoy_overloaded` (`:5082`, which calls `synth_overflow` directly with no pool).
7. **(M50-B folded) Additive byte-preservation — exhaustive `%RESPONSE_CODE_DETAILS%`/`%RESPONSE_FLAGS%`-logging fixture list:** `0012`, `0040`, `0046`, `0050`, `0051`, `0052`, `0053`, `0054`, `0055`, `0056`, `0057` (the `0051`/`0052` inclusion is the M50-B fix). NONE drives a circuit-breaker overflow 503; the overflow trigger fixture `0023` logs NO access log. So the `:995` `via_upstream`→overflow discriminator changes ZERO existing-fixture rcd (every other `endpoint:Some` outcome is `Some(Response)` → still `via_upstream`), and the `:923` budget tag + the `UO` derive arm change ZERO existing-fixture bytes → `0001`-`0057` byte-identical. The `:1238`/`:1239` `NR`/`UH` arms are untouched → `0056`/`0057` byte-identical.

## File structure

- **`crates/envoy-http1/src/hcm.rs`** (modify) — three edits + comment fixes:
  - §A pool-overflow discriminator at the retry-loop consumption site (`:990-1002`, replacing the unconditional `via_upstream` at `:995`).
  - §A′ budget-arm tag at `:923` (one assignment after `outgoing = synth_overflow(close);`).
  - §B derive arm at the record-build site (`:1237`) + the explanatory comment block (`:1225-1242`); fold M49-2 (host-miss `:1536`→`:1553`, route-miss `:1555`→`:1572`) + M49-3 (keep the `:1225` anchor) citation fixes.
  - Three new in-process backstop tests in the `#[cfg(test)] mod tests` block (model on `h1_no_healthy_access_log_carries_uh_flag` at `:5380` + the production pool harness at `:5976`).
- **`tests/fixtures/0058-accesslog-rf-overflow/`** (create) — `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`.
- **`tests/differential/tests/access_log_rf_overflow.rs`** (create) — structural clone of `access_log_rf_no_healthy.rs` → `0058`.
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** (modify) — §E: the `%RESPONSE_FLAGS%` row (`:1020`), the `%RESPONSE_CODE_DETAILS%` row (`:1031`), the overflow circuit-breaker row (`:37`, re-anchor `:542`/`:569`→`:503`/`:510` per M49-3).

---

## Task 1: §A pool-overflow discriminator + §B derive arm (pool-overflow in-process backstop)

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` (new test `h1_pool_overflow_access_log_carries_uo_flag` in `mod tests`)
- Modify: `crates/envoy-http1/src/hcm.rs:990-1002` (§A), `crates/envoy-http1/src/hcm.rs:1225-1242` (§B + comment fixes)

- [ ] **Step 1: Write the failing pooled-overflow backstop test.** Add to the `#[cfg(test)] mod tests` block (place it adjacent to `h1_no_healthy_access_log_carries_uh_flag`, ~`:5461`). Model the `pool_mgr: Some(...)` wiring on the production harness at `:5976-6018` (build `bootstrap` → `cluster_mgr` → `H1PoolManager::for_bootstrap` → `pool_mgr: Some(...)`), but give the cluster a `circuit_breakers` block and a dead endpoint, and add a `{rc,rcd,rf}` FileSink. Reference `acquire_rejects_with_pending_overflow_when_max_pending_requests_zero` (`pool.rs:801`) for the reject semantics.

```rust
    /// Phase 50 (ADR-0107) §F backstop: drive the FULL H1 dispatch path with a
    /// CONFIGURED pool (`pool_mgr: Some`) whose cluster carries
    /// `circuit_breakers.thresholds:[{max_connections:1, max_pending_requests:0}]`
    /// and a single dead endpoint (`127.0.0.1:1`, never dialed). The first
    /// connect-on-miss is rejected with `PoolError::PendingOverflow` → the
    /// `AcquireOutcome::Overflow` → `AttemptResult{endpoint:Some, outcome:None}`
    /// consumed at the retry-loop site (hcm.rs:990) → the overflow synth-503.
    /// Asserts the FILE json access-log line carries the overflow detail and the
    /// derived UO flag — the sole in-process proof of §A's outcome discriminator
    /// + §B's derive arm on the POOL-overflow path. Fail-first: pre-change it
    /// renders `"rcd":"via_upstream","rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_pool_overflow_access_log_carries_uo_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt)
                .await
                .expect("open FileSink"),
        );
        // STATIC cluster, dead endpoint 127.0.0.1:1 (never dialed), circuit
        // breakers max_connections:1 / max_pending_requests:0 → the
        // connect-on-miss pending-gate rejects the first request.
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_pending_requests: 0
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 1 } } }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let cluster_mgr = Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::clone(&registry))
                .await
                .expect("cluster mgr"),
        );
        let pool_token = tokio_util::sync::CancellationToken::new();
        let pool_mgr = crate::pool::H1PoolManager::for_bootstrap(
            &bootstrap,
            &cluster_mgr,
            Arc::clone(&registry),
            pool_token.clone(),
        )
        .expect("pool manager builds");
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: Arc::clone(&cluster_mgr),
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: Some(Arc::clone(&pool_mgr)),
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        // The overflow synth-503 + 81-byte body must be UNCHANGED.
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "overflow synth-503 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.ends_with(
                "upstream connect error or disconnect/reset before headers. reset reason: overflow"
            ),
            "overflow synth-503 body unchanged: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n",
            "pool-overflow access-log line carries the overflow rcd + rf:\"UO\": {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the test to verify it FAILS.**

Run: `cargo test -p envoy-http1 h1_pool_overflow_access_log_carries_uo_flag -- --nocapture`
Expected: FAIL on the final `assert_eq!` — `logged` is `{"rc":503,"rcd":"via_upstream","rf":"-"}\n` (the pre-change behavior). (If it fails earlier — e.g. the bootstrap/pool does not build, or the status is not 503 — STOP and `superpowers:systematic-debugging`; the harness wiring must be correct before the assertion is the failing point.)

- [ ] **Step 3: Implement §A — the pool-overflow outcome discriminator.** In `crates/envoy-http1/src/hcm.rs`, replace line `:995` (`response_code_details_for_log = Some("via_upstream".to_owned());`) inside the `if let Some(endpoint) = attempt.endpoint {` branch (`:990`):

```rust
                            if let Some(endpoint) = attempt.endpoint {
                                // 06.2 Task 6: capture the resolved upstream endpoint
                                // for the access-log `%UPSTREAM_HOST%` token (last
                                // attempt's endpoint wins). Skipped on pick()->None.
                                upstream_host_for_log = Some(endpoint.to_string());
                                // phase 50 (ADR-0107): discriminate the pool-overflow
                                // outcome (endpoint:Some + outcome:None — UNIQUELY the
                                // AcquireOutcome::Overflow result, hcm.rs:640; success
                                // :600 / reset :620 / connect-fail :629 all carry a
                                // non-None outcome) from a real upstream response. The
                                // overflow path is NOT a real upstream response →
                                // Envoy emits %RESPONSE_CODE_DETAILS% =
                                // "upstream_reset_before_response_started{overflow}"
                                // / %RESPONSE_FLAGS% = "UO" (state-0 recon); the
                                // derive at :1237 maps the detail => "UO". Covers BOTH
                                // pool arms (max_connections :503/:508 +
                                // max_pending_requests :510/:515). All other
                                // endpoint:Some outcomes keep "via_upstream"
                                // (byte-identical to pre-phase-50).
                                response_code_details_for_log = Some(
                                    if attempt.outcome.is_none() {
                                        "upstream_reset_before_response_started{overflow}"
                                    } else {
                                        "via_upstream"
                                    }
                                    .to_owned(),
                                );
                            } else {
```

(Leave the `else` no-healthy branch at `:996-1002` UNCHANGED.)

- [ ] **Step 4: Implement §B — the derive arm + comment fixes (M49-2 / M49-3).** At `crates/envoy-http1/src/hcm.rs:1237`, add the third arm and update the comment block (`:1225-1242`) to document the new arm AND fix the drifted `synth_404` citations:

```rust
                // phase 48 (ADR-0105) / phase 49 (ADR-0106) / phase 50 (ADR-0107):
                // %RESPONSE_FLAGS% is derived 1:1 from the per-request
                // %RESPONSE_CODE_DETAILS%:
                //   route_not_found     → NR (NoRoute)          — the two no-route
                //                          synth_404 arms (host-miss :1553 +
                //                          route-miss :1572).
                //   no_healthy_upstream → UH (NoHealthyUpstream) — the single
                //                          pick()->None no-healthy synth-503 arm
                //                          (:1000-1001).
                //   upstream_reset_before_response_started{overflow}
                //                       → UO (UpstreamOverflow) — the overflow
                //                          synth-503: both pool arms (the
                //                          outcome:None discriminator at :995) and
                //                          the request-budget arm (:923).
                // Each detail is set ONLY on its own arm(s) → each is 1:1 with
                // its flag. All other paths keep the "-" no-flags sentinel. Read
                // by-ref here; `response_code_details_for_log` is moved into the
                // `response_code_details:` field below.
                response_flags: match response_code_details_for_log.as_deref() {
                    Some("route_not_found") => "NR",
                    Some("no_healthy_upstream") => "UH",
                    Some("upstream_reset_before_response_started{overflow}") => "UO",
                    _ => "-",
                }
                .to_owned(),
```

- [ ] **Step 5: Run the backstop + the existing flag backstops to verify GREEN + no regression.**

Run: `cargo test -p envoy-http1 access_log_carries -- --nocapture`
Expected: `h1_pool_overflow_access_log_carries_uo_flag` PASS; `h1_no_healthy_access_log_carries_uh_flag` PASS (unchanged); `h1_route_miss_access_log_carries_route_not_found_rcd` PASS (unchanged). Then run the broader unit suite for safety:
Run: `cargo test -p envoy-http1`
Expected: all PASS (the `synth_overflow`/pool-overflow/no-healthy/route-miss unit tests are byte-unaffected).

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 50 T1: UO on the pool-overflow path — §A outcome discriminator + §B derive arm [ADR-0107]"
```

---

## Task 2: §A′ budget-arm UO tag (request-budget-overflow in-process backstop)

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` (new test `h1_request_budget_overflow_access_log_carries_uo_flag`)
- Modify: `crates/envoy-http1/src/hcm.rs:923` (one assignment)

- [ ] **Step 1: Write the failing budget-overflow backstop test.** Add to `mod tests` near `request_budget_overflow_max_requests_zero` (`:6878`). Use `cluster_mgr_with_endpoint_max_requests("backend", port, 0)` for the `max_requests:0` cluster, `pool_mgr: None` (the budget arm bypasses the pool), and a `{rc,rcd,rf}` FileSink. Build the `HCMConfig` inline (model on `h1_no_healthy_access_log_carries_uh_flag` at `:5409`, swapping the cluster_mgr + routing to `"backend"` with `metadata_match: None`).

```rust
    /// Phase 50 (ADR-0107) §A′ backstop: the pre-route request-budget overflow
    /// (`max_requests:0`, BudgetAcquisition::Rejected at hcm.rs:911) calls
    /// synth_overflow at :923 and BYPASSES the retry loop, so it is tagged
    /// directly at :923 (not via the :995 discriminator). Asserts the FILE json
    /// access-log line carries the overflow rcd + rf:"UO" — the in-process proof
    /// for the budget arm (its differential witness is deferred: M50-C).
    /// Fail-first: pre-change it renders `"rcd":null,"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_request_budget_overflow_access_log_carries_uo_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // A live backend port is required for cluster_mgr_with_endpoint_max_requests;
        // it is NEVER contacted (the budget gate fires before any dispatch).
        let (port, _reqs) = spawn_fail_then_ok_upstream(200, 0).await;
        let (cluster_mgr, _registry) =
            cluster_mgr_with_endpoint_max_requests("backend", port, 0).await;
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
        );
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
        map.insert(
            "rf".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_FLAGS%".to_string()),
        );
        let fmt = envoy_accesslog::CompiledJsonFormat::from_map(&map).expect("valid json_format");
        let sink = Arc::new(
            envoy_accesslog::FileSink::new(log_path.clone(), fmt)
                .await
                .expect("open FileSink"),
        );
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr,
            http2_protocol_options: None,
            stats: mk_stats("ingress_http"),
            access_log: vec![sink],
            filter_pipeline: test_router_only_pipeline(),
            pool_mgr: None,
            route_config: RwLock::new(Arc::new(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "default".to_string(),
                    domains: vec!["*".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::Route(RouteAction_Route {
                            cluster: "backend".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "request-budget overflow synth-503 status unchanged: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n",
            "request-budget overflow access-log line carries the overflow rcd + rf:\"UO\": {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the test to verify it FAILS.**

Run: `cargo test -p envoy-http1 h1_request_budget_overflow_access_log_carries_uo_flag -- --nocapture`
Expected: FAIL on the final `assert_eq!` — `logged` is `{"rc":503,"rcd":null,"rf":"-"}\n` (the budget arm leaves rcd at its `None` init). (If `cluster_mgr_with_endpoint_max_requests` / `spawn_fail_then_ok_upstream` / `hcm_config_*` helper names differ, re-grep `mod tests` and adapt; if the status is not 503, STOP → `superpowers:systematic-debugging`.)

- [ ] **Step 3: Implement §A′ — the budget-arm tag.** In `crates/envoy-http1/src/hcm.rs`, immediately after `outgoing = synth_overflow(close);` (`:923`) inside the `if let envoy_config::BudgetAcquisition::Rejected = request_acquire {` block, add:

```rust
                        // phase 50 (ADR-0107): the request-budget (max_requests)
                        // overflow is the SAME UO/overflow disposition as the pool
                        // arms — same synth_overflow helper, same 503 wire shape.
                        // Tag the rcd so the :1237 derive maps it => "UO". This arm
                        // BYPASSES the retry loop, so it is tagged HERE (not via the
                        // :995 discriminator). In-process-backstopped (M50-C: its
                        // differential witness is deferred — 0058 exercises only the
                        // pool PendingOverflow arm).
                        response_code_details_for_log =
                            Some("upstream_reset_before_response_started{overflow}".to_owned());
```

- [ ] **Step 4: Run the test to verify it PASSES + no regression.**

Run: `cargo test -p envoy-http1 overflow -- --nocapture`
Expected: `h1_request_budget_overflow_access_log_carries_uo_flag` PASS; `h1_pool_overflow_access_log_carries_uo_flag` PASS; `request_budget_overflow_max_requests_zero` + `synth_overflow_emits_81_byte_body_and_x_envoy_overloaded` PASS (the 503/body/headers/stats are byte-unaffected — only the access-log var changed).

- [ ] **Step 5: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 50 T2: UO on the request-budget overflow path — §A' :923 tag [ADR-0107]"
```

---

## Task 3: Fixture `0058-accesslog-rf-overflow` + differential test

**Files:**
- Create: `tests/fixtures/0058-accesslog-rf-overflow/envoy.yaml`
- Create: `tests/fixtures/0058-accesslog-rf-overflow/envoy-rust.yaml`
- Create: `tests/fixtures/0058-accesslog-rf-overflow/expectations.yaml`
- Create: `tests/fixtures/0058-accesslog-rf-overflow/README.md`
- Create: `tests/differential/tests/access_log_rf_overflow.rs`

- [ ] **Step 1: Create `envoy-rust.yaml`.** Merge the `0057` access-log/listener shape with the `0023` overflow cluster (STATIC, dead `127.0.0.1:1`, `max_connections:1`/`max_pending_requests:0`). Use the 3-key `json_format {rc, rcd, rf}` (matches the state-0 recon line exactly).

```yaml
node: { id: envoy-rust-phase-50-fixture-0058, cluster: envoy-rust-phase-50 }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0058-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: overflow_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend_cluster }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # STATIC cluster carrying circuit_breakers max_connections:1 /
    # max_pending_requests:0. The single endpoint is the LITERAL unreachable
    # 127.0.0.1:1 — NEVER dialed: the connect-on-miss pending-gate rejects the
    # first request with the overflow synth-503 BEFORE any connect. A literal
    # address (not a {{BACKEND_*}} marker) keeps both configs byte-identical
    # with NO backend spawned (the 0057 pattern).
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_pending_requests: 0
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
```

- [ ] **Step 2: Create `envoy.yaml`** — byte-identical to `envoy-rust.yaml` EXCEPT the `node` line and the access-log mount path (`/tmp/0058-envoy-mount/access.log`). Match the `0057` envoy.yaml's node/admin shape (compare `tests/fixtures/0057-accesslog-rf-no-healthy/envoy.yaml` for the exact upstream-side preamble — admin block, any `bootstrap`-required keys). Keep the cluster + listener + json_format identical to `envoy-rust.yaml`.

- [ ] **Step 3: Create `expectations.yaml`** — clone `tests/fixtures/0057-accesslog-rf-no-healthy/expectations.yaml`, swap the mount paths to `0058`, swap the comment to describe the overflow path, and keep the single probe:

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0058-envoy-mount/access.log
    envoy_rust: /tmp/0058-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `backend_cluster`, whose circuit_breakers
    # set max_connections:1 / max_pending_requests:0. The connect-on-miss
    # pending-gate rejects the first request → the overflow synth-503
    # (`…reset reason: overflow`) BEFORE the dead 127.0.0.1:1 endpoint is dialed.
    # THIRD non-`-` %RESPONSE_FLAGS% witness: UO (UpstreamOverflow) (phase 50,
    # ADR-0107), AND the FIRST witness of the overflow %RESPONSE_CODE_DETAILS%.
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). The overflow
    # synth-503 is deterministic on BOTH sides; envoy-rust DERIVES
    # %RESPONSE_FLAGS% = UO from %RESPONSE_CODE_DETAILS% =
    # `upstream_reset_before_response_started{overflow}` at the H1 record-build
    # site (hcm.rs:1237; was the no-flags sentinel `-` / `via_upstream`).
    # state-0 recon (live v1.33.0): {"rc":503,
    # "rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}.
    #   rc:  "%RESPONSE_CODE%"          → 503  (json NUMBER)
    #   rcd: "%RESPONSE_CODE_DETAILS%"  → "upstream_reset_before_response_started{overflow}"
    #   rf:  "%RESPONSE_FLAGS%"         → "UO"
    # Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf. Compact
    # separators + ONE trailing `\n` (ADR-0092 §E). Emitted line:
    #   {"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `README.md`** — clone the `0057` README structure; describe: the overflow topology, the `UO` witness (third `%RESPONSE_FLAGS%` value), the FIRST overflow-`rcd` witness, that the endpoint is never dialed, and the pure-cross-proxy-equality assertion. Note the M50-C deferral (the request-budget arm is NOT exercised here).

- [ ] **Step 5: Create the differential test `tests/differential/tests/access_log_rf_overflow.rs`** — structural clone of `access_log_rf_no_healthy.rs`:

```rust
//! Docker-gated differential test for fixture 0058-accesslog-rf-overflow.
//! Phase 50 (ADR-0107) — the THIRD non-`-` `%RESPONSE_FLAGS%` witness: `UO`
//! (UpstreamOverflow), BYTE-EXACT cross-proxy on the circuit-breaker overflow
//! 503 path, AND the FIRST witness of the overflow `%RESPONSE_CODE_DETAILS%`
//! (`upstream_reset_before_response_started{overflow}`). A STATIC cluster with
//! `circuit_breakers.thresholds:[{max_connections:1, max_pending_requests:0}]`
//! and a single dead endpoint (`127.0.0.1:1`, never dialed): the connect-on-miss
//! pending-gate rejects the first `GET /` → the overflow synth-503 BEFORE any
//! connect. envoy-rust now sets `%RESPONSE_CODE_DETAILS%` =
//! `upstream_reset_before_response_started{overflow}` at the retry-loop
//! consumption site (the `outcome:None` overflow discriminator, hcm.rs:995) and
//! DERIVES `%RESPONSE_FLAGS%` = `UO` from it (hcm.rs:1237; was `via_upstream`/`-`).
//! Upstream Envoy v1.33 emits the same here (state-0 recon:
//! {"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /` probe, `expected_status:
//! 503`, json_format {rc, rcd, rf}); asserts the emitted JSON line is
//! byte-identical. PURE cross-proxy equality (deterministic on both sides).
//! H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_overflow() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0058-accesslog-rf-overflow");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 6: Rebuild the DEBUG envoy-bin (memory `differential-harness-uses-debug-envoy-bin`) and run the differential if Docker is available.**

Run: `cargo build -p envoy-bin` then `cargo test -p differential --test access_log_rf_overflow -- --nocapture`
Expected: PASS (both sides emit `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`). **If Docker is unavailable / the run is host-flaky** (memories: differential fixtures flake under parallel load / bridge-IP host-sensitivity) → this is **CI-authoritative**; do NOT treat a local Docker-absent skip as a failure. The state-4 verification gate re-runs it on CI. At minimum confirm the configs parse: `cargo run -p envoy-bin -- -c tests/fixtures/0058-accesslog-rf-overflow/envoy-rust.yaml --mode validate` (or the project's config-validate entrypoint) returns OK.

- [ ] **Step 7: Commit.**

```bash
git add tests/fixtures/0058-accesslog-rf-overflow/ tests/differential/tests/access_log_rf_overflow.rs
git commit -m "phase 50 T3: fixture 0058-accesslog-rf-overflow + differential test [ADR-0107]"
```

---

## Task 4: §E BEHAVIOR_CONTRACT updates (+ M49-3 re-anchor)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1020` (the `%RESPONSE_FLAGS%` row)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1031` (the `%RESPONSE_CODE_DETAILS%` row)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:37` (the overflow circuit-breaker row — re-anchor per M49-3)

- [ ] **Step 1: Update the `%RESPONSE_FLAGS%` row (`:1020`).** (a) Add the **`UO` per-flag equivalence rule**: a config-deterministic single static constant (brace-free), derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{overflow}` at the H1 record-build site (`hcm.rs:1225`), set on BOTH pool-overflow arms (the `outcome:None` discriminator) and the request-budget arm; the 503 status/body/headers are unchanged. (b) Move `UO` OUT of the "Other non-`-` flags (`UF`/`UO`/`DC`/`URX`) remain unwitnessed" list (leave `UF`/`DC`/`URX`). (c) Add the witnessing-fixtures sentence: "Phase 50 (ADR-0107) fixture **0058** witnesses `UO` byte-exact on the circuit-breaker overflow 503 path; both proxies emit `UO`. The request-budget (`max_requests`) overflow UO is in-process-backstopped only — differential witness deferred (M50-C)."

- [ ] **Step 2: Update the `%RESPONSE_CODE_DETAILS%` row (`:1031`).** Change "The connect-failure / overflow failure details (non-deterministic OS-derived brace content) remain deferred (M45-2)" to scope it to connect-failure ONLY, and add: the overflow detail `upstream_reset_before_response_started{overflow}` is now witnessed byte-exact deterministic (the brace content is a FIXED reset-reason enum, NOT the OS-derived connect-failure phrase) by fixture **0058** (phase 50, ADR-0107) — refining M45-2 + ADR-0102 §B (only the connect-failure rcd remains non-deterministic). Add `0058` alongside `0053`/`0054`/`0055` in the witnessed-failure-path-detail list, and add `upstream_reset_before_response_started{overflow}` to the rcd-disposition enumeration (set at the H1 retry-loop consumption site, the `outcome:None` overflow discriminator `hcm.rs:~995`, + the request-budget arm `:923`).

- [ ] **Step 3: Re-anchor the overflow circuit-breaker row (`:37`) per M49-3.** Change the stale citation `hcm.rs:542`/`hcm.rs:569` to the actual arms `hcm.rs:503` (`PoolError::Overflow`/`max_connections`) / `hcm.rs:510` (`PoolError::PendingOverflow`/`max_pending_requests`). Append a note: the overflow `%RESPONSE_CODE_DETAILS%` (`upstream_reset_before_response_started{overflow}`) + `%RESPONSE_FLAGS%` (`UO`) are now witnessed byte-exact in the access log (fixture 0058, phase 50) — distinct from the still-non-deterministic connect-failure rcd. Do NOT change the wire-shape equivalence (status + 81-byte body + `x-envoy-overloaded`).

- [ ] **Step 4: Verify the doc edits don't break any doc-driven test + re-confirm additivity.**

Run: `cargo test -p envoy-http1 && cargo test -p envoy-accesslog`
Expected: all PASS (these doc edits are prose-only; no test references the changed line numbers). Eyeball-confirm `0056`/`0057` fixture expectations and the `:1238`/`:1239` derive arms are untouched.

- [ ] **Step 5: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 50 T4: BEHAVIOR_CONTRACT — UO witnessed + overflow rcd deterministic + M49-3 re-anchor [ADR-0107]"
```

---

## State-3 exit checklist (NOT this session — the session AFTER does state-4)

The state-3 implementation session ends after Task 4. It does NOT run the full §7.5 gate (that is state-4, `superpowers:verification-before-completion`). The state-3 session must: append a `PROGRESS.md` entry per task (RED→GREEN evidence), update `STATE.md` to `state-4-next`, push, and confirm CI. The carry-forward consumption to record at close:

- **CONSUMED by phase 50:** the `UO` slice of **M45-2** (+ the overflow-rcd-deterministic refinement); **M49-3** on the overflow circuit-breaker row + the `:1225` derive anchor; **M49-2** on the derive-comment `synth_404` citations (Task 1 Step 4).
- **NEW carry-forward — M50-C:** the request-budget (`max_requests`) overflow UO/rcd is in-process-backstopped only; its differential witness is deferred (no `max_requests` access-log fixture). **The unverified part is the rcd STRING, not just the flag:** the `upstream_reset_before_response_started{overflow}` rcd was recon-confirmed ONLY on the pool path (`max_pending_requests`); Envoy's request-level breaker (`max_requests`, checked before pool acquire) may emit a DIFFERENT `%RESPONSE_CODE_DETAILS%` (e.g. a brace-free `upstream_overflow`) while still flagging `UO`. Whoever witnesses the budget arm must RE-RECON the rcd string against live Envoy — do NOT assume the pool-arm string carries over. Fold whenever a request-budget access-log fixture is next added.
- **Still live (NONE blocks):** M48-2, M42-1, M45-1, the connect-failure slice of M45-2 (`UF`/`DC`/`URX`), M40-1, M39-*, M38-*, CF-39-1, M37-*, M36-*, M34-*, M33-*, the empty-`metadata_match` doc-comment, M29-*, M30-*, the phase-31 cosmetics, the HTTP-filters-family (1)-(4).

## §6.1 split gate (re-evaluated at PLAN close)

4 tasks, ~70-120 LoC net (§A ~12 LoC + §A′ ~3 LoC + §B ~5 LoC + 3 backstop tests ~120 LoC of test + 4 fixture files + 1 differential test + 3 doc edits). Well under ~25 tasks / ~1500 LoC → **§6.1 does NOT fire. ADR-0108 stays reserved-but-unfired** (reclaimable by the next NEW phase pick per the lapsed-reservation convention).

## §6.2 reconciliation ADR

NOT needed — the §6.2 recon (above) CONFIRMED every SPEC §A-§F fact; no fact was overturned. The one PLAN decision (tag the budget arm — SPEC §3.1) follows the SPEC's recommended default and is recorded here + tracked as M50-C; it does not overturn a SPEC fact, so no new ADR fires.
