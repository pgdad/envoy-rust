# Phase 49 — `49-accesslog-rf-no-healthy` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD (failing test first) per doctrine D-3.1.

**Goal:** Differentially witness the SECOND non-`-` `%RESPONSE_FLAGS%` value — `UH` (NoHealthyUpstream) — BYTE-EXACT on the no-healthy-upstream 503 path, by adding one arm (`Some("no_healthy_upstream") => "UH"`) to the phase-48 `%RESPONSE_FLAGS%` derive at `crates/envoy-http1/src/hcm.rs:1232` (today it maps only `route_not_found => "NR"`, so this path renders the no-flags sentinel `"-"`).

**Architecture:** Purely additive, single-site `src/` change. At the H1 unified access-log record-build site (`crates/envoy-http1/src/hcm.rs:1219`), the `response_flags` field is derived (phase 48, ADR-0105) from the already-computed `response_code_details_for_log`: today an `if/else` mapping `Some("route_not_found") => "NR"`, else `"-"` (`:1232`-`:1239`). envoy-rust sets `response_code_details_for_log = Some("no_healthy_upstream")` at EXACTLY ONE per-request site — `hcm.rs:1000-1001`, the `pick()->None` no-healthy synth-503 arm (phase 45, ADR-0102) — so `no_healthy_upstream` is 1:1 with Envoy's `UH` (NoHealthyUpstream) flag, exactly as `route_not_found` is 1:1 with `NR`. We convert the `if/else` to a three-arm `match` that adds `Some("no_healthy_upstream") => "UH"`. No new `Op` / `AccessLogRecord` field / variable / enum field / crate / dependency / fuzz-target / `ConfigError` variant. The `%RESPONSE_FLAGS%` operator already renders in all three encoders; only the backing value changes on this one path, and the `route_not_found => "NR"` mapping is UNCHANGED → fixture `0056` stays byte-identical.

**Tech Stack:** Rust (`crates/envoy-http1`), `envoy-accesslog` (`FileSink` + `CompiledJsonFormat`), `tokio` test harness, the Docker-gated differential harness (`tests/differential`, `kind: http1_access_log_byte_exact`), fixture data under `tests/fixtures/`.

---

## Derive-extension form — DECIDED (resolves SPEC §3.1)

**Chosen: convert the phase-48 `if/else` at `hcm.rs:1232` into a three-arm `match`.** Both a `match` and a chained `else if` are output-equivalent (SPEC §3.1); the `match` is chosen because it reads cleanest with three arms and makes the 1:1 RCD→flag mapping table explicit. At `hcm.rs:1219` the `AccessLogRecord` is built; the `response_flags:` field currently reads:

```rust
                response_flags: if response_code_details_for_log.as_deref()
                    == Some("route_not_found")
                {
                    "NR"
                } else {
                    "-"
                }
                .to_owned(),
```

becomes:

```rust
                response_flags: match response_code_details_for_log.as_deref() {
                    Some("route_not_found") => "NR",
                    Some("no_healthy_upstream") => "UH",
                    _ => "-",
                }
                .to_owned(),
```

**Why this over the alternatives:**
- **Minimal + additive.** One field-expression change at one site, adding exactly one arm. No new mutable variable, no enum-field change, no edits to the `BuildOutcome::Synth` construction sites. Smallest possible diff that satisfies the scope → lowest risk to the `0001`-`0056` byte-identical invariant.
- **Correct + 1:1.** `response_code_details_for_log` is `Some("no_healthy_upstream")` at the build site **iff** the `pick()->None` no-healthy synth-503 arm fired (§6.2 recon finding 1 confirms `Some("no_healthy_upstream".to_owned())` is set at ONLY `hcm.rs:1001`). The `route_not_found => "NR"` arm is preserved verbatim → the no-route path is unchanged. The borrow at `:1232` is valid: `response_code_details_for_log` is read by reference (`.as_deref()`) here and not moved until the `response_code_details:` field at `:1263`.
- **Rejected — chained `else if`.** Output-equivalent but reads worse as the number of mapped details grows; the `match` over `.as_deref()` is the idiomatic Rust form for a finite set of string-literal cases.

---

## §6.2 recon — DONE this session (re-verified against disk; M48-1 line-citation precision accounted for)

The state-2 §6.2 recon ran during PLAN authoring; findings locked into the tasks below. (`rg`/`grep -n` over the live tree, this session.)

1. **Single `no_healthy_upstream` set-site re-verified.** `grep -rn "no_healthy_upstream" crates/` → the per-request RCD `response_code_details_for_log = Some("no_healthy_upstream".to_owned());` is set at EXACTLY **`hcm.rs:1001`** (the `pick()->None` `else` arm; comment at `:999`). All other `no_healthy_upstream` occurrences are NOT the RCD set-site and MUST NOT be touched: `:437` (`response: synth_no_healthy_upstream(close)` — the synth *response* body, not the RCD) + `:1693` (`fn synth_no_healthy_upstream` definition) + the test-module references (`:5048`/`:5053`/`:5285`/`:5291`/`:5364`/`:5365`). The sibling detail set at the same dispatch point is `via_upstream` (`:995`, the routed-success arm); the no-route arms set `route_not_found`. None aliases the no-healthy path → the new `UH` arm is provably 1:1. (The whole-tree `grep -rn no_healthy_upstream crates/` also surfaces two non-`hcm.rs` occurrences — `crates/envoy-bin/tests/upstream_active_health_check.rs:187`/`:243`, a test-function name + a comment, NOT RCD set-sites — so the single-set-site claim holds across the whole `crates/` tree, not just `hcm.rs`; plan-review M-1.)
2. **Derive site re-verified.** The phase-48 `response_flags` derive is the `if/else` at **`hcm.rs:1232`-`:1239`** (inside the `AccessLogRecord { … }` literal that starts at `:1219`); the `.as_deref()` shared borrow ends at `:1239` and the owned `String` moves into `response_code_details:` at **`:1263`** — the edit preserves this borrow-before-move discipline (carry-forward **M48-1** ACTIONED: the live derive is at `:1232`, the record-build site at `:1219`/`:1225`).
3. **Byte-preservation fixture list re-greped (exhaustive).** `grep -rln "RESPONSE_FLAGS" tests/fixtures/` → exactly `0012`, `0040`, `0046` (happy-path 200/direct_response → flag stays `"-"`) and `0056` (no-route 404 → `"NR"`, set by the UNCHANGED `route_not_found` arm). NONE drives a no-healthy-upstream 503. The no-healthy fixture `0053` logs only `rc`/`rcd`/`method`/`proto` — NOT `%RESPONSE_FLAGS%` (verified: `grep -rln RESPONSE_FLAGS tests/fixtures/0053-accesslog-rcd-no-healthy/` returns nothing). ⇒ Adding the `UH` arm changes ZERO bytes in any existing fixture → all `0001`-`0056` stay byte-identical. `0056` (the `NR` fixture) is specifically untouched because the `route_not_found` arm is preserved verbatim.
4. **Clone sources present.** The backstop `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` (`hcm.rs:5291`) + the differential test `tests/differential/tests/access_log_rcd_no_healthy.rs` + the fixture `tests/fixtures/0053-accesslog-rcd-no-healthy/` (envoy.yaml/envoy-rust.yaml/expectations.yaml/README.md) all exist. Next free fixture number = `0057` (`0056` is the highest).
5. **Driver supports the one-probe fixture with NO harness change.** The `http1_access_log_byte_exact` driver already drives a `GET /` probe with `expected_status: 503` (proven by `0053`); fixture `0057` reuses that path verbatim with `rf:"%RESPONSE_FLAGS%"` added to the `json_format`. No harness change.
6. **Recon §A-§E facts NOT overturned** → per SPEC §6.2 / ADR-0106, NO new reconciliation ADR is needed. ADR-0106 stands.
7. **Fuzz SKIP confirmed.** `%RESPONSE_FLAGS%` is an existing standalone operator already covered by `accesslog_format_parse` / `parse_bootstrap`; it parses identically whether it renders `"-"`, `"NR"`, or `"UH"`. NO new operator/grammar → NO new fuzz target, `ci.yml` UNCHANGED.

---

## Task 1: In-process backstop — the no-healthy arm emits `rf:"UH"` (RED)

Add one `#[tokio::test]` unit test in `crates/envoy-http1/src/hcm.rs` (the `#[cfg(test)] mod tests` block), a near-verbatim clone of the phase-45 backstop `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` (`:5291`), with `rf: "%RESPONSE_FLAGS%"` added to the `json_format` map and the asserted line extended to include `"rf":"UH"`. This is the canonical RED test for the derive (SPEC §E). The no-healthy path is a SINGLE arm (unlike phase 48's two no-route arms) → one backstop.

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (test module; insert immediately after `:5367`, the end of `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd`)

- [ ] **Step 1: Write the failing test**

```rust
    /// Phase 49 T1 backstop (ADR-0106): the no-healthy `pick()->None` synth-503
    /// arm (hcm.rs:1000-1001) emits `%RESPONSE_FLAGS%` = `UH` (NoHealthyUpstream).
    /// Clone of `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` with
    /// `rf` added to the json_format. `no_healthy_upstream` (set at the
    /// `pick()->None` arm) is 1:1 with the UH flag, derived at the record-build
    /// site (hcm.rs:1232). The 503 status/body are UNCHANGED (additive). Keys
    /// sort UTF-8: rc, rcd, rf.
    #[tokio::test]
    async fn h1_no_healthy_access_log_carries_uh_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // json_format logging %RESPONSE_CODE% (rc) + %RESPONSE_CODE_DETAILS%
        // (rcd) + %RESPONSE_FLAGS% (rf) — keys sort by UTF-8 byte order
        // (rc, rcd, rf).
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
        // A route to `subset_cluster` whose metadata_match selects a
        // non-existent subset (`{stage:nonexistent}`) → subset-miss → 503.
        let mut envoy_lb = std::collections::BTreeMap::new();
        envoy_lb.insert("stage".to_string(), "nonexistent".to_string());
        let config = Arc::new(HCMConfig {
            stat_prefix: "ingress_http".to_string(),
            cluster_mgr: cluster_mgr_no_fallback_subset().await,
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
                            cluster: "subset_cluster".into(),
                            retry_policy: None,
                            hash_policy: vec![],
                            metadata_match: Some(LbMetadata { envoy_lb }),
                        }),
                        typed_per_filter_config: Default::default(),
                    }],
                }],
            })),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        // The 503 + body must be UNCHANGED by the additive flag derive.
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "no-healthy synth-503 status unchanged: {resp_str}"
        );
        assert!(
            resp_str.ends_with("no healthy upstream"),
            "no-healthy synth-503 body unchanged: {resp_str}"
        );
        // Brief yield so the FileSink flush reaches disk.
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"no_healthy_upstream\",\"rf\":\"UH\"}\n",
            "no-healthy access-log line carries rf:\"UH\": {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the test to verify it FAILS**

Run: `cargo test -p envoy-http1 h1_no_healthy_access_log_carries_uh_flag`
Expected: FAIL on the `assert_eq!` — the emitted line is `{"rc":503,"rcd":"no_healthy_upstream","rf":"-"}\n` (the derive maps only `route_not_found`; `Some("no_healthy_upstream")` falls through to the `"-"` else-branch at `:1237`), not the expected `...,"rf":"UH"}`.

- [ ] **Step 3: Commit the RED test**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 49: T1 in-process backstop for rf:\"UH\" on the no-healthy arm (RED) [ADR-0106]"
```

---

## Task 2: Add the `no_healthy_upstream => "UH"` arm to the H1 `%RESPONSE_FLAGS%` derive (GREEN)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs:1225` (the comment block) + `:1232`-`:1239` (the derive)

- [ ] **Step 1: Apply the derive extension (replace the `if/else` with a three-arm `match`)**

Replace exactly (the comment block at `:1225`-`:1231` plus the `if/else` at `:1232`-`:1239`):

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

with:

```rust
                // phase 48 (ADR-0105) / phase 49 (ADR-0106): %RESPONSE_FLAGS% is
                // derived 1:1 from the per-request %RESPONSE_CODE_DETAILS%:
                //   route_not_found     → NR (NoRoute)          — the two no-route
                //                          synth_404 arms (host-miss :1536 +
                //                          route-miss :1555).
                //   no_healthy_upstream → UH (NoHealthyUpstream) — the single
                //                          pick()->None no-healthy synth-503 arm
                //                          (:1000-1001).
                // Each detail is set ONLY on its own arm(s) → each is 1:1 with
                // its flag. All other paths keep the "-" no-flags sentinel. Read
                // by-ref here; `response_code_details_for_log` is moved into the
                // `response_code_details:` field below.
                response_flags: match response_code_details_for_log.as_deref() {
                    Some("route_not_found") => "NR",
                    Some("no_healthy_upstream") => "UH",
                    _ => "-",
                }
                .to_owned(),
```

- [ ] **Step 2: Run the Task-1 backstop to verify it PASSES**

Run: `cargo test -p envoy-http1 h1_no_healthy_access_log_carries_uh_flag`
Expected: PASS (emitted line now `{"rc":503,"rcd":"no_healthy_upstream","rf":"UH"}\n`).

- [ ] **Step 3: Run the full crate test suite to verify NO regression**

Run: `cargo test -p envoy-http1`
Expected: PASS, including:
- the phase-48 `h1_route_miss_access_log_carries_nr_flag` / `h1_host_miss_access_log_carries_nr_flag` backstops (the `route_not_found => "NR"` arm is unchanged → `rf:"NR"` still emitted);
- the phase-45 `h1_no_healthy_access_log_carries_no_healthy_upstream_rcd` backstop (its `json_format` does NOT log `rf` → its asserted line `{"rc":503,"rcd":"no_healthy_upstream"}\n` is unaffected);
- all happy-path access-log tests (flag stays `"-"`).

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 49: add no_healthy_upstream=>UH arm to H1 %RESPONSE_FLAGS% derive (GREEN) [ADR-0106]"
```

---

## Task 3: Fixture `0057-accesslog-rf-no-healthy` (one probe: no-healthy 503)

Build the fixture from the `0053` template (the `subset_cluster` + NO_FALLBACK `lb_subset_config` + a route `metadata_match` selecting the non-existent `stage: nonexistent` subset → `pick()->None` synth-503), adding `rf: "%RESPONSE_FLAGS%"` to the `json_format`. One probe in `expectations.yaml`, `expected_status: 503`. Retarget the node id / mount paths from `0053` to `0057` / phase-49.

**Files:**
- Create: `tests/fixtures/0057-accesslog-rf-no-healthy/envoy.yaml`
- Create: `tests/fixtures/0057-accesslog-rf-no-healthy/envoy-rust.yaml`
- Create: `tests/fixtures/0057-accesslog-rf-no-healthy/expectations.yaml`
- Create: `tests/fixtures/0057-accesslog-rf-no-healthy/README.md`

- [ ] **Step 1: Create `envoy.yaml`** (reference side — `admin` block present, bind `0.0.0.0`, mount path `/tmp/0057-envoy-mount/access.log`; the `0053` topology with `rf:"%RESPONSE_FLAGS%"` added to the `json_format`)

```yaml
node: { id: envoy-rust-phase-49-fixture-0057, cluster: envoy-rust-phase-49 }
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
                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/0057-envoy-mount/access.log
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
                      # (the fixture-0038 `/nope` pattern). NO_FALLBACK → no
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

- [ ] **Step 2: Create `envoy-rust.yaml`** (subject side — NO `admin` block, bind `127.0.0.1`, mount path `/tmp/0057-envoy-rust-mount/access.log`; route table + cluster + `json_format` BYTE-IDENTICAL to `envoy.yaml` modulo the address/mount-path/admin divergences)

```yaml
node: { id: envoy-rust-phase-49-fixture-0057, cluster: envoy-rust-phase-49 }
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
                      path: /tmp/0057-envoy-rust-mount/access.log
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
                      # (the fixture-0038 `/nope` pattern). NO_FALLBACK → no
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

- [ ] **Step 3: Create `expectations.yaml`** (one probe — no-healthy 503 — `expected_status: 503`)

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0057-envoy-mount/access.log
    envoy_rust: /tmp/0057-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `subset_cluster` via a `metadata_match`
    # selecting the NON-EXISTENT `stage: nonexistent` subset (the fixture-0038
    # `/nope` pattern). The `lb_subset_config` is NO_FALLBACK, so the subset
    # resolves to NO eligible endpoint → `pick()->None` → the no-healthy-upstream
    # synth-503 (`no healthy upstream`) at ROUTING time — the literal
    # `127.0.0.1:1` endpoint is never dialed. This is the SECOND non-`-`
    # %RESPONSE_FLAGS% witness: UH (NoHealthyUpstream) (phase 49, ADR-0106), the
    # %RESPONSE_FLAGS% analogue of phase 45's %RESPONSE_CODE_DETAILS% =
    # `no_healthy_upstream` (fixture 0053).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). There is NO static
    # expected literal: the `http1_access_log_byte_exact` driver asserts every
    # line is byte-identical between upstream Envoy v1.33.0 and envoy-rust. The
    # no-healthy synth-503 is deterministic on BOTH sides, so the rendered line
    # is identical. envoy-rust now DERIVES `%RESPONSE_FLAGS%` = `UH` from
    # `%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream` at the H1 record-build
    # site (hcm.rs:1232; was the no-flags sentinel `"-"`).
    #
    # The tokens are all config-deterministic regardless of CI run:
    #   rc:     "%RESPONSE_CODE%"          → 503  (json NUMBER, not a string —
    #                                              precedent fixture 0047)
    #   rcd:    "%RESPONSE_CODE_DETAILS%"  → "no_healthy_upstream"
    #   rf:     "%RESPONSE_FLAGS%"         → "UH"
    #   method: "%REQ(:METHOD)%"           → "GET"
    #   proto:  "%PROTOCOL%"              → "HTTP/1.1"
    #
    # Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rcd, rf —
    # the json_format AUTHORING order { rc, rcd, rf, method, proto } is
    # irrelevant. Compact separators + ONE trailing `\n` (ADR-0092 §E). The
    # emitted line is:
    #   {"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
    # `expected_status: 503` — the driver asserts each probe's upstream status on
    # BOTH sides before scraping the access-log line (it defaults to 200). The
    # no-healthy synth returns 503, so declare it.
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `README.md`** (clone of `0053`'s README, retargeted to the `%RESPONSE_FLAGS%`=`UH` witness; document: the SECOND non-`-` flag (after phase 48's `NR`); the single probe / single no-healthy arm; the `subset_cluster` NO_FALLBACK topology; the per-side divergence table (admin/bind/mount path); the byte-identical line `{"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}`; the `0001`-`0056` byte-preservation argument; cross-refs to `0053` (the `rcd` sibling) / `0056` (the `NR` sibling); deferred H2 (M45-1) + other flags UF/UO/DC/URX (M45-2, `UH` now consumed). Re-verify every `hcm.rs` line number cited in prose against the live file — M48-1: derive at `:1232`, record-build at `:1219`/`:1225`, no-healthy RCD set-site at `:1000-1001`.)

- [ ] **Step 5: Commit**

```bash
git add tests/fixtures/0057-accesslog-rf-no-healthy/
git commit -m "phase 49: fixture 0057-accesslog-rf-no-healthy (one probe, rf:UH byte-exact) [ADR-0106]"
```

---

## Task 4: Differential test `access_log_rf_no_healthy.rs`

**Files:**
- Create: `tests/differential/tests/access_log_rf_no_healthy.rs` (a structural clone of `access_log_rcd_no_healthy.rs`, pointing at the `0057` fixture)

- [ ] **Step 1: Write the test wrapper**

```rust
//! Docker-gated differential test for fixture 0057-accesslog-rf-no-healthy.
//! Phase 49 (ADR-0106) — the SECOND non-`-` `%RESPONSE_FLAGS%` witness: `UH`
//! (NoHealthyUpstream), BYTE-EXACT cross-proxy on the no-healthy-upstream 503
//! path. A NO_FALLBACK `lb_subset_config` cluster (`subset_selectors:
//! [{ keys: [stage] }]`) with a single route whose `metadata_match` selects the
//! NON-EXISTENT `stage: nonexistent` subset (the fixture-0038 `/nope` pattern)
//! → `pick()->None` → the deterministic 503 `no healthy upstream` synth at
//! ROUTING time (the literal `127.0.0.1:1` endpoint is never dialed; no backend
//! spawns). envoy-rust now DERIVES `%RESPONSE_FLAGS%` = `UH` from
//! `%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream` at the H1 record-build site
//! (`hcm.rs:1232`; was the no-flags sentinel `"-"`); upstream Envoy v1.33 emits
//! the same flag here (state-0 recon: `{"rc":503,"rcd":"no_healthy_upstream",
//! "rf":"UH"}`). Spawns Envoy v1.33 in a container; spawns envoy-rust as a
//! subprocess; drives `kind: http1_access_log_byte_exact` (a `GET /` probe whose
//! file access-logger carries a `json_format` with `%RESPONSE_CODE%` /
//! `%RESPONSE_CODE_DETAILS%` / `%RESPONSE_FLAGS%` / `%REQ(:METHOD)%` /
//! `%PROTOCOL%`); reads each side's file access-log and asserts the emitted JSON
//! object is byte-identical:
//!   {"method":"GET","proto":"HTTP/1.1","rc":503,"rcd":"no_healthy_upstream","rf":"UH"}
//! PURE cross-proxy equality (no static literal — the no-healthy synth-503 is
//! deterministic on both sides). H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_no_healthy() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0057-accesslog-rf-no-healthy");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Compile-check the differential crate** (the test is Docker-gated; it does NOT run the container locally on a clean compile, but it MUST compile)

Run: `cargo test -p differential --no-run`
Expected: compiles clean (no new harness code; the `0057` fixture deserializes against the existing `Http1AccessLogByteExact` driver).

> **NOTE (host-environment):** the Docker differential (real Envoy vs envoy-rust) is **CI-authoritative** at the state-4 §7.5 gate (see memory `envoy-rust-state4-ci-first-execution`). This host may not run the container reliably; do NOT treat a local Docker-gated non-run as a failure. The differential `0057` green + all `0001`-`0056` still green are confirmed by the state-4 CI run.

- [ ] **Step 3: Commit**

```bash
git add tests/differential/tests/access_log_rf_no_healthy.rs
git commit -m "phase 49: differential test access_log_rf_no_healthy (fixture 0057) [ADR-0106]"
```

---

## Task 5: BEHAVIOR_CONTRACT — `%RESPONSE_FLAGS%` row update (add the `UH` per-flag rule)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1020` (the `%RESPONSE_FLAGS%` access-log-field-mapping row)

- [ ] **Step 1: Update the row** to record the second witnessed non-`-` flag. Keep the existing `:1225` site anchor (carry-forward **M49-1** — do NOT introduce a competing `:1232` citation for the same record-build block).

Replace exactly:

```
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`. Renders Envoy's no-flags sentinel `"-"` on every path EXCEPT the no-route 404 path, where it renders `NR` (NoRoute). **Per-flag equivalence — `NR`:** a config-deterministic single static constant (no combination, brace-free), set on BOTH H1 no-route `synth_404` arms (host-miss + route-miss), derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `route_not_found` at the H1 record-build site (`hcm.rs:1225`); the 404 status/body/headers/`%RESPONSE_CODE_DETAILS%` are unchanged. Other non-`-` flags (`UH`/`UF`/`UO`/`DC`/`URX`) remain unwitnessed (M45-2, non-deterministic surfaces) and still need their own per-flag rules. | value-exact (`-` no-flags case + `NR` no-route case) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. Phase 48 (ADR-0105) fixture **0056** witnesses `NR` byte-exact on BOTH the route-miss and host-miss 404 arms; both proxies emit `NR`. H2 no-route `%RESPONSE_FLAGS%` deferred (M45-1 — no H2 access-log differential driver). |
```

with:

```
| `%RESPONSE_FLAGS%` | `AccessLogRecord.response_flags: String`. Renders Envoy's no-flags sentinel `"-"` on every path EXCEPT two witnessed failure paths: the no-route 404 path renders `NR` (NoRoute), and the no-healthy-upstream 503 path renders `UH` (NoHealthyUpstream). **Per-flag equivalence — `NR`:** a config-deterministic single static constant (no combination, brace-free), set on BOTH H1 no-route `synth_404` arms (host-miss + route-miss), derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `route_not_found` at the H1 record-build site (`hcm.rs:1225`); the 404 status/body/headers/`%RESPONSE_CODE_DETAILS%` are unchanged. **Per-flag equivalence — `UH`:** likewise a config-deterministic single static constant (no combination, brace-free), set on the single H1 `pick()->None` no-healthy synth-503 arm, derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `no_healthy_upstream` at the same H1 record-build site (`hcm.rs:1225`); the 503 status/body/headers/`%RESPONSE_CODE_DETAILS%` are unchanged. Other non-`-` flags (`UF`/`UO`/`DC`/`URX`) remain unwitnessed (M45-2, non-deterministic surfaces) and still need their own per-flag rules. | value-exact (`-` no-flags case + `NR` no-route case + `UH` no-healthy case) | Fixture 0012's direct_response happy-path produces `-`; both proxies emit `-`. Phase 48 (ADR-0105) fixture **0056** witnesses `NR` byte-exact on BOTH the route-miss and host-miss 404 arms; both proxies emit `NR`. Phase 49 (ADR-0106) fixture **0057** witnesses `UH` byte-exact on the no-healthy-upstream 503 arm; both proxies emit `UH`. H2 no-route/no-healthy `%RESPONSE_FLAGS%` deferred (M45-1 — no H2 access-log differential driver). |
```

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 49: BEHAVIOR_CONTRACT %RESPONSE_FLAGS% row — second non-\"-\" flag UH witnessed (fixture 0057) [ADR-0106]"
```

---

## Task 6: Local verification sweep (state-3 close-out; full §7.5 gate runs at state-4)

This is the developer's local pre-flight — NOT the state-4 verification gate (that re-runs the full §7.5 set in CI and quotes outputs to `PROGRESS.md`). Run the cheap-and-local subset; the Docker differential + `0001`-`0056` byte-identical + h2spec + fuzz are CI-authoritative at state-4.

**Files:** none (verification only)

- [ ] **Step 1: clippy clean**

Run: `cargo clippy -p envoy-http1 -p differential --all-targets --all-features -- -D warnings`
Expected: no warnings (the three-arm `match` is idiomatic; no new lint surface).

- [ ] **Step 2: fmt clean**

Run: `cargo fmt --all -- --check`
Expected: clean. (If the inserted `match` block reflows, run `cargo fmt --all` and re-commit — see memory `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase.)

- [ ] **Step 3: full workspace unit tests** (non-Docker)

Run: `cargo test --workspace`
Expected: PASS (the new `rf:"UH"` backstop + all existing tests; the differential Docker tests are `#[ignore]`/Docker-gated and skip locally).

- [ ] **Step 4: confirm byte-preservation reasoning (no existing fixture regressed)**

Run: `grep -rln "RESPONSE_FLAGS" tests/fixtures/`
Expected: only `0012`, `0040`, `0046`, `0056` (+ the new `0057`). Re-confirm none of `0012`/`0040`/`0046`/`0056` drives a no-healthy-upstream 503 (`0012`/`0040`/`0046` happy-path 200 → `"-"`; `0056` no-route 404 → `"NR"`, set by the unchanged `route_not_found` arm) → `0001`-`0056` byte-identical holds.

- [ ] **Step 5: final fmt-fix commit if needed** (otherwise nothing to commit)

```bash
cargo fmt --all
git add -A && git commit -m "phase 49: cargo fmt [ADR-0106]" || echo "nothing to reformat"
```

---

## Scope / gate summary

- **Task count:** 6 tasks (~30-90 LoC: one ~6-line `match` derive at `hcm.rs:1232` + one ~75-line backstop test + a 4-file fixture + a ~30-line differential test + a 1-row contract edit). **§6.1 split does NOT fire** (well under ~25 tasks / ~1500 LoC). **ADR-0107 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the lapsed-reservation convention).
- **No new** `Op` / `AccessLogRecord` field / variable / `BuildOutcome::Synth` enum field / crate / dependency / fuzz-target / `ConfigError` variant. `#![forbid(unsafe_code)]` holds.
- **Additive invariant:** all `0001`-`0056` fixtures stay byte-identical (§6.2 finding 3). Only the no-healthy path's previously-`"-"` flag changes — and no existing fixture both hits a no-healthy-upstream 503 AND logs `%RESPONSE_FLAGS%`. The `route_not_found => "NR"` arm is preserved verbatim → `0056` is specifically untouched.
- **Acceptance (re-run at state-4, SPEC §5):** (a) `0057` green (cross-proxy-equal `rf:"UH"` on the no-healthy 503) + (b) all `0001`-`0056` green simultaneously + (c) h2spec ≥95% (no H2 change) + (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean (no new target) + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved.
- **Carry-forwards:** **M48-1** (line-citation drift) ACTIONED in this PLAN (re-verified the derive at `:1232`, record-build at `:1219`/`:1225`, no-healthy RCD set-site at `:1000-1001`); **M49-1** (BEHAVIOR_CONTRACT `:1225` site anchor) ACTIONED in Task 5 (kept the `:1225` anchor); **M45-2** (the `UH` slice) CONSUMED (UH moves from unwitnessed → witnessed); M42-1 CONTINUED (the `%RESPONSE_FLAGS%` vocabulary keeps expanding); M48-2 + M45-1 (H2 no-route/no-healthy flag) + the remaining M45-2 flags (UF/UO/DC/URX) remain deferred.

_The state-3 implementation (`superpowers:executing-plans` or `superpowers:subagent-driven-development`) is the session AFTER this PLAN lands. Per §5.1, one state per session: this session writes the PLAN only._
