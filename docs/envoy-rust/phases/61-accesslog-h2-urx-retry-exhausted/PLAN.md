# Phase 61 — `61-accesslog-h2-urx-retry-exhausted` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (failing test first) per doctrine D-3.1.

**Goal:** Witness the FOURTH H2 `%RESPONSE_FLAGS%` value, `URX` (UpstreamRetryLimitExceeded), byte-exact on the H2 retry-limit-exceeded path, via a NEW fixture `0067`.

**Architecture:** A single new per-stream boolean `retry_limit_exceeded_for_log_h2` in `crates/envoy-http2/src/hcm.rs` — declared alongside the other `*_for_log_h2` locals, set `true` at the EXISTING retry-loop's post-loop limit-exceeded exit (the same gate that already ticks `upstream_rq_retry_limit_exceeded`), threaded as a new parameter through `finalize_h2_stream`'s sole call site, and consumed by a boolean-gated wrapper on the existing three-arm `%RESPONSE_FLAGS%` derive. UNLIKE phases 50/57, the underlying retry-limit-exceeded status/mechanics are ALREADY correct and ALREADY covered by an existing phase-16 in-process test (`h2_retry_limit_exceeded_path_always_503`) — this phase is a PURE access-log rcd/rf fix; NO status-code, retry-mechanics, or harness-driver change. Mirrors the H1 phase-51 pattern (`crates/envoy-http1/src/hcm.rs:859`/`:1182`/`:1391`) and its exact backstop precedent (`h1_retry_limit_exceeded_access_log_carries_urx_flag`, `crates/envoy-http1/src/hcm.rs:7326`).

**Tech Stack:** Rust (`crates/envoy-http2`), `envoy-config`/`envoy-cluster`/`envoy-accesslog` (test-util), the `h2` crate (client handshake, reused transitively via the existing `drive_h2_once` test helper), `tokio` test harness, the Docker-gated differential harness (`tests/differential`, `kind: http2_access_log_byte_exact`), fixture data under `tests/fixtures/`.

## Global Constraints

- `#![forbid(unsafe_code)]` holds — no `unsafe` anywhere in this phase.
- NO new `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant (SPEC §2).
- Load-bearing additivity invariant: all `0001`-`0066` fixtures stay byte-identical (SPEC §2, re-verified §3 item 2 below — WIDER than the SPEC's own claim: no existing H2 fixture carries ANY `retry_policy` at all).
- NO status-code change anywhere — `h2_retry_limit_exceeded_path_always_503`'s existing assertions (status 503, `x-envoy-attempt-count: 2`, all four retry counters) are untouched; only the access-log `rcd`/`rf` fields gain a NEW witness.
- No new fuzz target (SPEC §I — `%RESPONSE_FLAGS%` is an existing operator; no H2 codec/framing change).

---

## §3 PLAN-VERIFY re-confirmation (done this session, before authoring tasks)

All seven SPEC §3 items were re-checked against the live tree (no drift found):

1. **Line numbers confirmed exact.** The per-stream `*_for_log_h2` locals block: `upstream_host_for_log_h2` declared `hcm.rs:536`, `response_code_details_for_log_h2` at `:540`, `upstream_cluster_for_log_h2` at `:545` — SPEC's `:536`-`:544` citation is confirmed (the declaration block spans exactly these lines; the new boolean is added immediately after `:545`). The retry-loop post-loop split: `hcm.rs:815`-`821` (`if attempts > 1 && !retry_budget_blocked { if final_retriable { cluster.upstream_rq_retry_limit_exceeded().inc(); } else { cluster.upstream_rq_retry_success().inc(); } }`) — confirmed exact, no drift. `finalize_h2_stream`'s SOLE call site: `hcm.rs:852`-`866` (the `.await` lands at `:867`). The function signature: `hcm.rs:881`-`908` (body opens `:909`). The three-arm `%RESPONSE_FLAGS%` derive: `hcm.rs:985`-`990`. ALL SPEC citations confirmed byte-exact against the live tree — no sibling-session drift.
2. **Additivity re-grep confirmed (WIDER than SPEC claimed, per the state-1 spec-review finding).** `grep -n "retry_policy\|circuit_breakers"` over every existing H2-listener fixture's `envoy-rust.yaml` (`0009`, `0010`, `0018`, `0021`, `0064`, `0065`, `0066`) finds ZERO `retry_policy` hits in ANY of them (only `0021`'s `circuit_breakers.max_connections: 4` and `0066`'s pool-overflow `circuit_breakers` block — both irrelevant to THIS phase's gate, which requires `attempts > 1 && !retry_budget_blocked && final_retriable`, itself requiring a `retry_policy` on the route). No existing H2 fixture can reach the new gate — confirmed re-derived fresh this session, not trusted from SPEC's enumeration.
3. **§C threading form finalized.** `grep -n "finalize_h2_stream("` over `crates/envoy-http2/src/hcm.rs` confirms exactly ONE call site (`:852`) plus the `async fn` declaration itself (`:881`) — no other caller. A single new `bool` parameter (a `Copy` primitive) is the minimum-viable form; adopted.
4. **§G backstop shape DECIDED: a NEW sibling test, `h2_retry_limit_exceeded_access_log_carries_urx_flag`.** This session found the EXACT H1 precedent this phase should mirror: `h1_retry_limit_exceeded_access_log_carries_urx_flag` (`crates/envoy-http1/src/hcm.rs:7326`) is ALREADY a standalone sibling test (NOT an extension of H1's own counter-focused retry test) — H1 never extended its counter test either. This also matches the phase-56/57/58 H2 convention (`h2_route_miss_access_log_carries_nr_flag`, `h2_no_healthy_access_log_carries_uh_flag`, `h2_pool_overflow_access_log_carries_uo_flag`, `h2_request_budget_overflow_access_log_carries_uo_flag` — ALL new sibling tests, none extended a pre-existing test). Extending `h2_retry_limit_exceeded_path_always_503` in-place was considered and REJECTED: that test's own doc comment and assertion block are scoped to counters/headers (mirroring its H1 counterpart `retry_limit_exceeded_path_always_503`), and every prior H2 access-log flag witness in this sub-family used a dedicated new test — consistency wins, and it avoids growing an unrelated test's assertion surface.
5. **Harness backend-wiring edit shape re-confirmed.** `tests/differential/src/lib.rs:3114`-`3120` (the `needs_health_aware_backend` fixture-name allowlist, currently ending `|| fixture_name == "0059-accesslog-rf-retry-exhausted");` at `:3120`) and `:3179`-`3184` (the per-path `else if fixture_name == "0059-accesslog-rf-retry-exhausted" { Some("/retry-exhausted=503".to_string()) }` arm at `:3179`-`3182`) — both confirmed exact, unchanged since the state-1 recon. This phase adds a `"0067-accesslog-h2-urx-retry-exhausted"` arm to BOTH (mechanical, reusing the IDENTICAL `/retry-exhausted=503` per-path string `0059` already uses — the backend topology is upstream-protocol-agnostic, confirmed by the state-0 recon).
6. **§6.1 split decision: does NOT fire.** This PLAN has 5 tasks / an estimated ~180-260 LoC (a 1-line declare + a 1-line set + a 3-line new-parameter thread (call site + signature) + a ~5-line derive wrapper + one ~75-line in-process backstop test, ~130-150 LoC less than phase 58's pool-overflow backstop because NO pool-manager wiring is needed here (reuses the plain `spawn_fail_then_ok_h1_upstream` + `h1_backend_cluster` + `drive_h2_once` helpers verbatim) + a 4-file fixture (~130 LoC incl. README, closely modeled on `0066`) + a ~25-line differential test + a 4-line harness-wiring edit + two BEHAVIOR_CONTRACT row edits) — well under the ~25-task/~1500-LoC gate. No split; **ADR-0119 stays reserved-but-unfired** for a §6.2 reconciliation (not expected to fire — see item 7 below).
7. **Fixture number `0067` re-confirmed still next-free.** `ls tests/fixtures/ | sort | tail` shows the highest existing fixture is `0066-accesslog-h2-rf-overflow`; no sibling session has landed `0067` in between as of this PLAN-write.

No §6.2 reconciliation ADR is needed — none of SPEC §A-§I is overturned by this re-verification.

---

## Task 1: Retry-loop discriminator + threaded parameter + derive wrapper + in-process backstop (§A + §B + §C + §D + §G)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (locals block `:545`-ish; retry-loop post-loop split `:815`-`821`; `finalize_h2_stream` call site `:852`-`867` and signature `:881`-`908`; derive `:985`-`990`; new test)

**Interfaces:**
- Produces: nothing new consumed by later tasks — Task 2/3 (fixture + differential test) exercise this via the Docker differential, not via Rust symbols.

- [ ] **Step 1: Write the failing retry-limit-exceeded access-log backstop test**

Insert immediately after `h2_retry_limit_exceeded_path_always_503`'s closing brace (`crates/envoy-http2/src/hcm.rs:4474`, right before the next test's doc comment `/// 16 Task 5 (no-retry regression): ...`). Mirrors the H1 backstop `h1_retry_limit_exceeded_access_log_carries_urx_flag` (`crates/envoy-http1/src/hcm.rs:7326`) exactly, reusing the SAME always-503 upstream helper and cluster helper the counter-focused `h2_retry_limit_exceeded_path_always_503` test already uses, plus the existing `drive_h2_once` driving helper (which already drains the body and settles for 100ms — no new driving code needed, unlike phase 58's pool-overflow backstop which needed manual `h2::client` wiring for its pool-manager-specific setup):

```rust
    /// Phase 61 (ADR-0118) §A/§B/§C/§D/§G backstop: drive the H2
    /// retry-limit-exceeded (L9) path — an always-503 H1 upstream
    /// (`spawn_fail_then_ok_h1_upstream(503, 1000)`, fail_count much greater
    /// than the 2 attempts) + `retry_policy{retry_on:"5xx",num_retries:1}` ->
    /// both attempts 503, the budget of 1 consumed, the last 503 surfaced
    /// downstream verbatim -- with a {rc,rcd,rf} FILE json access-log.
    /// Asserts the logged line carries rcd:"via_upstream" (a REAL upstream
    /// 503, UNCHANGED -- matches Envoy, NOT rewritten) and the DERIVED
    /// rf:"URX" (set at the limit-exceeded loop-exit boolean, §B, NOT
    /// rcd-derived). H2 mirror of the H1 backstop
    /// `h1_retry_limit_exceeded_access_log_carries_urx_flag`
    /// (`crates/envoy-http1/src/hcm.rs:7326`). The counter/header assertions
    /// for this SAME topology are ALREADY covered by the existing phase-16
    /// test `h2_retry_limit_exceeded_path_always_503` (above) -- this test
    /// adds ONLY the access-log proof, not a duplicate counter check.
    /// Fail-first: pre-change the derive's rcd-match falls to `_ => "-"`
    /// (via_upstream is unmatched) -> it renders `"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_retry_limit_exceeded_access_log_carries_urx_flag() {
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

        // Always-503 H1 backend: fail_count 1000 >> the 2 attempts -> every
        // attempt 503 -> the retry budget of 1 is consumed -> limit-exceeded (L9).
        let (upstream_addr, _reqs) = spawn_fail_then_ok_h1_upstream(503, 1000).await;
        let (cluster_mgr, _cluster) = h1_backend_cluster(upstream_addr).await;
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test-retry".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
                name: "r".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
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
                            retry_policy: Some(envoy_config::RetryPolicy {
                                retry_on: "5xx".into(),
                                num_retries: Some(1),
                                retriable_status_codes: vec![],
                            }),
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
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mut built = Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
            .await
            .expect("build HCM config");
        built.access_log = vec![sink];
        let config = Arc::new(built);

        let (status, _headers) = drive_h2_once(config).await;
        assert_eq!(
            status, 503,
            "retry-limit-exceeded surfaces the last upstream 503 verbatim"
        );
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n",
            "retry-limit-exceeded access-log line carries rcd:via_upstream + rf:URX: {logged:?}"
        );
    }
```

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http2 h2_retry_limit_exceeded_access_log_carries_urx_flag`
Expected: FAIL — logged line is `{"rc":503,"rcd":"via_upstream","rf":"-"}\n` (the derive has no `URX` arm yet; `via_upstream` falls to `_ => "-"`).

- [ ] **Step 3: Commit the RED test**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 61 task 1: RED test for H2 retry-limit-exceeded access-log rcd/rf [ADR-0118]"
```

- [ ] **Step 4: Declare the boolean (§A)**

Insert immediately after the existing locals block (`crates/envoy-http2/src/hcm.rs:545`, right after `let mut upstream_cluster_for_log_h2: Option<String> = None;`):

Replace exactly:

```rust
    let mut upstream_cluster_for_log_h2: Option<String> = None;
```

with:

```rust
    let mut upstream_cluster_for_log_h2: Option<String> = None;
    // phase 61 (ADR-0118): per-stream %RESPONSE_FLAGS% = "URX" discriminator.
    // URX (UpstreamRetryLimitExceeded) is NOT derivable from
    // %RESPONSE_CODE_DETAILS% (the completing response's rcd stays the
    // shared "via_upstream") — set at the retry-loop's post-loop
    // limit-exceeded exit (§B) and consumed by the derive wrapper (§D).
    // Mirrors the H1 phase-51 `retry_limit_exceeded_for_log` local
    // (crates/envoy-http1/src/hcm.rs:859) exactly.
    let mut retry_limit_exceeded_for_log_h2: bool = false;
```

- [ ] **Step 5: Set the boolean at the retry-limit-exceeded post-loop exit (§B)**

Replace exactly (`crates/envoy-http2/src/hcm.rs:815`-`821`):

```rust
                    if attempts > 1 && !retry_budget_blocked {
                        if final_retriable {
                            cluster.upstream_rq_retry_limit_exceeded().inc();
                        } else {
                            cluster.upstream_rq_retry_success().inc();
                        }
                    }
```

with:

```rust
                    if attempts > 1 && !retry_budget_blocked {
                        if final_retriable {
                            cluster.upstream_rq_retry_limit_exceeded().inc();
                            // phase 61 (ADR-0118) §B: same gate as the counter
                            // above — the boolean and the counter provably
                            // co-fire. Mirrors the H1 phase-51 set-site
                            // (crates/envoy-http1/src/hcm.rs:1126-1128) exactly.
                            retry_limit_exceeded_for_log_h2 = true;
                        } else {
                            cluster.upstream_rq_retry_success().inc();
                        }
                    }
```

- [ ] **Step 6: Thread the boolean through `finalize_h2_stream`'s call site (§C)**

Replace exactly (`crates/envoy-http2/src/hcm.rs:852`-`867`):

```rust
    finalize_h2_stream(
        &config,
        &mut pipeline,
        send_response,
        resp,
        req_arrival_instant,
        req_arrival_systime,
        &envoy_req,
        request_body_len,
        upstream_host_for_log_h2,
        route_name_for_log_h2,
        response_code_details_for_log_h2,
        upstream_cluster_for_log_h2,
        dynamic_metadata,
    )
    .await
```

with:

```rust
    finalize_h2_stream(
        &config,
        &mut pipeline,
        send_response,
        resp,
        req_arrival_instant,
        req_arrival_systime,
        &envoy_req,
        request_body_len,
        upstream_host_for_log_h2,
        route_name_for_log_h2,
        response_code_details_for_log_h2,
        upstream_cluster_for_log_h2,
        dynamic_metadata,
        retry_limit_exceeded_for_log_h2,
    )
    .await
```

- [ ] **Step 7: Thread the boolean through `finalize_h2_stream`'s signature (§C)**

Replace exactly (`crates/envoy-http2/src/hcm.rs:881`-`908`):

```rust
async fn finalize_h2_stream(
    config: &Arc<HCMConfig>,
    pipeline: &mut envoy_filter::FilterPipeline,
    send_response: h2::server::SendResponse<Bytes>,
    mut resp: Response,
    req_arrival_instant: Instant,
    req_arrival_systime: SystemTime,
    envoy_req: &Request,
    request_body_len: u64,
    upstream_host_for_log_h2: Option<String>,
    // Phase 41: the matched route's config `name` (None = unnamed), computed at
    // the `handle_one_stream` match site and threaded here for %ROUTE_NAME%.
    route_name_for_log_h2: Option<String>,
    // Phase 42 (ADR-0099): the per-response-path %RESPONSE_CODE_DETAILS% detail
    // (direct_response / via_upstream / None), computed at the
    // `handle_one_stream` dispatch site and threaded here.
    response_code_details_for_log_h2: Option<String>,
    // Phase 43 (ADR-0100): the routed cluster name (Some on a proxy arm, None
    // for direct_response / synth / error paths), captured at the proxy ARM
    // ENTRY and threaded here for %UPSTREAM_CLUSTER%.
    upstream_cluster_for_log_h2: Option<String>,
    // Phase 33 T10: the pipeline's dynamic metadata, captured before
    // `filter_req` was dropped at the decode site, threaded here so the H2
    // record build can render %DYNAMIC_METADATA%.
    dynamic_metadata: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, String>,
    >,
) -> Result<(), Http2Error> {
```

with:

```rust
async fn finalize_h2_stream(
    config: &Arc<HCMConfig>,
    pipeline: &mut envoy_filter::FilterPipeline,
    send_response: h2::server::SendResponse<Bytes>,
    mut resp: Response,
    req_arrival_instant: Instant,
    req_arrival_systime: SystemTime,
    envoy_req: &Request,
    request_body_len: u64,
    upstream_host_for_log_h2: Option<String>,
    // Phase 41: the matched route's config `name` (None = unnamed), computed at
    // the `handle_one_stream` match site and threaded here for %ROUTE_NAME%.
    route_name_for_log_h2: Option<String>,
    // Phase 42 (ADR-0099): the per-response-path %RESPONSE_CODE_DETAILS% detail
    // (direct_response / via_upstream / None), computed at the
    // `handle_one_stream` dispatch site and threaded here.
    response_code_details_for_log_h2: Option<String>,
    // Phase 43 (ADR-0100): the routed cluster name (Some on a proxy arm, None
    // for direct_response / synth / error paths), captured at the proxy ARM
    // ENTRY and threaded here for %UPSTREAM_CLUSTER%.
    upstream_cluster_for_log_h2: Option<String>,
    // Phase 33 T10: the pipeline's dynamic metadata, captured before
    // `filter_req` was dropped at the decode site, threaded here so the H2
    // record build can render %DYNAMIC_METADATA%.
    dynamic_metadata: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, String>,
    >,
    // Phase 61 (ADR-0118): the retry-limit-exceeded loop-exit discriminator
    // (§B), consumed by the %RESPONSE_FLAGS% derive wrapper (§D). A `Copy`
    // primitive — no lifetime/ownership complications, unlike the
    // `Option<String>` fields above.
    retry_limit_exceeded_for_log_h2: bool,
) -> Result<(), Http2Error> {
```

- [ ] **Step 8: Extend the `%RESPONSE_FLAGS%` derive with the boolean-gated wrapper (§D)**

Replace exactly (`crates/envoy-http2/src/hcm.rs:985`-`990`):

```rust
        let response_flags_for_log_h2: &str = match response_code_details_for_log_h2.as_deref() {
            Some("route_not_found") => "NR",
            Some("no_healthy_upstream") => "UH",
            Some("upstream_reset_before_response_started{overflow}") => "UO",
            _ => "-",
        };
```

with:

```rust
        // Phase 61 (ADR-0118): "URX" (UpstreamRetryLimitExceeded) is NOT
        // derivable from %RESPONSE_CODE_DETAILS% (the completing response's
        // rcd stays the shared "via_upstream") — checked FIRST via the
        // boolean, mirroring the H1 phase-51 wrapper exactly
        // (crates/envoy-http1/src/hcm.rs:1391-1392).
        let response_flags_for_log_h2: &str = if retry_limit_exceeded_for_log_h2 {
            "URX"
        } else {
            match response_code_details_for_log_h2.as_deref() {
                Some("route_not_found") => "NR",
                Some("no_healthy_upstream") => "UH",
                Some("upstream_reset_before_response_started{overflow}") => "UO",
                _ => "-",
            }
        };
```

- [ ] **Step 9: Run the Task-1 test to verify it PASSES**

Run: `cargo test -p envoy-http2 h2_retry_limit_exceeded_access_log_carries_urx_flag`
Expected: PASS (logged line `{"rc":503,"rcd":"via_upstream","rf":"URX"}\n`).

- [ ] **Step 10: Run the full crate test suite to verify NO regression**

Run: `cargo test -p envoy-http2`
Expected: PASS, including:
- the existing `h2_retry_limit_exceeded_path_always_503` (phase 16) — status/header/counter assertions unchanged (the boolean-set is additive; no counter/status/header touched);
- `h2_retry_success_path_503_then_200` and every OTHER retry-family test (`attempts > 1 && !retry_budget_blocked` with `final_retriable == false` on the success path — the boolean stays `false`, `else` branch fires, unaffected);
- the phase-56/57/58 backstops (`h2_route_miss_access_log_carries_nr_flag`, `h2_host_miss_access_log_carries_nr_flag`, `h2_no_healthy_access_log_carries_uh_flag`, `h2_pool_overflow_access_log_carries_uo_flag`, `h2_request_budget_overflow_access_log_carries_uo_flag`) — NONE of them configures a `retry_policy`, so `retry_limit_exceeded_for_log_h2` stays `false` and the derive's `else` branch (the unchanged three-arm match) fires identically;
- every other test in the crate (`attempts > 1` requires a `retry_policy` with a budget — absent everywhere else).

- [ ] **Step 11: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 61 task 1: H2 retry-limit-exceeded rf:URX discriminator + derive wrapper (GREEN) [ADR-0118]"
```

---

## Task 2: Fixture `0067-accesslog-h2-urx-retry-exhausted` (one probe: H2 retry-limit-exceeded 503)

Build the fixture from the `0059` template (the H1 `URX` witness) crossed with `0064`/`0065`/`0066`'s H2C listener shape. UNLIKE fixture `0066`, the cluster is a PLAIN `STRICT_DNS` H1-upstream cluster — NO `typed_extension_protocol_options` needed (the state-0 recon and the existing phase-16 in-process test both confirm the retry loop is upstream-protocol-agnostic).

**Files:**
- Create: `tests/fixtures/0067-accesslog-h2-urx-retry-exhausted/envoy.yaml`
- Create: `tests/fixtures/0067-accesslog-h2-urx-retry-exhausted/envoy-rust.yaml`
- Create: `tests/fixtures/0067-accesslog-h2-urx-retry-exhausted/expectations.yaml`
- Create: `tests/fixtures/0067-accesslog-h2-urx-retry-exhausted/README.md`

- [ ] **Step 1: Create `envoy.yaml`** (reference side — `admin` block present, bind `0.0.0.0`, mount path `/tmp/0067-envoy-mount/access.log`, backend host `{{BACKEND_HOST}}`/`{{BACKEND_PORT}}` per the harness's `HealthAwareHttp1Backend` convention)

```yaml
node: { id: envoy-rust-phase-61-fixture-0067, cluster: envoy-rust-phase-61 }
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
                      path: /tmp/0067-envoy-mount/access.log
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
                    - name: retry_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/retry-exhausted" }
                          route:
                            cluster: backend_cluster
                            retry_policy:
                              retry_on: "5xx"
                              num_retries: 1
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # STRICT_DNS, plain H1 upstream (NO typed_extension_protocol_options —
    # the retry loop is upstream-protocol-agnostic, confirmed by the
    # state-0 recon AND the existing phase-16 in-process test). The
    # health-aware backend serves 503 on every /retry-exhausted attempt
    # (stateless), so the retry budget of 1 is consumed and the last 503
    # surfaces downstream verbatim.
    - name: backend_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{BACKEND_PORT}} }
```

- [ ] **Step 2: Create `envoy-rust.yaml`** (subject side — NO `admin` block, bind `127.0.0.1`, mount path `/tmp/0067-envoy-rust-mount/access.log`; otherwise byte-identical)

```yaml
node: { id: envoy-rust-phase-61-fixture-0067, cluster: envoy-rust-phase-61 }
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
                      path: /tmp/0067-envoy-rust-mount/access.log
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
                    - name: retry_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/retry-exhausted" }
                          route:
                            cluster: backend_cluster
                            retry_policy:
                              retry_on: "5xx"
                              num_retries: 1
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{BACKEND_PORT}} }
```

- [ ] **Step 3: Create `expectations.yaml`** (one probe — H2 retry-limit-exceeded 503 — reuses `Driver::Http2AccessLogByteExact` verbatim)

```yaml
driver:
  kind: http2_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0067-envoy-mount/access.log
    envoy_rust: /tmp/0067-envoy-rust-mount/access.log
  probes:
    # Probe 1: GET /retry-exhausted, routed to `backend_cluster` via
    # retry_policy{retry_on:"5xx",num_retries:1}. The health-aware backend
    # serves 503 on EVERY attempt (stateless) -> both attempts 503 -> the
    # retry budget of 1 is consumed -> limit-exceeded (L9) -> the last 503
    # surfaces downstream verbatim. This is the FOURTH non-`-` H2
    # %RESPONSE_FLAGS% witness: URX (UpstreamRetryLimitExceeded) (phase 61,
    # ADR-0118), the H2 analogue of fixture 0059 (phase 51).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static
    # literal: the `http2_access_log_byte_exact` driver asserts every line is
    # byte-identical between upstream Envoy v1.33.0 and envoy-rust. The
    # retry-limit-exceeded rcd (via_upstream, a REAL completing 503, not a
    # synth) is deterministic on BOTH sides.
    #
    # Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rcd, rf.
    # The emitted line is:
    #   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}
    - method: get
      path: /retry-exhausted
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `README.md`**

```markdown
# Fixture 0067 — H2 access-log `%RESPONSE_FLAGS%` retry-limit-exceeded failure path (`URX`, byte-exact)

The H2 analogue of fixture `0059` (phase 51, the H1 `URX` witness) and the
FOURTH fixture built on `Driver::Http2AccessLogByteExact` (opened by phase
56, fixture `0064`; extended by phase 57's `0065` and phase 58's `0066`).
Phase 61 (ADR-0118) witnesses the FOURTH H2 `%RESPONSE_FLAGS%` value, `URX`
(UpstreamRetryLimitExceeded), byte-exact on the H2 retry-limit-exceeded 503
path.

## What this proves

Before this phase, envoy-rust's H2 `%RESPONSE_FLAGS%` derive had no arm for
the retry-limit-exceeded disposition — its completing response's rcd is the
SAME `via_upstream` string a normal successful upstream response carries, so
it fell to the derive's `_ => "-"` arm. Phase 61 (i) declares a new
per-stream boolean discriminator, (ii) sets it at the retry-loop's EXISTING
post-loop limit-exceeded exit (the same gate that already ticks
`upstream_rq_retry_limit_exceeded`), (iii) threads it through
`finalize_h2_stream`'s sole call site, and (iv) wraps the existing
three-arm derive with a boolean-gated check. UNLIKE fixtures `0058`
(phase 50) / `0065` (phase 57), NO status-code correction was needed — the
underlying H2 retry-limit-exceeded mechanics (status 503,
`x-envoy-attempt-count: 2`, all four retry counters) were ALREADY correct
and ALREADY covered by the existing phase-16 in-process test
`h2_retry_limit_exceeded_path_always_503`.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /retry-exhausted` | retry-limit-exceeded (`num_retries:1`, always-503 backend) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}
```

The cluster is a PLAIN `STRICT_DNS` H1-upstream cluster (NO
`typed_extension_protocol_options`) — the retry loop is upstream-protocol-
agnostic, confirmed by BOTH the state-0 recon (live Envoy emits the
identical rcd/rf pair regardless of the cluster's upstream protocol) and the
existing phase-16 in-process test (which already drives an H1-protocol
backend through the H2 downstream path). This differs from fixture `0066`,
whose pool-overflow arm required an H2-upstream cluster to route through the
H2 connection pool.

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness driver change this phase. The backend
wiring gate (`tests/differential/src/lib.rs`'s `needs_health_aware_backend`
allowlist + the `/retry-exhausted=503` per-path arm, both previously keyed
to `0059` only) gains a `"0067-accesslog-h2-urx-retry-exhausted"` arm
(mechanical, two additions reusing the IDENTICAL per-path string `0059`
already uses).

## `0001`-`0066` byte-preservation

This phase's change is additive — gated on `attempts > 1 &&
!retry_budget_blocked && final_retriable`, which requires a route
`retry_policy` whose budget is fully consumed by consecutive retriable
outcomes. NONE of the pre-existing H2 fixtures (`0009`, `0010`, `0018`,
`0021`, `0064`, `0065`, `0066`) configures ANY `retry_policy` at all
(re-confirmed by a fresh `grep -n retry_policy` over each
`envoy-rust.yaml` — zero hits), so `0001`-`0066` stay byte-identical; only
the new `0067` observes the new `rf:"URX"` witness.

## Cross-references

- ADR: ADR-0118 (state-1 brainstorm + state-2 PLAN — the H2 `URX` witness).
- Related fixtures: `0059` (the H1 `URX` witness this fixture mirrors on
  H2); `0064`/`0065`/`0066` (the H2 `NR`/`UH`/`UO` witnesses that
  opened/extended `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%` strings
  beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`, still open
  for future one-flag-at-a-time phases.
```

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0067-accesslog-h2-urx-retry-exhausted/
git commit -m "phase 61 task 2: fixture 0067-accesslog-h2-urx-retry-exhausted (one probe, H2 rf:URX byte-exact) [ADR-0118]"
```

---

## Task 3: Differential test `access_log_h2_urx_retry_exhausted.rs` + harness backend-wiring edit (§E + §F)

**Files:**
- Create: `tests/differential/tests/access_log_h2_urx_retry_exhausted.rs` (a structural clone of `access_log_h2_rf_overflow.rs`, pointing at the `0067` fixture)
- Modify: `tests/differential/src/lib.rs` (`needs_health_aware_backend` allowlist `:3114`-`3120`; the per-path arm `:3179`-`3184`)

- [ ] **Step 1: Write the differential test wrapper**

```rust
//! Docker-gated differential test for fixture
//! 0067-accesslog-h2-urx-retry-exhausted.
//! Phase 61 (ADR-0118) — the FOURTH H2 `%RESPONSE_FLAGS%` witness: `URX`
//! (UpstreamRetryLimitExceeded), byte-exact cross-proxy on the H2
//! retry-limit-exceeded 503 path — the H2 analogue of fixture `0059`
//! (phase 51). A `STRICT_DNS` plain-H1-upstream cluster with
//! `retry_policy:{retry_on:"5xx",num_retries:1}` against an always-503
//! health-aware backend — both attempts 503, the retry budget of 1
//! consumed, the last 503 surfaced downstream verbatim. Spawns Envoy v1.33
//! in a container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted
//! line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}
//! PURE cross-proxy equality (no static literal). UNLIKE fixtures
//! 0058/0065, no status-code correction was needed this phase — only rf
//! changes (rcd was already the correct `via_upstream`).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_urx_retry_exhausted() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0067-accesslog-h2-urx-retry-exhausted");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Add the `0067` arm to the `needs_health_aware_backend` allowlist**

Replace exactly (`tests/differential/src/lib.rs:3114`-`3120`):

```rust
    let needs_health_aware_backend = needs_backend
        && (fixture_name == "0019-upstream-active-health-check"
            || fixture_name == "0020-upstream-connection-pooling-and-per-class-counters"
            || fixture_name == "0022-upstream-outlier-detection-consecutive-5xx"
            || fixture_name == "0024-upstream-retry-on-5xx"
            || fixture_name == "0025-upstream-circuit-breaker-retry-budget"
            || fixture_name == "0059-accesslog-rf-retry-exhausted");
```

with:

```rust
    let needs_health_aware_backend = needs_backend
        && (fixture_name == "0019-upstream-active-health-check"
            || fixture_name == "0020-upstream-connection-pooling-and-per-class-counters"
            || fixture_name == "0022-upstream-outlier-detection-consecutive-5xx"
            || fixture_name == "0024-upstream-retry-on-5xx"
            || fixture_name == "0025-upstream-circuit-breaker-retry-budget"
            || fixture_name == "0059-accesslog-rf-retry-exhausted"
            || fixture_name == "0067-accesslog-h2-urx-retry-exhausted");
```

- [ ] **Step 3: Add the `0067` arm to the per-path `/retry-exhausted=503` mapping**

Replace exactly (`tests/differential/src/lib.rs:3179`-`3184`):

```rust
            let per_path = if fixture_name == "0024-upstream-retry-on-5xx" {
                Some("/retry-exhausted=503".to_string())
            } else if fixture_name == "0025-upstream-circuit-breaker-retry-budget" {
                Some("/budget-blocked=503".to_string())
            } else if fixture_name == "0059-accesslog-rf-retry-exhausted" {
                // phase 51 (ADR-0108) fixture 0059: the retry-limit-exceeded (L9)
                // access-log %RESPONSE_FLAGS%=URX witness. STATELESS always-503
                // `/retry-exhausted` (retry_script stays None — both attempts 503,
                // the budget of 1 consumed, the last 503 surfaced verbatim).
                Some("/retry-exhausted=503".to_string())
            } else {
                per_path
            };
```

with:

```rust
            let per_path = if fixture_name == "0024-upstream-retry-on-5xx" {
                Some("/retry-exhausted=503".to_string())
            } else if fixture_name == "0025-upstream-circuit-breaker-retry-budget" {
                Some("/budget-blocked=503".to_string())
            } else if fixture_name == "0059-accesslog-rf-retry-exhausted"
                || fixture_name == "0067-accesslog-h2-urx-retry-exhausted"
            {
                // phase 51 (ADR-0108) fixture 0059 / phase 61 (ADR-0118)
                // fixture 0067: the H1/H2 retry-limit-exceeded (L9) access-log
                // %RESPONSE_FLAGS%=URX witnesses. STATELESS always-503
                // `/retry-exhausted` (retry_script stays None — both attempts
                // 503, the budget of 1 consumed, the last 503 surfaced
                // verbatim). Identical per-path mapping reused for both
                // fixtures — the retry loop is upstream-protocol-agnostic.
                Some("/retry-exhausted=503".to_string())
            } else {
                per_path
            };
```

- [ ] **Step 4: Compile-check the differential crate**

Run: `cargo test -p differential --no-run`
Expected: compiles clean (the `0067` fixture deserializes against the existing `Driver::Http2AccessLogByteExact`; the two-arm harness edit is a mechanical string-match addition, no new function/type).

> **NOTE (host-environment):** the Docker differential (real Envoy vs envoy-rust) is **CI-authoritative** at the state-4 §7.5 gate (see memory `envoy-rust-state4-ci-first-execution`). This host may not run the container reliably; do NOT treat a local Docker-gated non-run as a failure. The differential `0067` green + all `0001`-`0066` still green are confirmed by the state-4 CI run.

- [ ] **Step 5: Commit**

```bash
git add tests/differential/tests/access_log_h2_urx_retry_exhausted.rs tests/differential/src/lib.rs
git commit -m "phase 61 task 3: differential test access_log_h2_urx_retry_exhausted (fixture 0067) + harness backend-wiring [ADR-0118]"
```

---

## Task 4: BEHAVIOR_CONTRACT updates (§H)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_FLAGS%` row, `:1020`; the `%RESPONSE_CODE_DETAILS%` row, `:1031`)

- [ ] **Step 1: Update the `%RESPONSE_FLAGS%` row's H2-witness sentence**

Replace exactly (the trailing H2 sentence of the row — a substring of the giant single-line row at `:1020`, unique in the file):

```
The remaining H2 `%RESPONSE_FLAGS%` values (`URX`/`UF`/`UC`) remain deferred as the continuing carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

with:

```
`URX` is now ALSO witnessed byte-exact on H2 by fixture **0067** (phase 61, ADR-0118) — set via the retry-loop's post-loop limit-exceeded exit boolean (NOT derivable from `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` on this path — the SAME non-rcd-derivable pattern H1 established at phase 51), threaded through `finalize_h2_stream` as a new parameter — ADVANCING carry-forward **M56-1** (the `URX` slice consumed). UNLIKE phases 50/57, NO status-code correction was needed — envoy-rust's H2 retry-limit-exceeded mechanics (status 503, `x-envoy-attempt-count: 2`, all four retry counters) were ALREADY correct and ALREADY covered by an existing phase-16 in-process test. The remaining H2 `%RESPONSE_FLAGS%` values (`UF`/`UC`) remain deferred as the continuing carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

- [ ] **Step 2: Update the `%RESPONSE_CODE_DETAILS%` row's H2-witness sentence**

Replace exactly (a substring of the giant single-line row at `:1031`, unique in the file):

```
`upstream_reset_before_response_started{overflow}` is now ALSO witnessed on H2 (fixture **0066**, phase 58, ADR-0115), set on BOTH the H2 pool-overflow arm and the H2 request-budget arm. The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`) remain deferred as the continuing carry-forward **M56-1**.
```

with:

```
`upstream_reset_before_response_started{overflow}` is now ALSO witnessed on H2 (fixture **0066**, phase 58, ADR-0115), set on BOTH the H2 pool-overflow arm and the H2 request-budget arm. The H2 retry-limit-exceeded path (fixture **0067**, phase 61, ADR-0118) is now ALSO witnessed on H2, but its `%RESPONSE_CODE_DETAILS%` stays the shared `via_upstream` (a REAL completing 503, unchanged) — `%RESPONSE_FLAGS%`=`URX` is the discriminating signal there, NOT this field. The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`) remain deferred as the continuing carry-forward **M56-1**.
```

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 61 task 4: BEHAVIOR_CONTRACT rf/rcd rows — H2 URX witnessed (fixture 0067) [ADR-0118]"
```

---

## Task 5: Local verification sweep (state-3 close-out; full §7.5 gate runs at state-4)

This is the developer's local pre-flight — NOT the state-4 verification gate (that re-runs the full §7.5 set in CI and quotes outputs to `PROGRESS.md`). Run the cheap-and-local subset; the Docker differential + `0001`-`0066` byte-identical + h2spec are CI-authoritative at state-4.

**Files:** none (verification only)

- [ ] **Step 1: clippy clean**

Run: `cargo clippy -p envoy-http2 -p differential --all-targets --all-features -- -D warnings`
Expected: no warnings (the boolean declare/set/thread and the `if`-wrapped derive are idiomatic; no new lint surface).

- [ ] **Step 2: fmt clean**

Run: `cargo fmt --all -- --check`
Expected: clean. (If any inserted block reflows, run `cargo fmt --all` and re-commit — see memory `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase.)

- [ ] **Step 3: full workspace unit tests** (non-Docker)

Run: `cargo test --workspace`
Expected: PASS (the new backstop + all existing tests; the differential Docker tests are Docker-gated and skip locally per the harness's own gating — do not treat a local skip as a failure).

- [ ] **Step 4: confirm byte-preservation reasoning (no existing H2 fixture regressed)**

Run: `for f in 0009 0010 0018 0021 0064 0065 0066; do echo "=== $f ==="; grep -n retry_policy tests/fixtures/${f}-*/envoy-rust.yaml || echo "(none)"; done`
Expected: every fixture shows `(none)` — re-confirms NONE configures a `retry_policy`, so none can newly reach `attempts > 1 && !retry_budget_blocked && final_retriable`; `0001`-`0066` stay byte-identical; only `0067` observes the new `rf:"URX"` witness.

- [ ] **Step 5: final fmt-fix commit if needed** (otherwise nothing to commit)

```bash
cargo fmt --all
git add -A && git commit -m "phase 61: cargo fmt [ADR-0118]" || echo "nothing to reformat"
```

---

## Scope / gate summary

- **Task count:** 5 tasks (~180-260 LoC: a 1-line boolean declare + a 1-line set + a 3-line new-parameter thread (call site + signature) + a ~5-line derive wrapper + one ~75-line in-process backstop test (smaller than phase 58's pool-overflow backstop — no pool-manager wiring needed, reuses `spawn_fail_then_ok_h1_upstream`/`h1_backend_cluster`/`drive_h2_once` verbatim) + a 4-file fixture (~130 LoC incl. README, closely modeled on `0066`) + a ~25-line differential test + a 4-line harness-wiring edit + two BEHAVIOR_CONTRACT row edits). **§6.1 split does NOT fire** (well under ~25 tasks / ~1500 LoC — re-confirmed §3 item 6 above). **ADR-0119 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the standing lapsed-reservation convention, unless a future §6.2 reconciliation for THIS phase needs it — not expected).
- **No new** `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant. `#![forbid(unsafe_code)]` holds. NO status-code change anywhere.
- **Additive invariant:** all `0001`-`0066` fixtures stay byte-identical (§3 item 2 above; re-verified Task 5 Step 4). Only the new boolean-gated `URX` arm changes behavior, gated on a route `retry_policy` whose budget is fully consumed — and NO existing H2 fixture configures ANY `retry_policy` at all.
- **Acceptance (re-run at state-4, SPEC §5):** (a) fixture `0067` green (cross-proxy-equal status `503` + whole-line `{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"via_upstream","rf":"URX"}`) + (b) all `0001`-`0066` green simultaneously + (c) h2spec ≥95% (no H2 codec/framing change) + (d) no new fuzz target (SPEC §I) + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved.
- **Carry-forwards:** this phase ADVANCES **M56-1** (consumes the `URX` slice; `UF`/`UC` + the remaining H2 failure-path rcd strings stay open). M57-1 + M55-1 + M53-2 + M53-3 + M48-2 + M42-1 + the `DC`/retry-budget-overflow slices of M45-2 + the phase-58 candidate carry-forward (H2 request-budget arm's own differential fixture) + M40-1 + M39-1/M39-2 + M38-1/M38-2 + CF-39-1 + the HTTP-filters-family (1)-(4) + older stay live; NONE blocks.

_The state-3 implementation (`superpowers:executing-plans` or `superpowers:subagent-driven-development`) is the session AFTER this PLAN lands. Per §5.1, one state per session: this session writes the PLAN only._
