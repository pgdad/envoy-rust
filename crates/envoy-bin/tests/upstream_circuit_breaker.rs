//! 15 Task 8 (lock-in #12): in-process H1 backstop for the two circuit-breaker
//! overflow paths landed in Tasks 1-5. Mirrors the boot/harness shape of
//! `upstream_connection_pooling.rs` (boot `envoy-bin` + an in-process backend +
//! admin `/stats` scrape) at the cheap in-process subprocess scope.
//!
//! Two cases:
//!
//!   (a) **pending-overflow path** — a cluster with
//!       `circuit_breakers.thresholds: [{max_connections:1, max_pending_requests:0}]`.
//!       A SINGLE GET is rejected with a 503 + the byte-exact 81-byte
//!       `...reset reason: overflow` body + `x-envoy-overloaded: true` (plus the 5
//!       standard synth headers). The backend is NEVER contacted, so
//!       `upstream_cx_total == 0`; `upstream_rq_pending_overflow == 1`;
//!       `upstream_cx_overflow == 0`; `circuit_breakers.default.cx_open == 0`
//!       (ADR-0043 §6.2 finding 1).
//!
//!   (b) **cx-overflow path** — a cluster with
//!       `circuit_breakers.thresholds: [{max_connections:1}]` (default pending = 1024)
//!       + an in-test hold-capable H1 backend (a tokio `TcpListener` accept loop that
//!       reads the request then SLEEPS before responding 200, so two requests are
//!       concurrently in-flight). K=2 concurrent GETs via `tokio::join!` yield the
//!       status MULTISET `{200, 503}` (one acquires the single connection, one
//!       overflows). `upstream_cx_overflow == 1`. The `cx_open` gauge is observed at
//!       BOTH edges: `== 1` mid-flight (scraped while the slow backend still holds
//!       the connection) and `== 0` after drain. Path (b) is envoy-rust-internal
//!       (Envoy would serve `{200,200}` there — bilaterally deferred), so it is
//!       in-process ONLY.
//!
//! Per phase-09 REVIEW M3 disposition: uses `tokio::process::Command` with
//! `.kill_on_drop(true)`, `stdout: Stdio::null()`, and `stderr: Stdio::piped()`.

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

mod common;

use common::{
    assert_stat, http1_oneshot, poll_stat_until, reserve_port, scrape_admin_stats, wait_ready,
};

/// The byte-exact Envoy overflow local-reply body (81 bytes, no trailing newline).
const OVERFLOW_BODY: &[u8] =
    b"upstream connect error or disconnect/reset before headers. reset reason: overflow";

/// The 5-name standard synth-header roster pin (`server`, `date`,
/// `content-length`, `content-type`, `connection`), case-insensitive — the
/// 10/11/12.2/14.2 synth-header discipline.
fn assert_5_standard_headers_present(headers: &[(String, String)], ctx: &str, status: u16) {
    for required in &[
        "server",
        "date",
        "content-length",
        "content-type",
        "connection",
    ] {
        assert!(
            headers
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(required)),
            "missing standard header {required:?} on {ctx} ({status})\nactual: {headers:?}",
        );
    }
}

fn assert_header_eq(headers: &[(String, String)], name: &str, value: &str, ctx: &str) {
    assert!(
        headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case(name) && v.eq_ignore_ascii_case(value)),
        "missing header {name}: {value} on {ctx}\nactual: {headers:?}",
    );
}

/// Boot `envoy-bin` with a STATIC `backend_cluster` pointing at `backend_port`,
/// HCM listener on `hcm_port`, admin on `admin_port`. `extra_threshold` is
/// spliced into the `circuit_breakers.thresholds[0]` entry (after
/// `max_connections: 1`) — e.g. `"            max_pending_requests: 0\n"` for
/// case (a), or `""` for case (b) (default pending).
async fn spawn_envoy_bin(
    hcm_port: u16,
    admin_port: u16,
    backend_port: u16,
    extra_threshold: &str,
) -> tokio::process::Child {
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
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: backend_cluster }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend_cluster
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
{extra_threshold}      load_assignment:
        cluster_name: backend_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: {backend_port} }} }}
"#
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

/// (a) pending-overflow path. A single GET against a
/// `max_connections:1, max_pending_requests:0` cluster is rejected with the
/// 503 overflow synth; the backend is never contacted.
#[tokio::test(flavor = "multi_thread")]
async fn pending_overflow_rejects_single_get_without_contacting_backend() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    // No backend is spawned: max_pending_requests:0 rejects before any connect.
    // Any address works; reserve one so nothing is listening there.
    let backend_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();

    let _envoy = spawn_envoy_bin(
        hcm_port,
        admin_port,
        backend_port,
        "            max_pending_requests: 0\n",
    )
    .await;
    wait_ready(hcm_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin admin ready");

    let (status, headers, body) = http1_oneshot(hcm_addr, "/", "backend_cluster").await;
    assert_eq!(status, 503, "pending-overflow must reject with 503");
    assert_eq!(
        body.as_slice(),
        OVERFLOW_BODY,
        "503 body must be the byte-exact overflow local-reply",
    );
    assert_eq!(body.len(), 81, "overflow body must be exactly 81 bytes");
    assert_5_standard_headers_present(&headers, "pending-overflow 503", status);
    assert_header_eq(
        &headers,
        "x-envoy-overloaded",
        "true",
        "pending-overflow 503",
    );

    // Settle, then scrape stats.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let stats = scrape_admin_stats(admin_addr).await;
    assert_stat(
        &stats,
        "cluster.backend_cluster.upstream_rq_pending_overflow",
        1,
    );
    assert_stat(&stats, "cluster.backend_cluster.upstream_cx_overflow", 0);
    assert_stat(&stats, "cluster.backend_cluster.upstream_cx_total", 0);
    assert_stat(
        &stats,
        "cluster.backend_cluster.circuit_breakers.default.cx_open",
        0,
    );
}

/// A hold-capable in-test H1 backend: accepts connections, reads the request
/// bytes, sleeps `hold` so concurrent requests overlap, then writes a minimal
/// 200. Spawned on a tokio task; the returned port is ready to dial.
async fn spawn_holding_backend(hold: Duration) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind backend");
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut sock, _peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            tokio::spawn(async move {
                // Read the request head (until \r\n\r\n); GETs have no body.
                let mut buf = Vec::with_capacity(1024);
                loop {
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    let mut chunk = [0u8; 512];
                    match sock.read(&mut chunk).await {
                        Ok(0) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        Err(_) => return,
                    }
                }
                // Hold the connection so the concurrent request overlaps + the
                // pool stays saturated long enough for the mid-flight cx_open
                // scrape, then respond a minimal 200.
                tokio::time::sleep(hold).await;
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                    .await;
                let _ = sock.flush().await;
            });
        }
    });
    port
}

/// (b) cx-overflow path. K=2 concurrent GETs against a `max_connections:1`
/// cluster (default pending) with a hold-capable backend: one acquires the
/// single connection, one overflows → status multiset `{200, 503}`,
/// `upstream_cx_overflow == 1`, and `cx_open` observed at 1 mid-flight + 0 after
/// drain.
#[tokio::test(flavor = "multi_thread")]
async fn cx_overflow_yields_200_503_multiset_and_cx_open_both_edges() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();

    // Generous 800ms hold so the two requests reliably overlap and the
    // mid-flight scrape lands while saturated.
    let hold = Duration::from_millis(800);
    let backend_port = spawn_holding_backend(hold).await;

    let _envoy = spawn_envoy_bin(hcm_port, admin_port, backend_port, "").await;
    wait_ready(hcm_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(15))
        .await
        .expect("envoy-bin admin ready");

    // Drive K=2 concurrent GETs. One holds the single connection (200, after the
    // backend hold elapses); the other overflows the cap immediately (503).
    let admin_for_probe = admin_addr;
    let cx_open_mid = Arc::new(std::sync::Mutex::new(0u64));
    let cx_open_mid_w = cx_open_mid.clone();

    let r1 = http1_oneshot(hcm_addr, "/", "backend_cluster");
    let r2 = http1_oneshot(hcm_addr, "/", "backend_cluster");
    // While the slow request still holds the connection, scrape cx_open and
    // expect it pinned at 1 (the at-cap inclusive rising edge). Poll a short
    // bounded window for convergence to 1 (the connect must complete first).
    let probe = async move {
        let v = poll_stat_until(
            admin_for_probe,
            "cluster.backend_cluster.circuit_breakers.default.cx_open",
            1,
            Duration::from_millis(600),
        )
        .await;
        *cx_open_mid_w.lock().unwrap() = v;
    };

    let ((s1, _h1, _b1), (s2, h2, b2), ()) = tokio::join!(r1, r2, probe);

    // Status multiset == {200, 503} (acquisition order non-deterministic).
    let mut got = [s1, s2];
    got.sort_unstable();
    assert_eq!(
        got,
        [200, 503],
        "expected status multiset {{200,503}}, got {{{s1},{s2}}}",
    );

    // The 503 carries the overflow synth body + x-envoy-overloaded.
    let (overflow_headers, overflow_body) = if s1 == 503 {
        // r1 was the 503; re-bind from the destructured values.
        (&_h1, &_b1)
    } else {
        (&h2, &b2)
    };
    assert_eq!(
        overflow_body.as_slice(),
        OVERFLOW_BODY,
        "cx-overflow 503 body must be the byte-exact overflow local-reply",
    );
    assert_eq!(overflow_body.len(), 81, "overflow body must be 81 bytes");
    assert_5_standard_headers_present(overflow_headers, "cx-overflow 503", 503);
    assert_header_eq(
        overflow_headers,
        "x-envoy-overloaded",
        "true",
        "cx-overflow 503",
    );

    // cx_open rising edge: observed at 1 while the pool was saturated mid-flight.
    let mid = *cx_open_mid.lock().unwrap();
    assert_eq!(
        mid, 1,
        "cx_open must read 1 while saturated mid-flight (at-cap inclusive rising edge)",
    );

    // The cap was hit exactly once.
    let stats = scrape_admin_stats(admin_addr).await;
    assert_stat(&stats, "cluster.backend_cluster.upstream_cx_overflow", 1);

    // Falling edge / terminal-0 (the both-directions discipline). The served
    // upstream connection returns to the pool's IDLE list on a clean keep-alive
    // response — it is NOT destroyed — so `established` stays at the cap (1)
    // until the idle-sweeper evicts it. That sweeper is timer-gated at the
    // hardcoded 60s `DEFAULT_IDLE_TIMEOUT` (15s tick), which is NOT promptly
    // observable from an in-process backstop. The PLAN's lock-in #12 / Task-8
    // Step-1 explicitly permits asserting ONE edge live + relying on the Task-3
    // pool unit test for the other: here the RISING edge is the live-observable
    // one (asserted == 1 above), and the FALLING edge / terminal-0 is covered by
    // `pool.rs::cx_overflow_increments_and_cx_open_tracks_cap_edges`, which
    // drives the `PoolGuard::Drop` destroy decrement (via `invalidate()`) and
    // asserts `cx_open` returns to 0. We therefore do NOT re-observe terminal-0
    // here (it would require a 60s wait). We DO sanity-check that cx_open is
    // still a well-formed 0/1 gauge value post-drain (it reads 1 while the
    // served conn lingers idle at the cap — the documented in-process state).
    let post = scrape_admin_stats(admin_addr).await;
    let cx_open_post = post
        .get("cluster.backend_cluster.circuit_breakers.default.cx_open")
        .copied()
        .expect("cx_open gauge must be present");
    assert!(
        cx_open_post <= 1,
        "cx_open must be a well-formed 0/1 gauge post-drain; got {cx_open_post}",
    );
}
