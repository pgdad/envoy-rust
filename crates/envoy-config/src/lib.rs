#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint. See `docs/envoy-rust/DECISIONS.md`
//! ADR-0008 for the extraction rationale.

pub mod bootstrap;
pub mod matcher;

pub use bootstrap::{
    AccessLog, AccessLogTypedConfig, Address, Admin, AppendAction, Bootstrap,
    CertificateValidationContext, Cluster, ClusterType, CodecType, CommonTlsContext, DataSource,
    DirectResponse, DnsLookupFamily, DownstreamTlsContext, Endpoint, ExplicitHttpConfig,
    FileAccessLog, FilterChain, FilterChainMatch, HeaderMatcher, HeaderMatcherMode,
    HeaderMutationConfig, HeaderMutationEntry, HeaderValue, HeaderValueOption,
    Http1ProtocolOptions, Http2ProtocolOptions, HttpConnectionManagerConfig, HttpFilter,
    HttpFilterTypedConfig, HttpProtocolOptions, HttpStatus, Int64Range, LbEndpoint, LbPolicy,
    Listener, LoadAssignment, LocalRateLimitConfig, LocalityLbEndpoints, Mutations, NetworkFilter,
    Node, Route, RouteAction, RouteAction_Route, RouteConfiguration, RouteMatch, RouterConfig,
    SafeRegex, SocketAddress, StaticResources, StringMatcher, StringMatcherMode, TcpProxyConfig,
    TlsCertificate, TokenBucket, TransportSocket, TransportSocketTypedConfig, TypedConfig,
    TypedExtensionProtocolOptions, UpstreamTlsContext, VirtualHost,
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
    #[error("unknown cluster '{0}'")]
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
    /// HCM `codec_type: HTTP2` declared on a listener whose `filter_chains[*]`
    /// carries a `transport_socket` of name `envoy.transport_sockets.tls`. Phase
    /// 05's H2 posture is plaintext H2C only — TLS+ALPN+H2 is deferred per
    /// parent-05 SPEC §4. Whichever later phase ships ALPN-negotiated H2 over
    /// TLS retires this variant.
    #[error(
        "HTTP/2 over TLS is not supported in phase 05; the listener must be plaintext or use codec_type: HTTP1/AUTO"
    )]
    Http2OverTlsNotSupported,
    /// `http2_protocol_options.<field>` value violates RFC 7540's wire-format
    /// range constraint. `field` names the offending field; `value` is the
    /// configured value; `range` is the inclusive (min, max) interval.
    #[error(
        "Http2ProtocolOptions field {field} value {value} out of range; must be in [{}, {}]",
        .range.0, .range.1
    )]
    Http2ProtocolOptionsOutOfRange {
        field: &'static str,
        value: u32,
        range: (u32, u32),
    },
    /// Cluster-side `typed_extension_protocol_options.HttpProtocolOptions`'s
    /// `explicit_http_config` had BOTH `http_protocol_options` (H1 arm) AND
    /// `http2_protocol_options` (H2 arm) set. Envoy's proto defines these as
    /// a oneof; at most one may be set. 05.3 NEW per SPEC §3 D2.a.
    #[error(
        "cluster '{cluster}': explicit_http_config has both http_protocol_options and http2_protocol_options set; at most one is permitted"
    )]
    MutuallyExclusiveExplicitHttpConfig { cluster: String },
    /// A `@type` URL field did not equal its expected literal. General-
    /// purpose: any future call site running an `@type` URL check (typed
    /// extensions, transport sockets, network filters, http filters, ...) may
    /// reuse this variant. First instantiated at 05.3 for cluster-side
    /// `typed_extension_protocol_options.HttpProtocolOptions.@type`
    /// per SPEC §3 D2.a; the variant carries no use-site discriminator.
    #[error("typed config @type {got:?} not supported; expected {expected:?}")]
    UnsupportedTypedConfigUrl { got: String, expected: &'static str },
    #[error("unsupported codec_type: {got:?}; only AUTO, HTTP1, and HTTP2 are supported")]
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
    /// Superseded by `EmptyHttpFilters` / `RouterNotTerminal` /
    /// `DuplicateRouterFilter` at 07.1; retained for ledger discipline
    /// per D-3.5 (typed-error API is grow-only). No code path
    /// constructs this variant after 07.1 Task 4.
    #[error(
        "unsupported HTTP filter count: {count}; phase 04.x's HCM accepts exactly one filter (the router)"
    )]
    MultipleHttpFilters { count: usize },

    /// 07.1 D4.1: listener's http_filters list is empty.
    ///
    /// HCM listeners must declare at least one HTTP filter (the
    /// `Router` filter — terminus). Empty lists are not legal per the
    /// terminal-router validator.
    #[error("HCM listener {listener:?} has empty http_filters list (must contain at least Router)")]
    EmptyHttpFilters { listener: String },

    /// 07.1 D4.1: listener's Router filter is not at the terminus
    /// position.
    ///
    /// The validator requires Router to be the last entry in
    /// `http_filters`. Earlier-Router placements trigger this error.
    #[error(
        "HCM listener {listener:?}: Router filter is not at the terminus (found at position {position})"
    )]
    RouterNotTerminal { listener: String, position: usize },

    /// 07.1 D4.1: listener's http_filters list contains more than one
    /// Router filter.
    ///
    /// The validator requires exactly one Router. Duplicate Routers
    /// trigger this error.
    #[error("HCM listener {listener:?}: filter chain contains duplicate Router filter")]
    DuplicateRouterFilter { listener: String },

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

    /// 06.2 NEW: an `access_log[*].name` value is not
    /// `envoy.access_loggers.file`. Phase 06.2 supports only the file
    /// access logger; stdout / gRPC / OpenTelemetry loggers are deferred
    /// to later observability-family phases per parent-06 SPEC §4.
    #[error(
        "unsupported access log type: {actual}; only 'envoy.access_loggers.file' with @type ending in .FileAccessLog is supported"
    )]
    UnsupportedAccessLogType { actual: String },

    /// 06.2 NEW: a `FileAccessLog.path` was the empty string. The file
    /// sink (`envoy-accesslog::FileSink::new`) cannot meaningfully open
    /// "" — reject at parse time rather than letting the open fail later.
    #[error("access log path must be non-empty")]
    InvalidAccessLogPath,

    /// 06.3 D14.3: listener with codec_type HTTP1 or AUTO routes to a cluster
    /// whose typed_extension_protocol_options.HttpProtocolOptions.
    /// explicit_http_config.http2_protocol_options is set. Closes
    /// phase-05.3 REVIEW I1 substantively — ADR-0028's option-(B) deferred
    /// the H1-listener H2-arm dispatch (envoy-http1 ↔ envoy-http2 cycle);
    /// the deferral is correct doctrine but the deferred path must be
    /// visibly rejected at config-load time so operators don't get a
    /// confusing 502 (or worse, silent H1-on-the-wire to an H2-only backend)
    /// at runtime.
    #[error(
        "listener '{listener}' has codec_type HTTP1 (or AUTO) but routes to cluster '{cluster}' whose typed_extension_protocol_options selects HTTP/2 upstream; H1-listener × H2-cluster dispatch is deferred per ADR-0028"
    )]
    Http2ClusterFromHttp1Listener { listener: String, cluster: String },

    /// 07.2: HeaderMutation entry uses an `append_action` outside the
    /// supported subset (`APPEND_IF_EXISTS_OR_ADD` / `OVERWRITE_IF_EXISTS_OR_ADD`).
    /// `ADD_IF_ABSENT` / `OVERWRITE_IF_EXISTS` parse at the schema level but are
    /// rejected here. `position` is the entry index within its mutations list.
    #[error(
        "HCM listener {listener:?}: HeaderMutation entry at position {position} uses unsupported append_action {action}"
    )]
    UnsupportedHeaderMutationAppendAction {
        listener: String,
        position: usize,
        action: String,
    },

    /// 07.2: HeaderMutation entry has an empty `header.key`.
    #[error(
        "HCM listener {listener:?}: HeaderMutation entry at position {position} has an empty header key"
    )]
    EmptyHeaderMutationKey { listener: String, position: usize },

    /// 07.2: HeaderMutation entry's `header.key` contains a byte outside the
    /// RFC 7230 §3.2.6 token set.
    #[error(
        "HCM listener {listener:?}: HeaderMutation entry at position {position} has an invalid token in header key {key:?}"
    )]
    InvalidHeaderMutationKey {
        listener: String,
        position: usize,
        key: String,
    },

    /// 09: LocalRateLimit filter has an empty `stat_prefix`.
    #[error("HCM listener {listener:?}: LocalRateLimit filter has an empty stat_prefix")]
    EmptyLocalRateLimitStatPrefix { listener: String },

    /// 09: LocalRateLimit filter's `token_bucket.max_tokens` is zero.
    #[error("HCM listener {listener:?}: LocalRateLimit filter token_bucket.max_tokens must be > 0")]
    TokenBucketMaxTokensMustBePositive { listener: String },

    /// 09: LocalRateLimit filter's `token_bucket.fill_interval` is missing, has
    /// the wrong shape, has an unsupported unit suffix, or parses to zero.
    #[error(
        "HCM listener {listener:?}: LocalRateLimit filter token_bucket.fill_interval is invalid: {message}"
    )]
    InvalidTokenBucketFillInterval { listener: String, message: String },

    /// 09: LocalRateLimit filter's `status.code` is not 429. Phase 09 accepts
    /// 429 only.
    #[error(
        "HCM listener {listener:?}: LocalRateLimit filter status.code {code} is unsupported (phase 09 accepts 429 only)"
    )]
    UnsupportedLocalRateLimitStatusCode { listener: String, code: u16 },
}

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let mut bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    bootstrap::validate(&mut bootstrap)?;
    Ok(bootstrap)
}
