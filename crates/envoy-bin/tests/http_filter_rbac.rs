//! Phase-10 in-process backstop: end-to-end RBAC filter exercise against a real
//! envoy-bin subprocess (no Docker).
//!
//! Per phase-09 REVIEW M3 disposition + phase-10 SPEC §6.4: uses
//! `tokio::process::Command + .kill_on_drop(true) + Stdio::piped()` on stderr.
//! Discipline adopted directly from the 07.2 + 08.2 backstop precedents.
//! No regression to `std::process::Command`. Closes 09 REVIEW M3 here.
//!
//! Bootstrap shape: HCM + [envoy.filters.http.rbac, envoy.filters.http.router]
//! with action: ALLOW + one policy `pass_with_header` requiring x-rbac-pass: yes
//! on the request. 4 sequential GET probes alternate header presence:
//!   probe 1 (no header)         → 403, body "RBAC: access denied"
//!   probe 2 (x-rbac-pass: yes)  → 200, body "ok\n"
//!   probe 3 (x-rbac-pass: no)   → 403, body "RBAC: access denied"
//!   probe 4 (x-rbac-pass: yes)  → 200, body "ok\n"
//!
//! Direct code-spot-check evidence: both precedent backstops were read in full
//! via the `Read` tool before this file was authored (the PLAN's awareness-only
//! doctrine note in 09 REVIEW M3 disposition).

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Reserve a free TCP port on 127.0.0.1 by binding an ephemeral port and
/// immediately dropping the listener (matching the 07.2 + 08.2 precedent
/// `StdListener::bind(("127.0.0.1", 0))` style).
fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// Wait for a TCP listener at `addr` to accept a connection, with exponential
/// backoff up to `budget`. Returns `Ok(())` on success; `Err` on timeout.
/// Mirrors the `wait_ready_result` shape from `admin_drain_listeners.rs` (08.2).
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

/// Open a fresh TCP connection to `addr`, write an HTTP/1.1 GET with
/// `Connection: close` (and optionally `extra_header`), read-to-end, split
/// head/body at `\r\n\r\n`, parse the status code from the status line, and
/// return `(status, body)`. Panics on any I/O or parse failure.
async fn probe(addr: SocketAddr, extra_header: Option<(&str, &str)>) -> (u16, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("probe connect timeout")
        .expect("probe connect");
    let mut req = String::from("GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n");
    if let Some((name, value)) = extra_header {
        req.push_str(&format!("{name}: {value}\r\n"));
    }
    req.push_str("\r\n");
    stream
        .write_all(req.as_bytes())
        .await
        .expect("write request");
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("probe read timeout")
        .expect("probe read");
    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("\\r\\n\\r\\n header terminator not found");
    let head = &buf[..head_end];
    let body = buf[head_end + 4..].to_vec();
    let head_str = std::str::from_utf8(head).expect("ASCII response head");
    let status_line = head_str.lines().next().expect("status line");
    // e.g. "HTTP/1.1 403 Forbidden"
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .expect("status code token")
        .parse()
        .expect("parse status code");
    (status, body)
}

#[tokio::test]
async fn http_filter_rbac_in_process_backstop() {
    let admin_port = reserve_port();
    let listener_port = reserve_port();

    // Bootstrap YAML mirrors fixture 0017 (`tests/fixtures/0017-http-filter-rbac/
    // envoy-rust.yaml`) with concrete port values substituted in.
    // NOTE (PLAN deviation #1): `codec_type: HTTP1` is added immediately after
    // `stat_prefix: ingress_http` — the PLAN's verbatim skeleton omits this
    // field but the envoy-config schema marks it required (Task 6 hit
    // `missing field 'codec_type'` empirically; all 3 precedent backstops
    // include it; fixture 0017 includes it).
    let bootstrap_yaml = format!(
        r#"node:
  cluster: phase-10-rbac-backstop
  id: phase-10-rbac-backstop
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
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: default
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.rbac
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
                      rules:
                        action: ALLOW
                        policies:
                          "pass_with_header":
                            permissions:
                              - any: true
                            principals:
                              - header:
                                  name: x-rbac-pass
                                  string_match: {{ exact: "yes" }}
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

    // Per phase-09 REVIEW M3 + phase-10 SPEC §6.4 + 07.2/08.2 precedent:
    // tokio::process::Command + .kill_on_drop(true).
    // NOTE (PLAN deviation #2): stderr is Stdio::piped() (NOT Stdio::null() as
    // the PLAN skeleton specifies) so we can surface envoy-bin startup errors on
    // readiness failure — matching the 08.2 precedent (`admin_drain_listeners.rs`
    // lines 158-159) and the 07.2 precedent (`http_filter_header_mutation.rs`
    // line 187). This is load-bearing for diagnosing failures.
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();

    // Wait for the data-plane listener to bind. Dump stderr on failure so any
    // envoy-bin startup error surfaces in test output (08.2 precedent pattern).
    let ready = tokio::time::timeout(
        Duration::from_secs(10),
        wait_ready_result(listener_addr, Duration::from_secs(10)),
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
        panic!("envoy-bin listener never became ready at {listener_addr}");
    }

    // 4 sequential probes — ordering is load-bearing for [403, 200, 403, 200].
    let (s1, b1) = probe(listener_addr, None).await;
    let (s2, b2) = probe(listener_addr, Some(("x-rbac-pass", "yes"))).await;
    let (s3, b3) = probe(listener_addr, Some(("x-rbac-pass", "no"))).await;
    let (s4, b4) = probe(listener_addr, Some(("x-rbac-pass", "yes"))).await;

    // Dump stderr on assertion failure so envoy-bin runtime errors surface.
    let all_ok = s1 == 403
        && b1 == b"RBAC: access denied"
        && s2 == 200
        && b2 == b"ok\n"
        && s3 == 403
        && b3 == b"RBAC: access denied"
        && s4 == 200
        && b4 == b"ok\n";
    if !all_ok {
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
        assert_eq!(s1, 403, "probe-1 (no header) → 403");
        assert_eq!(b1.as_slice(), b"RBAC: access denied", "probe-1 body");
        assert_eq!(s2, 200, "probe-2 (x-rbac-pass: yes) → 200");
        assert_eq!(b2.as_slice(), b"ok\n", "probe-2 body");
        assert_eq!(s3, 403, "probe-3 (x-rbac-pass: no) → 403");
        assert_eq!(b3.as_slice(), b"RBAC: access denied", "probe-3 body");
        assert_eq!(s4, 200, "probe-4 (x-rbac-pass: yes) → 200");
        assert_eq!(b4.as_slice(), b"ok\n", "probe-4 body");
    }

    // Explicit kill + wait on success path (discipline: kill_on_drop is the
    // safety net; explicit kill+wait is the discipline per 08.2 precedent).
    child.kill().await.ok();
    let _ = child.wait().await;
}
