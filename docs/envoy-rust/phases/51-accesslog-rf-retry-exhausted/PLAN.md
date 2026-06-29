# Phase 51 — `51-accesslog-rf-retry-exhausted` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Every task is `superpowers:test-driven-development` (RED → GREEN → commit). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Differentially witness the FOURTH non-`-` `%RESPONSE_FLAGS%` value — `URX` (UpstreamRetryLimitExceeded) — BYTE-EXACT on the H1 retry-limit-exceeded path (`retry_policy:{retry_on:"5xx", num_retries:N}` where every attempt 5xxs → the budget of `N` is consumed and the last upstream response is returned downstream verbatim, ADR-0045 finding L9), by threading ONE new per-request boolean from envoy-rust's retry-loop limit-exceeded exit into the phase-48/49/50 `%RESPONSE_FLAGS%` derive.

**Architecture:** Unlike `NR`/`UH`/`UO` (each 1:1 with a unique `%RESPONSE_CODE_DETAILS%`), the retry-limit-exceeded path's rcd is the SHARED `via_upstream` (the final attempt is a real upstream 503 — envoy-rust ALREADY emits this, matching Envoy, so NO rcd change). `URX` therefore CANNOT be rcd-derived; it needs a SEPARATE boolean discriminator. This phase (§A) declares `retry_limit_exceeded_for_log` alongside the other `*_for_log` locals and sets it `true` at the retry-loop limit-exceeded exit (`crates/envoy-http1/src/hcm.rs:1126-1128`, the same gate as `upstream_rq_retry_limit_exceeded().inc()`), and (§B) wraps the `%RESPONSE_FLAGS%` derive (`hcm.rs:1274`) as `if retry_limit_exceeded_for_log { "URX" } else { <unchanged NR/UH/UO/`-` match> }`. Additive: every existing fixture `0001`-`0058` stays byte-identical.

**Tech Stack:** Rust (workspace), `tokio`, `envoy-http1` HCM, `envoy-accesslog` (FileSink + CompiledJsonFormat), `differential` test crate (testcontainers + upstream Envoy `v1.33.0`), `HealthAwareHttp1Backend` (the `0024` retry-topology helper).

**Scope lock:** ADR-0108. NO new `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant. NO `%RESPONSE_CODE_DETAILS%` change (already matches Envoy). H1-only. `#![forbid(unsafe_code)]` holds. Projected ~3 tasks / ~40-90 LoC → §6.1 split does NOT fire (ADR-0109 stays reserved-but-unfired).

---

## Pre-flight: state confirmation (already done at PLAN-write; re-confirm at impl start)

- `git status` clean; `HEAD` at the phase-51 state-1 brainstorm commit (`53b7657`) or later.
- `docs/envoy-rust/phases/51-accesslog-rf-retry-exhausted/SPEC.md` present; this `PLAN.md` is the state-2 output; `PROGRESS.md` ABSENT.
- **Concurrency guard (memory `concurrent-loop-sessions-race-on-phase-pick`):** before any commit, re-run `git status --porcelain`; if a sibling already advanced STATE or wrote files, defer to the further-along session and remove only your own untracked artifacts.

## §6.2 recon — PLAN-VERIFY results (ALL CONFIRMED against the tree at PLAN-write; no SPEC fact overturned)

1. **The §A set-site is provably 1:1 with the L9 path (SPEC §3.1).** The post-loop split at `hcm.rs:1126-1132` reads:
   ```rust
   if attempts > 1 && !retry_budget_blocked {
       if final_retriable {
           cluster.upstream_rq_retry_limit_exceeded().inc();   // :1128
       } else {
           cluster.upstream_rq_retry_success().inc();
       }
   }
   ```
   `final_retriable` is assigned per attempt at `:1057-1062`. The `if final_retriable` arm (`:1127`) is the EXACT gate Envoy increments `URX` on (num_retries consumed with the final attempt still retriable). Setting `retry_limit_exceeded_for_log = true` co-located with `.inc()` at `:1128` is the UNIQUE set-site. **EXCLUDED (confirmed):** (a) the retry-BUDGET-blocked exit (`retry_budget_blocked = true` at `:1098`) is gated out by `!retry_budget_blocked` — its `URX`-vs-other disposition is un-recon'd (stays M45-2); (b) the pre-loop request-budget overflow (`hcm.rs:932`, the `BudgetAcquisition::Rejected`/`max_requests` arm) assigns `outgoing = synth_overflow(close)` and BYPASSES the retry loop entirely → it never reaches `:1126`, and it already tags `rcd="…{overflow}"` → renders `UO` (phase 50), NOT `URX`. No other path sets the boolean.
2. **The §A boolean must be declared at the OUTER scope (`:844`), not inside the loop.** `attempts` / `final_retriable` / `retry_budget_blocked` are declared INSIDE the proxy `else` block (`:959-978`); the set-site (`:1126`) is in that block. The derive READ-site (`:1274`) is below the writer-arm match, in the OUTER fn scope where `response_code_details_for_log` / `upstream_host_for_log` live (`:835-844`). So declare `let mut retry_limit_exceeded_for_log = false;` adjacent to `response_code_details_for_log` (`:844`) — visible at BOTH the set-site and the derive. Default `false` on every non-L9 path.
3. **The §B derive-wrapper form (SPEC §3.2).** `hcm.rs:1274` today is `response_flags: match response_code_details_for_log.as_deref() { Some("route_not_found") => "NR", Some("no_healthy_upstream") => "UH", Some("upstream_reset_before_response_started{overflow}") => "UO", _ => "-" }.to_owned()`. Wrap as `if retry_limit_exceeded_for_log { "URX" } else { <that match> }.to_owned()`. The borrow-before-move discipline holds: `.as_deref()` is a shared borrow that ends before `response_code_details_for_log` MOVES into the `response_code_details:` field at `:1304`; the new `bool` is `Copy` (no borrow/move interaction). The `NR`/`UH`/`UO` arms are UNREACHABLE when the boolean is set (it is set only on the L9 path, where rcd is `via_upstream` → the old `_ => "-"` arm) → those arms stay byte-identical → `0056`/`0057`/`0058` unchanged.
4. **NO rcd change (state-0 recon, locked).** Live `envoyproxy/envoy:v1.33.0` (digest `sha256:56da5afd…`) at the `0024` `/retry-exhausted` topology + `{rc,rcd,rf}` json_format + `GET /retry-exhausted` → byte-stable `{"rc":503,"rcd":"via_upstream","rf":"URX"}`. envoy-rust ALREADY emits `rcd:"via_upstream"` here (the final attempt is a real upstream 503 → the `endpoint:Some` + `outcome:Some(Response)` arm at `:1019-1024` keeps `via_upstream`). So `%RESPONSE_CODE_DETAILS%` matches byte-exact today — NO change. `URX` is a clean brace-free deterministic constant (no combination).
5. **Additive byte-preservation — disjointness PROVEN (SPEC §3.3).** A repo scan confirms NO fixture carries BOTH an `access_log` AND a `retry_policy` (verified: the `access_log ∧ retry_policy` intersection is empty). The `%RESPONSE_FLAGS%`/`%RESPONSE_CODE_DETAILS%`-logging fixtures are `0012`, `0040`, `0046`, `0050`, `0051`, `0052`, `0053`, `0054`, `0055`, `0056`, `0057`, `0058`; NONE drives a retry-limit-exceeded outcome → `retry_limit_exceeded_for_log` stays `false` on every existing fixture → the derive renders identically → `0001`-`0058` byte-identical. Fixture `0024` (the retry fixture) carries NO `access_log` (it is a keep-alive stat fixture) → untouched; `0050` (happy-path `via_upstream`) renders `rf:"-"` unchanged.
6. **Driver/probe + health-aware-backend wiring (§D) — the ONE fresh integration point (SPEC §3.5).** `AccessLogByteExactProbe` already supports an arbitrary `path` + `expected_status` (`tests/differential/src/lib.rs`, struct fields). The real work is THREE fixture-name-gated backend edits in `tests/differential/src/lib.rs`: (i) add `0059-accesslog-rf-retry-exhausted` to the `needs_health_aware_backend` allowlist (`:3095-3100`, currently `0019/0020/0022/0024/0025`); (ii) add a `/retry-exhausted=503` `--per-path` arm for `0059` in the SECOND `per_path` rebind (`:3155-3161`, currently `0024`/`0025`-only); (iii) the `0059` YAML carries `{{BACKEND_HOST}}`/`{{BACKEND_PORT}}` so `needs_backend = scan_needs_marker(..., "BACKEND_PORT")` fires. `BACKEND_HOST` resolves to `host.docker.internal` (Envoy side, `:3306`) / `127.0.0.1` (envoy-rust side, `:3385`); `BACKEND_PORT` is the health-aware backend's actual port (identical both sides). The stateless `--per-path=503` (NOT a stateful `--retry-script`) is exactly right — both attempts must 503; `retry_script` stays `None` for `0059` (so `spawn_with_retry_script(None, Some("/retry-exhausted=503"))`).
7. **§F backstop harness — a FULL backstop is feasible (SPEC §3.6, spec-review M-2 resolved).** The cited phase-16 test at `hcm.rs:3613` uses fail-then-OK (retry SUCCESS), but `retry_limit_exceeded_path_always_503` (`hcm.rs:6607`) already drives the L9 path via `spawn_fail_then_ok_upstream(503, 1000)` (fail_count 1000 ≫ the 2 attempts → effectively always-503). So §F is a structural clone of the phase-50 `h1_request_budget_overflow_access_log_carries_uo_flag` backstop (`hcm.rs:7112`, which wires an inline `HCMConfig` + a `{rc,rcd,rf}` FileSink) with: a plain `cluster_mgr_with_endpoint("backend", port)` (`:2163`) + `spawn_fail_then_ok_upstream(503, 1000)` (`:6512`) + `pool_mgr: None` + `retry_policy: Some(...)` on the route. NO need for the reduced derive-level fallback.

## File structure

- **`crates/envoy-http1/src/hcm.rs`** (modify) — three edits, all in one task:
  - §A boolean declaration at the `*_for_log` locals block (`:844`, after `response_code_details_for_log`).
  - §A set at the retry-loop limit-exceeded exit (`:1127-1128`, inside `if final_retriable`).
  - §B derive wrapper at the record-build site (`:1274`).
  - One new in-process backstop test in the `#[cfg(test)] mod tests` block (model on `h1_request_budget_overflow_access_log_carries_uo_flag` at `:7112` + `retry_limit_exceeded_path_always_503` at `:6607`).
- **`tests/fixtures/0059-accesslog-rf-retry-exhausted/`** (create) — `envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`.
- **`tests/differential/tests/access_log_rf_retry_exhausted.rs`** (create) — structural clone of `access_log_rf_overflow.rs` → `0059`.
- **`tests/differential/src/lib.rs`** (modify) — the three fixture-name-gated backend-wiring edits (`:3100`, `:3155-3161`).
- **`docs/envoy-rust/BEHAVIOR_CONTRACT.md`** (modify) — the `%RESPONSE_FLAGS%` row (`:1020`) + the retry-limit-exceeded wire-shape note (`:389`).

---

## Task 1: §A `retry_limit_exceeded_for_log` boolean + §B derive wrapper (in-process backstop)

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` (new test `h1_retry_limit_exceeded_access_log_carries_urx_flag` in `mod tests`)
- Modify: `crates/envoy-http1/src/hcm.rs:844` (§A decl), `:1126-1128` (§A set), `:1274` (§B derive)

- [ ] **Step 1: Write the failing retry-limit-exceeded backstop test.** Add to the `#[cfg(test)] mod tests` block, adjacent to `h1_request_budget_overflow_access_log_carries_uo_flag` (`~:7112`). Model the inline `HCMConfig` + FileSink shape on that test; take the always-503 backend + retry_policy from `retry_limit_exceeded_path_always_503` (`:6607`). All referenced helpers (`tempdir`, `spawn_fail_then_ok_upstream`, `cluster_mgr_with_endpoint`, `mk_stats`, `test_router_only_pipeline`, `drive`, `StdDuration`) already exist in `mod tests`.

```rust
    /// Phase 51 (ADR-0108) §F backstop: drive the H1 retry-limit-exceeded (L9)
    /// path — an always-503 backend (`spawn_fail_then_ok_upstream(503, 1000)`,
    /// fail_count ≫ the 2 attempts) + `retry_policy{retry_on:"5xx",num_retries:1}`
    /// → both attempts 503, the budget of 1 consumed, the last 503 surfaced
    /// downstream verbatim — with a {rc,rcd,rf} FILE json access-log. Asserts the
    /// logged line carries rcd:"via_upstream" (a REAL upstream 503, UNCHANGED —
    /// matches Envoy, NOT rewritten) and the DERIVED rf:"URX" (set at the
    /// limit-exceeded loop-exit boolean, NOT rcd-derived). The sole in-process
    /// proof of §A's discriminator + §B's derive wrapper. Fail-first: pre-change
    /// the derive's rcd-match falls to `_ => "-"` (via_upstream is unmatched) →
    /// it renders `"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_retry_limit_exceeded_access_log_carries_urx_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        // Always-503 backend: fail_count 1000 ≫ the 2 attempts → every attempt
        // 503 → the retry budget of 1 is consumed → limit-exceeded (L9).
        let (port, _reqs) = spawn_fail_then_ok_upstream(503, 1000).await;
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
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
            })),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "retry-limit-exceeded surfaces the last upstream 503 verbatim: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n",
            "retry-limit-exceeded access-log line carries rcd:via_upstream + rf:URX: {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the test to verify it FAILS.**

Run: `cargo test -p envoy-http1 h1_retry_limit_exceeded_access_log_carries_urx_flag -- --nocapture`
Expected: FAIL on the final `assert_eq!` — `logged` is `{"rc":503,"rcd":"via_upstream","rf":"-"}\n` (pre-change: the derive's rcd-match has no `via_upstream` arm → `_ => "-"`). (If it fails EARLIER — the backend/cluster does not build, or the status is not 503, or `RetryPolicy` field names differ — STOP and re-grep `mod tests` / `envoy_config::RetryPolicy`; the harness wiring must be correct so the assertion is the failing point. If still stuck → `superpowers:systematic-debugging`.)

- [ ] **Step 3: Implement §A — declare the boolean at the `*_for_log` locals.** In `crates/envoy-http1/src/hcm.rs`, immediately after the `response_code_details_for_log` declaration (`:844`):

```rust
        let mut response_code_details_for_log: Option<String> = None;
        // phase 51 (ADR-0108): per-request %RESPONSE_FLAGS% = "URX" discriminator.
        // URX (UpstreamRetryLimitExceeded) is the FIRST flag NOT 1:1 with a unique
        // %RESPONSE_CODE_DETAILS% — the retry-limit-exceeded path's rcd is the
        // SHARED "via_upstream" (a real upstream 503, already matching Envoy), so
        // the :1274 derive cannot key on rcd here. Set true ONLY at the retry-loop
        // limit-exceeded exit (:1128, the same gate as
        // upstream_rq_retry_limit_exceeded); read by the :1274 derive. `Copy` → no
        // borrow/move interaction with the rcd String. Stays false on every other
        // path (default → "-"/no-flags).
        let mut retry_limit_exceeded_for_log = false;
```

- [ ] **Step 4: Implement §A — set the boolean at the limit-exceeded exit.** At the post-loop split (`:1126-1132`), set the boolean inside the `if final_retriable` arm, co-located with the counter increment:

```rust
                        if attempts > 1 && !retry_budget_blocked {
                            if final_retriable {
                                cluster.upstream_rq_retry_limit_exceeded().inc();
                                // phase 51 (ADR-0108): the L9 retry-limit-exceeded
                                // exit — num_retries consumed with the final attempt
                                // still retriable → the last upstream response is
                                // surfaced downstream verbatim. Envoy renders
                                // %RESPONSE_FLAGS% = "URX" here (access-log-only,
                                // never a response header). Set the discriminator
                                // co-located with the counter (one shared gate) so
                                // the :1274 derive renders "URX". The rcd stays
                                // "via_upstream" (a real upstream 503 — UNCHANGED).
                                // EXCLUDED: the retry-BUDGET-blocked exit (gated out
                                // by !retry_budget_blocked) and the pre-loop
                                // request-budget overflow (:932, bypasses the loop →
                                // renders "UO").
                                retry_limit_exceeded_for_log = true;
                            } else {
                                cluster.upstream_rq_retry_success().inc();
                            }
                        }
```

- [ ] **Step 5: Implement §B — wrap the `%RESPONSE_FLAGS%` derive.** At `crates/envoy-http1/src/hcm.rs:1274`, wrap the existing rcd-match in the boolean check. Also update the explanatory comment block above it (`:1256-1273`) to document the URX branch:

```rust
                // phase 48 (ADR-0105) / 49 (ADR-0106) / 50 (ADR-0107) / 51
                // (ADR-0108): %RESPONSE_FLAGS%. Phase 51 prepends a boolean branch
                // for "URX" (UpstreamRetryLimitExceeded) — the FIRST flag NOT
                // derivable from %RESPONSE_CODE_DETAILS% (the retry-limit-exceeded
                // path's rcd is the shared "via_upstream"); it keys on the
                // `retry_limit_exceeded_for_log` boolean set at the retry-loop
                // limit-exceeded exit (:1128). The else-branch is the unchanged
                // phase-48/49/50 rcd-match:
                //   route_not_found     → NR (NoRoute)          — the two no-route
                //                          synth_404 arms (host-miss + route-miss).
                //   no_healthy_upstream → UH (NoHealthyUpstream) — the single
                //                          pick()->None no-healthy synth-503 arm.
                //   upstream_reset_before_response_started{overflow}
                //                       → UO (UpstreamOverflow) — the overflow
                //                          synth-503 (both pool arms + the
                //                          request-budget arm).
                // The boolean is set ONLY on the L9 path (rcd = via_upstream → the
                // else-match's `_ => "-"` arm), so the NR/UH/UO arms are unreachable
                // with it set → byte-identical to phase 50. Read by-ref here;
                // `response_code_details_for_log` is moved into the
                // `response_code_details:` field below (bool is Copy — no interaction).
                response_flags: if retry_limit_exceeded_for_log {
                    "URX"
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

- [ ] **Step 6: Run the backstop + the existing flag backstops to verify GREEN + no regression.**

Run: `cargo test -p envoy-http1 access_log_carries -- --nocapture`
Expected: `h1_retry_limit_exceeded_access_log_carries_urx_flag` PASS; `h1_request_budget_overflow_access_log_carries_uo_flag` PASS (unchanged); `h1_pool_overflow_access_log_carries_uo_flag` / `h1_no_healthy_access_log_carries_uh_flag` / the route-miss backstop PASS (unchanged). Then the retry-counter tests + the broader unit suite:
Run: `cargo test -p envoy-http1`
Expected: all PASS — `retry_limit_exceeded_path_always_503`, `retry_success_path_503_then_200`, the budget/overflow tests are byte-unaffected (only the access-log `response_flags` var changed; the boolean defaults false on every non-L9 path).

- [ ] **Step 7: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 51 T1: URX on the retry-limit-exceeded path — §A boolean + §B derive wrapper [ADR-0108]"
```

---

## Task 2: Fixture `0059-accesslog-rf-retry-exhausted` + differential test + harness backend wiring

**Files:**
- Create: `tests/fixtures/0059-accesslog-rf-retry-exhausted/envoy.yaml`
- Create: `tests/fixtures/0059-accesslog-rf-retry-exhausted/envoy-rust.yaml`
- Create: `tests/fixtures/0059-accesslog-rf-retry-exhausted/expectations.yaml`
- Create: `tests/fixtures/0059-accesslog-rf-retry-exhausted/README.md`
- Create: `tests/differential/tests/access_log_rf_retry_exhausted.rs`
- Modify: `tests/differential/src/lib.rs:3100` (needs_health_aware_backend allowlist), `:3155-3161` (per_path arm)

- [ ] **Step 1: Wire the health-aware backend for `0059` (edit 1 — the allowlist).** In `tests/differential/src/lib.rs`, add `0059` to `needs_health_aware_backend` (`:3095-3100`):

```rust
    let needs_health_aware_backend = needs_backend
        && (fixture_name == "0019-upstream-active-health-check"
            || fixture_name == "0020-upstream-connection-pooling-and-per-class-counters"
            || fixture_name == "0022-upstream-outlier-detection-consecutive-5xx"
            || fixture_name == "0024-upstream-retry-on-5xx"
            || fixture_name == "0025-upstream-circuit-breaker-retry-budget"
            || fixture_name == "0059-accesslog-rf-retry-exhausted");
```

- [ ] **Step 2: Wire the `/retry-exhausted=503` per-path arm for `0059` (edit 2).** In the SECOND `per_path` rebind (`:3155-3161`), add a `0059` arm. `retry_script` stays `None` for `0059` (the `:3148-3154` block is untouched — `0059` is not `0024`/`0025`), so the backend spawns via `spawn_with_retry_script(None, Some("/retry-exhausted=503"))` — a STATELESS always-503 path (both attempts 503, no cyclic window):

```rust
            let per_path = if fixture_name == "0024-upstream-retry-on-5xx" {
                Some("/retry-exhausted=503".to_string())
            } else if fixture_name == "0025-upstream-circuit-breaker-retry-budget" {
                Some("/budget-blocked=503".to_string())
            } else if fixture_name == "0059-accesslog-rf-retry-exhausted" {
                Some("/retry-exhausted=503".to_string())
            } else {
                per_path
            };
```

- [ ] **Step 3: Create `envoy-rust.yaml`.** Merge the `0024` retry topology (subject side: binds `127.0.0.1`, STRICT_DNS `{{BACKEND_HOST}}`/`{{BACKEND_PORT}}` cluster, a single `/retry-exhausted` route with `retry_policy:{retry_on:"5xx", num_retries:1}`) with the `0058` access-log block (`{rc,rcd,rf}` json_format FileSink). Drop `/retry-success` and `include_attempt_count_in_response` (not needed — the access log logs rc/rcd/rf only, no headers):

```yaml
# Phase 51 fixture-0059 (envoy-rust side). The retry-limit-exceeded (L9) path:
# a single /retry-exhausted route with retry_policy{retry_on:"5xx",num_retries:1}
# to a STRICT_DNS backend that 503s every attempt → the retry budget of 1 is
# consumed and the last upstream 503 is returned verbatim. A {rc,rcd,rf} json
# access-log witnesses %RESPONSE_FLAGS% = URX (the FOURTH non-`-` flag, phase 51,
# ADR-0108) byte-exact; the rcd stays via_upstream (a real upstream 503 — already
# matching Envoy). Backend = HealthAwareHttp1Backend at {{BACKEND_HOST}}:{{BACKEND_PORT}},
# spawned by the harness with `--per-path /retry-exhausted=503` (stateless,
# fixture-name-gated on 0059).
node: { id: envoy-rust-phase-51-fixture-0059, cluster: envoy-rust-phase-51 }
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
                      path: /tmp/0059-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: retry_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/retry-exhausted" }
                          route:
                            cluster: backend
                            retry_policy: { retry_on: "5xx", num_retries: 1 }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{BACKEND_PORT}}
```

- [ ] **Step 4: Create `envoy.yaml`** — the reference side. Identical HCM/route/cluster/access-log block to `envoy-rust.yaml`, EXCEPT: bind `0.0.0.0` (not `127.0.0.1`) on the listener; the `node` id is `envoy-phase-51-fixture-0059`; the access-log mount path is `/tmp/0059-envoy-mount/access.log`. **CRITICAL (plan-review C1):** the admin block MUST use a LITERAL `port_value: 0` — do NOT use `{{ADMIN_PORT}}`. The `{{ADMIN_PORT}}` marker is only substituted when `needs_admin_port` is true, and that gate (`tests/differential/src/lib.rs:2846-2850`) fires ONLY for `Driver::AdminScrape | Http1KeepAlive | Http2KeepAlive` — NOT the `http1_access_log_byte_exact` driver `0059` uses. An unresolved `{{ADMIN_PORT}}` is left literal → invalid Envoy bootstrap → the reference container fails to start → `0059` RED on CI. Clone the admin preamble from the SAME-DRIVER template `tests/fixtures/0058-accesslog-rf-overflow/envoy.yaml` (`admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }`), NOT from `0024` (an `Http1KeepAlive` fixture, where `{{ADMIN_PORT}}` IS substituted). Keep the json_format `{rc,rcd,rf}` identical to `envoy-rust.yaml` (so the emitted line is byte-identical cross-proxy). Example preamble:

```yaml
node: { id: envoy-phase-51-fixture-0059, cluster: envoy-phase-51 }
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
                      path: /tmp/0059-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: retry_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/retry-exhausted" }
                          route:
                            cluster: backend
                            retry_policy: { retry_on: "5xx", num_retries: 1 }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: {{BACKEND_HOST}}
                      port_value: {{BACKEND_PORT}}
```

- [ ] **Step 5: Create `expectations.yaml`** — clone `tests/fixtures/0058-accesslog-rf-overflow/expectations.yaml`, swap the mount paths to `0059`, the probe `path` to `/retry-exhausted`, and the comment to describe the retry-limit-exceeded path:

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0059-envoy-mount/access.log
    envoy_rust: /tmp/0059-envoy-rust-mount/access.log
  probes:
    # Probe 1: GET /retry-exhausted routed to STRICT_DNS `backend`, which 503s
    # every attempt (harness `--per-path /retry-exhausted=503`). The route's
    # retry_policy{retry_on:"5xx",num_retries:1} consumes its single retry → both
    # attempts 503 → the last upstream 503 is returned downstream VERBATIM
    # (ADR-0045 L9). FOURTH non-`-` %RESPONSE_FLAGS% witness: URX
    # (UpstreamRetryLimitExceeded) (phase 51, ADR-0108).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). The retry-exhausted
    # 503 is deterministic on BOTH sides. The rcd is `via_upstream` (a REAL
    # upstream 503 — NOT a unique failure string; envoy-rust already emits it,
    # matching Envoy). envoy-rust DERIVES %RESPONSE_FLAGS% = URX from a SEPARATE
    # boolean set at the retry-loop limit-exceeded exit (hcm.rs:1128), NOT from
    # the rcd (was the no-flags sentinel `-`).
    # state-0 recon (live v1.33.0): {"rc":503,"rcd":"via_upstream","rf":"URX"}.
    #   rc:  "%RESPONSE_CODE%"          → 503  (json NUMBER)
    #   rcd: "%RESPONSE_CODE_DETAILS%"  → "via_upstream"
    #   rf:  "%RESPONSE_FLAGS%"         → "URX"
    # Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf. Compact
    # separators + ONE trailing `\n` (ADR-0092 §E). Emitted line:
    #   {"rc":503,"rcd":"via_upstream","rf":"URX"}
    - method: get
      path: /retry-exhausted
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 6: Create `README.md`** — clone the `0058` README structure; describe: the retry-limit-exceeded (L9) topology (STRICT_DNS backend that 503s every attempt + the `--per-path /retry-exhausted=503` harness wiring), the `URX` witness (fourth `%RESPONSE_FLAGS%` value, the FIRST not 1:1 with a unique rcd), that the rcd stays `via_upstream` (unchanged — already matching Envoy), the pure-cross-proxy-equality assertion, and that `0059` is the FIRST access-log fixture needing a real health-aware backend.

- [ ] **Step 7: Create the differential test `tests/differential/tests/access_log_rf_retry_exhausted.rs`** — structural clone of `access_log_rf_overflow.rs`:

```rust
//! Docker-gated differential test for fixture 0059-accesslog-rf-retry-exhausted.
//! Phase 51 (ADR-0108) — the FOURTH non-`-` `%RESPONSE_FLAGS%` witness: `URX`
//! (UpstreamRetryLimitExceeded), BYTE-EXACT cross-proxy on the H1 retry-limit-
//! exceeded 503 path. A single `/retry-exhausted` route with
//! `retry_policy{retry_on:"5xx",num_retries:1}` to a STRICT_DNS backend that 503s
//! every attempt (harness `--per-path /retry-exhausted=503`): the single retry is
//! consumed and the last upstream 503 is returned downstream verbatim (ADR-0045
//! L9). The FIRST `%RESPONSE_FLAGS%` value NOT 1:1 with a unique
//! `%RESPONSE_CODE_DETAILS%` — the rcd is the shared `via_upstream` (a real
//! upstream 503, already matching Envoy, UNCHANGED), so envoy-rust DERIVES `URX`
//! from a SEPARATE boolean set at the retry-loop limit-exceeded exit
//! (hcm.rs:1128), rendered by the :1274 derive wrapper (was `-`).
//! Upstream Envoy v1.33 emits the same here (state-0 recon:
//! {"rc":503,"rcd":"via_upstream","rf":"URX"}).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /retry-exhausted` probe,
//! `expected_status: 503`, json_format {rc, rcd, rf}); asserts the emitted JSON
//! line is byte-identical. PURE cross-proxy equality (deterministic both sides).
//! 0059 is the FIRST access-log fixture needing a real health-aware backend.
//! H1-only (H2 deferred — M45-1).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_retry_exhausted() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0059-accesslog-rf-retry-exhausted");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 8: Build the harness + confirm configs parse; run the differential if Docker is available.**

Run: `cargo build -p envoy-bin` (memory `differential-harness-uses-debug-envoy-bin` — the differential runs `target/debug/envoy-bin`; rebuild it before running with the new fixture) then `cargo build -p differential --tests` (compiles the lib.rs edits + the new test).
Run: `cargo test -p differential --test access_log_rf_retry_exhausted -- --nocapture`
Expected: PASS (both sides emit `{"rc":503,"rcd":"via_upstream","rf":"URX"}`). **If Docker is unavailable / the run is host-flaky** (memories: `differential-fixtures-flake-under-parallel-load`, `differential-host-bridge-ip-192-168-65-2`) → this is **CI-authoritative**; do NOT treat a local Docker-absent skip or a known host-flake as a regression. The state-4 verification gate re-runs it on CI. At minimum confirm the subject config parses: `cargo run -p envoy-bin -- -c tests/fixtures/0059-accesslog-rf-retry-exhausted/envoy-rust.yaml --mode validate` (or the project's config-validate entrypoint) returns OK.

- [ ] **Step 9: Commit.**

```bash
git add tests/fixtures/0059-accesslog-rf-retry-exhausted/ tests/differential/tests/access_log_rf_retry_exhausted.rs tests/differential/src/lib.rs
git commit -m "phase 51 T2: fixture 0059-accesslog-rf-retry-exhausted + differential test + health-aware backend wiring [ADR-0108]"
```

---

## Task 3: §E BEHAVIOR_CONTRACT updates

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1020` (the `%RESPONSE_FLAGS%` row)
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:389` (the retry-limit-exceeded wire-shape note)

- [ ] **Step 1: Update the `%RESPONSE_FLAGS%` row (`:1020`).** (a) Add the **`URX` per-flag equivalence rule**: a config-deterministic single static constant (no combination, brace-free), set on the single H1 retry-limit-exceeded loop-exit (cite by the `upstream_rq_retry_limit_exceeded` loop-exit SYMBOL, **not a raw line** — SPEC §E / the M48-1/M49-3 line-drift discipline), **derived from the `retry_limit_exceeded_for_log` boolean — NOT from `%RESPONSE_CODE_DETAILS%`, which is the shared `via_upstream` on this path (the FIRST flag not 1:1 with a unique rcd)**; the 503 status/body/headers AND the `via_upstream` rcd are unchanged. (b) In the "Other non-`-` flags (`UF`/`DC`/`URX`) remain unwitnessed (M45-2 …)" clause, REMOVE `URX` from that list (leave `UF`/`DC`). (c) Update the `value-exact` parenthetical to add `+ URX retry-limit-exceeded case`. (d) **(plan-review M2)** Fix the stale row OPENING — it still reads "Renders … `-` on every path EXCEPT **two** witnessed failure paths" (inaccurate since phases 49/50 added `UH`/`UO`); reword to reflect the now-FOUR witnessed non-`-` flags (`NR`/`UH`/`UO`/`URX`). (e) Add the witnessing-fixtures sentence: "Phase 51 (ADR-0108) fixture **0059** witnesses `URX` byte-exact on the H1 retry-limit-exceeded 503 path; both proxies emit `URX` (rcd unchanged at `via_upstream`)."

- [ ] **Step 2: Update the retry-limit-exceeded wire-shape note (`:389`).** It already states "Envoy's `%RESPONSE_FLAGS%` shows `URX` on this path, which is **access-log-only** and never surfaces as a response header." Append the cross-link: "Phase 51 (ADR-0108) fixture **0059** now witnesses `%RESPONSE_FLAGS% = URX` byte-exact cross-proxy here; envoy-rust derives it from the `retry_limit_exceeded_for_log` boolean set at the limit-exceeded loop-exit (the same gate as `upstream_rq_retry_limit_exceeded`), NOT from `%RESPONSE_CODE_DETAILS%` (which stays the shared `via_upstream`)."

- [ ] **Step 3: Verify the doc edits don't break any doc-driven test + re-confirm additivity.**

Run: `cargo test -p envoy-http1 && cargo test -p envoy-accesslog`
Expected: all PASS (these doc edits are prose-only; no test references the changed line numbers). Eyeball-confirm the `:1274` derive's `NR`/`UH`/`UO`/`-` arms and the `0056`/`0057`/`0058` fixture expectations are untouched.

- [ ] **Step 4: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 51 T3: BEHAVIOR_CONTRACT — URX witnessed (boolean-derived, rcd unchanged) [ADR-0108]"
```

---

## State-3 exit checklist (NOT this session — the session AFTER does state-4)

The state-3 implementation session ends after Task 3. It does NOT run the full §7.5 gate (that is state-4, `superpowers:verification-before-completion`). The state-3 session must: append a `PROGRESS.md` entry per task (RED→GREEN evidence), update `STATE.md` to `state-4-next`, push, and confirm CI. The carry-forward consumption to record at close:

- **CONSUMED by phase 51:** the `URX` slice of **M45-2** (leaving the connect-failure `UF`/`DC` + the retry-BUDGET-overflow slices live).
- **Still live (NONE blocks):** M50-C (the request-budget `max_requests` overflow UO/rcd — re-recon the rcd string before witnessing), M48-2, M42-1, M45-1 (H2 failure-path details + an H2 access-log differential driver), the connect-failure + retry-budget-overflow slices of M45-2, M40-1, M39-*, M38-*, CF-39-1, M37-*, M36-*, M34-*, M33-*, the empty-`metadata_match` doc-comment, M29-*, M30-*, the phase-31 cosmetics, the HTTP-filters-family (1)-(4).

## §6.1 split gate (re-evaluated at PLAN close)

3 tasks, ~40-90 LoC net: §A ~3 LoC (decl) + ~2 LoC (set) + §B ~6 LoC (wrapper) + 1 backstop test ~85 LoC of test + 4 fixture files + 1 differential test + 2 harness lib.rs edits (~8 LoC) + 2 doc edits. Well under ~25 tasks / ~1500 LoC → **§6.1 does NOT fire. ADR-0109 stays reserved-but-unfired** (reclaimable by the next NEW phase pick per the lapsed-reservation convention).

## §6.2 reconciliation ADR

NOT needed — the §6.2 recon (above) CONFIRMED every SPEC §A-§F fact; no fact was overturned. The set-site is 1:1 with the L9 path, the derive wrapper is additive, the rcd already matches Envoy, the existing-fixture sets are disjoint, and a FULL §F backstop is feasible (the SPEC §3.6 reduced-fallback is NOT needed). ADR-0108 stands.
