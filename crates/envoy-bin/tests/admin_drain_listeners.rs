//! Phase 08.2 D17.4b — in-process Docker-free backstop for the endpoint-
//! triggered drain flow that lands across Tasks 1-9. Spawns `envoy-bin`
//! against an in-memory bootstrap (admin + 1 HCM listener with one
//! `direct_response` route + `clusters: []` — the same shape fixture 0015
//! exercises against Docker), then asserts on the wire:
//!
//!   1. pre-drain `GET /ready` → 200 OK, body contains `LIVE\n`.
//!   2. `POST /drain_listeners` → 200 OK (D9 endpoint side-effect:
//!      `DrainState::drain()` flips `Live → Draining` and fires the
//!      `drain_signal()` Notify).
//!   3. post-drain `GET /ready` → 503, body contains `DRAINING\n` (Task 5
//!      D-ready three-arm `DrainStage` rebind).
//!   4. data-plane TCP connect to `hcm_drain_test`'s port within 5s →
//!      either Connection-Refused (the `Listener::serve` drain arm at
//!      `crates/envoy-listener/src/lib.rs:277` `drop`s the underlying
//!      `tokio::net::TcpListener`, closing the socket; subsequent
//!      `connect()` calls see RST/ECONNREFUSED) OR connect-succeeds-but-
//!      read-EOF (race window: the kernel listen-queue may still hold
//!      one accepted-but-not-yet-handled connection between
//!      `notify.notify_waiters()` waking and `drop(listener)` running, so
//!      a connect can still complete; the post-drop read will see EOF as
//!      the kernel rejects the half-handshake).
//!
//! No Docker — this is the in-process happy-path complement to Task 8's
//! Docker-gated `0015-admin-drain-listeners` differential wrapper. Mirrors
//! the shape of `admin_config_dump_server_info.rs` (08.1 D17.4a — single
//! `#[tokio::test]`, inline `reserve_port()` + `wait_ready_result()` +
//! one-shot TCP scrape with `Connection: close` + `shutdown(Write)`
//! against the admin handler's 5-second idle-read timeout).
//!
//! Architecture deviation #1 vs the 07.2 / 08.1 echo-trivial-filter
//! workaround: the data-plane listener uses an HCM filter with a
//! `direct_response` route (per the PLAN's Step-1 snippet and the
//! parent-08 SPEC §5.6 wire model) rather than the
//! `envoy.filters.network.echo` shortcut that sibling backstops
//! (`admin_config_dump_server_info.rs` lines 130-131) take. This shape
//! is necessary for the drain assertion: an `echo` filter binds via
//! `TcpListener::bind` directly in `envoy-bin/src/main.rs:182-189` and
//! is naturally excluded from `Listener::serve`'s drain observation per
//! the 08.2 PLAN architecture-decision lock-in #12; HCM goes through
//! the full `envoy_listener::Listener::serve` path and IS observed by
//! `drain_signal()`.

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready_result(addr: SocketAddr, budget: Duration) -> std::io::Result<()> {
    let deadline = Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => return Err(e),
        }
    }
}

/// One-shot HTTP/1.1 scrape against the admin port. Sends `Connection: close`
/// (and for POSTs, `Content-Length: 0`), half-closes the write side so the
/// handler's 5-second idle-read timeout (`IDLE_READ_TIMEOUT`) does not gate
/// EOF, reads to EOF. Returns the raw response bytes; callers parse the
/// status line via prefix-match (the existing wire-shape across all admin
/// endpoints is stable: `HTTP/1.1 <status> <reason>\r\n…`).
async fn scrape(admin_port: u16, req: &[u8]) -> std::io::Result<Vec<u8>> {
    let addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let mut s = TcpStream::connect(addr).await?;
    s.write_all(req).await?;
    s.shutdown().await.ok();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await?;
    Ok(buf)
}

#[tokio::test]
async fn admin_drain_listeners_in_process() {
    let admin_port = reserve_port();
    let data_plane_port = reserve_port();

    // In-memory bootstrap mirrors fixture 0015's `envoy-rust.yaml` (minus
    // the harness substitution markers) — admin + 1 HCM listener with a
    // single `direct_response` route, no clusters. The validator accepts
    // `clusters: []` for admin+HCM-with-direct_response configurations
    // (HCM with `direct_response` does not reference any cluster name,
    // so the `UnknownCluster` validator at envoy-config/src/bootstrap.rs
    // does not fire). The router `http_filter` is mandatory per the HCM
    // schema (envoy-config Task 5 / 04.x).
    let bootstrap_yaml = format!(
        r#"node:
  id: backstop-drain-test
  cluster: backstop-drain-test
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners:
    - name: hcm_drain_test
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {data_plane_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: drain_test
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = dir.path().join("bootstrap.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap_yaml.as_bytes())
        .unwrap();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    // Dump stderr on a wait-ready failure so any envoy-bin startup error
    // surfaces in the test output. Without this the panic would only show
    // "Connection refused".
    let ready = tokio::time::timeout(
        Duration::from_secs(10),
        wait_ready_result(admin_addr, Duration::from_secs(10)),
    )
    .await;
    if ready.is_err() || matches!(&ready, Ok(Err(_))) {
        if let Some(mut err_pipe) = child.stderr.take() {
            let mut stderr_buf = Vec::new();
            let _ = err_pipe.read_to_end(&mut stderr_buf).await;
            eprintln!(
                "envoy-bin stderr:\n{}",
                String::from_utf8_lossy(&stderr_buf)
            );
        }
        child.kill().await.ok();
        let _ = child.wait().await;
        panic!("admin never became ready at {admin_addr}");
    }

    // 1. pre-drain /ready → 200 OK + body contains `LIVE\n`.
    let resp = scrape(
        admin_port,
        b"GET /ready HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await
    .expect("scrape /ready pre-drain");
    assert!(
        resp.starts_with(b"HTTP/1.1 200 OK\r\n"),
        "pre-drain /ready must be 200 OK; got: {}",
        String::from_utf8_lossy(&resp),
    );
    assert!(
        resp.windows(5).any(|w| w == b"LIVE\n"),
        "pre-drain /ready body must contain `LIVE\\n`; got: {}",
        String::from_utf8_lossy(&resp),
    );

    // 2. POST /drain_listeners → 200 OK (side effect: DrainState::drain()
    //    flips Live → Draining and fires drain_signal()).
    let resp = scrape(
        admin_port,
        b"POST /drain_listeners HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
    )
    .await
    .expect("scrape /drain_listeners");
    assert!(
        resp.starts_with(b"HTTP/1.1 200 OK\r\n"),
        "/drain_listeners POST must be 200 OK; got: {}",
        String::from_utf8_lossy(&resp),
    );

    // 3. post-drain /ready → 503 + body contains `DRAINING\n`.
    let resp = scrape(
        admin_port,
        b"GET /ready HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    )
    .await
    .expect("scrape /ready post-drain");
    assert!(
        resp.starts_with(b"HTTP/1.1 503 "),
        "post-drain /ready must be 503; got: {}",
        String::from_utf8_lossy(&resp),
    );
    assert!(
        resp.windows(9).any(|w| w == b"DRAINING\n"),
        "post-drain /ready body must contain `DRAINING\\n`; got: {}",
        String::from_utf8_lossy(&resp),
    );

    // 4. Data-plane refuse-or-EOF within 5s. Per the Listener::serve drain
    //    arm at crates/envoy-listener/src/lib.rs:277, the underlying
    //    TcpListener is `drop`ped on drain_signal(); subsequent connects
    //    see RST/ECONNREFUSED OR (race window: kernel listen-queue holds
    //    a half-handshake) connect-succeeds-but-read-EOF. Either is the
    //    drain success signal.
    let addr: SocketAddr = format!("127.0.0.1:{data_plane_port}").parse().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_failure: Option<String> = None;
    loop {
        if Instant::now() >= deadline {
            child.kill().await.ok();
            let _ = child.wait().await;
            panic!("data-plane listener did not drain within 5s; last_failure={last_failure:?}",);
        }
        match tokio::time::timeout(Duration::from_millis(200), TcpStream::connect(addr)).await {
            // connect refused — drain success (the common path).
            Ok(Err(_)) => {
                child.kill().await.ok();
                let _ = child.wait().await;
                return;
            }
            // connect succeeded — check for immediate EOF (drop-after-accept race).
            Ok(Ok(mut stream)) => {
                let mut buf = [0u8; 1];
                match tokio::time::timeout(Duration::from_millis(50), stream.read(&mut buf)).await {
                    Ok(Ok(0)) => {
                        child.kill().await.ok();
                        let _ = child.wait().await;
                        return;
                    }
                    Ok(Ok(_n)) => {
                        last_failure = Some("connect succeeded + read returned data".into());
                    }
                    Ok(Err(_)) | Err(_) => {
                        child.kill().await.ok();
                        let _ = child.wait().await;
                        return;
                    }
                }
            }
            // connect timed out — keep polling.
            Err(_) => {
                last_failure = Some("connect timed out".into());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
