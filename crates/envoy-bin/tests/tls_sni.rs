//! Phase 03.2 backstop: downstream multi-cert SNI cert selection through
//! envoy-bin. Two filter chains on a single listener; two TLS handshakes
//! (one per SNI); per-probe byte-exact peer-cert DER assertion.

#![forbid(unsafe_code)]

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use rcgen::{CertificateParams, IsCa, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{CertificateDer, ServerName};
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

struct DownstreamPki {
    _dir: tempfile::TempDir,
    leaf_a_cert_pem_path: std::path::PathBuf,
    leaf_a_key_pem_path: std::path::PathBuf,
    leaf_b_cert_pem_path: std::path::PathBuf,
    leaf_b_key_pem_path: std::path::PathBuf,
    leaf_a_der: CertificateDer<'static>,
    leaf_b_der: CertificateDer<'static>,
    ca_der: CertificateDer<'static>,
}

fn build_downstream_pki() -> DownstreamPki {
    let dir = tempfile::tempdir().unwrap();
    let mut ca_params = CertificateParams::new(vec!["test-ca".into()]).unwrap();
    ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_kp = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_kp).unwrap();

    let leaf_a_params = CertificateParams::new(vec!["a.example.com".into()]).unwrap();
    let leaf_a_kp = KeyPair::generate().unwrap();
    let leaf_a_cert = leaf_a_params
        .signed_by(&leaf_a_kp, &ca_cert, &ca_kp)
        .unwrap();
    let leaf_a_cert_pem_path = dir.path().join("leaf-a.pem");
    let leaf_a_key_pem_path = dir.path().join("leaf-a.key");
    std::fs::write(&leaf_a_cert_pem_path, leaf_a_cert.pem()).unwrap();
    std::fs::write(&leaf_a_key_pem_path, leaf_a_kp.serialize_pem()).unwrap();

    let leaf_b_params = CertificateParams::new(vec!["b.example.com".into()]).unwrap();
    let leaf_b_kp = KeyPair::generate().unwrap();
    let leaf_b_cert = leaf_b_params
        .signed_by(&leaf_b_kp, &ca_cert, &ca_kp)
        .unwrap();
    let leaf_b_cert_pem_path = dir.path().join("leaf-b.pem");
    let leaf_b_key_pem_path = dir.path().join("leaf-b.key");
    std::fs::write(&leaf_b_cert_pem_path, leaf_b_cert.pem()).unwrap();
    std::fs::write(&leaf_b_key_pem_path, leaf_b_kp.serialize_pem()).unwrap();

    DownstreamPki {
        _dir: dir,
        leaf_a_cert_pem_path,
        leaf_a_key_pem_path,
        leaf_b_cert_pem_path,
        leaf_b_key_pem_path,
        leaf_a_der: leaf_a_cert.der().clone().into_owned(),
        leaf_b_der: leaf_b_cert.der().clone().into_owned(),
        ca_der: ca_cert.der().clone().into_owned(),
    }
}

async fn spawn_tcp_echo() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let (mut r, mut w) = stream.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    addr
}

#[tokio::test(flavor = "multi_thread")]
async fn tls_sni_multi_cert_dispatch() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let pki = build_downstream_pki();
    let upstream_addr = spawn_tcp_echo().await;
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
        - filter_chain_match:
            server_names: ["a.example.com"]
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: {leaf_a_cert}
                    private_key:
                      filename: {leaf_a_key}
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match:
            server_names: ["b.example.com"]
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: {leaf_b_cert}
                    private_key:
                      filename: {leaf_b_key}
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
                      port_value: {upstream_port}
"#,
        leaf_a_cert = pki.leaf_a_cert_pem_path.display(),
        leaf_a_key = pki.leaf_a_key_pem_path.display(),
        leaf_b_cert = pki.leaf_b_cert_pem_path.display(),
        leaf_b_key = pki.leaf_b_key_pem_path.display(),
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

    // Build a ClientConfig trusting the test CA.
    let mut roots = rustls::RootCertStore::empty();
    roots.add(pki.ca_der.clone()).unwrap();
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

    // Probe A.
    let tcp = TcpStream::connect(listener_addr).await.unwrap();
    let server_name = ServerName::try_from("a.example.com").unwrap();
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .expect("probe-A handshake");
    let presented_a: CertificateDer<'static> = tls
        .get_ref()
        .1
        .peer_certificates()
        .expect("peer certs A")
        .first()
        .expect("at least one cert A")
        .clone()
        .into_owned();
    assert_eq!(
        presented_a.as_ref(),
        pki.leaf_a_der.as_ref(),
        "SNI a.example.com must select leaf-A",
    );
    let payload_a = b"probe-A\n";
    tls.write_all(payload_a).await.unwrap();
    let mut buf_a = vec![0u8; payload_a.len()];
    tls.read_exact(&mut buf_a).await.unwrap();
    assert_eq!(buf_a, payload_a);
    tls.shutdown().await.ok();
    drop(tls);

    // Probe B.
    let tcp = TcpStream::connect(listener_addr).await.unwrap();
    let server_name = ServerName::try_from("b.example.com").unwrap();
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .expect("probe-B handshake");
    let presented_b: CertificateDer<'static> = tls
        .get_ref()
        .1
        .peer_certificates()
        .expect("peer certs B")
        .first()
        .expect("at least one cert B")
        .clone()
        .into_owned();
    assert_eq!(
        presented_b.as_ref(),
        pki.leaf_b_der.as_ref(),
        "SNI b.example.com must select leaf-B",
    );
    let payload_b = b"probe-B\n";
    tls.write_all(payload_b).await.unwrap();
    let mut buf_b = vec![0u8; payload_b.len()];
    tls.read_exact(&mut buf_b).await.unwrap();
    assert_eq!(buf_b, payload_b);
    tls.shutdown().await.ok();
    drop(tls);

    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
}
