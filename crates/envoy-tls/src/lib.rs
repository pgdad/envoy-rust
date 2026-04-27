#![forbid(unsafe_code)]

//! Phase 03.1 TLS surface for envoy-rust. Owns rustls server/client config
//! construction, the cert/key PEM loader, and the `TlsError` typed-error enum.
//!
//! D-3.2 + ADR-0018 + ADR-0019: this is the only crate in the workspace that
//! depends on rustls / tokio-rustls / rustls-pki-types / rustls-pemfile /
//! aws-lc-rs. envoy-listener and envoy-cluster stay rustls-free.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use tokio::net::TcpStream;

/// Install the aws-lc-rs default crypto provider for this process.
///
/// rustls requires a single default crypto provider per process. This is
/// idempotent: the second-or-later call returns `Err(_)`, which the caller
/// should treat as a no-op (the provider is already installed). Tests use
/// the same idiom (`let _ = envoy_tls::install_default_crypto_provider();`).
///
/// Architectural note: this lives in envoy-tls so envoy-bin and other
/// consumers don't need a direct rustls dep. Per SPEC §3 D1 and ADR-0019,
/// envoy-tls is the only crate with rustls in its dep tree.
pub fn install_default_crypto_provider() -> Result<(), Arc<rustls::crypto::CryptoProvider>> {
    rustls::crypto::aws_lc_rs::default_provider().install_default()
}

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
        let cert_path = Path::new(
            certs[0]
                .certificate_chain
                .filename
                .as_deref()
                .expect("validator ensures TLS DataSource carries filename"),
        );
        let key_path = Path::new(
            certs[0]
                .private_key
                .filename
                .as_deref()
                .expect("validator ensures TLS DataSource carries filename"),
        );
        let key = load_certified_key(cert_path, key_path)?;
        let resolver: Arc<dyn ResolvesServerCert> = Arc::new(SingleCertResolver(Arc::new(key)));
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver);
        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// 03.2-only: build from a full `envoy_config::Listener` by walking all
    /// filter chains. For each chain that carries a `transport_socket` with a
    /// `DownstreamTlsContext`, load its cert+key into a `CertifiedKey`; for
    /// each SNI in the chain's `filter_chain_match.server_names`, insert the
    /// key into the `SniResolver`'s map (keyed lowercase). At most one chain
    /// may have an empty `server_names` (the catch-all); its key becomes the
    /// resolver's `default`.
    ///
    /// The validator already rejects overlapping `server_names`, multiple
    /// catch-all chains, and mixed TLS+plaintext listeners — `from_listener`
    /// trusts those guarantees.
    ///
    /// If any chain in the listener carries TLS, the entire listener is
    /// treated as TLS (rustls multiplexes by SNI inside a single
    /// `ServerConfig`).
    pub fn from_listener(listener: &envoy_config::Listener) -> Result<Self, TlsError> {
        let mut map: HashMap<String, Arc<CertifiedKey>> = HashMap::new();
        let mut default: Option<Arc<CertifiedKey>> = None;

        for chain in &listener.filter_chains {
            let Some(socket) = &chain.transport_socket else {
                continue;
            };
            let envoy_config::TransportSocketTypedConfig::Downstream(ctx) = &socket.typed_config
            else {
                // Validator rejects mismatched direction on listeners; defensive.
                continue;
            };

            let certs = &ctx.common_tls_context.tls_certificates;
            let cert = certs.first().ok_or(TlsError::DownstreamRequiresCert)?;
            let cert_path = Path::new(
                cert.certificate_chain
                    .filename
                    .as_deref()
                    .expect("validator ensures TLS DataSource carries filename"),
            );
            let key_path = Path::new(
                cert.private_key
                    .filename
                    .as_deref()
                    .expect("validator ensures TLS DataSource carries filename"),
            );
            let certified_key = Arc::new(load_certified_key(cert_path, key_path)?);

            let server_names = chain
                .filter_chain_match
                .as_ref()
                .map(|m| m.server_names.as_slice())
                .unwrap_or(&[]);

            if server_names.is_empty() {
                // Catch-all chain. Validator ensured at most one.
                default = Some(certified_key);
            } else {
                for sni in server_names {
                    map.insert(sni.to_lowercase(), certified_key.clone());
                }
            }
        }

        let resolver: Arc<dyn ResolvesServerCert> = Arc::new(SniResolver { map, default });
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

/// SNI-keyed `ResolvesServerCert`. Map keys are lowercase per parent-SPEC §6
/// signpost 21 (rustls 0.23's `ClientHello::server_name()` returns lowercase).
/// The validator (`envoy_config::ConfigError::MultipleListenersWithOverlappingSni`)
/// rejects overlapping SNIs at config-load time, so this resolver assumes
/// well-formed input.
pub struct SniResolver {
    pub map: std::collections::HashMap<String, std::sync::Arc<rustls::sign::CertifiedKey>>,
    /// Catch-all chain's certified key. None when the listener has no
    /// catch-all chain — unknown SNIs then return None and rustls aborts the
    /// handshake with `unrecognized_name`.
    pub default: Option<std::sync::Arc<rustls::sign::CertifiedKey>>,
}

impl std::fmt::Debug for SniResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SniResolver")
            .field("snis", &self.map.keys().collect::<Vec<_>>())
            .field("has_default", &self.default.is_some())
            .finish()
    }
}

impl rustls::server::ResolvesServerCert for SniResolver {
    fn resolve(
        &self,
        client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<std::sync::Arc<rustls::sign::CertifiedKey>> {
        let sni = client_hello.server_name()?;
        // rustls 0.23 returns lowercase already; we store lowercase; .get() is direct.
        self.map.get(sni).cloned().or_else(|| self.default.clone())
    }
}

/// Client-side TLS configuration. Build via `from_context`; drive a connected
/// upstream `TcpStream` through the rustls client handshake via `connect`.
///
/// 03.1 ships the implementation + unit tests; 03.2 wires consumers
/// (envoy-tcp's `TcpProxy::handle` gains an `Option<Arc<UpstreamTls>>` field;
/// envoy-bin builds the `Arc<UpstreamTls>` per cluster with
/// `transport_socket: Upstream(...)`).
#[derive(Debug)]
pub struct UpstreamTls {
    config: Arc<ClientConfig>,
    server_name: ServerName<'static>,
}

impl UpstreamTls {
    /// Build from a parsed `envoy_config::UpstreamTlsContext`. Loads the CA
    /// PEM from `validation_context.trusted_ca.filename` into a `RootCertStore`;
    /// builds a `ClientConfig` with that root store, no client auth (mTLS
    /// deferred), default cipher suites/protocols. Parses `cfg.sni` into a
    /// `ServerName::DnsName` via `rustls-pki-types`; rejects IP literals
    /// (Envoy's `UpstreamTlsContext.sni` is documented DNS-name-only).
    pub fn from_context(cfg: &envoy_config::UpstreamTlsContext) -> Result<Self, TlsError> {
        let ca_path_str = cfg
            .common_tls_context
            .validation_context
            .as_ref()
            .map(|vc| {
                vc.trusted_ca
                    .filename
                    .as_deref()
                    .expect("validator ensures TLS DataSource carries filename")
            })
            .ok_or_else(|| {
                TlsError::RustlsConfig(
                    "UpstreamTls::from_context: validation_context required".to_string(),
                )
            })?;
        let ca_path = Path::new(ca_path_str);
        let roots = load_root_store(ca_path)?;
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        let server_name = parse_dns_server_name(&cfg.sni)?;
        Ok(Self {
            config: Arc::new(config),
            server_name,
        })
    }

    /// Hands a connected upstream `TcpStream` through the rustls client
    /// handshake; returns the post-handshake stream.
    pub async fn connect(
        &self,
        upstream: TcpStream,
    ) -> Result<tokio_rustls::client::TlsStream<TcpStream>, TlsError> {
        let connector = tokio_rustls::TlsConnector::from(self.config.clone());
        connector
            .connect(self.server_name.clone(), upstream)
            .await
            .map_err(|source| TlsError::Handshake { source })
    }
}

fn load_root_store(ca_path: &Path) -> Result<RootCertStore, TlsError> {
    let bytes = std::fs::read(ca_path).map_err(|source| TlsError::FileRead {
        path: ca_path.to_path_buf(),
        source,
    })?;
    let mut slice = bytes.as_slice();
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut slice)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| TlsError::CaParse {
            path: ca_path.to_path_buf(),
        })?;
    if certs.is_empty() {
        return Err(TlsError::CaParse {
            path: ca_path.to_path_buf(),
        });
    }
    let mut roots = RootCertStore::empty();
    for cert in certs {
        roots
            .add(cert)
            .map_err(|e| TlsError::RustlsConfig(format!("RootCertStore::add: {e}")))?;
    }
    Ok(roots)
}

fn parse_dns_server_name(sni: &str) -> Result<ServerName<'static>, TlsError> {
    use std::convert::TryFrom;
    let parsed = ServerName::try_from(sni).map_err(|e| TlsError::InvalidServerName {
        sni: sni.to_string(),
        reason: format!("parse: {e}"),
    })?;
    match parsed {
        ServerName::DnsName(name) => Ok(ServerName::DnsName(name.to_owned())),
        ServerName::IpAddress(_) => Err(TlsError::InvalidServerName {
            sni: sni.to_string(),
            reason: "IP literals not accepted in upstream sni; Envoy requires a DNS name".into(),
        }),
        // ServerName is non-exhaustive in some pki-types versions; default
        // any future variant to rejection.
        _ => Err(TlsError::InvalidServerName {
            sni: sni.to_string(),
            reason: "unsupported ServerName variant".into(),
        }),
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
