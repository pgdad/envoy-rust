//! Phase 08.1 D17.4a — in-process backstop for the 4 new admin endpoints
//! landed across Tasks 6-9 (`/config_dump`, `/server_info`, `/clusters`,
//! `/listeners`). Spawns `envoy-bin` against an in-memory bootstrap that
//! carries the admin listener + 1 listener (empty `filter_chains`) + 1
//! STRICT_DNS cluster (one populated locality / one lb_endpoint), scrapes
//! each of the 4 endpoints via a one-shot HTTP/1.1 client, and asserts:
//!
//!   - `/config_dump` → 200, body parses as JSON, top-level `configs` key
//!     present (Task 6 envelope).
//!   - `/server_info` → 200, body parses as JSON, `state == "LIVE"`
//!     (Task 7 envelope).
//!   - `/clusters`   → 200, plain-text body contains `backstop_cluster::`
//!     (Task 8 line-format).
//!   - `/listeners`  → 200, plain-text body contains
//!     `listener_0::0.0.0.0:<port>` (Task 9 line-format).
//!
//! No Docker — the in-process happy-path complement to Task 11's
//! Docker-gated fixture-0014 bilateral assertion. Mirrors the shape of
//! `crates/envoy-bin/tests/admin_only.rs` (single `#[tokio::test]`,
//! `reserve_port()` + `wait_ready()` + one-shot TCP scrape with
//! `Connection: close` + `shutdown(Write)` against the admin handler's
//! 5-second idle-read timeout).

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready_result(addr: SocketAddr, budget: Duration) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => return Err(e),
        }
    }
}

struct ScrapeResult {
    status: u16,
    body: Vec<u8>,
}

/// One-shot HTTP/1.1 GET against the admin port. Sends `Connection: close`
/// and half-closes the write side after the request so the handler's
/// 5-second idle-read timeout (admin handler `IDLE_READ_TIMEOUT`) does not
/// gate EOF. Reads to EOF, then splits off the response body and parses
/// the status line via `httparse::Response`.
async fn scrape(admin_port: u16, path: &str) -> ScrapeResult {
    let addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
    let mut s = TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    s.shutdown().await.ok();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.unwrap();

    let mut hdr_storage = [httparse::EMPTY_HEADER; 32];
    let mut resp = httparse::Response::new(&mut hdr_storage);
    let parsed = resp
        .parse(&buf)
        .unwrap_or_else(|e| panic!("parse {path} response: {e}; bytes: {buf:?}"));
    let headers_end = match parsed {
        httparse::Status::Complete(n) => n,
        httparse::Status::Partial => panic!(
            "incomplete response for {path}: {:?}",
            String::from_utf8_lossy(&buf)
        ),
    };
    let status = resp
        .code
        .unwrap_or_else(|| panic!("response for {path} has no status code"));
    let body = buf[headers_end..].to_vec();
    ScrapeResult { status, body }
}

#[tokio::test]
async fn admin_config_dump_server_info_in_process() {
    let admin_port = reserve_port();
    let listener_port = reserve_port();

    // Bootstrap shape — applies the 4 schema constraints surfaced at Task 12
    // plus one extra adaptation surfaced at Task 13 build-time (deviation 5
    // in PROGRESS):
    //   - no `connect_timeout` (Cluster has #[serde(deny_unknown_fields)]).
    //   - 1 populated locality + 1 lb_endpoint (EmptyClusterEndpoints validator).
    //   - `lb_policy: ROUND_ROBIN` (mandatory Cluster field).
    //   - 1 listener only (TooManyListeners validator caps at 1).
    //   - listener carries 1 `envoy.filters.network.echo` filter (the
    //     simplest filter the bin recognises); empty `filter_chains` parses
    //     but envoy-bin's startup expects ≥1 filter (crates/envoy-bin/src/main.rs).
    // STRICT_DNS target is `localhost:7001` — known-resolvable across the
    // platforms the workspace tests on (matches `cluster.rs`'s 05.1 golden
    // test) so the cluster-build-time `lookup_host` succeeds.
    let yaml = format!(
        r#"
node:
  id: backstop-test
  cluster: backstop-cluster
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
  clusters:
    - name: backstop_cluster
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backstop_cluster
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7001
"#
    );

    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
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

    // /config_dump — 200 + JSON-parseable + top-level `configs` key present.
    let cd = scrape(admin_port, "/config_dump").await;
    assert_eq!(cd.status, 200, "/config_dump status: body={:?}", cd.body);
    let cd_json: serde_json::Value = serde_json::from_slice(&cd.body)
        .unwrap_or_else(|e| panic!("/config_dump body not JSON: {e}; body: {:?}", cd.body));
    assert!(
        cd_json.get("configs").is_some(),
        "/config_dump JSON missing `configs` key: {cd_json}"
    );

    // /server_info — 200 + JSON-parseable + `state == "LIVE"`.
    let si = scrape(admin_port, "/server_info").await;
    assert_eq!(si.status, 200, "/server_info status; body={:?}", si.body);
    let si_json: serde_json::Value = serde_json::from_slice(&si.body)
        .unwrap_or_else(|e| panic!("/server_info body not JSON: {e}; body: {:?}", si.body));
    assert_eq!(
        si_json.get("state").and_then(|v| v.as_str()),
        Some("LIVE"),
        "/server_info `state` field; got: {si_json}"
    );

    // /clusters — 200 + plain-text body contains `backstop_cluster::` line(s).
    let cl = scrape(admin_port, "/clusters").await;
    assert_eq!(cl.status, 200, "/clusters status; body={:?}", cl.body);
    let cl_body = String::from_utf8(cl.body).expect("/clusters body utf-8");
    assert!(
        cl_body.contains("backstop_cluster::"),
        "/clusters body missing `backstop_cluster::` line; body: {cl_body:?}"
    );

    // /listeners — 200 + plain-text body contains `listener_0::0.0.0.0:<port>`.
    let ls = scrape(admin_port, "/listeners").await;
    assert_eq!(ls.status, 200, "/listeners status; body={:?}", ls.body);
    let ls_body = String::from_utf8(ls.body).expect("/listeners body utf-8");
    let needle = format!("listener_0::0.0.0.0:{listener_port}");
    assert!(
        ls_body.contains(&needle),
        "/listeners body missing `{needle}`; body: {ls_body:?}"
    );

    child.kill().await.ok();
    let _ = child.wait().await;
}
