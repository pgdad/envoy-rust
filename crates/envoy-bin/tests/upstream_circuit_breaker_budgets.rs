//! 17 Task 9 (ADR-0047): in-process backstop for the circuit-breaker BUDGET
//! paths landed in Tasks 1-7. Mirrors the boot/harness shape of
//! `upstream_retry.rs` (the phase-16 backstop) and `upstream_circuit_breaker.rs`
//! (the phase-15 overflow backstop): boot `envoy-bin` from a tempfile bootstrap,
//! drive real H1 requests at an in-process backend, and scrape the admin
//! `/stats` endpoint.
//!
//! Four paths. The first three are the in-process equivalents of fixture-0025's
//! three probes (sequential, single-request budget outcomes); the fourth is the
//! ONE regime the differential fixture deliberately omits — budget caps ABOVE
//! zero tripped by CONCURRENT load, including the momentary `*_open` gauge
//! observed at its NON-ZERO edge (scrape mid-flight while slots are held). This
//! file is the ONLY place in the project where the >0-cap concurrency behavior
//! and the non-zero gauge edges are asserted.
//!
//!   (i)  budget-blocked retry (probe-1 equivalent) — cluster `budget_zero`
//!        (`max_retries:0, track_remaining:true`) + retry_policy + an
//!        always-503 backend. The retry the policy would dispatch is BLOCKED by
//!        the zero retry budget: the attempt-1 503 surfaces VERBATIM (backend
//!        body), `x-envoy-attempt-count: 1`, `upstream_rq_retry_overflow: 1`,
//!        `upstream_rq_retry: 0`, `upstream_rq_total: 1`.
//!
//!   (ii) budget-allowed retry (probe-2 equivalent) — cluster `budget_default`
//!        (only `track_remaining:true`, caps default to 3/1024) + retry_policy +
//!        a 503-then-200 backend. The retry is admitted: final 200,
//!        `x-envoy-attempt-count: 2`; post-settle gauges `remaining_retries: 3`
//!        and `remaining_rq: 1024` (the L5 defaults read back through the full
//!        stack at rest).
//!
//!   (iii) request-breaker overflow (probe-3 equivalent) — cluster `rq_zero`
//!        (`max_requests:0`, no retry_policy). The request is rejected BEFORE any
//!        upstream connect: byte-exact 81-byte overflow local-reply 503 +
//!        `x-envoy-overloaded: true` + `x-envoy-attempt-count: 1`,
//!        `upstream_rq_pending_overflow: 1`, `upstream_rq_5xx: 1` (L3), and the
//!        backend is NEVER contacted (the backend's request counter stays 0).
//!
//!   (iv) the >0-cap CONCURRENCY regime (THE ONLY PLACE THIS IS ASSERTED):
//!        (iv-a) cluster `rq_one` (`max_requests:1`) + a SLOW backend (holds the
//!               response) + TWO concurrent requests → exactly one 200 + one
//!               503-overflow, `upstream_rq_pending_overflow: 1`, AND
//!               `circuit_breakers.default.rq_open == 1` observed DURING the hold
//!               (the L4 momentary-gauge non-zero rising edge) then `0` after.
//!        (iv-b) cluster `retry_one` (`max_retries:1`) + retry_policy + a SLOW
//!               always-503 backend + TWO concurrent requests → exactly one
//!               `upstream_rq_retry_overflow` tick (one request wins the single
//!               retry slot, the other is budget-blocked) and a combined
//!               `upstream_rq_retry: 1`.
//!
//! Concurrency synchronization design (path iv) — why it is deterministic:
//!
//!   * The request-budget guard (iv-a) is held across the WHOLE dispatch (the
//!     retry loop in `envoy-http1/src/hcm.rs`), i.e. for the entire upstream
//!     request duration. With a backend that holds the response for ~1s, the
//!     winning request keeps `active_requests == 1` (→ `rq_open == 1`) the whole
//!     time. Two requests are launched with `tokio::join!`; the loser hits the
//!     `max_requests:1` cap immediately and overflows (no wait). A `probe` future
//!     joined alongside polls `/stats` for `rq_open == 1` within a bounded window
//!     that is much shorter than the hold (poll budget 700ms, hold 1500ms), so
//!     the mid-flight scrape lands deterministically while the slot is held.
//!     After both requests complete and their guards drop, `rq_open` returns to 0
//!     (re-synced by the guard's `Drop` — the single-source-of-truth discipline).
//!
//!   * The retry-budget guard (iv-b) is held across the back-off + the in-flight
//!     RETRY attempt. Both requests' FIRST attempts hit the same slow always-503
//!     backend and complete at roughly the same time (after the hold). Both then
//!     reach `try_acquire_retry` near-simultaneously; the single retry slot
//!     (`max_retries:1`) admits exactly one. The other gets `Rejected` IMMEDIATELY
//!     (the slot is already held — it does not need to wait for the winner's retry
//!     to finish), so the contention is decided at acquisition time, not at retry
//!     completion. The slow backend forces the two first-attempts to overlap,
//!     which is what guarantees both retry decisions happen while the slot is
//!     contended (without the hold, one request could finish its entire retry —
//!     releasing the slot — before the other asks, yielding 0 overflows). Hence
//!     exactly one `upstream_rq_retry_overflow` and a combined `upstream_rq_retry`
//!     of 1.
//!
//! Per phase-09 REVIEW M3 disposition: uses `tokio::process::Command` with
//! `.kill_on_drop(true)`, `stdout: Stdio::null()`, `stderr: Stdio::piped()`.

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod common;

use common::{
    assert_stat, http1_oneshot, poll_stat_until, reserve_port, scrape_admin_stats, wait_ready,
};

/// The byte-exact Envoy overflow local-reply body (81 bytes, no trailing newline).
const OVERFLOW_BODY: &[u8] =
    b"upstream connect error or disconnect/reset before headers. reset reason: overflow";

fn assert_header_value(headers: &[(String, String)], name: &str, expected: &str, ctx: &str) {
    let val = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_else(|| panic!("header {name:?} absent on {ctx}\nactual: {headers:?}",));
    assert_eq!(
        val, expected,
        "header {name:?} on {ctx}: expected {expected:?}, got {val:?}"
    );
}

fn assert_header_present(headers: &[(String, String)], name: &str, ctx: &str) {
    assert!(
        headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name)),
        "header {name:?} absent on {ctx}\nactual: {headers:?}",
    );
}

// ── in-process backends ───────────────────────────────────────────────────────

/// Write a minimal H1 response (status + body), echoing the keep-alive decision.
async fn write_response(
    sock: &mut TcpStream,
    status: u16,
    reason: &str,
    body: &[u8],
    wants_close: bool,
) {
    let conn = if wants_close { "close" } else { "keep-alive" };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         content-length: {len}\r\n\
         content-type: text/plain\r\n\
         connection: {conn}\r\n\r\n",
        len = body.len(),
    );
    let _ = sock.write_all(resp.as_bytes()).await;
    if !body.is_empty() {
        let _ = sock.write_all(body).await;
    }
    let _ = sock.flush().await;
}

/// Spawn an in-process H1 keep-alive backend whose behavior depends on the
/// request path, with a shared `request_ctr` incremented on EVERY served request
/// (used by path iii to assert zero backend contact). The connection serves a
/// keep-alive request loop (mirroring the phase-16 `upstream_retry.rs` backend)
/// so the envoy-rust H1 upstream pool can REUSE the connection across retry
/// attempts — a fresh-connection-per-attempt backend would race the pool's
/// keep-alive reuse and surface spurious 502 resets on the retried attempt.
///
/// Path routing:
/// - `/blocked` → 503 `service unavailable\n` (always; the retry the policy
///   would fire is gated away upstream by max_retries:0).
/// - `/allowed` → 503-then-200 via a per-path cyclic counter (fail:1 window —
///   first 503 `fail\n`, second 200 `ok\n`), so an admitted retry succeeds.
/// - `/slow-200` → sleep `hold` then 200 `ok\n` (iv-a slow backend).
/// - `/slow-503` → sleep `hold` then 503 `fail\n` (iv-b slow backend).
/// - anything else → 200 `ok\n`.
async fn spawn_backend(hold: Duration, request_ctr: Arc<AtomicU64>) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind backend");
    let port = listener.local_addr().unwrap().port();
    // Per-path cyclic counter for the 503-then-200 `/allowed` window. Global
    // across connections (the proxy may use a fresh OR reused conn per attempt).
    let allowed_ctr = Arc::new(AtomicU64::new(0));
    tokio::spawn(async move {
        loop {
            let (sock, _peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            let request_ctr = request_ctr.clone();
            let allowed_ctr = allowed_ctr.clone();
            tokio::spawn(serve_backend_conn(sock, hold, request_ctr, allowed_ctr));
        }
    });
    port
}

/// Serve one upstream TCP connection with a keep-alive request loop: read a
/// request head, dispatch by path, respond, and loop until the client signals
/// `Connection: close` (or EOF).
async fn serve_backend_conn(
    mut sock: TcpStream,
    hold: Duration,
    request_ctr: Arc<AtomicU64>,
    allowed_ctr: Arc<AtomicU64>,
) {
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    loop {
        // Read until we have a full request head (`\r\n\r\n`).
        let head_end = loop {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
            let mut chunk = [0u8; 512];
            match sock.read(&mut chunk).await {
                Ok(0) => return, // Clean EOF between requests.
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(_) => return,
            }
        };

        let head = std::str::from_utf8(&buf[..head_end]).unwrap_or("");
        let path = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("/")
            .to_string();
        let wants_close = head.lines().any(|l| {
            l.to_ascii_lowercase().starts_with("connection:")
                && l.to_ascii_lowercase().contains("close")
        });
        buf.drain(..head_end);

        // Count this served request (path iii asserts this stays 0).
        request_ctr.fetch_add(1, Ordering::Relaxed);

        if path.starts_with("/blocked") {
            write_response(
                &mut sock,
                503,
                "Service Unavailable",
                b"service unavailable\n",
                wants_close,
            )
            .await;
        } else if path.starts_with("/allowed") {
            // fail:1 cyclic window: idx 0 → 503, idx 1 → 200, …
            let idx = allowed_ctr.fetch_add(1, Ordering::Relaxed);
            if idx.is_multiple_of(2) {
                write_response(
                    &mut sock,
                    503,
                    "Service Unavailable",
                    b"fail\n",
                    wants_close,
                )
                .await;
            } else {
                write_response(&mut sock, 200, "OK", b"ok\n", wants_close).await;
            }
        } else if path.starts_with("/slow-200") {
            // Hold so concurrent requests overlap + the request-budget slot stays
            // held long enough for the mid-flight rq_open scrape.
            tokio::time::sleep(hold).await;
            write_response(&mut sock, 200, "OK", b"ok\n", wants_close).await;
        } else if path.starts_with("/slow-503") {
            // Hold so both first-attempts overlap, forcing the retry-budget slot
            // to be contended at acquisition time (iv-b).
            tokio::time::sleep(hold).await;
            write_response(
                &mut sock,
                503,
                "Service Unavailable",
                b"fail\n",
                wants_close,
            )
            .await;
        } else {
            write_response(&mut sock, 200, "OK", b"ok\n", wants_close).await;
        }

        if wants_close {
            return;
        }
    }
}

// ── bootstrap ─────────────────────────────────────────────────────────────────

/// Boot `envoy-bin` with five STATIC clusters (all pointing at `backend_port`),
/// an HCM listener on `hcm_port` (HTTP1), admin on `admin_port`. The vhost has
/// `include_attempt_count_in_response: true`. Routes:
///   /blocked      → budget_zero    (max_retries:0, track_remaining) + retry_policy
///   /allowed      → budget_default (track_remaining only)            + retry_policy
///   /rq-blocked   → rq_zero        (max_requests:0)                  (no retry)
///   /slow-200     → rq_one         (max_requests:1)                  (no retry)
///   /slow-503     → retry_one      (max_retries:1, track_remaining)  + retry_policy
async fn spawn_envoy_bin(
    hcm_port: u16,
    admin_port: u16,
    backend_port: u16,
) -> tokio::process::Child {
    let cluster = |name: &str, breakers: &str| {
        format!(
            r#"    - name: {name}
      type: STATIC
      lb_policy: ROUND_ROBIN
{breakers}      load_assignment:
        cluster_name: {name}
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }} }}
"#
        )
    };
    let clusters = format!(
        "{}{}{}{}{}",
        cluster(
            "budget_zero",
            "      circuit_breakers:\n        thresholds:\n          - priority: DEFAULT\n            max_retries: 0\n            track_remaining: true\n",
        ),
        cluster(
            "budget_default",
            "      circuit_breakers:\n        thresholds:\n          - priority: DEFAULT\n            track_remaining: true\n",
        ),
        cluster(
            "rq_zero",
            "      circuit_breakers:\n        thresholds:\n          - priority: DEFAULT\n            max_requests: 0\n",
        ),
        cluster(
            "rq_one",
            "      circuit_breakers:\n        thresholds:\n          - priority: DEFAULT\n            max_requests: 1\n",
        ),
        cluster(
            "retry_one",
            "      circuit_breakers:\n        thresholds:\n          - priority: DEFAULT\n            max_retries: 1\n            track_remaining: true\n",
        ),
    );

    let bootstrap = format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {hcm_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                codec_type: HTTP1
                stat_prefix: ingress_http
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      include_attempt_count_in_response: true
                      routes:
                        - match: {{ prefix: "/blocked" }}
                          route:
                            cluster: budget_zero
                            retry_policy: {{ retry_on: "5xx", num_retries: 1 }}
                        - match: {{ prefix: "/allowed" }}
                          route:
                            cluster: budget_default
                            retry_policy: {{ retry_on: "5xx", num_retries: 1 }}
                        - match: {{ prefix: "/rq-blocked" }}
                          route: {{ cluster: rq_zero }}
                        - match: {{ prefix: "/slow-200" }}
                          route: {{ cluster: rq_one }}
                        - match: {{ prefix: "/slow-503" }}
                          route:
                            cluster: retry_one
                            retry_policy: {{ retry_on: "5xx", num_retries: 1 }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
{clusters}"#
    );
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    tmp.write_all(bootstrap.as_bytes())
        .expect("write bootstrap");
    let path = tmp.path().to_path_buf();
    // Persist tempfile by leaking it; the test process exits shortly.
    std::mem::forget(tmp);
    tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin")
}

/// Boot once and exercise all four budget paths against one envoy-bin instance.
/// Per-cluster counters are independent (each path uses its own cluster), so the
/// paths do not interfere; this keeps boot latency paid once.
#[tokio::test(flavor = "multi_thread")]
async fn budget_backstop_four_paths() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();

    // Slow-backend hold for the path-iv concurrency probes. Comfortably longer
    // than the mid-flight scrape latency: hold 1500ms, scrape within 700ms.
    let hold = Duration::from_millis(1500);
    let backend_ctr = Arc::new(AtomicU64::new(0));
    let backend_port = spawn_backend(hold, backend_ctr.clone()).await;

    let _envoy = spawn_envoy_bin(hcm_port, admin_port, backend_port).await;
    wait_ready(hcm_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin admin ready");

    // ─────────────────────────────────────────────────────────────────────────
    // (i) budget-blocked retry: /blocked → budget_zero (max_retries:0). The
    // policy's retry is gated away; the attempt-1 503 surfaces verbatim.
    // ─────────────────────────────────────────────────────────────────────────
    let (status, headers, body) = http1_oneshot(hcm_addr, "/blocked", "backend").await;
    assert_eq!(status, 503, "(i) budget-blocked: expected verbatim 503");
    assert_eq!(
        body, b"service unavailable\n",
        "(i) budget-blocked: backend's verbatim body"
    );
    assert_header_value(&headers, "x-envoy-attempt-count", "1", "(i) /blocked");
    // It is a REAL upstream 503, not a synth-overflow: no x-envoy-overloaded.
    assert!(
        !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("x-envoy-overloaded")),
        "(i) budget-blocked 503 must NOT carry x-envoy-overloaded (real upstream 503)"
    );

    poll_stat_until(
        admin_addr,
        "cluster.budget_zero.upstream_rq_retry_overflow",
        1,
        Duration::from_secs(5),
    )
    .await;
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "cluster.budget_zero.upstream_rq_retry_overflow", 1);
    assert_stat(&s, "cluster.budget_zero.upstream_rq_retry", 0);
    assert_stat(&s, "cluster.budget_zero.upstream_rq_total", 1);

    // ─────────────────────────────────────────────────────────────────────────
    // (ii) budget-allowed retry: /allowed → budget_default. 503-then-200; the
    // retry is admitted by the default budget. Post-settle remaining_* gauges
    // read the L5 defaults (3 / 1024) back through the full stack at rest.
    // ─────────────────────────────────────────────────────────────────────────
    let (status, headers, body) = http1_oneshot(hcm_addr, "/allowed", "backend").await;
    assert_eq!(
        status, 200,
        "(ii) budget-allowed: expected final 200 on retry"
    );
    assert_eq!(body, b"ok\n", "(ii) budget-allowed: retried body `ok\\n`");
    assert_header_value(&headers, "x-envoy-attempt-count", "2", "(ii) /allowed");

    poll_stat_until(
        admin_addr,
        "cluster.budget_default.upstream_rq_retry_success",
        1,
        Duration::from_secs(5),
    )
    .await;
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "cluster.budget_default.upstream_rq_retry", 1);
    assert_stat(&s, "cluster.budget_default.upstream_rq_retry_success", 1);
    assert_stat(&s, "cluster.budget_default.upstream_rq_total", 2);
    // L5 defaults read back at rest (no in-flight request → cap - 0 = cap).
    assert_stat(
        &s,
        "cluster.budget_default.circuit_breakers.default.remaining_retries",
        3,
    );
    assert_stat(
        &s,
        "cluster.budget_default.circuit_breakers.default.remaining_rq",
        1024,
    );

    // ─────────────────────────────────────────────────────────────────────────
    // (iii) request-breaker overflow: /rq-blocked → rq_zero (max_requests:0).
    // Rejected before any upstream connect → 81-byte overflow synth-503 +
    // x-envoy-overloaded + x-envoy-attempt-count:1; backend never contacted.
    // ─────────────────────────────────────────────────────────────────────────
    let ctr_before = backend_ctr.load(Ordering::Relaxed);
    let (status, headers, body) = http1_oneshot(hcm_addr, "/rq-blocked", "backend").await;
    assert_eq!(status, 503, "(iii) rq-overflow: expected synth 503");
    assert_eq!(
        body.as_slice(),
        OVERFLOW_BODY,
        "(iii) rq-overflow: byte-exact overflow local-reply body"
    );
    assert_eq!(body.len(), 81, "(iii) rq-overflow: body must be 81 bytes");
    assert_header_present(&headers, "x-envoy-overloaded", "(iii) /rq-blocked");
    assert_header_value(&headers, "x-envoy-attempt-count", "1", "(iii) /rq-blocked");
    // Backend NEVER contacted: the request counter did not move.
    assert_eq!(
        backend_ctr.load(Ordering::Relaxed),
        ctr_before,
        "(iii) rq-overflow: backend must NOT be contacted (request counter unchanged)"
    );

    poll_stat_until(
        admin_addr,
        "cluster.rq_zero.upstream_rq_pending_overflow",
        1,
        Duration::from_secs(5),
    )
    .await;
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "cluster.rq_zero.upstream_rq_pending_overflow", 1);
    assert_stat(&s, "cluster.rq_zero.upstream_rq_5xx", 1);
    assert_stat(&s, "cluster.rq_zero.upstream_rq_total", 0);

    // ─────────────────────────────────────────────────────────────────────────
    // (iv-a) >0-cap request concurrency: /slow-200 → rq_one (max_requests:1).
    // TWO concurrent requests; the slow backend holds the winner's slot ~1.5s.
    // One 200 (the slot holder) + one 503-overflow (the loser). rq_open is the
    // L4 momentary gauge: observed at 1 DURING the hold, 0 after both drain.
    // ─────────────────────────────────────────────────────────────────────────
    let rq_open_mid = Arc::new(std::sync::Mutex::new(u64::MAX));
    let rq_open_mid_w = rq_open_mid.clone();
    let admin_for_probe = admin_addr;

    let r1 = http1_oneshot(hcm_addr, "/slow-200", "backend");
    let r2 = http1_oneshot(hcm_addr, "/slow-200", "backend");
    // While the winner holds the single request slot, poll rq_open and expect it
    // to converge to 1 (the at-cap inclusive edge). Poll budget 700ms ≪ 1500ms
    // hold, so the scrape reliably lands mid-flight.
    let probe = async move {
        let v = poll_stat_until(
            admin_for_probe,
            "cluster.rq_one.circuit_breakers.default.rq_open",
            1,
            Duration::from_millis(700),
        )
        .await;
        *rq_open_mid_w.lock().unwrap() = v;
    };
    let ((s1, _h1, _b1), (s2, h2, b2), ()) = tokio::join!(r1, r2, probe);

    let mut got = [s1, s2];
    got.sort_unstable();
    assert_eq!(
        got,
        [200, 503],
        "(iv-a) expected status multiset {{200,503}}, got {{{s1},{s2}}}"
    );
    // The 503 is the overflow synth (the loser of the single-slot race).
    let (of_headers, of_body) = if s1 == 503 { (&_h1, &_b1) } else { (&h2, &b2) };
    assert_eq!(
        of_body.as_slice(),
        OVERFLOW_BODY,
        "(iv-a) overflow 503 body must be the byte-exact local-reply"
    );
    assert_header_present(of_headers, "x-envoy-overloaded", "(iv-a) overflow 503");

    // rq_open NON-ZERO rising edge: observed at 1 while the slot was held.
    let mid = *rq_open_mid.lock().unwrap();
    assert_eq!(
        mid, 1,
        "(iv-a) rq_open must read 1 while the request slot is held mid-flight \
         (L4 momentary non-zero edge)"
    );

    // pending-overflow ticked exactly once (the loser).
    poll_stat_until(
        admin_addr,
        "cluster.rq_one.upstream_rq_pending_overflow",
        1,
        Duration::from_secs(5),
    )
    .await;
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "cluster.rq_one.upstream_rq_pending_overflow", 1);

    // Falling edge: after both requests drained, rq_open returns to 0 (re-synced
    // by the RequestBudgetGuard's Drop — single source of truth).
    let post = poll_stat_until(
        admin_addr,
        "cluster.rq_one.circuit_breakers.default.rq_open",
        0,
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(
        post, 0,
        "(iv-a) rq_open must return to 0 after both requests complete"
    );

    // ─────────────────────────────────────────────────────────────────────────
    // (iv-b) >0-cap retry concurrency: /slow-503 → retry_one (max_retries:1).
    // TWO concurrent requests against the slow always-503 backend. Both first
    // attempts overlap (forced by the hold) and complete ~together; both then
    // reach try_acquire_retry. The single retry slot admits exactly one; the
    // other overflows. Combined: upstream_rq_retry == 1, retry_overflow == 1.
    // ─────────────────────────────────────────────────────────────────────────
    let r1 = http1_oneshot(hcm_addr, "/slow-503", "backend");
    let r2 = http1_oneshot(hcm_addr, "/slow-503", "backend");
    let ((s1, h1, _b1), (s2, h2, _b2)) = tokio::join!(r1, r2);

    // Both downstream responses are 503 (one exhausted its admitted retry, the
    // other was retry-budget-blocked on its first attempt's 503).
    assert_eq!(s1, 503, "(iv-b) request 1 final status 503");
    assert_eq!(s2, 503, "(iv-b) request 2 final status 503");

    // Exactly one request retried (attempt-count 2), the other was budget-blocked
    // (attempt-count 1). The order is non-deterministic; assert the multiset.
    let ac = |h: &[(String, String)]| -> String {
        h.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-envoy-attempt-count"))
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default()
    };
    let mut counts = [ac(&h1), ac(&h2)];
    counts.sort();
    assert_eq!(
        counts,
        ["1".to_string(), "2".to_string()],
        "(iv-b) one request retried (attempt-count 2), one was budget-blocked \
         (attempt-count 1); got {counts:?}"
    );

    // Exactly one retry fired and exactly one retry overflowed.
    poll_stat_until(
        admin_addr,
        "cluster.retry_one.upstream_rq_retry_overflow",
        1,
        Duration::from_secs(5),
    )
    .await;
    let s = scrape_admin_stats(admin_addr).await;
    assert_stat(&s, "cluster.retry_one.upstream_rq_retry", 1);
    assert_stat(&s, "cluster.retry_one.upstream_rq_retry_overflow", 1);
}
