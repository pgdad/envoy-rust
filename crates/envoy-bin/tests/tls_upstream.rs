//! Phase 03.2 backstop: plaintext downstream → envoy-bin → TLS upstream.
//! Spawns envoy-bin as a subprocess pointing at an in-process tokio_rustls
//! TLS echo server. No Docker; runs on every `cargo test`. Mirror of
//! `tests/tls_downstream.rs` from phase 03.1.

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, IsCa, KeyPair, KeyUsagePurpose};
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

struct UpstreamPki {
    _dir: tempfile::TempDir,
    ca_pem_path: std::path::PathBuf,
    cert_der: rustls::pki_types::CertificateDer<'static>,
    key_der_pkcs8: Vec<u8>,
}

fn build_upstream_pki() -> UpstreamPki {
    let dir = tempfile::tempdir().unwrap();
    let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_kp = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_kp).unwrap();

    let leaf_params = CertificateParams::new(vec!["envoy-rust.test".into()]).unwrap();
    let leaf_kp = KeyPair::generate().unwrap();
    let leaf_cert = leaf_params.signed_by(&leaf_kp, &ca_cert, &ca_kp).unwrap();

    let ca_pem_path = dir.path().join("ca.pem");
    std::fs::write(&ca_pem_path, ca_cert.pem()).unwrap();

    let cert_der = leaf_cert.der().clone().into_owned();
    let key_der_pkcs8 = leaf_kp.serialize_der();

    UpstreamPki {
        _dir: dir,
        ca_pem_path,
        cert_der,
        key_der_pkcs8,
    }
}

async fn spawn_tls_echo(pki: &UpstreamPki) -> SocketAddr {
    let cert_chain = vec![pki.cert_der.clone()];
    let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
        rustls::pki_types::PrivatePkcs8KeyDer::from(pki.key_der_pkcs8.clone()),
    );
    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .expect("server config");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(stream).await else {
                    return;
                };
                let (mut r, mut w) = tokio::io::split(&mut tls);
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_upstream_round_trip() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let pki = build_upstream_pki();
    let upstream_addr = spawn_tls_echo(&pki).await;
    let listener_port = reserve_port();
    let ca_pem_str = pki.ca_pem_path.to_string_lossy();

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
                      address: 127.0.0.1
                      port_value: {upstream_port}
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: "envoy-rust.test"
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: {ca_pem_str}
"#,
        upstream_port = upstream_addr.port(),
    );

    let mut cfg_file = tempfile::NamedTempFile::new().unwrap();
    cfg_file.write_all(yaml.as_bytes()).unwrap();
    cfg_file.flush().unwrap();

    let bin = env!("CARGO_BIN_EXE_envoy-bin");
    let mut child = tokio::process::Command::new(bin)
        .arg("-c")
        .arg(cfg_file.path())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_ready(listener_addr, Duration::from_secs(10)).await;

    let mut stream = TcpStream::connect(listener_addr).await.expect("connect");
    let payload = b"hello, tls upstream\n";
    stream.write_all(payload).await.expect("write");
    let mut response = vec![0u8; payload.len()];
    stream.read_exact(&mut response).await.expect("read_exact");
    assert_eq!(response, payload);

    drop(stream);
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
}
