# Phase 65 — `65-accesslog-h2-rcd-upstream-reset` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task follows `superpowers:test-driven-development` — the failing test is written and *observed failing* before implementation.

**Goal:** Differentially witness the deterministic H2 upstream-reset `%RESPONSE_CODE_DETAILS%` string `upstream_reset_before_response_started{connection_termination}` byte-exact, and migrate the H2 `UC` `%RESPONSE_FLAGS%` derivation from the phase-64 `reset_for_log_h2` boolean onto that now-unique rcd (retiring the boolean). Consumes carry-forward **M64-1**.

**Architecture:** Two source edits in `crates/envoy-http2/src/hcm.rs`. (1) At the post-loop reconciliation region, set the deterministic reset rcd when the final attempt's `AttemptOutcome` is `Reset`, guarded `!retry_limit_exceeded_for_log_h2` — this OVERRIDES the shared `"via_upstream"` written in-loop at `:757`. (2) The `%RESPONSE_FLAGS%` derive then keys `UC` off that now-unique rcd string (mirroring the existing `{overflow} => "UO"` arm), which makes the `reset_for_log_h2` boolean redundant — it is deleted from all 5 of its sites. A new differential fixture `0070` reuses phase 64's already-built `Http2CloseBackend` + `{{H2_CLOSE_BACKEND_PORT}}` marker verbatim (ZERO new harness). This is the exact H2 analogue of phase 54's H1 work (ADR-0111).

**Tech Stack:** Rust (pinned toolchain), `tokio`, `h2` (codec only), `envoy-accesslog`, `envoy-config`, `testcontainers` (differential harness), upstream Envoy `v1.33.0` (digest `sha256:56da5afd…`).

## Global Constraints

- **`#![forbid(unsafe_code)]` holds** in every crate root — no `unsafe`, no exemption (D-3.8).
- **NO new** `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant / fuzz target / differential-harness code. `ci.yml` unchanged (fixtures are not enumerated there; `tests/differential/tests/*.rs` are auto-discovered — `tests/differential/Cargo.toml` has no `[[test]]` entries).
- **Load-bearing additivity invariant:** all fixtures `0001`-`0069` stay byte-identical. Verified at PLAN-write (see PLAN-VERIFY §3.2 below): of the 16 fixtures logging `%RESPONSE_CODE_DETAILS%`, only `0062` drives a close backend — and it is H1 (`CLOSE_BACKEND_PORT`, the `envoy-http1` code path, untouched by this phase). The sole H2-reset fixture `0069` logs **no** rcd. `0067` (H2 `URX`) logs rcd but is retry-exhausted, so the `!retry_limit_exceeded_for_log_h2` guard blocks the set anyway.
- **`AttemptOutcome` is `Copy`** (`crates/envoy-config/src/bootstrap.rs:1930`) and is referenced as `envoy_config::AttemptOutcome` inside `hcm.rs`.
- **JSON access-log keys sort by UTF-8 byte order** (ADR-0094 §A) on BOTH proxies — declaration order in YAML/`BTreeMap` is irrelevant to the emitted line.
- **Scope locked by ADR-0122.** `ADR-0123` stays **reserved-but-unfired** (§6.1 split does NOT fire; §6.2 reconciliation did NOT fire — see "PLAN-VERIFY results").
- **Fixture `0070` is backend-spawning** → expect **LOCAL-RED** on this dev host (memory `differential-host-bridge-ip-192-168-65-2` / `tcpclosebackend-ipv6-unreachable-host-flake`) and **GREEN on CI**. CI is AUTHORITATIVE.

---

## PLAN-VERIFY results (SPEC §3, re-derived FRESH against the live tree this session)

| # | Item | Result |
|---|---|---|
| 3.1 | §A set-site + guard | **CONFIRMED.** `retry_limit_exceeded_for_log_h2` is set at `hcm.rs:869`, `connect_failure_for_log_h2` at `:885`, `reset_for_log_h2` at `:896` — all in the post-loop reconciliation block, all AFTER the in-loop `via_upstream` set at `:757` and BEFORE the derive at `:1087` (reached via `finalize_h2_stream(…)` at `:928`). So the guard at `:896` reads an already-computed `retry_limit_exceeded_for_log_h2`, and an rcd set there wins over `:757`. |
| 3.2 | Additivity re-grep | **CONFIRMED** — see Global Constraints. No existing fixture both logs rcd and drives an H2 pure-reset. |
| 3.3 | Live-Envoy rcd re-confirm | **CONFIRMED (standing empirical proof, no new Docker run needed).** Fixture `0062-accesslog-rcd-upstream-reset` is a LANDED, CI-green differential fixture asserting the exact literal `upstream_reset_before_response_started{connection_termination}` **cross-proxy against real Envoy v1.33.0** on the H1 reset path. Phase-64 Finding 1 captured the byte-identical string on the H2 path (byte-stable across 3 repeats + a container restart). The brace content is a fixed reset-reason enum (no OS text), structurally proven on `{overflow}` (fixtures `0058` H1 / `0066` H2). |
| 3.4 | `reset_for_log_h2` sweep | **CONFIRMED, exactly 5 sites, zero drift:** decl `:577` (comment block `:567`-`:576`), post-loop set `:896`-`:897` (comment `:889`-`:895`), call-site arg `:944`, parameter `:1000` (doc comment `:997`-`:999`), derive branch `:1091`-`:1099`. No other consumer in `crates/envoy-http2/src/`. |
| 3.5 | §G backstop form | **CORRECTION (in-scope, no §6.2).** The existing `spawn_upstream_h2_reset_server()` (`hcm.rs:4928`) accepts **exactly one** connection then its task returns, dropping the `TcpListener`. A retry-exhausted negative test needs the SECOND attempt to also reset; against the one-shot helper attempt 2 would get `ConnectionRefused` → `ConnectFailure`/`UF`, **not** `URX`. Task 2 therefore adds a NEW multi-accept helper `spawn_upstream_h2_reset_server_multi()`. The positive backstop is extended in-place (Task 1). |
| 3.6 | §6.1 split | **DOES NOT FIRE.** 6 tasks, ~130 net LoC in `crates/` + ~150 lines of new fixture/test files — far under the ~25-task / ~1500-LoC gate. `ADR-0123` stays reserved-but-unfired. |
| 3.7 | Fixture number + json key order | **`0070` is next-free** (highest is `0069`). **CORRECTION (cosmetic, no §6.2):** SPEC §C's illustrative expected line lists keys in YAML-declaration order. Keys actually sort by UTF-8 byte order (ADR-0094 §A, as fixture `0069`'s own `expectations.yaml` documents), so the real emitted line is `{"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`. No behavioral impact — the driver asserts pure cross-proxy whole-line equality, not a static literal. |

**Neither correction overturns a §A-§G fact → `ADR-0123` remains reserved-but-UNFIRED.**

---

## File Structure

| File | Responsibility | Action |
|---|---|---|
| `crates/envoy-http2/src/hcm.rs` | H2 HCM: the rcd set (§A), the `%RESPONSE_FLAGS%` derive (§B), the `reset_for_log_h2` removal (§B/§F), the in-process backstops (§G) | Modify |
| `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/envoy.yaml` | Reference-side config (H2C listener → H2-upstream STRICT_DNS cluster → `Http2CloseBackend`), logs `{rc,rcd,rf,method,proto}` | Create |
| `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/envoy-rust.yaml` | Subject-side config (same, per-side bind/host deltas) | Create |
| `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/expectations.yaml` | `http2_access_log_byte_exact` driver + one `GET /` probe, `expected_status: 503` | Create |
| `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/README.md` | Fixture rationale + the exact emitted line | Create |
| `tests/differential/tests/access_log_h2_rcd_upstream_reset.rs` | Thin `run_fixture` wrapper (auto-discovered by cargo) | Create |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1020,1031` | `%RESPONSE_FLAGS%` + `%RESPONSE_CODE_DETAILS%` rows | Modify |
| `docs/envoy-rust/phases/65-.../PROGRESS.md` | Running execution log | Create (Task 1) |

---

### Task 1: §A — set the deterministic reset rcd (positive backstop first)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs:889-897` (the post-loop reset set-site)
- Test: `crates/envoy-http2/src/hcm.rs:4963` (extend the existing backstop `h2_upstream_reset_access_log_carries_uc_flag`)
- Create: `docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md`

**Interfaces:**
- Consumes: `final_outcome_h2: Option<envoy_config::AttemptOutcome>` (phase-63 loop-scoped capture), `retry_limit_exceeded_for_log_h2: bool` (set at `:869`), `response_code_details_for_log_h2: Option<String>` (declared `let mut` at `:543`).
- Produces: `response_code_details_for_log_h2 == Some("upstream_reset_before_response_started{connection_termination}")` on the pure-reset final-outcome path. Task 3 consumes this to derive `UC`.

- [ ] **Step 1: Extend the existing backstop to assert the rcd (the failing test)**

In `crates/envoy-http2/src/hcm.rs`, inside `h2_upstream_reset_access_log_carries_uc_flag` (`:4963`), add an `rcd` key to the json_format map, immediately after the existing `"rf"` insert:

```rust
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
```

Then replace the final assertion (currently `assert_eq!(logged, "{\"rc\":503,\"rf\":\"UC\"}\n", …)`) with:

```rust
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}\n",
            "upstream-reset access-log line carries the deterministic rcd + rf:UC: {logged:?}"
        );
```

(`CompiledJsonFormat::from_map` takes a `BTreeMap`, so the emitted keys sort: `rc`, `rcd`, `rf`.)

Also update the test's doc comment: replace the phrase `NOT rcd-derived, since the H2-side reset rcd stays the shared "via_upstream", deferred as M64-1` with `rcd-derived as of phase 65 (ADR-0122): the pure-reset path now sets the deterministic "upstream_reset_before_response_started{connection_termination}", consuming M64-1`.

- [ ] **Step 2: Run the test to verify it FAILS**

Run: `cargo test -p envoy-http2 h2_upstream_reset_access_log_carries_uc_flag -- --nocapture`

Expected: **FAIL**. The emitted line is `{"rc":503,"rcd":"via_upstream","rf":"UC"}` — the rcd is the shared `via_upstream` written in-loop at `:757`, not the deterministic string. (`rf` is already `UC`, derived from the still-present `reset_for_log_h2` boolean.)

- [ ] **Step 3: Implement §A — the guarded rcd set**

In `crates/envoy-http2/src/hcm.rs`, replace the phase-64 comment block + set at `:889`-`:897`:

```rust
                    // 64 (ADR-0121): flag UC when the FINAL attempt was a
                    // reset — independent of the retry split. A reset
                    // retried to success has final_outcome_h2 =
                    // Some(Response) → not flagged. If BOTH this and
                    // retry_limit_exceeded_for_log_h2 are set (un-recon'd
                    // combination, SPEC §4), the derive's URX-before-UC
                    // ordering renders URX deterministically.
                    reset_for_log_h2 =
                        matches!(final_outcome_h2, Some(envoy_config::AttemptOutcome::Reset));
```

with:

```rust
                    // 64 (ADR-0121): flag UC when the FINAL attempt was a
                    // reset — independent of the retry split. A reset
                    // retried to success has final_outcome_h2 =
                    // Some(Response) → not flagged.
                    reset_for_log_h2 =
                        matches!(final_outcome_h2, Some(envoy_config::AttemptOutcome::Reset));
                    // 65 (ADR-0122) §A: on the PURE-reset final outcome, set the
                    // DETERMINISTIC reset %RESPONSE_CODE_DETAILS%, OVERRIDING the
                    // shared "via_upstream" written in-loop at hcm.rs:757 (a reset
                    // has endpoint:Some + outcome:Some(Reset), so the phase-58
                    // `outcome.is_none()` overflow discriminator leaves it
                    // "via_upstream"). Guarded on !retry_limit_exceeded_for_log_h2
                    // (computed just above, hcm.rs:869): a retry-exhausted reset
                    // KEEPS "via_upstream" and renders URX — preserving today's
                    // behavior exactly (the un-recon'd combination of SPEC §4).
                    // Mirrors the H1 phase-54 set-site (ADR-0111) exactly.
                    if reset_for_log_h2 && !retry_limit_exceeded_for_log_h2 {
                        response_code_details_for_log_h2 = Some(
                            "upstream_reset_before_response_started{connection_termination}"
                                .to_owned(),
                        );
                    }
```

- [ ] **Step 4: Run the test to verify it PASSES**

Run: `cargo test -p envoy-http2 h2_upstream_reset_access_log_carries_uc_flag -- --nocapture`

Expected: **PASS**. Emitted line is now `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`. (`rf` still comes from the boolean at this point; Task 3 migrates it.)

- [ ] **Step 5: Confirm no in-process regression**

Run: `cargo test -p envoy-http2`

Expected: PASS, 0 failures. In particular `h2_retry_limit_exceeded_path_always_503` and `h2_connect_failure_access_log_carries_uf_flag` stay green (the guard leaves their rcd untouched).

- [ ] **Step 6: Create PROGRESS.md and commit**

Create `docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md` with a `# Phase 65 — PROGRESS` header and a `## Task 1 — §A rcd set` section quoting the Step 2 (fail) and Step 4 (pass) outputs.

```bash
git add crates/envoy-http2/src/hcm.rs docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md
git commit -m "phase 65 task 1: set deterministic H2 reset rcd on the pure-reset path (§A) [ADR-0122]"
```

---

### Task 2: §G-negative — lock the `!retry_limit_exceeded_for_log_h2` guard

The guard added in Task 1 is the single most error-prone line in this phase, and the differential fixture `0070` **cannot** exercise the retry-exhausted-reset path. SPEC §G marks this test **REQUIRED, not optional** (mirroring phase-54 spec-review M3).

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (add a multi-accept helper + a new backstop test, both in the `mod tests` block near `spawn_upstream_h2_reset_server` at `:4928`)

**Interfaces:**
- Consumes: `build_cluster_mgr_with_upstream(addr, envoy_cluster::UpstreamProtocol::Http2)` (`:1395`), `drive_h2_once(config) -> (u16, Vec<(String,String)>)` (`:4499`), `envoy_config::RetryPolicy { retry_on: String, num_retries: Option<u32>, retriable_status_codes: Vec<u32> }` (`bootstrap.rs:1917`). `retry_on: "reset"` sets `on_reset` (`bootstrap.rs:2026`), and `is_retriable(AttemptOutcome::Reset) == on_reset` (`bootstrap.rs:1998`).
- Produces: `spawn_upstream_h2_reset_server_multi() -> (SocketAddr, tokio::task::JoinHandle<()>)` — a reset upstream that accepts **many** connections and resets **every** stream.

- [ ] **Step 1: Write the failing test**

In `crates/envoy-http2/src/hcm.rs`, immediately after `spawn_upstream_h2_reset_server` (`:4928`-`:4945`), add the multi-accept helper:

```rust
    /// 65 (ADR-0122) §G: like `spawn_upstream_h2_reset_server` but accepts an
    /// UNBOUNDED number of connections and resets EVERY stream on each. The
    /// one-shot helper above closes its listener as soon as its single
    /// connection's task returns, so a RETRIED attempt would hit
    /// ConnectionRefused (→ ConnectFailure/UF) instead of a second reset
    /// (→ Reset/URX). The retry-exhausted-reset backstop needs this shape.
    async fn spawn_upstream_h2_reset_server_multi() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            loop {
                let (tcp, _peer) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut conn = match h2::server::handshake(tcp).await {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    // Reset every stream: accept it, then drop the responder
                    // without ever calling send_response (an implicit RST_STREAM).
                    while let Some(Ok((_req, send_response))) = conn.accept().await {
                        drop(send_response);
                    }
                });
            }
        });
        (addr, handle)
    }
```

Then add the negative backstop test immediately after the (Task-1-extended) `h2_upstream_reset_access_log_carries_uc_flag`:

```rust
    /// 65 (ADR-0122) §G (negative case, REQUIRED): a RETRY-EXHAUSTED reset
    /// (`retry_on: "reset"`, `num_retries: 1`, against an always-reset
    /// upstream) must NOT take the §A rcd-set — the `!retry_limit_exceeded_for_log_h2`
    /// guard keeps the shared `via_upstream`, and the derive's URX-before-UC
    /// ordering renders `URX`. This is the one path fixture `0070` cannot
    /// exercise. Mirrors the H1 phase-54 guard backstop.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx() {
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

        let (upstream_addr, _upstream_handle) = spawn_upstream_h2_reset_server_multi().await;
        let cluster_mgr =
            build_cluster_mgr_with_upstream(upstream_addr, envoy_cluster::UpstreamProtocol::Http2)
                .await;
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test-retry-exhausted-reset".to_string(),
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
                                retry_on: "reset".into(),
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
        assert_eq!(status, 503, "retry-exhausted reset still surfaces a 503");
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n",
            "retry-exhausted reset keeps via_upstream rcd and renders URX: {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the test to verify it PASSES immediately**

Run: `cargo test -p envoy-http2 h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx -- --nocapture`

Expected: **PASS**. This test is a *characterization/guard* test — it asserts behavior Task 1's guard already preserves. It exists to FAIL LOUDLY if a later refactor (Task 3) drops the guard.

> **If it FAILS with `rf:"UC"` and the deterministic rcd**, the guard is wrong: `retry_limit_exceeded_for_log_h2` is not being set on this path. Stop and invoke `superpowers:systematic-debugging` — do not weaken the assertion.
> **If it FAILS with `rf:"UF"`**, the multi-accept helper is not accepting the retry's second connection. Re-check the `loop { … tokio::spawn(…) }` shape.

- [ ] **Step 3: Prove the guard is load-bearing (mutation check)**

Temporarily delete ` && !retry_limit_exceeded_for_log_h2` from the Task-1 `if` condition, then run:

Run: `cargo test -p envoy-http2 h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx`

Expected: **FAIL** — the line becomes `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"URX"}` (rcd wrongly overridden). **Restore the guard** and re-run to confirm PASS. This proves the test actually pins the guard.

- [ ] **Step 4: Run the full crate suite**

Run: `cargo test -p envoy-http2`

Expected: PASS, 0 failures.

- [ ] **Step 5: Append to PROGRESS.md and commit**

```bash
git add crates/envoy-http2/src/hcm.rs docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md
git commit -m "phase 65 task 2: required retry-exhausted-reset guard backstop + multi-accept reset helper (§G) [ADR-0122]"
```

---

### Task 3: §B + §F — derive `UC` from the rcd, retire `reset_for_log_h2`

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` — 5 boolean sites + 3 comment blocks + the derive

**Interfaces:**
- Consumes: `response_code_details_for_log_h2` set by Task 1.
- Produces: `finalize_h2_stream` loses its trailing `reset_for_log_h2: bool` parameter — its signature becomes `(…, retry_limit_exceeded_for_log_h2: bool, connect_failure_for_log_h2: bool) -> Result<(), Http2Error>`. No other caller exists (`finalize_h2_stream(` appears exactly twice: the call at `:928`, the definition at `:960`).

- [ ] **Step 1: Add the rcd-match arm and delete the boolean branch (the behavior change)**

Replace the derive at `hcm.rs:1087`-`:1107`:

```rust
        let response_flags_for_log_h2: &str = if retry_limit_exceeded_for_log_h2 {
            "URX"
        } else if connect_failure_for_log_h2 {
            "UF"
        } else if reset_for_log_h2 {
            // …phase-64 comment…
            "UC"
        } else {
            match response_code_details_for_log_h2.as_deref() {
                Some("route_not_found") => "NR",
                Some("no_healthy_upstream") => "UH",
                Some("upstream_reset_before_response_started{overflow}") => "UO",
                _ => "-",
            }
        };
```

with:

```rust
        let response_flags_for_log_h2: &str = if retry_limit_exceeded_for_log_h2 {
            "URX"
        } else if connect_failure_for_log_h2 {
            "UF"
        } else {
            // 65 (ADR-0122) §B: `UC` now derives 1:1 from the DETERMINISTIC
            // reset rcd (set post-loop in §A), exactly like `UO` keys off
            // `{overflow}` — so the phase-64 `reset_for_log_h2` boolean is
            // redundant and was RETIRED. `URX`/`UF` keep their booleans: the
            // retry-limit-exceeded path's rcd is a real completing 503
            // (`via_upstream`) and the connect-failure rcd carries
            // non-deterministic OS text (M45-2). Mirrors H1's post-phase-54
            // derivation split exactly.
            match response_code_details_for_log_h2.as_deref() {
                Some("route_not_found") => "NR",
                Some("no_healthy_upstream") => "UH",
                Some("upstream_reset_before_response_started{overflow}") => "UO",
                Some("upstream_reset_before_response_started{connection_termination}") => "UC",
                _ => "-",
            }
        };
```

Match-arm order among the four rcd strings is irrelevant (all distinct). The `.as_deref()` shared borrow still ends before the owned `String` moves into `response_code_details:` at `:1131` — borrow discipline unchanged.

- [ ] **Step 2: Delete the four remaining `reset_for_log_h2` sites**

1. **Post-loop set** (`:896`-`:897`, plus its `// 64 (ADR-0121): flag UC when …` comment `:889`-`:895`): delete both, and fold the `matches!` directly into the Task-1 `if` so no binding remains:

```rust
                    // 65 (ADR-0122) §A: on the PURE-reset final outcome, set the
                    // DETERMINISTIC reset %RESPONSE_CODE_DETAILS%, OVERRIDING the
                    // shared "via_upstream" written in-loop at hcm.rs:757. Guarded
                    // on !retry_limit_exceeded_for_log_h2 (computed just above): a
                    // retry-exhausted reset KEEPS "via_upstream" and renders URX.
                    // §B: `UC` then derives 1:1 from this rcd — the phase-64
                    // `reset_for_log_h2` boolean was RETIRED. A reset RETRIED to
                    // success has final_outcome_h2 = Some(Response) → not set.
                    if matches!(final_outcome_h2, Some(envoy_config::AttemptOutcome::Reset))
                        && !retry_limit_exceeded_for_log_h2
                    {
                        response_code_details_for_log_h2 = Some(
                            "upstream_reset_before_response_started{connection_termination}"
                                .to_owned(),
                        );
                    }
```

2. **Declaration** (`:577`) and its comment block (`:567`-`:576`, the `// 64 (ADR-0121): per-stream %RESPONSE_FLAGS% = "UC" …` paragraph): delete both entirely.
3. **Call-site arg** (`:944`): delete the line `        reset_for_log_h2,`.
4. **Parameter + doc comment** (`:997`-`:1000`): delete the `// Phase 64 (ADR-0121): the reset final-outcome discriminator …` comment and the `reset_for_log_h2: bool,` parameter.

**§F sweep discipline:** leave BACKWARD-LOOKING historical narrative comments verbatim (D-3.4/D-3.5 — e.g. the phase-57/63/64 "as of phase NN, …" prose in the `synth_h2_*` doc comments). Correct only ACTIVE-state prose that describes today's mechanism. Re-grep to confirm zero live references remain:

Run: `grep -n "reset_for_log_h2" crates/envoy-http2/src/hcm.rs`
Expected: **no output** (exit 1).

> **Opportunistic fold-in (M64-2, optional):** `hcm.rs:236` carries a stale comment naming the now-removed `synth_h2_502`. If you are already editing that comment region, correct it and note the fold-in in `PROGRESS.md`; otherwise leave it as a live carry-forward.

- [ ] **Step 3: Run both backstops to verify they still PASS**

Run: `cargo test -p envoy-http2 h2_upstream_reset_access_log_carries_uc_flag h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx -- --nocapture`

Expected: **PASS** both. The positive one now derives `UC` from the rcd (output-equivalent); the negative one still renders `URX` with `via_upstream`.

- [ ] **Step 4: Verify the whole workspace still compiles and passes**

Run: `cargo build --workspace --all-targets`
Expected: clean (the deleted parameter has exactly one call site).

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean — in particular no `unused_variables`/`unused_mut` left behind by the deletion.

Run: `cargo test -p envoy-http2`
Expected: PASS, 0 failures.

- [ ] **Step 5: Append to PROGRESS.md and commit**

```bash
git add crates/envoy-http2/src/hcm.rs docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md
git commit -m "phase 65 task 3: derive H2 UC from the reset rcd, retire reset_for_log_h2 (§B/§F) [ADR-0122]"
```

---

### Task 4: §C + §D — fixture `0070` + the differential test

Reuses phase 64's `Http2CloseBackend` verbatim: `tests/differential/src/lib.rs:3345` does `scan_needs_marker(&backend_scan_sources, "H2_CLOSE_BACKEND_PORT")`, so any fixture containing the `{{H2_CLOSE_BACKEND_PORT}}` marker auto-spawns the backend. **No harness edit, no allowlist edit, no `ci.yml` edit.**

**Files:**
- Create: `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- Create: `tests/differential/tests/access_log_h2_rcd_upstream_reset.rs`

**Interfaces:**
- Consumes: `differential::run_fixture(&Path) -> Result<()>`; markers `{{PORT}}`, `{{BACKEND_HOST}}`, `{{H2_CLOSE_BACKEND_PORT}}`; driver `kind: http2_access_log_byte_exact`.
- Produces: the byte-exact cross-proxy witness of the deterministic reset rcd.

- [ ] **Step 1: Create `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/envoy.yaml`**

Identical to `0069`'s `envoy.yaml` except the node id, the mount path, and the added `rcd` key:

```yaml
node: { id: envoy-rust-phase-65-fixture-0070, cluster: envoy-rust-phase-65 }
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
                      path: /tmp/0070-envoy-mount/access.log
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
                    - name: upstream_reset_vh
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
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
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
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{H2_CLOSE_BACKEND_PORT}} }
```

- [ ] **Step 2: Create `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/envoy-rust.yaml`**

Same as Step 1 but with the documented per-side deltas 0069 already uses (no `admin:` block, bind `127.0.0.1`, mount path `/tmp/0070-envoy-rust-mount/access.log`):

```yaml
node: { id: envoy-rust-phase-65-fixture-0070, cluster: envoy-rust-phase-65 }
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
                      path: /tmp/0070-envoy-rust-mount/access.log
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
                    - name: upstream_reset_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend_cluster }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    # STRICT_DNS cluster, H2 upstream (typed_extension_protocol_options), NO
    # circuit_breakers and NO retry_policy — the retry-exhausted-reset path is
    # deliberately NOT exercised here (it keeps `via_upstream`; the in-process
    # backstop `h2_retry_exhausted_reset_keeps_via_upstream_rcd_and_renders_urx`
    # pins it). The single endpoint is the SPAWNED Http2CloseBackend, which
    # completes a genuine H2 handshake, accepts the stream, then resets it
    # WITHOUT responding → the reset synth-503 with the DETERMINISTIC rcd
    # `upstream_reset_before_response_started{connection_termination}` (phase 65,
    # ADR-0122 — consumes M64-1). The {method,proto,rc,rcd,rf} log line omits
    # %UPSTREAM_HOST%, so the per-side {{BACKEND_HOST}} divergence is invisible
    # → byte-identity holds.
    - name: backend_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
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
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{H2_CLOSE_BACKEND_PORT}} }
```

- [ ] **Step 3: Create `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/expectations.yaml`**

```yaml
driver:
  kind: http2_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0070-envoy-mount/access.log
    envoy_rust: /tmp/0070-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `backend_cluster`, a STRICT_DNS H2-upstream
    # cluster with NO circuit_breakers and NO retry_policy whose single endpoint
    # is the SPAWNED Http2CloseBackend ({{H2_CLOSE_BACKEND_PORT}} ->
    # http2-echo-server --close-before-response, built at phase 64). Both proxies
    # DIAL it, the H2 handshake completes, the backend accepts the stream then
    # resets it without responding -> the reset synth-503.
    #
    # Phase 65 (ADR-0122) witnesses the DETERMINISTIC reset
    # %RESPONSE_CODE_DETAILS% on the H2 side -- CONSUMING carry-forward M64-1.
    # `UC` now derives 1:1 FROM that rcd (the phase-64 `reset_for_log_h2`
    # boolean was RETIRED), exactly as H1 does post-phase-54.
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static literal.
    # The response body is NOT compared. The rcd's brace content
    # `connection_termination` is a FIXED reset-reason enum (NOT OS-derived,
    # unlike the connect-failure rcd -- M45-2), hence witnessable byte-exact;
    # the identical string is already witnessed on H1 by fixture 0062 (phase 54).
    #
    # Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rcd, rf.
    # The emitted line is:
    #   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `tests/fixtures/0070-accesslog-h2-rcd-upstream-reset/README.md`**

```markdown
# 0070 — accesslog H2 upstream-reset `%RESPONSE_CODE_DETAILS%`

Phase 65 (ADR-0122). Witnesses the **deterministic H2 upstream-reset
`%RESPONSE_CODE_DETAILS%`** — `upstream_reset_before_response_started{connection_termination}` —
byte-exact cross-proxy on the H2 upstream-disconnect-before-headers 503 path, and
proves `%RESPONSE_FLAGS%` = `UC` now derives **1:1 from that rcd** (the phase-64
`reset_for_log_h2` boolean was retired). **Consumes carry-forward M64-1.**

The H2 analogue of fixture `0062` (phase 54, ADR-0111), which witnessed the
identical rcd string on the H1 path.

## Topology

An H2C listener (`codec_type: HTTP2`) routes `/` to `backend_cluster`, a
`STRICT_DNS` cluster whose upstream protocol is H2
(`typed_extension_protocol_options` → `explicit_http_config.http2_protocol_options`).
Its single endpoint is the harness-spawned **`Http2CloseBackend`** (marker
`{{H2_CLOSE_BACKEND_PORT}}`, auto-launched by `tests/differential/src/lib.rs`):
it completes a genuine H2 handshake, accepts the request stream, then drops the
responder **without responding** — an implicit `RST_STREAM`. There is **no**
`retry_policy` and **no** `circuit_breakers`, so the reset is the *final*
attempt's outcome.

## Emitted line (byte-identical on both proxies)

Keys sort by UTF-8 byte order (ADR-0094 §A):

    {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}

## Per-side deltas

Only the documented ones fixture `0069` already uses: the reference side binds
`0.0.0.0` and carries an `admin:` block; the subject side binds `127.0.0.1`.
`{{BACKEND_HOST}}` resolves to `host.docker.internal` (reference) and
`127.0.0.1` (subject). The log line omits `%UPSTREAM_HOST%`, so that divergence
is invisible and byte-identity holds.

## Determinism

`connection_termination` is a **fixed reset-reason enum**, not OS-derived text —
unlike the connect-failure rcd (M45-2), which is why fixtures `0060`/`0068` omit
`rcd` entirely. The same fixed-enum shape is already witnessed byte-exact by
`0058`/`0066` (`{overflow}`) and `0062` (`{connection_termination}`, H1).

## Local vs CI

This fixture spawns a backend → expect **LOCAL-RED** on dev hosts whose bridge
routing cannot reach a host-spawned backend from the Envoy container (see
`tcpclosebackend-ipv6-unreachable-host-flake` / the `0061`/`0062`/`0069`
precedent). **CI is authoritative.**
```

- [ ] **Step 5: Create `tests/differential/tests/access_log_h2_rcd_upstream_reset.rs`**

```rust
//! Docker-gated differential test for fixture
//! 0070-accesslog-h2-rcd-upstream-reset.
//! Phase 65 (ADR-0122) — witnesses the DETERMINISTIC H2 upstream-reset
//! `%RESPONSE_CODE_DETAILS%` (`upstream_reset_before_response_started{connection_termination}`)
//! byte-exact cross-proxy, and proves `%RESPONSE_FLAGS%` = `UC` now derives
//! 1:1 from that rcd (the phase-64 `reset_for_log_h2` boolean was retired) —
//! CONSUMING carry-forward M64-1. The H2 analogue of fixture 0062 (phase 54).
//! A STRICT_DNS H2-upstream cluster with NO circuit_breakers and NO
//! retry_policy whose single endpoint is the spawned Http2CloseBackend
//! (completes a genuine H2 handshake, then resets the stream without
//! responding). Spawns Envoy v1.33 in a container; spawns envoy-rust as a
//! subprocess; drives `kind: http2_access_log_byte_exact` (the phase-56
//! driver, reused verbatim); reads each side's file access-log and asserts
//! the emitted line is byte-identical (keys sort per ADR-0094 §A):
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_rcd_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0070-accesslog-h2-rcd-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 6: Rebuild the DEBUG binary, then run the fixture**

The differential harness runs `target/debug/envoy-bin` (memory `differential-harness-uses-debug-envoy-bin`), so it must be rebuilt to pick up Tasks 1+3:

```bash
cargo build -p envoy-bin
cargo test -p differential --test access_log_h2_rcd_upstream_reset -- --nocapture
```

Expected on **CI: PASS**. Expected on **this dev host: RED** with the reference side reporting a connect-failure/`UF` instead of a reset (the documented host-bridge-IP flake). Record BOTH outcomes in `PROGRESS.md`; do **not** weaken the fixture to make the local run green.

- [ ] **Step 7: Confirm 0069 is still byte-identical (the additivity spot-check)**

```bash
cargo test -p differential --test access_log_h2_uc_upstream_reset -- --nocapture
```

Expected: same result as before this phase (CI green; locally RED for the same host-bridge reason). Its `rf:"UC"` must be unchanged — it now arrives via the rcd-match instead of the boolean.

- [ ] **Step 8: Append to PROGRESS.md and commit**

```bash
git add tests/fixtures/0070-accesslog-h2-rcd-upstream-reset tests/differential/tests/access_log_h2_rcd_upstream_reset.rs docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md
git commit -m "phase 65 task 4: fixture 0070 + differential test for the H2 reset rcd (§C/§D) [ADR-0122]"
```

---

### Task 5: §E — BEHAVIOR_CONTRACT updates

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md:1020` (`%RESPONSE_FLAGS%` row), `:1031` (`%RESPONSE_CODE_DETAILS%` row)

- [ ] **Step 1: INVERT the H2-`UC` clause in the `%RESPONSE_FLAGS%` row (`:1020`)**

The row currently contains this sentence — it must be **inverted, not appended to** (phase-54 spec-review M2 discipline; leaving it would make the row self-contradictory):

> **On H2, `UC` is witnessed differently** (fixture **0069**, phase 64, ADR-0121): the H2 reset arm's `%RESPONSE_CODE_DETAILS%` stays the shared `via_upstream` (the H2-side deterministic rcd is deferred as carry-forward **M64-1**, distinct from the H1-side M53-1 that phase 54 already consumed) — so H2's `UC` is derived from a `reset_for_log_h2` boolean set post-loop from the SAME final-outcome capture phase 63 introduced (`final_outcome_h2`), read a second time, NOT 1:1 from rcd, exactly like H2's own `URX`/`UF` siblings and UNLIKE H1's rcd-derived `UC`.

Replace it with:

> **On H2, `UC` is now derived EXACTLY as on H1** (fixture **0070**, phase 65, ADR-0122): the H2 pure-reset path sets the DETERMINISTIC `%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{connection_termination}` post-loop (overriding the in-loop shared `via_upstream`, guarded `!retry_limit_exceeded_for_log_h2` so a retry-exhausted reset keeps `via_upstream` and renders `URX`), and `UC` derives **1:1 from that rcd** (the `UO`/`{overflow}` pattern). The phase-64 `reset_for_log_h2` boolean discriminator was RETIRED — **CONSUMING carry-forward M64-1**. H2's `URX`/`UF` remain boolean-derived (their rcds genuinely stay `via_upstream`), so both protocols now share the identical derivation split: `{NR, UH, UO, UC}` rcd-derived, `{URX, UF}` boolean-derived.

Also append to the row's evidence column (after the phase-64/fixture-0069 sentence):

> Phase 65 (ADR-0122) fixture **0070** witnesses the H2 reset `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}` byte-exact on the same path; H2's `UC` now derives 1:1 from that rcd (the phase-64 boolean discriminator retired) — **CONSUMING carry-forward M64-1** and completing full H1/H2 parity for the deterministic upstream-reset rcd.

While editing, refresh any stale H2-derive-site anchor in the touched clauses to the post-change line number (phase-54 spec-review M1 discipline).

- [ ] **Step 2: Add the H2 reset rcd to the `%RESPONSE_CODE_DETAILS%` row (`:1031`)**

In the definition column, after the phase-54 H1 pure-reset clause, add:

> — and, on **H2**, the pure-reset synth-503 (the final-outcome `AttemptOutcome::Reset` path, guarded `!retry_limit_exceeded_for_log_h2`, at the post-loop reconciliation region of `crates/envoy-http2/src/hcm.rs`) → `Some("upstream_reset_before_response_started{connection_termination}")` (phase 65, ADR-0122, overriding the in-loop `via_upstream`)

In the evidence column, replace the trailing sentence *"The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`) remain deferred as the continuing carry-forward **M56-1**."* with:

> The H2 upstream-reset path (fixture **0070**, phase 65, ADR-0122) now ALSO witnesses `rcd:"upstream_reset_before_response_started{connection_termination}"` cross-proxy byte-exact — the H2 sibling of the H1 witness at fixture `0062`, and the value H2's `UC` flag now derives from 1:1. This **CONSUMES carry-forward M64-1**; `M56-1` was already fully closed at phase 64. The connect-failure rcd (H1 `0060` / H2 `0068`) remains the sole non-deterministic reset-reason (OS-derived text, M45-2) and stays unwitnessed.

Update the trailing default-absent fixture list to include `0069` (which logs no rcd) and note `0070` as the new rcd-logging fixture.

- [ ] **Step 3: Verify the contract has no self-contradiction left**

```bash
grep -n "reset_for_log_h2" docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: **no output** — the retired boolean must not be described as an active mechanism anywhere.

```bash
grep -c "M64-1" docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: every remaining mention describes it as **CONSUMED**, not deferred.

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md
git commit -m "phase 65 task 5: BEHAVIOR_CONTRACT — H2 UC now rcd-derived, M64-1 consumed (§E) [ADR-0122]"
```

---

### Task 6: Workspace-green pre-flight

This is a **pre-flight**, not the §7.5 gate — the authoritative gate runs at state-4 (`superpowers:verification-before-completion`), where the Docker differential + h2spec + fuzz surface is CI-authoritative (memory `envoy-rust-state4-ci-first-execution`).

- [ ] **Step 1: Run the five local gates**

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace --no-fail-fast
cargo deny check
```

Expected: build/clippy/fmt/deny **clean**. `cargo test --workspace` will show the documented host-flakes (`differential-fixtures-flake-under-parallel-load`, `tcpclosebackend-ipv6-unreachable-host-flake` for `0061`/`0062`/`0069`/`0070`, `envoyrust-h2-handshake-test-host-flake`). Record the exact failing set in `PROGRESS.md` and confirm **each** is a documented flake, not a phase regression.

> If `cargo deny check` reds on a freshly-published advisory against an existing dep, patch-bump it (`cargo update -p <dep> --precise <ver>`) — do not treat it as a phase regression (memory `cargo-deny-reds-on-unrelated-advisory`).

- [ ] **Step 2: Confirm the retired boolean is gone workspace-wide**

```bash
grep -rn "reset_for_log_h2" crates/ tests/ docs/envoy-rust/BEHAVIOR_CONTRACT.md
```
Expected: **no output**.

- [ ] **Step 3: Append the pre-flight outputs to PROGRESS.md and commit**

```bash
git add docs/envoy-rust/phases/65-accesslog-h2-rcd-upstream-reset/PROGRESS.md
git commit -m "phase 65: state-3 implementation complete — local pre-flight recorded [ADR-0122]"
```

- [ ] **Step 4: Push and let CI adjudicate**

```bash
git push origin main
```

CI is authoritative for fixture `0070`, the `0001`-`0069` additivity, and h2spec. Confirm both jobs green before the state-4 verification session declares the §7.5 gate met.

---

## Acceptance (SPEC §5 / §7.5 — re-run at state-4)

- **(a)** fixture `0070` green (cross-proxy-equal status `503` + byte-identical whole line carrying `"rcd":"upstream_reset_before_response_started{connection_termination}"` and `"rf":"UC"`).
- **(b)** all `0001`-`0069` green simultaneously (additive — the invariant re-verified at PLAN-VERIFY §3.2; `0069`'s `rf:"UC"` byte-identical post-migration).
- **(c)** h2spec ≥95% (NO H2 codec/framing change).
- **(d)** no new fuzz target (SPEC §H) — `ci.yml` unchanged.
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy … -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean.
- **(f)** `REVIEW.md` approved (state-5).

`#![forbid(unsafe_code)]` holds. NO new crate/dependency/`Op`/`AccessLogRecord` field/`ConfigError` variant/test-harness code. **NO `%RESPONSE_FLAGS%` value change** — the witnessed H2 flag stays `UC`; only its DERIVATION moves from the boolean to the rcd.

## Self-review

- **Spec coverage:** §A → Task 1. §B → Task 3 (derive arm + boolean removal). §C → Task 4 Steps 1-4. §D → Task 4 Step 5. §E → Task 5. §F → Task 3 Step 2 (sweep, with the D-3.4/D-3.5 historical-comment rule stated). §G → Task 1 (positive, extended in place) + Task 2 (the REQUIRED negative case). §H → Global Constraints (no fuzz target). No spec section is unimplemented.
- **Type consistency:** `reset_for_log_h2: bool` is introduced-then-removed consistently (Task 1 keeps it, Task 3 deletes all 5 sites, Task 6 Step 2 greps to prove it). `spawn_upstream_h2_reset_server_multi()` is defined in Task 2 Step 1 and used only there. `finalize_h2_stream`'s post-Task-3 signature is stated explicitly in Task 3's Interfaces block. The rcd literal `upstream_reset_before_response_started{connection_termination}` is byte-identical across Tasks 1, 3, 4, 5.
- **No placeholders:** every code step carries the actual code; every command carries its expected output, including the two documented-flake escape hatches (local RED for `0070`, `cargo deny` advisory bumps).
- **Ordering rationale:** the guard (Task 1) is pinned by its negative test (Task 2) *before* the boolean is retired (Task 3), so a refactor error in Task 3 cannot silently pass. Task 2 Step 3's mutation check proves the negative test is load-bearing rather than vacuous.
