//! End-to-end bin test: write an admin-only config, spawn the `envoy-bin`
//! binary as a subprocess, and verify it serves `GET /ready`. This is a
//! backstop for the main contract — the real differential assertion is the
//! fixture-0002 acceptance test in Task 18.

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

async fn wait_ready(addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return,
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => panic!("admin never became ready at {addr}: {e}"),
        }
    }
}

#[tokio::test]
async fn admin_only_config_serves_ready() {
    let port = reserve_port();
    let yaml = format!(
        r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {port}

static_resources:
  listeners: []
  clusters: []
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
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    wait_ready(addr, Duration::from_secs(10)).await;

    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"GET /ready HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    s.shutdown().await.ok();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).await.unwrap();
    let text = std::str::from_utf8(&buf).unwrap();
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "status: {text:?}");
    assert!(text.ends_with("LIVE\n"), "body: {text:?}");

    child.kill().await.ok();
    let _ = child.wait().await;
}
