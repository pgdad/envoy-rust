//! Phase-24 in-process backstop: end-to-end CSRF filter exercise against a
//! real envoy-bin subprocess (no Docker).
//!
//! Complements Docker fixture 0032 — the fixture does the bilateral
//! envoy-upstream/envoy-rust comparison; this backstop is the fast in-process
//! guard that directly asserts the §6.2-verified CSRF behavior (the backstop
//! cannot rely on the differential harness allow-list).
//!
//! NOTE: extract-a-shared-test-support-crate stays deferred per the standing
//! risk-managed decision (this file is the Nth in-process backstop). The
//! duplication is mechanical and the refactor carries non-trivial risk relative
//! to the value at this stage — so this file mirrors the phase-23 cors backstop
//! `crates/envoy-bin/tests/http_filter_cors.rs` verbatim in structure, swapping
//! only the csrf-specific config + probes.
//!
//! Bootstrap shape: HCM (codec_type HTTP1) + [envoy.filters.http.csrf,
//! envoy.filters.http.router] → cluster `backend` → in-process tokio HTTP/1.1
//! upstream (returns "ok\n"). One route with `typed_per_filter_config`
//! CsrfPolicy: filter_enabled 100%, additional_origins exact
//! "additional.csrf.test".
//!
//! 5 sequential probes:
//!   probe 1 (POST same-origin):
//!       POST / + Host: csrf.test + Origin: http://csrf.test
//!       → 200, body "ok\n" (origin authority == target authority)
//!   probe 2 (POST evil-origin):
//!       POST / + Host: csrf.test + Origin: http://evil.example.com
//!       → 403, body "Invalid origin" (byte-exact)
//!   probe 3 (POST additional-origin):
//!       POST / + Host: csrf.test + Origin: http://additional.csrf.test
//!       → 200, body "ok\n" (matches additional_origins)
//!   probe 4 (GET evil-origin, safe method bypass):
//!       GET / + Host: csrf.test + Origin: http://evil.example.com
//!       → 200, body "ok\n" (GET is a safe method — CSRF check bypassed)
//!   probe 5 (POST no-source):
//!       POST / + Host: csrf.test (no Origin/Referer)
//!       → 403, body "Invalid origin" (byte-exact)

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Reserve a free TCP port on 127.0.0.1 by binding an ephemeral port and
/// immediately dropping the listener (matching the cors/jwt_authn/rbac/fault
/// backstop precedent).
fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// Wait for a TCP listener at `addr` to accept a connection, with exponential
/// backoff up to `budget`. Returns `Ok(())` on success; `Err` on timeout.
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

/// Spawn a minimal HTTP/1.1 backend that accepts multiple connections in a
/// loop and responds with `HTTP/1.1 200 OK\r\ncontent-length: 3\r\n\r\nok\n`
/// to each request. The task runs until the listener is garbage-collected
/// (it never panics — failures are silently ignored so the probe-side
/// assertions surface the real error instead).
async fn spawn_http1_backend() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                // Drain the request (wait for the blank-line terminator).
                let mut buf = vec![0u8; 8192];
                let mut total = 0usize;
                loop {
                    let Ok(n) = stream.read(&mut buf[total..]).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    total += n;
                    if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    if total >= buf.len() {
                        return;
                    }
                }
                let response = b"HTTP/1.1 200 OK\r\n\
                    content-type: text/plain\r\n\
                    content-length: 3\r\n\
                    connection: close\r\n\
                    \r\n\
                    ok\n";
                let _ = stream.write_all(response).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    addr
}

/// Open a fresh TCP connection to `addr`, write an HTTP/1.1 request with
/// `Connection: close` and the given `host`, read-to-end, split head/body at
/// `\r\n\r\n`, parse the status code from the status line, parse response
/// header name/value pairs, and return `(status, headers, body)`. Panics on
/// any I/O or parse failure.
///
/// `method`: HTTP method string (e.g. "GET" or "POST")
/// `host`: the value for the request `Host` header
/// `extra_headers`: additional request headers to append (each already
///   formatted as "Name: value\r\n")
async fn http_probe(
    addr: SocketAddr,
    method: &str,
    host: &str,
    extra_headers: &[&str],
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("probe connect timeout")
        .expect("probe connect");
    let mut req = format!("{method} / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for h in extra_headers {
        req.push_str(h);
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
    let mut lines = head_str.lines();
    let status_line = lines.next().expect("status line");
    // e.g. "HTTP/1.1 200 OK"
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .expect("status code token")
        .parse()
        .expect("parse status code");
    // Remaining lines are `Name: value` header fields.
    let headers: Vec<(String, String)> = lines
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (name, value) = l.split_once(':').expect("header field colon");
            (name.trim().to_string(), value.trim().to_string())
        })
        .collect();
    (status, headers, body)
}

/// Dump stderr from the child process (if available) and kill + wait on it.
/// Used on the failure path so envoy-bin runtime errors surface in test output.
/// Mirrors the cors/jwt_authn backstop pattern exactly.
async fn dump_stderr_and_kill(child: &mut tokio::process::Child) {
    // KILL FIRST: while the child is alive it holds the write end of the stderr
    // pipe open, so `read_to_end` on a live child blocks forever. Killing first
    // closes the write end so the buffered stderr drains and EOFs.
    child.kill().await.ok();
    if let Some(mut err_pipe) = child.stderr.take() {
        let mut stderr_buf = Vec::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            err_pipe.read_to_end(&mut stderr_buf),
        )
        .await;
        eprintln!(
            "envoy-bin stderr:\n{}",
            String::from_utf8_lossy(&stderr_buf)
        );
    }
    let _ = child.wait().await;
}

#[tokio::test]
async fn http_filter_csrf_in_process_backstop() {
    let backend_addr = spawn_http1_backend().await;
    let backend_port = backend_addr.port();
    let listener_port = reserve_port();

    // Bootstrap YAML mirrors fixture 0032 (`tests/fixtures/0032-http-filter-csrf/
    // envoy-rust.yaml`) with concrete port values substituted in.
    // Uses STATIC cluster (in-process backend; no DNS needed — mirrors the cors
    // backstop / http1_router_upstream.rs precedent) and no admin block (not
    // needed for the 5 behavioral probes; mirrors the fixture envoy-rust.yaml).
    let bootstrap_yaml = format!(
        r#"node:
  cluster: phase-24-csrf-backstop
  id: phase-24-csrf-backstop
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
                          route: {{ cluster: backend }}
                          typed_per_filter_config:
                            envoy.filters.http.csrf:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
                              filter_enabled:
                                default_value:
                                  numerator: 100
                                  denominator: HUNDRED
                              additional_origins:
                                - exact: "additional.csrf.test"
                http_filters:
                  - name: envoy.filters.http.csrf
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
                      filter_enabled:
                        default_value:
                          numerator: 100
                          denominator: HUNDRED
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: {backend_port}
"#
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = dir.path().join("bootstrap.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(bootstrap_yaml.as_bytes())
        .unwrap();

    // Per phase-09 REVIEW M3 + phase-10 SPEC §6.4 + cors/jwt_authn/rbac/fault
    // precedent: tokio::process::Command + .kill_on_drop(true). stderr is
    // Stdio::piped() (NOT Stdio::null()) so envoy-bin startup/runtime errors
    // surface on failure.
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
    // envoy-bin startup error surfaces in test output (cors + jwt_authn precedent).
    let ready = tokio::time::timeout(
        Duration::from_secs(10),
        wait_ready_result(listener_addr, Duration::from_secs(10)),
    )
    .await;
    if ready.is_err() || matches!(&ready, Ok(Err(_))) {
        dump_stderr_and_kill(&mut child).await;
        panic!("envoy-bin listener never became ready at {listener_addr}");
    }

    // ---- 5 sequential probes — §6.2 CSRF semantics verification -----------------
    // Host: csrf.test for all probes (the target authority). The CSRF check
    // compares the source origin authority against the target authority and the
    // configured additional_origins; mismatches on unsafe (POST) methods → 403.

    // probe 1: POST same-origin (Origin authority == Host authority)
    //   → 200, body "ok\n"
    let (s1, _h1, b1) = http_probe(
        listener_addr,
        "POST",
        "csrf.test",
        &["Origin: http://csrf.test\r\n"],
    )
    .await;

    // probe 2: POST evil-origin (mismatch, unsafe method)
    //   → 403, body "Invalid origin"
    let (s2, _h2, b2) = http_probe(
        listener_addr,
        "POST",
        "csrf.test",
        &["Origin: http://evil.example.com\r\n"],
    )
    .await;

    // probe 3: POST additional-origin (matches additional_origins exact matcher)
    //   → 200, body "ok\n"
    let (s3, _h3, b3) = http_probe(
        listener_addr,
        "POST",
        "csrf.test",
        &["Origin: http://additional.csrf.test\r\n"],
    )
    .await;

    // probe 4: GET evil-origin (safe method — CSRF check bypassed)
    //   → 200, body "ok\n"
    let (s4, _h4, b4) = http_probe(
        listener_addr,
        "GET",
        "csrf.test",
        &["Origin: http://evil.example.com\r\n"],
    )
    .await;

    // probe 5: POST no-source (no Origin/Referer header, unsafe method)
    //   → 403, body "Invalid origin"
    let (s5, _h5, b5) = http_probe(listener_addr, "POST", "csrf.test", &[]).await;

    // ---- Assertions — dump stderr on any failure --------------------------------

    let all_ok = s1 == 200
        && b1 == b"ok\n"
        && s2 == 403
        && b2 == b"Invalid origin"
        && s3 == 200
        && b3 == b"ok\n"
        && s4 == 200
        && b4 == b"ok\n"
        && s5 == 403
        && b5 == b"Invalid origin";
    if !all_ok {
        dump_stderr_and_kill(&mut child).await;
    }

    // probe 1: POST same-origin → 200
    assert_eq!(s1, 200, "probe-1 (POST, same-origin) → 200");
    assert_eq!(b1.as_slice(), b"ok\n", "probe-1 body");

    // probe 2: POST evil-origin → 403 "Invalid origin"
    assert_eq!(s2, 403, "probe-2 (POST, evil-origin) → 403");
    assert_eq!(
        b2.as_slice(),
        b"Invalid origin",
        "probe-2: 403 body must be byte-exact \"Invalid origin\"; got: {b2:?}"
    );

    // probe 3: POST additional-origin → 200
    assert_eq!(s3, 200, "probe-3 (POST, additional-origin) → 200");
    assert_eq!(b3.as_slice(), b"ok\n", "probe-3 body");

    // probe 4: GET evil-origin (safe method bypass) → 200
    assert_eq!(s4, 200, "probe-4 (GET, evil-origin, safe method) → 200");
    assert_eq!(b4.as_slice(), b"ok\n", "probe-4 body");

    // probe 5: POST no-source → 403 "Invalid origin"
    assert_eq!(s5, 403, "probe-5 (POST, no source) → 403");
    assert_eq!(
        b5.as_slice(),
        b"Invalid origin",
        "probe-5: 403 body must be byte-exact \"Invalid origin\"; got: {b5:?}"
    );

    // Explicit kill + wait on the success path (kill_on_drop is the safety net;
    // explicit kill+wait is the discipline per the cors + jwt_authn precedent).
    child.kill().await.ok();
    let _ = child.wait().await;
}
