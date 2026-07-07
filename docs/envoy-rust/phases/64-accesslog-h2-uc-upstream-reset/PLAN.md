# Phase 64 — `64-accesslog-h2-uc-upstream-reset` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Every implementation task uses superpowers:test-driven-development — write the failing test FIRST. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Witness the SIXTH and FINAL H2 `%RESPONSE_FLAGS%` value, `UC` (UpstreamConnectionTermination), byte-exact on the H2 upstream-disconnect-before-headers 503 path, AND correct envoy-rust's H2 post-connect-dispatch-failure synth status from `502`→`503` to match upstream Envoy — via a NEW fixture `0069` — CLOSING carry-forward M56-1 (no H2 `%RESPONSE_FLAGS%` value remains open).

**Architecture:** Mirrors H1's phase-53 `UC` witness (same status-fix + boolean-discriminator shape) adapted to H2's phase-56/57/58/61/63 threading pattern (a new parameter through `finalize_h2_stream`'s sole call site), reusing the EXISTING `final_outcome_h2` loop-scoped capture phase 63 already added (no new loop state — smaller surface than phase 63 in `crates/`). Four surgical edits in `crates/envoy-http2/src/hcm.rs`: (1) rename `synth_h2_502()` → `synth_h2_reset()` IN PLACE, correcting its status `502`→`503` (its SOLE remaining call site, the `AcquireOutcome::Sent(Err(e))` arm); (2) a NEW per-stream boolean `reset_for_log_h2`, set post-loop by reading the EXISTING `final_outcome_h2` a second time; (3) thread the boolean through `finalize_h2_stream`'s signature + its ONE call site; (4) an `else if reset_for_log_h2 { "UC" }` branch on the H2 `%RESPONSE_FLAGS%` derive, ordered AFTER `UF`. UNLIKE phase 63, this phase ALSO needs a NEW H2-protocol-aware test-harness backend (§E) — a raw TCP accept-then-close backend would be misclassified by envoy-rust's own H2 client as `ConnectFailure`/`UF` (empirically proven by this session's own spike, matching the SPEC's state-0 recon), so the fixture needs a backend that completes a genuine H2 handshake, accepts the request stream, then resets it — a NEW `--close-before-response` mode on `http2-echo-server` + a NEW `Http2CloseBackend` harness struct + a NEW `H2_CLOSE_BACKEND_PORT` marker wired into `tests/differential/src/lib.rs`. Plus a NEW in-process backstop, a NEW differential fixture `0069`, a NEW differential test, and the BEHAVIOR_CONTRACT update.

**Tech Stack:** Rust, `crates/envoy-http2` (HCM + client + codec), `crates/envoy-config` (`AttemptOutcome`, already `Copy`), `crates/envoy-accesslog` (FileSink/json_format), the `h2` crate (client + server handshake), `tests/differential` (`Http2AccessLogByteExact` driver, `backend.rs`, `lib.rs` marker wiring), `tests/helpers/http2-echo-server`.

## Global Constraints

- `#![forbid(unsafe_code)]` holds — no `unsafe` anywhere in this phase.
- NO new `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant (SPEC §2/§J).
- Load-bearing additivity invariant: all `0001`-`0068` fixtures stay byte-identical (SPEC §2, re-verified §3 item 2 below).
- No new fuzz target (SPEC §J — `%RESPONSE_FLAGS%` is an existing operator; no H2 codec/framing change).

---

## §3 PLAN-VERIFY re-confirmation (done this session, before authoring tasks)

All eight SPEC §3 items were re-checked against the live tree this session (no drift found):

1. **Line numbers confirmed exact, fresh re-grep this session.** `synth_h2_502()`'s definition: `hcm.rs:1162`-`1172`. Its SOLE remaining call site: the `AcquireOutcome::Sent(Err(e))` arm, `:384`-`395` (comment `:385`-`387`, warn `:388`, call `:390`). The sibling `AcquireOutcome::ConnectFailure` arm (already fixed at phase 63, untouched here): `:396`-`408`, calling `synth_h2_connect_failure()` (defined `:1186`-`1196`, doc comment `:1174`-`1185` — the doc comment's closing sentence, "still 502, deferred as the continuing M56-1 `UC` slice (a future phase)", is ACTIVE-state prose describing today and MUST be corrected this phase per SPEC §A). `synth_h2_no_healthy_upstream()`'s own doc comment (`:1198`-`1207`) ALSO makes a now-doubly-stale claim ("`synth_h2_502()`'s OTHER call sites (connect-error `:387`, send-error `:398`) are UNCHANGED — still 502") — this predates phase 63 (which already redirected the connect-error site) and is corrected as a cheap adjacent bonus fix in Task 1 (out of SPEC's named scope but trivial and in the same edit region; noted explicitly so REVIEW.md sees it as a deliberate call, not scope creep). The `*_for_log_h2` locals: `retry_limit_exceeded_for_log_h2` declared `:555`, `connect_failure_for_log_h2` declared `:565` (SPEC's `:565` confirmed exact — the new `reset_for_log_h2` is declared immediately after, at the same block). The loop-scoped `final_outcome_h2: Option<AttemptOutcome>` capture: declared `:697`, set per-iteration `:779` (`final_outcome_h2 = attempt.outcome;`) — UNCHANGED by this phase, read a SECOND time. The post-loop set site for `connect_failure_for_log_h2`: `:873`-`876` (immediately after the `retry_limit_exceeded_for_log_h2` if-block ending `:861`, before `drop(retry_guard_slot)` at `:880`) — the new `reset_for_log_h2 = matches!(final_outcome_h2, Some(envoy_config::AttemptOutcome::Reset));` is added immediately after this. The M63-1 stale comment: `hcm.rs:835`-`837` (exact text: `"the no-healthy-upstream synth-503, connect-failure synth-502, reset synth-502, and overflow"` / `"synth-503 paths"`) — BOTH `synth-502` mentions become `synth-503` (all four arms are 503 after this phase). `finalize_h2_stream`'s SOLE call site: `:907`-`924` (`connect_failure_for_log_h2,` at `:922`, closing `)` at `:923`) and its `async fn` signature: `:938`-`975` (`connect_failure_for_log_h2: bool,` at `:974`, closing `) -> Result<(), Http2Error> {` at `:975`) — the new `reset_for_log_h2: bool,` parameter is added immediately after `:974`/`:922` respectively. The H2 `%RESPONSE_FLAGS%` derive: `:1061`-`1072` (the `else if connect_failure_for_log_h2 { "UF" }` arm is `:1063`-`1064`; the new `else if reset_for_log_h2 { "UC" }` arm is inserted immediately after, before the `else { match ... }` fallback at `:1065`).
2. **Additivity re-grep confirmed fresh this session.** `grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1\|CLOSE_BACKEND_PORT\|close-before-response"` over every existing H2-listener fixture's `envoy-rust.yaml` (`0009`, `0010`, `0018`, `0021`, `0064`, `0065`, `0066`, `0067`, `0068`) finds: `0021` has `circuit_breakers` (headroom only, real reachable backend); `0065` mentions `127.0.0.1:1` only in a comment (excluded pre-dial by a subset-miss); `0066` has `circuit_breakers` (pre-connect pending-reject); `0067` has `retry_policy` (a REAL always-503 upstream via `Http2EchoBackend`, which always responds — never resets); `0068` has a literal dead endpoint (kernel-refused connect → `ConnectFailure`, not `Sent(Err)`). NONE of `0009`/`0010`/`0018`/`0064` carries any of these tokens. **No existing H2 fixture's spawned backend ever completes a handshake then resets mid-stream without responding** — confirmed re-derived fresh this session (`Http2EchoBackend`, the only spawned H2 backend in use today, always responds 200). The additivity invariant holds; only the NEW fixture `0069` reaches `AcquireOutcome::Sent(Err(e))`.
3. **`finalize_h2_stream` call-site count confirmed.** `grep -n "finalize_h2_stream("` over `crates/envoy-http2/src/hcm.rs` returns exactly TWO hits: the `async fn finalize_h2_stream(` declaration (`:938`) and its ONE call site (`:907`). The single-new-`bool`-parameter form (phase 61/63's own precedent) is adopted.
4. **`synth_h2_502()` call-site count confirmed.** `grep -n "synth_h2_502()"` over the file returns exactly ONE call site (`:390`) plus doc-comment mentions (`:1175`, `:1177`, `:1179`, `:1182`, `:1183`, `:1202`, `:1205`, `:1226`, `:1230`, `:2825`, `:2827`) that are all prose, not calls. The rename-in-place approach (SPEC §A) is confirmed safe — no sibling session has added a second caller.
5. **§E backend design re-verified empirically THIS session** (not blindly trusted from the SPEC's own state-0 recon). A temporary spike (`crates/envoy-http2/src/hcm.rs`, added then FULLY REVERTED — `git status --porcelain` clean afterward, confirmed) added an in-process backend that completes a genuine `h2::server::handshake`, accepts the first stream via `conn.accept()`, then `drop`s the `SendResponse` handle without responding, wired via `build_cluster_mgr_with_upstream(addr, UpstreamProtocol::Http2)` + `synth_h2_hcm_config_proxy` + `spawn_h2_hcm` + a real `h2::client::handshake` + `send_request`. The driven request observed downstream status **502** — i.e. the CURRENT (pre-phase-64) `Sent(Err(e))`/`synth_h2_502()` arm, NOT `ConnectFailure`'s already-503 arm. This directly confirms the SPEC's Finding 2: this backend design lands in the arm this phase fixes, not the arm phase 63 already fixed. Additionally, re-reading `wait_h2_accept_ready` (`tests/differential/src/backend.rs:526`-`549`) confirms it performs ONLY `h2::client::handshake` (never opens a stream, never calls `send_request`) — so it cannot race with the close-before-response mode's stream-level reset (which only fires on `conn.accept()`, i.e. an actual stream open). The §E backend design is confirmed correct.
6. **§6.1 split decision: does NOT fire.** This PLAN has 9 tasks / an estimated ~350-450 LoC: a ~10-line synth rename+status-fix + a handful of comment edits (~15 lines) + a 1-line boolean declare + a 1-line post-loop set + a 3-line new-parameter thread (call site + signature) + a ~2-line derive-branch insert + one ~95-line in-process backstop test (needs its own inline reset-after-handshake backend helper, slightly larger than phase 63's since that reused an existing dead-endpoint helper) + a ~45-line `http2-echo-server` `--close-before-response` mode (argv + dispatch + unit tests + an integration test) + a ~55-line `Http2CloseBackend` harness struct (near-verbatim clone of `Http2EchoBackend`) + a ~30-line `lib.rs` marker-wiring block (near-verbatim clone of the `CLOSE_BACKEND_PORT` block) + a 4-file fixture `0069` (~150 LoC incl. README, closely modeled on fixture `0061`'s spawned-backend shape + fixture `0068`'s H2 listener shape) + a ~25-line differential test + two BEHAVIOR_CONTRACT prose edits — well under the ~25-task/~1500-LoC gate. Larger than phase 63's 7 tasks (~230-320 LoC) because this phase ALSO needs the entirely-new H2-aware close-before-response test-harness surface (§E), exactly as SPEC §6.1 projected (~10-14 tasks estimated at SPEC-write time; this PLAN's 9 tasks covers the same edit surface). **ADR-0122 is reserved but NOT expected to fire** for a §6.2 reconciliation — the state-0 recon (SPEC) + this session's OWN re-verification (item 5 above) both empirically confirm the design; no SPEC fact is overturned.
7. **Fixture number `0069` re-confirmed still next-free.** `ls tests/fixtures/ | sort | tail` shows the highest existing fixture is `0068-accesslog-h2-uf-connect-failure`; no sibling session has landed `0069` in between as of this PLAN-write.
8. **Comment-sweep scope DECIDED** (item 1 above states the concrete result): fix the M63-1 anchor (`:835`-`837`, both `synth-502`→`synth-503`) and the ACTIVE-state closing sentence of `synth_h2_connect_failure()`'s doc comment (`:1183`-`1185`) — both explicitly named by SPEC §A. ALSO fix `synth_h2_no_healthy_upstream()`'s doc comment (`:1205`-`1207`), which makes the same class of now-inaccurate "still 502" claim (not named by SPEC, but trivially cheap and in the identical edit region — folded into Task 1, called out explicitly). LEAVE VERBATIM every other historical "as of phase NN" narrative comment (the phase-57/61/63-authored doc comments describing THEIR OWN state at authoring time) — per the standing D-3.4/D-3.5 convention of not rewriting past narrative.

No §6.2 reconciliation ADR is needed — none of SPEC §A-§J is overturned by this re-verification.

---

## Task 1: Rename `synth_h2_502()` → `synth_h2_reset()`, correct 502→503, thread `reset_for_log_h2`, render `UC`, the comment sweep, and the §H in-process backstop

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (arm `:384`-`395`; helper `:1162`-`1172`; doc comments `:1183`-`1185` and `:1205`-`1207`; M63-1 anchor `:835`-`837`; boolean declare after `:565`; boolean set after `:876`; `finalize_h2_stream` signature after `:974` + call site after `:922`; derive after `:1064`; new test appended to the `tests` module)

This is the load-bearing task: it makes the SPEC's §A-§D+§H edits in one TDD cycle. The NEW in-process backstop (§H) is the fail-first vehicle — there is no pre-existing test on this arm to flip (SPEC §3 item 4's grep confirms zero status-asserting tests reference this call site today), so the backstop test itself proves both the status fix AND the `UC` derive.

- [ ] **Step 1: Write the failing backstop test**

Append to the `tests` module in `crates/envoy-http2/src/hcm.rs`, immediately after `h2_connect_failure_access_log_carries_uf_flag` (ends `:4884`):

```rust
    /// Spawn an in-process H2 server that completes a genuine handshake,
    /// accepts the FIRST request stream, then drops the `SendResponse`
    /// handle WITHOUT calling `send_response` — an implicit stream reset.
    /// Mirrors `spawn_upstream_h2_server` (`:1479`) but resets instead of
    /// responding. Used ONLY by this phase's backstop.
    async fn spawn_upstream_h2_reset_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (tcp, _peer) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut conn = match h2::server::handshake(tcp).await {
                Ok(c) => c,
                Err(_) => return,
            };
            if let Some(Ok((_req, send_response))) = conn.accept().await {
                drop(send_response);
            }
        });
        (addr, handle)
    }

    /// 64 (ADR-0121) §B/§C/§D/§H backstop: drive a single H2 request against
    /// a backend that completes the handshake then resets the stream (NO
    /// retry_policy), wired to a {rc,rf} FILE json access-log. Asserts the
    /// downstream response is the synth-503 (§A) AND the logged line carries
    /// the DERIVED rf:"UC" (set post-loop from the reset final-outcome
    /// boolean, reading the EXISTING `final_outcome_h2` capture a second
    /// time — NOT rcd-derived, since the H2-side reset rcd stays the shared
    /// "via_upstream", deferred as M64-1). H2 mirror of the H1 backstop
    /// `h1_upstream_reset_access_log_carries_uc_flag`
    /// (`crates/envoy-http1/src/hcm.rs:7734`), adapted to H2's
    /// boolean-not-rcd derive mechanism (matching the H2
    /// `h2_connect_failure_access_log_carries_uf_flag` shape exactly, one
    /// arm over). Fail-first: pre-change this observes status 502 and a
    /// logged line of `{"rc":502,"rf":"-"}` (the rcd-match's `_ => "-"` arm,
    /// since `response_code_details_for_log_h2` stays `via_upstream`).
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_upstream_reset_access_log_carries_uc_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
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

        let (upstream_addr, _upstream_handle) = spawn_upstream_h2_reset_server().await;
        let cluster_mgr =
            build_cluster_mgr_with_upstream(upstream_addr, envoy_cluster::UpstreamProtocol::Http2)
                .await;
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test-upstream-reset".to_string(),
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
        let registry = Arc::new(envoy_stats::StatsRegistry::new());
        let mut built = Http1HCMConfig::from_config(&cfg, cluster_mgr, registry, None)
            .await
            .expect("build HCM config");
        built.access_log = vec![sink];
        let config = Arc::new(built);

        let (status, _headers) = drive_h2_once(config).await;
        assert_eq!(status, 503, "upstream-reset surfaces the synth-503 downstream");
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rf\":\"UC\"}\n",
            "upstream-reset access-log line carries rf:UC: {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the test — verify it FAILS**

Run: `cargo test -p envoy-http2 h2_upstream_reset_access_log_carries_uc_flag -- --nocapture`
Expected: FAIL — actual status `502`, actual logged line `{"rc":502,"rf":"-"}\n`.

- [ ] **Step 3: §A — rename `synth_h2_502()` → `synth_h2_reset()`, correct 502→503, fix the arm + its comment + warn string**

At `hcm.rs:384`-`395`, change:
```rust
        AcquireOutcome::Sent(Err(e)) => {
            // Post-connect send/recv failure → classify as Reset (the upstream
            // connected but did not deliver a complete response). The H2
            // synth-502 preserves the pre-phase-16 dispatch-failure shape.
            tracing::warn!(error = %e, "H2 listener: upstream dispatch failed — emitting 502");
            H2AttemptResult {
                response: synth_h2_502(),
                endpoint: Some(endpoint),
                outcome: Some(envoy_config::AttemptOutcome::Reset),
                upstream_response: false,
            }
        }
```
to:
```rust
        AcquireOutcome::Sent(Err(e)) => {
            // Post-connect send/recv failure → classify as Reset (the upstream
            // connected but did not deliver a complete response). 64
            // (ADR-0121) §A: corrected the previously-unvalidated synth-502
            // to the synth-503 that matches upstream Envoy on this path.
            tracing::warn!(error = %e, "H2 listener: upstream dispatch failed — emitting 503");
            H2AttemptResult {
                response: synth_h2_reset(),
                endpoint: Some(endpoint),
                outcome: Some(envoy_config::AttemptOutcome::Reset),
                upstream_response: false,
            }
        }
```

At `hcm.rs:1157`-`1172`, change:
```rust
/// Emit a generic 502 Bad Gateway response with no body. Used by
/// `handle_one_stream` when upstream dispatch fails (no healthy endpoint,
/// connect error, or send_request error). Mirrors the shape of
/// envoy-http1's `synth_status(502, _)` without the H1 Connection:
/// header (H2 has its own connection lifecycle).
fn synth_h2_502() -> Response {
    Response {
        status: 502,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
        ],
        body: Bytes::from_static(b""),
    }
}
```
to:
```rust
/// 64 (ADR-0121) §A: the H2 post-connect dispatch-failure synth-503 —
/// renamed in place from `synth_h2_502()` (this was the function's SOLE
/// remaining call site after phase 63 redirected the connect-failure arm
/// to `synth_h2_connect_failure()`) and corrected 502→503 to match
/// upstream Envoy's status on the upstream-reset path. Used ONLY at the
/// `AcquireOutcome::Sent(Err(e))` arm (`:384`-`395`).
fn synth_h2_reset() -> Response {
    Response {
        status: 503,
        reason: None,
        headers: vec![
            ("server".to_string(), "envoy-rust".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
        ],
        body: Bytes::from_static(b""),
    }
}
```

At `hcm.rs:1174`-`1185` (the `synth_h2_connect_failure` doc comment), change the closing sentence:
```rust
/// `synth_h2_502()`'s
/// OTHER call site (the post-connect send/recv-failure `Reset` arm, `:384`-
/// `395`) is UNCHANGED — still 502, deferred as the continuing M56-1 `UC`
/// slice (a future phase).
```
to:
```rust
/// `synth_h2_502()`'s
/// OTHER call site (the post-connect send/recv-failure `Reset` arm, `:384`-
/// `395`) was corrected by phase 64 (ADR-0121), renaming the helper to
/// `synth_h2_reset()` (also 503) — closing carry-forward M56-1.
```

At `hcm.rs:1198`-`1207` (the `synth_h2_no_healthy_upstream` doc comment — the bonus adjacent fix per §3 item 8), change:
```rust
/// `synth_h2_502()`'s OTHER call sites (connect-error `:387`, send-error
/// `:398`) are UNCHANGED — still 502, deferred as the continuing M56-1
/// carry-forward (the H2 `UF`/`UC` slices, future phases).
```
to:
```rust
/// `synth_h2_502()`'s two OTHER call sites (connect-error, send-error) were
/// both corrected to 503 by phases 63 (ADR-0120, `synth_h2_connect_failure()`)
/// and 64 (ADR-0121, `synth_h2_reset()`) respectively — closing carry-forward
/// M56-1.
```

At `hcm.rs:835`-`837` (the M63-1 stale comment anchor), change:
```rust
                    // Post-loop reconciliation (mirrors H1).
                    // L5: upstream_rq_5xx reflects the COMPLETING REAL upstream
                    // response only (retried-away 5xx attempts do NOT tick it).
                    // Gated on the completing attempt having received a real upstream
                    // response — synth local replies (the no-healthy-upstream synth-
                    // 503, connect-failure synth-502, reset synth-502, and overflow
                    // synth-503 paths) do NOT tick it, preserving the pre-phase-16
                    // baseline (they never did). Single source of truth.
```
to:
```rust
                    // Post-loop reconciliation (mirrors H1).
                    // L5: upstream_rq_5xx reflects the COMPLETING REAL upstream
                    // response only (retried-away 5xx attempts do NOT tick it).
                    // Gated on the completing attempt having received a real upstream
                    // response — synth local replies (the no-healthy-upstream synth-
                    // 503, connect-failure synth-503, reset synth-503, and overflow
                    // synth-503 paths) do NOT tick it, preserving the pre-phase-16
                    // baseline (they never did). Single source of truth.
```

- [ ] **Step 4: §B — declare and set `reset_for_log_h2`**

At `hcm.rs:565` (immediately after `connect_failure_for_log_h2`'s declaration), add:
```rust
    // 64 (ADR-0121): per-stream %RESPONSE_FLAGS% = "UC" (UpstreamConnection-
    // Termination) discriminator. Set true POST-LOOP when the FINAL attempt's
    // outcome was AttemptOutcome::Reset (a reset RETRIED to success must NOT
    // flag UC — so this reads the final outcome, not a per-attempt set). Like
    // URX/UF, UC is NOT 1:1 with a unique %RESPONSE_CODE_DETAILS% (the reset
    // rcd stays the shared "via_upstream" — the H2-side deterministic rcd is
    // deferred, M64-1), so it keys on this boolean. Reuses the EXISTING
    // final_outcome_h2 capture from phase 63 — no new loop-scoped state.
    // Mirrors the H1 phase-53 `reset_for_log` local
    // (crates/envoy-http1/src/hcm.rs:865) exactly.
    let mut reset_for_log_h2: bool = false;
```

At `hcm.rs:876` (immediately after `connect_failure_for_log_h2`'s post-loop set, before `drop(retry_guard_slot);`), add:
```rust
                    // 64 (ADR-0121): flag UC when the FINAL attempt was a
                    // reset — independent of the retry split. A reset
                    // retried to success has final_outcome_h2 =
                    // Some(Response) → not flagged. If BOTH this and
                    // retry_limit_exceeded_for_log_h2 are set (un-recon'd
                    // combination, SPEC §4), the derive's URX-before-UC
                    // ordering renders URX deterministically.
                    reset_for_log_h2 = matches!(
                        final_outcome_h2,
                        Some(envoy_config::AttemptOutcome::Reset)
                    );
```

- [ ] **Step 5: §C — thread `reset_for_log_h2` through `finalize_h2_stream`**

At `hcm.rs:922` (the call site, immediately after `connect_failure_for_log_h2,`), add `reset_for_log_h2,` as a new argument:
```rust
        retry_limit_exceeded_for_log_h2,
        connect_failure_for_log_h2,
        reset_for_log_h2,
    )
    .await
}
```

At `hcm.rs:974` (the signature, immediately after `connect_failure_for_log_h2: bool,`), add:
```rust
    // Phase 63 (ADR-0120): the connect-failure final-outcome discriminator
    // (§E), consumed by the %RESPONSE_FLAGS% derive wrapper (§G). A `Copy`
    // primitive, same shape as retry_limit_exceeded_for_log_h2 above.
    connect_failure_for_log_h2: bool,
    // Phase 64 (ADR-0121): the reset final-outcome discriminator (§B),
    // consumed by the %RESPONSE_FLAGS% derive wrapper (§D). A `Copy`
    // primitive, same shape as connect_failure_for_log_h2 above.
    reset_for_log_h2: bool,
) -> Result<(), Http2Error> {
```

- [ ] **Step 6: §D — extend the H2 `%RESPONSE_FLAGS%` derive with the `UC` branch**

At `hcm.rs:1061`-`1072`, change:
```rust
        let response_flags_for_log_h2: &str = if retry_limit_exceeded_for_log_h2 {
            "URX"
        } else if connect_failure_for_log_h2 {
            "UF"
        } else {
            match response_code_details_for_log_h2.as_deref() {
                Some("route_not_found") => "NR",
                Some("no_healthy_upstream") => "UH",
                Some("upstream_reset_before_response_started{overflow}") => "UO",
                _ => "-",
            }
        };
```
to:
```rust
        let response_flags_for_log_h2: &str = if retry_limit_exceeded_for_log_h2 {
            "URX"
        } else if connect_failure_for_log_h2 {
            "UF"
        } else if reset_for_log_h2 {
            // Phase 64 (ADR-0121): "UC" (UpstreamConnectionTermination) is
            // likewise NOT derivable from %RESPONSE_CODE_DETAILS% (the reset
            // rcd stays the shared "via_upstream" on the H2 side — M64-1) —
            // checked THIRD via the boolean, ORDERED AFTER UF, mirroring the
            // H1 phase-52/53 wrapper ordering exactly. This CLOSES
            // carry-forward M56-1 — all six H2 %RESPONSE_FLAGS% values are
            // now witnessed.
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

- [ ] **Step 7: Run the backstop test — verify it PASSES**

Run: `cargo test -p envoy-http2 h2_upstream_reset_access_log_carries_uc_flag -- --nocapture`
Expected: PASS.

- [ ] **Step 8: Run the full `envoy-http2` crate test suite — verify no regression**

Run: `cargo test -p envoy-http2`
Expected: PASS (all existing tests, including the phase-63 `h2_connect_failure_*` tests, which are unaffected by this task's changes — they hit the SIBLING `ConnectFailure` arm, untouched here).

- [ ] **Step 9: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 64: rename synth_h2_502->synth_h2_reset, correct 502->503, derive UC [ADR-0121]"
```

---

## Task 2: `http2-echo-server` gains a `--close-before-response` mode (§E-i)

**Files:**
- Modify: `tests/helpers/http2-echo-server/src/main.rs` (`Args` struct `:43`-`45`; `parse_argv` `:65`-`83`; `print_help` `:85`-`92`; `run` `:98`-`125`; `handle_connection` `:127`-`176`; tests module `:274`-`341`)

- [ ] **Step 1: Write the failing argv unit test**

Add to the `tests` module, immediately after `parse_argv_accepts_port`:
```rust
    #[test]
    fn parse_argv_accepts_close_before_response() {
        let args = parse_argv(&[
            "--port".to_string(),
            "7000".to_string(),
            "--close-before-response".to_string(),
        ])
        .unwrap();
        assert_eq!(
            args,
            Args {
                port: 7000,
                close_before_response: true,
            }
        );
    }
```

- [ ] **Step 2: Run it — verify it FAILS**

Run: `cargo test -p http2-echo-server parse_argv_accepts_close_before_response`
Expected: FAIL — compile error (`Args` has no `close_before_response` field; `parse_argv_accepts_port`'s existing `Args { port: 7000 }` literal will also need the new field, see Step 3).

- [ ] **Step 3: Add the `close_before_response` field + argv branch**

Change `Args` (`:42`-`45`):
```rust
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
    close_before_response: bool,
}
```

Change `parse_argv` (`:65`-`83`):
```rust
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    let mut i = 0;
    let mut port: Option<u16> = None;
    let mut close_before_response = false;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
            "--close-before-response" => {
                close_before_response = true;
                i += 1;
            }
            "--port" => {
                let v = args.get(i + 1).ok_or(ArgvError::MissingValue)?;
                port = Some(v.parse().map_err(|_| ArgvError::InvalidPort)?);
                i += 2;
            }
            _ => return Err(ArgvError::Trailing),
        }
    }
    Ok(Args {
        port: port.ok_or(ArgvError::MissingFlag("--port"))?,
        close_before_response,
    })
}
```

Fix the OTHER 3 existing `Args { port: N }` literals in the file (`parse_argv_accepts_port`, and the two argv-error tests construct no `Args` — only `parse_argv_accepts_port` needs the field added):
```rust
    #[test]
    fn parse_argv_accepts_port() {
        let args = parse_argv(&["--port".to_string(), "7000".to_string()]).unwrap();
        assert_eq!(
            args,
            Args {
                port: 7000,
                close_before_response: false,
            }
        );
    }
```

Change `print_help` (`:85`-`92`):
```rust
fn print_help() {
    println!(
        "http2-echo-server: HTTP/2 cleartext echo server helper for the envoy-rust differential harness.\n\
         \n\
         Usage:\n  http2-echo-server --port <u16> [--close-before-response]\n  \
         http2-echo-server --help\n  http2-echo-server --version"
    );
}
```

- [ ] **Step 4: Run the argv tests — verify they PASS**

Run: `cargo test -p http2-echo-server parse_argv`
Expected: PASS (all 5 argv tests: `accepts_port`, `accepts_close_before_response`, `rejects_missing_port`, `help_returns_help_requested`, `version_returns_version_requested`).

- [ ] **Step 5: Write the failing integration test for the close-before-response behavior**

Add to the `tests` module, immediately after `echo_round_trip_against_in_test_h2_client`:
```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn close_before_response_resets_stream_without_responding() {
        // Spawn the helper's close-before-response path directly (no argv
        // parse needed — the connection handler is exercised in-process).
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _server_task = tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await {
                handle_connection_close_before_response(tcp).await;
            }
        });
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        // The handshake succeeds (unlike a raw TCP close); the SUBSEQUENT
        // request fails because the server resets the stream instead of
        // responding.
        let req = http::Request::builder()
            .method("GET")
            .uri("http://testharness/test")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let result = response_fut.await;
        assert!(
            result.is_err(),
            "close-before-response must reset the stream, not respond: {result:?}"
        );
    }
```

- [ ] **Step 6: Run it — verify it FAILS**

Run: `cargo test -p http2-echo-server close_before_response_resets_stream_without_responding`
Expected: FAIL — compile error (`handle_connection_close_before_response` does not exist yet).

- [ ] **Step 7: Implement `handle_connection_close_before_response` + wire the dispatch**

Add immediately after `handle_connection` (`:176`):
```rust
/// `--close-before-response` mode: complete a genuine H2 handshake, accept
/// the FIRST request stream, then drop the `SendResponse` handle WITHOUT
/// calling `send_response` — an implicit stream reset (RST_STREAM). Used by
/// fixture `0069`'s `Http2CloseBackend` to drive envoy-rust's H2 client into
/// `AcquireOutcome::Sent(Err(e))` (phase 64, ADR-0121) — a raw TCP
/// accept-then-close backend would instead be misclassified as a connect
/// failure by envoy-rust's own H2 client (its `Client::connect` folds the
/// TCP-connect and h2::client::handshake into one call with a 10 ms
/// handshake-failure-detection window).
async fn handle_connection_close_before_response(tcp: tokio::net::TcpStream) {
    let mut conn = match envoy_http2::codec::server_handshake(tcp).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "h2 handshake failed (close-before-response)");
            return;
        }
    };
    if let Some(stream_result) = conn.accept().await {
        match stream_result {
            Ok((_req, send_response)) => {
                tracing::debug!("close-before-response: resetting stream without responding");
                drop(send_response);
            }
            Err(e) => {
                tracing::warn!(error = %e, "h2 stream accept failed (close-before-response)");
            }
        }
    }
}
```

Change `run` (`:98`-`125`) to dispatch on the new flag:
```rust
async fn run(args: Args) -> Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;
    tracing::info!("http2-echo-server listening on 0.0.0.0:{}", args.port);

    let mut join_set: JoinSet<()> = JoinSet::new();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("shutdown signal received");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        if args.close_before_response {
                            join_set.spawn(handle_connection_close_before_response(stream));
                        } else {
                            join_set.spawn(handle_connection(stream));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "accept failed; continuing");
                    }
                }
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 8: Run the integration test — verify it PASSES**

Run: `cargo test -p http2-echo-server close_before_response_resets_stream_without_responding -- --nocapture`
Expected: PASS.

- [ ] **Step 9: Run the full `http2-echo-server` crate test suite**

Run: `cargo test -p http2-echo-server`
Expected: PASS (all 7 tests: 5 argv + `echo_round_trip_against_in_test_h2_client` + `close_before_response_resets_stream_without_responding`).

- [ ] **Step 10: Commit**

```bash
git add tests/helpers/http2-echo-server/src/main.rs
git commit -m "phase 64: http2-echo-server gains --close-before-response mode [ADR-0121]"
```

---

## Task 3: `Http2CloseBackend` harness struct (§E-ii)

**Files:**
- Modify: `tests/differential/src/backend.rs` (new struct after `Http2EchoBackend`'s `Drop` impl, `:521`, before `wait_h2_accept_ready` at `:526`)

No new test file — this struct's correctness is proven end-to-end by Task 5/6's fixture + differential test (Docker-gated) and Task 1's in-process backstop (which already proves the underlying protocol behavior in-process). This task is a structural harness addition mirroring `Http2EchoBackend` verbatim; its own "test" is a successful `cargo build -p differential`.

- [ ] **Step 1: Add `Http2CloseBackend`**

Insert after `Http2EchoBackend`'s `Drop` impl closing brace (`:521`), before the `wait_h2_accept_ready` doc comment (`:523`):
```rust
/// A running `http2-echo-server --close-before-response` host subprocess —
/// completes a genuine H2 handshake, accepts the first request stream, then
/// resets it without responding. Phase 64 (ADR-0121): the H2-aware sibling
/// of `TcpCloseBackend` (`:92`) — a raw TCP accept-then-close backend would
/// be misclassified by envoy-rust's own H2 client as a connect failure
/// (`UF`), not the post-connect reset (`UC`) this backend drives. Reuses
/// `wait_h2_accept_ready` (`:526`) for readiness — its handshake-only probe
/// never opens a stream, so it cannot race with this backend's
/// stream-level reset.
pub struct Http2CloseBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl Http2CloseBackend {
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving h2 close-backend port")?;
        let bin = locate_http2_echo_server().context("locating http2-echo-server binary")?;
        let child = tokio::process::Command::new(&bin)
            .arg("--port")
            .arg(port.to_string())
            .arg("--close-before-response")
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| {
                format!(
                    "spawning {} --port {port} --close-before-response",
                    bin.display()
                )
            })?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        wait_h2_accept_ready(addr, Duration::from_secs(2))
            .await
            .with_context(|| {
                format!(
                    "http2-echo-server --close-before-response never became h2-accept-ready on {addr}"
                )
            })?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Per ADR-0015 + 05.1 STRICT_DNS posture: always `host.docker.internal`.
    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for Http2CloseBackend {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return,
                }
            }
        }
    }
}
```

- [ ] **Step 2: Build the differential crate — verify it compiles**

Run: `cargo build -p differential --tests`
Expected: clean build (the new struct is unused until Task 4 wires it — expect an `unused` warning at this point, not an error; if `-D warnings` is enabled for this crate, silence it with `#[allow(dead_code)]` temporarily removed once Task 4 lands, since by then it IS used).

- [ ] **Step 3: Commit**

```bash
git add tests/differential/src/backend.rs
git commit -m "phase 64: add Http2CloseBackend harness struct [ADR-0121]"
```

---

## Task 4: Wire `H2_CLOSE_BACKEND_PORT` marker scan + launch arm (§E-iii)

**Files:**
- Modify: `tests/differential/src/lib.rs` (new marker-scan/launch block after the `_close_backend`/`CLOSE_BACKEND_PORT` block, `:3302`-`3312`; `upstream_kvs` block `:3319`-`3408`; `subject_kvs` block `:3409`-`~3452`)

- [ ] **Step 1: Add the `H2_CLOSE_BACKEND_PORT` scan + spawn arm**

Insert immediately after the existing `_close_backend`/`close_backend_port_str` block (`:3302`-`3312`), before the `// (c) Build per-side substitution maps` comment (`:3314`):
```rust
    // Phase 64 (ADR-0121): the H2-aware handshake-then-reset upstream for
    // the fixture-0069 reset/UC witness. Distinct from CLOSE_BACKEND_PORT
    // (a raw TCP accept-then-close backend, which envoy-rust's H2 client
    // would misclassify as a connect failure) — this marker spawns the
    // Http2CloseBackend (a genuine H2 handshake, then a stream-level reset).
    let needs_h2_close_backend =
        scan_needs_marker(&backend_scan_sources, "H2_CLOSE_BACKEND_PORT");
    let _h2_close_backend: Option<crate::backend::Http2CloseBackend> = if needs_h2_close_backend {
        Some(
            crate::backend::Http2CloseBackend::spawn()
                .await
                .context("spawning Http2CloseBackend")?,
        )
    } else {
        None
    };
    let h2_close_backend_port_str = _h2_close_backend.as_ref().map(|b| b.port().to_string());
```

- [ ] **Step 2: Thread `h2_close_backend_port_str` into `upstream_kvs`**

In the `upstream_kvs` block, immediately after the existing `CLOSE_BACKEND_PORT` push (`:3341`-`3343`), add:
```rust
        // Phase 64 (ADR-0121): the H2-aware handshake-then-reset backend port.
        if let Some(h2cp) = h2_close_backend_port_str.as_deref() {
            v.push(("H2_CLOSE_BACKEND_PORT", h2cp.to_string()));
        }
```
And extend the `BACKEND_HOST`-gating `if` condition (`:3344`-`3351`) to also check `h2_close_backend_port_str`:
```rust
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
            || http1_backend_1_port_str.is_some()
            || http1_backend_2_port_str.is_some()
            || http2_backend_port_str.is_some()
            || close_backend_port_str.is_some()
            || h2_close_backend_port_str.is_some()
        {
            // Per ADR-0015: container-side reaches the host backend via
            // host.docker.internal (with the harness's with_host call below).
            v.push(("BACKEND_HOST", "host.docker.internal".to_string()));
        }
```

- [ ] **Step 3: Thread `h2_close_backend_port_str` into `subject_kvs`**

In the `subject_kvs` block, immediately after its own `CLOSE_BACKEND_PORT` push (`:3431`-`3433`), add:
```rust
        // Phase 64 (ADR-0121): the H2-aware handshake-then-reset backend port.
        if let Some(h2cp) = h2_close_backend_port_str.as_deref() {
            v.push(("H2_CLOSE_BACKEND_PORT", h2cp.to_string()));
        }
```
And extend its OWN `BACKEND_HOST`-gating `if` condition (mirrors upstream's, `:3434`-`3441`) the identical way:
```rust
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
            || http1_backend_1_port_str.is_some()
            || http1_backend_2_port_str.is_some()
            || http2_backend_port_str.is_some()
            || close_backend_port_str.is_some()
            || h2_close_backend_port_str.is_some()
        {
            v.push(("BACKEND_HOST", "127.0.0.1".to_string()));
        }
```

- [ ] **Step 4: Build the differential crate — verify it compiles clean (no `unused` warning now)**

Run: `cargo build -p differential --tests`
Expected: clean build, no warnings on `Http2CloseBackend`/`h2_close_backend_port_str` (now used).

- [ ] **Step 5: Commit**

```bash
git add tests/differential/src/lib.rs
git commit -m "phase 64: wire H2_CLOSE_BACKEND_PORT marker into run_fixture [ADR-0121]"
```

---

## Task 5: New differential fixture `0069-accesslog-h2-uc-upstream-reset` (§F)

**Files:**
- Create: `tests/fixtures/0069-accesslog-h2-uc-upstream-reset/envoy.yaml`
- Create: `tests/fixtures/0069-accesslog-h2-uc-upstream-reset/envoy-rust.yaml`
- Create: `tests/fixtures/0069-accesslog-h2-uc-upstream-reset/expectations.yaml`
- Create: `tests/fixtures/0069-accesslog-h2-uc-upstream-reset/README.md`

Structural clone of fixture `0068`'s H2C listener shape + fixture `0061`'s spawned-backend cluster shape (STRICT_DNS, `{{BACKEND_HOST}}`/`{{H2_CLOSE_BACKEND_PORT}}`, NO `circuit_breakers`, NO `retry_policy`).

- [ ] **Step 1: Write `envoy-rust.yaml`**

```yaml
node: { id: envoy-rust-phase-64-fixture-0069, cluster: envoy-rust-phase-64 }
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
                      path: /tmp/0069-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
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
    # circuit_breakers and NO retry_policy. The single endpoint is the
    # SPAWNED Http2CloseBackend ({{BACKEND_HOST}} = host.docker.internal here,
    # 127.0.0.1 on the subject side) at the harness-reserved
    # {{H2_CLOSE_BACKEND_PORT}}. Both proxies DIAL it, the H2 handshake
    # completes, the backend accepts the request stream then resets it
    # WITHOUT responding → the reset synth-503 (rf:"UC"). The {method,proto,
    # rc,rf}-only log line omits %UPSTREAM_HOST%, so the per-side
    # {{BACKEND_HOST}} divergence is invisible → byte-identity holds.
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

- [ ] **Step 2: Write `envoy.yaml`** (identical shape; `0.0.0.0` bind + admin block, per the standard per-side divergence discipline)

```yaml
node: { id: envoy-rust-phase-64-fixture-0069, cluster: envoy-rust-phase-64 }
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
                      path: /tmp/0069-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
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

- [ ] **Step 3: Write `expectations.yaml`**

```yaml
driver:
  kind: http2_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0069-envoy-mount/access.log
    envoy_rust: /tmp/0069-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `backend_cluster`, a STRICT_DNS H2-upstream
    # cluster with NO circuit_breakers and NO retry_policy whose single
    # endpoint is the SPAWNED Http2CloseBackend ({{H2_CLOSE_BACKEND_PORT}} ->
    # http2-echo-server --close-before-response). Both proxies DIAL it, the H2
    # handshake completes, the backend accepts the stream then resets it
    # without responding -> the reset synth-503. SIXTH and FINAL H2
    # %RESPONSE_FLAGS% witness: UC (UpstreamConnectionTermination) (phase 64,
    # ADR-0121) -- closes carry-forward M56-1.
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static
    # literal: the fixture logs ONLY {method, proto, rc, rf} -- the reset
    # %RESPONSE_CODE_DETAILS% (deterministic but deferred, M64-1) and the
    # response body are NOT compared. envoy-rust returns 503 (Task 1
    # corrected the unvalidated 502) and DERIVES %RESPONSE_FLAGS% = UC from
    # the reset final-outcome boolean (NOT from rcd).
    # state-0 recon (SPEC, live v1.33.0, digest sha256:56da5afd..., byte-stable
    # across 3 repeats + a restart): status 503, rf "UC" -- IDENTICAL to the
    # H1 UC witness (fixture 0061).
    #
    # Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rf. The
    # emitted line is:
    #   {"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Write `README.md`**

```markdown
# Fixture 0069 — H2 access-log `%RESPONSE_FLAGS%` upstream-reset path (`UC`, byte-exact)

The H2 analogue of fixture `0061` (phase 53, the H1 `UC` witness) and the
SIXTH fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`; extended by phase 57's `0065`, phase 58's `0066`, phase 61's
`0067`, and phase 63's `0068`). Phase 64 (ADR-0121) witnesses the SIXTH and
FINAL H2 `%RESPONSE_FLAGS%` value, `UC` (UpstreamConnectionTermination),
byte-exact on the H2 upstream-disconnect-before-headers 503 path — CLOSING
carry-forward M56-1 (no H2 `%RESPONSE_FLAGS%` value remains open).

## What this proves

Before this phase, envoy-rust's H2 `AcquireOutcome::Sent(Err(e))` arm emitted
a generic `synth_h2_502()` — a genuine, previously-unvalidated status
divergence (upstream Envoy returns 503 here), the SAME class of bug phases
52 (H1) / 57 / 63 (H2) each fixed for their own arms. Phase 64 (i) renames
`synth_h2_502()` → `synth_h2_reset()` in place and corrects the status
(503), (ii) declares a new per-stream boolean `reset_for_log_h2`, set
post-loop by reading the EXISTING `final_outcome_h2` capture (phase 63) a
SECOND time — no new loop-scoped state, (iii) threads it through
`finalize_h2_stream`'s sole call site, and (iv) extends the H2
`%RESPONSE_FLAGS%` derive with a boolean-gated `UC` branch ordered AFTER
`UF`.

**UNLIKE the H1 `UC` witness (fixture 0061, which reuses `TcpCloseBackend` — a
raw TCP accept-then-close backend), this fixture needs a NEW H2-protocol-aware
backend.** envoy-rust's own H2 client (`Client::connect`) folds the
TCP-connect and `h2::client::handshake` into one call with a 10 ms
handshake-failure-detection window; a raw accept-then-close backend (no H2
bytes at all) fails entirely inside that window, landing in the
ALREADY-FIXED `ConnectFailure`/`UF` arm — NOT the `Sent(Err(e))`/`UC` arm
this phase fixes. The fixture's backend (`http2-echo-server
--close-before-response`, via the NEW `Http2CloseBackend` harness struct)
instead completes a GENUINE H2 handshake, accepts the request stream, then
drops it without responding — confirmed (state-0 recon + this session's own
PLAN-write re-verification, both empirically) to drive BOTH envoy-rust's H2
client into `Sent(Err(e))`/`Reset` AND live upstream Envoy v1.33.0 into the
IDENTICAL `503`/`UC` disposition.

> **⚠ LOCAL-RED expected; CI is AUTHORITATIVE.** This fixture SPAWNS a
> backend, so it is subject to the host's Docker bridge-IP differential flake
> (memory `differential-host-bridge-ip-192-168-65-2`): expect LOCAL-RED on
> this dev host and GREEN on native-Linux CI.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | reset (handshake completes, stream reset before response) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}
```

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness driver change this phase. The backend spawn
is PURELY marker-driven: the `{{H2_CLOSE_BACKEND_PORT}}` marker in the
cluster endpoint triggers the `Http2CloseBackend` launch arm in `run_fixture`
(`tests/differential/src/lib.rs`) — a distinct marker from H1's
`{{CLOSE_BACKEND_PORT}}` (`TcpCloseBackend`), since the two backends are
fundamentally different (raw-TCP-close vs. genuine-H2-handshake-then-reset).

## `0001`-`0068` byte-preservation

This phase's changes are additive — gated on the `AcquireOutcome::Sent(Err(e))`
arm, which requires a backend that completes an H2 handshake then resets a
stream without responding. NONE of the pre-existing H2 fixtures (`0009`,
`0010`, `0018`, `0021`, `0064`-`0068`) reaches this arm — re-confirmed by a
fresh `grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1"` over each
`envoy-rust.yaml` this session (`0021`'s `circuit_breakers` gates a reachable,
always-responding backend; `0065`'s `127.0.0.1:1` is excluded pre-dial;
`0066`'s `circuit_breakers` pending-gate rejects pre-connect; `0067`'s
`retry_policy` drives a REAL always-503 `Http2EchoBackend`, which always
responds; `0068`'s literal dead endpoint hits `ConnectFailure`, not
`Sent(Err)`) — so `0001`-`0068` stay byte-identical; only the new `0069`
observes the new `rf:"UC"` witness.

## Cross-references

- ADR: ADR-0121 (state-1 brainstorm + state-2 PLAN — the H2 `UC` witness,
  closing carry-forward M56-1).
- Related fixtures: `0061` (the H1 `UC` witness whose derive mechanism this
  mirrors, but NOT its `TcpCloseBackend` harness — see "What this proves"
  above); `0064`/`0065`/`0066`/`0067`/`0068` (the H2 `NR`/`UH`/`UO`/`URX`/`UF`
  witnesses that opened/extended `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1 CLOSED** — all six H2 `%RESPONSE_FLAGS%` values
  (`NR`/`UH`/`UO`/`URX`/`UF`/`UC`) are now witnessed, matching H1's own
  six-flag completion at phase 53. **NEW carry-forward M64-1** — the H2-side
  deterministic `UC` `%RESPONSE_CODE_DETAILS%`
  (`upstream_reset_before_response_started{connection_termination}`),
  deferred to keep this witness minimum-viable (mirrors H1's own deferred
  rcd at phase 53, later consumed by phase 54's M53-1 — M64-1 is the H2-side
  analogue, distinct and still open).
```

- [ ] **Step 5: Run the Docker-gated differential test locally (expect LOCAL-RED per §5/README)**

Run: `cargo test -p differential access_log_h2_uc_upstream_reset -- --nocapture` (only runs after Task 6 creates the test file — if run now, this step is a no-op; defer this run to Task 6 Step 3).

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/0069-accesslog-h2-uc-upstream-reset/
git commit -m "phase 64: add fixture 0069-accesslog-h2-uc-upstream-reset [ADR-0121]"
```

---

## Task 6: Differential test `access_log_h2_uc_upstream_reset.rs` (§G)

**Files:**
- Create: `tests/differential/tests/access_log_h2_uc_upstream_reset.rs`

- [ ] **Step 1: Write the test** (thin wrapper, structural clone of `access_log_h2_uf_connect_failure.rs`)

```rust
//! Docker-gated differential test for fixture
//! 0069-accesslog-h2-uc-upstream-reset.
//! Phase 64 (ADR-0121) — the SIXTH and FINAL H2 `%RESPONSE_FLAGS%` witness:
//! `UC` (UpstreamConnectionTermination), byte-exact cross-proxy on the H2
//! upstream-disconnect-before-headers 503 path — closing carry-forward
//! M56-1. A STRICT_DNS H2-upstream cluster with NO circuit_breakers and NO
//! retry_policy whose single endpoint is the spawned Http2CloseBackend
//! (completes a genuine H2 handshake, then resets the stream without
//! responding): both proxies DIAL it, the handshake completes, the reset
//! fires post-connect -> the reset synth-503. envoy-rust now (a) returns
//! 503 (Task 1 corrected the unvalidated 502) and (b) DERIVES
//! `%RESPONSE_FLAGS%` = `UC` from the reset final-outcome boolean (NOT from
//! `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` and is
//! NOT logged/compared this phase — M64-1). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted
//! line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_uc_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0069-accesslog-h2-uc-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Build the differential test crate**

Run: `cargo build -p differential --tests`
Expected: clean build.

- [ ] **Step 3: Run the test locally (expect LOCAL-RED per the fixture README — this dev host's Docker bridge-IP flake; NOT a regression)**

Run: `cargo test -p differential --test access_log_h2_uc_upstream_reset -- --nocapture`
Expected: EITHER pass, or fail with a bridge-IP-related connectivity error (memory `differential-host-bridge-ip-192-168-65-2`). Record the actual local outcome in PROGRESS.md (Task 9) either way — CI is authoritative for the §7.5 gate (deferred to state-4).

- [ ] **Step 4: Commit**

```bash
git add tests/differential/tests/access_log_h2_uc_upstream_reset.rs
git commit -m "phase 64: add differential test access_log_h2_uc_upstream_reset [ADR-0121]"
```

---

## Task 7: BEHAVIOR_CONTRACT.md updates (§I)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_FLAGS%` row, line 1020 — two distinct edits within the same long table-cell paragraph)

- [ ] **Step 1: Extend the "Per-flag equivalence — `UC`" description to distinguish H1's rcd-derived `UC` from H2's boolean-derived `UC`**

Find this exact substring (the end of the H1 `UC` per-flag description, immediately before the `DC` sentence) in `docs/envoy-rust/BEHAVIOR_CONTRACT.md`:

```
is now witnessed byte-exact at phase 54 (ADR-0111), fixture **0062**. Other non-`-` flags (`DC`) remain unwitnessed (M45-2, non-deterministic surfaces) and still need their own per-flag rules.
```

Replace it with:

```
is now witnessed byte-exact at phase 54 (ADR-0111), fixture **0062**. **On H2, `UC` is witnessed differently** (fixture **0069**, phase 64, ADR-0121): the H2 reset arm's `%RESPONSE_CODE_DETAILS%` stays the shared `via_upstream` (the H2-side deterministic rcd is deferred as carry-forward **M64-1**, distinct from the H1-side M53-1 that phase 54 already consumed) — so H2's `UC` is derived from a `reset_for_log_h2` boolean set post-loop from the SAME final-outcome capture phase 63 introduced (`final_outcome_h2`), read a second time, NOT 1:1 from rcd, exactly like H2's own `URX`/`UF` siblings and UNLIKE H1's rcd-derived `UC`. Other non-`-` flags (`DC`) remain unwitnessed (M45-2, non-deterministic surfaces) and still need their own per-flag rules.
```

- [ ] **Step 2: Extend the evidence cell to record fixture `0069` and close M56-1**

Find this exact substring (the end of the evidence cell, currently the last sentence of the row):

```
The remaining H2 `%RESPONSE_FLAGS%` value (`UC`) remains deferred as the continuing carry-forward **M56-1**, witnessable by a future phase exactly as phase 53 did for H1 after phase 52 built the H1 `UF` pattern.
```

Replace it with:

```
`UC` is now ALSO witnessed byte-exact on H2 by fixture **0069** (phase 64, ADR-0121) — set via the SAME final-outcome-capture mechanism as `URX`/`UF` (a `reset_for_log_h2` boolean reading the EXISTING `final_outcome_h2` capture a second time — NOT derivable from `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` on this path, deferred as carry-forward **M64-1**, the H2-side deterministic `UC` rcd), threaded through `finalize_h2_stream` as a third new parameter, ordered AFTER `UF` in the derive — **CLOSING carry-forward M56-1**: all six H2 `%RESPONSE_FLAGS%` values (`NR`/`UH`/`UO`/`URX`/`UF`/`UC`) are now witnessed, full parity with H1's own six-flag completion at phase 53. Like `UF`, this phase ALSO corrected a genuine status-code divergence — envoy-rust's H2 post-connect-dispatch-failure arm previously emitted a previously-unvalidated `502` (via the generic `synth_h2_502()`, renamed in place); it now emits `503` via `synth_h2_reset()`, matching upstream Envoy and closing out the whole per-arm H2 status-correction sweep phases 52 (H1) / 57 / 63 / 64 progressively made.
```

- [ ] **Step 3: Verify the edits landed correctly**

Run: `grep -c "fixture \*\*0069\*\*" docs/envoy-rust/BEHAVIOR_CONTRACT.md`
Expected: `2` (one hit per edit above).

- [ ] **Step 4: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 64: BEHAVIOR_CONTRACT.md — H2 UC witnessed, M56-1 closed [ADR-0121]"
```

---

## Task 8: Local verification sweep (state-3 close-out; full §7.5 gate runs at state-4)

**Files:** none (verification only)

- [ ] **Step 1: cargo build**

Run: `cargo build --workspace --all-targets`
Expected: clean build.

- [ ] **Step 2: cargo clippy**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 3: cargo fmt --check**

Run: `cargo fmt --all -- --check`
Expected: clean (if not, run `cargo fmt --all` and amend the commit — see Step 7).

- [ ] **Step 4: cargo test --workspace**

Run: `cargo test --workspace`
Expected: PASS (all existing tests + the 3 new tests this phase adds: the envoy-http2 backstop, the http2-echo-server argv test, the http2-echo-server integration test; the Docker-gated differential tests skip locally per the harness's own gating — a local skip is not a failure. The `0069` differential test is expected to be LOCAL-RED per Task 6 Step 3's own note — do not treat that as this step's failure; re-run this step with `--skip access_log_h2_uc_upstream_reset` if needed to confirm everything ELSE is green).

- [ ] **Step 5: cargo deny**

Run: `cargo deny check`
Expected: clean. (If a fresh RustSec advisory reds an existing dep — NOT a phase regression — patch-bump it per memory `cargo-deny-reds-on-unrelated-advisory`.)

- [ ] **Step 6: confirm byte-preservation reasoning (no existing H2 fixture regressed)**

Run: `for f in 0009 0010 0018 0021 0064 0065 0066 0067 0068; do echo "=== $f ==="; grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1" tests/fixtures/${f}-*/envoy-rust.yaml || echo "(none)"; done`
Expected: matches §3 item 2's re-derivation above; `0001`-`0068` stay byte-identical; only `0069` observes the new `rf:"UC"` witness.

- [ ] **Step 7: final fmt-fix commit if needed** (otherwise nothing to commit)

```bash
cargo fmt --all
git add -A && git commit -m "phase 64: cargo fmt [ADR-0121]" || echo "nothing to reformat"
```

---

## Task 9: PROGRESS.md + handoff to state-4

**Files:**
- Create: `docs/envoy-rust/phases/64-accesslog-h2-uc-upstream-reset/PROGRESS.md`

> **NOTE:** Tasks 1–8 are the STATE-3 implementation session(s) — NOT this state-2 PLAN-write session (§5.1: one state per session). This task closes the state-3 arc by recording the running log; the state-4 verification (the full §7.5 gate, quoting all command outputs into PROGRESS.md, plus the authoritative CI run) is the session AFTER.

- [ ] **Step 1: Write PROGRESS.md**

Record, per task: what landed, the exact files touched, and the local command outputs (Tasks 1-4, 8). Note explicitly that the Docker differential `0069` (Task 5/6) is expected LOCAL-RED (host bridge-IP flake) and is deferred to the state-4 CI gate for authoritative confirmation, and that ADR-0122 did NOT fire (the §3 PLAN-VERIFY re-confirmation confirmed all §A-§J facts, including the backend-design re-verification in item 5).

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/phases/64-accesslog-h2-uc-upstream-reset/PROGRESS.md
git commit -m "phase 64: PROGRESS.md — state-3 implementation log [ADR-0121]"
```

---

## Out of scope (deferred — do NOT implement)

- **The deterministic `UC` `%RESPONSE_CODE_DETAILS%`** (`upstream_reset_before_response_started{connection_termination}`) — recon-confirmed deterministic, but deferred to keep this witness minimum-viable. Continuing carry-forward **M64-1** (distinct from M53-1, already consumed by phase 54 for H1).
- **The retry-exhausted-reset combination** (both `retry_limit_exceeded_for_log_h2` AND `reset_for_log_h2` set) — un-recon'd on live Envoy; the derive's `URX`-checked-first ordering renders `URX` deterministically regardless.
- **A hard RST vs. the graceful implicit stream-reset** (`drop(send_response)`) this phase uses — un-recon'd whether Envoy renders a different flag on a hard RST; out of scope.
- **The `DC` downstream-disconnect flag** — timing-dependent; rejected at every prior consideration (ADR-0102 through ADR-0118); no new information this session changes that.
- **M57-1's `content-length` header omission**, **M53-2's BEHAVIOR_CONTRACT "(H1)" qualifier**, and **M53-3's un-recon'd retry-on-reset combination** — trivial standalone fixes, deliberately NOT bundled into this phase.
- **A family pivot to xDS CDS/LDS hot-reload or LB `least_request`/`random`** — deferred to a future pick, not rejected outright.
- **Fuzz:** `%RESPONSE_FLAGS%` is an existing operator; the H2 codec itself is unchanged → NO new fuzz target, `ci.yml` unchanged (SPEC §J).

## Scope / gate summary

- **Task count:** 9 tasks (~350-450 LoC — see §3 item 6 above for the itemized estimate). **§6.1 split does NOT fire** (well under ~25 tasks / ~1500 LoC). **ADR-0122 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the standing lapsed-reservation convention, unless a future §6.2 reconciliation for THIS phase needs it — not expected; this session's own re-verification, §3 item 5, already confirmed the one make-or-break design fact empirically).
- **No new** `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant. `#![forbid(unsafe_code)]` holds.
- **Additive invariant:** all `0001`-`0068` fixtures stay byte-identical (§3 item 2 above; re-verified Task 8 Step 6). Only the new boolean-gated `UC` arm changes behavior, gated on `AcquireOutcome::Sent(Err(e))` — a path NO existing fixture reaches.
- **Acceptance (re-run at state-4, SPEC §5):** (a) fixture `0069` green (cross-proxy-equal status `503` + whole-line `{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UC"}`) + (b) all `0001`-`0068` green simultaneously + (c) h2spec ≥95% (no H2 codec/framing change) + (d) no new fuzz target (SPEC §J) + (e) build/clippy/fmt/test/deny clean + (f) `REVIEW.md` approved. **Fixture `0069` is a backend-spawning fixture → expect LOCAL-RED on this dev host and GREEN on CI — CI is AUTHORITATIVE.**
- **Carry-forwards:** this phase CLOSES **M56-1** (consumes the FINAL `UC` slice — no H2 `%RESPONSE_FLAGS%` value remains open) and opens **M64-1** (the H2-side deterministic `UC` rcd, deferred). M57-1 + M55-1 + M53-2 + M53-3 + M48-2 + M42-1 + the `DC`/retry-budget-overflow slices of M45-2 + the phase-58 candidate carry-forward (H2 request-budget arm's own differential fixture) + M40-1 + M39-1/M39-2 + M38-1/M38-2 + CF-39-1 + the HTTP-filters-family (1)-(4) + older stay live; NONE blocks.

_The state-3 implementation (`superpowers:executing-plans` or `superpowers:subagent-driven-development`) is the session AFTER this PLAN lands. Per §5.1, one state per session: this session writes the PLAN only._
