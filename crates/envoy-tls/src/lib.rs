#![forbid(unsafe_code)]

//! Phase 03.1 TLS surface for envoy-rust. Owns rustls server/client config
//! construction, the cert/key PEM loader, and the `TlsError` typed-error enum.
//!
//! D-3.2 + ADR-0018 + ADR-0019: this is the only crate in the workspace that
//! depends on rustls / tokio-rustls / rustls-pki-types / rustls-pemfile /
//! aws-lc-rs. envoy-listener and envoy-cluster stay rustls-free.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::ServerConfig;
use rustls::pki_types::CertificateDer;
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use tokio::net::TcpStream;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("loading cert/key file {path:?}: {source}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parsing PEM at {path:?}: no leaf certificate found")]
    CertParse { path: PathBuf },
    #[error("parsing private key at {0:?}: {1}")]
    KeyParse(PathBuf, String),
    #[error("rustls config build: {0}")]
    RustlsConfig(String),
    #[error("invalid SNI {sni:?} in upstream context: {reason}")]
    InvalidServerName { sni: String, reason: String },
    #[error("TLS handshake: {source}")]
    Handshake {
        #[source]
        source: std::io::Error,
    },
    #[error("loading trusted_ca PEM at {path:?}: no CA certificate found")]
    CaParse { path: PathBuf },
    #[error("downstream context requires at least one tls_certificate")]
    DownstreamRequiresCert,
}

/// Server-side TLS configuration. Build via `from_context`; drive a connected
/// `TcpStream` through the rustls server handshake via `accept`.
#[derive(Debug)]
pub struct DownstreamTls {
    config: Arc<ServerConfig>,
}

impl DownstreamTls {
    /// Build from a parsed envoy_config::DownstreamTlsContext.
    ///
    /// 03.1: single-cert path. Loads cert+key PEMs from the configured filenames;
    /// constructs a `SingleCertResolver` wrapping the resulting `CertifiedKey`.
    /// Rejects empty `tls_certificates` with `TlsError::DownstreamRequiresCert`.
    pub fn from_context(cfg: &envoy_config::DownstreamTlsContext) -> Result<Self, TlsError> {
        let certs = &cfg.common_tls_context.tls_certificates;
        if certs.is_empty() {
            return Err(TlsError::DownstreamRequiresCert);
        }
        // 03.1 honors the first tls_certificate only. The validator rejects
        // the empty case; multi-cert SNI selection lands in 03.2 via
        // `from_listener`.
        let cert_path = Path::new(&certs[0].certificate_chain.filename);
        let key_path = Path::new(&certs[0].private_key.filename);
        let key = load_certified_key(cert_path, key_path)?;
        let resolver: Arc<dyn ResolvesServerCert> = Arc::new(SingleCertResolver(Arc::new(key)));
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver);
        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// Hands a connected downstream `TcpStream` through the rustls server
    /// handshake; returns the post-handshake stream. On handshake failure
    /// returns `TlsError::Handshake`; the listener's accept loop logs at
    /// `warn!` and drops the connection per phase 02.2's posture.
    pub async fn accept(
        &self,
        downstream: TcpStream,
    ) -> Result<tokio_rustls::server::TlsStream<TcpStream>, TlsError> {
        let acceptor = tokio_rustls::TlsAcceptor::from(self.config.clone());
        acceptor
            .accept(downstream)
            .await
            .map_err(|source| TlsError::Handshake { source })
    }
}

/// In-crate `ResolvesServerCert` that returns the wrapped `CertifiedKey` for
/// any ClientHello regardless of SNI. The `ServerConfig` is built via
/// `with_cert_resolver` (rather than the simpler `with_single_cert`) so the
/// 03.2 SNI multi-cert resolver is a drop-in replacement.
#[derive(Debug)]
struct SingleCertResolver(Arc<CertifiedKey>);

impl ResolvesServerCert for SingleCertResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

/// Load + verify a PEM cert chain + PEM private key from disk; return the
/// rustls-signing-key-bearing `CertifiedKey`.
fn load_certified_key(cert_path: &Path, key_path: &Path) -> Result<CertifiedKey, TlsError> {
    let cert_bytes = std::fs::read(cert_path).map_err(|source| TlsError::FileRead {
        path: cert_path.to_path_buf(),
        source,
    })?;
    let mut cert_slice = cert_bytes.as_slice();
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_slice)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::KeyParse(cert_path.to_path_buf(), format!("certs: {e}")))?;
    if certs.is_empty() {
        return Err(TlsError::CertParse {
            path: cert_path.to_path_buf(),
        });
    }

    let key_bytes = std::fs::read(key_path).map_err(|source| TlsError::FileRead {
        path: key_path.to_path_buf(),
        source,
    })?;
    let mut key_slice = key_bytes.as_slice();
    let key = rustls_pemfile::private_key(&mut key_slice)
        .map_err(|e| TlsError::KeyParse(key_path.to_path_buf(), format!("private_key: {e}")))?
        .ok_or_else(|| {
            TlsError::KeyParse(key_path.to_path_buf(), "no private key found".to_string())
        })?;

    let signing_key = rustls::crypto::aws_lc_rs::sign::any_supported_type(&key).map_err(|e| {
        TlsError::KeyParse(key_path.to_path_buf(), format!("any_supported_type: {e}"))
    })?;

    Ok(CertifiedKey::new(certs, signing_key))
}
