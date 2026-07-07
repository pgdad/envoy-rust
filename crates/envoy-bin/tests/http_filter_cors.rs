//! Phase-23 in-process backstop: end-to-end CORS filter exercise against a
//! real envoy-bin subprocess (no Docker).
//!
//! Complements Docker fixture 0031 — the fixture does the bilateral
//! envoy-upstream/envoy-rust comparison; this backstop is the fast in-process
//! guard that directly asserts the §6.2-verified CORS header values (phase-10
//! M1 lesson: the backstop cannot rely on the differential harness allow-list).
//!
//! NOTE (M18-9/M21-3/M22): extract-a-shared-test-support-crate is now at N≥8
//! in-process backstops (this file is the 8th). Consolidation stays deferred
//! per the standing risk-managed decision — the duplication is mechanical and
//! the refactor carries non-trivial risk relative to the value at this stage.
//!
//! NOTE (ADR-0058 L7 absent-filter negative path): the envoy-rust-only fatal
//! reject for a route `typed_per_filter_config` referencing a filter that is
//! NOT in the HCM http_filters chain (`ConfigError::PerRouteConfigForAbsentFilter`)
//! is covered at unit-test granularity by
//! `envoy_config::tests::cors_per_route_config_without_cors_filter_is_fatal`
//! in `crates/envoy-config/src/bootstrap.rs`. Wiring a second envoy-bin boot
//! expecting a startup failure would require polling for process exit with a
//! timeout and is heavy for the net coverage gain — the unit test exercises the
//! exact code path (parse_bootstrap returns Err before any runtime starts).
//!
//! NOTE (admin stats): this backstop does NOT scrape `/stats` for
//! `http.<prefix>.cors.origin_valid` / `origin_invalid` because the jwt_authn
//! backstop template has no admin-scrape helper; adding one here would be
//! out-of-scope for a single backstop. Stats are validated by the CorsFilter
//! unit tests in `crates/envoy-http1/src/` (Task 3).
//!
//! Bootstrap shape: HCM (codec_type HTTP1) + [envoy.filters.http.cors,
//! envoy.filters.http.router] → cluster `backend` → in-process tokio HTTP/1.1
//! upstream (returns "ok\n"). One route with `typed_per_filter_config`
//! CorsPolicy: allow_origin exact "http://allowed.example.com",
//! allow_methods "GET, POST, OPTIONS", allow_headers "x-custom-header, content-type",
//! max_age "3600".
//!
//! 4 sequential probes (Host: envoy.test):
//!   probe 1 (OPTIONS preflight, allowed origin):
//!       OPTIONS / + Origin: http://allowed.example.com
//!                 + Access-Control-Request-Method: GET
//!       → 200, empty body, ACAO=http://allowed.example.com, ACAM present,
//!         ACAH present, ACMA present, NO content-type (ADR-0059 empty-body rule)
//!   probe 2 (GET decoration, allowed origin):
//!       GET / + Origin: http://allowed.example.com
//!       → 200, body "ok\n", ACAO=http://allowed.example.com
//!   probe 3 (GET decoration, disallowed origin):
//!       GET / + Origin: http://evil.example.com
//!       → 200, body "ok\n", NO access-control-allow-origin
//!   probe 4 (GET, no origin):
//!       GET /
//!       → 200, body "ok\n", NO access-control-* headers

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

mod common;

use common::{dump_stderr_and_kill, reserve_port, spawn_http1_backend, wait_ready};

/// Open a fresh TCP connection to `addr`, write an HTTP/1.1 request with
/// `Connection: close` and `Host: envoy.test`, read-to-end, split head/body
/// at `\r\n\r\n`, parse the status code from the status line, parse response
/// header name/value pairs, and return `(status, headers, body)`. Panics on
/// any I/O or parse failure.
///
/// `method`: HTTP method string (e.g. "GET" or "OPTIONS")
/// `extra_headers`: additional request headers to append (each already
///   formatted as "Name: value\r\n")
async fn http_probe(
    addr: SocketAddr,
    method: &str,
    extra_headers: &[&str],
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("probe connect timeout")
        .expect("probe connect");
    let mut req = format!("{method} / HTTP/1.1\r\nHost: envoy.test\r\nConnection: close\r\n");
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

/// Find a header value by case-insensitive name from the parsed header list.
fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

#[tokio::test]
async fn http_filter_cors_in_process_backstop() {
    let backend_addr = spawn_http1_backend().await;
    let backend_port = backend_addr.port();
    let listener_port = reserve_port();

    // Bootstrap YAML mirrors fixture 0031 (`tests/fixtures/0031-http-filter-cors/
    // envoy-rust.yaml`) with concrete port values substituted in.
    // Uses STATIC cluster (in-process backend; no DNS needed — mirrors
    // http1_router_upstream.rs precedent) and no admin block (not needed for
    // the 4 behavioral probes; mirrors the fixture envoy-rust.yaml).
    let bootstrap_yaml = format!(
        r#"node:
  cluster: phase-23-cors-backstop
  id: phase-23-cors-backstop
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
                            envoy.filters.http.cors:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
                              allow_origin_string_match:
                                - exact: "http://allowed.example.com"
                              allow_methods: "GET, POST, OPTIONS"
                              allow_headers: "x-custom-header, content-type"
                              max_age: "3600"
                http_filters:
                  - name: envoy.filters.http.cors
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors
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

    // Per phase-09 REVIEW M3 + phase-10 SPEC §6.4 + jwt_authn/rbac/fault precedent:
    // tokio::process::Command + .kill_on_drop(true). stderr is Stdio::piped()
    // (NOT Stdio::null()) so envoy-bin startup/runtime errors surface on failure.
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
    // envoy-bin startup error surfaces in test output (jwt_authn + rbac precedent).
    let ready = tokio::time::timeout(
        Duration::from_secs(10),
        wait_ready(listener_addr, Duration::from_secs(10)),
    )
    .await;
    if ready.is_err() || matches!(&ready, Ok(Err(_))) {
        dump_stderr_and_kill(&mut child).await;
        panic!("envoy-bin listener never became ready at {listener_addr}");
    }

    // ---- 4 sequential probes — §6.2 CORS semantics verification ----------------

    // probe 1: OPTIONS preflight, allowed origin
    //   → 200, empty body, ACAO=http://allowed.example.com, ACAM present,
    //     ACAH present, ACMA="3600" present, NO content-type (ADR-0059)
    let (s1, h1, b1) = http_probe(
        listener_addr,
        "OPTIONS",
        &[
            "Origin: http://allowed.example.com\r\n",
            "Access-Control-Request-Method: GET\r\n",
        ],
    )
    .await;

    // probe 2: GET with allowed origin (CORS decoration path)
    //   → 200, body "ok\n", ACAO=http://allowed.example.com
    let (s2, h2, b2) = http_probe(
        listener_addr,
        "GET",
        &["Origin: http://allowed.example.com\r\n"],
    )
    .await;

    // probe 3: GET with disallowed origin (L7 negative path — envoy-rust-only
    //   behavior: pass through to backend but strip CORS response headers)
    //   → 200, body "ok\n", NO access-control-allow-origin
    let (s3, h3, b3) = http_probe(
        listener_addr,
        "GET",
        &["Origin: http://evil.example.com\r\n"],
    )
    .await;

    // probe 4: GET with no Origin (non-CORS request)
    //   → 200, body "ok\n", NO access-control-* headers
    let (s4, h4, b4) = http_probe(listener_addr, "GET", &[]).await;

    // ---- Assertions — dump stderr on any failure --------------------------------

    let all_ok = s1 == 200
        && b1.is_empty()
        && find_header(&h1, "access-control-allow-origin") == Some("http://allowed.example.com")
        && find_header(&h1, "access-control-allow-methods").is_some()
        && find_header(&h1, "access-control-allow-headers").is_some()
        && find_header(&h1, "access-control-max-age") == Some("3600")
        && find_header(&h1, "content-type").is_none()
        && s2 == 200
        && b2 == b"ok\n"
        && find_header(&h2, "access-control-allow-origin") == Some("http://allowed.example.com")
        && s3 == 200
        && b3 == b"ok\n"
        && find_header(&h3, "access-control-allow-origin").is_none()
        && s4 == 200
        && b4 == b"ok\n"
        && !h4
            .iter()
            .any(|(k, _)| k.to_ascii_lowercase().starts_with("access-control-"));
    if !all_ok {
        dump_stderr_and_kill(&mut child).await;
    }

    // probe 1: OPTIONS preflight with allowed origin
    assert_eq!(s1, 200, "probe-1 (preflight, allowed origin) → 200");
    assert!(
        b1.is_empty(),
        "probe-1: preflight body must be empty (ADR-0059); got: {b1:?}"
    );
    assert_eq!(
        find_header(&h1, "access-control-allow-origin"),
        Some("http://allowed.example.com"),
        "probe-1: access-control-allow-origin must echo the allowed origin; headers: {h1:?}"
    );
    assert!(
        find_header(&h1, "access-control-allow-methods").is_some(),
        "probe-1: access-control-allow-methods must be present; headers: {h1:?}"
    );
    assert!(
        find_header(&h1, "access-control-allow-headers").is_some(),
        "probe-1: access-control-allow-headers must be present; headers: {h1:?}"
    );
    assert_eq!(
        find_header(&h1, "access-control-max-age"),
        Some("3600"),
        "probe-1: access-control-max-age must be \"3600\" (per policy); headers: {h1:?}"
    );
    assert!(
        find_header(&h1, "content-type").is_none(),
        "probe-1: content-type must be absent on empty preflight response (ADR-0059); headers: {h1:?}"
    );

    // probe 2: GET with allowed origin — decoration path
    assert_eq!(s2, 200, "probe-2 (GET, allowed origin) → 200");
    assert_eq!(b2.as_slice(), b"ok\n", "probe-2 body");
    assert_eq!(
        find_header(&h2, "access-control-allow-origin"),
        Some("http://allowed.example.com"),
        "probe-2: access-control-allow-origin must echo the allowed origin; headers: {h2:?}"
    );

    // probe 3: GET with disallowed origin — no CORS headers
    assert_eq!(s3, 200, "probe-3 (GET, disallowed origin) → 200");
    assert_eq!(b3.as_slice(), b"ok\n", "probe-3 body");
    assert!(
        find_header(&h3, "access-control-allow-origin").is_none(),
        "probe-3: access-control-allow-origin must be ABSENT for disallowed origin; headers: {h3:?}"
    );

    // probe 4: GET with no origin — no access-control-* headers at all
    assert_eq!(s4, 200, "probe-4 (GET, no origin) → 200");
    assert_eq!(b4.as_slice(), b"ok\n", "probe-4 body");
    assert!(
        !h4.iter()
            .any(|(k, _)| k.to_ascii_lowercase().starts_with("access-control-")),
        "probe-4: no access-control-* headers must be present for a non-CORS request; headers: {h4:?}"
    );

    // Explicit kill + wait on the success path (kill_on_drop is the safety net;
    // explicit kill+wait is the discipline per the jwt_authn + rbac precedent).
    child.kill().await.ok();
    let _ = child.wait().await;
}
