# Phase 48 — `48-accesslog-rf-no-route` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (failing test first) per doctrine D-3.1.

**Goal:** Differentially witness the FIRST non-`-` `%RESPONSE_FLAGS%` value — `NR` (NoRoute) — BYTE-EXACT on the no-route 404 path, by populating `AccessLogRecord.response_flags = "NR"` on envoy-rust's two H1 no-route `synth_404` arms (the field is hard-coded `"-"` at the single H1 record-build site today).

**Architecture:** Purely additive, single-site `src/` change. At the H1 unified access-log record-build site (`crates/envoy-http1/src/hcm.rs:1225`) the `response_flags` field is hard-coded `"-".to_owned()`. Both no-route `synth_404` arms (host-miss `:1536` + route-miss `:1555`) already thread `response_code_details_for_log = Some("route_not_found")` into the record built unconditionally below the writer-arm match. `route_not_found` is set ONLY at those two arms → it is 1:1 with Envoy's `NR` (NoRoute) flag. So we **derive** the flag at the build site: `response_flags = "NR"` when `response_code_details_for_log == Some("route_not_found")`, else `"-"`. No new `Op` / `AccessLogRecord` field / variable / enum field / crate / dependency / fuzz-target / `ConfigError` variant. The `%RESPONSE_FLAGS%` operator already renders in all three encoders; only the backing value changes on this one path.

**Tech Stack:** Rust (`crates/envoy-http1`), `envoy-accesslog` (`FileSink` + `CompiledJsonFormat`), `tokio` test harness, the Docker-gated differential harness (`tests/differential`, `kind: http1_access_log_byte_exact`), fixture data under `tests/fixtures/`.

---

## Threading mechanism — DECIDED (resolves SPEC §3.1 / §B)

**Chosen: option (b) "derive", builder-site variant.** At `hcm.rs:1225`, replace the hard-coded `response_flags: "-".to_owned()` with a derivation from the already-computed `response_code_details_for_log`:

```rust
// phase 48 (ADR-0105): %RESPONSE_FLAGS% = NR (NoRoute) on the no-route 404
// path. `route_not_found` is set (via the writer-arm at :866) ONLY at the two
// no-route synth_404 arms (host-miss :1536 + route-miss :1555) → it is 1:1
// with Envoy's NR flag. All other paths keep the "-" no-flags sentinel.
// (read-by-ref here; `response_code_details_for_log` is moved into the record
// at the `response_code_details:` field below.)
response_flags: if response_code_details_for_log.as_deref() == Some("route_not_found") {
    "NR"
} else {
    "-"
}
.to_owned(),
```

**Why this over the alternatives:**
- **Minimal + additive.** One field-expression change at one site. No new mutable variable, no `BuildOutcome::Synth` enum-field change, no edits to the 5 `Synth` construction sites. Smallest possible diff that satisfies the scope → lowest risk to the `0001`-`0055` byte-identical invariant.
- **Correct + 1:1.** `response_code_details_for_log` is `Some("route_not_found")` at `hcm.rs:1225` **iff** one of the two no-route `synth_404` arms fired (re-grep §6.2 finding 2 confirms `Some("route_not_found")` appears at ONLY `:1536` and `:1555`; the proxy-success arm sets `via_upstream`, `synth_400`/`synth_501` set `None`, `direct_response` sets `Some("direct_response")`). The borrow at `:1225` is valid: `response_code_details_for_log` is read by reference here and not moved until the `response_code_details:` field at `:1249`.
- **Rejected option (a)** — extend `BuildOutcome::Synth(Response, Option<&'static str>)` (`hcm.rs:1403`) with a flags field and set it at all 5 construction sites — as wider-churn YAGNI. It buys explicit flag/RCD separation we do not need for a single 1:1 flag. The future non-1:1 flags (`UH`/`UF`/`UO`/`DC`/`URX`, deferred as **M45-2**, which ride non-deterministic connect/overflow/timeout surfaces) can revisit the threading shape if/when they land — they will need their own trigger plumbing regardless.

---

## §6.2 recon — DONE this session (re-verified against disk; M47-1 line-citation drift accounted for)

The state-2 §6.2 recon ran during PLAN authoring; findings locked into the tasks below:

1. **Set-sites re-verified (M47-1 drift confirmed & corrected).** `grep -n` over `crates/envoy-http1/src/hcm.rs` (NOTE the H1 HCM is `crates/envoy-http1/src/hcm.rs`, NOT `crates/envoy-http/...`):
   - **`:1225`** — `response_flags: "-".to_owned(), // 06.2 always emits "-"` — the SINGLE production record-build site (the edit target).
   - **`:1536`** — host-miss arm: `return BuildOutcome::Synth(synth_404(close), Some("route_not_found"));` (phase 47).
   - **`:1555`** — route-miss arm: `return BuildOutcome::Synth(synth_404(close), Some("route_not_found"));` (phase 46). ⚠️ ADR-0103 / the existing backstop doc-comments cite the stale `:1553`; the live line is `:1555` (M47-1 drift — do NOT trust the cited numbers).
   - **`:866`** (writer-arm): `response_code_details_for_log = details.map(str::to_owned);` — where the synth detail becomes the per-request RCD.
   - **`:1249`** — `response_code_details: response_code_details_for_log,` — the by-value move (read `response_flags` by-ref at `:1225`, before this).
   - Other `response_flags` occurrence: **`:1844`** `response_flags: "-".into(),` — a `Default`/test-helper constructor, NOT a production site → DO NOT touch.
   - `BuildOutcome::Synth` construction sites that KEEP `"-"`: `:809` (`synth_501`), `:1510` (`synth_400`), `:1562` (`direct_response`). DO NOT touch.
2. **Byte-preservation fixture list re-greped (exhaustive).** `grep -rln "RESPONSE_FLAGS" tests/fixtures/` → exactly `0012`, `0040`, `0046` log `%RESPONSE_FLAGS%`; ALL three are happy-path 200s (flag stays `"-"`). The two no-route 404 fixtures `0054`/`0055` log only `rc`/`rcd`/`method`/`proto` — NOT `%RESPONSE_FLAGS%`. ⇒ Deriving `"NR"` on the no-route path changes ZERO bytes in any existing fixture → all `0001`-`0055` stay byte-identical.
3. **Driver supports the two-probe fixture with NO harness change.** `Driver::Http1AccessLogByteExact { probes: Vec<AccessLogByteExactProbe>, .. }` (`tests/differential/src/lib.rs:121`); `AccessLogByteExactProbe` (`:1015`) carries `method`/`path`/`host`/`extra_headers`/`body`/`expected_status` (default 200). Fixture `0054` uses one probe; `0056` declares two (route-miss + host-miss), each `expected_status: 404`.
4. **Recon §A-§D facts NOT overturned** → per SPEC §6.2 / ADR-0105, NO new reconciliation ADR is needed. ADR-0105 stands.
5. **Fuzz SKIP confirmed.** `%RESPONSE_FLAGS%` is an existing standalone operator already covered by `accesslog_format_parse` / `parse_bootstrap`; it parses identically whether it renders `"-"` or `"NR"`. NO new operator/grammar → NO new fuzz target, `ci.yml` UNCHANGED.

---

## Task 1: In-process backstops — both no-route arms emit `rf:"NR"` (RED)

Add two `#[tokio::test]` unit tests in `crates/envoy-http1/src/hcm.rs` (the `#[cfg(test)] mod tests` block, adjacent to the phase-46/47 backstops `h1_route_miss_access_log_carries_route_not_found_rcd` at `:5365` and `h1_host_miss_access_log_carries_route_not_found_rcd` at `:5457`). Each is a near-verbatim clone of its phase-46/47 sibling, with `rf: "%RESPONSE_FLAGS%"` added to the `json_format` map and the asserted line extended to include `"rf":"NR"`. These cover BOTH arms (SPEC §E) and are the canonical RED test for the derive.

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (test module; insert after `:5535`, the end of `h1_host_miss_access_log_carries_route_not_found_rcd`)

- [ ] **Step 1: Write the two failing tests**

```rust
    /// Phase 48 T1 backstop (ADR-0105): the route-miss no-route `synth_404` arm
    /// (hcm.rs:1555) emits `%RESPONSE_FLAGS%` = `NR` (NoRoute). Clone of
    /// `h1_route_miss_access_log_carries_route_not_found_rcd` with `rf` added to
    /// the json_format. `route_not_found` (set at the route-miss arm) is 1:1 with
    /// the NR flag, derived at the record-build site (hcm.rs:1225). The 404
    /// status/body are UNCHANGED (additive). Keys sort UTF-8: rc, rcd, rf.
    #[tokio::test]
    async fn h1_route_miss_access_log_carries_nr_flag() {
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
        // vhost `domains:["*"]` (host-miss arm never hit) + a SINGLE route on
        // `/specific`. Probing `/nomatch` misses → the route-miss arm (:1555).
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
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
                            prefix: Some("/specific".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(envoy_config::DirectResponse {
                            status: 200,
                            body: envoy_config::DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
        });
        let req = b"GET /nomatch HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 404 "),
            "route-miss synth-404 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.contains("content-length: 0\r\n"),
            "route-miss synth-404 body unchanged (empty): {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\",\"rf\":\"NR\"}\n",
            "route-miss access-log line carries rf:\"NR\": {logged:?}"
        );
    }

    /// Phase 48 T1 backstop (ADR-0105): the host-miss no-route `synth_404` arm
    /// (hcm.rs:1536) emits `%RESPONSE_FLAGS%` = `NR` (NoRoute). Clone of
    /// `h1_host_miss_access_log_carries_route_not_found_rcd` with `rf` added. The
    /// `Host: nomatch.test` MUST be non-empty (an empty Host trips the codec's
    /// synth_400 guard — a different path).
    #[tokio::test]
    async fn h1_host_miss_access_log_carries_nr_flag() {
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
        // vhost `domains:["match.test"]` (NON-wildcard) + catch-all `/` route.
        // Probing `Host: nomatch.test` matches NO vhost → the host-miss arm (:1536).
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_empty().await,
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
                    domains: vec!["match.test".to_string()],
                    include_attempt_count_in_response: false,
                    routes: vec![Route {
                        name: String::new(),
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(envoy_config::DirectResponse {
                            status: 200,
                            body: envoy_config::DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
        });
        let req = b"GET / HTTP/1.1\r\nHost: nomatch.test\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 404 "),
            "host-miss synth-404 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.contains("content-length: 0\r\n"),
            "host-miss synth-404 body unchanged (empty): {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":404,\"rcd\":\"route_not_found\",\"rf\":\"NR\"}\n",
            "host-miss access-log line carries rf:\"NR\": {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they FAIL**

Run: `cargo test -p envoy-http1 h1_route_miss_access_log_carries_nr_flag h1_host_miss_access_log_carries_nr_flag`
Expected: BOTH FAIL on the `assert_eq!` — the emitted line is `{"rc":404,"rcd":"route_not_found","rf":"-"}\n` (the field is still hard-coded `"-"` at `:1225`), not the expected `...,"rf":"NR"}`.

- [ ] **Step 3: Commit the RED tests**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 48: T1 in-process backstops for rf:\"NR\" on both no-route arms (RED) [ADR-0105]"
```

---

## Task 2: Thread `response_flags = "NR"` at the H1 record-build site (GREEN)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs:1225`

- [ ] **Step 1: Apply the derive (replace the hard-coded `"-"`)**

Replace exactly:

```rust
                response_flags: "-".to_owned(), // 06.2 always emits "-"
```

with:

```rust
                // phase 48 (ADR-0105): %RESPONSE_FLAGS% = NR (NoRoute) on the
                // no-route 404 path. `route_not_found` is set (via the writer-arm
                // at :866) ONLY at the two no-route synth_404 arms (host-miss
                // :1536 + route-miss :1555) → it is 1:1 with Envoy's NR flag.
                // All other paths keep the "-" no-flags sentinel. Read by-ref
                // here; `response_code_details_for_log` is moved into the
                // `response_code_details:` field below.
                response_flags: if response_code_details_for_log.as_deref()
                    == Some("route_not_found")
                {
                    "NR"
                } else {
                    "-"
                }
                .to_owned(),
```

- [ ] **Step 2: Run the Task-1 backstops to verify they PASS**

Run: `cargo test -p envoy-http1 h1_route_miss_access_log_carries_nr_flag h1_host_miss_access_log_carries_nr_flag`
Expected: BOTH PASS (emitted line now `{"rc":404,"rcd":"route_not_found","rf":"NR"}\n`).

- [ ] **Step 3: Run the full crate test suite to verify NO regression**

Run: `cargo test -p envoy-http1`
Expected: PASS, including the phase-46/47 `..._carries_route_not_found_rcd` backstops (their `json_format` does NOT log `rf` → their asserted lines are unaffected) and all happy-path access-log tests (flag stays `"-"`).

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 48: thread response_flags=NR on H1 no-route synth_404 arms (GREEN) [ADR-0105]"
```

---

## Task 3: Fixture `0056-accesslog-rf-no-route` (two probes: route-miss + host-miss)

Build the fixture from the `0054` template (a `direct_response` listener, `clusters: []`, no upstream), switching the vhost to the NON-wildcard `domains: ["match.test"]` (the `0055` shape — required so the host-miss probe can miss the vhost) and adding `rf: "%RESPONSE_FLAGS%"` to the `json_format`. Two probes in `expectations.yaml`, each `expected_status: 404`.

**Files:**
- Create: `tests/fixtures/0056-accesslog-rf-no-route/envoy.yaml`
- Create: `tests/fixtures/0056-accesslog-rf-no-route/envoy-rust.yaml`
- Create: `tests/fixtures/0056-accesslog-rf-no-route/expectations.yaml`
- Create: `tests/fixtures/0056-accesslog-rf-no-route/README.md`

- [ ] **Step 1: Create `envoy.yaml`** (reference side — `admin` block present, `generate_request_id: false`, bind `0.0.0.0`, mount path `/tmp/0056-envoy-mount/access.log`)

```yaml
node: { id: envoy-rust-phase-48-fixture-0056, cluster: envoy-rust-phase-48 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http1_listener
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0056-envoy-mount/access.log
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

- [ ] **Step 2: Create `envoy-rust.yaml`** (subject side — NO `admin` block, NO `generate_request_id`, bind `127.0.0.1`, mount path `/tmp/0056-envoy-rust-mount/access.log`; route table + vhost + `json_format` BYTE-IDENTICAL to `envoy.yaml`)

```yaml
node: { id: envoy-rust-phase-48-fixture-0056, cluster: envoy-rust-phase-48 }
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
                      path: /tmp/0056-envoy-rust-mount/access.log
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

- [ ] **Step 3: Create `expectations.yaml`** (two probes — route-miss then host-miss — both `expected_status: 404`)

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0056-envoy-mount/access.log
    envoy_rust: /tmp/0056-envoy-rust-mount/access.log
  probes:
    # Probe 1 — ROUTE-MISS (the :1555 arm). `Host: match.test` MATCHES the
    # non-wildcard vhost; `GET /nomatch` matches no route → the no-matching-route
    # synth_404. Envoy emits %RESPONSE_FLAGS% = NR (state-1 recon, ADR-0105:
    # `{"rc":404,"rcd":"route_not_found","rf":"NR"}`). envoy-rust now derives
    # NR from `route_not_found` at the record-build site (hcm.rs:1225).
    - method: get
      path: /nomatch
      host: match.test
      expected_status: 404
    # Probe 2 — HOST-MISS (the :1536 arm). `Host: nomatch.test` matches NO vhost
    # `domains` entry → the no-matching-virtual_host synth_404 (the route walk
    # never runs). Envoy emits the same NR flag here (state-1 recon, ADR-0105).
    - method: get
      path: /specific
      host: nomatch.test
      expected_status: 404
  # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static literal:
  # the `http1_access_log_byte_exact` driver asserts each emitted line is
  # byte-identical between upstream Envoy v1.33.0 and envoy-rust. Both no-route
  # synth_404 arms are deterministic on both sides, so each rendered line is
  # identical. Keys sort UTF-8 (ADR-0094 §A): method, proto, rc, rcd, rf — the
  # json_format AUTHORING order { rc, rcd, rf, method, proto } is irrelevant.
  # Compact separators + ONE trailing `\n` (ADR-0092 §E). Each line is:
  #   {"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found","rf":"NR"}
```

- [ ] **Step 4: Create `README.md`** (clone of `0054`/`0055` README, retargeted to the `%RESPONSE_FLAGS%`=`NR` witness; document: the FIRST non-`-` flag; the two probes / two arms; the `domains: ["match.test"]` non-wildcard table; the per-side divergence table; the byte-identical line; the `0001`-`0055` byte-preservation argument; cross-refs to `0054`/`0055`/`0046`; deferred H2 (M45-1) + other flags (M45-2). Re-verify all `hcm.rs` line numbers cited in prose against the live file — M47-1: use `:1555` route-miss, `:1536` host-miss, `:1225` build-site.)

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0056-accesslog-rf-no-route/
git commit -m "phase 48: fixture 0056-accesslog-rf-no-route (two probes, rf:NR byte-exact) [ADR-0105]"
```

---

## Task 4: Differential test `access_log_rf_no_route.rs`

**Files:**
- Create: `tests/differential/tests/access_log_rf_no_route.rs` (a structural clone of `access_log_rcd_route_not_found.rs`, pointing at the `0056` fixture)

- [ ] **Step 1: Write the test wrapper**

```rust
//! Docker-gated differential test for fixture 0056-accesslog-rf-no-route.
//! Phase 48 (ADR-0105) — the FIRST non-`-` `%RESPONSE_FLAGS%` witness: `NR`
//! (NoRoute), BYTE-EXACT cross-proxy on the no-route 404 path. A route table
//! with a SINGLE NON-wildcard vhost `domains: ["match.test"]` (one `/specific`
//! direct_response route) is probed TWICE: (1) route-miss `GET /nomatch`
//! (`Host: match.test`) → the no-matching-route synth_404 arm (`hcm.rs:1555`);
//! (2) host-miss `GET /specific` (`Host: nomatch.test`) → the
//! no-matching-virtual_host synth_404 arm (`hcm.rs:1536`). Both are 404
//! `route_not_found` paths (`clusters: []`; no backend spawns). envoy-rust now
//! DERIVES `%RESPONSE_FLAGS%` = `NR` from `route_not_found` at the H1
//! record-build site (`hcm.rs:1225`; was the hard-coded `"-"`); upstream Envoy
//! v1.33 emits the same flag on both arms (state-1 recon:
//! `{"rc":404,"rcd":"route_not_found","rf":"NR"}`). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http1_access_log_byte_exact` (the json_format adds `rf:%RESPONSE_FLAGS%`);
//! reads each side's file access-log and asserts every emitted line is
//! byte-identical:
//!   {"method":"GET","proto":"HTTP/1.1","rc":404,"rcd":"route_not_found","rf":"NR"}
//! PURE cross-proxy equality (no static literal). H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_no_route() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0056-accesslog-rf-no-route");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Compile-check the differential crate** (the test is Docker-gated; it does NOT run the container locally on a clean compile, but it MUST compile)

Run: `cargo test -p differential --no-run`
Expected: compiles clean (no new harness code; the `0056` fixture deserializes against the existing `Http1AccessLogByteExact` driver).

> **NOTE (host-environment):** the Docker differential (real Envoy vs envoy-rust) is **CI-authoritative** at the state-4 §7.5 gate (see memory `envoy-rust-state4-ci-first-execution`). This host may not run the container reliably; do NOT treat a local Docker-gated non-run as a failure. The differential `0056` green + all `0001`-`0055` still green are confirmed by the state-4 CI run.

- [ ] **Step 3: Commit**

```bash
git add tests/differential/tests/access_log_rf_no_route.rs
git commit -m "phase 48: differential test access_log_rf_no_route (fixture 0056) [ADR-0105]"
```

---

## Task 5: BEHAVIOR_CONTRACT — `%RESPONSE_FLAGS%` row update

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1020` (the `%RESPONSE_FLAGS%` access-log-field-mapping row)

- [ ] **Step 1: Update the row** to record the first witnessed non-`-` flag

Replace exactly:

```
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`. 06.2 always emits the literal `"-"` (Envoy's no-flags sentinel). Future fixtures exercising non-`-` flag combinations need per-flag equivalence rules added to this table. | value-exact (06.2 no-flags case) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. |
```

with:

```
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`. Renders Envoy's no-flags sentinel `"-"` on every path EXCEPT the no-route 404 path, where it renders `NR` (NoRoute). **Per-flag equivalence — `NR`:** a config-deterministic single static constant (no combination, brace-free), set on BOTH H1 no-route `synth_404` arms (host-miss + route-miss), derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `route_not_found` at the H1 record-build site (`hcm.rs:1225`); the 404 status/body/headers/`%RESPONSE_CODE_DETAILS%` are unchanged. Other non-`-` flags (`UH`/`UF`/`UO`/`DC`/`URX`) remain unwitnessed (M45-2, non-deterministic surfaces) and still need their own per-flag rules. | value-exact (`-` no-flags case + `NR` no-route case) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. Phase 48 (ADR-0105) fixture **0056** witnesses `NR` byte-exact on BOTH the route-miss and host-miss 404 arms; both proxies emit `NR`. H2 no-route `%RESPONSE_FLAGS%` deferred (M45-1 — no H2 access-log differential driver). |
```

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 48: BEHAVIOR_CONTRACT %RESPONSE_FLAGS% row — first non-\"-\" flag NR witnessed (fixture 0056) [ADR-0105]"
```

---

## Task 6: Local verification sweep (state-3 close-out; full §7.5 gate runs at state-4)

This is the developer's local pre-flight — NOT the state-4 verification gate (that re-runs the full §7.5 set in CI and quotes outputs to `PROGRESS.md`). Run the cheap-and-local subset; the Docker differential + `0001`-`0055` byte-identical + h2spec + fuzz are CI-authoritative at state-4.

**Files:** none (verification only)

- [ ] **Step 1: clippy clean**

Run: `cargo clippy -p envoy-http1 -p differential --all-targets --all-features -- -D warnings`
Expected: no warnings (the `if/else` derive is idiomatic; no new lint surface).

- [ ] **Step 2: fmt clean**

Run: `cargo fmt --all -- --check`
Expected: clean. (If the inserted `if/else` block reflows, run `cargo fmt --all` and re-commit — see memory `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase.)

- [ ] **Step 3: full workspace unit tests** (non-Docker)

Run: `cargo test --workspace`
Expected: PASS (the two new `rf:"NR"` backstops + all existing tests; the differential Docker tests are `#[ignore]`/Docker-gated and skip locally).

- [ ] **Step 4: confirm byte-preservation reasoning (no existing fixture regressed)**

Run: `grep -rln "RESPONSE_FLAGS" tests/fixtures/`
Expected: only `0012`, `0040`, `0046` (+ the new `0056`). Re-confirm none of `0012`/`0040`/`0046` drives a no-route 404 (all happy-path 200 → flag stays `"-"`) → `0001`-`0055` byte-identical holds.

- [ ] **Step 5: final fmt-fix commit if needed** (otherwise nothing to commit)

```bash
cargo fmt --all
git add -A && git commit -m "phase 48: cargo fmt [ADR-0105]" || echo "nothing to reformat"
```

---

## Scope / gate summary

- **Task count:** 6 tasks (~40-110 LoC: one ~10-line derive at `hcm.rs:1225` + two ~75-line backstop tests + a 4-file fixture + a ~15-line differential test + a 1-row contract edit). **§6.1 split does NOT fire** (well under ~25 tasks / ~1500 LoC). **ADR-0106 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the lapsed-reservation convention).
- **No new** `Op` / `AccessLogRecord` field / variable / `BuildOutcome::Synth` enum field / crate / dependency / fuzz-target / `ConfigError` variant. `#![forbid(unsafe_code)]` holds.
- **Additive invariant:** all `0001`-`0055` fixtures stay byte-identical (§6.2 finding 2). Only the no-route path's previously-`"-"` flag changes — and no existing fixture both hits a no-route 404 AND logs `%RESPONSE_FLAGS%`.
- **Acceptance (re-run at state-4, SPEC §5):** (a) `0056` green (cross-proxy-equal `rf:"NR"` on both arms) + (b) all `0001`-`0055` green simultaneously + (c) h2spec ≥95% (no H2 change) + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean (no new target) + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved.
- **Carry-forwards:** M47-1 (line-citation drift) ACTIONED in this PLAN (re-verified `:1225`/`:1536`/`:1555`); M42-1 CONTINUED (not consumed — the `%RESPONSE_FLAGS%` vocabulary keeps expanding); M45-1 (H2 no-route flag) + M45-2 (non-deterministic flags UH/UF/UO/DC/URX) remain deferred.

_The state-3 implementation (`superpowers:executing-plans` or `superpowers:subagent-driven-development`) is the session AFTER this PLAN lands. Per §5.1, one state per session: this session writes the PLAN only._
