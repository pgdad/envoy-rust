#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint. See `docs/envoy-rust/DECISIONS.md`
//! ADR-0008 for the extraction rationale.

pub mod bootstrap;
pub mod matcher;

pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType, CodecType,
    CommonTlsContext, DataSource, DirectResponse, DownstreamTlsContext, Endpoint, FilterChain,
    FilterChainMatch, HeaderMatcher, HeaderMatcherMode, HttpConnectionManagerConfig, HttpFilter,
    HttpFilterTypedConfig, Int64Range, LbEndpoint, LbPolicy, Listener, LoadAssignment,
    LocalityLbEndpoints, NetworkFilter, Node, Route, RouteConfiguration, RouteMatch, RouterConfig,
    SafeRegex, SocketAddress, StaticResources, StringMatcher, StringMatcherMode, TcpProxyConfig,
    TlsCertificate, TransportSocket, TransportSocketTypedConfig, TypedConfig, UpstreamTlsContext,
    VirtualHost,
};

/// The only network filter name envoy-rust recognizes in phase 01.
pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

/// The TCP-proxy network filter name. envoy-rust accepts it as of phase 02.1;
/// runtime dispatch lands in phase 02.2. See ADR-0014.
pub const TCP_PROXY_FILTER: &str = "envoy.filters.network.tcp_proxy";

/// The HTTP connection manager network filter name. envoy-rust accepts it as
/// of phase 04.1; runtime dispatch lands in tasks 10–11. See ADR-0020.
pub const HCM_FILTER: &str = "envoy.filters.network.http_connection_manager";

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
    /// Within one listener, two filter chains declared the same SNI value
    /// (case-insensitive) in their `filter_chain_match.server_names`. Note: the
    /// variant name follows the parent-phase-03 SPEC §7's projection
    /// (`MultipleListenersWithOverlappingSni`) — the rule is intra-listener (per
    /// listener, not across listeners), but the name is preserved verbatim. The
    /// `listener` field names the offending listener; the `sni` field names the
    /// duplicated SNI in lowercased canonical form.
    #[error("listener {listener:?} has two filter chains with overlapping SNI {sni:?}")]
    MultipleListenersWithOverlappingSni { listener: String, sni: String },
    /// Within one listener, more than one filter chain has empty
    /// `filter_chain_match.server_names` (or no `filter_chain_match`). At most one
    /// catch-all chain is allowed per listener.
    #[error("listener {listener:?} has more than one catch-all filter chain (empty server_names)")]
    MultipleCatchAllFilterChains { listener: String },
    /// Within one listener with multiple filter chains, at least one chain
    /// carries `transport_socket: TLS` while another does not. Phase-03 does not
    /// support mixing TLS and plaintext chains on the same listener (would
    /// require `tls_inspector` listener filter, deferred to a later phase).
    #[error("listener {listener:?} mixes TLS and plaintext filter chains")]
    MixedTlsAndPlaintextFilterChainsOnListener { listener: String },
    #[error("unsupported codec_type: {got:?}; only AUTO and HTTP1 are supported in phase 04")]
    UnsupportedCodecType { got: bootstrap::CodecType },
    #[error(
        "unsupported HTTP filter: {name}; only envoy.filters.http.router is supported in phase 04.x"
    )]
    UnsupportedHttpFilter { name: String },
    #[error("unsupported route matcher: {matcher}; exactly one of `prefix` or `path` must be set")]
    UnsupportedRouteMatcher { matcher: &'static str },
    #[error(
        "unsupported virtual_host domain: {domain}; only \"*\" or syntactically-valid DNS names are supported in phase 04"
    )]
    UnsupportedDomainMatcher { domain: String },
    #[error("RouteConfiguration `{route_config}` has no virtual_hosts")]
    EmptyVirtualHosts { route_config: String },
    #[error("VirtualHost `{virtual_host}` has no routes")]
    EmptyRoutes { virtual_host: String },
    #[error("VirtualHost `{virtual_host}` has no domains")]
    EmptyDomains { virtual_host: String },
    #[error("invalid status code: {status}; must be in 100..=599")]
    InvalidStatusCode { status: u16 },
    #[error("unsupported DataSource at field `{field}`: requires `{requires}`")]
    UnsupportedDataSource {
        field: &'static str,
        requires: &'static str,
    },
    #[error(
        "unsupported HTTP filter count: {count}; phase 04.x's HCM accepts exactly one filter (the router)"
    )]
    MultipleHttpFilters { count: usize },

    /// HeaderMatcher.name was empty. Phase 04.2.
    #[error("HeaderMatcher.name must be non-empty")]
    EmptyHeaderName,

    /// SafeRegex.regex failed `regex::Regex::new`. Phase 04.2 (under ADR-0021).
    #[error("invalid regex `{regex}`: {source}")]
    InvalidRegex {
        regex: String,
        #[source]
        source: regex::Error,
    },

    /// Int64Range.start >= Int64Range.end (the half-open interval would be
    /// empty). Phase 04.2.
    #[error("invalid Int64Range: start {start} must be < end {end}")]
    InvalidInt64Range { start: i64, end: i64 },

    /// HeaderMatcher's hand-rolled Deserialize encountered an unrecognized mode
    /// key. Phase 04.2; the seven recognized keys are `exact_match`,
    /// `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`,
    /// `present_match`, `string_match`.
    #[error(
        "unknown HeaderMatcher mode key: {got:?}; expected one of exact_match, prefix_match, suffix_match, safe_regex_match, range_match, present_match, string_match"
    )]
    UnknownHeaderMatcherMode { got: String },

    /// StringMatcher's hand-rolled Deserialize encountered an unrecognized mode
    /// key. Phase 04.2; the five recognized keys are `exact`, `prefix`, `suffix`,
    /// `safe_regex`, `contains`. (`ignore_case` is a peer of the mode key, not a
    /// mode key itself; it does not trip this error.)
    #[error(
        "unknown StringMatcher mode key: {got:?}; expected one of exact, prefix, suffix, safe_regex, contains"
    )]
    UnknownStringMatcherMode { got: String },
}

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let mut bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    bootstrap::validate(&mut bootstrap)?;
    Ok(bootstrap)
}
