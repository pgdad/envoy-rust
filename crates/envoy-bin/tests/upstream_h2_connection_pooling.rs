//! 13.2 Task 6 (D9.3-H2): in-process H2 backstop for the H2-pool-reuse
//! property. Sibling of the 13.1 H1 backstop at
//! `crates/envoy-bin/tests/upstream_connection_pooling.rs`; mirrors the
//! bilateral Docker fixture 0021 (`upstream_h2_connection_pooling`) at
//! the cheap in-process subprocess scope.
//!
//! Shape:
//!   1. Spawn the `http2-echo-server` helper (phase 05.3) on a reserved
//!      port (H2C — plaintext H2 — no per-path flag; the helper always
//!      200-echos every request).
//!   2. Spawn `envoy-bin` with a synthesized bootstrap pointing a STATIC
//!      `backend_cluster` at the helper's port. The cluster carries the
//!      `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`
//!      block (H2 upstream); the HCM listener uses `codec_type: HTTP2`
//!      (H2 downstream — per ADR-0039 to avoid the
//!      `ConfigError::Http2ClusterFromHttp1Listener` gate at
//!      `crates/envoy-config/src/bootstrap.rs:1997-2016`).
//!   3. Open ONE downstream H2 conn via `h2::client::handshake`, drive
//!      5 sequential `GET /` streams via cloned `SendRequest<()>`
//!      (mirroring `tests/differential/src/lib.rs::drive_http2_keep_alive`'s
//!      per-stream-clone idiom — the documented H2 multiplex shape; under
//!      sequential await the streams complete one at a time but the
//!      upstream pool can still multiplex them on a single upstream
//!      conn).
//!   4. Settle 500ms (matches fixture 0021's `settle_ms: 500`), GET
//!      `/stats` from admin, assert the 5 counter rows fixture 0021
//!      pins: `upstream_cx_total = 1` (THE H2-pool-reuse property —
//!      single upstream conn for all 5 streams) +
//!      `upstream_cx_http2_total = 1` (the 13.2 D7.2 per-codec split) +
//!      `upstream_rq_total = 5` + `downstream_rq_2xx = 5` +
//!      `downstream_rq_total = 5`.
//!
//! Per phase-09 REVIEW M3 disposition + SPEC §6.4: uses
//! `tokio::process::Command` with `.kill_on_drop(true)`,
//! `stdout: Stdio::null()`, and `stderr: Stdio::piped()`. Discipline
//! copied verbatim from the H1 sibling.
//!
//! The 5-standard-header presence assertion (per 10 REVIEW M1 + 13.1
//! Task 8) is omitted at the H2 surface: H2 has no concept of the H1
//! standard header roster (`server`/`date`/`content-length`/
//! `content-type`/`connection`); the H1 sibling check was a per-non-2xx
//! discipline that does not translate. The fixture-0021 workload is
//! all-2xx; the H1 sibling's check only fires on non-2xx; under H2 the
//! check has no analog. The discipline is preserved on the H1 side
//! (`upstream_connection_pooling.rs:131-146`); no carry-forward to the
//! H2 sibling is meaningful.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Settle window past the last response before scraping admin stats.
/// Matches fixture 0021's `settle_ms: 500` (Task 5 ADR-0039 pivot).
const SETTLE_MS: u64 = 500;

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// Wait for TCP-accept readiness on `addr` within `budget`. Identical
/// shape to the H1 sibling's `wait_ready`. H2 handshake readiness is
/// checked separately at the H2-driver site (the proxy's H2 listener
/// accepts TCP first, then completes the HTTP/2 preface — TCP readiness
/// is the gating signal here, matching `http2_router_upstream.rs`'s
/// pattern).
async fn wait_ready(addr: SocketAddr, budget: Duration) -> std::io::Result<()> {
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

/// Open a fresh TCP conn to admin, GET `/stats`, parse the
/// `<name>: <value>` text lines into a map (only rows with a numeric
/// value are retained). Identical shape to the H1 sibling — the admin
/// listener is HTTP/1.1 regardless of the data-plane HCM `codec_type`.
async fn scrape_admin_stats(admin: SocketAddr) -> HashMap<String, u64> {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(admin))
        .await
        .expect("admin connect timeout")
        .expect("admin connect");
    let req = "GET /stats HTTP/1.1\r\nHost: admin\r\nConnection: close\r\n\r\n";
    stream.write_all(req.as_bytes()).await.expect("admin write");
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("admin read timeout")
        .expect("admin read");
    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("admin head terminator");
    let body = std::str::from_utf8(&buf[head_end + 4..]).expect("admin body utf8");
    let mut out = HashMap::new();
    for line in body.lines() {
        if let Some((name, value)) = line.split_once(": ")
            && let Ok(v) = value.trim().parse::<u64>()
        {
            out.insert(name.trim().to_string(), v);
        }
    }
    out
}

fn assert_stat(stats: &HashMap<String, u64>, name: &str, expected: u64) {
    let actual = stats
        .get(name)
        .copied()
        .unwrap_or_else(|| panic!("stat {name:?} absent; have {} rows", stats.len()));
    assert_eq!(
        actual, expected,
        "stat {name:?}: expected {expected}, got {actual}"
    );
}

/// Spawn the `http2-echo-server` helper. Mirrors the H1 sibling's
/// `cargo run --manifest-path` pattern (the helper isn't pre-built;
/// `cargo run` compiles-on-demand on first hit and then re-uses the
/// artifact across subsequent runs). The helper has no `--per-path`
/// flag — it always returns 200 — so the workload simplifies to 5
/// GETs to `/`.
async fn spawn_backend(port: u16) -> tokio::process::Child {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join("tests/helpers/http2-echo-server/Cargo.toml");
    tokio::process::Command::new(env!("CARGO"))
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--")
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn http2-echo-server backend")
}

/// Boot envoy-bin with a STATIC bootstrap pointing `backend_cluster` at
/// `backend_port`. Admin listener bound at the reserved `admin_port`.
///
/// Topology per ADR-0039:
/// - Downstream HCM `codec_type: HTTP2` (H2 listener). The H1-listener
///   × H2-cluster path is rejected by the parse-time gate at
///   `crates/envoy-config/src/bootstrap.rs:1997-2016`
///   (`ConfigError::Http2ClusterFromHttp1Listener` — ADR-0028 deferral).
/// - Upstream cluster carries
///   `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.explicit_http_config.http2_protocol_options: {}`
///   (H2 upstream).
async fn spawn_envoy_bin(
    hcm_port: u16,
    admin_port: u16,
    backend_port: u16,
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
                codec_type: HTTP2
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
            max_connections: 4
      typed_extension_protocol_options:
        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {{}}
      load_assignment:
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

/// THE in-process backstop for the H2-pool-reuse property — see
/// fixture 0021 for the bilateral Docker check.
///
/// Workload: 5 sequential `GET /` streams over ONE downstream H2 conn
/// (mirrors fixture 0021's driver verbatim). Per ADR-0039 + the Task 5
/// fixture-0021 acceptance, the upstream H2 pool reuses ONE upstream
/// conn for all 5 stream-dispatches: `upstream_cx_total = 1` and
/// `upstream_cx_http2_total = 1`.
#[tokio::test(flavor = "multi_thread")]
async fn upstream_h2_connection_pooling() {
    let hcm_port = reserve_port();
    let admin_port = reserve_port();
    let backend_port = reserve_port();
    let hcm_addr: SocketAddr = format!("127.0.0.1:{hcm_port}").parse().unwrap();
    let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let backend_addr: SocketAddr = format!("127.0.0.1:{backend_port}").parse().unwrap();

    let _backend = spawn_backend(backend_port).await;
    // 30s budget matches the H1 sibling's backend readiness budget
    // (the `cargo run --manifest-path` step compiles on first hit;
    // subsequent runs reuse the cached artifact and return in <1s).
    wait_ready(backend_addr, Duration::from_secs(30))
        .await
        .expect("backend ready");

    let _envoy = spawn_envoy_bin(hcm_port, admin_port, backend_port).await;
    wait_ready(hcm_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin HCM ready");
    wait_ready(admin_addr, Duration::from_secs(10))
        .await
        .expect("envoy-bin admin ready");

    // Open ONE downstream H2 conn — drives all 5 sequential streams.
    // Mirrors `drive_http2_keep_alive` at
    // `tests/differential/src/lib.rs:1506-1612` modulo inline-adaptation
    // for the backstop scope (we don't need the helper's `side_name`
    // error context or the `Http1KeepAliveRequest`/`KeepAliveExpectedStat`
    // serde-driven workload — the backstop's workload is fixed).
    let tcp = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(hcm_addr))
        .await
        .expect("downstream H2 connect timeout")
        .expect("downstream H2 connect");
    let (send_request, conn) = h2::client::handshake(tcp)
        .await
        .expect("downstream H2 handshake");
    let conn_handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    for _ in 0..5 {
        // Per-stream clone of `SendRequest` is the documented h2
        // multiplex idiom (h2::client::SendRequest derives Clone
        // precisely to support multiplexed stream issuance from one
        // connection). Mirrors the differential helper.
        let mut sr = send_request.clone();
        // Absolute-form URI so the h2 codec populates `:authority`.
        let req = http::Request::builder()
            .method("GET")
            .uri("http://backend_cluster/")
            .body(())
            .expect("build H2 request");
        let (response_fut, _send_stream) = sr
            .send_request(req, /*end_of_stream=*/ true)
            .expect("send_request");
        let resp = tokio::time::timeout(Duration::from_secs(10), response_fut)
            .await
            .expect("H2 response timeout")
            .expect("H2 response");
        assert_eq!(resp.status().as_u16(), 200, "expected 200 for GET /");
        // Drain the response body so the stream completes cleanly
        // before issuing the next request. Best-effort flow-control
        // release (errors here re-surface on the next data().await if
        // the stream is broken).
        let mut body_stream = resp.into_body();
        while let Some(chunk) = body_stream.data().await {
            let chunk = chunk.expect("H2 body chunk");
            body_stream
                .flow_control()
                .release_capacity(chunk.len())
                .ok();
        }
    }

    // Teardown mirrors `drive_http2_keep_alive`'s shape: drop the last
    // `SendRequest` handle (closes the h2 Connection future's inbound
    // channel) then abort the conn-driving spawn so the test returns
    // as soon as the response is drained, without tying wall-time to
    // peer-side GOAWAY round-trips. The post-abort future is never
    // polled again, so no clean GOAWAY fires — that's intentional per
    // the Task 5 fold-in's teardown-comment correction.
    drop(send_request);
    conn_handle.abort();
    let _ = conn_handle.await;

    // Settle for the stat increments to flush. Matches fixture 0021's
    // `settle_ms: 500`.
    tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;

    let stats = scrape_admin_stats(admin_addr).await;

    // The 5 fixture-0021 stat rows verbatim (per ADR-0039).
    //
    // Downstream HCM counters (2): all-2xx workload.
    assert_stat(&stats, "http.ingress_http.downstream_rq_2xx", 5);
    assert_stat(&stats, "http.ingress_http.downstream_rq_total", 5);
    // Cluster-side upstream counters (3): the H2-pool-reuse property
    // + the 13.2 D7.2 per-codec stat split.
    assert_stat(&stats, "cluster.backend_cluster.upstream_rq_total", 5);
    // THE H2-pool-reuse property — single upstream conn for all 5
    // streams (the downstream side multiplexes 5 streams over 1 H2
    // conn; the upstream pool dispatches each over the same 1 pooled
    // upstream H2 conn).
    assert_stat(&stats, "cluster.backend_cluster.upstream_cx_total", 1);
    // The per-codec split (13.2 D7.2 `cluster.<name>.upstream_cx_http2_total`).
    assert_stat(&stats, "cluster.backend_cluster.upstream_cx_http2_total", 1);
}
