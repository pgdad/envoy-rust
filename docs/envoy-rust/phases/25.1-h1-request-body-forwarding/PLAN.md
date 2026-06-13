# Phase 25.1 — H1 Request-Body Forwarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (the project default per `feedback_execution_style`; dispatch implementers SERIALLY per `feedback_serial_subagent_dispatch`) to implement this plan task-by-task. Every task is TDD (test first). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the pre-existing H1 request-body-forwarding gap — read the Content-Length-delimited downstream request body into a `Bytes` BEFORE the filter pipeline, expose it to the pipeline as `FilterRequest.body`, and forward it to the upstream (replacing the always-empty `Some(Bytes::new())`).

**Architecture:** A single, contained change to the H1 router data path in `crates/envoy-http1/src/hcm.rs`. The body read is RELOCATED from the post-response discard-drain (`:678-697`) to BEFORE the filter-pipeline boundary conversion (`:631`), accumulating into a `Bytes` instead of discarding. The per-attempt upstream request (`run_attempt`, `:315`) forwards `req.body.clone()` (replay-safe across retries, mirroring the H2 side). When `body_len == 0` (every existing fixture — they are bodyless or `content-length: 0`) the new path is a body-wise no-op, so all 32 existing Docker-gated fixtures stay green (the load-bearing regression-equivalence invariant). NO new fixture, NO new crate, NO new dependency.

**Tech Stack:** Rust, `tokio` (async socket reads), `bytes` (`Bytes`/`BytesMut`), the existing `envoy-http1` HCM + `envoy-filter::FilterRequest`.

**Scope locked by:** ADR-0064 (split), ADR-0063 (parent §6.2 reconciliation), ADR-0062 (parent scope). SPEC: `docs/envoy-rust/phases/25.1-h1-request-body-forwarding/SPEC.md`.

**Code anchors (code-HEAD `9b0e7b925`, verify line numbers before editing — the file is ~5228 lines):**
- `run_attempt(config, cluster, cluster_name, req: &Request, host_header, close)` signature `hcm.rs:315-321`; `out_req` build `:349-357`; the always-empty `body: Some(Bytes::new())` `:356`.
- Per-request handler: `body_len = parse_content_length(&req.headers)?` `:597`; `chunked` `:598`; `request_body_len` `:608`; `let mut req = req;` `:615`; pipeline clone `:616`; `resolve_route` + `apply_route_config` `:624-625`; the boundary conversion `filter_req` `:631-636` (`body: req.body.take()` `:635`); write-back `:639-642`; `let consumed = req.bytes_consumed; buf.advance(consumed);` `:676-677`; the discard-drain loop `:678-697`.
- `FilterRequest { method, path, headers, body: Option<Bytes> }` — `crates/envoy-filter/src/types.rs:28-35`.
- Test helpers (in the `hcm.rs` `#[cfg(test)] mod tests`): `spawn_in_process_upstream(response: &'static [u8]) -> u16`; `cluster_mgr_with_endpoint(name, port).await -> Arc<ClusterManager>`; `hcm_config_with_cluster(prefix, RouteAction::Route(RouteAction_Route { cluster, retry_policy }), cluster_mgr) -> Arc<HCMConfig>`; `drive(cfg, req_bytes).await -> Vec<u8>` (drives a raw request through the HCM, returns the downstream response bytes).

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/envoy-http1/src/hcm.rs` | H1 HCM + router data path | MODIFY — relocate the body read before the pipeline; forward `req.body.clone()` upstream; remove the discard-drain; add tests. |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | differential equivalence contract | MODIFY — add the "Request body forwarding (HTTP/1.1)" note (Task 3). |

No other files change. (`envoy-filter::FilterRequest.body` already exists; the H2 side already forwards bodies.)

---

## Task 1: H1 forwards the Content-Length request body upstream

**Files:**
- Modify: `crates/envoy-http1/src/hcm.rs` (the per-request handler body region ~`:580-697`; `run_attempt` `:356`; the test module).
- Test: `crates/envoy-http1/src/hcm.rs` (inline `#[cfg(test)] mod tests`).

- [ ] **Step 1: Add a capturing in-process upstream test helper.**

In the `hcm.rs` test module, next to `spawn_in_process_upstream`, add a variant that records the bytes the upstream received so the test can assert the forwarded body. (`std::sync::{Arc, Mutex}` / `tokio::time::Duration` / `tokio::io::{AsyncReadExt, AsyncWriteExt}` / `tokio::net::TcpListener` are already imported by the existing helpers.)

```rust
/// 25.1 D1: like `spawn_in_process_upstream`, but RECORDS the bytes the
/// upstream received (request head + body) into the returned shared buffer,
/// so a test can assert the forwarded request body. Reads in a loop with a
/// short per-read timeout; once a read times out (the small test request has
/// fully arrived) it stops reading and writes the canned `response`. Returns
/// `(port, captured)`.
async fn spawn_capturing_upstream(
    response: &'static [u8],
) -> (u16, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured_acceptor = captured.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let captured_conn = captured_acceptor.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                loop {
                    match tokio::time::timeout(
                        Duration::from_millis(200),
                        sock.read(&mut buf),
                    )
                    .await
                    {
                        Ok(Ok(0)) => break,            // peer closed
                        Ok(Ok(n)) => captured_conn.lock().unwrap().extend_from_slice(&buf[..n]),
                        Ok(Err(_)) => break,           // io error
                        Err(_elapsed) => break,        // request fully arrived
                    }
                }
                let _ = sock.write_all(response).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (port, captured)
}
```

- [ ] **Step 2: Write the failing test.**

Add this test to the `hcm.rs` test module (model: `route_walk_dispatches_route_action_to_client_connect`, `:2457`). It drives an H1 `POST` with a Content-Length body through the HCM and asserts the upstream received the body bytes.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_forwards_request_body_upstream() {
    // 25.1 D1: an H1 POST with a Content-Length-delimited body must reach the
    // upstream with its body intact (today it does not — the router forwards an
    // always-empty body and drains-and-discards the downstream body).
    let upstream_response: &'static [u8] =
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    let (upstream_port, captured) = spawn_capturing_upstream(upstream_response).await;
    let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
    let cfg = hcm_config_with_cluster(
        "/",
        RouteAction::Route(RouteAction_Route {
            cluster: "backend".into(),
            retry_policy: None,
        }),
        cluster_mgr,
    );
    let req = b"POST /submit HTTP/1.1\r\nHost: x.test\r\nContent-Length: 11\r\nConnection: close\r\n\r\nhello world";
    let resp = drive(cfg, req).await;
    let s = String::from_utf8_lossy(&resp);
    assert!(s.starts_with("HTTP/1.1 200 OK\r\n"), "downstream got 200: {s}");

    let got = captured.lock().unwrap().clone();
    let got_str = String::from_utf8_lossy(&got);
    assert!(
        got_str.starts_with("POST /submit HTTP/1.1\r\n"),
        "upstream received the request line: {got_str}"
    );
    assert!(
        got_str.ends_with("hello world"),
        "upstream received the request body bytes: {got_str}"
    );
}
```

- [ ] **Step 3: Run the test to verify it FAILS.**

Run: `cargo test -p envoy-http1 h1_forwards_request_body_upstream -- --nocapture`
Expected: FAIL — the assertion `upstream received the request body bytes` fails because today the upstream receives an empty body (`out_req.body = Some(Bytes::new())`) and the downstream body is drained-and-discarded.

- [ ] **Step 4: Relocate the body read to BEFORE the filter pipeline (accumulate into `Bytes`).**

In the per-request handler, AFTER `let mut req = req;` (`:615`) and BEFORE the boundary conversion `let mut filter_req = …` (`:631`), insert the body-read block below. It advances `buf` past the request head, then accumulates exactly `body_len` body bytes (those already in `buf` plus any remaining read from `downstream`) into a `Bytes`, and stores it in `req.body`. For `body_len == 0` (every existing fixture) it is a no-op beyond the head advance.

Add `BytesMut` to the `bytes` import at the top of the file if not already present (the file imports `Bytes`; check the `use bytes::…` line). Insert:

```rust
        // 25.1 D1: read the Content-Length-delimited request body into `req.body`
        // BEFORE the filter pipeline, so a body-dependent filter (phase 25.2's
        // buffer) can length-check it and so the router arm can forward it
        // upstream. This REPLACES the former post-response discard-drain. Chunked
        // requests carry no Content-Length (`body_len == 0`) and are 501-rejected
        // below without a body read. The idle-read-timeout → `Ok(())` graceful
        // close, the `UnexpectedEof`, and the io-error dispositions match the
        // former drain loop verbatim.
        let consumed = req.bytes_consumed;
        buf.advance(consumed);
        let request_body: Bytes = if body_len > 0 {
            let mut body_buf = BytesMut::with_capacity(body_len);
            let from_buf = buf.len().min(body_len);
            body_buf.extend_from_slice(&buf[..from_buf]);
            buf.advance(from_buf);
            let mut remaining = body_len - from_buf;
            while remaining > 0 {
                let mut chunk = [0u8; 4096];
                let to_read = chunk.len().min(remaining);
                let n = match tokio::time::timeout(
                    IDLE_READ_TIMEOUT,
                    downstream.read(&mut chunk[..to_read]),
                )
                .await
                {
                    Ok(Ok(0)) => return Err(Http1Error::UnexpectedEof),
                    Ok(Ok(n)) => n,
                    Ok(Err(source)) => return Err(Http1Error::Io { source }),
                    Err(_elapsed) => return Ok(()),
                };
                body_buf.extend_from_slice(&chunk[..n]);
                remaining -= n;
            }
            body_buf.freeze()
        } else {
            Bytes::new()
        };
        req.body = Some(request_body);
```

NOTE: `req.body` is now `Some(<the body>)`, so the existing boundary conversion `body: req.body.take()` (`:635`) hands the real body to `decode_headers`, and the write-back `req.body = filter_req.body;` (`:642`) preserves any filter mutation. No change to those lines.

- [ ] **Step 5: Forward the body upstream in `run_attempt`.**

At `hcm.rs:356`, change the always-empty body to clone the (now-populated) `req.body`. The clone is per-attempt (the retry loop re-invokes `run_attempt`), so a retried request replays the same body — replay-safe, mirroring the H2 buffered-clone (ADR-0044).

Replace:
```rust
        // Chunked-request-body forwarding is a SPEC §4 non-goal.
        body: Some(Bytes::new()),
```
with:
```rust
        // 25.1 D1: forward the downstream request body (read before the pipeline)
        // upstream. Cloned per attempt → replay-safe across retries (mirrors the
        // H2 buffered-clone, ADR-0044). Chunked/streaming bodies remain a non-goal
        // (chunked is 501-rejected before any body read; `req.body` is then empty).
        body: req.body.clone(),
```

- [ ] **Step 6: Remove the now-dead discard-drain.**

DELETE the old head-advance + discard-drain block at `:676-697` (it is now performed before the pipeline in Step 4). Concretely, remove:
```rust
        // 6. Advance the buffer past the consumed request + body.
        let consumed = req.bytes_consumed;
        buf.advance(consumed);
        // 7. Drain body bytes (read_exact-style; up to body_len).
        let drained_so_far = buf.len().min(body_len);
        buf.advance(drained_so_far);
        let mut remaining = body_len - drained_so_far;
        while remaining > 0 {
            let mut throwaway = [0u8; 4096];
            let to_read = throwaway.len().min(remaining);
            let n = match tokio::time::timeout(
                IDLE_READ_TIMEOUT,
                downstream.read(&mut throwaway[..to_read]),
            )
            .await
            {
                Ok(Ok(0)) => return Err(Http1Error::UnexpectedEof),
                Ok(Ok(n)) => n,
                Ok(Err(source)) => return Err(Http1Error::Io { source }),
                Err(_elapsed) => return Ok(()),
            };
            remaining -= n;
        }
```
Verify nothing later in the handler reads `consumed`, `drained_so_far`, or `remaining` (they were local to this block). The body bytes are already consumed from `buf`/`downstream` by Step 4, so the next keep-alive request parses from a clean buffer position.

- [ ] **Step 7: Run the test to verify it PASSES.**

Run: `cargo test -p envoy-http1 h1_forwards_request_body_upstream -- --nocapture`
Expected: PASS — the upstream now receives `POST /submit …\r\n\r\nhello world`.

- [ ] **Step 8: Run the full `envoy-http1` test suite to catch local regressions.**

Run: `cargo test -p envoy-http1`
Expected: PASS (all existing H1 tests still green — body-read is a no-op for the bodyless tests).

- [ ] **Step 9: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 25.1 Task 1: H1 forwards Content-Length request body upstream"
```

---

## Task 2: Regression invariants — chunked 501, bodyless unchanged, keep-alive pipelining, retry replay

**Files:**
- Test: `crates/envoy-http1/src/hcm.rs` (inline test module).

- [ ] **Step 1: Write a test that a chunked request is still 501-rejected (no body read).**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_chunked_request_still_501_after_body_forwarding() {
    // 25.1 D1 regression: chunked requests carry no Content-Length, so the
    // body-read is skipped (body_len == 0) and the existing 501 rejection stands.
    let (upstream_port, _captured) =
        spawn_capturing_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
    let cfg = hcm_config_with_cluster(
        "/",
        RouteAction::Route(RouteAction_Route {
            cluster: "backend".into(),
            retry_policy: None,
        }),
        cluster_mgr,
    );
    let req = b"POST /c HTTP/1.1\r\nHost: x.test\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n0\r\n\r\n";
    let resp = drive(cfg, req).await;
    let s = String::from_utf8_lossy(&resp);
    assert!(
        s.starts_with("HTTP/1.1 501 "),
        "chunked request is 501-rejected: {s}"
    );
}
```

- [ ] **Step 2: Write a test that a bodyless GET is unchanged (proxies, upstream sees no body).**

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_bodyless_get_unchanged_after_body_forwarding() {
    // 25.1 D1 regression: a GET with no body proxies exactly as before
    // (body_len == 0 → the body-read block is a no-op beyond the head advance).
    let (upstream_port, captured) =
        spawn_capturing_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
    let cfg = hcm_config_with_cluster(
        "/",
        RouteAction::Route(RouteAction_Route {
            cluster: "backend".into(),
            retry_policy: None,
        }),
        cluster_mgr,
    );
    let req = b"GET /g HTTP/1.1\r\nHost: x.test\r\nConnection: close\r\n\r\n";
    let resp = drive(cfg, req).await;
    assert!(
        String::from_utf8_lossy(&resp).starts_with("HTTP/1.1 200 OK\r\n"),
        "bodyless GET proxies"
    );
    let got = String::from_utf8_lossy(&captured.lock().unwrap()).to_string();
    assert!(got.starts_with("GET /g HTTP/1.1\r\n"), "upstream got the GET: {got}");
    // No request body bytes were appended after the head terminator.
    let body_after_head = got.split("\r\n\r\n").nth(1).unwrap_or("");
    assert!(body_after_head.is_empty(), "no body forwarded for a GET: {got:?}");
}
```

- [ ] **Step 3: Write a keep-alive pipelining test — two POSTs with bodies on ONE connection, the first body must not bleed into the second request.**

This is the highest-value regression test: it proves the body read advances `buf` correctly so the next request parses from a clean position. Use the existing multi-request keep-alive driver if present (search the test module for a driver that writes multiple requests on one connection, e.g. the pattern at `:1936` writing a second request, or `Driver::Http1KeepAlive` usage); otherwise drive two requests on one `drive`-style connection. Concretely, model it on the existing keep-alive test infrastructure in `hcm.rs`/`client.rs`. The assertion: the upstream receives BOTH `POST /one …\r\n\r\naaa` and `POST /two …\r\n\r\nbbbb` with their correct, non-interleaved bodies, and the downstream gets two 200s.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_keep_alive_two_bodied_posts_do_not_bleed() {
    // 25.1 D1 regression: on a single keep-alive connection, request 1's body
    // bytes must be fully consumed from `buf` so request 2 parses cleanly.
    let (upstream_port, captured) =
        spawn_capturing_upstream(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").await;
    let cluster_mgr = cluster_mgr_with_endpoint("backend", upstream_port).await;
    let cfg = hcm_config_with_cluster(
        "/",
        RouteAction::Route(RouteAction_Route {
            cluster: "backend".into(),
            retry_policy: None,
        }),
        cluster_mgr,
    );
    // Two pipelined requests on ONE connection (no `Connection: close` on the
    // first). Use the test module's existing single-connection multi-request
    // driver (model: the second-request write at hcm.rs:1936). Pseudocode:
    //   write "POST /one … Content-Length: 3\r\n\r\naaa"
    //   write "POST /two … Content-Length: 4\r\nConnection: close\r\n\r\nbbbb"
    //   read both responses.
    let resps = drive_keep_alive(
        cfg,
        &[
            b"POST /one HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\naaa",
            b"POST /two HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbbbb",
        ],
    )
    .await;
    assert_eq!(resps.len(), 2, "two responses");
    assert!(resps.iter().all(|r| String::from_utf8_lossy(r).starts_with("HTTP/1.1 200 OK")));
    let got = String::from_utf8_lossy(&captured.lock().unwrap()).to_string();
    assert!(got.contains("POST /one"), "upstream saw request 1: {got}");
    assert!(got.contains("aaa"), "upstream saw body 1: {got}");
    assert!(got.contains("POST /two"), "upstream saw request 2 (clean parse): {got}");
    assert!(got.contains("bbbb"), "upstream saw body 2: {got}");
}
```

NOTE for the implementer: if the test module has no `drive_keep_alive` multi-request helper, write a minimal one next to `drive` (open one `TcpStream` to the HCM-served listener, write each request, read each response framed by `Content-Length`/`Connection: close`). Reuse the exact connection-driving idiom `drive` already uses; do not invent a new HCM entry point.

- [ ] **Step 4: Write a retry-replay test — a retried POST replays the body.**

Use the 16.x stateful fail-then-succeed backend pattern (search the test module / `tests` for `retry`-related helpers; model on the phase-16 retry tests). Configure a route with `retry_policy: Some(RetryPolicy { retry_on: "5xx", num_retries: 1, … })` and a backend that 500s once then 200s; assert the SECOND (retried) upstream attempt received the same body bytes.

```rust
#[tokio::test(flavor = "multi_thread")]
async fn h1_retried_post_replays_body() {
    // 25.1 D1 regression: out_req.body = req.body.clone() per attempt → a retried
    // POST replays the same body (replay-safe, ADR-0044). Model on the phase-16
    // retry tests: a fail-then-succeed backend that records each attempt's body.
    // Assert the retried attempt received "replayme".
    // (See the phase-16 retry test helpers for the stateful-backend + retry_policy
    //  construction; reuse them verbatim — do NOT hand-roll a new retry harness.)
    // ... construct cfg with retry_policy num_retries:1, drive a POST with body
    //     "replayme", assert both attempts (or at least the final) saw the body.
}
```

NOTE: if reusing the phase-16 stateful-backend helper proves heavy, this replay property is also covered transitively by the per-attempt `req.body.clone()` being the only body source. At minimum, assert via a 2-attempt capturing backend that the body appears in the retried request. If the phase-16 helpers are not reachable from `hcm.rs`'s test module, mark this as covered by the in-process backstop and the fixture-0033 path in `25.2`, and leave a one-line `// 25.1 retry-replay: covered by …` comment rather than a stub test (no placeholder tests — D-3 anti-pattern).

- [ ] **Step 5: Run the regression tests.**

Run: `cargo test -p envoy-http1`
Expected: PASS (the new regression tests + all pre-existing H1 tests green).

- [ ] **Step 6: Commit.**

```bash
git add crates/envoy-http1/src/hcm.rs
git commit -m "phase 25.1 Task 2: regression tests — chunked 501, bodyless, keep-alive pipelining, retry replay"
```

---

## Task 3: BEHAVIOR_CONTRACT note + isolated-crate & workspace verification

**Files:**
- Modify: `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (add the request-body-forwarding note).

- [ ] **Step 1: Add the "Request body forwarding (HTTP/1.1)" note to BEHAVIOR_CONTRACT.md.**

Add a short section (after the `## Response body — no-healthy-upstream synth-503` section, before `## Header allow-list`):

```markdown
## Request body forwarding (HTTP/1.1)

As of phase 25.1, the HTTP/1.1 router forwards the **Content-Length-delimited**
downstream request body to the upstream verbatim (it is read into a `Bytes`
before the filter pipeline runs, exposed to the pipeline as `FilterRequest.body`,
and cloned per upstream attempt — replay-safe across retries). This closes the
pre-existing phase-04.3 gap where H1 forwarded an always-empty body. The body is
compared cross-proxy byte-exact under the existing `response_body` / echo-server
fixtures (differentially proven by fixture `0033-http-filter-buffer` in phase
25.2). **Chunked / streaming request bodies remain a non-goal** — a
`Transfer-Encoding: chunked` request is 501-rejected before any body read.
HTTP/2 already buffers and forwards request bodies (unchanged).
```

- [ ] **Step 2: Run the isolated-crate build (per `project_isolated_crate_build_blindspot`).**

Run: `cargo build -p envoy-http1`
Expected: clean (no per-crate feature-unification blind spot).

- [ ] **Step 3: Run the workspace state-3 gates (the full §7.5 differential is the NEXT state).**

Run: `cargo build --workspace --all-targets`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Run: `cargo test --workspace` (run `-p envoy-bin` helpers standalone if the nested-cargo backstop flakes — `project_workspace_test_nested_cargo_backstop_flake`)
Expected: all clean. (`cargo deny check`, the Docker differential, and the fuzz short-run are the state-4 `superpowers:verification-before-completion` gate — NOT this state.)

- [ ] **Step 4: Commit.**

```bash
git add docs/envoy-rust/BEHAVIOR_CONTRACT.md
git commit -m "phase 25.1 Task 3: BEHAVIOR_CONTRACT request-body-forwarding note + state-3 verification"
```

---

## Self-Review (run after the plan is written; checklist, not a dispatch)

1. **Spec coverage:** SPEC §3 D1 = Task 1 (read-before-pipeline + forward + remove drain). SPEC §3.3 invariants (chunked 501 / bodyless / keep-alive / retry replay) = Task 2. SPEC §2 BEHAVIOR_CONTRACT note = Task 3. SPEC §1 acceptance "all 32 fixtures green" = the state-4 gate (next state), set up by Task 3's workspace gates. No SPEC requirement is unmapped.
2. **Placeholder scan:** Task 2 Step 4 (retry replay) is the only soft spot — it gives an explicit fallback (reuse phase-16 helpers, or a documented one-line comment, NOT a stub test). All other steps carry complete code.
3. **Type consistency:** `spawn_capturing_upstream → (u16, Arc<Mutex<Vec<u8>>>)` used consistently in Tasks 1-2; `RouteAction::Route(RouteAction_Route { cluster, retry_policy })` matches the existing test idiom; `req.body: Option<Bytes>` written in Step 4, cloned in Step 5, read-back unchanged at `:642`.

---

## Execution notes

- **State-3 is subagent-driven** (`feedback_execution_style`); dispatch implementers SERIALLY (`feedback_serial_subagent_dispatch` — parallel implementers race on shared `main` and this harness garbles large parallel tool batches).
- **Pre-build the helpers** before any Docker work later (`project_flaky_access_log_fixture_0012`); not needed for this state's pure-Rust tests.
- After Task 3, phase 25.1 is at state-3-complete → the NEXT session runs state-4 `superpowers:verification-before-completion` (the full §7.5 gate: workspace + clippy + fmt + test + deny + the Docker 32-fixture differential LOCALLY per `feedback_state4_runs_docker_differential`, with the AUTHORITATIVE Linux CI anchor per ADR-0049 + the fixture flake family `project_flaky_access_log_fixture_0012`).
