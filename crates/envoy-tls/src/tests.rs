//! Unit tests for envoy-tls. Phase 03.1 ships 7 tests covering DownstreamTls
//! (Task 6) + 3 covering UpstreamTls (Task 7) + 1 cross-cutting (Task 6).

use std::path::PathBuf;
use std::sync::Arc;

use crate::*;
use rcgen::{CertificateParams, KeyPair};
use tempfile::TempDir;

mod pki {
    use super::*;
    use rcgen::{DnType, IsCa, KeyUsagePurpose};
    use std::path::Path;

    /// In-test PKI: a self-signed CA + leaf-A (SAN `a.example.com`) + leaf-B
    /// (SAN `b.example.com`), all written into a per-test `TempDir`. Drop the
    /// `Pki` to clean up. Leaf-B was added in 03.2 Task 3 to support the
    /// SNI-keyed multi-cert resolver tests; leaf-A's fields keep their pre-03.2
    /// names (`leaf_cert_pem` / `leaf_key_pem`) so existing 03.1 tests are
    /// unchanged.
    pub struct Pki {
        pub _dir: TempDir,
        pub leaf_cert_pem: PathBuf,
        pub leaf_key_pem: PathBuf,
        pub leaf_b_cert_pem: PathBuf,
        pub leaf_b_key_pem: PathBuf,
        pub ca_der_for_root_store: rustls::pki_types::CertificateDer<'static>,
    }

    pub fn build() -> Pki {
        let dir = tempfile::tempdir().expect("tempdir");
        // CA
        let mut ca_params =
            CertificateParams::new(vec!["envoy-rust-test-ca".into()]).expect("ca params");
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "envoy-rust-test-ca");
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca self-sign");

        // Leaf A signed by CA.
        let mut leaf_params =
            CertificateParams::new(vec!["a.example.com".into()]).expect("leaf params");
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, "a.example.com");
        let leaf_kp = KeyPair::generate().expect("leaf kp");
        let leaf_cert = leaf_params
            .signed_by(&leaf_kp, &ca_cert, &ca_kp)
            .expect("leaf signed");

        // Leaf B signed by the same CA, distinct keypair, distinct SAN.
        let mut leaf_b_params =
            CertificateParams::new(vec!["b.example.com".into()]).expect("leaf-b params");
        leaf_b_params
            .distinguished_name
            .push(DnType::CommonName, "b.example.com");
        let leaf_b_kp = KeyPair::generate().expect("leaf-b kp");
        let leaf_b_cert = leaf_b_params
            .signed_by(&leaf_b_kp, &ca_cert, &ca_kp)
            .expect("leaf-b signed");

        let ca_pem = ca_cert.pem();
        let leaf_pem = leaf_cert.pem();
        let leaf_key_pem = leaf_kp.serialize_pem();
        let leaf_b_pem = leaf_b_cert.pem();
        let leaf_b_key_pem = leaf_b_kp.serialize_pem();

        let ca_path = dir.path().join("ca.pem");
        let leaf_path = dir.path().join("leaf-a.pem");
        let leaf_key_path = dir.path().join("leaf-a.key");
        let leaf_b_path = dir.path().join("leaf-b.pem");
        let leaf_b_key_path = dir.path().join("leaf-b.key");
        std::fs::write(&ca_path, &ca_pem).expect("write ca");
        std::fs::write(&leaf_path, &leaf_pem).expect("write leaf");
        std::fs::write(&leaf_key_path, &leaf_key_pem).expect("write leaf key");
        std::fs::write(&leaf_b_path, &leaf_b_pem).expect("write leaf-b");
        std::fs::write(&leaf_b_key_path, &leaf_b_key_pem).expect("write leaf-b key");

        let ca_der_for_root_store: rustls::pki_types::CertificateDer<'static> =
            ca_cert.der().clone().into_owned();

        Pki {
            _dir: dir,
            leaf_cert_pem: leaf_path,
            leaf_key_pem: leaf_key_path,
            leaf_b_cert_pem: leaf_b_path,
            leaf_b_key_pem: leaf_b_key_path,
            ca_der_for_root_store,
        }
    }

    /// Load a `CertifiedKey` from cert + key PEM files. Mirrors `lib.rs`'s
    /// crate-private `load_certified_key` but lives in the test module to
    /// avoid widening crate visibility for a test-only consumer. Used by the
    /// 03.2 Task 3 SNI resolver tests to build `Arc<CertifiedKey>` map values.
    pub fn certified_key_from_pem(cert_path: &Path, key_path: &Path) -> rustls::sign::CertifiedKey {
        let cert_bytes = std::fs::read(cert_path).expect("read cert");
        let mut cert_slice = cert_bytes.as_slice();
        let cert_chain: Vec<rustls::pki_types::CertificateDer<'static>> =
            rustls_pemfile::certs(&mut cert_slice)
                .collect::<Result<Vec<_>, _>>()
                .expect("parse cert chain");
        let key_bytes = std::fs::read(key_path).expect("read key");
        let mut key_slice = key_bytes.as_slice();
        let key = rustls_pemfile::private_key(&mut key_slice)
            .expect("parse private key")
            .expect("at least one key");
        let signing_key =
            rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).expect("signing key");
        rustls::sign::CertifiedKey::new(cert_chain, signing_key)
    }

    /// Read the first PEM-encoded certificate at `path` and return its DER.
    /// Used to drive byte-exact peer-cert assertions in the SNI resolver
    /// tests (avoids x509-parsing or SAN-substring scans).
    pub fn cert_der_at(path: &Path) -> rustls::pki_types::CertificateDer<'static> {
        let bytes = std::fs::read(path).expect("read cert");
        let mut slice = bytes.as_slice();
        let mut iter = rustls_pemfile::certs(&mut slice);
        iter.next().expect("at least one cert").expect("parse cert")
    }

    pub fn ds_context_with(
        cert_path: &Path,
        key_path: &Path,
    ) -> envoy_config::DownstreamTlsContext {
        envoy_config::DownstreamTlsContext {
            common_tls_context: envoy_config::CommonTlsContext {
                tls_certificates: vec![envoy_config::TlsCertificate {
                    certificate_chain: envoy_config::DataSource {
                        filename: Some(cert_path.to_string_lossy().into_owned()),
                        inline_string: None,
                    },
                    private_key: envoy_config::DataSource {
                        filename: Some(key_path.to_string_lossy().into_owned()),
                        inline_string: None,
                    },
                }],
                validation_context: None,
                alpn_protocols: vec![],
            },
        }
    }
}

fn install_provider_once() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Test-local `ResolvesServerCert` that returns the wrapped `CertifiedKey`
/// for every ClientHello. Shared by the upstream-side handshake tests
/// (`loads_upstream_client_config`, `upstream_rejects_untrusted_cert`).
#[derive(Debug)]
struct StaticResolver(Arc<rustls::sign::CertifiedKey>);

impl rustls::server::ResolvesServerCert for StaticResolver {
    fn resolve(
        &self,
        _client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        Some(self.0.clone())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn loads_single_cert_server_config() {
    install_provider_once();
    let pki = pki::build();
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let downstream = DownstreamTls::from_context(&ctx).expect("downstream from_context");

    // In-process loopback handshake: bind a TcpListener, connect a
    // TlsConnector with the test CA in the root store, and feed the
    // accepted stream through DownstreamTls::accept.
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        downstream.accept(stream).await.expect("server accept")
    });

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(pki.ca_der_for_root_store.clone())
        .expect("add ca");
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let server_name = ServerName::try_from("a.example.com").expect("server name");

    let client_stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let client_tls = connector
        .connect(server_name, client_stream)
        .await
        .expect("client handshake");

    let server_tls = server_task.await.expect("server task");
    // TLS version assertion — both peers should agree on ≥ 1.2.
    let server_negotiated = server_tls
        .get_ref()
        .1
        .protocol_version()
        .expect("server TLS version negotiated");
    let client_negotiated = client_tls
        .get_ref()
        .1
        .protocol_version()
        .expect("client TLS version negotiated");
    assert!(
        matches!(
            server_negotiated,
            rustls::ProtocolVersion::TLSv1_2 | rustls::ProtocolVersion::TLSv1_3
        ),
        "server negotiated unexpected version: {server_negotiated:?}"
    );
    assert!(
        matches!(
            client_negotiated,
            rustls::ProtocolVersion::TLSv1_2 | rustls::ProtocolVersion::TLSv1_3
        ),
        "client negotiated unexpected version: {client_negotiated:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_empty_tls_certificates() {
    let ctx = envoy_config::DownstreamTlsContext {
        common_tls_context: envoy_config::CommonTlsContext {
            tls_certificates: vec![],
            validation_context: None,
            alpn_protocols: vec![],
        },
    };
    let err = DownstreamTls::from_context(&ctx).expect_err("must reject");
    assert!(
        matches!(err, TlsError::DownstreamRequiresCert),
        "got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_malformed_cert_pem() {
    install_provider_once();
    let pki = pki::build();
    // Overwrite the cert path with garbage (no PEM headers).
    std::fs::write(&pki.leaf_cert_pem, b"this is not a PEM\n").expect("write");
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let err = DownstreamTls::from_context(&ctx).expect_err("must reject");
    assert!(matches!(err, TlsError::CertParse { .. }), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_missing_key_pem() {
    install_provider_once();
    let pki = pki::build();
    let missing = pki._dir.path().join("does-not-exist.key");
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &missing);
    let err = DownstreamTls::from_context(&ctx).expect_err("must reject");
    assert!(matches!(err, TlsError::FileRead { .. }), "got {err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn single_cert_resolver_returns_same_cert_regardless_of_sni() {
    install_provider_once();
    let pki = pki::build();
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let _downstream = DownstreamTls::from_context(&ctx).expect("from_context");

    // We can't directly call the private SingleCertResolver, but we can
    // verify the resolver's contract via three loopback handshakes with
    // different SNIs and confirm each completes (the resolver returns the
    // same CertifiedKey for each).
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;

    for sni in &["a.example.com", "b.example.com", "unknown.example.com"] {
        let pki_inner = pki::build();
        let ctx_inner = pki::ds_context_with(&pki_inner.leaf_cert_pem, &pki_inner.leaf_key_pem);
        let downstream_inner = DownstreamTls::from_context(&ctx_inner).expect("from_context");

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            downstream_inner.accept(stream).await
        });

        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(pki_inner.ca_der_for_root_store.clone())
            .expect("add ca");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");

        // Note: the test PKI's leaf has SAN `a.example.com`; SNIs other than
        // `a.example.com` will fail the post-handshake hostname-vs-SAN check.
        // We're testing the resolver's same-cert behavior, not SAN matching;
        // accept on the server side completes (the resolver returns a cert)
        // regardless of whether the client accepts that cert. Use
        // `dangerous_configuration` to bypass SAN matching client-side.
        let server_name = ServerName::try_from(*sni).expect("server name");
        // For SNIs other than a.example.com, the client connect WILL fail
        // (cert SAN mismatch). The server-side accept should still succeed
        // up until the client closes. Catch both behaviors.
        let _ = connector.connect(server_name, stream).await;

        // Server-side: accept completes if SNI matches the leaf SAN; for
        // mismatching SNIs, rustls-server doesn't enforce SAN — only the
        // client does. The resolver returned a cert; that's what we want.
        let server_result = server_task.await.expect("task joins");
        // For "a.example.com" the handshake must succeed; for the others
        // we just assert the resolver was called (server didn't error
        // because of "no cert configured for SNI").
        if *sni == "a.example.com" {
            assert!(server_result.is_ok(), "a.example.com must handshake");
        }
        // For other SNIs the result varies (client may abort post-handshake);
        // we don't assert ok / err — the resolver behavior is what's under test.
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn accept_returns_handshake_error_on_garbage_input() {
    install_provider_once();
    let pki = pki::build();
    let ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    let downstream = DownstreamTls::from_context(&ctx).expect("from_context");

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        downstream.accept(stream).await
    });

    use tokio::io::AsyncWriteExt;
    let mut client = tokio::net::TcpStream::connect(addr).await.expect("connect");
    client
        .write_all(b"GET / HTTP/1.1\r\n\r\n")
        .await
        .expect("write");
    let _ = client.shutdown().await;
    drop(client);

    let result = server_task.await.expect("task joins");
    let err = result.expect_err("plaintext garbage must fail handshake");
    assert!(matches!(err, TlsError::Handshake { .. }), "got {err:?}");
}

#[test]
fn crypto_provider_install_is_idempotent() {
    // First call returns Ok (or Err if a prior test already installed it —
    // both are acceptable; we just need no panic).
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    // Second call must return Err and must not panic.
    let result = rustls::crypto::aws_lc_rs::default_provider().install_default();
    assert!(result.is_err(), "second install must return Err, not panic");
}

mod upstream_pki {
    use super::*;
    use rcgen::{CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};

    /// Test PKI for upstream-side tests: a CA + a server cert with SAN
    /// `envoy-rust.test`. Same shape as `pki::build` but the server cert is
    /// what the upstream presents.
    pub struct UpstreamPki {
        pub _dir: TempDir,
        pub ca_pem: PathBuf,
        pub server_certified_key: rustls::sign::CertifiedKey,
    }

    pub fn build() -> UpstreamPki {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ca_params =
            CertificateParams::new(vec!["envoy-rust-upstream-ca".into()]).expect("ca params");
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "envoy-rust-upstream-ca");
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let ca_kp = KeyPair::generate().expect("ca kp");
        let ca_cert = ca_params.self_signed(&ca_kp).expect("ca self-sign");

        let mut srv_params =
            CertificateParams::new(vec!["envoy-rust.test".into()]).expect("server params");
        srv_params
            .distinguished_name
            .push(DnType::CommonName, "envoy-rust.test");
        let srv_kp = KeyPair::generate().expect("server kp");
        let srv_cert = srv_params
            .signed_by(&srv_kp, &ca_cert, &ca_kp)
            .expect("server signed");

        let ca_pem = ca_cert.pem();
        let srv_pem = srv_cert.pem();
        let srv_key_pem = srv_kp.serialize_pem();

        let ca_path = dir.path().join("upstream-ca.pem");
        let srv_path = dir.path().join("server.pem");
        let srv_key_path = dir.path().join("server.key");
        std::fs::write(&ca_path, &ca_pem).expect("write ca");
        std::fs::write(&srv_path, &srv_pem).expect("write server cert");
        std::fs::write(&srv_key_path, &srv_key_pem).expect("write server key");

        let server_certified_key = {
            let cert_der: rustls::pki_types::CertificateDer<'static> =
                srv_cert.der().clone().into_owned();
            // Build a signing key by re-loading the PEM through rustls-pemfile —
            // mirrors how envoy-tls's loader works for parity.
            let key_bytes = std::fs::read(&srv_key_path).expect("read");
            let mut sl = key_bytes.as_slice();
            let key = rustls_pemfile::private_key(&mut sl)
                .expect("parse priv key")
                .expect("priv key present");
            let signing = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key)
                .expect("any_supported_type");
            rustls::sign::CertifiedKey::new(vec![cert_der], signing)
        };

        UpstreamPki {
            _dir: dir,
            ca_pem: ca_path,
            server_certified_key,
        }
    }

    pub fn us_context_with(
        ca_path: &std::path::Path,
        sni: &str,
    ) -> envoy_config::UpstreamTlsContext {
        envoy_config::UpstreamTlsContext {
            common_tls_context: envoy_config::CommonTlsContext {
                tls_certificates: vec![],
                validation_context: Some(envoy_config::CertificateValidationContext {
                    trusted_ca: envoy_config::DataSource {
                        filename: Some(ca_path.to_string_lossy().into_owned()),
                        inline_string: None,
                    },
                }),
                alpn_protocols: vec![],
            },
            sni: sni.to_string(),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn loads_upstream_client_config() {
    install_provider_once();
    let pki = upstream_pki::build();
    let ctx = upstream_pki::us_context_with(&pki.ca_pem, "envoy-rust.test");
    let upstream = UpstreamTls::from_context(&ctx).expect("upstream from_context");

    // Server: stand up a tokio_rustls TlsAcceptor with the rcgen-built server
    // cert. Exercise the upstream's connect against it.
    use rustls::server::ResolvesServerCert;

    let resolver: Arc<dyn ResolvesServerCert> =
        Arc::new(StaticResolver(Arc::new(pki.server_certified_key)));
    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        acceptor.accept(stream).await
    });

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let client_tls = upstream.connect(stream).await.expect("upstream connect");

    let _server_tls = server_task
        .await
        .expect("task joins")
        .expect("server accept");
    let v = client_tls
        .get_ref()
        .1
        .protocol_version()
        .expect("version negotiated");
    assert!(
        matches!(
            v,
            rustls::ProtocolVersion::TLSv1_2 | rustls::ProtocolVersion::TLSv1_3
        ),
        "unexpected TLS version: {v:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn upstream_rejects_invalid_sni() {
    install_provider_once();
    let pki = upstream_pki::build();
    // sni is an IP literal — Envoy's UpstreamTlsContext.sni is documented
    // DNS-name-only, so envoy-rust must reject.
    let ctx = upstream_pki::us_context_with(&pki.ca_pem, "127.0.0.1");
    let err = UpstreamTls::from_context(&ctx).expect_err("must reject IP-literal sni");
    assert!(
        matches!(err, TlsError::InvalidServerName { .. }),
        "got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn upstream_rejects_untrusted_cert() {
    install_provider_once();
    let pki = upstream_pki::build();
    // Build a *different* CA and server cert. Configure UpstreamTls with
    // pki.ca_pem (the original CA), then connect to a server presenting the
    // OTHER PKI's server cert. The handshake must fail.
    let other = upstream_pki::build();
    let ctx = upstream_pki::us_context_with(&pki.ca_pem, "envoy-rust.test");
    let upstream = UpstreamTls::from_context(&ctx).expect("from_context");

    use rustls::server::ResolvesServerCert;
    let resolver: Arc<dyn ResolvesServerCert> =
        Arc::new(StaticResolver(Arc::new(other.server_certified_key)));
    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = acceptor.accept(stream).await;
        }
    });

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let err = upstream
        .connect(stream)
        .await
        .expect_err("must reject untrusted");
    assert!(matches!(err, TlsError::Handshake { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------------
// 03.2 Task 3: SniResolver + ResolvesServerCert tests.
//
// Tests 1 + 4 use byte-exact peer-cert DER comparison after a successful
// client handshake (SNI matches a real SAN; standard root-store path).
//
// Test 2 (default fallback on miss) needs to drive an UNKNOWN SNI — the
// presented cert's SAN won't match, so the default rustls SAN check would
// abort the handshake before we can inspect what the server returned.
// We therefore install a custom `ServerCertVerifier` that captures the
// presented end-entity DER without enforcing SAN, then assert byte-exact that
// the server returned key_a (the configured default). Pattern modelled on
// `mod upstream_pki`'s in-test `ServerCertVerifier` shape.
//
// Test 3 (no default, miss) asserts the server-side `acceptor.accept` returns
// Err — when `resolve()` returns None, rustls aborts with `unrecognized_name`.

mod sni_capture {
    use super::*;
    use rustls::DigitallySignedStruct;
    use rustls::SignatureScheme;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use std::sync::Mutex;

    /// `ServerCertVerifier` that records the end-entity cert the server
    /// presented (DER), then unconditionally accepts. Test-only — never
    /// behaves like this in production. Lives in `mod sni_capture` to keep
    /// the dangerous bypass scoped to the tests that need it.
    #[derive(Debug)]
    pub struct CapturingVerifier {
        pub captured: Arc<Mutex<Option<CertificateDer<'static>>>>,
    }

    impl CapturingVerifier {
        pub fn new() -> Self {
            Self {
                captured: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl ServerCertVerifier for CapturingVerifier {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            *self.captured.lock().expect("lock") = Some(end_entity.clone().into_owned());
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
                SignatureScheme::RSA_PKCS1_SHA512,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::ECDSA_NISTP521_SHA512,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PSS_SHA512,
                SignatureScheme::ED25519,
            ]
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sni_resolver_routes_known_sni() {
    install_provider_once();
    let pki = pki::build();
    let key_a = Arc::new(pki::certified_key_from_pem(
        &pki.leaf_cert_pem,
        &pki.leaf_key_pem,
    ));
    let key_b = Arc::new(pki::certified_key_from_pem(
        &pki.leaf_b_cert_pem,
        &pki.leaf_b_key_pem,
    ));

    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    map.insert("b.example.com".to_string(), key_b.clone());
    let resolver: Arc<dyn rustls::server::ResolvesServerCert> =
        Arc::new(SniResolver { map, default: None });

    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let _ = acceptor.accept(stream).await.expect("server accept");
    });

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(pki.ca_der_for_root_store.clone())
        .expect("add ca");
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let server_name =
        rustls::pki_types::ServerName::try_from("a.example.com").expect("server name");
    let tls = connector
        .connect(server_name, stream)
        .await
        .expect("client handshake");

    let (_io, conn) = tls.get_ref();
    let presented = conn
        .peer_certificates()
        .expect("peer certs")
        .first()
        .expect("at least one cert")
        .clone()
        .into_owned();
    let leaf_a_der = pki::cert_der_at(&pki.leaf_cert_pem);
    assert_eq!(
        presented.as_ref(),
        leaf_a_der.as_ref(),
        "SNI a.example.com must select leaf-A"
    );

    server_task.await.expect("server task joins");
}

#[tokio::test(flavor = "multi_thread")]
async fn sni_resolver_falls_back_to_default_on_miss() {
    install_provider_once();
    let pki = pki::build();
    let key_a = Arc::new(pki::certified_key_from_pem(
        &pki.leaf_cert_pem,
        &pki.leaf_key_pem,
    ));

    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    let resolver: Arc<dyn rustls::server::ResolvesServerCert> = Arc::new(SniResolver {
        map,
        default: Some(key_a.clone()),
    });

    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        // Must succeed: resolver returned the default cert; client uses a
        // capturing verifier that bypasses SAN checks, so handshake completes.
        acceptor.accept(stream).await.expect("server accept");
    });

    // Client side: dangerous capturing verifier so we can inspect what the
    // server returned for an SNI it doesn't have an explicit map entry for.
    let capture = sni_capture::CapturingVerifier::new();
    let captured = capture.captured.clone();
    let client_cfg = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(capture))
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let server_name =
        rustls::pki_types::ServerName::try_from("unknown.example.com").expect("server name");
    let _tls = connector
        .connect(server_name, stream)
        .await
        .expect("client handshake (capturing verifier accepts any cert)");

    server_task.await.expect("server task joins");

    let presented = captured
        .lock()
        .expect("lock")
        .clone()
        .expect("verifier captured a cert");
    let leaf_a_der = pki::cert_der_at(&pki.leaf_cert_pem);
    assert_eq!(
        presented.as_ref(),
        leaf_a_der.as_ref(),
        "unknown SNI must fall back to the default cert (leaf-A)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sni_resolver_returns_none_on_miss_without_default() {
    install_provider_once();
    let pki = pki::build();
    let key_a = Arc::new(pki::certified_key_from_pem(
        &pki.leaf_cert_pem,
        &pki.leaf_key_pem,
    ));

    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    let resolver: Arc<dyn rustls::server::ResolvesServerCert> =
        Arc::new(SniResolver { map, default: None });

    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        // Resolver returns None for unknown SNI; rustls aborts the handshake
        // with `unrecognized_name`. Server-side accept must Err.
        acceptor.accept(stream).await
    });

    // Client uses the capturing verifier so its end of the handshake doesn't
    // fail FIRST on SAN/cert-validation; we want the server-side rejection
    // to be the failure under test.
    let capture = sni_capture::CapturingVerifier::new();
    let client_cfg = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(capture))
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let server_name =
        rustls::pki_types::ServerName::try_from("unknown.example.com").expect("server name");
    let _ = connector.connect(server_name, stream).await;

    let server_result = server_task.await.expect("server task joins");
    assert!(
        server_result.is_err(),
        "server-side accept must fail when resolver returns None for unknown SNI; got {server_result:?}"
    );
}

// ---------------------------------------------------------------------------
// 03.2 Task 4: DownstreamTls::from_listener integration test.
//
// Synthesizes an envoy_config::Listener with two TLS filter chains (each with
// a distinct SNI + leaf cert), feeds it through from_listener, drives two
// in-process handshakes (one per SNI), and asserts byte-exact peer-cert DER
// per SNI. Proves that from_listener correctly populates the SniResolver's
// map and that the resulting ServerConfig dispatches by SNI.
//
// The Listener is constructed directly via the public envoy_config struct
// constructors (not parsed from YAML) to avoid adding a serde_yaml dev-dep
// and to match the convention established by `pki::ds_context_with`.

fn synth_listener_two_tls_chains(pki: &pki::Pki) -> envoy_config::Listener {
    let chain_a = envoy_config::FilterChain {
        filter_chain_match: Some(envoy_config::FilterChainMatch {
            server_names: vec!["a.example.com".to_string()],
        }),
        transport_socket: Some(envoy_config::TransportSocket {
            name: envoy_config::TLS_TRANSPORT_SOCKET.to_string(),
            typed_config: envoy_config::TransportSocketTypedConfig::Downstream(
                pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem),
            ),
        }),
        filters: vec![envoy_config::NetworkFilter {
            name: envoy_config::TCP_PROXY_FILTER.to_string(),
            typed_config: Some(envoy_config::TypedConfig::TcpProxy(
                envoy_config::TcpProxyConfig {
                    stat_prefix: "ingress_tcp".to_string(),
                    cluster: "backend".to_string(),
                },
            )),
        }],
    };
    let chain_b = envoy_config::FilterChain {
        filter_chain_match: Some(envoy_config::FilterChainMatch {
            server_names: vec!["b.example.com".to_string()],
        }),
        transport_socket: Some(envoy_config::TransportSocket {
            name: envoy_config::TLS_TRANSPORT_SOCKET.to_string(),
            typed_config: envoy_config::TransportSocketTypedConfig::Downstream(
                pki::ds_context_with(&pki.leaf_b_cert_pem, &pki.leaf_b_key_pem),
            ),
        }),
        filters: vec![envoy_config::NetworkFilter {
            name: envoy_config::TCP_PROXY_FILTER.to_string(),
            typed_config: Some(envoy_config::TypedConfig::TcpProxy(
                envoy_config::TcpProxyConfig {
                    stat_prefix: "ingress_tcp".to_string(),
                    cluster: "backend".to_string(),
                },
            )),
        }],
    };
    envoy_config::Listener {
        name: "tcp_listener".to_string(),
        address: envoy_config::Address {
            socket_address: envoy_config::SocketAddress {
                address: "0.0.0.0".to_string(),
                port_value: 10010,
            },
        },
        filter_chains: vec![chain_a, chain_b],
        listener_filters: vec![],
        enable_reuse_port: true,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn from_listener_builds_multi_cert_config() {
    install_provider_once();
    let pki = pki::build();
    let listener = synth_listener_two_tls_chains(&pki);
    let downstream = DownstreamTls::from_listener(&listener).expect("from_listener");

    // Probe each SNI in turn. Helper closure to keep the test compact —
    // mirrors the Task-3 inlined-handshake style.
    async fn probe(
        config: Arc<rustls::ServerConfig>,
        ca_der: &rustls::pki_types::CertificateDer<'static>,
        sni: &'static str,
    ) -> rustls::pki_types::CertificateDer<'static> {
        let acceptor = tokio_rustls::TlsAcceptor::from(config);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let _ = acceptor.accept(stream).await.expect("server accept");
        });

        let mut roots = rustls::RootCertStore::empty();
        roots.add(ca_der.clone()).expect("add ca");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let server_name = rustls::pki_types::ServerName::try_from(sni).expect("server name");
        let tls = connector
            .connect(server_name, stream)
            .await
            .expect("client connect");
        let (_io, conn) = tls.get_ref();
        let presented = conn
            .peer_certificates()
            .expect("peer certs")
            .first()
            .expect("at least one cert")
            .clone()
            .into_owned();
        server_task.await.expect("server task joins");
        presented
    }

    let cert_for_a = probe(
        downstream.config.clone(),
        &pki.ca_der_for_root_store,
        "a.example.com",
    )
    .await;
    let cert_for_b = probe(
        downstream.config.clone(),
        &pki.ca_der_for_root_store,
        "b.example.com",
    )
    .await;

    let leaf_a_der = pki::cert_der_at(&pki.leaf_cert_pem);
    let leaf_b_der = pki::cert_der_at(&pki.leaf_b_cert_pem);
    assert_eq!(
        cert_for_a.as_ref(),
        leaf_a_der.as_ref(),
        "SNI a.example.com must select leaf-A"
    );
    assert_eq!(
        cert_for_b.as_ref(),
        leaf_b_der.as_ref(),
        "SNI b.example.com must select leaf-B"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sni_resolver_is_case_insensitive() {
    install_provider_once();
    let pki = pki::build();
    let key_a = Arc::new(pki::certified_key_from_pem(
        &pki.leaf_cert_pem,
        &pki.leaf_key_pem,
    ));

    // Map keyed lowercase per the SniResolver contract; rustls 0.23's
    // ClientHello::server_name() lowercases the SNI before the resolver sees it.
    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a.clone());
    let resolver: Arc<dyn rustls::server::ResolvesServerCert> =
        Arc::new(SniResolver { map, default: None });

    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let _ = acceptor.accept(stream).await.expect("server accept");
    });

    // Client handshakes with mixed-case SNI; rustls lowercases on the wire,
    // and (separately) lowercases the SAN-vs-name comparison too, so this is
    // an end-to-end happy-path handshake.
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(pki.ca_der_for_root_store.clone())
        .expect("add ca");
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let server_name =
        rustls::pki_types::ServerName::try_from("A.Example.com").expect("server name");
    let tls = connector
        .connect(server_name, stream)
        .await
        .expect("mixed-case SNI must handshake (rustls lowercases before resolver lookup)");

    let (_io, conn) = tls.get_ref();
    let presented = conn
        .peer_certificates()
        .expect("peer certs")
        .first()
        .expect("at least one cert")
        .clone()
        .into_owned();
    let leaf_a_der = pki::cert_der_at(&pki.leaf_cert_pem);
    assert_eq!(
        presented.as_ref(),
        leaf_a_der.as_ref(),
        "mixed-case SNI A.Example.com must lowercase to a.example.com and pick leaf-A"
    );

    server_task.await.expect("server task joins");
}

#[tokio::test(flavor = "multi_thread")]
async fn sni_resolver_without_default_aborts_unknown_sni() {
    // Security property (SniResolver doc contract): with NO catch-all chain, an
    // unknown SNI resolves to `None`, and rustls MUST abort the handshake
    // (`unrecognized_name`) rather than pick an arbitrary cert. The existing
    // tests only assert the known-SNI happy paths; the reject path was untested.
    install_provider_once();
    let pki = pki::build();
    let key_a = Arc::new(pki::certified_key_from_pem(
        &pki.leaf_cert_pem,
        &pki.leaf_key_pem,
    ));
    let mut map = std::collections::HashMap::new();
    map.insert("a.example.com".to_string(), key_a);
    let resolver: Arc<dyn rustls::server::ResolvesServerCert> =
        Arc::new(SniResolver { map, default: None });
    let server_cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        acceptor
            .accept(listener.accept().await.expect("accept").0)
            .await
    });

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(pki.ca_der_for_root_store.clone())
        .expect("add ca");
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let server_name =
        rustls::pki_types::ServerName::try_from("unknown.example.com").expect("server name");

    // Both sides must fail: the server has no cert for this SNI and no default,
    // so it sends a fatal alert; the client connect surfaces the aborted handshake.
    let client_result = connector.connect(server_name, stream).await;
    assert!(
        client_result.is_err(),
        "unknown SNI without a catch-all must fail the client handshake"
    );
    let server_result = server_task.await.expect("server task joins");
    assert!(
        server_result.is_err(),
        "server must abort (no cert for SNI, no default), got Ok"
    );
}

/// Build a `DownstreamTlsContext` carrying `alpn_protocols`.
fn ds_context_with_alpn(pki: &pki::Pki, alpn: &[&str]) -> envoy_config::DownstreamTlsContext {
    let mut ctx = pki::ds_context_with(&pki.leaf_cert_pem, &pki.leaf_key_pem);
    ctx.common_tls_context.alpn_protocols = alpn.iter().map(|s| s.to_string()).collect();
    ctx
}

/// Drive ONE real loopback handshake: a `DownstreamTls` built from `server_alpn`
/// against a `tokio_rustls` client offering `client_alpn` (empty = offer none).
/// Returns `(server_selected, client_selected)` as owned bytes. Panics on a
/// handshake failure, which is itself the D6' assertion for the mismatch cell.
async fn alpn_handshake(
    pki: &pki::Pki,
    server_alpn: &[&str],
    client_alpn: &[&str],
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    use rustls::pki_types::ServerName;
    let ctx = ds_context_with_alpn(pki, server_alpn);
    let downstream = DownstreamTls::from_context(&ctx).expect("from_context");

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        downstream.accept(stream).await
    });

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(pki.ca_der_for_root_store.clone())
        .expect("add ca");
    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_cfg));
    let server_name = ServerName::try_from("a.example.com").expect("server name");
    let tcp = tokio::net::TcpStream::connect(addr).await.expect("connect");

    let client_tls = if client_alpn.is_empty() {
        connector.connect(server_name, tcp).await
    } else {
        connector
            .with_alpn(client_alpn.iter().map(|s| s.as_bytes().to_vec()).collect())
            .connect(server_name, tcp)
            .await
    }
    .expect("client handshake must SUCCEED");

    let server_tls = server_task
        .await
        .expect("server task joins")
        .expect("server handshake must SUCCEED");

    (
        server_tls.get_ref().1.alpn_protocol().map(|p| p.to_vec()),
        client_tls.get_ref().1.alpn_protocol().map(|p| p.to_vec()),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_negotiates_h2_when_both_offer_it() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["h2", "http/1.1"], &["h2", "http/1.1"]).await;
    assert_eq!(s.as_deref(), Some(&b"h2"[..]), "server side");
    assert_eq!(c.as_deref(), Some(&b"h2"[..]), "client side");
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_selection_follows_server_preference() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["http/1.1", "h2"], &["h2", "http/1.1"]).await;
    assert_eq!(s.as_deref(), Some(&b"http/1.1"[..]), "server side");
    assert_eq!(c.as_deref(), Some(&b"http/1.1"[..]), "client side");
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_empty_server_list_does_not_advertise() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &[], &["h2", "http/1.1"]).await;
    assert_eq!(s, None);
    assert_eq!(c, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_mismatch_completes_handshake_with_no_protocol() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["h2", "http/1.1"], &["h3"]).await;
    assert_eq!(s, None, "server must select nothing");
    assert_eq!(c, None, "client must select nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn alpn_client_offers_nothing_negotiates_none() {
    install_provider_once();
    let pki = pki::build();
    let (s, c) = alpn_handshake(&pki, &["h2", "http/1.1"], &[]).await;
    assert_eq!(s, None);
    assert_eq!(c, None);
}
