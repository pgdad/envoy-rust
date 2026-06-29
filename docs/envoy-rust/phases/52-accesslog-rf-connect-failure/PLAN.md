# Phase 52 — `52-accesslog-rf-connect-failure` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Every implementation task uses superpowers:test-driven-development — write the failing test FIRST. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Differentially witness the FIFTH non-`-` `%RESPONSE_FLAGS%` value `UF` (UpstreamConnectionFailure) byte-exact on the upstream-connect-refused 503 path, AND correct envoy-rust's connect-failure synth status from `502`→`503` to match upstream Envoy.

**Architecture:** Three surgical edits in `crates/envoy-http1/src/hcm.rs` — (1) flip `synth_status(502, close)`→`synth_status(503, close)` at the three `AcquireOutcome::ConnectFailure` arms; (2) thread a per-request boolean `connect_failure_for_log`, set post-loop from a loop-scoped `final_outcome: Option<AttemptOutcome>` capture (because the retry-loop `break` carries no outcome) when the FINAL attempt was `AttemptOutcome::ConnectFailure`; (3) add an `else if connect_failure_for_log { "UF" }` branch to the `%RESPONSE_FLAGS%` derive. Plus a new differential fixture `0060` (the 0058 dead-endpoint pattern minus `circuit_breakers`, so envoy-rust DIALS the endpoint and the kernel refuses), an in-process backstop, the affected-unit-test/comment/warn sweep, and the BEHAVIOR_CONTRACT update.

**Tech Stack:** Rust, `crates/envoy-http1` (HCM), `crates/envoy-config` (`AttemptOutcome`), `crates/envoy-accesslog` (FileSink/json_format), the `tests/differential` `http1_access_log_byte_exact` driver.

**§6.2 recon disposition (state-2, this session):** the §6.2 empirical recon CONFIRMED every §A–§G SPEC fact against the live tree (anchors below are live line numbers). **No §A–§G fact was overturned → the conditional ADR-0111 does NOT fire.** Ledger head stays **ADR-0109**.

**§6.1 split:** NOT triggered. This plan is **8 tasks / ~200 LoC** net change — well under the ~25-task / ~1500-LoC gate. **ADR-0110 stays reserved-but-unfired** (reclaimed by the next NEW phase pick — the lapsed-reservation convention).

---

## Recon-confirmed anchors (live line numbers, `crates/envoy-http1/src/hcm.rs`)

| Surface | Line(s) | Confirmed shape |
|---|---|---|
| ConnectFailure synth arm — pool `PoolError::Connect` | `:501` | `AcquireOutcome::ConnectFailure(synth_status(502, close))` |
| ConnectFailure synth arm — pool-`None` one-shot | `:530` | `AcquireOutcome::ConnectFailure(synth_status(502, close))` |
| ConnectFailure synth arm — no-pool one-shot | `:547` | `AcquireOutcome::ConnectFailure(synth_status(502, close))` |
| Connect-failure runtime `tracing::warn!` "returning 502" | `:499`, `:528`, `:545` | three warn strings on the connect arms |
| Reset/send-fail arm (**NOT touched** — different path → `UC`, M52-1) | `:615` warn, `:618` synth | `synth_status(502, close)` + `outcome: Some(AttemptOutcome::Reset)` |
| `retry_limit_exceeded_for_log` local decl | `:854` | `let mut retry_limit_exceeded_for_log = false;` |
| `final_retriable` loop-scoped decl / per-iter set | `:974` / `:1067` | the capture pattern `connect_failure`'s `final_outcome` mirrors |
| Retry loop `break` (carries only `(response, upstream_response)`) | `:1112` | `break (attempt.response, attempt.upstream_response);` |
| `upstream_rq_5xx` gate (needs `completing_upstream_response`) | `:1124` | `if completing_upstream_response && final_response.status / 100 == 5` |
| Post-loop retry-split reconciliation region | `:1136`–`:1156` | `if attempts > 1 && !retry_budget_blocked { … }` |
| `%RESPONSE_FLAGS%` derive | `:1305` | `if retry_limit_exceeded_for_log { "URX" } else { match rcd … }` |
| `AttemptOutcome` enum (`Copy`) | `envoy-config/src/bootstrap.rs:1902`–`1910` | `#[derive(… Copy …)]`; variants `Response`/`ConnectFailure`/`Reset` |
| Affected 502-asserting connect-failure unit tests | `:3272`, `:6758`, `:6800` | `s.starts_with("HTTP/1.1 502 Bad Gateway\r\n")` |
| Stale connect-failure 502 comments | `:484`, `:1120`–`:1121`, `:1767`, `:4009` + `router.rs:63` | comment text only |
| §F backstop template (URX in-process test) | `:7234`–`:7311` | `h1_retry_limit_exceeded_access_log_carries_urx_flag` |
| `response.rs` reason phrases | `response.rs:88`–`89` | `502 => "Bad Gateway"`, `503 => "Service Unavailable"` |

> **Line drift:** all line numbers are the state-2 live values; treat them as ±a-few — re-grep the named token (`synth_status(502`, `final_retriable`, `response_flags:`, the test names) at execution rather than trusting the absolute line.

---

## Task 1: Correct the connect-failure synth status 502→503 (the three arms + the three runtime warns + the three affected unit tests)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (arms `:501`/`:530`/`:547`; warns `:499`/`:528`/`:545`; tests `:3272`/`:6758`/`:6800`)
- Test: same file (in-module unit tests)

This is a pure status correction (no flag work yet). The three existing connect-failure tests are the fail-first harness: flip their assertions to expect 503, watch them fail, then land the synth change.

- [ ] **Step 1: Flip the three affected unit-test assertions to expect 503 (the failing tests)**

In `route_walk_returns_upstream_connect_on_refused_port` (~`:3272`), change the assertion + message:
```rust
        assert!(
            s.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "expected 503 on UpstreamConnect, got: {s}"
        );
```
And its comment (~`:3255`): `// Route arm should propagate the connect failure as a 503 Service Unavailable`.

In `connect_failure_retried_on_connect_failure_policy` (~`:6758`):
```rust
        assert!(
            s.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "downstream must be synth-503 after exhausting connect-failure retries: {s}"
        );
```
And its doc comment (~`:6734`): `… Asserts: downstream synth-503, …`.

In `connect_failure_synth_does_not_tick_upstream_rq_5xx` (~`:6800`):
```rust
        assert!(
            s.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
            "downstream must be connect-failure synth-503: {s}"
        );
```
And the `rq_5xx 0` assertion message (~`:6806`) → replace `synth-502` with `synth-503` (the assertion VALUE stays `0` — a connect-failure synth has no real upstream response, so it does not tick `upstream_rq_5xx` regardless of status). And the doc comments (~`:6783`/`:6786`) → `synth-503`.

- [ ] **Step 2: Run the three tests — verify they FAIL**

Run: `cargo test -p envoy-http1 route_walk_returns_upstream_connect_on_refused_port connect_failure_retried_on_connect_failure_policy connect_failure_synth_does_not_tick_upstream_rq_5xx -- --nocapture`
Expected: all three FAIL — actual response line is `HTTP/1.1 502 Bad Gateway` (the synth is still 502).

- [ ] **Step 3: Change the synth status 502→503 on the three connect-failure arms + the three warns**

At `:501`, `:530`, `:547`: `AcquireOutcome::ConnectFailure(synth_status(503, close))`.
At the runtime `tracing::warn!` strings `:499`, `:528`, `:545`: change `"… — returning 502"` → `"… — returning 503"` (e.g. `"upstream connect failed (pool) — returning 503"`).
**Do NOT touch** the reset/send-fail arm at `:615` (`"upstream request failed — returning 502"`) / `:618` (`response: synth_status(502, close)`) — that is the `AttemptOutcome::Reset` path (different flag `UC`, deferred M52-1).

- [ ] **Step 4: Run the three tests — verify they PASS**

Run: `cargo test -p envoy-http1 route_walk_returns_upstream_connect_on_refused_port connect_failure_retried_on_connect_failure_policy connect_failure_synth_does_not_tick_upstream_rq_5xx -- --nocapture`
Expected: all three PASS (synth is now `HTTP/1.1 503 Service Unavailable`).

- [ ] **Step 5: Run the whole envoy-http1 unit suite — confirm no collateral**

Run: `cargo test -p envoy-http1`
Expected: PASS. (A repo-wide grep confirms ONLY these three tests assert a connect-failure 502 — `:3272`/`:6758`/`:6800`. The reset/send-fail arm at `:618` is untouched and has no status-asserting test of its own, so nothing else flips.)

- [ ] **Step 6: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 52 task 1: connect-failure synth status 502->503 (3 arms + warns + 3 tests)"
```

---

## Task 2: Thread `connect_failure_for_log` + render `UF` + the §F in-process backstop

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (decl ~`:854`; loop-scoped `final_outcome` ~`:974`; per-iter capture ~`:1067`; post-loop set ~`:1156`; derive `:1305`; derive comment block ~`:1280`–`:1304`)
- Test: same file — new in-process backstop test `h1_connect_failure_access_log_carries_uf_flag`

The backstop test is the fail-first harness for the discriminator + the derive branch. It mirrors the URX backstop (`:7234`) verbatim in shape, swapping the always-503 retry topology for a single kernel-refused connect (`127.0.0.1:1`, NO retry_policy) and the `{rc,rcd,rf}` log for `{rc,rf}` (rcd OMITTED — the connect-failure rcd is the shared `via_upstream`; the fixture omits it to mirror `0060`).

- [ ] **Step 1: Write the failing backstop test**

Add to the `#[cfg(test)] mod tests` block in `crates/envoy-http1/src/hcm.rs`, adjacent to the connect-failure tests (use the URX test `:7234` as the structural template; `cluster_mgr_with_endpoint("backend", 1)` is the kernel-refused endpoint already used by the `:3257`/`:6741`/`:6793` tests):
```rust
    /// phase 52 (ADR-0109): a single connect-failure attempt (endpoint
    /// 127.0.0.1:1, kernel-refused) with NO retry_policy, wired to a {rc,rf}
    /// FILE json access-log. Asserts the downstream response is the synth-503
    /// (Task 1) AND the logged line carries the DERIVED rf:"UF" (set post-loop
    /// from the connect-failure final-outcome boolean, NOT rcd-derived — the
    /// connect-failure rcd is the shared "via_upstream"). The sole in-process
    /// proof of §A's discriminator + §B's derive branch. Fail-first: pre-change
    /// the derive's rcd-match falls to `_ => "-"` (via_upstream unmatched) → it
    /// renders `"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_connect_failure_access_log_carries_uf_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // 127.0.0.1:1 is kernel-refused — a deterministic connect failure.
        let cluster_mgr = cluster_mgr_with_endpoint("backend", 1).await;
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "rc".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE%".to_string()),
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
            "connect-failure surfaces the synth-503 downstream: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rf\":\"UF\"}\n",
            "connect-failure access-log line carries rf:UF: {logged:?}"
        );
    }
```
> **PLAN-VERIFY (execution):** confirm the `HCMConfig` struct-literal field set + the helper names (`tempdir`, `mk_stats`, `test_router_only_pipeline`, `cluster_mgr_with_endpoint`, `drive`, `StdDuration`, the `RouteConfiguration`/`VirtualHost`/`Route` shapes) against the live URX test (`:7234`) at edit time — copy that test's exact field set verbatim and change only the cluster endpoint (port 1), `retry_policy: None`, the `{rc,rf}` map (drop the `rcd` insert), and the assertions. If `HCMConfig` has gained/lost a field since, mirror the URX test, not this snippet.

- [ ] **Step 2: Run the backstop — verify it FAILS on `rf`**

Run: `cargo test -p envoy-http1 h1_connect_failure_access_log_carries_uf_flag -- --nocapture`
Expected: FAIL at the final `assert_eq!` — the logged line is `{"rc":503,"rf":"-"}` (status 503 from Task 1, but the derive has no `UF` branch yet so `via_upstream` falls to `_ => "-"`).

- [ ] **Step 3: Declare the discriminator boolean**

After `let mut retry_limit_exceeded_for_log = false;` (~`:854`):
```rust
        // phase 52 (ADR-0109): per-request %RESPONSE_FLAGS% = "UF"
        // (UpstreamConnectionFailure) discriminator. Set true POST-LOOP when the
        // FINAL attempt's outcome was AttemptOutcome::ConnectFailure (a connect-
        // failure RETRIED to success must NOT flag UF — so this is the final
        // outcome, not a per-attempt set). Like URX, UF is NOT 1:1 with a unique
        // %RESPONSE_CODE_DETAILS% (the connect-failure rcd is the shared
        // "via_upstream"), so it keys on this boolean, not on the rcd.
        let mut connect_failure_for_log = false;
```

- [ ] **Step 4: Add the loop-scoped `final_outcome` capture**

After `let mut final_retriable = false;` (~`:974`), mirroring its `#[allow(unused_assignments)]`:
```rust
        // phase 52 (ADR-0109): the FINAL attempt's outcome. Captured each
        // iteration because the loop `break` carries only
        // (response, upstream_response), NOT attempt.outcome. Read post-loop to
        // set connect_failure_for_log. AttemptOutcome is Copy (no move/borrow
        // interaction with the per-iter `match attempt.outcome`).
        #[allow(unused_assignments)]
        let mut final_outcome: Option<AttemptOutcome> = None;
```
Inside the loop, adjacent to the existing `final_retriable = match attempt.outcome { … };` (~`:1067`), add the capture (place it immediately before that `match` so both read `attempt.outcome` together):
```rust
                            final_outcome = attempt.outcome;
```
> `AttemptOutcome` is already imported at `hcm.rs:12` — no new `use`.

- [ ] **Step 5: Set the boolean post-loop**

After the retry-split block `if attempts > 1 && !retry_budget_blocked { … }` (~`:1156`, before `drop(retry_guard_slot);` `:1160`):
```rust
                        // phase 52 (ADR-0109): flag UF when the FINAL attempt was a
                        // connect failure — independent of the retry split (a single
                        // connect-failure attempt with no retry_policy flags it too).
                        // A connect-failure retried to success has final_outcome =
                        // Some(Response) → not flagged. If BOTH this and
                        // retry_limit_exceeded_for_log are set (a retry-exhausted-
                        // connect-failure — un-recon'd combination, §4), the derive's
                        // URX-before-UF ordering renders URX deterministically.
                        connect_failure_for_log =
                            matches!(final_outcome, Some(AttemptOutcome::ConnectFailure));
```

- [ ] **Step 6: Add the `UF` branch to the `%RESPONSE_FLAGS%` derive**

At `:1305`, insert the `else if` between the `URX` branch and the rcd-`match`:
```rust
                response_flags: if retry_limit_exceeded_for_log {
                    "URX"
                } else if connect_failure_for_log {
                    "UF"
                } else {
                    match response_code_details_for_log.as_deref() {
                        Some("route_not_found") => "NR",
                        Some("no_healthy_upstream") => "UH",
                        Some("upstream_reset_before_response_started{overflow}") => "UO",
                        _ => "-",
                    }
                }
                .to_owned(),
```
Extend the derive's explanatory comment block (~`:1280`–`:1304`) with one sentence noting the new `connect_failure_for_log => "UF"` branch keys on the connect-failure final-outcome boolean (rcd = the shared `via_upstream`, which would otherwise fall to `_ => "-"`), mirroring the URX wording.

- [ ] **Step 7: Run the backstop — verify it PASSES**

Run: `cargo test -p envoy-http1 h1_connect_failure_access_log_carries_uf_flag -- --nocapture`
Expected: PASS — logged line is `{"rc":503,"rf":"UF"}`.

- [ ] **Step 8: Run the whole envoy-http1 suite — confirm no collateral on the URX/UO/UH/NR backstops**

Run: `cargo test -p envoy-http1`
Expected: PASS — in particular the URX/UO/UH/NR in-process access-log backstops (`:5532`/`:5663`/`:5929`/`:6012`/`:7219`/`:7309`) are unchanged (their paths never set `connect_failure_for_log`).

- [ ] **Step 9: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 52 task 2: thread connect_failure_for_log + render UF + in-process backstop"
```

---

## Task 3: New differential fixture `0060-accesslog-rf-connect-failure`

**Files:**
- Create: `tests/fixtures/0060-accesslog-rf-connect-failure/envoy.yaml`
- Create: `tests/fixtures/0060-accesslog-rf-connect-failure/envoy-rust.yaml`
- Create: `tests/fixtures/0060-accesslog-rf-connect-failure/expectations.yaml`
- Create: `tests/fixtures/0060-accesslog-rf-connect-failure/README.md`

The 0058 pattern **minus `circuit_breakers`** (so envoy-rust DIALS the dead endpoint and the kernel refuses → the connect-failure synth-503, instead of 0058's pre-connect pending-gate reject) and the json_format reduced to `{rc, rf}` (the connect-failure rcd is non-deterministic — OMITTED). No backend spawned (literal `127.0.0.1:1`, no `{{BACKEND_*}}` marker).

- [ ] **Step 1: Write `envoy-rust.yaml`**

```yaml
node: { id: envoy-rust-phase-52-fixture-0060, cluster: envoy-rust-phase-52 }
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
                      path: /tmp/0060-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rf: "%RESPONSE_FLAGS%"
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: connect_failure_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend_cluster }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # STATIC cluster with NO circuit_breakers and NO retry_policy. The single
    # endpoint is the LITERAL unreachable 127.0.0.1:1 — DIALED on the first
    # request (no pending-gate to reject pre-connect, UNLIKE 0058): the kernel
    # refuses the connect → the connect-failure synth-503 (rf:"UF"). A literal
    # address (not a {{BACKEND_*}} marker) keeps the cluster byte-identical
    # across both files with NO backend spawned (the 0057/0058 discipline).
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
```

- [ ] **Step 2: Write `envoy.yaml`**

Byte-identical to `envoy-rust.yaml` EXCEPT the three documented per-side deltas (the 0058 discipline): prepend the `admin:` line, bind `0.0.0.0`, and the envoy-side log path:
```yaml
node: { id: envoy-rust-phase-52-fixture-0060, cluster: envoy-rust-phase-52 }
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
                      path: /tmp/0060-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rf: "%RESPONSE_FLAGS%"
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: connect_failure_vh
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
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
```

- [ ] **Step 3: Write `expectations.yaml`**

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0060-envoy-mount/access.log
    envoy_rust: /tmp/0060-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `backend_cluster`, a STATIC cluster with NO
    # circuit_breakers and NO retry_policy and one LITERAL dead endpoint
    # 127.0.0.1:1. Both proxies DIAL it; the kernel refuses → the connect-failure
    # synth-503. FIFTH non-`-` %RESPONSE_FLAGS% witness: UF
    # (UpstreamConnectionFailure) (phase 52, ADR-0109).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). The fixture logs
    # ONLY {rc, rf} — the connect-failure %RESPONSE_CODE_DETAILS% AND the
    # response body carry the OS-derived transport-failure reason
    # (non-deterministic across environments — M45-2), so rcd is OMITTED and the
    # driver does not compare the body. envoy-rust returns 503 (Task 1 corrected
    # the unvalidated 502) and DERIVES %RESPONSE_FLAGS% = UF from the
    # connect-failure final-outcome boolean (NOT from rcd).
    # state-0 recon (live v1.33.0, digest sha256:56da5afd…, byte-stable across 8
    # repeats + a restart): status 503, rf "UF".
    #   rc: "%RESPONSE_CODE%"   → 503  (json NUMBER)
    #   rf: "%RESPONSE_FLAGS%"  → "UF"
    # Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rf. Compact separators +
    # ONE trailing `\n` (ADR-0092 §E). Emitted line:
    #   {"rc":503,"rf":"UF"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Write `README.md`**

Clone `tests/fixtures/0058-accesslog-rf-overflow/README.md`'s structure, retitling to fixture 0060 / phase 52 / ADR-0109 / `UF`. Cover: (1) what it proves (`UF` byte-exact on the connect-refused 503 path); (2) the json_format `{rc, rf}` table (rcd OMITTED — non-deterministic; the body not compared); (3) the trigger — the dead `127.0.0.1:1` endpoint is **DIALED and kernel-refused** (the key contrast with 0058, whose pending-gate rejects pre-connect — note the removed `circuit_breakers`); (4) the per-side divergences table (bind, admin, log path); (5) driver `http1_access_log_byte_exact`, no new harness code, no backend spawn; (6) cross-references ADR-0109, related fixtures 0058/0059/0057, deferred surfaces (the reset `UC` M52-1, the connect-failure rcd/body M45-2, H2 M45-1).

- [ ] **Step 5: Confirm the YAML pair diff matches the 0058 discipline**

Run: `diff tests/fixtures/0060-accesslog-rf-connect-failure/envoy.yaml tests/fixtures/0060-accesslog-rf-connect-failure/envoy-rust.yaml`
Expected: exactly three hunks — the `admin:` line (envoy-only), `0.0.0.0` vs `127.0.0.1` listener bind, and `0060-envoy-mount` vs `0060-envoy-rust-mount` log path. Nothing else (the cluster/route/json_format are byte-identical).

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/0060-accesslog-rf-connect-failure/
git commit -m "phase 52 task 3: fixture 0060 (connect-failure UF, 0058 pattern minus circuit_breakers)"
```

---

## Task 4: New differential test `access_log_rf_connect_failure.rs`

**Files:**
- Create: `tests/differential/tests/access_log_rf_connect_failure.rs`

A structural clone of `tests/differential/tests/access_log_rf_overflow.rs` pointed at `0060`. Each file under `tests/differential/tests/` is its own auto-discovered integration-test binary — NO registry edit. Per SPEC §D the dead-endpoint pattern needs NO `needs_health_aware_backend` allowlist entry and NO `--per-path` map arm (no backend, no shared-IP machinery).

- [ ] **Step 1: Write the test file**

```rust
//! Docker-gated differential test for fixture 0060-accesslog-rf-connect-failure.
//! Phase 52 (ADR-0109) — the FIFTH non-`-` `%RESPONSE_FLAGS%` witness: `UF`
//! (UpstreamConnectionFailure), BYTE-EXACT cross-proxy on the upstream-connect-
//! refused 503 path. A STATIC cluster with NO circuit_breakers and NO
//! retry_policy and a single dead endpoint (`127.0.0.1:1`): both proxies DIAL
//! it, the kernel refuses the connect → the connect-failure synth-503.
//! envoy-rust now (a) returns 503 (Task 1 corrected the unvalidated 502) and
//! (b) DERIVES `%RESPONSE_FLAGS%` = `UF` from the connect-failure final-outcome
//! boolean (NOT from `%RESPONSE_CODE_DETAILS%`, which — like the response body —
//! carries the non-deterministic OS transport-failure reason and is NOT logged
//! / NOT compared). Upstream Envoy v1.33 emits status 503 + `rf:"UF"` here
//! (state-0 recon: byte-stable across 8 repeats + a container restart). Drives
//! `kind: http1_access_log_byte_exact` (a `GET /` probe, `expected_status: 503`,
//! json_format {rc, rf}); asserts the emitted line `{"rc":503,"rf":"UF"}` is
//! byte-identical. The driver asserts status + the access-log line but NOT the
//! response body. H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_connect_failure() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0060-accesslog-rf-connect-failure");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Confirm it compiles (the differential crate builds)**

Run: `cargo build -p differential --tests`
Expected: clean build (the new test binary compiles).
> **NOTE (host limitation):** the Docker differential itself (real Envoy vs envoy-rust) is **CI-authoritative** — this dev host's Docker bridge routing flakes the differential fixtures (memory `differential-host-bridge-ip-192-168-65-2`, `differential-fixtures-flake-under-parallel-load`). Do NOT treat a local RED `access_log_rf_connect_failure` as a regression; the state-4 verification gate runs it on CI. Compile-check locally; defer the green assertion to CI (per memory `envoy-rust-state4-ci-first-execution`).

- [ ] **Step 3: Rebuild the debug envoy-bin (the differential runs `target/debug/envoy-bin`)**

Run: `cargo build -p envoy-bin`
Expected: clean build — so that if the differential IS exercised (CI or a working-Docker host) it picks up the 503/`UF` change rather than a stale binary (memory `differential-harness-uses-debug-envoy-bin`).

- [ ] **Step 4: Commit**

```bash
git add tests/differential/tests/access_log_rf_connect_failure.rs
git commit -m "phase 52 task 4: differential test access_log_rf_connect_failure (fixture 0060)"
```

---

## Task 5: BEHAVIOR_CONTRACT updates (§E)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md`

Three updates per SPEC §E, plus the stale-502 connect-failure status sweep the correction necessitates (D-3.3 — the contract is canonical).

- [ ] **Step 1: Extend the `%RESPONSE_FLAGS%` row (line ~1020) with the `UF` rule + the M51-1 anchor reconciliation**

Add a `**Per-flag equivalence — `UF`:**` clause mirroring the `URX` clause: a config-deterministic single static constant (no combination, brace-free), **NOT derived from `%RESPONSE_CODE_DETAILS%`** (the connect-failure rcd is the shared `via_upstream` — the SECOND flag not 1:1 with a unique rcd), instead derived from the `connect_failure_for_log` boolean set post-loop when the FINAL attempt's `AttemptOutcome` is `ConnectFailure`; the connect-failure response is the synth-**503** (corrected from an unvalidated 502 to match Envoy), and the `%RESPONSE_CODE_DETAILS%` + response body carry the non-deterministic OS transport-failure reason and are NOT witnessed. Update the trailing `value-exact (… + URX …)` enumeration to add the `UF` connect-failure case, and add the witnessing-fixture sentence (`Phase 52 (ADR-0109) fixture 0060 witnesses UF byte-exact …`). Move `UF` from the "Other non-`-` flags (`UF`/`DC`) remain unwitnessed (M45-2)" set to witnessed (leaving `DC`). **Fold M51-1:** in this same row, change the in-text H1 record-build-site anchor from `hcm.rs:1225` → `hcm.rs:1305` for the NR/UH/UO/URX/UF rules (the live `response_flags:` derive site). **Replace ALL `:1225` occurrences in this single long row** — it appears ~4 times (the NR, UH, and URX clauses each cite it explicitly, plus once as "the H1 record-build site" for UO); a zero-context executor must sweep every one, not just the first. Confirm with `grep -c 'hcm.rs:1225' docs/envoy-rust/BEHAVIOR_CONTRACT.md` before and after (should drop to 0 in this row).

- [ ] **Step 2: Correct the stale connect-failure 502 mentions to 503**

Sweep `BEHAVIOR_CONTRACT.md` for connect-failure status mentions and correct the **connect-failure** ones (NOT the reset/send-fail ones, which stay 502 pending M52-1):
- Line ~36: `The connect-fail 502 + send-fail 502 paths keep …` → `The connect-fail 503 + send-fail 502 paths keep …`.
- Line ~289 (`downstream_rq_5xx`): `proxy synth-502/503 (no-endpoint, connect-fail, send-fail)` → make explicit that connect-fail is now 503 (e.g. `proxy synth-503 (no-endpoint, connect-fail, overflow) / synth-502 (send-fail)`), keeping the row's "symmetric on `response_status_for_log`" semantics intact (the counter fires on any 5xx — both 502 and 503 are 5xx, so the COUNT is unaffected; only the prose status label changes).
- Line ~295 (`upstream_rq_total`): `Synth-502 paths (envoy-rust-side 502 on connect-fail) do NOT increment` → `Synth-503 paths (envoy-rust-side 503 on connect-fail) do NOT increment`.
- Line ~387 (per-attempt reconciliation): `the no-healthy-upstream synth-503, connect-failure synth-502, reset synth-502, and overflow synth-503 paths` → `… connect-failure synth-503, reset synth-502, …`.

> Re-grep `grep -nE 'connect.?fail.{0,20}502|502.{0,20}connect' docs/envoy-rust/BEHAVIOR_CONTRACT.md` at execution to catch any mention this list missed; correct each connect-failure one to 503, leave reset/send-fail at 502.

- [ ] **Step 3: Confirm no other contract row contradicts the connect-failure 503**

Run: `grep -nE 'connect.?fail|502' docs/envoy-rust/BEHAVIOR_CONTRACT.md`
Expected: every remaining `502` mention is the reset/send-fail path or an unrelated row; no connect-failure row still says 502.

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 52 task 5: BEHAVIOR_CONTRACT — UF rule + connect-failure 503 + M51-1 anchor"
```

---

## Task 6: Stale-502 comment/doc sweep (§G — the non-edit-site comments)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (comments `:484`, `:1120`–`:1121`, `:1767`, `:4009`)
- Modify: `crates/envoy-http1/src/router.rs` (comment `:63`)

Pure comment/doc edits — no behavior. (The edit-site comments at the three arms and the derive, plus the test comments/messages, were already updated in Tasks 1–2.)

- [ ] **Step 1: Update the connect-failure comments to 503**

- `hcm.rs:484`: `// Connect failed → synth-503, AttemptOutcome::ConnectFailure.`
- `hcm.rs:1120`–`1121`: in the `upstream_rq_5xx` comment, change `connect-failure synth-502` → `connect-failure synth-503` (KEEP `reset synth-502` unchanged on the same line).
- `hcm.rs:1767`: the `synth_status` doc — `the connect-fail 503` (was `the connect-fail 502`).
- `hcm.rs:4009`: the test-module doc — `connect-fail-503` (KEEP `send-fail-502` unchanged): `… (no-endpoint-503, connect-fail-503, send-fail-502, …)`.
- `router.rs:63`: `… the connect-fail synth-503.` (was `synth-502`).

- [ ] **Step 2: Confirm the only remaining `502` references are the reset/send-fail arm**

Run: `grep -nE '502|Bad Gateway' crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/router.rs`
Expected: the surviving `502` references are ONLY the reset/send-fail arm (`hcm.rs:615` warn + `:618` synth + any reset-arm test) and `router.rs:25` (the `RouterError::Connect` Display string — an error message, not a status; leave it). NO connect-failure `502` remains.

- [ ] **Step 3: Build + clippy the crate (comment edits are free, but confirm no doctest/format breakage)**

Run: `cargo build -p envoy-http1 && cargo clippy -p envoy-http1 --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/envoy-http1/src/hcm.rs crates/envoy-http1/src/router.rs
git commit -m "phase 52 task 6: sweep stale connect-failure synth-502 comments -> 503"
```

---

## Task 7: Local verification pass (the non-Docker subset of the §7.5 gate)

**Files:** none (verification only)

The full §7.5 gate (incl. the Docker differential `0060` + the h2spec/fuzz suites) runs at the state-4 verification session on CI. This task is the local pre-flight — the deterministic, non-Docker checks — so state-3 lands green-where-runnable.

- [ ] **Step 1: Workspace build (all targets)**

Run: `cargo build --workspace --all-targets`
Expected: clean.

- [ ] **Step 2: Clippy (all targets, all features, deny warnings)**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean. (Watch for an `unused_assignments` on `final_outcome` — if it fires, confirm the `#[allow(unused_assignments)]` from Task 2 step 4 is present, mirroring `final_retriable`.)

- [ ] **Step 3: fmt check**

Run: `cargo fmt --all -- --check`
Expected: clean. (Per memory `envoy-rust-state4-ci-first-execution`, fmt is the usual first CI red mid-phase — fix any drift now: `cargo fmt --all`.)

- [ ] **Step 4: Full workspace test (the unit + in-process suite; Docker differentials are `#[ignore]`/gated off without `DIFFERENTIAL=1`)**

Run: `cargo test --workspace`
Expected: PASS, including the four flag backstops (NR/UH/UO/URX) + the new `h1_connect_failure_access_log_carries_uf_flag` + the three flipped connect-failure status tests.
> If a Docker differential fixture is attempted and flakes locally, that is the known host limitation (memory `differential-fixtures-flake-under-parallel-load`) — re-confirm on CI, do not treat as a regression.

- [ ] **Step 5: cargo deny**

Run: `cargo deny check`
Expected: clean. (If a fresh RustSec advisory reds an existing dep — NOT a phase regression — patch-bump it per memory `cargo-deny-reds-on-unrelated-advisory`.)

- [ ] **Step 6: No commit (verification only)**

This task produces no diff. If any step required a fix (fmt, a dep bump), commit that fix with a descriptive message and re-run the failing step.

---

## Task 8: PROGRESS.md + handoff to state-4

**Files:**
- Create: `docs/envoy-rust/phases/52-accesslog-rf-connect-failure/PROGRESS.md`

> **NOTE:** Tasks 1–7 are the STATE-3 implementation session(s) — NOT this state-2 PLAN-write session (§5.1: one state per session). This task closes the state-3 arc by recording the running log; the state-4 verification (the full §7.5 gate, quoting all command outputs into PROGRESS.md) is the session AFTER.

- [ ] **Step 1: Write PROGRESS.md**

Record, per task: what landed, the exact files touched, and the local command outputs (Tasks 1/2/7). Note explicitly that the Docker differential `0060` (Task 3/4) is deferred to the state-4 CI gate (host limitation), and that ADR-0111 did NOT fire (the §6.2 recon confirmed all §A–§G facts).

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/phases/52-accesslog-rf-connect-failure/PROGRESS.md
git commit -m "phase 52: PROGRESS.md — state-3 implementation log"
```

---

## Out of scope (deferred — do NOT implement)

- **The reset/send-fail arm (`hcm.rs:615`/`:618`)** — `AttemptOutcome::Reset`, a different post-connect path → Envoy's `UC` flag, un-recon'd trigger; stays `synth_status(502, close)`. NEW carry-forward **M52-1** (the `UC` flag + the reset 502→503 status — re-recon before witnessing).
- **The connect-failure `%RESPONSE_CODE_DETAILS%` + the response body** — both carry the non-deterministic OS transport-failure reason (M45-2); NOT logged in `0060`, NOT compared by the driver.
- **Retry-exhausted-connect-failure (`UF`+`URX` combination)** — `0060` uses NO retry_policy → a single clean `UF`; the un-recon'd combination is out of fixture scope (the `URX`-before-`UF` derive ordering renders it deterministically if both ever set).
- **H2 connect-failure `%RESPONSE_FLAGS%`** — the H2 record-build site hard-codes `"-"`; no H2 access-log differential driver (M45-1).
- **`DC` flag + the retry-budget-overflow slice** of M45-2 — stay deferred.
- **Fuzz:** `%RESPONSE_FLAGS%` is an existing operator; no new operator/grammar → NO new fuzz target, `ci.yml` unchanged.

## Acceptance (§7.5, re-run at state-4 on CI)

(a) fixture `0060` green (cross-proxy status `503` + whole-line `{"rc":503,"rf":"UF"}`) — **CI-authoritative**; (b) `0001`–`0059` all green simultaneously (additive — `connect_failure_for_log` is false on every existing fixture; the 502→503 change touches no existing GREEN fixture, since envoy-rust's connect-failure 502 was never differentially validated); (c) h2spec ≥95% (no HTTP/2 change); (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean (no new target); (e) build/clippy/fmt/test/deny clean (Tasks 1/2/6/7); (f) `REVIEW.md` approved. `#![forbid(unsafe_code)]` holds; NO new crate / dependency / fuzz-target / `Op` / `AccessLogRecord` field / `ConfigError` variant.
