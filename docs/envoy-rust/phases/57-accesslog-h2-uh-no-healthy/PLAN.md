# Phase 57 — `57-accesslog-h2-uh-no-healthy` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (failing test first) per doctrine D-3.1.

**Goal:** Witness the SECOND H2 `%RESPONSE_FLAGS%` value, `UH` (NoHealthyUpstream), byte-exact on the H2 `pick()->None` no-healthy-upstream 503 path — correcting envoy-rust's H2 no-healthy synth status 502 → 503 in the same motion, via a NEW fixture `0065`.

**Architecture:** A two-site source fix in `crates/envoy-http2/src/hcm.rs` (a new `synth_h2_no_healthy_upstream()` helper replacing the `synth_h2_502()` call at the ONE `pick()->None` arm inside `run_h2_attempt`; the caller-loop's `if`/`else` split setting `response_code_details_for_log_h2 = Some("no_healthy_upstream")` on the `else` arm) plus a one-arm extension to the existing H2 `%RESPONSE_FLAGS%` derive (phase 56) — reusing the existing `Driver::Http2AccessLogByteExact` harness verbatim (no harness change). The H2 analogue of fixture `0057` (phase 49, the H1 `UH` witness): the IDENTICAL `subset_cluster`/`metadata_match`/NO_FALLBACK trigger, only `codec_type: HTTP2` substituted.

**Tech Stack:** Rust (`crates/envoy-http2`), `envoy-config`/`envoy-cluster`/`envoy-accesslog` (test-util), the `h2` crate (client handshake in in-process backstops), `tokio` test harness, the Docker-gated differential harness (`tests/differential`, `kind: http2_access_log_byte_exact`), fixture data under `tests/fixtures/`.

## Global Constraints

- `#![forbid(unsafe_code)]` holds — no `unsafe` anywhere in this phase.
- NO new `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant (SPEC §2).
- Load-bearing additivity invariant: all `0001`-`0064` fixtures stay byte-identical (SPEC §2, re-verified §3.2 below).
- `synth_h2_502()`'s OTHER call sites (connect-error `hcm.rs:387`, send-error `hcm.rs:398`) are UNTOUCHED — they remain 502, deferred as the continuing M56-1 carry-forward.
- No new fuzz target (SPEC §H — `%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%` are existing operators; no H2 codec/framing change).

---

## §3 PLAN-VERIFY re-confirmation (done this session, before authoring tasks)

All six SPEC §3 items were re-checked against the live tree (no drift found):

1. **Line numbers confirmed exact.** `run_h2_attempt`'s `pick()->None` arm: the `synth_h2_502()` call is at `hcm.rs:189` (inside the `else` block of the `let Some(endpoint) = cluster.pick_endpoint(...) else { ... }` starting at `:186`). The H2 `response_flags` one-arm derive: `hcm.rs:948` (`let response_flags_for_log_h2: &str = match ...`). The caller-loop `if let Some(endpoint) = attempt.endpoint { ... }` block: `hcm.rs:688`-`694` (exact). All three match SPEC's citations exactly — no drift.
2. **Additivity re-grep confirmed.** `grep -c lb_subset_config` over fixtures `0009`, `0010`, `0021`, `0064`'s `envoy-rust.yaml` returns `0` for all four — none configures `lb_subset_config` or an empty CLA on an H2 listener, so none can reach `pick()->None`. The new caller-loop `else` branch and the new derive arm are unreachable by any pre-existing fixture.
3. **`synth_h2_502()`'s other call sites re-confirmed at `hcm.rs:387` and `hcm.rs:398`** (the spec-review's corrected citations — this session's own independent re-grep matches). Both are inside a DIFFERENT function's connect-error/send-error arms, structurally distinct from the pick-none arm at `:189` — the §A replacement touches only the one call site.
4. **Exact H1 `UH` backstop test name confirmed: `h1_no_healthy_access_log_carries_uh_flag`** (`crates/envoy-http1/src/hcm.rs:5530`). Cited as the §F pattern below.
5. **§6.1 split decision: does NOT fire.** This PLAN has 6 tasks / an estimated ~250-350 LoC (helper fn ~15 LoC + call-site swap ~5 LoC + caller-loop else ~3 LoC + derive arm ~2 LoC + two in-process tests ~130 LoC + a 4-file fixture ~120 LoC + a ~25-line differential test + BEHAVIOR_CONTRACT edits) — well under the ~25-task/~1500-LoC gate. No split; ADR-0115 stays reserved-but-unfired (reclaimed by the next NEW phase pick per the standing lapsed-reservation convention).
6. **Fixture number `0065` re-confirmed still next-free.** `ls tests/fixtures/ | sort | tail` shows the highest existing fixture is `0064-accesslog-h2-rf-no-route`; no sibling session has landed `0065` in between.

No §6.2 reconciliation ADR is needed — none of SPEC §A-§H is overturned.

---

## Task 1: Correct the H2 no-healthy synth status 502 → 503 (§A)

Add a new `synth_h2_no_healthy_upstream()` helper (mirroring H1's `synth_no_healthy_upstream`: status 503, body byte-exact `no healthy upstream`, 19 bytes, NO trailing newline, headers `{server, content-type}` — the SAME H2-appropriate header set `synth_h2_502()` uses, no `content-length`/`connection` — the differential fixture's assertion is the access-log line + status only, not headers/body). Replace the `synth_h2_502()` call at the `pick()->None` arm with it.

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (new fn after `synth_h2_502()`, `~:1050`; the pick-none arm `:186`-`:194`; the doc comment above `run_h2_attempt`, `:181`-`:184`; test module — new helper `cluster_mgr_no_fallback_subset()` + new test)

**Interfaces:**
- Produces: `fn synth_h2_no_healthy_upstream() -> Response` (crate-private, same signature shape as `synth_h2_502()`/`synth_h2_overflow()`) — used by Task 1's call-site swap and by no other task.
- Produces (test-only): `async fn cluster_mgr_no_fallback_subset() -> Arc<envoy_cluster::ClusterManager>` in the `#[cfg(test)] mod tests` block — reused by Task 2's backstop test.

- [ ] **Step 1: Add `LbMetadata` to the test module's `envoy_config` import list**

`crates/envoy-http2/src/hcm.rs` test module currently imports (near `:1084`):

```rust
    use envoy_config::{
        AppendAction, CodecType, DataSource, DirectResponse, HeaderMatcher, HeaderMatcherMode,
        HttpConnectionManagerConfig, HttpFilter, HttpFilterTypedConfig, Route, RouteAction,
        RouteAction_Route, RouteConfiguration, RouteMatch, RouterConfig, VirtualHost,
    };
```

Replace with (adds `LbMetadata`, alphabetically inserted):

```rust
    use envoy_config::{
        AppendAction, CodecType, DataSource, DirectResponse, HeaderMatcher, HeaderMatcherMode,
        HttpConnectionManagerConfig, HttpFilter, HttpFilterTypedConfig, LbMetadata, Route,
        RouteAction, RouteAction_Route, RouteConfiguration, RouteMatch, RouterConfig, VirtualHost,
    };
```

- [ ] **Step 2: Add the `cluster_mgr_no_fallback_subset()` test helper**

Insert into the test module (e.g. immediately before the `h2_route_miss_access_log_carries_nr_flag` test, `~:2296`) — an exact structural clone of the H1 test helper of the same name (`crates/envoy-http1/src/hcm.rs:5399`), duplicated here because test helpers are not shared cross-crate:

```rust
    /// Phase 57 (ADR-0114): a ClusterManager with ONE STATIC cluster
    /// `subset_cluster` carrying an `lb_subset_config` (single selector
    /// `keys:[stage]`, NO_FALLBACK). The ONE endpoint's `envoy.lb` metadata is
    /// `{stage: prod}` at the LITERAL unreachable address `127.0.0.1:1` — a
    /// route `metadata_match` selecting the non-existent `stage: nonexistent`
    /// subset makes `pick_endpoint` return `None` (NO_FALLBACK, no eligible
    /// subset) without ever dialing the endpoint. Structural clone of the H1
    /// helper of the same name (`crates/envoy-http1/src/hcm.rs:5399`) — test
    /// helpers are not shared cross-crate.
    async fn cluster_mgr_no_fallback_subset() -> Arc<envoy_cluster::ClusterManager> {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners: []
  clusters:
    - name: subset_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: NO_FALLBACK
        subset_selectors:
          - keys: [stage]
      load_assignment:
        cluster_name: subset_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
                metadata:
                  filter_metadata:
                    envoy.lb: { stage: prod }
"#;
        let bootstrap = envoy_config::parse_bootstrap(yaml).expect("bootstrap parses");
        Arc::new(
            envoy_cluster::from_bootstrap(&bootstrap, Arc::new(envoy_stats::StatsRegistry::new()))
                .await
                .expect("cluster mgr"),
        )
    }
```

- [ ] **Step 3: Write the failing status-only test**

Insert immediately after the helper from Step 2:

```rust
    /// Phase 57 (ADR-0114) Task 1: the H2 `pick()->None` no-healthy-upstream
    /// arm (`run_h2_attempt`, `hcm.rs:186`-`194`) must emit Envoy's byte-exact
    /// 503 `no healthy upstream` local reply — matching the H1
    /// `synth_no_healthy_upstream` precedent — NOT the generic H2 502
    /// (`synth_h2_502()`) it emits today. Drives a NO_FALLBACK subset-miss
    /// route (the fixture-0057/0038 pattern) over a real H2 connection.
    /// Fail-first: pre-change, the pick-none arm still calls `synth_h2_502()`
    /// → status 502, empty body.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_no_healthy_upstream_returns_503() {
        let mut envoy_lb = std::collections::BTreeMap::new();
        envoy_lb.insert("stage".to_string(), "nonexistent".to_string());
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
                            cluster: "subset_cluster".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: Some(LbMetadata { envoy_lb }),
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
        let cluster_mgr = cluster_mgr_no_fallback_subset().await;
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let config = Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
                .await
                .expect("build HCM config"),
        );

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
            "H2 no-healthy pick()->None arm must emit Envoy's 503, not the generic 502"
        );
        let mut body = resp.into_body();
        let mut collected: Vec<u8> = Vec::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(
            collected, b"no healthy upstream",
            "H2 no-healthy synth-503 body must be byte-exact"
        );
    }
```

- [ ] **Step 4: Run it RED**

Run: `cargo test -p envoy-http2 h2_no_healthy_upstream_returns_503`
Expected: FAIL on the `assert_eq!(resp.status(), 503, ...)` — the pick-none arm still calls `synth_h2_502()`, which returns status 502.

- [ ] **Step 5: Commit the RED test**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 57 task 1: RED test for H2 no-healthy pick()->None 503 status [ADR-0114]"
```

- [ ] **Step 6: Add the `synth_h2_no_healthy_upstream()` helper**

Insert immediately after `synth_h2_502()`'s closing brace (`~:1050`, before the `synth_h2_overflow` doc comment):

```rust

/// 57 (ADR-0114) §A: the H2 no-healthy `pick()->None` synth-503 — mirrors
/// `envoy_http1::hcm::synth_no_healthy_upstream` exactly (status 503, body
/// byte-exact `no healthy upstream`, 19 bytes, NO trailing newline). Used
/// ONLY at `run_h2_attempt`'s `pick()->None` arm. Headers mirror
/// `synth_h2_502`'s H2-appropriate set (`server`, `content-type` — H2 has its
/// own connection lifecycle, no `connection` header, and the differential
/// fixture asserts status + access-log line only, not headers/body).
/// `synth_h2_502()`'s OTHER call sites (connect-error `:387`, send-error
/// `:398`) are UNCHANGED — still 502, deferred as the continuing M56-1
/// carry-forward (the H2 `UF`/`UC` slices, future phases).
fn synth_h2_no_healthy_upstream() -> Response {
    Response {
        status: 503,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
        ],
        body: Bytes::from_static(b"no healthy upstream"),
    }
}
```

- [ ] **Step 7: Swap the call site + update the doc comment/warn text**

Replace exactly (the doc comment above `run_h2_attempt` at `:181`-`:184`):

```rust
/// 16 Task 5: run ONE upstream attempt on the H2 path — pick an endpoint,
/// dispatch over the cluster's upstream protocol (H1-or-H2 fork lives INSIDE
/// here so the retry loop stays protocol-agnostic), and translate the upstream
/// response into a downstream `Response`. Pure of all counter side effects
```

with (adds one line about the phase-57 correction — the rest of the doc comment block is unchanged, do not touch lines below it):

```rust
/// 16 Task 5: run ONE upstream attempt on the H2 path — pick an endpoint,
/// dispatch over the cluster's upstream protocol (H1-or-H2 fork lives INSIDE
/// here so the retry loop stays protocol-agnostic), and translate the upstream
/// response into a downstream `Response`. 57 (ADR-0114): the `pick()->None`
/// arm now emits the dedicated `synth_h2_no_healthy_upstream()` 503 (matching
/// Envoy), not the generic `synth_h2_502()`. Pure of all counter side effects
```

Then replace exactly (the pick-none arm, `:186`-`:194`):

```rust
    let Some(endpoint) = cluster.pick_endpoint(request_hash_key, subset_match) else {
        tracing::warn!(cluster = %cluster.name(), "no healthy endpoint — emitting 502");
        return H2AttemptResult {
            response: synth_h2_502(),
            endpoint: None,
            outcome: None,
            upstream_response: false,
        };
    };
```

with:

```rust
    let Some(endpoint) = cluster.pick_endpoint(request_hash_key, subset_match) else {
        // 57 (ADR-0114): corrected from the generic synth_h2_502() to the
        // dedicated no-healthy 503, matching Envoy and the H1 precedent
        // (synth_no_healthy_upstream).
        tracing::warn!(cluster = %cluster.name(), "no healthy endpoint — emitting 503");
        return H2AttemptResult {
            response: synth_h2_no_healthy_upstream(),
            endpoint: None,
            outcome: None,
            upstream_response: false,
        };
    };
```

- [ ] **Step 8: Run the Task-1 test to verify it PASSES**

Run: `cargo test -p envoy-http2 h2_no_healthy_upstream_returns_503`
Expected: PASS (status 503, body byte-exact `no healthy upstream`).

- [ ] **Step 9: Run the full crate test suite to verify NO regression**

Run: `cargo test -p envoy-http2`
Expected: PASS, including the phase-56 `h2_route_miss_access_log_carries_nr_flag`/`h2_host_miss_access_log_carries_nr_flag` backstops (untouched code paths) and all connect-error/send-error tests (still 502 via the untouched `synth_h2_502()` call sites at `:387`/`:398`).

- [ ] **Step 10: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 57 task 1: H2 no-healthy pick()->None synth 502->503 (GREEN) [ADR-0114]"
```

---

## Task 2: `response_code_details_for_log_h2` else-branch + two-arm `%RESPONSE_FLAGS%` derive (§B + §C)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (caller-loop `:688`-`694`; derive `:940`-`951`; new test)

**Interfaces:**
- Consumes: `cluster_mgr_no_fallback_subset()` from Task 1 (Step 2).
- Produces: nothing new consumed by later tasks — Task 3/4 (fixtures) exercise this via the Docker differential, not via Rust symbols.

- [ ] **Step 1: Write the failing access-log backstop test**

Insert into the test module (e.g. immediately after Task 1's `h2_no_healthy_upstream_returns_503`) — a structural clone of the phase-56 `h2_route_miss_access_log_carries_nr_flag` (`:2297`) crossed with the H1 `h1_no_healthy_access_log_carries_uh_flag` (`crates/envoy-http1/src/hcm.rs:5530`) topology:

```rust
    /// Phase 57 (ADR-0114) Task 2: the H2 no-healthy `pick()->None` arm's
    /// access-log line must carry `rcd:"no_healthy_upstream"` (set by the
    /// caller-loop's NEW `else` branch, §B) AND `rf:"UH"` (derived from that
    /// rcd by the extended two-arm match, §C). Structural clone of
    /// `h2_route_miss_access_log_carries_nr_flag`, using
    /// `cluster_mgr_no_fallback_subset()` (Task 1) + a `metadata_match`
    /// selecting the non-existent `stage: nonexistent` subset instead of a
    /// direct_response route. Fail-first: pre-change, the caller-loop only
    /// sets `response_code_details_for_log_h2` inside the `if let Some(...)`
    /// arm, so the pick-none path leaves it `None` → the derive's `_ => "-"`
    /// arm fires → `{"rc":503,"rcd":null,"rf":"-"}`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_no_healthy_access_log_carries_uh_flag() {
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

        let mut envoy_lb = std::collections::BTreeMap::new();
        envoy_lb.insert("stage".to_string(), "nonexistent".to_string());
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
                            cluster: "subset_cluster".to_string(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: Some(LbMetadata { envoy_lb }),
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
        let cluster_mgr = cluster_mgr_no_fallback_subset().await;
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
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
        assert_eq!(resp.status(), 503, "no-healthy synth-503 status unchanged");
        let mut body = resp.into_body();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"no_healthy_upstream\",\"rf\":\"UH\"}\n",
            "H2 no-healthy access-log line carries rcd:\"no_healthy_upstream\",rf:\"UH\": {logged:?}"
        );
    }
```

> **Worker note:** `tempfile` is already a dev-dependency of `envoy-http2` (see `Cargo.toml`); other tests in this module use `tempfile::tempdir()` — confirm the import path matches existing usage in the file (fully-qualified, no bare `use tempfile;` needed).

- [ ] **Step 2: Run it RED**

Run: `cargo test -p envoy-http2 h2_no_healthy_access_log_carries_uh_flag`
Expected: FAIL — logged line is `{"rc":503,"rcd":null,"rf":"-"}\n` (the caller-loop leaves `response_code_details_for_log_h2` as `None` on the pick-none path; the derive's `_ => "-"` arm fires).

- [ ] **Step 3: Commit the RED test**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 57 task 2: RED test for H2 no-healthy access-log rcd/rf [ADR-0114]"
```

- [ ] **Step 4: Add the caller-loop `else` branch (§B)**

Replace exactly (`hcm.rs:688`-`694`):

```rust
                        if let Some(endpoint) = attempt.endpoint {
                            // 06.2 Task 7: capture the resolved upstream endpoint for
                            // the access-log `%UPSTREAM_HOST%` token (last attempt's
                            // endpoint wins). Skipped on pick()->None.
                            upstream_host_for_log_h2 = Some(endpoint.to_string());
                            response_code_details_for_log_h2 = Some("via_upstream".to_owned());
                        }
```

with:

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

- [ ] **Step 5: Extend the `%RESPONSE_FLAGS%` derive to two arms (§C)**

Replace exactly (`hcm.rs:940`-`951`; the comment block + the one-arm match):

```rust
    if !config.inner.access_log.is_empty() {
        let duration = req_arrival_instant.elapsed();
        // Phase 56 (ADR-0113): the H2 sibling of the H1 phase-48 one-arm
        // %RESPONSE_FLAGS% derive (crates/envoy-http1/src/hcm.rs:1377,
        // ORIGINAL one-arm scope before phases 49-54 each added one more
        // arm). Deliberately mirrors ONLY that original scope, not H1's
        // current six-arm derive — the remaining H2 flags (UH/UO/URX/UF/UC)
        // are carry-forward M56-1, witnessed one-at-a-time by future phases.
        let response_flags_for_log_h2: &str = match response_code_details_for_log_h2.as_deref() {
            Some("route_not_found") => "NR",
            _ => "-",
        };
```

with:

```rust
    if !config.inner.access_log.is_empty() {
        let duration = req_arrival_instant.elapsed();
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

- [ ] **Step 6: Run the Task-2 test to verify it PASSES**

Run: `cargo test -p envoy-http2 h2_no_healthy_access_log_carries_uh_flag`
Expected: PASS (logged line `{"rc":503,"rcd":"no_healthy_upstream","rf":"UH"}\n`).

- [ ] **Step 7: Run the full crate test suite to verify NO regression**

Run: `cargo test -p envoy-http2`
Expected: PASS, including:
- Task 1's `h2_no_healthy_upstream_returns_503` (status/body unaffected by this task's rcd/flag changes);
- the phase-56 `h2_route_miss_access_log_carries_nr_flag`/`h2_host_miss_access_log_carries_nr_flag` backstops (the `route_not_found => "NR"` arm is unchanged, and those tests never hit `pick()->None`, so `response_code_details_for_log_h2` on their path is unaffected by the new `else` branch);
- all happy-path/proxy-success tests (the `if let Some(endpoint)` arm is unchanged — `via_upstream` still sets, flag stays `"-"`).

- [ ] **Step 8: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 57 task 2: H2 no-healthy rcd else-branch + rf:UH derive arm (GREEN) [ADR-0114]"
```

---

## Task 3: Fixture `0065-accesslog-h2-rf-no-healthy` (one probe: H2 no-healthy 503)

Build the fixture from the `0057` template (the `subset_cluster` + NO_FALLBACK `lb_subset_config` + a route `metadata_match` selecting the non-existent `stage: nonexistent` subset → `pick()->None` synth-503), substituting `0064`'s H2 listener shape (`codec_type: HTTP2` + `http2_protocol_options: {}`) for `0057`'s `codec_type: HTTP1`.

**Files:**
- Create: `tests/fixtures/0065-accesslog-h2-rf-no-healthy/envoy.yaml`
- Create: `tests/fixtures/0065-accesslog-h2-rf-no-healthy/envoy-rust.yaml`
- Create: `tests/fixtures/0065-accesslog-h2-rf-no-healthy/expectations.yaml`
- Create: `tests/fixtures/0065-accesslog-h2-rf-no-healthy/README.md`

- [ ] **Step 1: Create `envoy.yaml`** (reference side — `admin` block present, bind `0.0.0.0`, mount path `/tmp/0065-envoy-mount/access.log`)

```yaml
node: { id: envoy-rust-phase-57-fixture-0065, cluster: envoy-rust-phase-57 }
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
                      path: /tmp/0065-envoy-mount/access.log
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
                    - name: subset_vh
                      domains: ["*"]
                      # Single route → `subset_cluster` with a `metadata_match`
                      # selecting the NON-EXISTENT `stage: nonexistent` subset
                      # (the fixture-0038/0057 pattern). NO_FALLBACK → no
                      # eligible subset → 503 `no healthy upstream` at routing
                      # time, so the literal endpoint is never dialed.
                      routes:
                        - match: { prefix: "/" }
                          route:
                            cluster: subset_cluster
                            metadata_match: { filter_metadata: { envoy.lb: { stage: nonexistent } } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # `subset_cluster`: a PLAIN STATIC ROUND_ROBIN cluster carrying an
    # `lb_subset_config` (single selector keys:[stage], NO_FALLBACK). The ONE
    # endpoint carries `metadata.filter_metadata.envoy.lb: { stage: prod }` at
    # the LITERAL unreachable address `127.0.0.1:1` — it is NEVER dialed because
    # the route's `metadata_match` selects the non-existent `stage: nonexistent`
    # subset, so `pick()->None` fires the no-healthy-upstream synth-503 at
    # routing time. A literal address (not a {{BACKEND_IP}}/{{HTTP1_BACKEND_PORT}}
    # marker) keeps both configs byte-identical with NO backend spawned.
    - name: subset_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: NO_FALLBACK
        subset_selectors:
          - keys: [stage]
      load_assignment:
        cluster_name: subset_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
                metadata:
                  filter_metadata:
                    envoy.lb: { stage: prod }
```

- [ ] **Step 2: Create `envoy-rust.yaml`** (subject side — NO `admin` block, bind `127.0.0.1`, mount path `/tmp/0065-envoy-rust-mount/access.log`; otherwise byte-identical)

```yaml
node: { id: envoy-rust-phase-57-fixture-0065, cluster: envoy-rust-phase-57 }
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
                      path: /tmp/0065-envoy-rust-mount/access.log
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
                    - name: subset_vh
                      domains: ["*"]
                      # Single route → `subset_cluster` with a `metadata_match`
                      # selecting the NON-EXISTENT `stage: nonexistent` subset
                      # (the fixture-0038/0057 pattern). NO_FALLBACK → no
                      # eligible subset → 503 `no healthy upstream` at routing
                      # time, so the literal endpoint is never dialed.
                      routes:
                        - match: { prefix: "/" }
                          route:
                            cluster: subset_cluster
                            metadata_match: { filter_metadata: { envoy.lb: { stage: nonexistent } } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # `subset_cluster`: a PLAIN STATIC ROUND_ROBIN cluster carrying an
    # `lb_subset_config` (single selector keys:[stage], NO_FALLBACK). The ONE
    # endpoint carries `metadata.filter_metadata.envoy.lb: { stage: prod }` at
    # the LITERAL unreachable address `127.0.0.1:1` — it is NEVER dialed because
    # the route's `metadata_match` selects the non-existent `stage: nonexistent`
    # subset, so `pick()->None` fires the no-healthy-upstream synth-503 at
    # routing time. A literal address (not a {{BACKEND_IP}}/{{HTTP1_BACKEND_PORT}}
    # marker) keeps both configs byte-identical with NO backend spawned.
    - name: subset_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      lb_subset_config:
        fallback_policy: NO_FALLBACK
        subset_selectors:
          - keys: [stage]
      load_assignment:
        cluster_name: subset_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
                metadata:
                  filter_metadata:
                    envoy.lb: { stage: prod }
```

- [ ] **Step 3: Create `expectations.yaml`** (one probe — H2 no-healthy 503 — reuses `Driver::Http2AccessLogByteExact` verbatim)

```yaml
driver:
  kind: http2_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0065-envoy-mount/access.log
    envoy_rust: /tmp/0065-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / (any :authority — domains: ["*"]) routed to
    # `subset_cluster` via a `metadata_match` selecting the NON-EXISTENT
    # `stage: nonexistent` subset (the fixture-0038/0057 pattern). NO_FALLBACK
    # → no eligible subset → `pick()->None` → the no-healthy-upstream
    # synth-503 at ROUTING time — the literal `127.0.0.1:1` endpoint is never
    # dialed. This is the SECOND non-`-` H2 %RESPONSE_FLAGS% witness: UH
    # (NoHealthyUpstream) (phase 57, ADR-0114), the H2 analogue of fixture
    # 0057 (phase 49) and phase 56's H2 NR witness (fixture 0064).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static
    # literal: the `http2_access_log_byte_exact` driver asserts every line is
    # byte-identical between upstream Envoy v1.33.0 and envoy-rust. The
    # no-healthy synth-503 is deterministic on BOTH sides, so the rendered
    # line is identical. envoy-rust now (a) emits the dedicated
    # synth_h2_no_healthy_upstream() 503 (was the generic synth_h2_502()), (b)
    # sets rcd:"no_healthy_upstream" (was null), and (c) derives rf:"UH" from
    # that rcd (was "-").
    #
    # Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rcd, rf.
    # The emitted line is:
    #   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `README.md`**

```markdown
# Fixture 0065 — H2 access-log `%RESPONSE_FLAGS%` no-healthy-upstream failure path (`UH`, byte-exact)

The H2 analogue of fixture `0057` (phase 49, the H1 `UH` witness) and the
SECOND fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`). Phase 57 (ADR-0114) witnesses the SECOND H2
`%RESPONSE_FLAGS%` value, `UH` (NoHealthyUpstream), byte-exact on the H2
`pick()->None` no-healthy-upstream 503 path — AND corrects a genuine
differential-correctness bug found in the same motion (envoy-rust's H2
no-healthy arm previously returned a generic 502; Envoy returns 503).

## What this proves

Before this phase, envoy-rust's H2 `pick()->None` arm (`run_h2_attempt`,
`crates/envoy-http2/src/hcm.rs`) emitted the generic `synth_h2_502()` (status
502, empty body, `%RESPONSE_CODE_DETAILS%` = `null`, `%RESPONSE_FLAGS%` = `-`)
— a three-way divergence from live Envoy v1.33.0, which returns 503 + body
`no healthy upstream` + `rcd:"no_healthy_upstream"` + `rf:"UH"`. Phase 57 (i)
adds a dedicated `synth_h2_no_healthy_upstream()` helper (mirroring the H1
`synth_no_healthy_upstream` precedent) at the ONE `pick()->None` call site,
(ii) sets `response_code_details_for_log_h2 = Some("no_healthy_upstream")` in
the caller-loop's NEW `else` branch, and (iii) extends the phase-56 H2
one-arm `%RESPONSE_FLAGS%` derive to a two-arm match (`route_not_found` =>
`NR`, `no_healthy_upstream` => `UH`). All three trace to the SAME two code
sites the state-0 recon identified — no fourth divergence.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | `pick()->None` no-healthy | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
```

The route/cluster table is the IDENTICAL shape fixture `0057` uses (a
`subset_cluster` STATIC cluster with `lb_subset_config` NO_FALLBACK, a single
route `metadata_match` selecting the non-existent `stage: nonexistent`
subset) — only `codec_type: HTTP2` + `http2_protocol_options: {}` (fixture
`0064`'s listener shape) are substituted for `0057`'s `codec_type: HTTP1`.

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness change this phase. Drives the probe over
H2-prior-knowledge via `drive_http2`, scrapes both files, asserts the scraped
line count equals `probes.len()` (here 1), and calls
`access_log::assert_access_log_lines_byte_identical`.

## `0001`-`0064` byte-preservation

This phase's changes are additive — gated on `cluster.pick_endpoint(...)`
returning `None`, which requires a `lb_subset_config`/NO_FALLBACK subset-miss
(or an empty CLA) on an H2 listener. NONE of the pre-existing H2 fixtures
(`0009`, `0010`, `0021`, `0064`) configures `lb_subset_config` — re-confirmed
by `grep -c lb_subset_config` over each `envoy-rust.yaml` returning `0`. So
`0001`-`0064` stay byte-identical; only the new `0065` observes the changed
status/rcd/rf.

## Cross-references

- ADR: ADR-0114 (state-1 brainstorm + state-2 PLAN — the H2 `UH` witness +
  the 502->503 reconciliation).
- Related fixtures: `0057` (the H1 `UH` witness this fixture mirrors on H2);
  `0064` (the H2 `NR` witness that opened `Driver::Http2AccessLogByteExact`).
- Reconciles: the pre-existing `BEHAVIOR_CONTRACT.md` note "the H2 no-healthy
  arm returns 502" (flagged in passing during phase 56's SPEC drafting) —
  investigated and FIXED this phase.
- Carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`UO`/`URX`/`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%`
  strings beyond `route_not_found`/`no_healthy_upstream`, still open for
  future one-flag-at-a-time phases.
```

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0065-accesslog-h2-rf-no-healthy/
git commit -m "phase 57 task 3: fixture 0065-accesslog-h2-rf-no-healthy (one probe, H2 rf:UH byte-exact) [ADR-0114]"
```

---

## Task 4: Differential test `access_log_h2_rf_no_healthy.rs` (§E)

**Files:**
- Create: `tests/differential/tests/access_log_h2_rf_no_healthy.rs` (a structural clone of `access_log_h2_rf_no_route.rs`, pointing at the `0065` fixture)

- [ ] **Step 1: Write the test wrapper**

```rust
//! Docker-gated differential test for fixture 0065-accesslog-h2-rf-no-healthy.
//! Phase 57 (ADR-0114) — the SECOND H2 `%RESPONSE_FLAGS%` witness: `UH`
//! (NoHealthyUpstream), byte-exact cross-proxy on the H2 `pick()->None`
//! no-healthy-upstream 503 path — the H2 analogue of fixture `0057` (phase
//! 49). Also corrects envoy-rust's H2 no-healthy synth status 502 -> 503 to
//! match Envoy (the dedicated `synth_h2_no_healthy_upstream()` helper,
//! mirroring the H1 `synth_no_healthy_upstream` precedent). A NO_FALLBACK
//! `lb_subset_config` cluster (`subset_selectors: [{ keys: [stage] }]`) with
//! a single route whose `metadata_match` selects the NON-EXISTENT `stage:
//! nonexistent` subset (the fixture-0038/0057 pattern) -> `pick()->None` ->
//! the deterministic 503 `no healthy upstream` synth at ROUTING time (the
//! literal `127.0.0.1:1` endpoint is never dialed; no backend spawns).
//! Spawns Envoy v1.33 in a container; spawns envoy-rust as a subprocess;
//! drives `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted line
//! is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rf_no_healthy() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0065-accesslog-h2-rf-no-healthy");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Compile-check the differential crate**

Run: `cargo test -p differential --no-run`
Expected: compiles clean (no new harness code; the `0065` fixture deserializes against the existing `Driver::Http2AccessLogByteExact`).

> **NOTE (host-environment):** the Docker differential (real Envoy vs envoy-rust) is **CI-authoritative** at the state-4 §7.5 gate (see memory `envoy-rust-state4-ci-first-execution`). This host may not run the container reliably; do NOT treat a local Docker-gated non-run as a failure. The differential `0065` green + all `0001`-`0064` still green are confirmed by the state-4 CI run.

- [ ] **Step 3: Commit**

```bash
git add tests/differential/tests/access_log_h2_rf_no_healthy.rs
git commit -m "phase 57 task 4: differential test access_log_h2_rf_no_healthy (fixture 0065) [ADR-0114]"
```

---

## Task 5: BEHAVIOR_CONTRACT updates (§G)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_FLAGS%` row, `:1020`; the `%RESPONSE_CODE_DETAILS%` row, `:1031`)

- [ ] **Step 1: Update the `%RESPONSE_FLAGS%` row's H2-witness sentence**

Replace exactly (the trailing H2 sentence of the row — a substring of the giant single-line row at `:1020`, unique in the file):

```
The H2 access-log differential driver now exists (`Driver::Http2AccessLogByteExact`, phase 56, ADR-0113) and `NR` is witnessed byte-exact on H2 by fixture **0064** — CONSUMING carry-forward **M45-1**. The remaining H2 `%RESPONSE_FLAGS%` values (`UH`/`UO`/`URX`/`UF`/`UC`) remain deferred as NEW carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

with:

```
The H2 access-log differential driver now exists (`Driver::Http2AccessLogByteExact`, phase 56, ADR-0113) and `NR` is witnessed byte-exact on H2 by fixture **0064** — CONSUMING carry-forward **M45-1**. `UH` is now ALSO witnessed byte-exact on H2 by fixture **0065** (phase 57, ADR-0114), which ALSO corrects the H2 no-healthy synth status 502 → 503 to match Envoy (the H2 `synth_h2_no_healthy_upstream()` helper, mirroring the H1 `synth_no_healthy_upstream` precedent) — ADVANCING carry-forward **M56-1** (the `UH` slice consumed). The remaining H2 `%RESPONSE_FLAGS%` values (`UO`/`URX`/`UF`/`UC`) remain deferred as the continuing carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

- [ ] **Step 2: Update the `%RESPONSE_CODE_DETAILS%` row — reconcile the un-recon'd note**

Replace exactly (a substring of the giant single-line row at `:1031`, unique in the file):

```
The remaining H2 failure-path details (beyond `route_not_found`) remain deferred as part of carry-forward **M56-1** (which also carries forward, un-investigated, the note that the H2 no-healthy arm returns 502 — flagged in passing during phase 56's SPEC drafting, not yet reconciled).
```

with:

```
`no_healthy_upstream` is now ALSO witnessed on H2 (fixture **0065**, phase 57, ADR-0114) — phase 57 investigated and FIXED the previously un-recon'd note that "the H2 no-healthy arm returns 502": the H2 `pick()->None` arm now emits envoy-rust's dedicated `synth_h2_no_healthy_upstream()` helper (503, mirroring the H1 `synth_no_healthy_upstream` precedent) instead of the generic `synth_h2_502()`, and `response_code_details_for_log_h2` is now set to `Some("no_healthy_upstream")` on that arm (the caller-loop `else` branch). The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`) remain deferred as the continuing carry-forward **M56-1**.
```

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 57 task 5: BEHAVIOR_CONTRACT rf/rcd rows — H2 UH witnessed + 502->503 note reconciled (fixture 0065) [ADR-0114]"
```

---

## Task 6: Local verification sweep (state-3 close-out; full §7.5 gate runs at state-4)

This is the developer's local pre-flight — NOT the state-4 verification gate (that re-runs the full §7.5 set in CI and quotes outputs to `PROGRESS.md`). Run the cheap-and-local subset; the Docker differential + `0001`-`0064` byte-identical + h2spec are CI-authoritative at state-4.

**Files:** none (verification only)

- [ ] **Step 1: clippy clean**

Run: `cargo clippy -p envoy-http2 -p differential --all-targets --all-features -- -D warnings`
Expected: no warnings (the two-arm `match` and the new helper are idiomatic; no new lint surface).

- [ ] **Step 2: fmt clean**

Run: `cargo fmt --all -- --check`
Expected: clean. (If any inserted block reflows, run `cargo fmt --all` and re-commit — see memory `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase.)

- [ ] **Step 3: full workspace unit tests** (non-Docker)

Run: `cargo test --workspace`
Expected: PASS (the two new backstops + all existing tests; the differential Docker tests are Docker-gated and skip locally per the harness's own gating — do not treat a local skip as a failure).

- [ ] **Step 4: confirm byte-preservation reasoning (no existing H2 fixture regressed)**

Run: `for f in 0009 0010 0021 0064; do grep -c lb_subset_config tests/fixtures/${f}-*/envoy-rust.yaml; done`
Expected: `0` for all four — re-confirms none can newly reach `pick()->None`, so `0001`-`0064` stay byte-identical; only `0065` observes the changed status/rcd/rf.

- [ ] **Step 5: final fmt-fix commit if needed** (otherwise nothing to commit)

```bash
cargo fmt --all
git add -A && git commit -m "phase 57: cargo fmt [ADR-0114]" || echo "nothing to reformat"
```

---

## Scope / gate summary

- **Task count:** 6 tasks (~250-350 LoC: a ~15-line synth helper + a ~5-line call-site swap + a ~10-line caller-loop `if`/`else` + a ~6-line derive extension + two ~90-130-line in-process backstop tests + a 4-file fixture (~120 LoC incl. README) + a ~25-line differential test + two BEHAVIOR_CONTRACT row edits). **§6.1 split does NOT fire** (well under ~25 tasks / ~1500 LoC — re-confirmed §3 item 5 above). **ADR-0115 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the standing lapsed-reservation convention).
- **No new** `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant. `#![forbid(unsafe_code)]` holds.
- **Additive invariant:** all `0001`-`0064` fixtures stay byte-identical (§3 item 2 above; re-verified Task 6 Step 4). Only the H2 `pick()->None` path's previously 502/`null`/`"-"` triple changes — and no existing H2 fixture configures `lb_subset_config`.
- **Acceptance (re-run at state-4, SPEC §5):** (a) fixture `0065` green (cross-proxy-equal status `503` + whole-line `{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}`) + (b) all `0001`-`0064` green simultaneously + (c) h2spec ≥95% (no H2 codec/framing change) + (d) no new fuzz target (SPEC §H) + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved.
- **Carry-forwards:** this phase ADVANCES **M56-1** (consumes the `UH` slice; `UO`/`URX`/`UF`/`UC` + the remaining H2 failure-path rcd strings stay open) and RECONCILES the un-recon'd `BEHAVIOR_CONTRACT.md:1031` "H2 no-healthy arm returns 502" note (fixed, not just documented — Task 1/2). M55-1 + M53-2 + M53-3 + M48-2 + M42-1 + the `DC`/retry-budget-overflow slices of M45-2 + M40-1 + M39-1/M39-2 + M38-1/M38-2 + CF-39-1 + the HTTP-filters-family (1)-(4) + older stay live; NONE blocks.

_The state-3 implementation (`superpowers:executing-plans` or `superpowers:subagent-driven-development`) is the session AFTER this PLAN lands. Per §5.1, one state per session: this session writes the PLAN only._
