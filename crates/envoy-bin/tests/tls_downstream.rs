//! Phase 03.1 backstop: TLS-terminating tcp_proxy through envoy-bin against
//! an in-process plaintext echo upstream. Mirror of `tests/tcp_proxy.rs` from
//! phase 02.2. The real differential assertion is the Docker-gated
//! `tests/differential/tests/tls_downstream.rs` (Task 12).

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod common;

use common::{reserve_port, wait_ready};

struct TestPki {
    _dir: tempfile::TempDir,
    leaf_cert: std::path::PathBuf,
    leaf_key: std::path::PathBuf,
    ca_der: CertificateDer<'static>,
}

fn build_pki() -> TestPki {
    let dir = tempfile::tempdir().unwrap();
    let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_kp = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_kp).unwrap();

    let leaf_params = CertificateParams::new(vec!["a.example.com".into()]).unwrap();
    let leaf_kp = KeyPair::generate().unwrap();
    let leaf_cert = leaf_params.signed_by(&leaf_kp, &ca_cert, &ca_kp).unwrap();

    let leaf_pem_path = dir.path().join("leaf.pem");
    let leaf_key_path = dir.path().join("leaf.key");
    std::fs::write(&leaf_pem_path, leaf_cert.pem()).unwrap();
    std::fs::write(&leaf_key_path, leaf_kp.serialize_pem()).unwrap();

    let ca_der: CertificateDer<'static> = ca_cert.der().clone().into_owned();
    TestPki {
        _dir: dir,
        leaf_cert: leaf_pem_path,
        leaf_key: leaf_key_path,
        ca_der,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_downstream_round_trips_through_envoy_bin() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // In-process plaintext echo backend.
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

    let pki = build_pki();
    let listener_port = reserve_port();
    let leaf_cert_str = pki.leaf_cert.to_string_lossy();
    let leaf_key_str = pki.leaf_key.to_string_lossy();
    let yaml = format!(
        r#"
static_resources:
  listeners:
    - name: tls_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: {leaf_cert_str}
                    private_key:
                      filename: {leaf_key_str}
          filters:
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
                      port_value: {backend_port}
"#,
        backend_port = backend_addr.port(),
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
    wait_ready(listener_addr, Duration::from_secs(10))
        .await
        .expect("listener never became ready");

    // TLS client: trust only the test CA.
    let mut roots = rustls::RootCertStore::empty();
    roots.add(pki.ca_der.clone()).unwrap();
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let server_name = ServerName::try_from("a.example.com").unwrap();

    let tcp = TcpStream::connect(listener_addr).await.unwrap();
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .expect("handshake");

    let payload = b"tls round-trip through envoy-bin";
    tls.write_all(payload).await.unwrap();
    let mut buf = vec![0u8; payload.len()];
    tls.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, payload);

    tls.shutdown().await.ok();
    drop(tls);

    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
}
