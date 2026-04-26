#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint. See `docs/envoy-rust/DECISIONS.md`
//! ADR-0008 for the extraction rationale.

pub mod bootstrap;

pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType,
    CommonTlsContext, DataSource, DownstreamTlsContext, Endpoint, FilterChain, FilterChainMatch,
    LbEndpoint, LbPolicy, Listener, LoadAssignment, LocalityLbEndpoints, NetworkFilter, Node,
    SocketAddress, StaticResources, TcpProxyConfig, TlsCertificate, TransportSocket,
    TransportSocketTypedConfig, TypedConfig, UpstreamTlsContext,
};

/// The only network filter name envoy-rust recognizes in phase 01.
pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

/// The TCP-proxy network filter name. envoy-rust accepts it as of phase 02.1;
/// runtime dispatch lands in phase 02.2. See ADR-0014.
pub const TCP_PROXY_FILTER: &str = "envoy.filters.network.tcp_proxy";

/// The only transport-socket name envoy-rust accepts in phase 03. Future phases
/// may add `envoy.transport_sockets.raw_buffer` / `envoy.transport_sockets.quic`.
pub const TLS_TRANSPORT_SOCKET: &str = "envoy.transport_sockets.tls";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("parsing bootstrap YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error(
        "bootstrap configures neither an admin endpoint nor a listener; envoy-rust has nothing to do"
    )]
    NoRuntime,
    #[error("bootstrap has {0} listeners; phase 01 supports at most one")]
    TooManyListeners(usize),
    #[error("unsupported network filter '{0}'; envoy-rust accepts only '{1}'")]
    UnsupportedFilter(String, &'static str),
    #[error("filter '{0}' requires typed_config")]
    MissingTypedConfig(&'static str),
    #[error("filter '{0}' must not carry typed_config")]
    UnexpectedTypedConfig(&'static str),
    #[error("tcp_proxy filter references unknown cluster '{0}'")]
    UnknownCluster(String),
    #[error(
        "cluster '{cluster}' declares load_assignment.cluster_name '{assignment}'; these must match"
    )]
    LoadAssignmentNameMismatch { cluster: String, assignment: String },
    #[error("cluster '{0}' has zero lb_endpoints; ≥1 required")]
    EmptyClusterEndpoints(String),
    #[error(
        "unsupported transport_socket name '{0}'; envoy-rust accepts only 'envoy.transport_sockets.tls'"
    )]
    UnknownTransportSocketName(String),
    #[error(
        "transport_socket on the {side} side is the wrong direction (got '{got}'); listener requires DownstreamTlsContext, cluster requires UpstreamTlsContext"
    )]
    MismatchedTransportSocketDirection {
        side: &'static str,
        got: &'static str,
    },
    #[error(
        "tls_certificates on the {side} side has the wrong cardinality; listener requires ≥1, cluster requires 0 (mTLS deferred)"
    )]
    EmptyTlsCertificates { side: &'static str },
    #[error(
        "UpstreamTlsContext requires validation_context.trusted_ca; phase 03 has no insecure-skip surface"
    )]
    MissingValidationContext,
    #[error("UpstreamTlsContext.sni must be a non-empty DNS name")]
    EmptyUpstreamSni,
}

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    bootstrap::validate(&bootstrap)?;
    Ok(bootstrap)
}
