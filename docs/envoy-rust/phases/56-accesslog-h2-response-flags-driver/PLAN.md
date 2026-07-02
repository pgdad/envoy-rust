# Phase 56 — `56-accesslog-h2-response-flags-driver` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open the H2 access-log differential driver (`Driver::Http2AccessLogByteExact`) and witness the first H2 `%RESPONSE_FLAGS%` value, `NR` (NoRoute), byte-exact cross-proxy via a new fixture `0064`.

**Architecture:** Three independent additive slices, landed as three tasks: (1) a new `Driver` variant in the differential-test crate that drives H2-prior-knowledge probes and re-uses the existing whole-line byte-exact access-log comparison helper; (2) a one-arm `response_flags` derive in the H2 HCM's record-build site (`crates/envoy-http2/src/hcm.rs`), replacing today's hard-coded `"-"` literal, verified by two new in-process backstop tests; (3) a new fixture `0064` (an H2C clone of fixture `0056`'s route table) + its differential test, proving (1) and (2) byte-exact against live Envoy. A fourth task updates `BEHAVIOR_CONTRACT.md`.

**Tech Stack:** Rust, `h2` crate (H2 codec, already a permitted foundation — see `crates/envoy-http2/src/hcm.rs`'s existing `h2::client`/`h2::server` usage), `tokio`, the existing `tests/differential` test-orchestration crate (`testcontainers` + subprocess).

## Global Constraints

- `#![forbid(unsafe_code)]` holds in every crate touched (no `unsafe` is introduced by this plan).
- NO new crate, dependency, `Op`, `AccessLogRecord` field, or `ConfigError` variant (per SPEC §2 load-bearing invariant).
- ALL existing fixtures `0001`-`0063` must stay green — every code change in this plan is additive (gated on the `route_not_found` RCD value, or on a brand-new `Driver` variant no existing fixture references).
- Every step's shell commands assume the working directory is the repo root (`/home/esa/git/envoy-rust` on this host, but use a relative `cd` if resuming elsewhere — always verify with `pwd` first).
- Per `BOOTSTRAP_PROMPT.md` §5 state 3, append a short entry to `docs/envoy-rust/phases/56-accesslog-h2-response-flags-driver/PROGRESS.md` after each task completes (create the file at Task 1 if it does not yet exist). This plan does not repeat that instruction per-task; treat it as a standing step after every task's commit.
- Commit after each task with a message of the form `phase 56 task N: <short description>` (small, working-tree-clean commits; per this project's `git status --porcelain` concurrency guard, re-check for a clean tree immediately before every commit — a sibling autonomous session may be advancing the SAME repo concurrently).
- Do NOT edit `docs/envoy-rust/ROADMAP.md`, `STATE.md`, or `DECISIONS.md` in this plan's tasks — those are state-4/5/6 concerns, out of scope for state-3 implementation.

---

## File Structure

- **Modify** `tests/differential/src/lib.rs` — add the `Driver::Http2AccessLogByteExact` variant, its `"PORT"`-marker match entry, and its dispatch arm (Task 1).
- **Modify** `crates/envoy-http2/src/hcm.rs` — replace the hard-coded `response_flags: "-".to_owned()` literal (currently line 948) with a one-arm derive; add two new `#[tokio::test]` backstop tests to the existing `#[cfg(test)] mod tests` block (Task 2).
- **Create** `tests/fixtures/0064-accesslog-h2-rf-no-route/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}` — the H2C analogue of fixture `0056` (Task 3).
- **Create** `tests/differential/tests/access_log_h2_rf_no_route.rs` — the thin differential-test wrapper for fixture `0064` (Task 3).
- **Modify** `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the `%RESPONSE_FLAGS%` row and the `%RESPONSE_CODE_DETAILS%` row (Task 4).

No file in this plan exceeds a few hundred added lines; no split into `56.1`/`56.2` is needed (see "§6.1 split decision" below).

## §6.1 split decision

SPEC §6 left the split undecided pending an actual task count. This plan has **4 tasks** (harness variant / H2 derive+backstop / fixture+differential-test / BEHAVIOR_CONTRACT), well under the ~25-task gate, and the total estimated diff is ~350-450 LoC (a new enum variant + match arm ~60 LoC; one derive + two backstop tests ~110 LoC; four fixture files + one differential test ~140 LoC; two BEHAVIOR_CONTRACT paragraph edits ~30 LoC). **The split does NOT fire.** No `ADR-0114` split-ADR is needed. `ADR-0114` remains reserved for a §6.2 reconciliation only if a task below finds a SPEC §A-§G fact was wrong (none is expected — every fact was re-verified against the current tree immediately before this PLAN was written).

---

## Task 1: `Driver::Http2AccessLogByteExact` harness variant

**Files:**
- Modify: `tests/differential/src/lib.rs`

**Interfaces:**
- Consumes: `AccessLogByteExactProbe` (`method: Http1Method, path: String, host: String, extra_headers: Vec<(String,String)>, body: Option<String>, expected_status: u16`, already defined at `tests/differential/src/lib.rs:1015`), `AccessLogPaths` (`envoy: String, envoy_rust: String`, already defined at `:1001`), `drive_http2(addr: SocketAddr, method: &Http1Method, path: &str, host: &str, extra_headers: &[(String,String)]) -> Result<DriveHttp1Result>` (already defined at `:2141` — note NO `body` parameter; only supports `Http1Method::Get`/`Http1Method::Options`), `wait_file_lines` and `ACCESS_LOG_FLUSH_WAIT` (already used by the H1 sibling arm), `crate::access_log::assert_access_log_lines_byte_identical(envoy: &[String], envoy_rust: &[String]) -> Result<(), String>` (already defined at `tests/differential/src/access_log.rs:305`).
- Produces: a new `Driver::Http2AccessLogByteExact { probes: Vec<AccessLogByteExactProbe>, expected_access_log_paths: AccessLogPaths }` variant, deserializable from fixture YAML via `kind: http2_access_log_byte_exact` (automatic from the enum's `#[serde(tag = "kind", rename_all = "snake_case")]` attribute — no manual mapping needed).

This task is infrastructure-only: no fixture references the new variant yet (that lands in Task 3), so its test is compilation, not a runtime assertion.

- [ ] **Step 1: Add the `Driver::Http2AccessLogByteExact` variant**

Open `tests/differential/src/lib.rs`. Find the existing `Driver::Http1AccessLogByteExact` variant (around line 121):

```rust
    Http1AccessLogByteExact {
        // No Box needed: the `probes` `Vec` is already heap-indirected, so
        // this variant stays under clippy's `large_enum_variant` threshold
        // (unlike `Http1WithAccessLog`, which boxes its inline body rule).
        probes: Vec<AccessLogByteExactProbe>,
        expected_access_log_paths: AccessLogPaths,
    },
```

Immediately after it (before the next variant, `Http2 { ... }`), insert:

```rust
    /// Phase 56 (ADR-0113): the H2 sibling of `Http1AccessLogByteExact`.
    /// Drives a SEQUENCE of H2-prior-knowledge probes via `drive_http2`
    /// against an H2C listener whose file access-logger carries a CUSTOM
    /// `log_format`. After all probes complete, scrapes BOTH proxies'
    /// access-log files and asserts every emitted line is byte-identical via
    /// `access_log::assert_access_log_lines_byte_identical` — identical
    /// assertion machinery to the H1 sibling, only the wire driver differs.
    /// `drive_http2` currently supports GET/OPTIONS with no request body
    /// (see its `debug_assert!`); every probe's `body` field is therefore
    /// ignored on this arm (H2 fixtures needing a body must extend
    /// `drive_http2` first — none do as of this phase).
    Http2AccessLogByteExact {
        probes: Vec<AccessLogByteExactProbe>,
        expected_access_log_paths: AccessLogPaths,
    },
```

- [ ] **Step 2: Add it to the `"PORT"`-marker `matches!` list**

Find the `port_key` match (around line 2792), specifically the line:

```rust
        | Driver::Http2 { .. }
        | Driver::Http2ProbeList { .. }
```

Change it to:

```rust
        | Driver::Http2 { .. }
        | Driver::Http2ProbeList { .. }
        // Phase 56 (ADR-0113): the H2 access-log byte-exact driver runs over
        // the same {{PORT}} H2C listener convention as its H1 sibling and
        // the other HCM-shaped drivers.
        | Driver::Http2AccessLogByteExact { .. }
```

- [ ] **Step 3: Add the dispatch arm**

Find the end of the `Driver::Http1AccessLogByteExact { .. } => { ... }` arm (it ends with the `assert_access_log_lines_byte_identical` call and its `.map_err` — search for `"access log byte-exact mismatch:"` to locate the closing `}` of that match arm, around line 5265). Immediately after that arm's closing `}`, insert a new arm. It is a near-verbatim structural clone of the H1 arm, with `drive_http1(..., body)` replaced by `drive_http2(...)` (no body arg) and the log message prefixes changed from `http1` to `http2`:

```rust
        // Phase 56 (ADR-0113): the H2 sibling of the byte-exact access-log
        // driver above. Drives a SEQUENCE of H2 probes (via `drive_http2`),
        // then scrapes BOTH proxies' access-log files and asserts every
        // emitted line is byte-identical.
        Driver::Http2AccessLogByteExact {
            probes,
            expected_access_log_paths,
        } => {
            let expected_lines = probes.len();

            for (idx, probe) in probes.iter().enumerate() {
                let upstream_resp = drive_http2(
                    upstream_addr,
                    &probe.method,
                    &probe.path,
                    &probe.host,
                    &probe.extra_headers,
                )
                .await
                .with_context(|| {
                    format!("upstream envoy http2 drive (Http2AccessLogByteExact probe {idx})")
                })?;
                let subject_resp = drive_http2(
                    subject_addr,
                    &probe.method,
                    &probe.path,
                    &probe.host,
                    &probe.extra_headers,
                )
                .await
                .with_context(|| {
                    format!("envoy-rust http2 drive (Http2AccessLogByteExact probe {idx})")
                })?;
                if upstream_resp.status != probe.expected_status {
                    bail!(
                        "probe {idx}: upstream status {} != expected {}",
                        upstream_resp.status,
                        probe.expected_status,
                    );
                }
                if subject_resp.status != probe.expected_status {
                    bail!(
                        "probe {idx}: subject status {} != expected {}",
                        subject_resp.status,
                        probe.expected_status,
                    );
                }
            }

            let envoy_rust_path = std::path::PathBuf::from(&expected_access_log_paths.envoy_rust);
            if !wait_file_lines(&envoy_rust_path, expected_lines, ACCESS_LOG_FLUSH_WAIT).await {
                tracing::warn!(
                    "differential: envoy-rust access-log file {} still has < {} lines after {:?} (pre-shutdown wait)",
                    envoy_rust_path.display(),
                    expected_lines,
                    ACCESS_LOG_FLUSH_WAIT,
                );
            }

            let envoy_path = std::path::PathBuf::from(&expected_access_log_paths.envoy);
            if !wait_file_lines(&envoy_path, expected_lines, ACCESS_LOG_FLUSH_WAIT).await {
                tracing::warn!(
                    "differential: envoy access-log file {} still has < {} lines after {:?} (pre-stop wait)",
                    envoy_path.display(),
                    expected_lines,
                    ACCESS_LOG_FLUSH_WAIT,
                );
            }

            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let envoy_contents = std::fs::read_to_string(&envoy_path).with_context(|| {
                format!("read envoy access-log file at {}", envoy_path.display())
            })?;
            let envoy_rust_contents =
                std::fs::read_to_string(&envoy_rust_path).with_context(|| {
                    format!(
                        "read envoy-rust access-log file at {}",
                        envoy_rust_path.display()
                    )
                })?;
            let envoy_lines: Vec<String> = envoy_contents.lines().map(|s| s.to_owned()).collect();
            let envoy_rust_lines: Vec<String> =
                envoy_rust_contents.lines().map(|s| s.to_owned()).collect();

            if envoy_lines.len() != expected_lines {
                bail!(
                    "envoy emitted {} access-log lines but {} probes were driven; lines: {:?}",
                    envoy_lines.len(),
                    expected_lines,
                    envoy_lines,
                );
            }
            if envoy_rust_lines.len() != expected_lines {
                bail!(
                    "envoy-rust emitted {} access-log lines but {} probes were driven; lines: {:?}",
                    envoy_rust_lines.len(),
                    expected_lines,
                    envoy_rust_lines,
                );
            }

            crate::access_log::assert_access_log_lines_byte_identical(
                &envoy_lines,
                &envoy_rust_lines,
            )
            .map_err(|e| {
                anyhow::anyhow!(
                    "access log byte-exact mismatch: {}\nenvoy lines: {:?}\nenvoy-rust lines: {:?}",
                    e,
                    envoy_lines,
                    envoy_rust_lines,
                )
            })?;
        }
```

**Note for the implementer:** `upstream_addr`, `subject_addr`, `subject`, `upstream`, `Duration`, `bail!`, `wait_file_lines`, and `ACCESS_LOG_FLUSH_WAIT` are all already in scope in this function (`run_fixture`) — this arm sits in the same `match &expectations.driver { ... }` block as the H1 sibling, so no new `use` imports are needed.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p differential --tests`
Expected: clean compile, no warnings about an unreachable/unmatched `Driver` variant (the match is exhaustive because every arm was added).

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -p differential --tests --all-features -- -D warnings`
Expected: clean (no `large_enum_variant` warning — this variant's shape mirrors `Http1AccessLogByteExact`, which already passes this lint).

- [ ] **Step 6: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 56 task 1: add Driver::Http2AccessLogByteExact harness variant"
```

---

## Task 2: H2 `response_flags` one-arm `NR` derive + in-process backstops

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs`

**Interfaces:**
- Consumes: `response_code_details_for_log_h2: Option<String>` (already computed earlier in the same function, already correctly `Some("route_not_found")` on both no-route arms — confirmed by this session's state-0/state-2 recon; no change needed to how it's computed).
- Produces: the `AccessLogRecord.response_flags` field on the H2 record-build site now renders `"NR"` when `response_code_details_for_log_h2.as_deref() == Some("route_not_found")`, else `"-"` (unchanged from today for every other path).

TDD order: write the two failing backstop tests FIRST (Step 1), confirm they fail on the still-hard-coded `"-"` (Step 2), then implement the derive (Step 3) and confirm both pass (Step 4).

- [ ] **Step 1: Write the two failing backstop tests**

Open `crates/envoy-http2/src/hcm.rs`. Find the end of the `#[cfg(test)] mod tests` block's existing tests — locate `hcm_h2_with_file_access_log_writes_one_line_per_request` (search for that name; it currently ends the file's H2-access-log test group, right after the `serve_one_h2_request_with_access_log` helper). Insert the two new tests immediately after it:

```rust
    /// Phase 56 (ADR-0113): the H2 sibling of the H1 phase-48 backstop
    /// `h1_route_miss_access_log_carries_nr_flag`
    /// (`crates/envoy-http1/src/hcm.rs:5933`). Builds an H2 HCM with a
    /// SINGLE non-wildcard vhost `domains: ["match.test"]` (one `/specific`
    /// direct_response route), drives `GET /nomatch` with `:authority:
    /// match.test` (route-miss — matches the vhost, no route matches), and
    /// asserts the access-log line carries `rf: "NR"`. Uses the existing
    /// `h2_hcm_config_with_access_log` helper, extended with a
    /// non-wildcard-domain route table instead of its default `domains:
    /// ["*"]` — see the inline `HttpConnectionManagerConfig` literal below
    /// (this test builds its OWN config rather than reusing the helper's
    /// default route table, since the helper's route table is wildcard-only).
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_route_miss_access_log_carries_nr_flag() {
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

        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http_h2".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "backend_vh".to_string(),
                    domains: vec!["match.test".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: "myroute".to_string(),
                        r#match: RouteMatch {
                            prefix: Some("/specific".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
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
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
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
            .uri("http://match.test/nomatch")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        assert_eq!(resp.status(), 404, "route-miss synth-404 status unchanged");
        let mut body = resp.into_body();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\",\"rf\":\"NR\"}\n",
            "H2 route-miss access-log line carries rf:\"NR\": {logged:?}"
        );
    }

    /// Phase 56 (ADR-0113): the H2 sibling of the H1 phase-48 backstop
    /// `h1_host_miss_access_log_carries_nr_flag`
    /// (`crates/envoy-http1/src/hcm.rs:6016`). Same HCM config as
    /// `h2_route_miss_access_log_carries_nr_flag`; drives `GET /specific`
    /// with `:authority: nomatch.test` (host-miss — no vhost `domains` entry
    /// matches, the route walk never runs) and asserts the SAME `rf: "NR"`
    /// line.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_host_miss_access_log_carries_nr_flag() {
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

        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http_h2".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            access_log: vec![],
            route_config: Some(RouteConfiguration {
                name: "local_route".to_string(),
                validate_clusters: None,
                virtual_hosts: vec![VirtualHost {
                    name: "backend_vh".to_string(),
                    domains: vec!["match.test".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: "myroute".to_string(),
                        r#match: RouteMatch {
                            prefix: Some("/specific".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
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
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
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
            .uri("http://nomatch.test/specific")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        assert_eq!(resp.status(), 404, "host-miss synth-404 status unchanged");
        let mut body = resp.into_body();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            let _ = body.flow_control().release_capacity(chunk.len());
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\",\"rf\":\"NR\"}\n",
            "H2 host-miss access-log line carries rf:\"NR\": {logged:?}"
        );
    }
```

**Note for the implementer:** every type name used above (`HttpConnectionManagerConfig`, `CodecType`, `RouteConfiguration`, `VirtualHost`, `Route`, `RouteMatch`, `RouteAction`, `DirectResponse`, `DataSource`, `HttpFilter`, `HttpFilterTypedConfig`, `RouterConfig`, `Http1HCMConfig`) is ALREADY present in the existing `use envoy_config::{ ... };` block + the `use envoy_http1::HCMConfig as Http1HCMConfig;` line at the top of `mod tests` (confirmed by re-reading `crates/envoy-http2/src/hcm.rs:1074-1080` at PLAN-write time — the same imports the neighboring `h2_hcm_config_with_access_log` helper already uses) — no new `use` line is needed. `tempfile::tempdir()` (fully-qualified, matching this file's own convention — NOT the bare `tempdir()` the H1 file uses) and `envoy_accesslog::{JsonValueInput, CompiledJsonFormat, FileSink}` (also fully-qualified, matching this file's existing `h2_hcm_config_with_access_log`/`AccessLogRecord` usage) likewise need no new `use` line.

- [ ] **Step 2: Run the new tests to verify they fail**

Run: `cargo test -p envoy-http2 --lib h2_route_miss_access_log_carries_nr_flag h2_host_miss_access_log_carries_nr_flag -- --nocapture`
Expected: both FAIL, each with an assertion mismatch showing the logged line as `{"rc":404,"rcd":"route_not_found","rf":"-"}` (the still-hard-coded literal) instead of `..."rf":"NR"}`.

- [ ] **Step 3: Implement the one-arm `response_flags` derive**

Find the `AccessLogRecord { ... }` construction inside the H2 record-build function (search for `response_flags: "-".to_owned(),` — currently at `crates/envoy-http2/src/hcm.rs:948`). The surrounding context looks like:

```rust
    if !config.inner.access_log.is_empty() {
        let duration = req_arrival_instant.elapsed();
        let record = envoy_accesslog::AccessLogRecord {
            start_time: req_arrival_systime,
            method: envoy_req.method.clone(),
            path: x_envoy_original_path_or_path(envoy_req).to_owned(),
            protocol: "HTTP/2".to_owned(),
            response_code: response_status_for_log,
            response_flags: "-".to_owned(),
            bytes_received: request_body_len,
            bytes_sent: response_body_len,
            duration,
            upstream_service_time: extract_upstream_service_time(response_headers_for_log),
            forwarded_for: access_log_header_value(&envoy_req.headers, "x-forwarded-for"),
            user_agent: access_log_header_value(&envoy_req.headers, "user-agent"),
            request_id: access_log_header_value(&envoy_req.headers, "x-request-id"),
            authority: access_log_header_value(&envoy_req.headers, "host"),
            upstream_host: upstream_host_for_log_h2,
            upstream_cluster: upstream_cluster_for_log_h2,
            route_name: route_name_for_log_h2,
            response_code_details: response_code_details_for_log_h2,
            dynamic_metadata,
        };
```

Replace it with (note the field-construction order changes slightly because `response_code_details_for_log_h2` is moved by-value into the record, so the derive must borrow it BEFORE that move — insert the derive as a `let` binding immediately before the `AccessLogRecord { ... }` literal, then reference the borrow in the `response_flags:` field):

```rust
    if !config.inner.access_log.is_empty() {
        let duration = req_arrival_instant.elapsed();
        // Phase 56 (ADR-0113): the H2 sibling of the H1 phase-48 one-arm
        // %RESPONSE_FLAGS% derive (crates/envoy-http1/src/hcm.rs:1377,
        // ORIGINAL one-arm scope before phases 49-54 each added one more
        // arm). Deliberately mirrors ONLY that original scope, not H1's
        // current six-arm derive — the remaining H2 flags (UH/UO/URX/UF/UC)
        // are carry-forward M56-1, witnessed one-at-a-time by future phases.
        let response_flags_for_log_h2: &str =
            match response_code_details_for_log_h2.as_deref() {
                Some("route_not_found") => "NR",
                _ => "-",
            };
        let record = envoy_accesslog::AccessLogRecord {
            start_time: req_arrival_systime,
            method: envoy_req.method.clone(),
            path: x_envoy_original_path_or_path(envoy_req).to_owned(),
            protocol: "HTTP/2".to_owned(),
            response_code: response_status_for_log,
            response_flags: response_flags_for_log_h2.to_owned(),
            bytes_received: request_body_len,
            bytes_sent: response_body_len,
            duration,
            upstream_service_time: extract_upstream_service_time(response_headers_for_log),
            forwarded_for: access_log_header_value(&envoy_req.headers, "x-forwarded-for"),
            user_agent: access_log_header_value(&envoy_req.headers, "user-agent"),
            request_id: access_log_header_value(&envoy_req.headers, "x-request-id"),
            authority: access_log_header_value(&envoy_req.headers, "host"),
            upstream_host: upstream_host_for_log_h2,
            upstream_cluster: upstream_cluster_for_log_h2,
            route_name: route_name_for_log_h2,
            response_code_details: response_code_details_for_log_h2,
            dynamic_metadata,
        };
```

`response_code_details_for_log_h2.as_deref()` borrows the `Option<String>` (`as_deref()` gives `Option<&str>`) without consuming it, so the later `response_code_details: response_code_details_for_log_h2` move on the next few lines remains valid — the borrow's lifetime ends at the `match` statement, before the move.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p envoy-http2 --lib h2_route_miss_access_log_carries_nr_flag h2_host_miss_access_log_carries_nr_flag -- --nocapture`
Expected: both PASS.

- [ ] **Step 5: Run the full envoy-http2 test suite to confirm no regression**

Run: `cargo test -p envoy-http2 --lib`
Expected: all pre-existing tests still PASS (the derive change is additive — every existing H2 test either doesn't log `%RESPONSE_FLAGS%` at all, or hits a path where `response_code_details_for_log_h2` is never `Some("route_not_found")`, so the new arm never fires for them).

- [ ] **Step 6: Run clippy and fmt**

Run: `cargo clippy -p envoy-http2 --all-targets --all-features -- -D warnings`
Run: `cargo fmt -p envoy-http2 -- --check`
Expected: both clean.

- [ ] **Step 7: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 56 task 2: H2 response_flags NR derive + in-process backstops"
```

---

## Task 3: Fixture `0064` + differential test

**Files:**
- Create: `tests/fixtures/0064-accesslog-h2-rf-no-route/envoy.yaml`
- Create: `tests/fixtures/0064-accesslog-h2-rf-no-route/envoy-rust.yaml`
- Create: `tests/fixtures/0064-accesslog-h2-rf-no-route/expectations.yaml`
- Create: `tests/fixtures/0064-accesslog-h2-rf-no-route/README.md`
- Create: `tests/differential/tests/access_log_h2_rf_no_route.rs`

**Interfaces:**
- Consumes: `Driver::Http2AccessLogByteExact` (Task 1), the H2 `NR` derive (Task 2), `differential::run_fixture(&Path) -> Result<()>` (pre-existing).
- Produces: fixture `0064`, the FIRST H2 access-log differential fixture in the project.

This task exercises Task 1 + Task 2 end-to-end. It requires Docker (`testcontainers` spins up upstream Envoy) — per this project's established discipline, if Docker/the differential suite is unavailable or flaky on the current dev host, note that in `PROGRESS.md` and treat CI as authoritative (this fixture has NO backend spawn — `clusters: []`, matching fixture `0056`'s zero-backend-spawn shape — so it does NOT carry the host-bridge-IP or virtiofs-file-watch fragility some OTHER fixtures have; it should be reliably runnable locally too).

- [ ] **Step 1: Write `envoy.yaml`**

Create `tests/fixtures/0064-accesslog-h2-rf-no-route/envoy.yaml`:

```yaml
node: { id: envoy-rust-phase-56-fixture-0064, cluster: envoy-rust-phase-56 }
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
                      path: /tmp/0064-envoy-mount/access.log
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
                    - name: backend_vh
                      domains: ["match.test"]
                      routes:
                        - name: myroute
                          match: { prefix: "/specific" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 2: Write `envoy-rust.yaml`**

Create `tests/fixtures/0064-accesslog-h2-rf-no-route/envoy-rust.yaml`:

```yaml
node: { id: envoy-rust-phase-56-fixture-0064, cluster: envoy-rust-phase-56 }
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
                      path: /tmp/0064-envoy-rust-mount/access.log
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
                    - name: backend_vh
                      domains: ["match.test"]
                      routes:
                        - name: myroute
                          match: { prefix: "/specific" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 3: Write `expectations.yaml`**

Create `tests/fixtures/0064-accesslog-h2-rf-no-route/expectations.yaml`:

```yaml
driver:
  kind: http2_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0064-envoy-mount/access.log
    envoy_rust: /tmp/0064-envoy-rust-mount/access.log
  probes:
    # Probe 1 — ROUTE-MISS. `Host: match.test` (`:authority` on H2) MATCHES
    # the non-wildcard vhost; `GET /nomatch` matches no route → the
    # no-matching-route synth_404. Envoy emits %RESPONSE_FLAGS% = NR
    # (state-0 recon, ADR-0113: {"rc":404,"rcd":"route_not_found","rf":"NR"}).
    - method: get
      path: /nomatch
      host: match.test
      expected_status: 404
    # Probe 2 — HOST-MISS. `Host: nomatch.test` matches NO vhost `domains`
    # entry → the no-matching-virtual_host synth_404. Envoy emits the same
    # NR flag here (state-0 recon, ADR-0113).
    - method: get
      path: /specific
      host: nomatch.test
      expected_status: 404
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static
  # literal: the `http2_access_log_byte_exact` driver asserts each emitted
  # line is byte-identical between upstream Envoy v1.33.0 and envoy-rust.
  # Both no-route synth_404 arms are deterministic on both sides, so each
  # rendered line is identical. Keys sort UTF-8 (ADR-0094 §A): method,
  # proto, rc, rcd, rf. Each line is:
  #   {"method":"GET","proto":"HTTP/2","rc":404,"rcd":"route_not_found","rf":"NR"}
```

- [ ] **Step 4: Write `README.md`**

Create `tests/fixtures/0064-accesslog-h2-rf-no-route/README.md`:

```markdown
# Fixture 0064 — H2 access-log `%RESPONSE_FLAGS%` no-route failure path (`NR`, byte-exact)

The **FIRST H2 access-log differential fixture** in the project (phase 56,
ADR-0113) — opens `Driver::Http2AccessLogByteExact`, the H2 sibling of the
H1-only `Driver::Http1AccessLogByteExact` (fixtures 0040/0046-0055/0058-0063).
The H2 analogue of fixture 0056 (phase 48): witnesses the FIRST H2
`%RESPONSE_FLAGS%` value, `NR` (NoRoute), byte-exact cross-proxy on BOTH the
route-miss and host-miss `synth_404` arms.

## What this proves

`rc`/`rcd`/`proto`/`method` were ALREADY byte-identical between envoy-rust
and live Envoy on H2 for this trigger before this phase (state-0/state-2
recon) — `response_code_details_for_log_h2` has been correctly set to
`Some("route_not_found")` on both no-route arms since phase 42/43
(ADR-0099/ADR-0100). The ONLY prior gap was `%RESPONSE_FLAGS%`, hard-coded
`"-"` at the H2 record-build site. Phase 56 derives it: `"NR"` when
`%RESPONSE_CODE_DETAILS%` is `route_not_found`, else `"-"` — the H2 mirror
of the H1 phase-48 one-arm derive at its ORIGINAL scope
(`crates/envoy-http1/src/hcm.rs:1377` as it stood before phases 49-54 each
added one more arm).

## Probes

| # | request (H2, `:authority` = Host)           | arm        | emitted JSON object (byte-identical on both sides) |
|---|-----------------------------------------------|------------|------------------------------------------------------|
| 1 | `GET /nomatch` with `:authority: match.test`   | route-miss | see below                                             |
| 2 | `GET /specific` with `:authority: nomatch.test`| host-miss  | see below                                             |

```
{"method":"GET","proto":"HTTP/2","rc":404,"rcd":"route_not_found","rf":"NR"}
```

The route table is the IDENTICAL shape fixture `0056` uses (a single vhost
`domains: ["match.test"]`, one `/specific` `direct_response` route) — only
`codec_type: HTTP2` + `http2_protocol_options: {}` differ. `clusters: []` —
no upstream, no backend spawn, no `{{BACKEND_IP}}`/`{{HTTP1_BACKEND_PORT}}`
machinery needed.

## Driver

`kind: http2_access_log_byte_exact` (new this phase — `Driver::Http2AccessLogByteExact`,
`tests/differential/src/lib.rs`). Drives each probe over H2-prior-knowledge
via `drive_http2`, scrapes both files, asserts the scraped line count equals
`probes.len()` (here 2), and calls
`access_log::assert_access_log_lines_byte_identical` — the exact same
assertion machinery `http1_access_log_byte_exact` uses; only the wire driver
(`drive_http2` vs `drive_http1`) differs.

## `0001`-`0063` byte-preservation

This phase's H2 `response_flags` derive change is additive — gated on
`response_code_details_for_log_h2 == Some("route_not_found")`, which NO
existing H2 fixture (`0009`, `0010`, `0018`, `0021`) triggers (none of them
even carries an `access_log` block). `Driver::Http2AccessLogByteExact` is a
brand-new variant no pre-existing fixture references. So all `0001`-`0063`
stay byte-identical; only the new `0064` observes the changed value.

## Cross-references

- ADR: ADR-0113 (state-1 brainstorm + state-2 PLAN — opens the H2
  access-log differential driver + the H2 `NR` witness).
- Related fixtures: `0056` (the H1 `NR` witness this fixture mirrors on H2).
- New carry-forward: **M56-1** — the remaining H2 `%RESPONSE_FLAGS%` values
  (`UH`/`UO`/`URX`/`UF`/`UC`) + the H2 failure-path `%RESPONSE_CODE_DETAILS%`
  strings beyond `route_not_found`, now unblocked for future one-flag-at-a-time
  phases (the same cadence phases 49-54 used for H1 after phase 48).
```

- [ ] **Step 5: Write the differential test**

Create `tests/differential/tests/access_log_h2_rf_no_route.rs`:

```rust
//! Docker-gated differential test for fixture 0064-accesslog-h2-rf-no-route.
//! Phase 56 (ADR-0113) — the FIRST H2 access-log differential fixture in the
//! project. Opens `Driver::Http2AccessLogByteExact` (the H2 sibling of the
//! H1-only `Driver::Http1AccessLogByteExact`) and witnesses the FIRST H2
//! `%RESPONSE_FLAGS%` value, `NR` (NoRoute), byte-exact cross-proxy on BOTH
//! the route-miss and host-miss `synth_404` arms — the H2 analogue of
//! fixture 0056 (phase 48). `rc`/`rcd`/`proto`/`method` were already
//! byte-identical on H2 before this phase; the H2 record-build site's
//! previously hard-coded `response_flags: "-"` now derives `"NR"` from
//! `%RESPONSE_CODE_DETAILS%` = `route_not_found`. Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact`; reads each side's file access-log
//! and asserts every emitted line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":404,"rcd":"route_not_found","rf":"NR"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rf_no_route() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0064-accesslog-h2-rf-no-route");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 6: Confirm `0064` is genuinely next-free (re-check for a sibling race)**

Run: `ls tests/fixtures/ | sort -t- -k1 -n | tail -3`
Expected: `0064-accesslog-h2-rf-no-route` is the highest entry (no sibling session landed a `0064` first). If a sibling DID land a `0064` first, rename this fixture's directory + the `kind`/`expected_access_log_paths` mount-dir strings + the `README.md`/test-file references to the next-free number instead, and note the renumbering in `PROGRESS.md`.

- [ ] **Step 7: Run the new differential test**

Run: `cargo test -p differential --test access_log_h2_rf_no_route -- --nocapture`
Expected: PASS (requires Docker running locally; if Docker is unavailable on this host, note that in `PROGRESS.md` and defer to CI — per this project's `differential-harness-uses-debug-envoy-bin` memory, ensure `cargo build -p envoy-bin` has been run first with a DEBUG build, since the differential harness runs `target/debug/envoy-bin`, not release).

- [ ] **Step 8: Run the full differential suite to confirm no regression**

Run: `cargo test -p differential`
Expected: all pre-existing differential tests still PASS (fixtures `0001`-`0063` unaffected — see the README's byte-preservation argument above). This may take several minutes; per this project's standing memories, some fixtures are known to flake locally under parallel load or on this dev host's network topology (`differential-fixtures-flake-under-parallel-load`, `differential-host-bridge-ip-192-168-65-2`) — CI is authoritative for any such flake, not a regression from this phase's change.

- [ ] **Step 9: Commit**

```bash
git add tests/fixtures/0064-accesslog-h2-rf-no-route/ tests/differential/tests/access_log_h2_rf_no_route.rs
git commit -m "phase 56 task 3: fixture 0064 (H2 NR access-log witness) + differential test"
```

---

## Task 4: `BEHAVIOR_CONTRACT.md` updates

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

**Interfaces:**
- Consumes: nothing code-level — this is a documentation-only task recording what Tasks 1-3 proved.
- Produces: an updated `%RESPONSE_FLAGS%` row and `%RESPONSE_CODE_DETAILS%` row reflecting the now-open H2 driver + the `NR` witness + the M45-1→M56-1 carry-forward transition.

- [ ] **Step 1: Update the `%RESPONSE_FLAGS%` row**

Open `docs/envoy-rust/BEHAVIOR_CONTRACT.md`. Find the sentence (in the `%RESPONSE_FLAGS%` row, currently ending near line 1020): `H2 no-route/no-healthy/overflow/retry-limit-exceeded %RESPONSE_FLAGS% deferred (M45-1 — no H2 access-log differential driver).`

Replace it with:

```
The H2 access-log differential driver now exists (`Driver::Http2AccessLogByteExact`, phase 56, ADR-0113) and `NR` is witnessed byte-exact on H2 by fixture **0064** — CONSUMING carry-forward **M45-1**. The remaining H2 `%RESPONSE_FLAGS%` values (`UH`/`UO`/`URX`/`UF`/`UC`) remain deferred as NEW carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

(Re-grep the exact current wording immediately before editing — this SPEC/PLAN cites the phase-56-state-1-commit tree; if the sentence has drifted even slightly, match on the surrounding load-bearing phrase `no H2 access-log differential driver` instead of the full sentence.)

- [ ] **Step 2: Update the `%RESPONSE_CODE_DETAILS%` row**

Find the sentence (in the `%RESPONSE_CODE_DETAILS%` row, currently ending near line 1031): `the H2 failure-path details remain deferred (M45-1 — the H2 no-healthy arm returns 502, no H2 access-log differential driver).`

Replace it with:

```
`route_not_found` is now ALSO witnessed on H2 (fixture **0064**, phase 56, ADR-0113) — the H2 access-log differential driver now exists. The remaining H2 failure-path details (beyond `route_not_found`) remain deferred as part of carry-forward **M56-1** (which also carries forward, un-investigated, the note that the H2 no-healthy arm returns 502 — flagged in passing during phase 56's SPEC drafting, not yet reconciled).
```

- [ ] **Step 3: Verify the edits render sensibly**

Run: `grep -n "M45-1\|M56-1" docs/envoy-rust/BEHAVIOR_CONTRACT.md`
Expected: `M45-1` no longer appears as a live-deferral marker in these two rows (it may still appear elsewhere as a historical reference in surrounding prose — that's fine, do not touch unrelated M45-1 mentions outside these two specific sentences); `M56-1` appears in both edited rows.

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 56 task 4: BEHAVIOR_CONTRACT.md — H2 NR witnessed, M45-1 consumed, M56-1 opened"
```

---

## Self-Review

**Spec coverage:** SPEC §A (Driver variant) → Task 1. §B (H2 derive) → Task 2 Step 3. §C (fixture) → Task 3 Steps 1-4. §D (differential test) → Task 3 Step 5. §E (backstop) → Task 2 Steps 1-4. §F (BEHAVIOR_CONTRACT) → Task 4. §G (no fuzz target) → correctly no task added. All six SPEC scope items have a task.

**Placeholder scan:** every code block above is complete, copy-pasteable Rust/YAML/Markdown — no `TODO`/`TBD`/"similar to Task N" shorthand.

**Type consistency:** `Driver::Http2AccessLogByteExact { probes: Vec<AccessLogByteExactProbe>, expected_access_log_paths: AccessLogPaths }` (Task 1) is referenced identically in the `expectations.yaml` `kind: http2_access_log_byte_exact` (Task 3, auto-mapped by serde's `rename_all = "snake_case"`) and in the differential test's use of `differential::run_fixture` (unchanged signature). `response_flags_for_log_h2: &str` (Task 2 Step 3) is a local `let` binding consumed once, on the same line it's declared near — no cross-task signature drift possible since it never crosses a function boundary.

## §7.5 phase-done gate (re-run in full at state-4, after all 4 tasks land)

`cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check`, the differential suite (fixture `0064` + all `0001`-`0063`), `h2spec` (≥95%, unaffected by this phase). No new fuzz target to run. Quote all command outputs into `PROGRESS.md` per `BOOTSTRAP_PROMPT.md` §5 state 4.

_PLAN authored by the state-2 session (`superpowers:writing-plans`), following SPEC.md (locked by ADR-0113) and this session's re-verification of every SPEC §3 PLAN-VERIFY item against the current tree. The §6.1 split does NOT fire (see "§6.1 split decision" above) — no new ADR needed this session. The state-3 implementation (`superpowers:subagent-driven-development` or `superpowers:executing-plans`, with TDD per task) is the session after._
