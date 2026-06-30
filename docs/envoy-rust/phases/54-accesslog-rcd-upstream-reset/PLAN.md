# Phase 54 — `54-accesslog-rcd-upstream-reset` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Run every TDD step with `superpowers:test-driven-development`.

**Goal:** Differentially witness the deterministic upstream-reset `%RESPONSE_CODE_DETAILS%` string `upstream_reset_before_response_started{connection_termination}` byte-exact on the H1 upstream-disconnect-before-headers 503 path, set it on the pure-reset final-outcome path, migrate the `UC` `%RESPONSE_FLAGS%` derivation from the phase-53 `reset_for_log` boolean to the now-unique rcd-match, and retire the redundant boolean.

**Architecture:** A single H1-only code change in `crates/envoy-http1/src/hcm.rs`: (§A) set `response_code_details_for_log = Some("upstream_reset_before_response_started{connection_termination}")` on the pure-reset final-outcome path at the post-loop reconciliation region (overriding the in-loop `via_upstream`), guarded `!retry_limit_exceeded_for_log`; (§B) add `Some("upstream_reset_before_response_started{connection_termination}") => "UC"` to the record-build rcd-match and delete the `else if reset_for_log` branch + the `reset_for_log` declaration/set/comments. A new fixture `0062` (clone of `0061` + `rcd` in the json_format, reusing phase 53's accept-then-close `TcpCloseBackend` harness verbatim) + a thin differential test witness it cross-proxy. In-process backstops prove both the positive path and the M53-3 retry-exhausted-reset negative case.

**Tech Stack:** Rust (`envoy-http1`, `envoy-accesslog`, `envoy-config`), `tokio`, the `tests/differential` `run_fixture` harness (`http1_access_log_byte_exact` driver), the phase-53 `tcp-echo-server --close-on-accept` / `TcpCloseBackend` / `{{CLOSE_BACKEND_PORT}}` marker chain.

## Global Constraints

- **Load-bearing invariant:** all `0001`-`0061` differential fixtures stay BYTE-IDENTICAL. The §A rcd-set fires on no existing logged path (the only `{{CLOSE_BACKEND_PORT}}` pure-reset fixture is `0061`, which logs `{rc,rf}`-only — no `rcd`); the §B `UC`-via-rcd migration is output-equivalent to the `UC`-via-boolean derive on the reset path (`0061`'s `rf:"UC"` unchanged — the rcd-match now yields `UC`).
- **NO new** `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant / test-harness code. The `0062` backend reuses phase 53's `--close-on-accept` + `TcpCloseBackend` + the `{{CLOSE_BACKEND_PORT}}` launch arm verbatim.
- **NO `%RESPONSE_FLAGS%` value change** — the witnessed flag stays `UC`; only its DERIVATION moves from the boolean to the rcd. NO change to the synth status (already 503 since phase 53), synth body (not differentially compared), response headers, `x-envoy-attempt-count`, retry counters, or `upstream_rq_5xx`.
- **`#![forbid(unsafe_code)]` holds** — no `unsafe`.
- **`0062` is a backend-spawning fixture → expect LOCAL-RED on this dev host (the `differential-host-bridge-ip-192-168-65-2` flake) and GREEN on native-Linux CI — CI is AUTHORITATIVE** (the phase-53/0061 precedent). The locally-runnable proof of the code change is the in-process backstop set (Tasks 1–2).
- **Exact reset rcd string (state-0 recon, live `envoyproxy/envoy:v1.33.0`, byte-stable across 3 probes + a restart):** `upstream_reset_before_response_started{connection_termination}` — a FIXED reset-reason enum (deterministic, NOT OS text). Full emitted line: `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}` (keys sort by UTF-8 byte order: `rc` < `rcd` < `rf`; compact separators + ONE trailing `\n`).
- **Scope locked by ADR-0111.** `#![forbid(unsafe_code)]` ledger head ADR-0111; ADR-0112 reserved-but-unfired (§6.1 split, projected NOT to fire); ADR-0113 reserved (§6.2 reconciliation, lands inline ONLY if a §A–§G fact is overturned — none was during PLAN-VERIFY).

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `crates/envoy-http1/src/hcm.rs` | §A rcd-set (post-loop ~:1196-1200) + guard; §B derive migration (rcd-match ~:1373 + delete `else if reset_for_log` ~:1370 + decl ~:865-873); §F in-`hcm` comment retargets; §G in-process backstops (positive + negative) | 1, 2 |
| `tests/fixtures/0062-accesslog-rcd-upstream-reset/` (`envoy.yaml`, `envoy-rust.yaml`, `expectations.yaml`, `README.md`) | §C fixture — clone of `0061` + `rcd:"%RESPONSE_CODE_DETAILS%"` in the json_format | 3 |
| `tests/differential/tests/access_log_rcd_upstream_reset.rs` | §D thin `run_fixture` differential test | 3 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | §E rcd row (`:1031`) + `%RESPONSE_FLAGS%` `UC` clause INVERSION (`:1020`) + stale `:1343`→`:1366` anchor refresh | 4 |

---

## PLAN-VERIFY summary (SPEC §3 — resolved during planning, no §A–§G fact overturned → ADR-0113 stays reserved)

- **§3.1 §A set-site + override-ordering — CONFIRMED.** The reset path's in-loop rcd is written to `via_upstream` per-attempt at `hcm.rs:1055-1062` (reset = `endpoint:Some` + `outcome:Some(Reset)` → the `else "via_upstream"` arm). The phase-53 `reset_for_log` set sits post-loop at `hcm.rs:1200` (after the loop's last in-loop write, before the record-build derive at `:1366`). Setting the rcd at that same post-loop site therefore OVERRIDES the in-loop `via_upstream`. `retry_limit_exceeded_for_log` is set at `hcm.rs:1181` (before `:1200`), so the `!retry_limit_exceeded_for_log` guard reads a fully-resolved boolean.
- **§3.1 §B output-equivalence on 0061 — CONFIRMED.** `0061` logs `{rc,rf}`-only (never `rcd`), so §A's rcd-set does not change its line; the flag still renders `UC`, now via the rcd-match arm. Byte-identical.
- **§3.2 byte-preservation — CONFIRMED.** Fixtures logging `%RESPONSE_CODE_DETAILS%`: `0050/0051/0052/0053/0054/0055/0056/0057/0058/0059`. Fixtures logging `%RESPONSE_FLAGS%`: `0040/0046/0056/0057/0058/0059/0060/0061`. The ONLY `{{CLOSE_BACKEND_PORT}}` (accept-then-close pure-reset) fixture is `0061`. None of the rcd-logging fixtures drives a pure-reset final outcome → §A fires on no existing fixture; §B renders identically.
- **§3.4 `reset_for_log` sweep — CONFIRMED phase-53-local.** Only references: decl `hcm.rs:873`, set `:1200`, derive branch `:1370`, comments `:1358`/`:1362`. No other consumer. The in-process backstop test `h1_upstream_reset_access_log_carries_uc_flag` (`hcm.rs:7528`) asserts the reset path's `rf:"UC"` and carries a doc-comment ("NOT rcd-derived") that must be inverted.
- **§3.5 §G backstop form — CONFIRMED feasible.** The positive case extends `h1_upstream_reset_access_log_carries_uc_flag` to also log `rcd`. The M53-3 negative case (retry-exhausted-reset → rcd stays `via_upstream`, flag `URX`) is in-process drivable: `retry_on:"reset"` sets `on_reset=true` (`envoy-config/src/bootstrap.rs:1998`) and `is_retriable(_, AttemptOutcome::Reset) => on.on_reset` (`:1970`) → an accept-then-close backend + `RetryPolicy{retry_on:"reset", num_retries:Some(1)}` exhausts the retry budget with a final `Reset`.

---

## Task 1: §A rcd-set (unguarded) + §B derive migration + retire `reset_for_log` + extend the positive backstop

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — decl block `~:865-873`; post-loop reconciliation `~:1196-1200`; record-build comment `~:1358-1365` + derive `~:1366-1379`; rcd-match enumeration comment `~:1339-1343`; positive backstop test `~:7519-7603`.

**Interfaces:**
- Consumes: `final_outcome: Option<AttemptOutcome>` (set at `hcm.rs:1092`); `response_code_details_for_log: Option<String>`; the record-build `response_flags:` field.
- Produces: the reset path now sets `response_code_details_for_log = Some("upstream_reset_before_response_started{connection_termination}")`; the rcd-match arm `Some("upstream_reset_before_response_started{connection_termination}") => "UC"`; `reset_for_log` no longer exists.

> **Note on TDD ordering:** §A is written WITHOUT the `!retry_limit_exceeded_for_log` guard in this task. This keeps every existing test green (no existing test drives a `retry_on:"reset"`-exhausted path — the phase-51 URX rcd test at `hcm.rs:7373` is `retry_on:"5xx"`-based, whose final outcome is a `Response`, not a `Reset`, so unguarded §A does not fire there). Task 2 then adds the guard via a genuine red→green on the new negative test.

- [ ] **Step 1: Update the positive backstop test to assert the rcd (failing test).** In `crates/envoy-http1/src/hcm.rs`, in `h1_upstream_reset_access_log_carries_uc_flag` (`~:7528`): (a) add an `rcd` entry to the json_format map, after the `rc` insert and before the `rf` insert:

```rust
        map.insert(
            "rcd".to_string(),
            envoy_accesslog::JsonValueInput::Format("%RESPONSE_CODE_DETAILS%".to_string()),
        );
```

(b) change the assertion (`~:7598-7601`) to:

```rust
        assert_eq!(
            logged,
            "{\"rc\":503,\"rcd\":\"upstream_reset_before_response_started{connection_termination}\",\"rf\":\"UC\"}\n",
            "upstream-reset access-log line carries the deterministic reset rcd + rf:UC: {logged:?}"
        );
```

(c) invert the doc-comment (`~:7519-7526`) from "carries the DERIVED rf:"UC" (set post-loop from the reset final-outcome boolean, NOT rcd-derived …)" to:

```rust
    /// phase 53 (ADR-0110) / 54 (ADR-0111): the accept-then-close reset path (NO
    /// retry_policy), wired to a {rc,rcd,rf} FILE json access-log. Asserts the
    /// downstream is the synth-503 AND the logged line carries the deterministic
    /// reset rcd `upstream_reset_before_response_started{connection_termination}`
    /// (set by §A on the pure-reset final-outcome path, overriding the in-loop
    /// `via_upstream`) AND the rf:"UC" now DERIVED 1:1 from that rcd (the
    /// phase-50 `{overflow} => "UO"` precedent — the phase-53 `reset_for_log`
    /// boolean was retired). The in-process proof of §A's rcd-set + §B's
    /// rcd-match arm. Fail-first: pre-change the rcd stays `via_upstream`.
```

- [ ] **Step 2: Run the positive backstop test to verify it fails.**

Run: `cargo test -p envoy-http1 h1_upstream_reset_access_log_carries_uc_flag -- --nocapture`
Expected: FAIL — the logged line is `{"rc":503,"rcd":"via_upstream","rf":"UC"}` (the reset path still emits the in-loop `via_upstream` rcd), not the asserted `{connection_termination}`.

- [ ] **Step 3: Implement §A — set the reset rcd on the pure-reset path (unguarded for now).** In `crates/envoy-http1/src/hcm.rs`, replace the phase-53 set block at `~:1196-1200`:

```rust
                        // phase 53 (ADR-0110): flag UC when the FINAL attempt was a reset —
                        // independent of the retry split (a single reset attempt with no
                        // retry_policy flags it too). A reset retried to success has
                        // final_outcome = Some(Response) → not flagged.
                        reset_for_log = matches!(final_outcome, Some(AttemptOutcome::Reset));
```

with the §A rcd-set:

```rust
                        // phase 54 (ADR-0111): set the deterministic upstream-reset rcd on
                        // the pure-reset final-outcome path. Envoy renders
                        // %RESPONSE_CODE_DETAILS% =
                        // "upstream_reset_before_response_started{connection_termination}"
                        // here — a FIXED reset-reason enum (deterministic, UNLIKE the
                        // connect-failure rcd's OS-derived brace). This OVERRIDES the shared
                        // "via_upstream" the in-loop result-consumption arm wrote for the
                        // reset path (:1055), and the %RESPONSE_FLAGS% derive below maps it
                        // => "UC" (the phase-50 {overflow} => "UO" precedent). A reset
                        // retried to success has final_outcome = Some(Response) → not set
                        // (replay-safe, ADR-0044). [Task 2 adds the
                        // `&& !retry_limit_exceeded_for_log` guard for the M53-3 edge.]
                        if matches!(final_outcome, Some(AttemptOutcome::Reset)) {
                            response_code_details_for_log = Some(
                                "upstream_reset_before_response_started{connection_termination}"
                                    .to_owned(),
                            );
                        }
```

- [ ] **Step 4: Implement §B (i) — add the rcd-match arm.** In the record-build derive (`~:1373-1378`), add the `connection_termination => "UC"` arm after the `{overflow} => "UO"` arm. Keep the BRACED block form below (`=> { "UC" }`) — the single-line form `Some("upstream_reset_before_response_started{connection_termination}") => "UC",` exceeds 100 columns at this indentation and `cargo fmt --check` (Task 5 Step 4) rejects it; do NOT collapse it:

```rust
                    match response_code_details_for_log.as_deref() {
                        Some("route_not_found") => "NR",
                        Some("no_healthy_upstream") => "UH",
                        Some("upstream_reset_before_response_started{overflow}") => "UO",
                        Some("upstream_reset_before_response_started{connection_termination}") => {
                            "UC"
                        }
                        _ => "-",
                    }
```

- [ ] **Step 5: Implement §B (ii) — delete the `else if reset_for_log` branch.** Remove these two lines from the derive (`~:1370-1371`):

```rust
                } else if reset_for_log {
                    "UC"
```

so the chain reads `if retry_limit_exceeded_for_log { "URX" } else if connect_failure_for_log { "UF" } else { match … }`.

- [ ] **Step 6: Implement §B (iii) — delete the `reset_for_log` declaration.** Remove the phase-53 decl + its comment block at `~:865-873`:

```rust
        // phase 53 (ADR-0110): per-request %RESPONSE_FLAGS% = "UC"
        // (UpstreamConnectionTermination) discriminator. Set true POST-LOOP when
        // the FINAL attempt's outcome was AttemptOutcome::Reset (a reset RETRIED
        // to success must NOT flag UC — so this is the final outcome, not a
        // per-attempt set). Like URX/UF, UC is NOT 1:1 with a unique
        // %RESPONSE_CODE_DETAILS% (the reset rcd is the shared "via_upstream"),
        // so it keys on this boolean. `Copy` → no borrow/move interaction with
        // the rcd String.
        let mut reset_for_log = false;
```

(delete the blank line that separated it from the `outgoing` decl too, leaving one blank line).

- [ ] **Step 7: §F — retarget the record-build derive comment.** Replace the phase-53 `reset_for_log => "UC"` comment block at `~:1358-1365`:

```rust
                // phase 53 (ADR-0110): the `reset_for_log => "UC"`
                // (UpstreamConnectionTermination) branch — the THIRD flag NOT
                // derivable from %RESPONSE_CODE_DETAILS% (the reset rcd is the
                // shared "via_upstream", which would otherwise fall to the
                // else-match's `_ => "-"` arm); it keys on the `reset_for_log`
                // boolean set post-loop when the FINAL attempt's AttemptOutcome
                // is Reset. Ordered after UF — set ONLY on the reset final-outcome
                // path, so the NR/UH/UO arms stay byte-identical.
```

with:

```rust
                // phase 54 (ADR-0111): "UC" (UpstreamConnectionTermination) is now
                // derived 1:1 from %RESPONSE_CODE_DETAILS% =
                // "upstream_reset_before_response_started{connection_termination}"
                // (the rcd-match arm below — the phase-50 {overflow} => "UO"
                // precedent), set by §A on the pure-reset final-outcome path. The
                // phase-53 `reset_for_log` boolean was retired (the reset rcd is no
                // longer the shared "via_upstream"). UNLIKE URX/UF, whose rcds
                // genuinely STAY "via_upstream" (so they remain boolean-derived).
```

- [ ] **Step 8: §F — extend the rcd-match enumeration comment.** In the comment listing the rcd→flag mapping (`~:1339-1343`), after the `{overflow} → UO` lines, add:

```rust
                //   upstream_reset_before_response_started{connection_termination}
                //                       → UC (UpstreamConnectionTermination) — the
                //                          pure-reset synth-503 (§A, phase 54).
```

- [ ] **Step 9: Run the positive backstop test to verify it passes.**

Run: `cargo test -p envoy-http1 h1_upstream_reset_access_log_carries_uc_flag -- --nocapture`
Expected: PASS — the logged line is now `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`.

- [ ] **Step 10: Run the full `envoy-http1` test suite to confirm no regression.**

Run: `cargo test -p envoy-http1`
Expected: PASS — in particular the phase-51 URX rcd test (`~:7373`, asserts `{"rc":503,"rcd":"via_upstream","rf":"URX"}`, `retry_on:"5xx"`) stays green (its final outcome is a `Response`, not a `Reset`, so unguarded §A does not fire), and all `NR`/`UH`/`UO`/`UF` backstops stay green.

- [ ] **Step 11: Confirm `reset_for_log` is fully removed.**

Run: `grep -rn "reset_for_log" crates/`
Expected: NO matches (the boolean is retired; the SPEC §3.4 sweep is satisfied).

- [ ] **Step 12: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 54 §A+§B: set reset rcd {connection_termination} + migrate UC to rcd-match, retire reset_for_log [ADR-0111]"
```

---

## Task 2: §A `!retry_limit_exceeded_for_log` guard + §G retry-exhausted-reset negative backstop (M53-3)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` — the §A `if matches!(…)` condition (from Task 1); add one new in-process test in the test module (alongside `h1_upstream_reset_access_log_carries_uc_flag`).

**Interfaces:**
- Consumes: `retry_limit_exceeded_for_log: bool` (set at `hcm.rs:1181`); `envoy_config::RetryPolicy { retry_on, num_retries, retriable_status_codes }`; the test helpers `cluster_mgr_with_endpoint`, `mk_stats`, `test_router_only_pipeline`, `drive`, `tempdir`, `TcpListener`.
- Produces: the §A rcd-set is now suppressed on the retry-limit-exceeded path (rcd stays `via_upstream`, flag `URX`).

- [ ] **Step 1: Write the failing negative-case test.** In `crates/envoy-http1/src/hcm.rs`, immediately after `h1_upstream_reset_access_log_carries_uc_flag` (`~:7603`), add:

```rust
    /// phase 54 (ADR-0111) — the M53-3 NEGATIVE case: a retry-exhausted RESET
    /// (retry_on:"reset", num_retries:1; the accept-then-close backend resets
    /// every attempt). §A's rcd-set is guarded `!retry_limit_exceeded_for_log`,
    /// so the rcd STAYS the shared "via_upstream" (NOT {connection_termination})
    /// and the %RESPONSE_FLAGS% derive renders "URX" (its branch is checked
    /// before the rcd-match). Proves the single most error-prone line in §A:
    /// without the guard, §A would set rcd = "{connection_termination}" and the
    /// rcd assertion fails. The differential 0062 cannot exercise this path.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_retry_exhausted_reset_keeps_via_upstream_rcd_and_urx_flag() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                drop(sock);
            }
        });
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
            })),
        });
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "retry-exhausted reset surfaces the synth-503 downstream: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rcd\":\"via_upstream\",\"rf\":\"URX\"}\n",
            "retry-exhausted reset keeps rcd:via_upstream + rf:URX (the §A guard): {logged:?}"
        );
        server.abort();
    }
```

- [ ] **Step 2: Run the negative test to verify it fails.**

Run: `cargo test -p envoy-http1 h1_retry_exhausted_reset_keeps_via_upstream_rcd_and_urx_flag -- --nocapture`
Expected: FAIL — with unguarded §A (Task 1), the final attempt is a `Reset` so §A sets `rcd = "{connection_termination}"`; the logged line is `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"URX"}`, not the asserted `rcd:"via_upstream"`. (The flag is correctly `URX` either way — the derive's `retry_limit_exceeded_for_log` branch is checked first.)

- [ ] **Step 3: Add the `!retry_limit_exceeded_for_log` guard to §A.** In `crates/envoy-http1/src/hcm.rs`, change the §A condition (from Task 1 Step 3) from:

```rust
                        if matches!(final_outcome, Some(AttemptOutcome::Reset)) {
```

to:

```rust
                        if matches!(final_outcome, Some(AttemptOutcome::Reset))
                            && !retry_limit_exceeded_for_log
                        {
```

and update the bracketed `[Task 2 adds …]` note in the §A comment to read:

```rust
                        // (replay-safe, ADR-0044). Guarded `!retry_limit_exceeded_for_log`
                        // so the retry-exhausted-reset case (M53-3) keeps rcd =
                        // "via_upstream" and renders %RESPONSE_FLAGS% = "URX" (the derive's
                        // URX branch is checked first).
```

- [ ] **Step 4: Run the negative test to verify it passes.**

Run: `cargo test -p envoy-http1 h1_retry_exhausted_reset_keeps_via_upstream_rcd_and_urx_flag -- --nocapture`
Expected: PASS — the guard suppresses the rcd-set on the retry-exhausted path; the logged line is `{"rc":503,"rcd":"via_upstream","rf":"URX"}`.

- [ ] **Step 5: Run the full `envoy-http1` suite to confirm the positive case + all siblings stay green.**

Run: `cargo test -p envoy-http1`
Expected: PASS — `h1_upstream_reset_access_log_carries_uc_flag` (pure reset, no retry → guard irrelevant) still asserts `{connection_termination}`/`UC`; the URX/UF/UO/UH/NR backstops unchanged.

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 54 §A guard + §G: !retry_limit guard preserves M53-3 (retry-exhausted reset → via_upstream/URX) [ADR-0111]"
```

---

## Task 3: §C fixture `0062-accesslog-rcd-upstream-reset` + §D differential test

**Files:**
- Create: `tests/fixtures/0062-accesslog-rcd-upstream-reset/envoy.yaml`
- Create: `tests/fixtures/0062-accesslog-rcd-upstream-reset/envoy-rust.yaml`
- Create: `tests/fixtures/0062-accesslog-rcd-upstream-reset/expectations.yaml`
- Create: `tests/fixtures/0062-accesslog-rcd-upstream-reset/README.md`
- Create: `tests/differential/tests/access_log_rcd_upstream_reset.rs`

**Interfaces:**
- Consumes: the `{{PORT}}` / `{{BACKEND_HOST}}` / `{{CLOSE_BACKEND_PORT}}` markers (the `{{CLOSE_BACKEND_PORT}}` marker auto-spawns `TcpCloseBackend` via `tests/differential/src/lib.rs:3277`); the `http1_access_log_byte_exact` driver; `differential::run_fixture`.
- Produces: fixture `0062`, witnessed by `access_log_rcd_upstream_reset.rs`.

> The ONLY change vs `0061` is the json_format: `0062` adds `rcd: "%RESPONSE_CODE_DETAILS%"` (between `rc` and `rf`). The cluster/route/markers/per-side deltas are byte-identical to `0061`.

- [ ] **Step 1: Create `tests/fixtures/0062-accesslog-rcd-upstream-reset/envoy-rust.yaml`** (clone of `0061/envoy-rust.yaml`, node id/cluster → phase-54/0062, json_format gains `rcd`):

```yaml
node: { id: envoy-rust-phase-54-fixture-0062, cluster: envoy-rust-phase-54 }
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
                      path: /tmp/0062-envoy-rust-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
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
    # STRICT_DNS cluster with NO circuit_breakers and NO retry_policy. The single
    # endpoint is the SPAWNED accept-then-close backend ({{BACKEND_HOST}} =
    # host.docker.internal here, 127.0.0.1 on the subject side) at the
    # harness-reserved {{CLOSE_BACKEND_PORT}}. Both proxies DIAL it, the connect
    # completes, the upstream drains the request then closes (graceful FIN) →
    # the reset synth-503 (rcd:"upstream_reset_before_response_started{connection_termination}",
    # rf:"UC"). The per-side {{BACKEND_HOST}} divergence is invisible (%UPSTREAM_HOST%
    # is not logged) → byte-identity holds.
    - name: backend_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{CLOSE_BACKEND_PORT}} }
```

- [ ] **Step 2: Create `tests/fixtures/0062-accesslog-rcd-upstream-reset/envoy.yaml`** (clone of `0061/envoy.yaml`: `0.0.0.0` bind + admin block + `/tmp/0062-envoy-mount/...`, json_format gains `rcd`):

```yaml
node: { id: envoy-rust-phase-54-fixture-0062, cluster: envoy-rust-phase-54 }
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
                      path: /tmp/0062-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
                          rcd: "%RESPONSE_CODE_DETAILS%"
                          rf: "%RESPONSE_FLAGS%"
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
    # See envoy-rust.yaml for the accept-then-close trigger rationale.
    - name: backend_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: {{BACKEND_HOST}}, port_value: {{CLOSE_BACKEND_PORT}} }
```

- [ ] **Step 3: Create `tests/fixtures/0062-accesslog-rcd-upstream-reset/expectations.yaml`** (clone of `0061`'s, paths → `0062`, comment updated to the `{rc,rcd,rf}` line):

```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0062-envoy-mount/access.log
    envoy_rust: /tmp/0062-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `backend_cluster`, a STRICT_DNS cluster with
    # NO circuit_breakers and NO retry_policy whose single endpoint is the
    # SPAWNED accept-then-close backend ({{CLOSE_BACKEND_PORT}} -> TcpCloseBackend
    # via tcp-echo-server --close-on-accept). Both proxies DIAL it, the connect
    # completes, the upstream drains the request then closes (graceful FIN, NO
    # response) → the reset synth-503. SEVENTH %RESPONSE_CODE_DETAILS% witness +
    # the FIRST deterministic upstream-reset rcd (phase 54, ADR-0111).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). The reset rcd
    # `upstream_reset_before_response_started{connection_termination}` is a FIXED
    # reset-reason enum → DETERMINISTIC (state-0 recon: byte-stable across 3
    # probes + a container restart at live v1.33.0, digest sha256:56da5afd…). The
    # driver compares the access-log lines + status, NOT the body.
    #   rc:  "%RESPONSE_CODE%"          → 503  (json NUMBER)
    #   rcd: "%RESPONSE_CODE_DETAILS%"  → "upstream_reset_before_response_started{connection_termination}"
    #   rf:  "%RESPONSE_FLAGS%"         → "UC"
    # Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf. Compact
    # separators + ONE trailing `\n` (ADR-0092 §E). Emitted line:
    #   {"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `tests/fixtures/0062-accesslog-rcd-upstream-reset/README.md`.** Adapt `0061`'s README: the SEVENTH `%RESPONSE_CODE_DETAILS%` witness / FIRST deterministic upstream-reset rcd; the `{rc,rcd,rf}` json_format table; the emitted line `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`; the same accept-then-close trigger + per-side divergence table + LOCAL-RED/CI-AUTHORITATIVE warning; cross-refs to ADR-0111, fixtures 0061 (the `{rc,rf}`-only sibling whose backend harness this reuses), 0058 (the `{overflow}` rcd precedent), 0050 (the `%RESPONSE_CODE_DETAILS%` baseline). State that `UC`'s derivation moved from the (now-retired) `reset_for_log` boolean to the rcd-match (the phase-50 `UO` precedent), and CONSUMES carry-forward M53-1.

```markdown
# Fixture 0062 — access-log `%RESPONSE_CODE_DETAILS%` upstream-reset path (`upstream_reset_before_response_started{connection_termination}`, byte-exact)

The **SEVENTH differentially-witnessed `%RESPONSE_CODE_DETAILS%` value** (phase
54, ADR-0111), after `direct_response`/`via_upstream` (phase 42, fixture 0050),
`no_healthy_upstream` (phase 45, fixture 0053), `route_not_found` route-miss
(phase 46, fixture 0054) + host-miss (phase 47, fixture 0055), and
`upstream_reset_before_response_started{overflow}` (phase 50, fixture 0058) —
and the **FIRST deterministic upstream-reset rcd**. Witnesses
`upstream_reset_before_response_started{connection_termination}` BYTE-EXACT on
the upstream-disconnect-before-headers **503** path. CONSUMES carry-forward
**M53-1** (the deterministic `UC` rcd the phase-53 SPEC §4 earmarked).

This fixture is a structural clone of **0061** (phase 53, the `{rc,rf}`-only
`UC`-flag witness) — same accept-then-close `STRICT_DNS` backend reused via the
`{{CLOSE_BACKEND_PORT}}` marker — with the json_format extended to add
`rcd: "%RESPONSE_CODE_DETAILS%"`.

> **⚠ LOCAL-RED expected; CI is AUTHORITATIVE.** 0062 SPAWNS a backend (the
> accept-then-close `tcp-echo-server --close-on-accept` via the
> `{{CLOSE_BACKEND_PORT}}` marker) and is therefore subject to the host's Docker
> bridge-IP differential flake (memory `differential-host-bridge-ip-192-168-65-2`):
> **expect LOCAL-RED on this dev host and GREEN on native-Linux CI** — CI is the
> authority for the §7.5 gate (the phase-53/0061 precedent).

## What this proves

On an upstream disconnect before response headers (the upstream completes the
TCP connect, then closes — a graceful FIN — before delivering any response),
both proxies return a deterministic **503** and render
`%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{connection_termination}`
+ `%RESPONSE_FLAGS%` = `UC`. The brace content `connection_termination` is a
FIXED reset-reason enum (NOT OS-derived, UNLIKE the connect-failure rcd) → byte-
exact deterministic, structurally identical to the phase-50 `{overflow}` rcd
(fixture 0058). state-0 recon (live v1.33.0, digest sha256:56da5afd…): byte-
stable across 3 probes + a container restart.

envoy-rust now (§A) SETS the deterministic reset rcd on the pure-reset final-
outcome path (overriding the in-loop shared `via_upstream`, guarded
`!retry_limit_exceeded_for_log` so a retry-exhausted reset keeps `via_upstream` +
`URX`), and (§B) DERIVES `%RESPONSE_FLAGS%` = `UC` 1:1 from that rcd (the phase-50
`{overflow} => "UO"` precedent), RETIRING the phase-53 `reset_for_log` boolean.

The assertion is **pure cross-proxy equality** — there is NO static expected
literal; the byte-exact driver compares the lines + status (NOT the body).

## The `json_format` map

| key   | operator                  | rendered value                                                  |
|-------|---------------------------|-----------------------------------------------------------------|
| `rc`  | `%RESPONSE_CODE%`         | `503` (json NUMBER)                                             |
| `rcd` | `%RESPONSE_CODE_DETAILS%` | `upstream_reset_before_response_started{connection_termination}` |
| `rf`  | `%RESPONSE_FLAGS%`        | `UC`                                                           |

Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rcd, rf; compact separators +
ONE trailing `\n` (ADR-0092 §E). Emitted line:

```
{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}
```

## Per-side divergences

| Side       | bind address | admin block | access-log path                          | `{{BACKEND_HOST}}`     |
|------------|--------------|-------------|------------------------------------------|------------------------|
| envoy      | `0.0.0.0`    | yes (port 0)| `/tmp/0062-envoy-mount/access.log`       | `host.docker.internal` |
| envoy-rust | `127.0.0.1`  | omitted     | `/tmp/0062-envoy-rust-mount/access.log`  | `127.0.0.1`            |

Because the asserted line omits `%UPSTREAM_HOST%`, the per-side `{{BACKEND_HOST}}`
divergence never appears → byte-identity holds.

## Driver

`kind: http1_access_log_byte_exact` (same driver as 0061) — drives the `GET /`
probe, asserts each side's status == 503, scrapes both files, asserts the line
count == `probes.len()`, and calls `assert_access_log_lines_byte_identical`. The
`{{CLOSE_BACKEND_PORT}}` marker triggers the `TcpCloseBackend` launch arm in
`run_fixture` (`tests/differential/src/lib.rs`) — no per-driver backend allowlist,
no new harness code.

## Cross-references

- ADR: ADR-0111 (phase-54 pick + scope).
- Related fixtures: 0061 (`{rc,rf}`-only `UC` sibling, whose accept-then-close
  backend harness this reuses), 0058 (`{overflow}` rcd, the deterministic
  reset-reason-enum precedent), 0050 (the `%RESPONSE_CODE_DETAILS%` baseline).
- Consumes: M53-1. Retires: the phase-53 `reset_for_log` boolean.
- Deferred: the H2 reset rcd (M45-1), the `DC` flag (M45-2), an upstream RST vs
  the graceful FIN (un-recon'd reset-reason brace).
```

- [ ] **Step 5: Create the differential test `tests/differential/tests/access_log_rcd_upstream_reset.rs`** (clone of `access_log_rf_upstream_reset.rs` → `0062`):

```rust
//! Docker-gated differential test for fixture 0062-accesslog-rcd-upstream-reset.
//! Phase 54 (ADR-0111) — the SEVENTH `%RESPONSE_CODE_DETAILS%` witness and the
//! FIRST deterministic upstream-reset rcd:
//! `upstream_reset_before_response_started{connection_termination}`, BYTE-EXACT
//! cross-proxy on the upstream-disconnect-before-headers 503 path. A STRICT_DNS
//! cluster with NO circuit_breakers and NO retry_policy whose single endpoint is
//! a SPAWNED accept-then-close backend (`tcp-echo-server --close-on-accept` via
//! the `{{CLOSE_BACKEND_PORT}}` marker — reused from fixture 0061): both proxies
//! DIAL it, the connect completes, the upstream drains the request then closes
//! (graceful FIN, NO response) → the reset synth-503. envoy-rust now SETS the
//! deterministic reset rcd (§A, overriding the in-loop `via_upstream`, guarded
//! `!retry_limit_exceeded_for_log`) and DERIVES `%RESPONSE_FLAGS%` = `UC` from it
//! (§B, the phase-50 `{overflow} => "UO"` precedent; the phase-53 `reset_for_log`
//! boolean was retired). Upstream Envoy v1.33 emits status 503 +
//! `{"rc":503,"rcd":"upstream_reset_before_response_started{connection_termination}","rf":"UC"}`
//! here (state-0 recon: byte-stable across 3 probes + a container restart).
//! Drives `kind: http1_access_log_byte_exact` (a `GET /` probe,
//! `expected_status: 503`, json_format {rc, rcd, rf}); asserts the emitted line
//! is byte-identical. The driver asserts status + the access-log line but NOT the
//! response body. H1-only (H2 deferred — M45-1). Backend-spawning → LOCAL-RED on
//! the dev host (bridge-IP flake), GREEN on CI.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rcd_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0062-accesslog-rcd-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 6: Confirm the differential test compiles + is discovered (do NOT expect it green locally).**

Run: `cargo test -p differential --test access_log_rcd_upstream_reset --no-run`
Expected: compiles cleanly. (Running it green is CI's job — LOCAL-RED is expected on this dev host per the Global Constraints; do not trim known-failures or treat LOCAL-RED as a regression.)

- [ ] **Step 7: Commit.**

```bash
git add tests/fixtures/0062-accesslog-rcd-upstream-reset/ tests/differential/tests/access_log_rcd_upstream_reset.rs
git commit -m "phase 54 §C+§D: fixture 0062 + differential test (reset rcd {connection_termination}, byte-exact) [ADR-0111]"
```

---

## Task 4: §E BEHAVIOR_CONTRACT updates (rcd row + `UC` clause inversion + anchor refresh)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — the `%RESPONSE_FLAGS%` row (`~:1020`, the `UC` per-flag clause + the witnessed-set summary) and the `%RESPONSE_CODE_DETAILS%` row (`~:1031`).

**Interfaces:** documentation only — no code.

- [ ] **Step 1: §E (2) + spec-review M2 — INVERT the `UC` per-flag clause.** In the `%RESPONSE_FLAGS%` row, the existing `UC` clause reads "**Per-flag equivalence — `UC`:** … and — like `URX`/`UF` — **NOT derived from `%RESPONSE_CODE_DETAILS%`**: the upstream-reset path's rcd is the SHARED `via_upstream` … derived from the `reset_for_log` boolean … (`hcm.rs:1343`, ordered after the `UF` branch). … Note the reset `%RESPONSE_CODE_DETAILS%` … → witnessable byte-exact in a future phase (M53-1), but logged `{rc,rf}`-only here." REPLACE that entire clause (do NOT append) with:

```
**Per-flag equivalence — `UC`:** likewise a config-deterministic single static constant (no combination, brace-free), and — UNLIKE `URX`/`UF` (whose rcds genuinely stay the shared `via_upstream`) — **derived 1:1 from `%RESPONSE_CODE_DETAILS%` = `upstream_reset_before_response_started{connection_termination}`** (the `UO`/`{overflow}` pattern), set by phase 54 (ADR-0111) on the pure-reset final-outcome path at the post-loop reconciliation region (overriding the in-loop shared `via_upstream`, guarded `!retry_limit_exceeded_for_log` so a retry-exhausted reset keeps `via_upstream` and renders `URX`), read by the H1 record-build rcd-match (`hcm.rs:1366`, the arm after `{overflow} => "UO"`). The phase-53 `reset_for_log` boolean was RETIRED. The reset response is the synth-**503**. The reset `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}` is DETERMINISTIC (a fixed reset-reason enum, NOT OS-derived — UNLIKE the connect-failure rcd) and is now witnessed byte-exact at phase 54 (ADR-0111), fixture **0062**.
```

- [ ] **Step 2: §E (2) — update the witnessed-rcd-flag-set summary sentence.** In the same `%RESPONSE_FLAGS%` row, where it enumerates the witnessed set, ensure it reads that the rcd-derived flags are `{NR, UH, UO, UC}` and the boolean-derived flags (rcd stays `via_upstream`) are `{URX, UF}`. Update the closing per-fixture sentence for phase 53/0061 + add phase 54/0062: e.g. append "Phase 54 (ADR-0111) fixture **0062** witnesses the reset `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}` byte-exact on the same path; `UC` now derives 1:1 from that rcd (the `reset_for_log` boolean retired)." Leave the `0061` sentence but change its parenthetical from "rcd `connection_termination` deterministic but NOT logged this phase — M53-1" to "rcd `connection_termination` deterministic, witnessed at phase 54 (M53-1 consumed, fixture 0062)".

- [ ] **Step 3: §E (1) — extend the `%RESPONSE_CODE_DETAILS%` row.** In the `%RESPONSE_CODE_DETAILS%` row (`~:1031`): (a) in the internal-source cell, add the reset path to the set-site list — after the `{overflow}` clause: "**the pure-reset synth-503 (the final-outcome `AttemptOutcome::Reset` path, guarded `!retry_limit_exceeded_for_log`, at the post-loop reconciliation region `hcm.rs:~1200`) → `Some("upstream_reset_before_response_started{connection_termination}")` (phase 54, overriding the in-loop `via_upstream`)**". (b) In the rationale cell, add a fixture sentence: "fixture **0062** (`0062-accesslog-rcd-upstream-reset`, phase 54, ADR-0111) drives the accept-then-close upstream-disconnect-before-headers path → the 503 → `rcd:"upstream_reset_before_response_started{connection_termination}"` cross-proxy byte-exact (live-captured from v1.33.0; the FIFTH failure-path detail and the FIRST deterministic upstream-reset rcd). Its brace content `connection_termination` is a FIXED reset-reason enum (like `{overflow}`), so byte-exact deterministic — refining M45-2 such that ONLY the connect-failure rcd remains non-deterministic." (c) update the default-absent tail to "Default-absent on all fixtures 0001-0049 + 0051-0052 + 0056-0057 + 0059-0061 (no `%RESPONSE_CODE_DETAILS%` logged or no reset path), keeping them byte-identical." — verify the exact current tail wording and keep it consistent.

- [ ] **Step 4: §E + spec-review M1 — refresh the stale `:1343` derive-site anchor.** Within the `%RESPONSE_FLAGS%` row, update every `hcm.rs:1343` reference (in the `NR`/`UH`/`URX`/`UF` clauses and the `UC` clause) to `hcm.rs:1366` (the current record-build `response_flags:` derive line). If the exact line drifts during Tasks 1–2, use the post-change line number of the `response_flags: if retry_limit_exceeded_for_log {` head.

- [ ] **Step 5: Verify the edits — grep for residual staleness.**

Run: `grep -n "reset_for_log\|hcm.rs:1343\|NOT logged this phase\|future phase (M53-1)" docs/envoy-rust/BEHAVIOR_CONTRACT.md`
Expected: NO matches for `reset_for_log` / `hcm.rs:1343` in the rcd/flag rows, and the M53-1 "future phase" / "NOT logged this phase" earmark phrasing is gone (replaced by "witnessed at phase 54").

- [ ] **Step 6: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 54 §E: BEHAVIOR_CONTRACT reset rcd row + UC clause inversion + :1343→:1366 anchor [ADR-0111]"
```

---

## Task 5: §F exhaustive sweep + §3.2 byte-preservation re-grep + local verification

**Files:** none modified (verification + any residual cleanup the greps surface).

**Interfaces:** none.

- [ ] **Step 1: §3.4 — confirm the `reset_for_log` sweep is complete.**

Run: `grep -rn "reset_for_log" crates/ docs/ tests/`
Expected: NO matches anywhere. (Task 1 removed the decl/set/derive/comments; Task 4 removed the contract reference; the README/test doc-comments were retargeted.)

- [ ] **Step 2: §3.2 — re-confirm no existing rcd/rf fixture drives a pure-reset path.**

Run: `grep -rl "CLOSE_BACKEND_PORT" tests/fixtures/*/envoy-rust.yaml`
Expected: ONLY `tests/fixtures/0061-...` and `tests/fixtures/0062-...`. (No other fixture spawns the accept-then-close backend, so the §A rcd-set fires on no `0001`-`0060` path; `0061` logs `{rc,rf}`-only → unaffected.)

- [ ] **Step 3: §F — confirm no stray phase-53 `UC`-via-boolean comment remains in `hcm.rs`.**

Run: `grep -n "keys on this boolean\|keys on the .reset_for_log\|NOT.*rcd-derived\|boolean set post-loop when the FINAL attempt.s AttemptOutcome.*is Reset" crates/envoy-http1/src/hcm.rs`
Expected: NO matches (the reset comments now describe rcd-derivation). If any remain, retarget them and amend the relevant commit.

- [ ] **Step 4: Local build + lint + format + unit tests (the locally-runnable §7.5 (e) subset).**

Run:
```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p envoy-http1
cargo test -p envoy-accesslog
```
Expected: all clean. The two new in-process backstops (`h1_upstream_reset_access_log_carries_uc_flag`, `h1_retry_exhausted_reset_keeps_via_upstream_rcd_and_urx_flag`) pass. (The full `cargo test --workspace`, `cargo deny check`, the `0062` differential, and h2spec are the state-4 verification gate on CI — `0062` is LOCAL-RED-expected here.)

- [ ] **Step 5: Final commit (only if Steps 1–4 surfaced residual edits; otherwise skip).**

```bash
git add -A
git commit -m "phase 54 §F: residual reset_for_log sweep + verification cleanup [ADR-0111]"
```

---

## Self-Review (writing-plans checklist)

- **Spec coverage:** §A → Task 1 (Steps 3) + Task 2 (guard); §B → Task 1 (Steps 4–6); §C → Task 3 (Steps 1–4); §D → Task 3 (Step 5); §E → Task 4 (M1 anchor / M2 inversion / rcd row); §F → Task 1 (Steps 7–8) + Task 5 (Steps 1,3); §G positive → Task 1 (Steps 1–2,9) + §G negative (M53-3, REQUIRED) → Task 2; §3.1/§3.2/§3.4/§3.5 PLAN-VERIFY → resolved in the PLAN-VERIFY summary + Task 5 re-greps. Fuzz → SKIP (no new target, ci.yml unchanged — per SPEC §2). All §A–§G covered.
- **Placeholder scan:** every code step shows the exact code; every command shows expected output. No TODO/TBD.
- **Type consistency:** `response_code_details_for_log: Option<String>`, `final_outcome: Option<AttemptOutcome>`, `retry_limit_exceeded_for_log: bool`, `envoy_config::RetryPolicy { retry_on, num_retries, retriable_status_codes }`, `envoy_accesslog::JsonValueInput::Format`, `CompiledJsonFormat::from_map`, `FileSink::new` — all match the read-confirmed signatures at the cited line numbers.
- **§6.1 gate:** 5 tasks / ~50–90 net LoC of `crates/` change (one rcd-set + one rcd-match arm + a boolean removal) + fixture/test/doc — well under ~25 tasks / ~1500 LoC. **ADR-0112 stays reserved-but-unfired.** No split.
- **§6.2:** no §A–§G fact overturned during PLAN-VERIFY (override-ordering, the guard, 0061 output-equivalence, retry-on-reset feasibility all CONFIRMED against the source). **ADR-0113 stays reserved** (not fired inline).
