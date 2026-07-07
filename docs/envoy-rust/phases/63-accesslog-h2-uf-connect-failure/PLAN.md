# Phase 63 — `63-accesslog-h2-uf-connect-failure` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Every implementation task uses superpowers:test-driven-development — write the failing test FIRST. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Witness the FIFTH H2 `%RESPONSE_FLAGS%` value, `UF` (UpstreamConnectionFailure), byte-exact on the H2 upstream-connect-refused path, AND correct envoy-rust's H2 connect-failure synth status from `502`→`503` to match upstream Envoy — via a NEW fixture `0068`.

**Architecture:** Mirrors H1's phase-52 `UF` witness (the closest analogue — same status-fix + boolean-discriminator + 2-existing-test-update shape) adapted to H2's phase-61 threading pattern (a new parameter through `finalize_h2_stream`'s sole call site). Four surgical edits in `crates/envoy-http2/src/hcm.rs`: (1) a NEW `synth_h2_connect_failure()` helper (503) redirecting the SOLE `AcquireOutcome::ConnectFailure` match arm (today `synth_h2_502()`); (2) a NEW per-stream boolean `connect_failure_for_log_h2`, set post-loop from a NEW loop-scoped `final_outcome_h2: Option<AttemptOutcome>` capture (the H2 loop's `break` carries no outcome, mirroring H1's `final_outcome` capture exactly) when the FINAL attempt was `AttemptOutcome::ConnectFailure`; (3) thread the boolean through `finalize_h2_stream`'s signature + its ONE call site (phase 61's own new-parameter precedent); (4) an `else if connect_failure_for_log_h2 { "UF" }` branch on the H2 `%RESPONSE_FLAGS%` derive, ordered AFTER `URX`. Plus 2 existing-test status updates (502→503), a NEW in-process backstop, a NEW differential fixture `0068` (the `0066` H2C/H2-upstream shape minus `circuit_breakers` — the `0060` delta pattern), a NEW differential test, and the BEHAVIOR_CONTRACT update.

**Tech Stack:** Rust, `crates/envoy-http2` (HCM), `crates/envoy-config` (`AttemptOutcome`), `crates/envoy-accesslog` (FileSink/json_format), the `h2` crate (client handshake, via the existing `drive_h2_once` test helper), the `tests/differential` `http2_access_log_byte_exact` driver.

## Global Constraints

- `#![forbid(unsafe_code)]` holds — no `unsafe` anywhere in this phase.
- NO new `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant (SPEC §2).
- Load-bearing additivity invariant: all `0001`-`0067` fixtures stay byte-identical (SPEC §2, re-verified §3 item 2 below).
- No new fuzz target (SPEC §M — `%RESPONSE_FLAGS%` is an existing operator; no H2 codec/framing change).

---

## §3 PLAN-VERIFY re-confirmation (done this session, before authoring tasks)

All seven SPEC §3 items were re-checked against the live tree (no drift found):

1. **Line numbers confirmed exact** (all re-greped fresh this session — SEVERAL have drifted from the SPEC's own citations, since phase 61's edits shifted the file): the three connect-acquisition branches producing `AcquireOutcome::ConnectFailure` — H1-upstream-fork dial `hcm.rs:253`-`270` (warn `:262`-`267`, `ConnectFailure` at `:268`), H2-pool-manager `acquire()` `:272`-`312` (warn `:287`-`291`, `ConnectFailure` at `:293`), no-pool-wired per-call fallback `:313`-`333` (warn `:323`-`328`, `ConnectFailure` at `:330`) — SPEC cited `:253`-`336`, confirmed the outer match spans exactly `:252`-`336`. The SOLE synth call site for the `ConnectFailure` outcome: `hcm.rs:396`-`406` (`AcquireOutcome::ConnectFailure => { … response: synth_h2_502(), … outcome: Some(envoy_config::AttemptOutcome::ConnectFailure) … }`) — SPEC's `:396`-`406` confirmed exact. The `Sent(Err(e))`/Reset sibling arm (UNCHANGED, deferred `UC` slice): `:384`-`395`. The `*_for_log_h2` locals block: `upstream_host_for_log_h2` at `:536`, `response_code_details_for_log_h2` at `:540`, `upstream_cluster_for_log_h2` at `:545`, `retry_limit_exceeded_for_log_h2` at `:553` — SPEC's `:536`-`553` confirmed exact (the new boolean is added immediately after `:553`). The loop-scoped `final_retriable` declaration: `:675` (`#[allow(unused_assignments)] let mut final_retriable = false;` at `:674`-`675`) — the new `final_outcome_h2` local is declared immediately adjacent (mirrors H1's placement next to its own `final_retriable`). The per-iteration `final_retriable` assignment: `:756`-`761` (SPEC's `:756` confirmed exact — the capture of `final_outcome_h2 = attempt.outcome;` is inserted immediately before this `match`). The post-loop retry-outcome-counter split: `:823`-`834` (SPEC's `:815`-`834` confirmed — the exact `retry_limit_exceeded_for_log_h2 = true;` set-site is `:830`; the new `connect_failure_for_log_h2 = …` assignment is added immediately after this `if`-block, at `:835`-ish, before `drop(retry_guard_slot);` at `:838`). `finalize_h2_stream`'s SOLE call site: `:865`-`880` (the `.await` continuation lands after; SPEC's `:865`-ish confirmed) and its `async fn` signature: `:895`-`928` (body opens `:929`) — SPEC's citation confirmed, both are the ONLY sites (re-grep below). The H2 `%RESPONSE_FLAGS%` derive: `:1009`-`1018` (SPEC's `:985`-`1018` confirmed — the derive itself is `:1009`-`1018`, the surrounding comment block starts `:994`). The synth-helper cluster: `synth_h2_502` at `:1108`-`1118`, `synth_h2_no_healthy_upstream` at `:1130`-`1140`, `synth_h2_overflow` at `:1153`-`1168` — SPEC's `:1108`-`1168` confirmed exact (the new `synth_h2_connect_failure()` is inserted between `synth_h2_502` and `synth_h2_no_healthy_upstream`, i.e. after `:1118`). The two existing 502-asserting tests: `h2_connect_failure_retried_on_connect_failure_policy` at `:4651`-`4690` (assertion `:4666`-`4669`) and `h2_connect_failure_synth_does_not_tick_upstream_rq_5xx` at `:4700`-`4717` (assertion `:4706`) — SPEC's `:4651`-`4717` confirmed exact.
2. **Additivity re-grep confirmed.** `grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1"` over every existing H2-listener fixture's `envoy.yaml`/`envoy-rust.yaml` (`0009`, `0010`, `0018`, `0021`, `0064`, `0065`, `0066`, `0067`) finds: `0021` has `circuit_breakers.max_connections: 4` (headroom only, real reachable backend — never triggers `ConnectFailure`); `0065` mentions `127.0.0.1:1` only in a COMMENT explaining it is "NEVER dialed" (the `pick()->None` no-healthy-upstream arm fires before any dial — a `metadata_match` NO_FALLBACK subset-miss, confirmed by reading the fixture: the literal address is configured but excluded from the eligible set pre-dial); `0066` has `circuit_breakers.max_pending_requests: 0` gating a PRE-connect pending-reject (never reaches the dial); `0067` has `retry_policy` (an always-503 REAL upstream, never a connect failure). NONE of `0009`/`0010`/`0018`/`0064` carries any of these tokens. **No existing H2 fixture reaches the `AcquireOutcome::ConnectFailure` arm today** — confirmed re-derived fresh this session, not trusted from SPEC's enumeration. The additivity invariant holds.
3. **`finalize_h2_stream` call-site count confirmed.** `grep -n "finalize_h2_stream("` over `crates/envoy-http2/src/hcm.rs` returns exactly TWO hits: the `async fn finalize_h2_stream(` declaration (`:895`) and its ONE call site (`:865`). The single-new-`bool`-parameter form (phase 61's own precedent) is adopted.
4. **The exact 2 pre-existing `502`-asserting H2 tests re-confirmed.** `grep -n "status, 502"` in `crates/envoy-http2/src/hcm.rs` returns exactly 2 hits: `:4667` (inside `h2_connect_failure_retried_on_connect_failure_policy`) and `:4706` (inside `h2_connect_failure_synth_does_not_tick_upstream_rq_5xx`, as `assert_eq!(status, 502, …)`). No third hit under any other literal form (`== 502`, etc.) — a supplementary `grep -n "502"` over the file's test module shows every other `502` mention is inside these same two tests' doc comments/assertion messages (also updated by Task 1). The `AcquireOutcome::ConnectFailure` warn strings (`:266`/`:291`/`:328`, "returning 502") and the arm's own comments (`:387` Reset-sibling — unchanged; `:399` ConnectFailure arm — updated) are separate from the 2 status-asserting tests and are swept in Task 1 alongside them.
5. **§K backstop-helper-reuse shape DECIDED: a small adaptation, NOT verbatim reuse of `h2_hcm_config_with_retry`.** `h2_hcm_config_with_retry` (`hcm.rs:4231`-`4277`) wraps its built `Http1HCMConfig` directly in `Arc::new(...)` inside the helper — there is no exposed intermediate value to mutate `access_log` on afterward (unlike H1's `HCMConfig` struct literal, which sets `access_log` inline). Verbatim reuse is therefore not possible without an awkward `Arc::get_mut` on a freshly-created single-owner `Arc` (fragile, not idiomatic). **Decision:** the backstop builds its own inline `HttpConnectionManagerConfig` + calls `Http1HCMConfig::from_config(...).await` directly (structurally identical to `h2_hcm_config_with_retry`'s OWN body, `retry_policy: None`, `include_attempt_count_in_response: false`), binds the result to a local `built` (NOT immediately wrapped), sets `built.access_log = vec![sink];`, THEN wraps `Arc::new(built)` — exactly mirroring how phase 61's Task 1 Step 1 backstop test built its own inline config (`built.access_log = vec![sink]; let config = Arc::new(built);`) rather than reusing a shared helper. The dead-endpoint cluster reuses `h1_backend_cluster(refused_addr)` verbatim (already used by the two existing connect-failure tests at `:4654`/`:4703`) and the drive reuses `drive_h2_once` verbatim (no new driving code).
6. **§6.1 split decision: does NOT fire.** This PLAN has 7 tasks / an estimated ~230-320 LoC (a ~10-line new synth helper + a 1-line redirect + a 1-line boolean declare + a 1-line loop-scoped-local declare + a 1-line per-iteration capture + a 1-line post-loop set + a 3-line new-parameter thread (call site + signature) + a ~7-line derive wrapper + 2 small existing-test status-flip edits (~4 lines total) + 3 warn-string edits (~3 lines) + one ~80-line in-process backstop test (slightly larger than phase 61's URX backstop since it needs its OWN inline config, not a shared helper) + a 4-file fixture (~120 LoC incl. README, closely modeled on `0066` minus `circuit_breakers`) + a ~25-line differential test + two BEHAVIOR_CONTRACT row edits) — well under the ~25-task/~1500-LoC gate. Slightly larger than phase 61's 5 tasks (`~180-260` LoC) because this phase ALSO needs the new synth helper, the loop-scoped `final_outcome_h2` capture, and 2 existing-test status updates, exactly as SPEC §6.1 projected (~9-13 tasks estimated at SPEC-write time; this PLAN's 7 tasks with more steps-per-task covers the same edit surface — no split needed either way). **ADR-0121 stays reserved-but-unfired** for a §6.2 reconciliation (not expected to fire — the state-0 recon already empirically confirmed the wire shape matches H1's landed `UF` witness exactly).
7. **Fixture number `0068` re-confirmed still next-free.** `ls tests/fixtures/ | sort | tail` shows the highest existing fixture is `0067-accesslog-h2-urx-retry-exhausted`; no sibling session has landed `0068` in between as of this PLAN-write.

No §6.2 reconciliation ADR is needed — none of SPEC §A-§M is overturned by this re-verification.

---

## Task 1: Correct the H2 connect-failure synth status 502→503 (new helper + redirect + the 2 existing-test updates + the 3 warn strings)

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (new helper after `:1118`; redirect arm `:396`-`406`; warns `:266`/`:291`/`:328`; comment `:399`; tests `:4651`-`4690` and `:4700`-`4717`)

This is a pure status correction (no flag/discriminator work yet) — mirrors phase 52's Task 1 (H1's own status-fix task) adapted to H2's single shared call site. The two existing connect-failure tests are the fail-first harness: flip their assertions to expect 503, watch them fail, then land the synth change.

- [ ] **Step 1: Flip the two affected unit-test assertions to expect 503 (the failing tests)**

In `h2_connect_failure_retried_on_connect_failure_policy` (`hcm.rs:4665`-`4669`), change:
```rust
        let (status, _headers) = drive_h2_once(cfg).await;
        assert_eq!(
            status, 502,
            "downstream must be synth-502 after exhausting connect-failure retries"
        );
```
to:
```rust
        let (status, _headers) = drive_h2_once(cfg).await;
        assert_eq!(
            status, 503,
            "downstream must be synth-503 after exhausting connect-failure retries"
        );
```
And its doc comment (`:4643`): `Asserts: downstream synth-502,` → `Asserts: downstream synth-503,`.

In `h2_connect_failure_synth_does_not_tick_upstream_rq_5xx` (`hcm.rs:4705`-`4706`), change:
```rust
        let (status, _headers) = drive_h2_once(cfg).await;
        assert_eq!(status, 502, "downstream must be connect-failure synth-502");
```
to:
```rust
        let (status, _headers) = drive_h2_once(cfg).await;
        assert_eq!(status, 503, "downstream must be connect-failure synth-503");
```
And its doc comment (`:4692`, `:4695`): `a connect-failure synth-502 with NO retry_policy` → `a connect-failure synth-503 with NO retry_policy`; `the synth-502 (kernel-refused connect) never reached an upstream` → `the synth-503 (kernel-refused connect) never reached an upstream`. The `rq_5xx 0` assertion VALUE stays `0` (a connect-failure synth has no real upstream response, so it does not tick `upstream_rq_5xx` regardless of status) — only the doc-comment prose changes.

- [ ] **Step 2: Run the two tests — verify they FAIL**

Run: `cargo test -p envoy-http2 h2_connect_failure_retried_on_connect_failure_policy h2_connect_failure_synth_does_not_tick_upstream_rq_5xx -- --nocapture`
Expected: both FAIL — actual status is `502` (the synth is still 502).

- [ ] **Step 3: Declare `synth_h2_connect_failure()` (§A)**

Insert immediately after `synth_h2_502()`'s closing brace (`hcm.rs:1118`), before `synth_h2_no_healthy_upstream`'s doc comment:
```rust
/// 63 (ADR-0120) §A: the H2 connect-failure synth-503 — corrects the
/// previously-unvalidated `synth_h2_502()` (which this arm called pre-phase-63)
/// to match upstream Envoy's 503 on the connect-refused path. Mirrors
/// `synth_h2_502()`'s exact header shape (`server`, `content-type`, empty
/// body — NO `content-length`, matching the empty-body convention
/// `synth_h2_502` uses) but status 503. Used ONLY at the SOLE
/// `AcquireOutcome::ConnectFailure` match arm (`:396`-`406`), which covers
/// all three connect-acquisition branches (H1-upstream-fork dial, H2-pool-
/// manager `acquire()`, no-pool-wired per-call fallback). `synth_h2_502()`'s
/// OTHER call site (the post-connect send/recv-failure `Reset` arm, `:384`-
/// `395`) is UNCHANGED — still 502, deferred as the continuing M56-1 `UC`
/// slice (a future phase).
fn synth_h2_connect_failure() -> Response {
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

- [ ] **Step 4: Redirect the `AcquireOutcome::ConnectFailure` match arm (§B) + fix its comment**

Replace exactly (`hcm.rs:396`-`406`):
```rust
        AcquireOutcome::ConnectFailure => {
            // Connect-boundary failure (no request bytes left) → classify as
            // ConnectFailure, NOT Reset (mirrors envoy-http1's `run_attempt`).
            // The H2 synth-502 preserves the pre-phase-16 connect-failure shape.
            H2AttemptResult {
                response: synth_h2_502(),
                endpoint: Some(endpoint),
                outcome: Some(envoy_config::AttemptOutcome::ConnectFailure),
                upstream_response: false,
            }
        }
```
with:
```rust
        AcquireOutcome::ConnectFailure => {
            // Connect-boundary failure (no request bytes left) → classify as
            // ConnectFailure, NOT Reset (mirrors envoy-http1's `run_attempt`).
            // 63 (ADR-0120) §B: corrected from the generic synth_h2_502() to
            // the dedicated synth_h2_connect_failure() — matches upstream
            // Envoy's 503 on this path (was a previously-unvalidated 502).
            H2AttemptResult {
                response: synth_h2_connect_failure(),
                endpoint: Some(endpoint),
                outcome: Some(envoy_config::AttemptOutcome::ConnectFailure),
                upstream_response: false,
            }
        }
```

- [ ] **Step 5: Fix the three runtime `tracing::warn!` strings**

At `hcm.rs:266`: `"upstream connect failed (H1 fork) — returning 502"` → `"upstream connect failed (H1 fork) — returning 503"`.
At `hcm.rs:291`: `"H2 pool connect failed — returning 502"` → `"H2 pool connect failed — returning 503"`.
At `hcm.rs:328`: `"upstream connect failed (per-call) — returning 502"` → `"upstream connect failed (per-call) — returning 503"`.

- [ ] **Step 6: Run the two tests — verify they PASS**

Run: `cargo test -p envoy-http2 h2_connect_failure_retried_on_connect_failure_policy h2_connect_failure_synth_does_not_tick_upstream_rq_5xx -- --nocapture`
Expected: both PASS (status is now 503).

- [ ] **Step 7: Run the whole envoy-http2 unit suite — confirm no collateral**

Run: `cargo test -p envoy-http2`
Expected: PASS. (A repo-wide grep confirms ONLY these two tests assert a connect-failure 502 — `:4667`/`:4706`. The Reset/send-fail arm at `:390` is untouched and has no status-asserting test that would flip.)

- [ ] **Step 8: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 63 task 1: H2 connect-failure synth status 502->503 (new helper + redirect + 2 tests + 3 warns) [ADR-0120]"
```

---

## Task 2: Thread `connect_failure_for_log_h2` + render `UF` + the §K in-process backstop

**Files:**
- Modify: `crates/envoy-http2/src/hcm.rs` (locals block after `:553`; loop-scoped decl after `:675`; per-iteration capture before `:756`; post-loop set after `:834`; `finalize_h2_stream` call site `:865`-`880` and signature `:895`-`928`; derive `:1009`-`1018`; new test)

The backstop test is the fail-first harness for the discriminator + the derive branch. It mirrors the H1 backstop `h1_connect_failure_access_log_carries_uf_flag` (`crates/envoy-http1/src/hcm.rs:7592`) in shape and topology (a single kernel-refused connect, `127.0.0.1:1`, NO retry_policy, `{rc,rf}` log — rcd OMITTED), adapted to H2's own inline-config-building convention (§3 item 5 above).

- [ ] **Step 1: Write the failing backstop test**

Insert immediately after `h2_connect_failure_synth_does_not_tick_upstream_rq_5xx`'s closing brace (`crates/envoy-http2/src/hcm.rs:4717`), before the `// ── 17 Task 6` section comment:

```rust
    /// 63 (ADR-0120) §C/§D/§E/§F/§G/§K backstop: drive a single H2
    /// connect-failure attempt (endpoint 127.0.0.1:1, kernel-refused) with NO
    /// retry_policy, wired to a {rc,rf} FILE json access-log. Asserts the
    /// downstream response is the synth-503 (Task 1) AND the logged line
    /// carries the DERIVED rf:"UF" (set post-loop from the connect-failure
    /// final-outcome boolean, NOT rcd-derived — the connect-failure rcd is
    /// the shared "via_upstream"). The sole in-process proof of the
    /// discriminator + the derive branch. H2 mirror of the H1 backstop
    /// `h1_connect_failure_access_log_carries_uf_flag`
    /// (`crates/envoy-http1/src/hcm.rs:7592`). Fail-first: pre-change the
    /// derive's rcd-match falls to `_ => "-"` (via_upstream is unmatched) →
    /// it renders `"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h2_connect_failure_access_log_carries_uf_flag() {
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

        // 127.0.0.1:1 is kernel-refused — a deterministic connect failure.
        let refused_addr: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let (cluster_mgr, _cluster) = h1_backend_cluster(refused_addr).await;
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test-connect-failure".to_string(),
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
        assert_eq!(
            status, 503,
            "connect-failure surfaces the synth-503 downstream"
        );
        let logged = tokio::fs::read_to_string(&log_path).await.unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rf\":\"UF\"}\n",
            "connect-failure access-log line carries rf:UF: {logged:?}"
        );
    }
```

- [ ] **Step 2: Run the backstop — verify it FAILS on `rf`**

Run: `cargo test -p envoy-http2 h2_connect_failure_access_log_carries_uf_flag -- --nocapture`
Expected: FAIL at the final `assert_eq!` — the logged line is `{"rc":503,"rf":"-"}` (status 503 from Task 1, but the derive has no `UF` branch yet so `via_upstream` falls to `_ => "-"`).

- [ ] **Step 3: Declare the discriminator boolean (§C)**

Insert immediately after the existing locals block (`crates/envoy-http2/src/hcm.rs:553`, right after `let mut retry_limit_exceeded_for_log_h2: bool = false;`):

Replace exactly:
```rust
    let mut retry_limit_exceeded_for_log_h2: bool = false;
```
with:
```rust
    let mut retry_limit_exceeded_for_log_h2: bool = false;
    // 63 (ADR-0120): per-stream %RESPONSE_FLAGS% = "UF" (UpstreamConnection-
    // Failure) discriminator. Set true POST-LOOP when the FINAL attempt's
    // outcome was AttemptOutcome::ConnectFailure (a connect-failure RETRIED
    // to success must NOT flag UF — so this reads the final outcome, not a
    // per-attempt set). Like URX, UF is NOT 1:1 with a unique
    // %RESPONSE_CODE_DETAILS% (the connect-failure rcd is the shared
    // "via_upstream"), so it keys on this boolean. Mirrors the H1 phase-52
    // `connect_failure_for_log` local (crates/envoy-http1/src/hcm.rs:854)
    // exactly.
    let mut connect_failure_for_log_h2: bool = false;
```

- [ ] **Step 4: Declare the loop-scoped `final_outcome_h2` capture (§C)**

Insert immediately after the existing `final_retriable` declaration (`crates/envoy-http2/src/hcm.rs:675`):

Replace exactly:
```rust
                    #[allow(unused_assignments)]
                    let mut final_retriable = false;
```
with:
```rust
                    #[allow(unused_assignments)]
                    let mut final_retriable = false;
                    // 63 (ADR-0120): the FINAL attempt's outcome. Captured
                    // each iteration because the loop `break` carries only
                    // (response, upstream_response), NOT attempt.outcome.
                    // Read post-loop to set connect_failure_for_log_h2.
                    // AttemptOutcome is Copy (no move/borrow interaction with
                    // the per-iteration `match attempt.outcome` below).
                    // Mirrors H1's phase-52 `final_outcome` capture
                    // (crates/envoy-http1/src/hcm.rs:974) exactly.
                    #[allow(unused_assignments)]
                    let mut final_outcome_h2: Option<envoy_config::AttemptOutcome> = None;
```

- [ ] **Step 5: Capture `final_outcome_h2` every iteration (§D)**

Insert immediately before the existing `final_retriable = match attempt.outcome { … };` (`crates/envoy-http2/src/hcm.rs:756`):

Replace exactly:
```rust
                        final_retriable = match attempt.outcome {
```
with:
```rust
                        // 63 (ADR-0120): capture the final attempt's outcome
                        // (read post-loop to set connect_failure_for_log_h2).
                        final_outcome_h2 = attempt.outcome;
                        final_retriable = match attempt.outcome {
```

- [ ] **Step 6: Set the boolean post-loop (§E)**

Insert immediately after the existing retry-outcome-counter split block (`crates/envoy-http2/src/hcm.rs:823`-`834`, before `drop(retry_guard_slot);`):

Replace exactly:
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
                    // Release the retry-budget slot now, before building the outgoing response,
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
                    // 63 (ADR-0120): flag UF when the FINAL attempt was a
                    // connect failure — independent of the retry split (a
                    // single connect-failure attempt with no retry_policy
                    // flags it too). A connect-failure retried to success has
                    // final_outcome_h2 = Some(Response) → not flagged. If
                    // BOTH this and retry_limit_exceeded_for_log_h2 are set
                    // (a retry-exhausted-connect-failure — un-recon'd
                    // combination, SPEC §4), the derive's URX-before-UF
                    // ordering renders URX deterministically. Mirrors H1's
                    // phase-52 set-site (crates/envoy-http1/src/hcm.rs:1156)
                    // exactly.
                    connect_failure_for_log_h2 = matches!(
                        final_outcome_h2,
                        Some(envoy_config::AttemptOutcome::ConnectFailure)
                    );
                    // Release the retry-budget slot now, before building the outgoing response,
```

- [ ] **Step 7: Thread the boolean through `finalize_h2_stream`'s call site (§F)**

Replace exactly (`crates/envoy-http2/src/hcm.rs:865`-`880`):
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
        connect_failure_for_log_h2,
    )
    .await
```

- [ ] **Step 8: Thread the boolean through `finalize_h2_stream`'s signature (§F)**

Replace exactly (`crates/envoy-http2/src/hcm.rs:923`-`928`):
```rust
    // Phase 61 (ADR-0118): the retry-limit-exceeded loop-exit discriminator
    // (§B), consumed by the %RESPONSE_FLAGS% derive wrapper (§D). A `Copy`
    // primitive — no lifetime/ownership complications, unlike the
    // `Option<String>` fields above.
    retry_limit_exceeded_for_log_h2: bool,
) -> Result<(), Http2Error> {
```
with:
```rust
    // Phase 61 (ADR-0118): the retry-limit-exceeded loop-exit discriminator
    // (§B), consumed by the %RESPONSE_FLAGS% derive wrapper (§D). A `Copy`
    // primitive — no lifetime/ownership complications, unlike the
    // `Option<String>` fields above.
    retry_limit_exceeded_for_log_h2: bool,
    // Phase 63 (ADR-0120): the connect-failure final-outcome discriminator
    // (§E), consumed by the %RESPONSE_FLAGS% derive wrapper (§G). A `Copy`
    // primitive, same shape as retry_limit_exceeded_for_log_h2 above.
    connect_failure_for_log_h2: bool,
) -> Result<(), Http2Error> {
```

- [ ] **Step 9: Extend the `%RESPONSE_FLAGS%` derive with the boolean-gated wrapper (§G)**

Replace exactly (`crates/envoy-http2/src/hcm.rs:1009`-`1018`):
```rust
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
with:
```rust
        // Phase 63 (ADR-0120): "UF" (UpstreamConnectionFailure) is likewise
        // NOT derivable from %RESPONSE_CODE_DETAILS% (the connect-failure
        // rcd stays the shared "via_upstream") — checked SECOND via the
        // boolean, ORDERED AFTER URX, mirroring the H1 phase-52 wrapper
        // exactly (crates/envoy-http1/src/hcm.rs:1305).
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

- [ ] **Step 10: Run the backstop — verify it PASSES**

Run: `cargo test -p envoy-http2 h2_connect_failure_access_log_carries_uf_flag -- --nocapture`
Expected: PASS — logged line is `{"rc":503,"rf":"UF"}`.

- [ ] **Step 11: Run the whole envoy-http2 suite — confirm no collateral on the NR/UH/UO/URX backstops**

Run: `cargo test -p envoy-http2`
Expected: PASS — in particular `h2_route_miss_access_log_carries_nr_flag`, `h2_host_miss_access_log_carries_nr_flag`, `h2_no_healthy_access_log_carries_uh_flag`, `h2_pool_overflow_access_log_carries_uo_flag`, `h2_request_budget_overflow_access_log_carries_uo_flag`, and `h2_retry_limit_exceeded_access_log_carries_urx_flag` are unchanged (none of their paths sets `connect_failure_for_log_h2`); the two Task-1-flipped tests stay green at 503.

- [ ] **Step 12: Commit**

```bash
git add crates/envoy-http2/src/hcm.rs
git commit -m "phase 63 task 2: thread connect_failure_for_log_h2 + render UF + in-process backstop [ADR-0120]"
```

---

## Task 3: New differential fixture `0068-accesslog-h2-uf-connect-failure`

**Files:**
- Create: `tests/fixtures/0068-accesslog-h2-uf-connect-failure/envoy.yaml`
- Create: `tests/fixtures/0068-accesslog-h2-uf-connect-failure/envoy-rust.yaml`
- Create: `tests/fixtures/0068-accesslog-h2-uf-connect-failure/expectations.yaml`
- Create: `tests/fixtures/0068-accesslog-h2-uf-connect-failure/README.md`

The `0066` H2C-listener / H2-upstream-cluster shape **minus `circuit_breakers`** (so envoy-rust DIALS the dead endpoint and the kernel refuses → the connect-failure synth-503, instead of `0066`'s pre-connect pending-gate reject) — the SAME delta H1's `0060` applied to `0058`. `json_format` reduced to `{method, proto, rc, rf}` (the connect-failure rcd is non-deterministic — OMITTED, mirroring `0060`). No backend spawned (literal `127.0.0.1:1`, no `{{BACKEND_*}}` marker).

- [ ] **Step 1: Create `envoy-rust.yaml`** (subject side — NO `admin` block, bind `127.0.0.1`, mount path `/tmp/0068-envoy-rust-mount/access.log`)

```yaml
node: { id: envoy-rust-phase-63-fixture-0068, cluster: envoy-rust-phase-63 }
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
                      path: /tmp/0068-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rf: "%RESPONSE_FLAGS%"
                          method: "%REQ(:METHOD)%"
                          proto: "%PROTOCOL%"
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
    # STATIC cluster, H2 upstream (typed_extension_protocol_options), NO
    # circuit_breakers and NO retry_policy. The single endpoint is the
    # LITERAL unreachable 127.0.0.1:1 — DIALED on the first request (no
    # pending-gate to reject pre-connect, UNLIKE 0066): the kernel refuses
    # the connect → the connect-failure synth-503 (rf:"UF"). A literal
    # address (not a {{BACKEND_*}} marker) keeps the cluster byte-identical
    # across both files with NO backend spawned (the 0060/0065/0066
    # discipline).
    - name: backend_cluster
      type: STATIC
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
                    socket_address: { address: 127.0.0.1, port_value: 1 }
```

- [ ] **Step 2: Create `envoy.yaml`** (reference side — prepend `admin:`, bind `0.0.0.0`, mount path `/tmp/0068-envoy-mount/access.log`; otherwise byte-identical)

```yaml
node: { id: envoy-rust-phase-63-fixture-0068, cluster: envoy-rust-phase-63 }
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
                      path: /tmp/0068-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rf: "%RESPONSE_FLAGS%"
                          method: "%REQ(:METHOD)%"
                          proto: "%PROTOCOL%"
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

- [ ] **Step 3: Create `expectations.yaml`** (one probe: H2 connect-failure 503)

```yaml
driver:
  kind: http2_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0068-envoy-mount/access.log
    envoy_rust: /tmp/0068-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / (any :authority — domains: ["*"]) routed to
    # `backend_cluster`, a STATIC H2-upstream cluster with NO
    # circuit_breakers and NO retry_policy and one LITERAL dead endpoint
    # 127.0.0.1:1. Both proxies DIAL it; the kernel refuses → the
    # connect-failure synth-503. This is the FIFTH non-`-` H2
    # %RESPONSE_FLAGS% witness: UF (UpstreamConnectionFailure) (phase 63,
    # ADR-0120), the H2 analogue of fixture 0060 (phase 52).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). NO static
    # literal: the `http2_access_log_byte_exact` driver asserts every line is
    # byte-identical between upstream Envoy v1.33.0 and envoy-rust. The
    # fixture logs ONLY {method, proto, rc, rf} — the connect-failure
    # %RESPONSE_CODE_DETAILS% AND the response body carry the OS-derived
    # transport-failure reason (non-deterministic across environments —
    # M45-2), so rcd is OMITTED and the driver does not compare the body.
    # envoy-rust returns 503 (Task 1 corrected the unvalidated 502) and
    # DERIVES %RESPONSE_FLAGS% = UF from the connect-failure final-outcome
    # boolean (NOT from rcd).
    # state-0 recon (live v1.33.0, digest sha256:56da5afd…, byte-stable
    # across 3 repeats + a restart): status 503, rf "UF" — IDENTICAL to the
    # H1 UF witness (fixture 0060).
    #
    # Keys sort by UTF-8 byte order (ADR-0094 §A): method, proto, rc, rf.
    # The emitted line is:
    #   {"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `README.md`**

```markdown
# Fixture 0068 — H2 access-log `%RESPONSE_FLAGS%` connect-failure path (`UF`, byte-exact)

The H2 analogue of fixture `0060` (phase 52, the H1 `UF` witness) and the
FIFTH fixture built on `Driver::Http2AccessLogByteExact` (opened by phase 56,
fixture `0064`; extended by phase 57's `0065`, phase 58's `0066`, and phase
61's `0067`). Phase 63 (ADR-0120) witnesses the FIFTH H2 `%RESPONSE_FLAGS%`
value, `UF` (UpstreamConnectionFailure), byte-exact on the H2
upstream-connect-refused 503 path.

## What this proves

Before this phase, envoy-rust's H2 `AcquireOutcome::ConnectFailure` arm
emitted a generic `synth_h2_502()` — a genuine, previously-unvalidated status
divergence (upstream Envoy returns 503 here), the SAME class of bug H1's
phase 52 fixed for the H1 side. Phase 63 (i) corrects the status via a new
`synth_h2_connect_failure()` helper (503), (ii) declares a new per-stream
boolean discriminator set post-loop from a new loop-scoped final-outcome
capture (the H2 loop's `break` carries no outcome, mirroring the H1
`final_outcome` capture), (iii) threads it through `finalize_h2_stream`'s
sole call site, and (iv) extends the H2 `%RESPONSE_FLAGS%` derive with a
boolean-gated `UF` branch ordered AFTER `URX`.

## Probe

| # | request (H2, `:authority` = `envoy-rust.test`) | arm | emitted JSON object (byte-identical on both sides) |
|---|---|---|---|
| 1 | `GET /` | connect-failure (kernel-refused `127.0.0.1:1`) | see below |

```
{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}
```

The cluster is the `0066` shape (STATIC, H2-upstream via
`typed_extension_protocol_options`) **minus `circuit_breakers`** — the SAME
delta H1's `0060` applied to `0058`. Without the pending-gate, envoy-rust
DIALS the literal dead endpoint and the kernel refuses the connect,
triggering the `AcquireOutcome::ConnectFailure` arm (rather than `0066`'s
pre-connect `PoolError::PendingOverflow` reject).

## Driver

`kind: http2_access_log_byte_exact` (`Driver::Http2AccessLogByteExact`,
opened at phase 56) — NO harness driver change this phase. **NO
harness backend-wiring allowlist edit needed** (unlike phase 61's `0067`) —
the fixture's literal dead endpoint carries no `{{BACKEND_PORT}}` marker, so
`scan_needs_marker`'s `needs_backend` gate stays `false` automatically,
mirroring fixture `0060`'s (and `0066`'s) NO-backend-spawned simplicity.

## `0001`-`0067` byte-preservation

This phase's changes are additive — gated on the `AcquireOutcome::ConnectFailure`
arm, which requires a dead/refused endpoint reached via an ACTUAL dial (no
`circuit_breakers` pending-gate and no `pick()->None` short-circuit ahead of
it). NONE of the pre-existing H2 fixtures (`0009`, `0010`, `0018`, `0021`,
`0064`, `0065`, `0066`, `0067`) reaches this arm — re-confirmed by a fresh
`grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1"` over each
`envoy-rust.yaml` this session (`0021`'s `circuit_breakers` gates a reachable
backend; `0065`'s `127.0.0.1:1` is excluded pre-dial by a subset-miss;
`0066`'s `circuit_breakers` pending-gate rejects pre-connect; `0067`'s
`retry_policy` drives a REAL always-503 upstream) — so `0001`-`0067` stay
byte-identical; only the new `0068` observes the new `rf:"UF"` witness.

## Cross-references

- ADR: ADR-0120 (state-1 brainstorm + state-2 PLAN — the H2 `UF` witness).
- Related fixtures: `0060` (the H1 `UF` witness this fixture mirrors on H2);
  `0064`/`0065`/`0066`/`0067` (the H2 `NR`/`UH`/`UO`/`URX` witnesses that
  opened/extended `Driver::Http2AccessLogByteExact`).
- Carry-forward: **M56-1** — ONLY `UC` remains open (the last H2
  `%RESPONSE_FLAGS%` value) + the H2 failure-path `%RESPONSE_CODE_DETAILS%`
  strings beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`, still
  open for a future phase.
```

- [ ] **Step 5: Confirm the YAML pair diff matches the established discipline**

Run: `diff tests/fixtures/0068-accesslog-h2-uf-connect-failure/envoy.yaml tests/fixtures/0068-accesslog-h2-uf-connect-failure/envoy-rust.yaml`
Expected: exactly three hunks — the `admin:` line (envoy-only), `0.0.0.0` vs `127.0.0.1` listener bind, and `0068-envoy-mount` vs `0068-envoy-rust-mount` log path. Nothing else (the cluster/route/json_format are byte-identical).

- [ ] **Step 6: Commit**

```bash
git add tests/fixtures/0068-accesslog-h2-uf-connect-failure/
git commit -m "phase 63 task 3: fixture 0068-accesslog-h2-uf-connect-failure (one probe, H2 rf:UF byte-exact) [ADR-0120]"
```

---

## Task 4: Differential test `access_log_h2_uf_connect_failure.rs`

**Files:**
- Create: `tests/differential/tests/access_log_h2_uf_connect_failure.rs` (a structural clone of `access_log_h2_rf_overflow.rs`, pointing at the `0068` fixture)

A thin `differential::run_fixture(&dir)` wrapper. NO harness backend-wiring allowlist edit needed (§3 item 2 above / SPEC §I — the fixture's literal dead endpoint carries no `{{BACKEND_PORT}}` marker).

- [ ] **Step 1: Write the differential test wrapper**

```rust
//! Docker-gated differential test for fixture
//! 0068-accesslog-h2-uf-connect-failure.
//! Phase 63 (ADR-0120) — the FIFTH H2 `%RESPONSE_FLAGS%` witness: `UF`
//! (UpstreamConnectionFailure), byte-exact cross-proxy on the H2
//! upstream-connect-refused 503 path — the H2 analogue of fixture `0060`
//! (phase 52). A STATIC H2-upstream cluster with NO circuit_breakers and NO
//! retry_policy and a single dead endpoint (`127.0.0.1:1`): both proxies
//! DIAL it, the kernel refuses the connect → the connect-failure synth-503.
//! envoy-rust now (a) returns 503 (Task 1 corrected the unvalidated 502) and
//! (b) DERIVES `%RESPONSE_FLAGS%` = `UF` from the connect-failure
//! final-outcome boolean (NOT from `%RESPONSE_CODE_DETAILS%`, which — like
//! the response body — carries the non-deterministic OS transport-failure
//! reason and is NOT logged / NOT compared). Spawns Envoy v1.33 in a
//! container; spawns envoy-rust as a subprocess; drives
//! `kind: http2_access_log_byte_exact` (reusing the phase-56 driver
//! verbatim); reads each side's file access-log and asserts the emitted
//! line is byte-identical:
//!   {"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}
//! PURE cross-proxy equality (no static literal).

use std::path::PathBuf;

#[tokio::test]
async fn access_log_h2_uf_connect_failure() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0068-accesslog-h2-uf-connect-failure");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Confirm it compiles (the differential crate builds)**

Run: `cargo build -p differential --tests`
Expected: clean build (the new test binary compiles; the `0068` fixture deserializes against the existing `Driver::Http2AccessLogByteExact`).

> **NOTE (host-environment):** the Docker differential (real Envoy vs envoy-rust) is **CI-authoritative** at the state-4 §7.5 gate (memory `envoy-rust-state4-ci-first-execution`). This host may not run the container reliably; do NOT treat a local Docker-gated non-run as a failure. The differential `0068` green + all `0001`-`0067` still green are confirmed by the state-4 CI run.

- [ ] **Step 3: Rebuild the debug envoy-bin (the differential runs `target/debug/envoy-bin`)**

Run: `cargo build -p envoy-bin`
Expected: clean build — so that if the differential IS exercised (CI or a working-Docker host) it picks up the 503/`UF` change rather than a stale binary (memory `differential-harness-uses-debug-envoy-bin`).

- [ ] **Step 4: Commit**

```bash
git add tests/differential/tests/access_log_h2_uf_connect_failure.rs
git commit -m "phase 63 task 4: differential test access_log_h2_uf_connect_failure (fixture 0068) [ADR-0120]"
```

---

## Task 5: BEHAVIOR_CONTRACT updates (§L)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `%RESPONSE_FLAGS%` row, `:1020`; the `%RESPONSE_CODE_DETAILS%` row, `:1031`)

- [ ] **Step 1: Update the `%RESPONSE_FLAGS%` row's H2-witness sentence**

Replace exactly (the trailing H2 sentence of the row — a substring of the giant single-line row at `:1020`, unique in the file):

```
`URX` is now ALSO witnessed byte-exact on H2 by fixture **0067** (phase 61, ADR-0118) — set via the retry-loop's post-loop limit-exceeded exit boolean (NOT derivable from `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` on this path — the SAME non-rcd-derivable pattern H1 established at phase 51), threaded through `finalize_h2_stream` as a new parameter — ADVANCING carry-forward **M56-1** (the `URX` slice consumed). UNLIKE phases 50/57, NO status-code correction was needed — envoy-rust's H2 retry-limit-exceeded mechanics (status 503, `x-envoy-attempt-count: 2`, all four retry counters) were ALREADY correct and ALREADY covered by an existing phase-16 in-process test. The remaining H2 `%RESPONSE_FLAGS%` values (`UF`/`UC`) remain deferred as the continuing carry-forward **M56-1**, witnessable one-at-a-time by future phases exactly as phases 49-54 did for H1 after phase 48 built the H1 `NR` pattern.
```

with:

```
`URX` is now ALSO witnessed byte-exact on H2 by fixture **0067** (phase 61, ADR-0118) — set via the retry-loop's post-loop limit-exceeded exit boolean (NOT derivable from `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` on this path — the SAME non-rcd-derivable pattern H1 established at phase 51), threaded through `finalize_h2_stream` as a new parameter — ADVANCING carry-forward **M56-1** (the `URX` slice consumed). UNLIKE phases 50/57, NO status-code correction was needed — envoy-rust's H2 retry-limit-exceeded mechanics (status 503, `x-envoy-attempt-count: 2`, all four retry counters) were ALREADY correct and ALREADY covered by an existing phase-16 in-process test. `UF` is now ALSO witnessed byte-exact on H2 by fixture **0068** (phase 63, ADR-0120) — set via a NEW loop-scoped final-outcome capture + a post-loop boolean (NOT derivable from `%RESPONSE_CODE_DETAILS%`, which stays the shared `via_upstream` on this path, exactly as H1's phase-52 `UF` found), threaded through `finalize_h2_stream` as a second new parameter, ordered AFTER `URX` in the derive — ADVANCING carry-forward **M56-1** (the `UF` slice consumed, leaving ONLY `UC`). UNLIKE `URX`, this phase ALSO corrected a genuine status-code divergence — envoy-rust's H2 connect-failure arm previously emitted a previously-unvalidated `502` (via the generic `synth_h2_502()`); it now emits `503` via a dedicated `synth_h2_connect_failure()` helper, matching upstream Envoy. The remaining H2 `%RESPONSE_FLAGS%` value (`UC`) remains deferred as the continuing carry-forward **M56-1**, witnessable by a future phase exactly as phase 53 did for H1 after phase 52 built the H1 `UF` pattern.
```

- [ ] **Step 2: Update the `%RESPONSE_CODE_DETAILS%` row's H2-witness sentence**

Replace exactly (a substring of the giant single-line row at `:1031`, unique in the file):

```
The H2 retry-limit-exceeded path (fixture **0067**, phase 61, ADR-0118) is now ALSO witnessed on H2, but its `%RESPONSE_CODE_DETAILS%` stays the shared `via_upstream` (a REAL completing 503, unchanged) — `%RESPONSE_FLAGS%`=`URX` is the discriminating signal there, NOT this field. The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`) remain deferred as the continuing carry-forward **M56-1**.
```

with:

```
The H2 retry-limit-exceeded path (fixture **0067**, phase 61, ADR-0118) is now ALSO witnessed on H2, but its `%RESPONSE_CODE_DETAILS%` stays the shared `via_upstream` (a REAL completing 503, unchanged) — `%RESPONSE_FLAGS%`=`URX` is the discriminating signal there, NOT this field. The H2 connect-failure path (fixture **0068**, phase 63, ADR-0120) is now ALSO witnessed on H2, but its `%RESPONSE_CODE_DETAILS%` ALSO stays the shared `via_upstream` AND carries the OS-derived non-deterministic transport-failure reason (M45-2) — `%RESPONSE_FLAGS%`=`UF` is the discriminating signal there, NOT this field (the rcd is OMITTED from the fixture entirely, mirroring the H1 `0060` precedent). The remaining H2 failure-path details (beyond `route_not_found`/`no_healthy_upstream`/`{overflow}`) remain deferred as the continuing carry-forward **M56-1**.
```

- [ ] **Step 3: Commit**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 63 task 5: BEHAVIOR_CONTRACT rf/rcd rows — H2 UF witnessed (fixture 0068) [ADR-0120]"
```

---

## Task 6: Local verification sweep (state-3 close-out; full §7.5 gate runs at state-4)

This is the developer's local pre-flight — NOT the state-4 verification gate (that re-runs the full §7.5 set in CI and quotes outputs to `PROGRESS.md`). Run the cheap-and-local subset; the Docker differential + `0001`-`0067` byte-identical + h2spec are CI-authoritative at state-4.

**Files:** none (verification only)

- [ ] **Step 1: Workspace build (all targets)**

Run: `cargo build --workspace --all-targets`
Expected: clean.

- [ ] **Step 2: clippy clean**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: no warnings. (Watch for an `unused_assignments` lint on `final_outcome_h2` — if it fires, confirm the `#[allow(unused_assignments)]` from Task 2 Step 4 is present, mirroring `final_retriable`.)

- [ ] **Step 3: fmt clean**

Run: `cargo fmt --all -- --check`
Expected: clean. (If any inserted block reflows, run `cargo fmt --all` and re-commit — memory `envoy-rust-state4-ci-first-execution`: CI is often red-at-fmt mid-phase.)

- [ ] **Step 4: full workspace unit tests** (non-Docker)

Run: `cargo test --workspace`
Expected: PASS (the new backstop + the two flipped connect-failure status tests + all existing tests; the differential Docker tests are Docker-gated and skip locally per the harness's own gating — do not treat a local skip as a failure).

- [ ] **Step 5: cargo deny**

Run: `cargo deny check`
Expected: clean. (If a fresh RustSec advisory reds an existing dep — NOT a phase regression — patch-bump it per memory `cargo-deny-reds-on-unrelated-advisory`.)

- [ ] **Step 6: confirm byte-preservation reasoning (no existing H2 fixture regressed)**

Run: `for f in 0009 0010 0018 0021 0064 0065 0066 0067; do echo "=== $f ==="; grep -n "circuit_breakers\|retry_policy\|127.0.0.1:1" tests/fixtures/${f}-*/envoy-rust.yaml || echo "(none)"; done`
Expected: matches §3 item 2's re-derivation above (only `0021`'s headroom `circuit_breakers`, `0065`'s excluded-pre-dial comment mention, `0066`'s pending-gate `circuit_breakers`, `0067`'s `retry_policy` — NONE reaches `AcquireOutcome::ConnectFailure`); `0001`-`0067` stay byte-identical; only `0068` observes the new `rf:"UF"` witness.

- [ ] **Step 7: final fmt-fix commit if needed** (otherwise nothing to commit)

```bash
cargo fmt --all
git add -A && git commit -m "phase 63: cargo fmt [ADR-0120]" || echo "nothing to reformat"
```

---

## Task 7: PROGRESS.md + handoff to state-4

**Files:**
- Create: `docs/envoy-rust/phases/63-accesslog-h2-uf-connect-failure/PROGRESS.md`

> **NOTE:** Tasks 1–6 are the STATE-3 implementation session(s) — NOT this state-2 PLAN-write session (§5.1: one state per session). This task closes the state-3 arc by recording the running log; the state-4 verification (the full §7.5 gate, quoting all command outputs into PROGRESS.md) is the session AFTER.

- [ ] **Step 1: Write PROGRESS.md**

Record, per task: what landed, the exact files touched, and the local command outputs (Tasks 1/2/6). Note explicitly that the Docker differential `0068` (Task 3/4) is deferred to the state-4 CI gate (host limitation), and that ADR-0121 did NOT fire (the §3 PLAN-VERIFY re-confirmation confirmed all §A–§M facts).

- [ ] **Step 2: Commit**

```bash
git add docs/envoy-rust/phases/63-accesslog-h2-uf-connect-failure/PROGRESS.md
git commit -m "phase 63: PROGRESS.md — state-3 implementation log [ADR-0120]"
```

---

## Out of scope (deferred — do NOT implement)

- **The Reset/send-fail arm (`hcm.rs:384`-`395`, `synth_h2_502()`'s OTHER call site)** — a different post-connect path → Envoy's `UC` flag, un-recon'd trigger; stays 502. The continuing carry-forward **M56-1**, narrowed by this phase to ONLY `UC`.
- **The retry-exhausted-connect-failure combination** (both `retry_limit_exceeded_for_log_h2` AND `connect_failure_for_log_h2` set) — un-recon'd on live Envoy (mirrors H1's own un-recon'd combination); the derive's `URX`-checked-first ordering renders `URX` deterministically regardless.
- **The H2 request-budget arm's own differential access-log witness** — unrelated surface, not folded into this phase; remains a candidate future carry-forward slice.
- **The `DC` downstream-disconnect flag** — timing-dependent; rejected at every prior consideration (ADR-0102 through ADR-0120); no new information this session changes that.
- **M53-2's BEHAVIOR_CONTRACT "(H1)" qualifier** and **M57-1's `content-length` header omission** — trivial standalone fixes, not bundled into this phase.
- **Fuzz:** `%RESPONSE_FLAGS%` is an existing operator; no new operator/grammar → NO new fuzz target, `ci.yml` unchanged.

## Scope / gate summary

- **Task count:** 7 tasks (~230-320 LoC — see §3 item 6 above for the itemized estimate). **§6.1 split does NOT fire** (well under ~25 tasks / ~1500 LoC). **ADR-0121 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the standing lapsed-reservation convention, unless a future §6.2 reconciliation for THIS phase needs it — not expected).
- **No new** `Op` / `AccessLogRecord` field / crate / dependency / `ConfigError` variant. `#![forbid(unsafe_code)]` holds.
- **Additive invariant:** all `0001`-`0067` fixtures stay byte-identical (§3 item 2 above; re-verified Task 6 Step 6). Only the new boolean-gated `UF` arm changes behavior, gated on `AcquireOutcome::ConnectFailure` — a path NO existing fixture reaches.
- **Acceptance (re-run at state-4, SPEC §5):** (a) fixture `0068` green (cross-proxy-equal status `503` + whole-line `{"method":"GET","proto":"HTTP/2","rc":503,"rf":"UF"}`) + (b) all `0001`-`0067` green simultaneously + (c) h2spec ≥95% (no H2 codec/framing change) + (d) no new fuzz target (SPEC §M) + (e) build/clippy/fmt/test/deny clean (including the Task 1 status-update to the 2 existing tests) + (f) `REVIEW.md` approved.
- **Carry-forwards:** this phase ADVANCES **M56-1** (consumes the `UF` slice; ONLY `UC` stays open). M57-1 + M55-1 + M53-2 + M53-3 + M48-2 + M42-1 + the `DC`/retry-budget-overflow slices of M45-2 + the phase-58 candidate carry-forward (H2 request-budget arm's own differential fixture) + M40-1 + M39-1/M39-2 + M38-1/M38-2 + CF-39-1 + the HTTP-filters-family (1)-(4) + older stay live; NONE blocks.

_The state-3 implementation (`superpowers:executing-plans` or `superpowers:subagent-driven-development`) is the session AFTER this PLAN lands. Per §5.1, one state per session: this session writes the PLAN only._
