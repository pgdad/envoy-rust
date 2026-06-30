# Phase 53 — `53-accesslog-rf-upstream-reset` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Every task is TDD: write the failing test, run it RED, write the minimal implementation, run it GREEN, commit.

**Goal:** Differentially witness the SIXTH non-`-` `%RESPONSE_FLAGS%` value **`UC`** (UpstreamConnectionTermination) byte-exact on the upstream-disconnect-before-headers 503 path, AND correct envoy-rust's reset (send/recv-failure) synth status from an unvalidated `502` to Envoy's `503`.

**Architecture:** envoy-rust's H1 proxy arm already classifies a post-connect send/recv failure (the upstream closed/reset before delivering a complete response) into `AttemptOutcome::Reset` and synthesizes a response at one site (`crates/envoy-http1/src/hcm.rs:618`). This phase (i) changes that synth `502 → 503` to match Envoy, (ii) sets a NEW per-request boolean `reset_for_log` POST-LOOP when the FINAL attempt's outcome was `Reset` (REUSING phase 52's `final_outcome` capture — no new loop state), and (iii) adds an `else if reset_for_log { "UC" }` branch to the `%RESPONSE_FLAGS%` derive at `hcm.rs:1343`. Witnessing the flag requires a SPAWNED accept-then-close upstream (completes the TCP connect, then closes before any response): a new `tcp-echo-server --close-on-accept` mode + a `TcpCloseBackend` harness struct + a new `{{CLOSE_BACKEND_PORT}}` marker launch arm + a new backend-spawning fixture `0061`.

**Tech Stack:** Rust (workspace crates `envoy-http1`, `envoy-config`); test helper `tcp-echo-server` (tokio); differential harness `tests/differential` (testcontainers); fixtures under `tests/fixtures/`.

**Scope guards (from SPEC §2 / ADR-0110):** NO new `Op` / `AccessLogRecord` field / crate / dependency / fuzz-target / `ConfigError` variant. The `crates/` change is one `bool` local + a one-arm status-literal change + one derive branch. The NEW surface vs phase 52 is the accept-then-close backend (test-harness code only). `#![forbid(unsafe_code)]` holds.

**State-2 §6.2 reconciliation result (this PLAN-write, locked):**
- All SPEC §A–§G facts CONFIRMED against the live tree (see "Recon ground-truth" below). **ADR-0112 NOT fired** (nothing overturned).
- §6.1 split does NOT fire (9 tasks / ~300 LoC). **ADR-0111 stays reserved-but-unfired.**
- **Close-mode posture decision: read-then-close** (accept → one best-effort read to drain the request → drop the socket = graceful FIN, no response). This is the SPEC §C-named backstop; it guarantees BOTH proxies classify the event POST-connect (`Reset`/`UC`), removing any race to the connect-failure (`UF`) arm. Envoy still emits `UC` under read-then-close (the upstream closes before response headers regardless of whether it read the request first).
- **Marker decision: a NEW `{{CLOSE_BACKEND_PORT}}` marker** (NOT a reuse of `{{BACKEND_PORT}}`, which routes to the echoing `TcpProxyBackend`), paired with the existing per-side `{{BACKEND_HOST}}` split (STRICT_DNS, the 0003/0004 `TcpProxyBackend` precedent). Per-side host divergence (`host.docker.internal` upstream / `127.0.0.1` subject) is invisible because the asserted log line is `{rc,rf}`-only (NO `%UPSTREAM_HOST%`) → byte-identity holds.

---

## Recon ground-truth (verified this session — cite these in implementation)

**`crates/envoy-http1/src/hcm.rs`:**
- **Reset arm (the ONLY one):** `:613` `if let StreamHandle::Pooled(g) = &mut handle { g.invalidate(); }`; `:609`–`:615` the `tracing::warn!(… "upstream request failed — returning 502")`; `:617`–`:621` `AttemptResult { response: synth_status(502, close), endpoint: Some(endpoint), outcome: Some(AttemptOutcome::Reset), upstream_response: false }`. The three connect-failure arms (`AcquireOutcome::ConnectFailure` at `:626`, plus the `:530`/`:547`-region arms) are ALREADY 503 (phase 52) and are NOT touched.
- **Boolean decls:** `:863` `let mut connect_failure_for_log = false;` (preceded by `retry_limit_exceeded_for_log` at `:854`). The new `reset_for_log` decl goes alongside.
- **`final_outcome` capture:** `:990` `let mut final_outcome: Option<AttemptOutcome> = None;`; `:1082` `final_outcome = attempt.outcome;` (set each iteration). NO new loop state needed.
- **Post-loop set site:** `:1184`–`:1185` `connect_failure_for_log = matches!(final_outcome, Some(AttemptOutcome::ConnectFailure));`. The new `reset_for_log` set goes immediately after.
- **`upstream_rq_5xx` L5 gate:** `:1143` `if completing_upstream_response && final_response.status / 100 == 5 { cluster.upstream_rq_5xx().inc(); }`. A reset synth has `upstream_response: false` → `completing_upstream_response` is false → the gate stays false at 503 exactly as at 502. CONFIRMED non-interaction.
- **The derive:** `:1343` `response_flags: if retry_limit_exceeded_for_log { "URX" } else if connect_failure_for_log { "UF" } else { match response_code_details_for_log.as_deref() { Some("route_not_found") => "NR", Some("no_healthy_upstream") => "UH", Some("upstream_reset_before_response_started{overflow}") => "UO", _ => "-", } }.to_owned(),`. The new branch inserts after the `"UF"` branch (`:1345`–`:1346`).
- **Stale 502 references to sweep (§G):** `:615` warn string "returning 502"; `:618` `synth_status(502`; `:1140` comment "reset synth-502"; `:4049` doc comment "send-fail-502". The reason-phrase table `response.rs:88` `502 => "Bad Gateway"` is GENERIC (still used by cdn_loop's filter-local 502) → NOT touched. A whole-crate grep for `502` returns EXACTLY these five sites (verified).
- **No live test asserts reset→502** (verified by whole-crate grep): the only references are the `:4049` doc comment and `response.rs:88`. So the status change breaks no existing assertion.

**`AttemptOutcome` / retriability** (`crates/envoy-config/src/bootstrap.rs`): `enum AttemptOutcome { Response, ConnectFailure, Reset }` (`:1903`). `is_retriable(status, Reset)` returns `self.on.on_reset` (`:1970`) — only true when `retry_on: reset` is configured. Fixture 0061 has NO `retry_policy` → `retry_config` is `None` → `is_retriable` is never consulted, `final_retriable = false`, single attempt → `final_outcome = Some(Reset)` → flagged. CONFIRMED the FINAL-outcome rule is correct.

**Harness** (`tests/differential/`):
- `backend.rs:19`–`85` `TcpProxyBackend` (the clone model): `spawn()` → `reserve_port()` + `locate_tcp_echo_server()` + spawn with `--port` + `wait_accept_ready(addr, 1s)` + `kill_on_drop(true)`; `port()`; `container_host() -> "host.docker.internal"`; `Drop` SIGKILLs via `start_kill()` + 2s poll. `use crate::{reserve_port, wait_accept_ready};` at `:13`.
- `lib.rs`: `scan_needs_marker(sources, marker)` (`:1208`); the 6-source scan array `backend_scan_sources` (`:3080`); the per-marker launch arms follow the shape `let needs_X = scan_needs_marker(...); let _x = if needs_X { Some(spawn) } else { None }; let x_port_str = _x.as_ref().map(|b| b.port().to_string());` (e.g. `{{HTTP2_BACKEND_PORT}}` at `:3262`). The `{{BACKEND_HOST}}` per-side push is gated on an OR of all `*_port_str.is_some()` — upstream `host.docker.internal` at `:3301`–`:3313`, subject `127.0.0.1` at `:3385`–`:3392`. The kv push for each port marker is `if let Some(p) = x_port_str.as_deref() { v.push(("X", p.to_string())); }` in BOTH `upstream_kvs` (`:3318`-region) and `subject_kvs` (`:3366`-region).
- Driver `Http1AccessLogByteExact` (`lib.rs:110`): probe struct `AccessLogByteExactProbe` (`:1013`) has `method/path/host/extra_headers/body/expected_status` (NO `expected_body`); `expected_status` defaults 200. Status asserted per side vs `probe.expected_status` (`:5139`); line count must equal `probes.len()`; lines asserted byte-identical via `access_log::assert_access_log_lines_byte_identical` (`:5227`). NO per-driver backend allowlist — backend spawn is PURELY marker-driven.
- Fixture 0060 (`tests/fixtures/0060-accesslog-rf-connect-failure/`) + test `tests/differential/tests/access_log_rf_connect_failure.rs` are the structural clone targets (no-backend dead-literal `127.0.0.1:1`). Fixture 0052 (`tests/fixtures/0052-accesslog-upstream-host/`) is the backend-SPAWNING access-log precedent (uses `{{HTTP1_BACKEND_PORT}}`).
- TcpProxyBackend fixtures (e.g. 0004) wire the cluster as `type: STRICT_DNS` with `address: {{BACKEND_HOST}}` + `port_value: {{BACKEND_PORT}}`. 0061 mirrors this with `{{CLOSE_BACKEND_PORT}}`.

**`tcp-echo-server`** (`tests/helpers/tcp-echo-server/src/main.rs`): `Args { port: u16 }` (`:18`); `ArgvError` (`:28`); `parse_argv` (`:44`) with a `_ => return Err(ArgvError::Trailing)` catch-all (so a new flag MUST be an explicit arm); `run_on(listener, shutdown)` (`:69`) whose accept arm spawns `conns.spawn(async move { let (mut r, mut w) = stream.split(); let _ = tokio::io::copy(&mut r, &mut w).await; })` (the echo at `:87`); `run` (`:116`) calls `run_on(listener, ctrl_c)`. Argv unit tests at `:174`–`:208` (incl. `argv_parses_port` whose `Args { port: 10042 }` literal must gain the new field).

**`docs/envoy-rust/BEHAVIOR_CONTRACT.md` §F edit sites:** the `%RESPONSE_FLAGS%` row (`:1020`, "five witnessed failure paths" → six, add the `UC` per-flag rule + fixture 0061); the per-attempt-counting paragraph (`:387`, "reset synth-502" → "reset synth-503"); the `downstream_rq_5xx` row (`:289`, "synth-502 (send-fail)" parenthetical).

**`docs/envoy-rust/DECISIONS.md` ledger head: ADR-0110** (phase 53 pick, already landed). ADR-0111 reserved (split — will NOT fire). ADR-0112 reserved (§6.2 — will NOT fire).

**Fixture number 0061 is free** (highest existing is 0060).

---

## File Structure

**Modified (production crates — minimal):**
- `crates/envoy-http1/src/hcm.rs` — §A(i) status 502→503 (one arm) + warn/comment/doc sweep; §A(ii) `reset_for_log` decl + post-loop set; §B derive `"UC"` branch; + the two new in-process tests.

**Modified (test harness):**
- `tests/helpers/tcp-echo-server/src/main.rs` — `--close-on-accept` (read-then-close) mode + `Args.close_on_accept` + argv arm + unit test.
- `tests/differential/src/backend.rs` — NEW `TcpCloseBackend` struct (mirrors `TcpProxyBackend`).
- `tests/differential/src/lib.rs` — NEW `{{CLOSE_BACKEND_PORT}}` launch arm + kv pushes (both sides) + `BACKEND_HOST` gate extension (both sites).

**Created (fixture + differential test):**
- `tests/fixtures/0061-accesslog-rf-upstream-reset/{envoy.yaml,envoy-rust.yaml,expectations.yaml,README.md}`
- `tests/differential/tests/access_log_rf_upstream_reset.rs`

**Modified (docs):**
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — §F updates.

---

## Task 1: `tcp-echo-server --close-on-accept` (read-then-close) mode

**Files:**
- Modify: `tests/helpers/tcp-echo-server/src/main.rs` (`Args` `:18`, `ArgvError`/`parse_argv` `:44`, `run`/`run_on` `:69`/`:116`, tests `:174`)

- [ ] **Step 1: Write the failing argv test.** Add to the `#[cfg(test)] mod tests` block (after `argv_parses_port`):

```rust
    #[test]
    fn argv_parses_close_on_accept() {
        let got = parse_argv(&argv(&["--port", "10042", "--close-on-accept"])).expect("ok");
        assert_eq!(
            got,
            Args {
                port: 10042,
                close_on_accept: true
            }
        );
    }
```

Also update the existing `argv_parses_port` assertion literal to include the new field:

```rust
        assert_eq!(
            got,
            Args {
                port: 10042,
                close_on_accept: false
            }
        );
```

- [ ] **Step 2: Run it RED.** Run: `cargo test -p tcp-echo-server argv_parses_close_on_accept`
Expected: FAIL to COMPILE (`Args` has no `close_on_accept` field; `--close-on-accept` would also hit the `Trailing` arm).

- [ ] **Step 3: Implement the flag.** In `tests/helpers/tcp-echo-server/src/main.rs`:
  - Add the field to `Args`:
```rust
#[derive(Debug, PartialEq)]
struct Args {
    port: u16,
    close_on_accept: bool,
}
```
  - In `parse_argv`, add a flag arm before the `_ =>` catch-all, track it in a local, and populate the returned struct:
```rust
fn parse_argv(args: &[String]) -> Result<Args, ArgvError> {
    let mut i = 0;
    let mut port: Option<u16> = None;
    let mut close_on_accept = false;
    while i < args.len() {
        match args[i].as_str() {
            "--help" => return Err(ArgvError::HelpRequested),
            "--version" => return Err(ArgvError::VersionRequested),
            "--close-on-accept" => {
                close_on_accept = true;
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
        close_on_accept,
    })
}
```
  - Update `USAGE` to `"tcp-echo-server --port <PORT> [--close-on-accept]"`.
  - Plumb the flag into the accept loop. Change `run_on`'s signature to take the flag, and branch the per-connection task between echo and read-then-close:
```rust
async fn run_on(
    listener: TcpListener,
    shutdown: impl std::future::Future<Output = ()>,
    close_on_accept: bool,
) -> Result<()> {
    // ... unchanged up to the accept arm ...
                    Ok((mut stream, peer)) => {
                        tracing::debug!(?peer, "accepted");
                        conns.spawn(async move {
                            if close_on_accept {
                                // Phase 53 (ADR-0110): accept-then-close upstream.
                                // The handshake has completed (post-connect); do ONE
                                // best-effort read to drain whatever the client sent
                                // (the request), THEN drop the stream — a graceful FIN
                                // with NO response. The read-before-close guarantees
                                // BOTH proxies classify this as a POST-connect reset
                                // (UC), never a pre-connect connect-failure (UF).
                                use tokio::io::AsyncReadExt;
                                let mut buf = [0u8; 1024];
                                let _ = stream.read(&mut buf).await;
                                drop(stream);
                            } else {
                                let (mut r, mut w) = stream.split();
                                let _ = tokio::io::copy(&mut r, &mut w).await;
                            }
                        });
                    }
    // ... unchanged ...
}
```
  - In `run` (`:116`), pass the flag through: `run_on(listener, ctrl_c, args.close_on_accept).await`. (Confirm `args` is in scope at the `run_on` call site; if `run` does not currently hold the parsed `Args`, thread `close_on_accept` from `main`.)

- [ ] **Step 4: Run it GREEN + the full helper suite.** Run: `cargo test -p tcp-echo-server`
Expected: PASS (all argv tests incl. the new one; `echoes_round_trip` + `drain_exits_within_budget` still pass — the default path is unchanged).

- [ ] **Step 5: Commit.**
```bash
git add tests/helpers/tcp-echo-server/src/main.rs
git commit -m "phase 53 task 1: tcp-echo-server --close-on-accept (read-then-close) mode [ADR-0110]"
```

---

## Task 2: Correct the reset synth status 502 → 503 (§A(i) + §G sweep)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (reset arm `:615`/`:618`; comment `:1140`; doc `:4049`; new test in the `#[cfg(test)]` module)

- [ ] **Step 1: Write the failing in-process test.** Add to the `hcm.rs` test module (model: the phase-52 `h1_connect_failure_access_log_carries_uf_flag` at `:7361`; the accept-then-close loopback listener below is self-contained — there is NO existing upstream accept-then-close test to model from, so the listener code here is authoritative). This test drives a real accept-then-close loopback backend so it exercises the genuine `AttemptOutcome::Reset` arm:

```rust
    /// phase 53 (ADR-0110): an accept-then-close loopback upstream (completes
    /// the TCP connect, drains the request, then drops the socket — a graceful
    /// FIN with NO response) with NO retry_policy drives the single H1 reset arm
    /// (hcm.rs:618, AttemptOutcome::Reset). Asserts the downstream response is
    /// the synth-503 (Task 2 corrected the unvalidated 502 to match Envoy's UC
    /// path). Fail-first: pre-change the reset arm synthesizes 502.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_upstream_reset_returns_503() {
        use tokio::io::AsyncReadExt;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut sock, _)) => {
                        // read-then-close: drain the request (post-connect),
                        // then FIN with no response.
                        let mut buf = [0u8; 1024];
                        let _ = sock.read(&mut buf).await;
                        drop(sock);
                    }
                    Err(_) => break,
                }
            }
        });
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
        let config = hcm_config_router_only("/", "backend", cluster_mgr);
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "upstream-reset surfaces the synth-503 downstream: {resp_str}"
        );
        server.abort();
    }
```

> **Worker note:** `hcm_config_router_only(path, cluster, cluster_mgr)` is shorthand for the router-only HCM config the phase-52 UF test builds inline (an `HCMConfig` with `filter_pipeline: test_router_only_pipeline()`, a single prefix-`/` route to `cluster`, `retry_policy: None`, NO access_log). If no such helper exists, inline the `HCMConfig { … }` literal exactly as `h1_connect_failure_access_log_carries_uf_flag` does (`:7376`-region), minus the `access_log`/json bits (Task 2 asserts status only). Confirm `cluster_mgr_with_endpoint` (`:2238`) and `drive` are in scope (they are — used throughout this module).

- [ ] **Step 2: Run it RED.** Run: `cargo test -p envoy-http1 h1_upstream_reset_returns_503`
Expected: FAIL — `assert!` fires because the reset arm synthesizes `HTTP/1.1 502 …`.

- [ ] **Step 3: Implement the status change + sweep.** In `crates/envoy-http1/src/hcm.rs`:
  - `:615` warn string: `"upstream request failed — returning 502"` → `"upstream request failed — returning 503"`.
  - `:618`: `response: synth_status(502, close),` → `response: synth_status(503, close),`.
  - `:1140` comment: `// 502, and overflow synth-503 paths) do NOT tick it, preserving` → `// 503, and overflow synth-503 paths) do NOT tick it, preserving` (the reset synth is now 503; update the comment so it reads "...connect-failure synth-503, reset synth-503, and overflow synth-503...").
  - `:4049` doc comment: `send-fail-502` → `send-fail-503`.

- [ ] **Step 4: Run it GREEN + the crate suite.** Run: `cargo test -p envoy-http1`
Expected: PASS (`h1_upstream_reset_returns_503` passes; no other test regresses — the whole-crate grep confirmed no live test asserts reset→502).

- [ ] **Step 5: Commit.**
```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 53 task 2: reset synth status 502->503 to match Envoy + 502 comment/doc sweep [ADR-0110]"
```

---

## Task 3: `reset_for_log` boolean + `%RESPONSE_FLAGS%` = `UC` derive branch (§A(ii) + §B)

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (decl `:863`; post-loop set `:1185`; derive `:1346`; new test)

- [ ] **Step 1: Write the failing in-process flag test.** Add to the `hcm.rs` test module (model: `h1_connect_failure_access_log_carries_uf_flag` at `:7361`, with the accept-then-close listener from Task 1's test). This drives the reset path through a `{rc,rf}` json file access-log and asserts the DERIVED `UC`:

```rust
    /// phase 53 (ADR-0110): the accept-then-close reset path (NO retry_policy),
    /// wired to a {rc,rf} FILE json access-log. Asserts the downstream is the
    /// synth-503 AND the logged line carries the DERIVED rf:"UC" (set post-loop
    /// from the reset final-outcome boolean, NOT rcd-derived — the reset rcd is
    /// the shared "via_upstream"). The sole in-process proof of §A's
    /// discriminator + §B's derive branch. Fail-first: pre-change the derive's
    /// rcd-match falls to `_ => "-"` (via_upstream unmatched) → it renders
    /// `"rf":"-"`.
    #[tokio::test(flavor = "multi_thread")]
    async fn h1_upstream_reset_access_log_carries_uc_flag() {
        use tokio::io::AsyncReadExt;
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join("access.log");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut sock, _)) => {
                        let mut buf = [0u8; 1024];
                        let _ = sock.read(&mut buf).await;
                        drop(sock);
                    }
                    Err(_) => break,
                }
            }
        });
        let cluster_mgr = cluster_mgr_with_endpoint("backend", port).await;
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
        // Build the same router-only HCMConfig as the UF test, but with
        // `access_log: vec![sink]` and the accept-then-close cluster_mgr above.
        let config = /* HCMConfig { … access_log: vec![sink], cluster_mgr, … } — mirror :7376 */;
        let req = b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n";
        let resp = drive(config, req).await;
        let resp_str = String::from_utf8_lossy(&resp);
        assert!(
            resp_str.starts_with("HTTP/1.1 503 "),
            "upstream-reset surfaces the synth-503 downstream: {resp_str}"
        );
        tokio::time::sleep(StdDuration::from_millis(50)).await;
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            logged, "{\"rc\":503,\"rf\":\"UC\"}\n",
            "upstream-reset access-log line carries rf:UC: {logged:?}"
        );
        server.abort();
    }
```

> **Worker note:** Copy the `HCMConfig { … }` literal verbatim from `h1_connect_failure_access_log_carries_uf_flag` (`:7376`–`:7409`), substituting the `cluster_mgr` built above. Keep `retry_policy: None`. The ONLY differences vs the UF test are the cluster_mgr (accept-then-close listener vs `127.0.0.1:1`) and the expected flag (`UC` vs `UF`).

- [ ] **Step 2: Run it RED.** Run: `cargo test -p envoy-http1 h1_upstream_reset_access_log_carries_uc_flag`
Expected: FAIL — `assert_eq!` reports `{"rc":503,"rf":"-"}` (the reset rcd `via_upstream` falls to the derive's `_ => "-"` arm; no `reset_for_log` boolean yet).

- [ ] **Step 3: Implement the boolean + derive branch.** In `crates/envoy-http1/src/hcm.rs`:
  - Decl alongside `connect_failure_for_log` (after `:863`):
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
  - Post-loop set immediately after the `connect_failure_for_log` set (`:1185`):
```rust
        // phase 53 (ADR-0110): flag UC when the FINAL attempt was a reset —
        // independent of the retry split (a single reset attempt with no
        // retry_policy flags it too). A reset retried to success has
        // final_outcome = Some(Response) → not flagged.
        reset_for_log = matches!(final_outcome, Some(AttemptOutcome::Reset));
```
  - Derive branch after the `"UF"` branch (`:1346`):
```rust
                response_flags: if retry_limit_exceeded_for_log {
                    "URX"
                } else if connect_failure_for_log {
                    "UF"
                } else if reset_for_log {
                    "UC"
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
  Also add a one-line code comment above the derive (mirroring the phase-52 `UF` block at `:1334`) documenting that `UC` keys on `reset_for_log`, ordered after `UF`, set only on the reset final-outcome path (rcd = via_upstream → the else-match's `_` arm → the NR/UH/UO arms stay byte-identical).

- [ ] **Step 4: Run it GREEN + the crate suite.** Run: `cargo test -p envoy-http1`
Expected: PASS (the new test logs `{"rc":503,"rf":"UC"}`; Task 2's status test still passes; no regression — `reset_for_log` is set only on the reset final-outcome path, so URX/UF/NR/UH/UO render identically).

- [ ] **Step 5: Commit.**
```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 53 task 3: reset_for_log boolean + %RESPONSE_FLAGS%=UC derive branch [ADR-0110]"
```

---

## Task 4: `TcpCloseBackend` harness struct (§C(ii))

**Files:**
- Modify: `tests/differential/src/backend.rs` (add struct after `TcpProxyBackend`, ~`:85`)

- [ ] **Step 1: Add the struct** (a near-verbatim clone of `TcpProxyBackend`, passing the extra `--close-on-accept` flag). This is harness infrastructure exercised by the Task 7 differential test; there is no standalone unit test (the same as `TcpProxyBackend` itself has none):

```rust
/// Phase 53 (ADR-0110): a running `tcp-echo-server --close-on-accept` host
/// subprocess — an accept-then-close upstream that completes the TCP connect,
/// drains the request, then closes (graceful FIN, NO response). Used by the
/// fixture-0061 reset/`UC` witness via the `{{CLOSE_BACKEND_PORT}}` marker.
/// Drop posture identical to `TcpProxyBackend`.
pub struct TcpCloseBackend {
    port: u16,
    child: Option<tokio::process::Child>,
}

impl TcpCloseBackend {
    pub async fn spawn() -> Result<Self> {
        let port = reserve_port().context("reserving close-backend port")?;
        let bin = locate_tcp_echo_server().context("locating tcp-echo-server binary")?;
        let child = tokio::process::Command::new(&bin)
            .arg("--port")
            .arg(port.to_string())
            .arg("--close-on-accept")
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning {} --port {port} --close-on-accept", bin.display()))?;

        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .with_context(|| format!("tcp-echo-server --close-on-accept never became accept-ready on {addr}"))?;

        Ok(Self {
            port,
            child: Some(child),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn container_host(&self) -> &'static str {
        "host.docker.internal"
    }
}

impl Drop for TcpCloseBackend {
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

> **Worker note:** `wait_accept_ready` returns once the listener accepts a TCP connection — the `--close-on-accept` server still ACCEPTS (it just closes after), so readiness detection works unchanged. (`wait_accept_ready` opens and immediately drops a probe connection; the close-on-accept backend handles that fine.)

- [ ] **Step 2: Verify it compiles.** Run: `cargo build -p differential --tests`
Expected: clean compile (no warnings; `Result`, `reserve_port`, `wait_accept_ready`, `locate_tcp_echo_server`, `Stdio`, `Duration` are already imported in `backend.rs`).

- [ ] **Step 3: Commit.**
```bash
git add tests/differential/src/backend.rs
git commit -m "phase 53 task 4: TcpCloseBackend harness struct (accept-then-close) [ADR-0110]"
```

---

## Task 5: Wire `{{CLOSE_BACKEND_PORT}}` into `run_fixture` (§C/§E)

**Files:**
- Modify: `tests/differential/src/lib.rs` (launch arm after the `{{HTTP2_BACKEND_PORT}}` arm `:3272`; kv pushes in both `upstream_kvs` and `subject_kvs`; `BACKEND_HOST` gate OR-extension at both `:3301` and `:3385`)

- [ ] **Step 1: Add the launch arm** after the `{{HTTP2_BACKEND_PORT}}` arm (`:3272`), mirroring its shape:
```rust
    // Phase 53 (ADR-0110): the accept-then-close upstream for the fixture-0061
    // reset/UC witness. Distinct from {{BACKEND_PORT}} (which routes to the
    // echoing TcpProxyBackend); this marker spawns the close-on-accept backend.
    let needs_close_backend = scan_needs_marker(&backend_scan_sources, "CLOSE_BACKEND_PORT");
    let _close_backend: Option<crate::backend::TcpCloseBackend> = if needs_close_backend {
        Some(
            crate::backend::TcpCloseBackend::spawn()
                .await
                .context("spawning TcpCloseBackend")?,
        )
    } else {
        None
    };
    let close_backend_port_str = _close_backend.as_ref().map(|b| b.port().to_string());
```

- [ ] **Step 2: Push the marker into both kv maps.** In `upstream_kvs` (after the `HTTP2_BACKEND_PORT` push, ~`:3297`) AND in `subject_kvs` (after its `HTTP2_BACKEND_PORT` push, ~`:3384`), add (key off the semantic anchor "after the HTTP2 push", not the literal line number):
```rust
        if let Some(cp) = close_backend_port_str.as_deref() {
            v.push(("CLOSE_BACKEND_PORT", cp.to_string()));
        }
```

- [ ] **Step 3: Extend the `BACKEND_HOST` gate at BOTH sites.** Add `|| close_backend_port_str.is_some()` to the OR-chain in the upstream block (`:3301`-region, pushes `host.docker.internal`) AND the subject block (`:3385`-region, pushes `127.0.0.1`):
```rust
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
            || http1_backend_1_port_str.is_some()
            || http1_backend_2_port_str.is_some()
            || http2_backend_port_str.is_some()
            || close_backend_port_str.is_some()
        {
            v.push(("BACKEND_HOST", /* "host.docker.internal" upstream | "127.0.0.1" subject */));
        }
```

- [ ] **Step 4: Verify it compiles.** Run: `cargo build -p differential --tests`
Expected: clean compile. (No fixture references `{{CLOSE_BACKEND_PORT}}` yet → `needs_close_backend` is false for all existing fixtures → zero behavior change to fixtures 0001–0060.)

- [ ] **Step 5: Commit.**
```bash
git add tests/differential/src/lib.rs
git commit -m "phase 53 task 5: wire {{CLOSE_BACKEND_PORT}} marker -> TcpCloseBackend in run_fixture [ADR-0110]"
```

---

## Task 6: Fixture `0061-accesslog-rf-upstream-reset` (§D)

**Files:**
- Create: `tests/fixtures/0061-accesslog-rf-upstream-reset/envoy.yaml`
- Create: `tests/fixtures/0061-accesslog-rf-upstream-reset/envoy-rust.yaml`
- Create: `tests/fixtures/0061-accesslog-rf-upstream-reset/expectations.yaml`
- Create: `tests/fixtures/0061-accesslog-rf-upstream-reset/README.md`

> Model: fixture 0060 (the `{rc,rf}` json access-log + the H1 listener/HCM shape) with the cluster swapped from the dead-literal `127.0.0.1:1` to a STRICT_DNS `{{BACKEND_HOST}}:{{CLOSE_BACKEND_PORT}}` cluster (the 0004 `TcpProxyBackend` cluster shape). NO `circuit_breakers`, NO `retry_policy`.

- [ ] **Step 1: Create `envoy.yaml`** (upstream side — `{{BACKEND_HOST}}` renders `host.docker.internal`):
```yaml
node: { id: envoy-rust-phase-53-fixture-0061, cluster: envoy-rust-phase-53 }
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
                      path: /tmp/0061-envoy-mount/access.log
                      log_format:
                        json_format:
                          rc: "%RESPONSE_CODE%"
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
    # STATIC->STRICT_DNS cluster with NO circuit_breakers and NO retry_policy.
    # The single endpoint is the SPAWNED accept-then-close backend ({{BACKEND_HOST}}
    # = host.docker.internal here, 127.0.0.1 on the subject side) at the
    # harness-reserved {{CLOSE_BACKEND_PORT}}. Both proxies DIAL it, the connect
    # completes, the upstream drains the request then closes (graceful FIN) →
    # the reset synth-503 (rf:"UC"). The {rc,rf}-only log line omits
    # %UPSTREAM_HOST%, so the per-side {{BACKEND_HOST}} divergence is invisible →
    # byte-identity holds.
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

- [ ] **Step 2: Create `envoy-rust.yaml`** — byte-identical to `envoy.yaml` EXCEPT: omit the `admin:` line; listener binds `127.0.0.1` (not `0.0.0.0`); access-log `path: /tmp/0061-envoy-rust-mount/access.log`. (`{{BACKEND_HOST}}` renders `127.0.0.1` on this side.) Mirror exactly the 0060 upstream/subject delta set.

- [ ] **Step 3: Create `expectations.yaml`:**
```yaml
driver:
  kind: http1_access_log_byte_exact
  expected_access_log_paths:
    envoy: /tmp/0061-envoy-mount/access.log
    envoy_rust: /tmp/0061-envoy-rust-mount/access.log
  probes:
    # Probe 1: bare GET / routed to `backend_cluster`, a STRICT_DNS cluster with
    # NO circuit_breakers and NO retry_policy whose single endpoint is the
    # SPAWNED accept-then-close backend ({{CLOSE_BACKEND_PORT}} -> TcpCloseBackend
    # via tcp-echo-server --close-on-accept). Both proxies DIAL it, the connect
    # completes, the upstream drains the request then closes (graceful FIN, NO
    # response) → the reset synth-503. SIXTH non-`-` %RESPONSE_FLAGS% witness: UC
    # (UpstreamConnectionTermination) (phase 53, ADR-0110).
    #
    # ASSERTION = PURE CROSS-PROXY EQUALITY (whole-line `==`). The fixture logs
    # ONLY {rc, rf} — the reset %RESPONSE_CODE_DETAILS%
    # (upstream_reset_before_response_started{connection_termination}) is
    # DETERMINISTIC but deferred (M53-1, minimum-viable witness), and the driver
    # does not compare the body. envoy-rust returns 503 (Task 2 corrected the
    # unvalidated 502) and DERIVES %RESPONSE_FLAGS% = UC from the reset
    # final-outcome boolean (NOT from rcd).
    # state-0 recon (live v1.33.0, digest sha256:56da5afd…, byte-stable across 8
    # repeats + a restart): status 503, rf "UC".
    #   rc: "%RESPONSE_CODE%"   → 503  (json NUMBER)
    #   rf: "%RESPONSE_FLAGS%"  → "UC"
    # Keys sort by UTF-8 byte order (ADR-0094 §A): rc, rf. Compact separators +
    # ONE trailing `\n` (ADR-0092 §E). Emitted line:
    #   {"rc":503,"rf":"UC"}
    - method: get
      path: /
      host: envoy-rust.test
      expected_status: 503
```

- [ ] **Step 4: Create `README.md`** modeled on 0060's, updated for the reset path. MUST state: (a) the topology = a SPAWNED accept-then-close backend via the `{{CLOSE_BACKEND_PORT}}` marker (UNLIKE 0060's no-backend dead-literal); (b) the read-then-close posture guarantees POST-connect (`Reset`/`UC`) classification on both proxies; (c) the `{rc,rf}`-only log line; (d) **this is a backend-spawning fixture → expect LOCAL-RED on this dev host (the `differential-host-bridge-ip-192-168-65-2` flake) and GREEN on CI — CI is AUTHORITATIVE** (the 0052 backend-spawn access-log precedent); (e) the deterministic `UC` rcd is deferred (M53-1).

- [ ] **Step 5: Commit.**
```bash
git add tests/fixtures/0061-accesslog-rf-upstream-reset/
git commit -m "phase 53 task 6: fixture 0061 accept-then-close reset UC witness [ADR-0110]"
```

---

## Task 7: Differential test `access_log_rf_upstream_reset.rs` (§E)

**Files:**
- Create: `tests/differential/tests/access_log_rf_upstream_reset.rs`

- [ ] **Step 1: Create the test** — a thin `run_fixture` wrapper, a structural clone of `access_log_rf_connect_failure.rs`:
```rust
//! Docker-gated differential test for fixture 0061-accesslog-rf-upstream-reset.
//! Phase 53 (ADR-0110) — the SIXTH non-`-` `%RESPONSE_FLAGS%` witness: `UC`
//! (UpstreamConnectionTermination), BYTE-EXACT cross-proxy on the
//! upstream-disconnect-before-headers 503 path. A STRICT_DNS cluster with NO
//! circuit_breakers and NO retry_policy whose single endpoint is a SPAWNED
//! accept-then-close backend (`tcp-echo-server --close-on-accept` via the
//! `{{CLOSE_BACKEND_PORT}}` marker): both proxies DIAL it, the connect
//! completes, the upstream drains the request then closes (graceful FIN, NO
//! response) → the reset synth-503. envoy-rust now (a) returns 503 (Task 2
//! corrected the unvalidated 502) and (b) DERIVES `%RESPONSE_FLAGS%` = `UC`
//! from the reset final-outcome boolean (NOT from `%RESPONSE_CODE_DETAILS%`,
//! which is the shared `via_upstream`). Upstream Envoy v1.33 emits status 503 +
//! `rf:"UC"` here (state-0 recon: byte-stable across 8 repeats + a container
//! restart). Drives `kind: http1_access_log_byte_exact` (a `GET /` probe,
//! `expected_status: 503`, json_format {rc, rf}); asserts the emitted line
//! `{"rc":503,"rf":"UC"}` is byte-identical. The driver asserts status + the
//! access-log line but NOT the response body. H1-only (H2 deferred — M45-1).
//! Backend-spawning → LOCAL-RED on the dev host (bridge-IP flake), GREEN on CI.

use std::path::PathBuf;

#[tokio::test]
async fn access_log_rf_upstream_reset() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0061-accesslog-rf-upstream-reset");
    differential::run_fixture(&dir)
        .await
        .expect("fixture green");
}
```

- [ ] **Step 2: Build the differential test target** (the differential run itself is Docker-gated + a known LOCAL flake — CI is authoritative). Run: `cargo build -p differential --tests`
Expected: clean compile.

- [ ] **Step 3: (Optional, local-best-effort) run the fixture.** Run: `cargo test -p differential access_log_rf_upstream_reset -- --nocapture`
Expected: GREEN on CI; may be LOCAL-RED on this dev host (bridge-IP flake — `differential-host-bridge-ip-192-168-65-2`). A LOCAL-RED here is NOT a regression; the state-4 gate is CI-authoritative. Record the local outcome in PROGRESS.md but do not block on it.

- [ ] **Step 4: Commit.**
```bash
git add tests/differential/tests/access_log_rf_upstream_reset.rs
git commit -m "phase 53 task 7: differential test for fixture 0061 (UC witness) [ADR-0110]"
```

---

## Task 8: BEHAVIOR_CONTRACT updates (§F)

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (`:1020` `%RESPONSE_FLAGS%` row; `:387` per-attempt-counting paragraph; `:289` `downstream_rq_5xx` row; `:36` no-healthy-upstream row's wire-shape note; `:296` `cluster.<name>.upstream_rq_5xx` row)

> **Sweep completeness (plan-review I1):** a `502` in BEHAVIOR_CONTRACT.md appears at SIX lines. FIVE name the send-fail/reset path and MUST flip to `503` once Task 2 lands (`:36`, `:289`, `:296`, `:387`, plus the row at `:1020` already reframed below). ONE — `:1031` — is the H2 no-healthy arm (`"the H2 no-healthy arm returns 502"`) which is GENUINELY unchanged (H2 deferred, SPEC §4) and MUST be left as-is. The cdn_loop / generic reason-phrase `502` mentions (filter-local-reply path) are also untouched.

- [ ] **Step 1: Extend the `%RESPONSE_FLAGS%` row (`:1020`).** Change "five witnessed failure paths" → "six witnessed failure paths" and add the upstream-disconnect path. Add a **`UC`** per-flag-equivalence clause (mirroring the `UF` clause): a config-deterministic single static constant (no combination, brace-free), and — like `URX`/`UF` — **NOT derived from `%RESPONSE_CODE_DETAILS%`** (the reset path's rcd is the shared `via_upstream`); derived from the `reset_for_log` boolean set post-loop when the FINAL attempt's `AttemptOutcome` is `Reset` (a reset retried to success is NOT flagged), read by the H1 record-build derive (`hcm.rs:1343`, ordered after the `UF` branch). The reset response is the synth-**503** (corrected from a previously-unvalidated synth-502 to match Envoy). Note the reset `%RESPONSE_CODE_DETAILS%` `upstream_reset_before_response_started{connection_termination}` is DETERMINISTIC (a fixed reset-reason enum, NOT OS-derived — UNLIKE the connect-failure rcd) → witnessable byte-exact in a future phase (M53-1), but logged `{rc,rf}`-only here. Update the "value-exact (… case)" parenthetical to add the `UC` upstream-reset case, and the witnessing-fixtures sentence to add: "Phase 53 (ADR-0110) fixture **0061** witnesses `UC` byte-exact on the H1 upstream-disconnect-before-headers 503 path; both proxies emit `UC` (rcd `connection_termination` deterministic but NOT logged this phase — M53-1)."

- [ ] **Step 2: Fix the per-attempt-counting paragraph (`:387`).** Change "the no-healthy-upstream synth-503, connect-failure synth-503, reset synth-502, and overflow synth-503 paths" → "...reset synth-**503**...". (The reset synth is now 503; the non-tick disposition is unchanged — a reset has no real upstream response, so `upstream_rq_5xx` still does not tick regardless of 502 vs 503.)

- [ ] **Step 3: Fix the `downstream_rq_5xx` row (`:289`).** Change "proxy synth-503 (no-endpoint, connect-fail, overflow) / synth-502 (send-fail)" → "proxy synth-503 (no-endpoint, connect-fail, send-fail/reset, overflow)" and update the trailing parenthetical so it reads the count is unaffected by the reset 502→503 correction (both 502 and 503 are 5xx). Keep the symmetry note intact.

- [ ] **Step 4: Fix the no-healthy-upstream row's wire-shape note (`:36`).** Change "The connect-fail 503 + send-fail 502 paths keep `synth_status`'s empty body" → "The connect-fail 503 + send-fail/reset **503** paths keep `synth_status`'s empty body".

- [ ] **Step 5: Fix the `cluster.<name>.upstream_rq_5xx` row (`:296`).** Change "Synth local-reply paths (connect-fail 503, send-fail 502) bypass…" → "Synth local-reply paths (connect-fail 503, send-fail/reset **503**) bypass…".

- [ ] **Step 6: Verify the sweep is complete + the H2-502 survivor is intact.** Run: `grep -n '502' docs/envoy-rust/BEHAVIOR_CONTRACT.md`
Expected: the ONLY remaining `502` lines are `:1031` (the H2 no-healthy arm — "the H2 no-healthy arm returns 502", GENUINELY unchanged per SPEC §4's H2-deferral) plus any unrelated cdn_loop / generic reason-phrase mentions. NO line should still name the send-fail/reset path's `502` (`:36`, `:289`, `:296`, `:387`, `:1020` must all now read `503`/UC). If a send-fail/reset `502` survives, fix it; do NOT touch `:1031`.

- [ ] **Step 7: Commit.**
```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 53 task 8: BEHAVIOR_CONTRACT %RESPONSE_FLAGS% UC + reset synth-503 [ADR-0110]"
```

---

## Task 9: Full-workspace verification gate prep (§5 / §7.5 — state-4 dry-run)

> This task is the local pre-flight for the state-4 verification session. It does NOT replace state-4; it surfaces any fmt/clippy/build/test breakage now so state-3 lands clean.

**Files:** none (verification only; quote outputs into PROGRESS.md at state-4).

- [ ] **Step 1: Build all targets.** Run: `cargo build --workspace --all-targets`
Expected: clean.

- [ ] **Step 2: Clippy.** Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 3: Format check.** Run: `cargo fmt --all -- --check`
Expected: clean (run `cargo fmt --all` first to normalize the new code).

- [ ] **Step 4: Unit tests.** Run: `cargo test --workspace`
Expected: PASS. Known local exceptions (NOT regressions, CI-authoritative): the backend-spawning differential `access_log_rf_upstream_reset` (fixture 0061) may LOCAL-RED on the bridge-IP flake; the pre-existing host flakes recorded in memory (`envoyrust-h2-handshake-test-host-flake`, `eds-fatal-startup-test-port-reuse-flake`, parallel-load differential flakes). Re-run isolated to confirm.

- [ ] **Step 5: cargo-deny.** Run: `cargo deny check`
Expected: clean. If a freshly-published advisory reds an existing dep (memory `cargo-deny-reds-on-unrelated-advisory`), patch-bump that dep with `cargo update -p <X> --precise <ver>` — NOT a phase regression.

- [ ] **Step 6: Fuzz — SKIP.** No new fuzz target this phase (`%RESPONSE_FLAGS%` is an existing operator; `ci.yml` unchanged). Confirm `ci.yml` has no new fuzz step to add.

- [ ] **Step 7: Commit any fmt/deny normalization** (if Steps 3/5 changed files):
```bash
git add -A
git commit -m "phase 53 task 9: fmt/deny normalization pre-state-4 [ADR-0110]"
```

---

## Acceptance (SPEC §5 — re-verified at state-4)

(a) fixture `0061` green (cross-proxy-equal status `503` + whole-line `{"rc":503,"rf":"UC"}`); (b) all `0001`–`0060` green simultaneously (additive — `reset_for_log` set only on the reset final-outcome path; the 502→503 change touches no existing GREEN fixture); (c) h2spec ≥95% (no HTTP/2 change); (d) `parse_bootstrap`/`accesslog_format_parse` fuzz clean (no new target); (e) build/clippy/fmt/test/deny clean; (f) `REVIEW.md` approved. **0061 is backend-spawning → LOCAL-RED expected on this dev host, GREEN on CI — CI AUTHORITATIVE.** Consumes M52-1; new M53-1 (the deterministic `UC` rcd).

## Process notes

- **§6.1 split: did NOT fire** — 9 tasks / ~300 LoC, well under the ~25-task / ~1500-LoC gate. **ADR-0111 stays reserved-but-unfired** (reclaimed by the next NEW phase pick per the lapsed-reservation convention).
- **§6.2 reconciliation: ADR-0112 NOT fired** — the recon confirmed every §A–§G fact; the read-then-close posture and `{{CLOSE_BACKEND_PORT}}` marker are SPEC-§C-delegated PLAN choices, not fact-overturns.
- One state per session (§5.1): this session ends after the PLAN-write + plan-review. The state-3 implementation is the session after.
