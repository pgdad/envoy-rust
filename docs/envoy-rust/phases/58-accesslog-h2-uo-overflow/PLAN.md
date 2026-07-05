# Phase 58 — `58-accesslog-h2-uo-overflow` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (failing test first) per doctrine D-3.1.

**Goal:** Witness the THIRD H2 `%RESPONSE_FLAGS%` value, `UO` (UpstreamOverflow), byte-exact on the H2 pool/circuit-breaker overflow 503 path, via a NEW fixture `0066`.

**Architecture:** A two-site source fix in `crates/envoy-http2/src/hcm.rs` — (1) an outcome discriminator at the caller-loop's `if let Some(endpoint) = attempt.endpoint` site (distinguishing the pool-overflow `H2AttemptResult` from a real upstream response, mirroring H1's phase-50 discriminator), and (2) a direct tag on the pre-route request-budget `Rejected` arm (which bypasses the retry loop entirely, mirroring H1's phase-50 tag on the same arm) — plus a one-arm extension to the existing H2 `%RESPONSE_FLAGS%` derive (phases 56/57). UNLIKE phases 50 and 57, envoy-rust's H2 overflow status is ALREADY correct (503, via the pre-existing `synth_h2_overflow()`) — this phase is a PURE access-log rcd/rf fix, no status-code correction needed. Reuses the existing `Driver::Http2AccessLogByteExact` harness verbatim (no harness change). Two in-process backstops are REQUIRED (one per set-site, per the phase-58 state-1 spec-review finding): the pool-overflow arm needs a configured `H2PoolManager`; the request-budget arm needs no pool (it bypasses the pool entirely).

**Tech Stack:** Rust (`crates/envoy-http2`), `envoy-config`/`envoy-cluster`/`envoy-accesslog` (test-util), the `h2` crate (client handshake in in-process backstops), `tokio` test harness, the Docker-gated differential harness (`tests/differential`, `kind: http2_access_log_byte_exact`), fixture data under `tests/fixtures/`.

## Global Constraints

- `#![forbid(unsafe_code)]` holds — no `unsafe` anywhere in this phase.
- NO new `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant (SPEC §2).
- Load-bearing additivity invariant: all `0001`-`0065` fixtures stay byte-identical (SPEC §2, re-verified §3.2 below).
- NO status-code change anywhere — `synth_h2_overflow()` is untouched; only the access-log `rcd`/`rf` fields change.
- No new fuzz target (SPEC §H — `%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%` are existing operators; no H2 codec/framing change).

---

## §3 PLAN-VERIFY re-confirmation (done this session, before authoring tasks)

All seven SPEC §3 items were re-checked against the live tree (no drift found):

1. **Line numbers confirmed exact.** `run_h2_attempt`'s overflow-returning arm: `hcm.rs:407`-`417` (`outcome: None` at `:414`). The pre-route `BudgetAcquisition::Rejected` arm: `hcm.rs:613`-`636` (`overflow_resp` built at `:625`; `response_code_details_for_log_h2` is never assigned in this block — confirmed by re-reading the full block). The caller-loop `if let Some(endpoint) = attempt.endpoint { ... } else { ... }`: `hcm.rs:691`-`703` (unconditional `Some("via_upstream".to_owned())` at `:696`). The three-arm-to-be derive: `hcm.rs:949`-`963`. All match SPEC's citations exactly — no drift.
2. **Additivity re-grep confirmed (widened per spec-review).** `grep -n circuit_breakers` over EVERY `tests/fixtures/*/envoy-rust.yaml` with an H2 listener (`0009`, `0010`, `0018`, `0021`, `0064`, `0065`) finds only `0021` (`max_connections: 4` — headroom only, no `max_pending_requests`/`max_requests` cap) and `0018-http-filter-fault` (`clusters: []`, unreachable by definition). NONE can reach either the pool-overflow arm or the request-budget arm — both new code paths are unreachable by any pre-existing fixture.
3. **H1 backstop names re-confirmed unchanged:** `h1_pool_overflow_access_log_carries_uo_flag` (`crates/envoy-http1/src/hcm.rs:5625`) and `h1_request_budget_overflow_access_log_carries_uo_flag` (`crates/envoy-http1/src/hcm.rs:7224`). Both re-read in full this session — cited as the Task 1/Task 2 backstop patterns below.
4. **H2 pool test-harness precedent re-confirmed:** `H2PoolManager::for_bootstrap(bootstrap, cluster_mgr, registry, token)` (`crates/envoy-http2/src/pool.rs:592`) mirrors `H1PoolManager::for_bootstrap` — filters clusters to `upstream_protocol() == Http2`, reads `max_pending_requests` from the bootstrap's `circuit_breakers.thresholds[0]`. An EXISTING test in `crates/envoy-http2/src/hcm.rs` (`h2_hcm_pool_reuses_upstream_conn_across_sequential_requests`, `~:1870`-`1955`) already shows the exact wiring pattern needed: build `ClusterManager` + `H2PoolManager::for_bootstrap` from the SAME bootstrap/registry, build the inner `Http1HCMConfig`, wrap with `HCMConfig::wrap(inner, Some(pool_mgr))`, then spawn `HCM::new(hcm_config)` MANUALLY (the existing `spawn_h2_hcm` test helper hard-codes `pool: None` — its own comment says so at `:1198`-`1200`). Task 1's backstop clones this exact wiring, not a new shared helper (matching the established in-file precedent of inlining rather than adding shared infra for one-off pool-wired tests).
5. **§6.1 split decision: does NOT fire.** This PLAN has 6 tasks / an estimated ~300-420 LoC (a ~12-line discriminator + a ~10-line budget-arm tag + a ~4-line derive arm + two in-process backstop tests, ~110-150 LoC each because Task 1's needs the full pool-wiring boilerplate + a 4-file fixture ~130 LoC incl. README + a ~25-line differential test + two BEHAVIOR_CONTRACT row edits) — still well under the ~25-task/~1500-LoC gate. No split; ADR-0116 stays reserved-but-unfired (reclaimed by the next NEW phase pick per the standing lapsed-reservation convention).
6. **Fixture number `0066` re-confirmed still next-free.** `ls tests/fixtures/ | sort | tail` shows the highest existing fixture is `0065-accesslog-h2-rf-no-healthy`; no sibling session has landed `0066` in between.
7. **Request-budget arm's own differential fixture confirmed out of scope for this phase** (SPEC §4) — Task 2's in-process backstop (§F2) is the ONLY test coverage for that arm this phase; a dedicated differential fixture is left as a candidate future carry-forward slice (mirroring how H1's own equivalent gap, M50-C, was later closed cheaply by phase 55).

No §6.2 reconciliation ADR is needed — none of SPEC §A-§H is overturned.

---

## Task 1: Caller-loop overflow discriminator + three-arm derive + pool-overflow backstop (§A + §C + §F1)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (caller-loop `:691`-`703`; derive `:949`-`963`; new test)

**Interfaces:**
- Produces: nothing new consumed by later tasks — Task 3/4 (fixtures) exercise this via the Docker differential, not via Rust symbols. Task 2's backstop does NOT depend on Task 1's discriminator (different arm), but DOES depend on Task 1's derive extension (§C) to render `"UO"`.

- [ ] **Step 1: Write the failing pool-overflow access-log backstop test**

Insert into the test module (e.g. immediately after `h2_hcm_pool_reuses_upstream_conn_across_sequential_requests`, `~:1955`) — combines that test's pool-wiring pattern with the phase-57 `h2_no_healthy_access_log_carries_uh_flag` test's route/access-log-sink construction, and mirrors the H1 backstop `h1_pool_overflow_access_log_carries_uo_flag` (`crates/envoy-http1/src/hcm.rs:5625`):

```rust
    /// Phase 58 (ADR-0115) §A/§C/§F1 backstop: drive the FULL H2 dispatch path
    /// with a CONFIGURED `H2PoolManager` whose cluster carries
    /// `circuit_breakers.thresholds:[{max_connections:1,
    /// max_pending_requests:0}]` and a single dead endpoint (`127.0.0.1:1`,
    /// never dialed). The first connect-on-miss is rejected with
    /// `PoolError::PendingOverflow` → `AcquireOutcome::Overflow` →
    /// `H2AttemptResult{endpoint:Some, outcome:None}` (`hcm.rs:407`-`417`)
    /// consumed at the caller-loop site (`hcm.rs:691`). Asserts the FILE json
    /// access-log line carries the overflow detail and the derived `UO` flag
    /// — the sole in-process proof of §A's outcome discriminator + §C's
    /// derive arm on the POOL-overflow path. Fail-first: pre-change it
    /// renders `"rcd":"via_upstream","rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_pool_overflow_access_log_carries_uo_flag() {
        let tmp = tempfile::tempdir().unwrap();
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

        // STATIC cluster, dead endpoint 127.0.0.1:1 (never dialed), H2 upstream
        // (typed_extension_protocol_options), circuit breakers
        // max_connections:1 / max_pending_requests:0 → the H2 pool's
        // connect-on-miss pending-gate rejects the first request.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
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
      typed_extension_protocol_options:
        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
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
        let token = tokio_util::sync::CancellationToken::new();
        let pool_mgr = crate::pool::H2PoolManager::for_bootstrap(
            &bootstrap,
            &cluster_mgr,
            Arc::clone(&registry),
            token.clone(),
        )
        .expect("H2PoolManager::for_bootstrap");

        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http_h2".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
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
            }),
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let mut built = Http1HCMConfig::from_config(&cfg, Arc::clone(&cluster_mgr), Arc::clone(&registry), None)
            .await
            .expect("build HCM config");
        built.access_log = vec![sink];
        let inner = Arc::new(built);
        let hcm_config = Arc::new(HCMConfig::wrap(Arc::clone(&inner), Some(Arc::clone(&pool_mgr))));

        // Manual spawn (mirrors `h2_hcm_pool_reuses_upstream_conn_across_sequential_requests`
        // — the existing `spawn_h2_hcm` helper hard-codes `pool: None`).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hcm = HCM::new(hcm_config);
        let _server_handle = tokio::spawn(async move {
            loop {
                let (stream, _peer) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let hcm_clone = hcm.clone();
                tokio::spawn(async move {
                    let _ = hcm_clone.handle(stream).await;
                });
            }
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://x/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        assert_eq!(
            resp.status(),
            503,
            "pool-overflow synth-503 status unchanged"
        );
        let mut body = resp.into_body();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n",
            "H2 pool-overflow access-log line carries the overflow rcd + rf:\"UO\": {logged:?}"
        );
    }
```

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http2 h2_pool_overflow_access_log_carries_uo_flag`
Expected: FAIL — logged line is `{"rc":503,"rcd":"via_upstream","rf":"-"}\n` (the caller-loop unconditionally sets `via_upstream` on `endpoint:Some`; the derive has no `UO` arm yet).

- [ ] **Step 3: Commit the RED test**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 58 task 1: RED test for H2 pool-overflow access-log rcd/rf [ADR-0115]"
```

- [ ] **Step 4: Add the caller-loop overflow discriminator (§A)**

Replace exactly (`hcm.rs:691`-`703`):

```rust
                        if let Some(endpoint) = attempt.endpoint {
                            // 06.2 Task 7: capture the resolved upstream endpoint for
                            // the access-log `%UPSTREAM_HOST%` token (last attempt's
                            // endpoint wins). Skipped on pick()->None.
                            upstream_host_for_log_h2 = Some(endpoint.to_string());
                            response_code_details_for_log_h2 = Some("via_upstream".to_owned());
                        } else {
                            // 57 (ADR-0114) §B: the pick()->None no-healthy arm —
                            // mirrors the H1 caller-loop if/else pattern
                            // (crates/envoy-http1/src/hcm.rs:1029-1059).
                            response_code_details_for_log_h2 =
                                Some("no_healthy_upstream".to_owned());
                        }
```

with:

```rust
                        if let Some(endpoint) = attempt.endpoint {
                            // 06.2 Task 7: capture the resolved upstream endpoint for
                            // the access-log `%UPSTREAM_HOST%` token (last attempt's
                            // endpoint wins). Skipped on pick()->None.
                            upstream_host_for_log_h2 = Some(endpoint.to_string());
                            // 58 (ADR-0115) §A: discriminate the pool-overflow
                            // outcome (endpoint:Some + outcome:None — UNIQUELY
                            // the AcquireOutcome::Overflow result, hcm.rs:414;
                            // every other endpoint:Some path carries a
                            // non-None outcome) from a real upstream response
                            // — mirrors the H1 discriminator exactly
                            // (crates/envoy-http1/src/hcm.rs:1045-1052).
                            response_code_details_for_log_h2 = Some(
                                if attempt.outcome.is_none() {
                                    "upstream_reset_before_response_started{overflow}"
                                } else {
                                    "via_upstream"
                                }
                                .to_owned(),
                            );
                        } else {
                            // 57 (ADR-0114) §B: the pick()->None no-healthy arm —
                            // mirrors the H1 caller-loop if/else pattern
                            // (crates/envoy-http1/src/hcm.rs:1029-1059).
                            response_code_details_for_log_h2 =
                                Some("no_healthy_upstream".to_owned());
                        }
```

- [ ] **Step 5: Extend the `%RESPONSE_FLAGS%` derive to three arms (§C)**

Replace exactly (`hcm.rs:949`-`963`; the comment block + the two-arm match):

```rust
        // Phase 56 (ADR-0113) / phase 57 (ADR-0114): the H2 sibling of the H1
        // %RESPONSE_FLAGS% derive (crates/envoy-http1/src/hcm.rs:1377), grown
        // one arm per phase exactly as H1 did (48->49->50->51->52->53).
        // route_not_found => NR (phase 56); no_healthy_upstream => UH (phase
        // 57 — ALSO corrects the H2 no-healthy synth status 502->503, see
        // synth_h2_no_healthy_upstream()). The remaining H2 flags
        // (UO/URX/UF/UC) are the continuing carry-forward M56-1, witnessed
        // one-at-a-time by future phases.
        let response_flags_for_log_h2: &str = match response_code_details_for_log_h2.as_deref() {
            Some("route_not_found") => "NR",
            Some("no_healthy_upstream") => "UH",
            _ => "-",
        };
```

with:

```rust
        // Phase 56 (ADR-0113) / phase 57 (ADR-0114) / phase 58 (ADR-0115): the
        // H2 sibling of the H1 %RESPONSE_FLAGS% derive
        // (crates/envoy-http1/src/hcm.rs:1377), grown one arm per phase
        // exactly as H1 did (48->49->50->51->52->53). route_not_found => NR
        // (phase 56); no_healthy_upstream => UH (phase 57); the
        // pool/request-budget overflow detail => UO (phase 58 — set on BOTH
        // the pool-overflow arm, §A, and the request-budget arm, §B — no
        // status-code fix needed this phase, unlike 50/57). The remaining H2
        // flags (URX/UF/UC) are the continuing carry-forward M56-1,
        // witnessed one-at-a-time by future phases.
        let response_flags_for_log_h2: &str = match response_code_details_for_log_h2.as_deref() {
            Some("route_not_found") => "NR",
            Some("no_healthy_upstream") => "UH",
            Some("upstream_reset_before_response_started{overflow}") => "UO",
            _ => "-",
        };
```

- [ ] **Step 6: Run the Task-1 test to verify it PASSES**

Run: `cargo test -p envoy-http2 h2_pool_overflow_access_log_carries_uo_flag`
Expected: PASS (logged line `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}\n`).

- [ ] **Step 7: Run the full crate test suite to verify NO regression**

Run: `cargo test -p envoy-http2`
Expected: PASS, including:
- the phase-57 `h2_no_healthy_upstream_returns_503`/`h2_no_healthy_access_log_carries_uh_flag` backstops (the pick-none `else` arm is untouched; the derive's `no_healthy_upstream => "UH"` arm is unchanged);
- the phase-56 `h2_route_miss_access_log_carries_nr_flag`/`h2_host_miss_access_log_carries_nr_flag` backstops (unaffected — neither hits `endpoint:Some` with `outcome:None`);
- `h2_hcm_pool_reuses_upstream_conn_across_sequential_requests` (a successful pool dispatch — `outcome` is `Some(Response)` on every attempt, so the new discriminator's `else` branch — `"via_upstream"` — fires exactly as before; byte-identical);
- all happy-path/proxy-success tests (the `attempt.outcome.is_none()` check is `false` on every real response/reset/connect-failure outcome — only the overflow arm is `None`).

- [ ] **Step 8: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 58 task 1: H2 pool-overflow discriminator + rf:UO derive arm (GREEN) [ADR-0115]"
```

---

## Task 2: Request-budget-arm tag + budget-overflow backstop (§B + §F2)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (pre-route `BudgetAcquisition::Rejected` arm `:613`-`636`; new test)

**Interfaces:**
- Consumes: the three-arm derive from Task 1 (Step 5) — this task's backstop needs `Some("upstream_reset_before_response_started{overflow}") => "UO"` to already be in the match for its assertion to pass.
- Produces: nothing new consumed by later tasks.

- [ ] **Step 1: Write the failing request-budget-overflow access-log backstop test**

Insert into the test module (e.g. immediately after Task 1's `h2_pool_overflow_access_log_carries_uo_flag`) — mirrors the H1 backstop `h1_request_budget_overflow_access_log_carries_uo_flag` (`crates/envoy-http1/src/hcm.rs:7224`). Unlike Task 1, this arm needs NO pool (the request-budget gate fires BEFORE any pool contact) — reuses the existing `spawn_h2_hcm` helper (`pool: None` is fine, since this arm never touches the pool):

```rust
    /// Phase 58 (ADR-0115) §B/§F2 backstop: the pre-route request-budget
    /// overflow (`max_requests:0`, `BudgetAcquisition::Rejected` at
    /// `hcm.rs:613`) calls `synth_h2_overflow()` at `:625` and BYPASSES the
    /// retry loop entirely (no `run_h2_attempt` call), so it is tagged
    /// DIRECTLY at `:625`-ish (not via Task 1's §A discriminator). Asserts
    /// the FILE json access-log line carries the overflow rcd + the derived
    /// `rf:"UO"` — the in-process proof for the budget arm (its OWN
    /// differential witness is deferred as a candidate future carry-forward
    /// slice, mirroring H1's M50-C). Fail-first: pre-change it renders
    /// `"rcd":null,"rf":"-"` (the arm never sets `response_code_details_for_log_h2`
    /// today).
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_request_budget_overflow_access_log_carries_uo_flag() {
        let tmp = tempfile::tempdir().unwrap();
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

        // STATIC cluster, dead endpoint 127.0.0.1:1 (NEVER dialed — the
        // request-budget gate rejects BEFORE any pool/connect contact), plain
        // H1 upstream (no typed_extension_protocol_options needed — this arm
        // is protocol-agnostic, checked before run_h2_attempt is ever called),
        // circuit breakers max_requests:0.
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
    - name: budget_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_requests: 0
      load_assignment:
        cluster_name: budget_cluster
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

        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http_h2".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
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
                            cluster: "budget_cluster".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: None,
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            }),
            rds: None,
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let mut built = Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
            .await
            .expect("build HCM config");
        built.access_log = vec![sink];
        let config = Arc::new(built);

        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://x/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        assert_eq!(
            resp.status(),
            503,
            "request-budget overflow synth-503 status unchanged"
        );
        let mut body = resp.into_body();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{overflow}\",\"rf\":\"UO\"}\n",
            "H2 request-budget-overflow access-log line carries the overflow rcd + rf:\"UO\": {logged:?}"
        );
    }
```

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http2 h2_request_budget_overflow_access_log_carries_uo_flag`
Expected: FAIL — logged line is `{"rc":503,"rcd":null,"rf":"-"}\n` (the `Rejected` arm never sets `response_code_details_for_log_h2`).

- [ ] **Step 3: Commit the RED test**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 58 task 2: RED test for H2 request-budget-overflow access-log rcd/rf [ADR-0115]"
```

- [ ] **Step 4: Tag the request-budget `Rejected` arm (§B)**

Replace exactly (`hcm.rs:613`-`636`):

```rust
                if let envoy_cluster::BudgetAcquisition::Rejected = request_acquire {
                    _request_guard = None;
                    // The failed acquire already ticked
                    // upstream_rq_pending_overflow (§5.3 — single source of
                    // truth). L3 (ADR-0047): Envoy's overflow local reply ALSO
                    // ticks upstream_rq_5xx — mirror it here (the ONLY synth
                    // path that ticks it; the phase-16 completing-response gate
                    // for every OTHER path is untouched). upstream_rq_total is
                    // NOT ticked (constraint iv — no attempt ever dispatches).
                    cluster.upstream_rq_5xx().inc();
                    // The overflow synth-503 (81-byte body + x-envoy-overloaded)
                    // — the SAME helper the pool PendingOverflow arm uses.
                    let mut overflow_resp = synth_h2_overflow();
                    // L11: the overflow local reply carries
                    // x-envoy-attempt-count: 1 when the vhost flag is set (only
                    // the would-be first attempt; none ever dispatched).
                    if include_attempt_count_in_response {
                        overflow_resp
                            .headers
                            .push(("x-envoy-attempt-count".to_string(), "1".to_string()));
                    }
                    // Fall through to finalize_h2_stream (no pool contact,
                    // no retry loop).
                    overflow_resp
                } else {
```

with:

```rust
                if let envoy_cluster::BudgetAcquisition::Rejected = request_acquire {
                    _request_guard = None;
                    // The failed acquire already ticked
                    // upstream_rq_pending_overflow (§5.3 — single source of
                    // truth). L3 (ADR-0047): Envoy's overflow local reply ALSO
                    // ticks upstream_rq_5xx — mirror it here (the ONLY synth
                    // path that ticks it; the phase-16 completing-response gate
                    // for every OTHER path is untouched). upstream_rq_total is
                    // NOT ticked (constraint iv — no attempt ever dispatches).
                    cluster.upstream_rq_5xx().inc();
                    // The overflow synth-503 (81-byte body + x-envoy-overloaded)
                    // — the SAME helper the pool PendingOverflow arm uses.
                    let mut overflow_resp = synth_h2_overflow();
                    // 58 (ADR-0115) §B: the request-budget (max_requests)
                    // overflow is the SAME UO/overflow disposition as the pool
                    // arms — same synth_h2_overflow() helper, same 503 wire
                    // shape. Tag the rcd so the §C derive maps it => "UO".
                    // This arm BYPASSES the retry loop entirely (no
                    // run_h2_attempt call), so it is tagged HERE directly
                    // (not via §A's discriminator) — mirrors the H1 tag
                    // exactly (crates/envoy-http1/src/hcm.rs:951-952).
                    response_code_details_for_log_h2 =
                        Some("upstream_reset_before_response_started{overflow}".to_owned());
                    // L11: the overflow local reply carries
                    // x-envoy-attempt-count: 1 when the vhost flag is set (only
                    // the would-be first attempt; none ever dispatched).
                    if include_attempt_count_in_response {
                        overflow_resp
                            .headers
                            .push(("x-envoy-attempt-count".to_string(), "1".to_string()));
                    }
                    // Fall through to finalize_h2_stream (no pool contact,
                    // no retry loop).
                    overflow_resp
                } else {
```

- [ ] **Step 5: Run the Task-2 test to verify it PASSES**

Run: `cargo test -p envoy-http2 h2_request_budget_overflow_access_log_carries_uo_flag`
Expected: PASS (logged line `{"rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}\n`).

- [ ] **Step 6: Run the full crate test suite to verify NO regression**

Run: `cargo test -p envoy-http2`
Expected: PASS, including Task 1's backstop (a DIFFERENT arm — the pool-overflow discriminator is untouched by this task) and every other existing test (no cluster in any other test configures `circuit_breakers.thresholds.max_requests: 0`, so this arm is unreached elsewhere).

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 58 task 2: H2 request-budget-arm rcd tag (GREEN) [ADR-0115]"
```

---

## Task 3: Fixture `0066-accesslog-h2-rf-overflow` (one probe: H2 pool-overflow 503)

Build the fixture from the `0058` template (the H1 `UO` witness — a STATIC cluster with `circuit_breakers.thresholds:[{max_connections:1, max_pending_requests:0}]` + a literal dead endpoint `127.0.0.1:1`), substituting `0064`/`0065`'s H2C listener shape (`codec_type: HTTP2` + `http2_protocol_options: {}` + the `{rc,rcd,rf,method,proto}` json_format) for `0058`'s `codec_type: HTTP1`, AND adding `typed_extension_protocol_options`/`http2_protocol_options` to the cluster (an H2-upstream cluster — required for envoy-rust's side to route through the H2 pool and hit `PoolError::PendingOverflow`; the state-0 recon confirmed live Envoy emits the identical output regardless of the cluster's upstream protocol, so both sides use the same H2-upstream shape for config parity).

**Files:**
- Create: `tests/fixtures/0066-accesslog-h2-rf-overflow/envoy.yaml`
- Create: `tests/fixtures/0066-accesslog-h2-rf-overflow/envoy-rust.yaml`
- Create: `tests/fixtures/0066-accesslog-h2-rf-overflow/expectations.yaml`
- Create: `tests/fixtures/0066-accesslog-h2-rf-overflow/README.md`

- [ ] **Step 1: Create `envoy.yaml`** (reference side — `admin` block present, bind `0.0.0.0`, mount path `/tmp/0066-envoy-mount/access.log`)

```yaml
node: { id: envoy-rust-phase-58-fixture-0066, cluster: envoy-rust-phase-58 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP2
                http2_protocol_options: {}
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0066-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
                          method: "%REQ(:METHOD)%"
                          proto: "%PROTOCOL%"
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
    # STATIC cluster, H2 upstream (typed_extension_protocol_options), carrying
    # circuit_breakers max_connections:1 / max_pending_requests:0. The single
    # endpoint is the LITERAL unreachable 127.0.0.1:1 — NEVER dialed: the
    # connect-on-miss pending-gate rejects the first request with the
    # overflow synth-503 BEFORE any connect (the fixture-0058 pattern, H2
    # analogue).
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_pending_requests: 0
      typed_extension_protocol_options:
        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
```

- [ ] **Step 2: Create `envoy-rust.yaml`** (subject side — NO `admin` block, bind `127.0.0.1`, mount path `/tmp/0066-envoy-rust-mount/access.log`; otherwise byte-identical)

```yaml
node: { id: envoy-rust-phase-58-fixture-0066, cluster: envoy-rust-phase-58 }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 127.0.0.1, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP2
                http2_protocol_options: {}
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0066-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
                          method: "%REQ(:METHOD)%"
                          proto: "%PROTOCOL%"
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
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_pending_requests: 0
      typed_extension_protocol_options:
        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
```

- [ ] **Step 3: Create `expectations.yaml`** (one probe — H2 pool-overflow 503 — reuses `Driver::Http2AccessLogByteExact` verbatim)

```yaml
driver:
  kind: http2_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0066-envoy-mount/access.log
    envoy_rust: /tmp/0066-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / (any :authority — domains: ["*"]) routed to
    # `backend_cluster`, whose circuit_breakers max_pending_requests:0 rejects
    # the connect-on-miss with the overflow synth-503 BEFORE any connect. This
    # is the THIRD non-`-` H2 %RESPONSE_FLAGS% witness: UO (UpstreamOverflow)
    # (phase 58, ADR-0115), the H2 analogue of fixture 0058 (phase 50).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static
    # literal: the `http2_access_log_byte_exact` driver asserts every line is
    # byte-identical between upstream Envoy v1.33.0 and envoy-rust. The
    # overflow synth-503 is deterministic on BOTH sides (the brace content
    # `overflow` is a FIXED reset-reason enum, not OS-derived), so the
    # rendered line is identical. UNLIKE fixtures 0058/0065, envoy-rust's
    # status here was ALREADY correct (503) — only rcd/rf change.
    #
    # Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rcd, rf.
    # The emitted line is:
    #   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `README.md`**

```markdown
# Fixture 0066 — H2 access-log `%RESPONSE_FLAGS%` pool/circuit-breaker overflow failure path (`UO`, byte-exact)

The H2 analogue of fixture `0058` (phase 50, the H1 `UO` witness) and the
THIRD fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`; extended by phase 57, fixture `0065`). Phase 58 (ADR-0115)
witnesses the THIRD H2 `%RESPONSE_FLAGS%` value, `UO` (UpstreamOverflow),
byte-exact on the H2 pool/circuit-breaker overflow 503 path.

## What this proves

Before this phase, envoy-rust's H2 caller-loop unconditionally set
`response_code_details_for_log_h2 = Some("via_upstream")` whenever an attempt
had a picked endpoint (`endpoint: Some`) — including the pool-overflow
`H2AttemptResult` (`endpoint: Some`, `outcome: None`, `crates/envoy-http2/src/hcm.rs:407`-`417`),
which is NOT a real upstream response. Phase 58 (i) discriminates the
overflow outcome from a real response at that caller-loop site (mirroring
the H1 phase-50 discriminator), (ii) tags the pre-route request-budget
`Rejected` arm directly (it bypasses the retry loop entirely), and (iii)
extends the H2 `%RESPONSE_FLAGS%` derive to a third arm. UNLIKE fixtures
`0058` (phase 50) and `0065` (phase 57), NO status-code correction was
needed — envoy-rust's H2 overflow status was already correct (503, via the
pre-existing `synth_h2_overflow()`).

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | pool-overflow (`max_pending_requests:0`) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
```

The cluster is the IDENTICAL shape fixture `0058` uses (`circuit_breakers.thresholds:[{max_connections:1,max_pending_requests:0}]`,
a literal dead endpoint `127.0.0.1:1`), PLUS `typed_extension_protocol_options`
(an H2 upstream — required for envoy-rust's side to route through the H2 pool
and hit `PoolError::PendingOverflow`; the state-0 recon confirmed live Envoy
emits identical output regardless of the cluster's upstream protocol, so both
sides use the H2-upstream shape for config parity) — only `codec_type: HTTP2`
+ `http2_protocol_options: {}` (fixture `0064`/`0065`'s listener shape) are
substituted for `0058`'s `codec_type: HTTP1`.

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness change this phase. Drives the probe over
H2-prior-knowledge via `drive_http2`, scrapes both files, asserts the scraped
line count equals `probes.len()` (here 1), and calls
`access_log::assert_access_log_lines_byte_identical`.

## `0001`-`0065` byte-preservation

This phase's changes are additive — gated on (a) an attempt with
`endpoint:Some, outcome:None` (uniquely the pool-overflow result), and (b)
`try_acquire_request()` returning `Rejected` (requires `circuit_breakers.thresholds.max_requests: 0`).
NONE of the pre-existing H2 fixtures (`0009`, `0010`, `0018`, `0021`, `0064`,
`0065`) configures a `circuit_breakers` threshold that could reach either
path — re-confirmed by a fresh `grep -n circuit_breakers` over each
`envoy-rust.yaml` (only `0021`'s `max_connections: 4`, headroom only). So
`0001`-`0065` stay byte-identical; only the new `0066` observes the changed
rcd/rf.

## Cross-references

- ADR: ADR-0115 (state-1 brainstorm + state-2 PLAN — the H2 `UO` witness).
- Related fixtures: `0058` (the H1 `UO` witness this fixture mirrors on H2);
  `0064`/`0065` (the H2 `NR`/`UH` witnesses that opened/extended
  `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`URX`/`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%` strings
  beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`, still open for
  future one-flag-at-a-time phases. Also notes a candidate future
  carry-forward slice: the H2 request-budget arm's OWN differential
  access-log witness (a `max_requests: 0` trigger, distinct from this
  fixture's pool trigger) — covered at the in-process level only this phase
  (§F2), mirroring how H1's equivalent gap (M50-C) was later closed cheaply
  by phase 55.
```

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0066-accesslog-h2-rf-overflow/
git commit -m "phase 58 task 3: fixture 0066-accesslog-h2-rf-overflow (one probe, H2 rf:UO byte-exact) [ADR-0115]"
```

---

## Task 4: Differential test `access_log_h2_rf_overflow.rs` (§E)

**Files:**
- Create: `tests/differential/tests/access_log_h2_rf_overflow.rs` (a structural clone of `access_log_h2_rf_no_healthy.rs`, pointing at the `0066` fixture)

- [ ] **Step 1: Write the test wrapper**

```rust
//! Docker-gated differential test for fixture 0066-accesslog-h2-rf-overflow.
//! Phase 58 (ADR-0115) — the THIRD H2 `%RESPONSE_FLAGS%` witness: `UO`
//! (UpstreamOverflow), byte-exact cross-proxy on the H2 pool/circuit-breaker
//! overflow 503 path — the H2 analogue of fixture `0058` (phase 50). A
//! STATIC cluster with `circuit_breakers.thresholds:[{max_connections:1,
//! max_pending_requests:0}]` + a literal dead endpoint `127.0.0.1:1` (NEVER
//! dialed: the H2 pool's connect-on-miss pending-gate rejects the first
//! request with the deterministic overflow synth-503 BEFORE any connect).
//! Spawns Envoy v1.33 in a container; spawns envoy-rust as a subprocess;
//! drives `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted line
//! is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}
//! PURE cross-proxy equality (no static literal). UNLIKE fixtures 0058/0065,
//! no status-code correction was needed this phase — only rcd/rf change.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rf_overflow() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0066-accesslog-h2-rf-overflow");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Compile-check the differential crate**

Run: `cargo test -p differential --no-run`
Expected: compiles clean (no new harness code; the `0066` fixture deserializes against the existing `Driver::Http2AccessLogByteExact`).

> **NOTE (host-environment):** the Docker differential (real Envoy vs envoy-rust) is **CI-authoritative** at the state-4 §7.5 gate (see memory `envoy-rust-state4-ci-first-execution`). This host may not run the container reliably; do NOT treat a local Docker-gated non-run as a failure. The differential `0066` green + all `0001`-`0065` still green are confirmed by the state-4 CI run.

- [ ] **Step 3: Commit**

```bash
git add tests/differential/tests/access_log_h2_rf_overflow.rs
git commit -m "phase 58 task 4: differential test access_log_h2_rf_overflow (fixture 0066) [ADR-0115]"
```

---

## Task 5: BEHAVIOR_CONTRACT updates (§G)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_FLAGS%` row, `:1020`; the `%RESPONSE_CODE_DETAILS%` row, `:1031`)

- [ ] **Step 1: Update the `%RESPONSE_FLAGS%` row's H2-witness sentence**

Replace exactly (the trailing H2 sentence of the row — a substring of the giant single-line row at `:1020`, unique in the file):

```
The remaining H2 `%RESPONSE_FLAGS%` values (`UO`/`URX`/`UF`/`UC`) remain deferred as the continuing carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

with:

```
`UO` is now ALSO witnessed byte-exact on H2 by fixture **0066** (phase 58, ADR-0115) — set on BOTH the H2 pool-overflow arm (the `outcome:None` discriminator, mirroring H1's phase-50 pattern) AND the H2 request-budget arm (mirroring H1's own direct tag) — ADVANCING carry-forward **M56-1** (the `UO` slice consumed). UNLIKE phases 50/57, NO status-code correction was needed — envoy-rust's H2 overflow status was already correct (503, via the pre-existing `synth_h2_overflow()`). The remaining H2 `%RESPONSE_FLAGS%` values (`URX`/`UF`/`UC`) remain deferred as the continuing carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

- [ ] **Step 2: Update the `%RESPONSE_CODE_DETAILS%` row's H2-witness sentence**

Replace exactly (a substring of the giant single-line row at `:1031`, unique in the file):

```
The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`) remain deferred as the continuing carry-forward **M56-1**.
```

with:

```
`upstream_reset_before_response_started{overflow}` is now ALSO witnessed on H2 (fixture **0066**, phase 58, ADR-0115), set on BOTH the H2 pool-overflow arm and the H2 request-budget arm. The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`) remain deferred as the continuing carry-forward **M56-1**.
```

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 58 task 5: BEHAVIOR_CONTRACT rf/rcd rows — H2 UO witnessed (fixture 0066) [ADR-0115]"
```

---

## Task 6: Local verification sweep (state-3 close-out; full §7.5 gate runs at state-4)

This is the developer's local pre-flight — NOT the state-4 verification gate (that re-runs the full §7.5 set in CI and quotes outputs to `PROGRESS.md`). Run the cheap-and-local subset; the Docker differential + `0001`-`0065` byte-identical + h2spec are CI-authoritative at state-4.

**Files:** none (verification only)

- [ ] **Step 1: clippy clean**

Run: `cargo clippy -p envoy-http2 -p differential --all-targets --all-features -- -D warnings`
Expected: no warnings (the discriminator, the direct tag, and the three-arm `match` are idiomatic; no new lint surface).

- [ ] **Step 2: fmt clean**

Run: `cargo fmt --all -- --check`
Expected: clean. (If any inserted block reflows, run `cargo fmt --all` and re-commit — see memory `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase.)

- [ ] **Step 3: full workspace unit tests** (non-Docker)

Run: `cargo test --workspace`
Expected: PASS (the two new backstops + all existing tests; the differential Docker tests are Docker-gated and skip locally per the harness's own gating — do not treat a local skip as a failure).

- [ ] **Step 4: confirm byte-preservation reasoning (no existing H2 fixture regressed)**

Run: `for f in 0009 0010 0018 0021 0064 0065; do echo "=== $f ==="; grep -n circuit_breakers tests/fixtures/${f}-*/envoy-rust.yaml || echo "(none)"; done`
Expected: only `0021` shows a hit (`max_connections: 4`, headroom only, no `max_pending_requests`/`max_requests` cap); the rest show `(none)` — re-confirms none can newly reach either the pool-overflow or request-budget path, so `0001`-`0065` stay byte-identical; only `0066` observes the changed rcd/rf.

- [ ] **Step 5: final fmt-fix commit if needed** (otherwise nothing to commit)

```bash
cargo fmt --all
git add -A && git commit -m "phase 58: cargo fmt [ADR-0115]" || echo "nothing to reformat"
```

---

## Scope / gate summary

- **Task count:** 6 tasks (~300-420 LoC: a ~12-line discriminator + a ~10-line direct tag + a ~4-line derive-arm extension + two in-process backstop tests, ~110-150 LoC each + a 4-file fixture (~140 LoC incl. README) + a ~25-line differential test + two BEHAVIOR_CONTRACT row edits). **§6.1 split does NOT fire** (well under ~25 tasks / ~1500 LoC — re-confirmed §3 item 5 above). **ADR-0116 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the standing lapsed-reservation convention).
- **No new** `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant. `#![forbid(unsafe_code)]` holds. NO status-code change anywhere.
- **Additive invariant:** all `0001`-`0065` fixtures stay byte-identical (§3 item 2 above; re-verified Task 6 Step 4). Only the two overflow arms' previously `"via_upstream"`/`null` rcd + `"-"` rf change — and no existing H2 fixture configures a circuit-breaker threshold that reaches either arm.
- **Acceptance (re-run at state-4, SPEC §5):** (a) fixture `0066` green (cross-proxy-equal status `503` + whole-line `{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{overflow}","rf":"UO"}`) + (b) all `0001`-`0065` green simultaneously + (c) h2spec ≥95% (no H2 codec/framing change) + (d) no new fuzz target (SPEC §H) + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved.
- **Carry-forwards:** this phase ADVANCES **M56-1** (consumes the `UO` slice; `URX`/`UF`/`UC` + the remaining H2 failure-path rcd strings stay open); notes a candidate future carry-forward slice (the H2 request-budget arm's own DIFFERENTIAL fixture, mirroring H1's M50-C — its in-process backstop, §F2, IS delivered this phase). M57-1 + M55-1 + M53-2 + M53-3 + M48-2 + M42-1 + the `DC`/retry-budget-overflow slices of M45-2 + M40-1 + M39-1/M39-2 + M38-1/M38-2 + CF-39-1 + the HTTP-filters-family (1)-(4) + older stay live; NONE blocks.

_The state-3 implementation (`superpowers:executing-plans` or `superpowers:subagent-driven-development`) is the session AFTER this PLAN lands. Per §5.1, one state per session: this session writes the PLAN only._
