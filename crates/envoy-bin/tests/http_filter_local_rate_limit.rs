//! Phase 09 in-process backstop for `envoy.filters.http.local_ratelimit`.
//!
//! Boots `envoy-bin` with a synthesized bootstrap whose HCM contains
//! `http_filters: [local_ratelimit, router]` with `token_bucket { max_tokens:
//! 2, tokens_per_fill: 2, fill_interval: 60s }`. Drives 4 sequential
//! `GET /` requests against the bound listener. Asserts the status sequence
//! `[200, 200, 429, 429]` + body `"local_rate_limited"` on the 429 responses
//! (upstream Envoy v1.33 parity per ADR-0033) + 5 standard HTTP/1.1 response
//! headers (server / date / content-length / content-type / connection) on
//! the 429 responses. No Docker dependency; complementary to the
//! Docker-gated differential fixture at
//! `tests/differential/tests/http_filter_local_rate_limit.rs`.
//!
//! Phase-09 ADR-0033 ("Phase-09 SPEC §2.2 revision per upstream Envoy v1.33
//! empirical observation"): the original PLAN lock-in #33's direct
//! `x-envoy-ratelimited: true` per-header presence assertion is voided —
//! upstream Envoy v1.33's local_ratelimit emits NO `x-envoy-ratelimited`
//! header, and envoy-rust matches per ADR-0033 Commit B (Task 3 fixup at
//! `1c1de0f`). The revised assertion shape (body `"local_rate_limited"` +
//! 5 standard headers + NO `x-envoy-ratelimited`) lands at this commit.
//! The H1 HCM's `decorate_filter_synth_response` helper that populates the
//! 5 standard headers on filter-synth responses landed at ADR-0033 Commit C
//! (`ae2cef0`).

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdListener};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::sleep;

fn reserve_port() -> u16 {
    let listener = StdListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind ephemeral");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

async fn wait_ready(addr: SocketAddr, budget: Duration) -> Result<(), String> {
    let deadline = Instant::now() + budget;
    let mut backoff = Duration::from_millis(50);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).await.is_ok() {
            return Ok(());
        }
        sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_millis(500));
    }
    Err(format!(
        "listener at {addr} did not become ready within {budget:?}"
    ))
}

async fn send_request_and_collect(addr: SocketAddr) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timeout")
        .expect("connect ok");
    let req = b"GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n\r\n";
    stream.write_all(req).await.expect("write request");
    let mut buf = Vec::with_capacity(8192);
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("read timeout")
        .expect("read ok");
    parse_response(&buf)
}

fn parse_response(buf: &[u8]) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut resp = httparse::Response::new(&mut headers);
    let body_start = match resp.parse(buf).expect("parse response") {
        httparse::Status::Complete(n) => n,
        httparse::Status::Partial => panic!("partial response: {:?}", buf),
    };
    let status = resp.code.expect("status code");
    let header_list: Vec<(String, String)> = resp
        .headers
        .iter()
        .map(|h| {
            (
                h.name.to_lowercase(),
                String::from_utf8_lossy(h.value).into_owned(),
            )
        })
        .collect();
    let body = buf[body_start..].to_vec();
    (status, header_list, body)
}

#[tokio::test(flavor = "multi_thread")]
async fn local_rate_limit_enforces_429_after_token_exhaustion() {
    // Reserve an ephemeral port for the HCM listener.
    let listen_port = reserve_port();
    let admin_port = reserve_port();
    let listen_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), listen_port);

    // Synthesize bootstrap. token_bucket `max_tokens: 2, tokens_per_fill: 2,
    // fill_interval: 60s`: tokens_per_fill mirrors max_tokens for symmetry
    // with the fixture-0016 envoy-rust.yaml shape (envoy-rust accepts both 0
    // and N per validator lock-in #4; the 60s fill_interval makes refill
    // semantic moot within the 4-probe burst either way).
    let bootstrap = format!(
        r#"admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
node:
  cluster: phase-09-backstop
  id: phase-09-backstop
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listen_port}
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
                  - name: envoy.filters.http.local_ratelimit
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit
                      stat_prefix: phase_09_backstop
                      token_bucket:
                        max_tokens: 2
                        tokens_per_fill: 2
                        fill_interval: 60s
                      status: {{ code: 429 }}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    );

    // Write bootstrap to tempfile.
    let mut tempfile = tempfile::NamedTempFile::new().expect("tempfile");
    tempfile
        .write_all(bootstrap.as_bytes())
        .expect("write yaml");
    let yaml_path = tempfile.path().to_path_buf();

    // Spawn envoy-bin subprocess.
    let exe = env!("CARGO_BIN_EXE_envoy-bin");
    let mut child = Command::new(exe)
        .arg("-c")
        .arg(&yaml_path)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn envoy-bin");

    // Wait for the listener to bind.
    let wait_result = wait_ready(listen_addr, Duration::from_secs(5)).await;
    if let Err(e) = wait_result {
        let _ = child.kill();
        panic!("envoy-bin did not become ready: {e}");
    }

    // Drive 4 sequential GET / requests.
    let mut statuses = Vec::new();
    let mut header_lists = Vec::new();
    let mut body_lists = Vec::new();
    for _ in 0..4 {
        let (status, headers, body) = send_request_and_collect(listen_addr).await;
        statuses.push(status);
        header_lists.push(headers);
        body_lists.push(body);
    }

    // Cleanup: kill the subprocess.
    let _ = child.kill();
    let _ = child.wait();

    // Assert the status sequence.
    assert_eq!(
        statuses,
        vec![200u16, 200, 429, 429],
        "expected [200, 200, 429, 429], got {statuses:?}"
    );

    // ADR-0033 (upstream Envoy v1.33 parity): the two 429 responses (probes 3
    // and 4, 0-indexed) carry body `"local_rate_limited"` (18 bytes,
    // source-hardcoded on upstream; envoy-rust matches via Commit B's
    // `Bytes::from_static`).
    for (i, body) in body_lists.iter().enumerate().skip(2) {
        assert_eq!(
            body.as_slice(),
            b"local_rate_limited",
            "probe {i} (429 response) body must be `local_rate_limited`; got {:?}",
            String::from_utf8_lossy(body)
        );
    }
    // ADR-0033 Commit C: the H1 HCM's `decorate_filter_synth_response` helper
    // adds the 5 standard HTTP/1.1 response headers to filter-synth responses.
    // Assert all 5 are present on the 429 responses.
    for (i, headers) in header_lists.iter().enumerate().skip(2) {
        for standard in &[
            "server",
            "date",
            "content-length",
            "content-type",
            "connection",
        ] {
            let present = headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case(standard));
            assert!(
                present,
                "probe {i} (429 response) missing standard header {standard:?}; headers={headers:?}"
            );
        }
    }
    // ADR-0033 (upstream Envoy v1.33 parity): upstream's local_ratelimit
    // emits NO `x-envoy-ratelimited` header (that header belongs to the
    // global ratelimit filter, not local_ratelimit). envoy-rust matches per
    // Commit B's Task 3 fixup. Assert all 4 probes (both 200 and 429
    // responses) do NOT carry `x-envoy-ratelimited`.
    for (i, headers) in header_lists.iter().enumerate() {
        let has = headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("x-envoy-ratelimited"));
        assert!(
            !has,
            "probe {i} unexpectedly carries x-envoy-ratelimited (ADR-0033 voids); headers={headers:?}"
        );
    }
    // Assert the two 200 responses carry body "ok\n" (direct_response
    // inline string; unchanged from original PLAN).
    for (i, body) in body_lists.iter().enumerate().take(2) {
        assert_eq!(
            body.as_slice(),
            b"ok\n",
            "probe {i} (200 response) body must be `ok\\n`; got {:?}",
            String::from_utf8_lossy(body)
        );
    }
}
