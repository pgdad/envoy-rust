//! Phase 02.2 backstop: write a tcp_proxy config pointing at an in-process
//! tokio echo server, spawn `envoy-bin` as a subprocess, drive a payload
//! through the listener, assert byte-exact round-trip. Mirror of
//! `tests/admin_only.rs` from phase 01. The real differential assertion is
//! the Docker-gated `tests/differential/tests/tcp_proxy.rs` (Task 12).

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
            Err(e) => panic!("listener never became ready at {addr}: {e}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tcp_proxy_round_trips_through_envoy_bin() {
    // Spawn an in-process echo server as the upstream backend. (We do NOT
    // use the tcp-echo-server helper binary here — that's reserved for the
    // Docker-side differential harness in Task 12.)
    let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_addr = backend_listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = backend_listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });

    let listener_port = reserve_port();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: tcp_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
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
                      address: {backend_ip}
                      port_value: {backend_port}
"#,
        backend_ip = backend_addr.ip(),
        backend_port = backend_addr.port(),
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

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_ready(listener_addr, Duration::from_secs(10)).await;

    let mut s = TcpStream::connect(listener_addr).await.unwrap();
    let payload = b"hello, tcp_proxy\n";
    s.write_all(payload).await.unwrap();
    let mut buf = vec![0u8; payload.len()];
    s.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, payload);
    s.shutdown().await.ok();
    drop(s);

    child.kill().await.ok();
    let _ = child.wait().await;
}
