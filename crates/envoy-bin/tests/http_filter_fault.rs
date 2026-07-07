//! In-process backstop for the fault filter (abort path), exercised over an
//! HTTP/1.1 listener. Complements the H2 differential fixture 0018 — both
//! codecs covered across the two test tiers. Boots envoy-bin as a subprocess
//! with kill_on_drop discipline (09 REVIEW M3 pattern, standing since 10 Task 7).
//!
//! Per phase-09 REVIEW M3 disposition + SPEC §6.4: uses
//! `tokio::process::Command` with `.kill_on_drop(true)`, `stdout: Stdio::null()`,
//! and `stderr: Stdio::piped()` for diagnostics. Discipline copied verbatim from
//! the phase-10 `http_filter_rbac.rs` backstop precedent (read in full via the
//! `Read` tool before this file was authored, per lock-in #35).
//!
//! Bootstrap shape: HCM (codec_type HTTP1) + [envoy.filters.http.fault,
//! envoy.filters.http.router] with the fault filter configured to abort with
//! 503 @ 100%, gated on the request header `x-fault: abort`. 4 sequential GET
//! probes alternate header presence:
//!   probe 0 (x-fault: abort) → 503, body "fault filter abort"
//!   probe 1 (no header)      → 200, body "ok\n"
//!   probe 2 (x-fault: abort) → 503, body "fault filter abort"
//!   probe 3 (no header)      → 200, body "ok\n"
//!
//! On the 503 probes the backstop additionally asserts the per-probe standard
//! HTTP/1.1 header presence (10 REVIEW M1 lesson, SPEC §6.4 option (a)). The H1
//! abort response carries 5 standard headers including `connection` (the H1
//! `decorate_filter_synth_response` adds all 5). This is the H1 path: the H2
//! fixture 0018 asserts only 4 of these (without `connection`).

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;

use common::{reserve_port, wait_ready};

/// Open a fresh TCP connection to `addr`, write an HTTP/1.1 GET for `path` with
/// `Connection: close` (and optionally `x-fault: <fault_header>`), read-to-end,
/// split head/body at `\r\n\r\n`, parse the status code from the status line,
/// parse each response header line into `(name, value)` pairs, and return
/// `(status, headers, body)`. Panics on any I/O or parse failure.
///
/// Extends the RBAC precedent's `probe` helper (which returned only
/// `(status, body)`) to ALSO surface the parsed response headers — the 503-probe
/// header-presence assertion (SPEC §6.4 option (a)) needs them.
async fn http1_get(
    addr: SocketAddr,
    path: &str,
    fault_header: Option<&str>,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("probe connect timeout")
        .expect("probe connect");
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n");
    if let Some(value) = fault_header {
        req.push_str(&format!("x-fault: {value}\r\n"));
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
    // e.g. "HTTP/1.1 503 Service Unavailable"
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

#[tokio::test]
async fn http_filter_fault_in_process_backstop() {
    let admin_port = reserve_port();
    let listener_port = reserve_port();

    // Bootstrap YAML mirrors the fault fuzz seed `hcm_fault_filter.yaml` (Task 6)
    // with concrete port values substituted in, BUT with `codec_type: HTTP1`
    // (the seed uses HTTP2; this backstop is the H1 path, matching the RBAC
    // backstop precedent). `codec_type` is a required envoy-config schema field
    // (the RBAC backstop hit `missing field 'codec_type'` empirically), so it
    // is included.
    let bootstrap_yaml = format!(
        r#"node:
  cluster: phase-11-fault-backstop
  id: phase-11-fault-backstop
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
                  - name: envoy.filters.http.fault
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault
                      abort:
                        http_status: 503
                        percentage: {{ numerator: 100, denominator: HUNDRED }}
                      headers:
                        - name: x-fault
                          string_match: {{ exact: abort }}
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

    // Per phase-09 REVIEW M3 + SPEC §6.4 + the RBAC backstop precedent:
    // tokio::process::Command + .kill_on_drop(true). stderr is Stdio::piped()
    // (NOT Stdio::null()) so envoy-bin startup/runtime errors surface on failure
    // — load-bearing for diagnosis, matching the RBAC precedent.
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
    // envoy-bin startup error surfaces in test output.
    let ready = tokio::time::timeout(
        Duration::from_secs(10),
        wait_ready(listener_addr, Duration::from_secs(10)),
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

    // Probe sequence: [abort, pass, abort, pass]. Ordering is load-bearing for
    // the [503, 200, 503, 200] status sequence.
    let probes: [(Option<&str>, u16, &str); 4] = [
        (Some("abort"), 503, "fault filter abort"),
        (None, 200, "ok\n"),
        (Some("abort"), 503, "fault filter abort"),
        (None, 200, "ok\n"),
    ];
    for (i, (fault_header, expected_status, expected_body)) in probes.iter().enumerate() {
        let (status, headers, body) = http1_get(listener_addr, "/", *fault_header).await;

        // Dump stderr on assertion failure so envoy-bin runtime errors surface.
        let body_str = String::from_utf8_lossy(&body).to_string();
        let ok = status == *expected_status && body_str == *expected_body;
        if !ok {
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
        }
        assert_eq!(status, *expected_status, "probe {i}: status");
        assert_eq!(body_str, *expected_body, "probe {i}: body");

        if *expected_status == 503 {
            // 10 REVIEW M1 lesson (SPEC §6.4 option (a)): assert the standard
            // HTTP/1.1 headers are present on the abort response. The H1 path
            // carries 5 (incl. `connection`); the H2 fixture 0018 asserts 4
            // (without `connection`).
            for h in [
                "server",
                "date",
                "content-length",
                "content-type",
                "connection",
            ] {
                assert!(
                    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(h)),
                    "probe {i}: missing standard header {h:?} on 503; headers: {headers:?}"
                );
            }
        }
    }

    // Explicit kill + wait on the success path (kill_on_drop is the safety net;
    // explicit kill+wait is the discipline per the RBAC backstop precedent).
    child.kill().await.ok();
    let _ = child.wait().await;
}
