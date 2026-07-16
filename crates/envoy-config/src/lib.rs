#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint. See `docs/envoy-rust/DECISIONS.md`
//! ADR-0008 for the extraction rationale.

pub mod bootstrap;
pub mod cds;
pub mod eds;
pub mod lds;
pub mod matcher;
pub mod rds;

pub use bootstrap::{
    AccessLog, AccessLogFilter, AccessLogTypedConfig, Action, Address, Admin, AppendAction,
    AttemptOutcome, Bootstrap, Buffer, BufferPerRoute, CdnLoopConfig, CertificateValidationContext,
    CidrRange, CircuitBreakers, Cluster, ClusterType, CodecType, CommonLbConfig, CommonTlsContext,
    ComparisonFilter, ComparisonOp, ConfigSource, CorsConfig, CorsPolicy, CsrfPolicy, DataSource,
    DataSourceInline, DenominatorType, DirectResponse, DirectResponseConfig, DnsLookupFamily,
    DownstreamTlsContext, DynamicResources, EdsClusterConfig, Endpoint, ExplicitHttpConfig,
    FaultAbort, FaultConfig, FileAccessLog, FilterChain, FilterChainMatch, FractionalPercent,
    HashPolicy, HashPolicyHeader, HeaderMatcher, HeaderMatcherMode, HeaderMutationConfig,
    HeaderMutationEntry, HeaderToMetadataConfig, HeaderToMetadataKeyValue, HeaderToMetadataRule,
    HeaderToMetadataType, HeaderValue, HeaderValueOption, HealthCheck, HealthCheckPayload,
    Http1ProtocolOptions, Http2ProtocolOptions, HttpConnectionManagerConfig, HttpFilter,
    HttpFilterTypedConfig, HttpHealthCheck, HttpProtocolOptions, HttpStatus, Int64Range,
    JsonFormatValue, JwtAuthnConfig, JwtProvider, JwtRequirement, LbEndpoint, LbMetadata, LbPolicy,
    LbSubsetConfig, LbSubsetFallbackPolicy, LbSubsetSelector, Listener, LoadAssignment,
    LocalRateLimitConfig, LocalityLbEndpoints, MetadataEntry, MetadataMatcher, MetadataPathSegment,
    Mutations, NetworkFilter, NetworkRbacConfig, Node, OutlierDetection, PathConfigSource,
    PathMatcher, PayloadDecodeError, PerFilterConfig, Percent, Permission, PermissionSet, Policy,
    Principal, PrincipalSet, RbacConfig, Rds, RequirementRule, RetryConfig, RetryOn, RetryPolicy,
    Route, RouteAction, RouteAction_Route, RouteConfiguration, RouteMatch, RouterConfig,
    RoutingPriority, Rules, RuntimeFractionalPercent, RuntimeUInt32, SafeRegex, SetMetadataConfig,
    SocketAddress, StaticResources, StatusCodeFilter, StringMatcher, StringMatcherMode,
    SubstitutionFormatString, TcpHealthCheck, TcpProxyConfig, Thresholds, TlsCertificate,
    TokenBucket, TransportSocket, TransportSocketTypedConfig, TypedConfig,
    TypedExtensionProtocolOptions, UpstreamTlsContext, ValueMatcher, VirtualHost, parse_duration,
};
pub use cds::parse_cds_file;
pub use eds::parse_eds_file;
pub use lds::parse_lds_file;
pub use rds::{parse_rds_file, reparse_and_select_route_config};

/// The only network filter name envoy-rust recognizes in phase 01.
pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

/// The TCP-proxy network filter name. envoy-rust accepts it as of phase 02.1;
/// runtime dispatch lands in phase 02.2. See ADR-0014.
pub const TCP_PROXY_FILTER: &str = "envoy.filters.network.tcp_proxy";

/// The HTTP connection manager network filter name. envoy-rust accepts it as
/// of phase 04.1; runtime dispatch lands in tasks 10–11. See ADR-0020.
pub const HCM_FILTER: &str = "envoy.filters.network.http_connection_manager";

/// The direct-response network filter name. envoy-rust accepts it as of phase
/// 66 — the Network-filters family opener. A TERMINAL filter (see
/// `is_terminal_network_filter`). See ADR-0123.
pub const DIRECT_RESPONSE_FILTER: &str = "envoy.filters.network.direct_response";

/// 67.1 (ADR-0128 / ADR-0129): the Network-filters family's FIRST NON-TERMINAL
/// filter. Deliberately ABSENT from `is_terminal_network_filter` — that absence
/// IS its non-terminality. NOT to be confused with `envoy.filters.http.rbac`
/// (`crates/envoy-filter/src/rbac.rs`), a different feature sharing the name.
pub const NETWORK_RBAC_FILTER: &str = "envoy.filters.network.rbac";

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
    /// 66 (ADR-0123): a TERMINAL network filter appeared before the end of its
    /// filter chain. Mirrors upstream Envoy, which rejects the same shape with
    /// "terminal filter named <X> of type <X> must be the last filter in a
    /// network filter chain." `position` is 1-based.
    #[error(
        "terminal network filter '{name}' at position {position} of {chain_len} must be the last filter in its network filter chain"
    )]
    NetworkFilterNotTerminal {
        name: String,
        position: usize,
        chain_len: usize,
    },
    /// 67.1 D2 (ADR-0128 / ADR-0129): a NON-EMPTY network filter chain whose
    /// LAST filter is non-terminal. The bilateral dual of
    /// `NetworkFilterNotTerminal`. Upstream Envoy: `non-terminal filter named
    /// <X> of type <X> is the last filter in a network filter chain.` (SPEC R-1)
    ///
    /// An EMPTY `filters: []` chain stays ACCEPTED — measured upstream parity
    /// (SPEC R-7), which is what closed carry-forward M66-5. This variant is
    /// unreachable for an empty chain by construction (`filters.last()` is None).
    #[error(
        "listener {listener:?} filter_chains[{chain_index}]: non-terminal filter {last_filter:?} is the last filter in a network filter chain"
    )]
    NetworkFilterChainNotTerminated {
        listener: String,
        chain_index: usize,
        last_filter: String,
    },

    /// A NON-TERMINAL filter (e.g. network `rbac`) precedes `tcp_proxy` on a
    /// **TLS-downstream** filter chain.
    ///
    /// The PLAINTEXT form is SUPPORTED from phase **67.3** (ADR-0135): the
    /// establishment/data-phase split of `envoy_listener::ConnectionHandler` lets
    /// `tcp_proxy` connect upstream and relay a server-first banner BEFORE the
    /// chain's first-byte gate resolves, then gate the downstream→upstream
    /// direction on the first byte (or a data-less FIN). Only the TLS-downstream
    /// form stays fail-loud: the D6 probe MEASURED (`envoyproxy/envoy:v1.33.0`)
    /// that upstream Envoy establishes the `tcp_proxy` upstream at raw-TCP accept
    /// (BEFORE the handshake) and takes the RBAC verdict on the first DECRYPTED
    /// byte — an ordering envoy-rust's TLS handler does not yet reproduce.
    /// Owner: **CF-67-7**.
    ///
    /// (`direct_response`, the other establishment-time terminal, needs no rejection
    /// — `envoy-bin` bypasses the chain for it entirely, which is exact measured
    /// parity. See ADR-0132 decision 2.)
    ///
    /// This rejection is a **deliberate FAIL-LOUD divergence** (`ADR-0049`
    /// decision-2 (b)): upstream Envoy ACCEPTS this config. It is strictly better
    /// than shipping a wrong-ordering runtime for a composition envoy-rust cannot
    /// yet reproduce, and it is not a `BOOTSTRAP_PROMPT.md` §6.3 stub — the correct
    /// behavior is chartered to a future TLS-establishment phase (**CF-67-7**).
    /// Recorded in `BEHAVIOR_CONTRACT.md`, never silent.
    #[error(
        "listener {listener:?} filter_chains[{chain_index}]: non-terminal filter {non_terminal:?} \
         before terminal filter {terminal:?} on a TLS-downstream chain is not yet supported — \
         upstream Envoy establishes the upstream at raw-TCP accept and takes the RBAC verdict on the \
         first decrypted byte (CF-67-7 owns this; upstream Envoy accepts this config; the plaintext \
         form IS supported from phase 67.3)"
    )]
    UnsupportedNetworkFilterChainComposition {
        listener: String,
        chain_index: usize,
        non_terminal: String,
        terminal: String,
    },

    #[error("unknown cluster '{0}'")]
    UnknownCluster(String),
    /// 18 D1 (L8, ADR-0049): `dynamic_resources.cds_config.resource_api_version`
    /// carried an unsupported value. envoy-rust accepts only `"V3"` or an
    /// absent field; any other value (e.g. `"V2"`) is rejected loudly.
    #[error(
        "dynamic_resources.cds_config.resource_api_version '{0}' is unsupported; envoy-rust accepts only 'V3' or absent"
    )]
    UnsupportedResourceApiVersion(String),
    /// 18 D2: reading the CDS file at the configured path failed (I/O error).
    /// Part of the D1 schema surface; first raised by the Task-2 CDS parser.
    #[error("reading CDS file '{path}': {source}")]
    CdsFileError {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// 18 D2/D3: parsing the CDS file's contents failed. Part of the D1 schema
    /// surface; first raised by the Task-2/Task-3 CDS loader.
    #[error("parsing CDS file '{path}': {message}")]
    CdsParseError { path: String, message: String },
    /// 19 D2: reading the LDS file at the configured path failed (I/O error).
    /// Part of the D1 schema surface; first raised by the Task-2 LDS parser.
    #[error("reading LDS file '{path}': {source}")]
    LdsFileError {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// 19 D2/D3: parsing the LDS file's contents failed. Part of the D1 schema
    /// surface; first raised by the Task-2/Task-3 LDS loader.
    #[error("parsing LDS file '{path}': {message}")]
    LdsParseError { path: String, message: String },
    /// 20 D1 (L9, ADR-0051/0052): an HCM declares neither `route_config` (inline)
    /// nor `rds` (file). Exactly one is required; raised at parse time by
    /// `check_route_sources`. `stat_prefix` names the offending HCM.
    #[error(
        "missing route source on HCM (stat_prefix {stat_prefix:?}): exactly one of `route_config` or `rds` is required"
    )]
    MissingRouteSource { stat_prefix: String },
    /// 20 D1 (L9, ADR-0051/0052): an HCM declares BOTH `route_config` (inline) and
    /// `rds` (file). They are mutually exclusive; raised at parse time by
    /// `check_route_sources`. `stat_prefix` names the offending HCM.
    #[error(
        "ambiguous route source on HCM (stat_prefix {stat_prefix:?}): `route_config` and `rds` are mutually exclusive"
    )]
    AmbiguousRouteSource { stat_prefix: String },
    /// 20 D2: reading the RDS file at the configured path failed (I/O error).
    /// Part of the D1 schema surface; first raised by the Task-2 RDS loader.
    #[error("reading RDS file '{path}': {source}")]
    RdsFileError {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// 20 D2/D3: parsing the RDS file's contents failed. Part of the D1 schema
    /// surface; first raised by the Task-2/Task-3 RDS loader.
    #[error("parsing RDS file '{path}': {message}")]
    RdsParseError { path: String, message: String },
    /// 20 D3: the `rds.route_config_name` was not found among the route
    /// configurations in the RDS file. Part of the D1 schema surface; first
    /// raised by the Task-3 RDS loader.
    #[error("RDS route_config_name {name:?} not found in {path:?}")]
    RdsRouteConfigNotFound { name: String, path: String },
    /// 21 D1 (ADR-0053/0054): a non-EDS cluster carries neither an inline
    /// `load_assignment` nor (validly) an `eds_cluster_config`.
    #[error("cluster {cluster:?}: a non-EDS cluster requires `load_assignment`")]
    MissingLoadAssignment { cluster: String },
    /// 21 D1: a `type: EDS` cluster carries no `eds_cluster_config` (L6 6c).
    #[error("cluster {cluster:?}: a `type: EDS` cluster requires `eds_cluster_config`")]
    MissingEdsClusterConfig { cluster: String },
    /// 21 D1: `eds_cluster_config` set on a non-EDS cluster (L6 6b).
    #[error("cluster {cluster:?}: `eds_cluster_config` set on a non-EDS cluster")]
    EdsConfigOnNonEdsCluster { cluster: String },
    /// 21 D1: an inline `load_assignment` on a `type: EDS` cluster (L6 6a —
    /// envoy-rust is stricter than Envoy, which accepts-and-ignores).
    #[error(
        "cluster {cluster:?}: a `type: EDS` cluster must not carry an inline `load_assignment`"
    )]
    LoadAssignmentOnEdsCluster { cluster: String },
    /// 21 D2: reading the EDS file at the configured path failed (I/O error).
    #[error("EDS file error reading {path:?}: {source}")]
    EdsFileError {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// 21 D2: parsing the EDS file's contents failed.
    #[error("EDS file parse error in {path:?}: {message}")]
    EdsParseError { path: String, message: String },
    /// 21 D3: the EDS `ClusterLoadAssignment` selected by name was not found in
    /// the EDS file.
    #[error("EDS ClusterLoadAssignment {name:?} not found in {path:?}")]
    EdsClusterLoadAssignmentNotFound { name: String, path: String },
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

    /// Phase 32 (ADR-0079): a `FileAccessLog.log_format.text_format_source.inline_string`
    /// failed to compile (unknown/malformed command operator). Boot-fatal per ADR-0049.
    #[error("invalid access-log format string: {detail}")]
    InvalidAccessLogFormat { detail: String },

    /// Phase 38 (ADR-0092 §E): a `log_format` (`SubstitutionFormatString`) set
    /// NEITHER or BOTH of `{text_format_source, json_format}`. Exactly one arm is
    /// required (the v1.33.0 oneof — both-set and neither-set are both boot-fatal).
    #[error("log_format must set exactly one of text_format_source or json_format: {detail}")]
    AmbiguousLogFormat { detail: String },

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

    /// 67.1: a network `rbac` filter's `stat_prefix` is present but empty.
    /// Upstream enforces `min_len 1` via a proto constraint
    /// (`RBACValidationError.StatPrefix`). An ABSENT `stat_prefix` is a serde
    /// missing-field error, not this variant.
    #[error("listener {listener:?}: network rbac filter stat_prefix must be non-empty")]
    EmptyNetworkRbacStatPrefix { listener: String },

    /// 67.1 D3 (CF-67-4, ADR-0129): a NETWORK `rbac` Permission/Principal leaf
    /// that an L4 filter cannot evaluate.
    ///
    /// `header` is rejected in PARITY with upstream Envoy, which rejects it at
    /// config load (`Found header(name: ":path"…`, SPEC R-6, measured).
    /// `url_path` and `metadata` are rejected as a deliberate FAIL-LOUD
    /// divergence (ADR-0049 decision-2 (b)): upstream ACCEPTS a matcher that can
    /// never match at L4. No differential observable — neither fixture uses them.
    ///
    /// `67.2` WIDENS the allow-list to admit the connection-level arms. These
    /// three rejections stay permanently.
    #[error(
        "listener {listener:?}: network rbac policy {policy_name:?} uses matcher {arm:?} at {path}, which cannot be evaluated at L4"
    )]
    UnsupportedNetworkRbacMatcher {
        listener: String,
        policy_name: String,
        arm: &'static str,
        path: String,
    },

    /// 67.2 (ADR-0133): a `CidrRange` in a NETWORK rbac policy has an invalid
    /// prefix width for its address family (`prefix_len > 32` on IPv4 /
    /// `> 128` on IPv6). Config-load-time fatal (ADR-0049). Scope-neutral
    /// `listener {listener:?}` per the 67.1 W-1 generalization (ADR-0130).
    #[error(
        "listener {listener:?}: network rbac policy {policy_name:?} has an invalid CidrRange at {path}: {detail}"
    )]
    InvalidCidrRange {
        listener: String,
        policy_name: String,
        path: String,
        detail: String,
    },

    // The SEVEN RBAC tree/empty-set/leaf variants below are raised by BOTH
    // `envoy.filters.http.rbac` (phase 10) and `envoy.filters.network.rbac`
    // (phase 67.1), which has no HCM. Their messages are therefore scope-neutral
    // (`listener {listener:?}`, never `HCM listener`) — 67.1 W-1, ADR-0130. The
    // other `"HCM listener"` variants in this enum are genuinely HCM-scoped.
    //
    // `RbacMetadataMatcherInvalid` is the SEVENTH (REVIEW.md I-2). It was
    // previously left `"HCM listener"`-scoped on the theory that a network rbac
    // filter's `metadata` leaf "is rejected outright by `validate_l4_permission`
    // (67.1 D3) before that error can be reached." **That stated the validation
    // order backwards.** In `validate_network_rbac_config` (`bootstrap.rs`),
    // `validate_rbac_rules` runs FIRST — and it validates `Metadata` leaves
    // structurally via `validate_metadata_matcher` — and only THEN does the L4
    // allow-list walk run. So a structurally-invalid `metadata` leaf (an empty
    // `filter`, or a `path` that is not exactly one segment) raises this variant
    // before `validate_l4_permission` ever sees it. Reproduced, and pinned by
    // `structurally_invalid_metadata_leaf_is_not_reported_as_an_hcm_error`.
    //
    // That ORDER IS DELIBERATE and must not be swapped to "fix" this: running
    // `validate_rbac_rules` first is what bounds tree depth before the L4
    // recursion descends the same tree, a stack-safety guarantee pinned by
    // `network_rbac_depth_bound_precedes_the_l4_walk`. The message is generalized
    // instead — it stays accurate for the HTTP filter, whose listener IS an HCM
    // listener.
    /// 10: RBAC filter has no policies (rules.policies is empty).
    #[error("listener {listener:?}: RBAC filter has no policies (rules.policies is empty)")]
    EmptyRbacPolicies { listener: String },

    /// 10: RBAC policy has no permissions.
    #[error("listener {listener:?}: RBAC policy {policy_name:?} has no permissions")]
    EmptyRbacPolicyPermissions {
        listener: String,
        policy_name: String,
    },

    /// 10: RBAC policy has no principals.
    #[error("listener {listener:?}: RBAC policy {policy_name:?} has no principals")]
    EmptyRbacPolicyPrincipals {
        listener: String,
        policy_name: String,
    },

    /// 10: RBAC policy has an empty Permission set
    /// (`Permission::AndRules` or `Permission::OrRules` with empty `rules`).
    #[error(
        "listener {listener:?}: RBAC policy {policy_name:?} has an empty Permission set at {path}"
    )]
    EmptyRbacPermissionSet {
        listener: String,
        policy_name: String,
        path: String,
    },

    /// 10: RBAC policy has an empty Principal set
    /// (`Principal::AndIds` or `Principal::OrIds` with empty `ids`).
    #[error(
        "listener {listener:?}: RBAC policy {policy_name:?} has an empty Principal set at {path}"
    )]
    EmptyRbacPrincipalSet {
        listener: String,
        policy_name: String,
        path: String,
    },

    /// 10: RBAC policy Permission/Principal tree exceeds RBAC_TREE_MAX_DEPTH.
    /// Defense-in-depth bound at parse time; the runtime evaluator inherits it.
    #[error(
        "listener {listener:?}: RBAC policy {policy_name:?} Permission/Principal tree exceeds RBAC_TREE_MAX_DEPTH ({depth} > 16)"
    )]
    RbacTreeTooDeep {
        listener: String,
        policy_name: String,
        depth: u32,
    },

    /// Phase 35: an RBAC `metadata` matcher is malformed — an empty `filter`
    /// (Envoy: PGV min_len 1) or a `path` whose length is not exactly 1 (Envoy accepts a
    /// multi-segment path; envoy-rust's flat string store cannot resolve it → stricter boot-fatal).
    /// Both are config-load-time fatal (ADR-0049).
    ///
    /// 67.1 (REVIEW.md I-2): the message is **scope-neutral**. This variant is
    /// reachable from a NETWORK `rbac` filter, which has no HCM — see the block
    /// comment above.
    #[error(
        "listener {listener:?}: RBAC policy {policy_name:?} metadata matcher at {path} is invalid: {detail}"
    )]
    RbacMetadataMatcherInvalid {
        listener: String,
        policy_name: String,
        path: String,
        detail: String,
    },

    /// Phase 11: fault filter `abort.http_status` outside the syntactic HTTP
    /// status band (100..=599).
    #[error(
        "listener {listener:?}: fault abort http_status {status} is not a valid HTTP status code (must be 100-599)"
    )]
    InvalidFaultAbortStatus { listener: String, status: u16 },

    /// Phase 11: fault filter `abort.percentage.numerator` exceeds its denominator.
    #[error(
        "listener {listener:?}: fault abort percentage numerator {numerator} exceeds denominator {denominator}"
    )]
    FaultPercentageOutOfRange {
        listener: String,
        numerator: u32,
        denominator: u32,
    },

    /// Phase 11: fault filter fractional percentage (0 < numerator < denominator)
    /// is not supported — phase-11 scope is deterministic 0%/100% only (a
    /// fractional per-request abort is non-differential-testable per the
    /// differential contract; SPEC §4 + §5.6).
    #[error(
        "listener {listener:?}: fault abort fractional percentage {numerator}/{denominator} is unsupported (deterministic 0% or 100% only)"
    )]
    UnsupportedFractionalFaultPercentage {
        listener: String,
        numerator: u32,
        denominator: u32,
    },

    /// Phase 22: jwt_authn filter has no providers (at least one required).
    #[error("jwt_authn filter on listener `{listener}` has no providers; at least one is required")]
    JwtAuthnNoProviders { listener: String },

    /// Phase 22: jwt_authn rule references a provider name not in the providers map.
    #[error(
        "jwt_authn rule on listener `{listener}` references unknown provider `{provider_name}`"
    )]
    JwtAuthnUnknownProvider {
        listener: String,
        provider_name: String,
    },

    /// Phase 22: jwt_authn provider's local_jwks is not inline or fails to parse.
    #[error(
        "jwt_authn provider `{provider}` on listener `{listener}` has an invalid or non-inline local_jwks"
    )]
    JwtAuthnInvalidJwks { listener: String, provider: String },

    /// 23 D3 (ADR-0058 / L7): a route carries `typed_per_filter_config` for a
    /// filter that is NOT present in the enclosing HCM's http_filters chain.
    /// envoy-rust rejects this as startup-fatal (the ADR-0049 all-fatal posture);
    /// upstream Envoy accepts-and-ignores (recorded divergence, BEHAVIOR_CONTRACT).
    #[error(
        "route per-filter config names filter {filter:?} which is absent from the HTTP filter chain"
    )]
    PerRouteConfigForAbsentFilter { filter: String },

    /// 24 D3 (ADR-0061 L6): a csrf `filter_enabled.default_value` is neither 0%
    /// nor 100%. envoy-rust honors only deterministic gating (the phase-11 fault
    /// precedent); fractional gating needs the unimplemented RTDS runtime layer.
    #[error(
        "csrf filter_enabled on listener `{listener}` is non-deterministic (numerator must be 0 or the denominator value)"
    )]
    UnsupportedNonDeterministicCsrfFilterEnabled { listener: String },

    /// 24 D3 (ADR-0061 L6): a csrf `filter_enabled.runtime_key` is present.
    /// envoy-rust has no RTDS runtime layer to honor it (the ADR-0049 all-fatal posture).
    #[error(
        "csrf filter_enabled on listener `{listener}` has a runtime_key, which requires the unimplemented RTDS runtime layer"
    )]
    UnsupportedRuntimeKeyedCsrfFilterEnabled { listener: String },

    /// 12.1: cluster has more than one `health_checks` entry (phase-12 supports 0 or 1).
    #[error(
        "cluster '{cluster}' has more than one health_checks entry; phase 12 supports at most one"
    )]
    UnsupportedMultipleHealthChecks { cluster: String },

    /// 12.1 / 68 / 69: cluster's health check sets NONE of `http_health_check`,
    /// `tcp_health_check`, `grpc_health_check` (custom still deferred, fail-loud).
    #[error(
        "cluster '{cluster}' health check sets none of http_health_check/tcp_health_check/grpc_health_check; custom_health_check is not supported"
    )]
    UnsupportedHealthCheckType { cluster: String },

    /// 69 (ADR-0139): a health check sets MORE THAN ONE of
    /// http_health_check / tcp_health_check / grpc_health_check — the upstream
    /// `HealthCheck.health_checker` oneof rejects this at load (Generalizes the
    /// phase-68 `BothHttpAndTcpHealthCheck`.)
    #[error(
        "cluster '{cluster}' health check sets more than one of http_health_check/tcp_health_check/grpc_health_check (mutually exclusive)"
    )]
    MultipleHealthCheckers { cluster: String },

    /// 69 (ADR-0139): grpc_health_check on a cluster whose upstream is not HTTP/2.
    /// Real Envoy makes this load-fatal (MEASURED v1.33.0: "cluster must support
    /// HTTP/2 for gRPC healthchecking").
    #[error(
        "cluster '{cluster}' uses grpc_health_check but the cluster does not support HTTP/2 (set typed_extension_protocol_options HttpProtocolOptions.explicit_http_config.http2_protocol_options)"
    )]
    GrpcHealthCheckRequiresHttp2 { cluster: String },

    /// 12.1: `healthy_threshold` or `unhealthy_threshold` is zero (must be >= 1).
    #[error("cluster '{cluster}' health check {field} must be >= 1")]
    InvalidHealthCheckThreshold {
        cluster: String,
        field: &'static str,
    },

    /// 12.1: `timeout`/`interval` failed `parse_duration` or parsed to zero.
    /// §6.2 item-6: a sub-second decimal `0.5s` fails `parse_duration` and surfaces here.
    #[error(
        "cluster '{cluster}' health check {field} is not a positive integer-second duration (e.g. `1s`)"
    )]
    InvalidHealthCheckTiming {
        cluster: String,
        field: &'static str,
    },

    /// 62 D1 (ADR-0119): `common_http_protocol_options.idle_timeout` is present
    /// but not a positive `parse_duration` scalar (`"<N>s"`/`"<N>ms"`/`"<N>us"`).
    /// Fail-closed at parse time, mirroring Envoy's Duration validation, rather
    /// than silently falling back to the 60s default.
    #[error(
        "cluster '{cluster}' common_http_protocol_options.idle_timeout is not a positive duration (e.g. `30s`)"
    )]
    InvalidClusterIdleTimeout { cluster: String },

    /// 12.1: `http_health_check.path` is empty.
    #[error("cluster '{cluster}' http_health_check.path must be non-empty")]
    EmptyHealthCheckPath { cluster: String },

    /// 68 (ADR-0137 PV-1): a `tcp_health_check` `send`/`receive` payload `text`
    /// was odd-length or non-hex. Native fail-loud (byte-parity with Envoy's
    /// `invalid hex string` waived — config-load errors are not a wire surface).
    #[error(
        "cluster '{cluster}' tcp_health_check payload text '{value}' is not a valid hex string"
    )]
    InvalidHealthCheckPayloadHex { cluster: String, value: String },

    /// 68 (ADR-0137 PV-1): a `tcp_health_check` payload `binary` was not valid base64.
    #[error("cluster '{cluster}' tcp_health_check payload binary '{value}' is not valid base64")]
    InvalidHealthCheckPayloadBase64 { cluster: String, value: String },

    /// 68 (ADR-0137 PV-1): a `tcp_health_check` payload set neither `text` nor
    /// `binary` (or both). The `Payload` oneof requires exactly one.
    #[error("cluster '{cluster}' tcp_health_check payload must set exactly one of text or binary")]
    EmptyHealthCheckPayload { cluster: String },

    /// 12.1: `common_lb_config.healthy_panic_threshold.value` is outside [0.0, 100.0].
    #[error("cluster '{cluster}' healthy_panic_threshold value {value} is outside [0.0, 100.0]")]
    InvalidPanicThreshold { cluster: String, value: f64 },

    /// 13.1 D2: `circuit_breakers.thresholds` carries >1 entry. Phase-13 supports
    /// exactly 0 or 1 entry (DEFAULT priority only). Multi-priority circuit-breaking
    /// defers per parent SPEC §4.
    #[error(
        "cluster '{cluster}' carries multiple circuit_breakers.thresholds entries — phase-13 supports at most one (DEFAULT priority only)"
    )]
    UnsupportedMultipleCircuitBreakerThresholds { cluster: String },

    /// 13.1 D2: `circuit_breakers.thresholds[0].priority` is non-DEFAULT.
    /// Phase-13 supports DEFAULT only. HIGH priority defers per parent SPEC §4.
    #[error(
        "cluster '{cluster}' carries circuit_breakers.thresholds[0].priority = {priority:?} — phase-13 supports DEFAULT only"
    )]
    UnsupportedCircuitBreakerPriority {
        cluster: String,
        priority: crate::RoutingPriority,
    },

    /// 13.1 D2: `circuit_breakers.thresholds[0].max_connections: 0` is structurally
    /// meaningless — it would prevent any upstream connection. Reject explicitly.
    #[error(
        "cluster '{cluster}' carries circuit_breakers.thresholds[0].max_connections = {value} — must be >= 1"
    )]
    InvalidMaxConnections { cluster: String, value: u32 },

    /// 15 D2: `max_pending_requests > 0` (the pending-request queue) is deferred per ADR-0043;
    /// only `max_pending_requests: 0` (no-queue) is supported at phase-15 scope.
    #[error(
        "cluster '{cluster}': max_pending_requests={value} is unsupported; only 0 (no-queue) is accepted at this scope"
    )]
    UnsupportedNonZeroMaxPendingRequests { cluster: String, value: u32 },

    /// 14.1 D2 (parent-14 D2): outlier_detection.consecutive_5xx or
    /// outlier_detection.consecutive_gateway_failure is zero. Both detector thresholds
    /// must be >= 1 when present (the validator rejects `0`; absent is fine and means
    /// the detector is not configured).
    #[error("cluster '{cluster}' outlier_detection {field} must be >= 1")]
    InvalidOutlierDetectionThreshold {
        cluster: String,
        field: &'static str,
    },

    /// 14.1 D2: outlier_detection.interval or outlier_detection.base_ejection_time
    /// failed `parse_duration` or parsed to zero. Integer-second / millisecond /
    /// microsecond suffixes only (per parse_duration's contract); sub-second decimals
    /// (e.g. `0.5s`) are rejected (§6.2 item-6).
    #[error(
        "cluster '{cluster}' outlier_detection {field} is not a positive integer-unit duration (e.g. `10s`)"
    )]
    InvalidOutlierDetectionTiming {
        cluster: String,
        field: &'static str,
    },

    /// 14.1 D2: outlier_detection.max_ejection_percent is outside `[0, 100]`. The
    /// boundary values 0 and 100 are both accepted (0 ⇒ cap blocks all ejections;
    /// 100 ⇒ no cap effectively).
    #[error(
        "cluster '{cluster}' outlier_detection.max_ejection_percent {value} is outside [0, 100]"
    )]
    InvalidMaxEjectionPercent { cluster: String, value: u32 },

    /// 28 D1 (ADR-0070): a RING_HASH cluster's `ring_hash_lb_config.hash_function`
    /// is `MURMUR_HASH_2`. Upstream Envoy accepts it; envoy-rust deliberately
    /// narrows phase-28 to `XX_HASH` only (a documented divergence,
    /// BEHAVIOR_CONTRACT) and rejects it as startup-fatal (the ADR-0049 all-fatal
    /// posture).
    #[error(
        "cluster '{cluster}' ring_hash_lb_config.hash_function MURMUR_HASH_2 is unsupported; envoy-rust accepts XX_HASH only"
    )]
    UnsupportedHashFunction { cluster: String },

    /// 28 D1 (ADR-0069): a RING_HASH cluster's `ring_hash_lb_config.minimum_ring_size`
    /// exceeds its `maximum_ring_size`. Structurally inconsistent; rejected as
    /// startup-fatal (the ADR-0049 all-fatal posture).
    #[error(
        "cluster '{cluster}' ring_hash_lb_config.minimum_ring_size {minimum} exceeds maximum_ring_size {maximum}"
    )]
    RingSizeInversion {
        cluster: String,
        minimum: u64,
        maximum: u64,
    },

    /// 29 D1 (ADR-0072): a MAGLEV cluster's `maglev_lb_config.table_size` is not a
    /// prime number. Envoy requires a prime table size ("The table size of maglev
    /// must be prime number"); rejected as startup-fatal (the ADR-0049 all-fatal
    /// posture).
    #[error("cluster '{cluster}' maglev_lb_config.table_size {table_size} is not a prime number")]
    MaglevTableSizeNotPrime { cluster: String, table_size: u64 },

    /// 29 D1 (ADR-0072): a MAGLEV cluster's `maglev_lb_config.table_size` exceeds
    /// Envoy's maximum 5000011. Rejected as startup-fatal (the ADR-0049 all-fatal
    /// posture).
    #[error(
        "cluster '{cluster}' maglev_lb_config.table_size {table_size} exceeds the maximum 5000011"
    )]
    MaglevTableSizeTooLarge { cluster: String, table_size: u64 },

    /// 28 Task 4 (ADR-0069): a route `hash_policy` entry names a `policy_specifier`
    /// other than `header`. Envoy's `RouteAction.hash_policy` is a oneof
    /// (`header` / `cookie` / `connection_properties` / `query_parameter` /
    /// `filter_state`); the phase-28 MVP supports only `header`. An unsupported
    /// specifier is rejected as startup-fatal (the ADR-0049 all-fatal posture)
    /// rather than silently ignored, so the proxy never mis-routes by dropping a
    /// hash policy. `specifier` names the offending oneof key.
    #[error(
        "route hash_policy specifier '{specifier}' is unsupported; envoy-rust accepts only 'header'"
    )]
    UnsupportedHashPolicy { specifier: String },

    /// 31 Task 2 (ADR-0077 §6.2-LOCKED): a `cdn_loop` filter's `cdn_id` is the
    /// empty string. `cdn_id` is REQUIRED to be a non-empty RFC-7230 token;
    /// rejected as startup-fatal (the ADR-0049 all-fatal posture — Envoy itself
    /// rejects an empty cdn_id at boot). `listener` names the offending HCM.
    #[error(
        "cdn_loop filter on listener `{listener}` has an empty cdn_id; a non-empty token is required"
    )]
    CdnLoopEmptyCdnId { listener: String },

    /// 31 Task 2 (ADR-0077 §6.2-LOCKED): a `cdn_loop` filter's `cdn_id` is
    /// non-empty but is not a bare RFC-7230 token — it carries a comma, a space,
    /// or some other non-`tchar` byte. Rejected as startup-fatal (the ADR-0049
    /// all-fatal posture). `listener` names the offending HCM; `cdn_id` echoes
    /// the rejected value.
    #[error(
        "cdn_loop filter on listener `{listener}` has an invalid cdn_id {cdn_id:?}; it must be a bare RFC-7230 token (no comma, space, or other non-tchar)"
    )]
    CdnLoopInvalidCdnId { listener: String, cdn_id: String },

    /// Phase 33 (§A5-LOCKED): a `set_metadata` filter entry has an empty
    /// `metadata_namespace`. Envoy rejects this boot-fatally (PGV: length ≥ 1);
    /// envoy-rust matches (ADR-0049 all-fatal). `listener` names the offending HCM.
    #[error(
        "set_metadata filter on listener `{listener}` has an empty metadata_namespace; a non-empty namespace is required"
    )]
    SetMetadataEmptyNamespace { listener: String },

    /// Phase 34 (§A5-LOCKED): a `header_to_metadata` rule is malformed (empty header, no action,
    /// empty key, or an on_header_missing with no value). Envoy rejects these boot-fatally; envoy-rust
    /// matches (ADR-0049). `listener` names the offending HCM; `detail` the specific violation.
    #[error("header_to_metadata filter on listener `{listener}` has an invalid rule: {detail}")]
    HeaderToMetadataInvalidRule { listener: String, detail: String },
}

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let mut bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    // 20 D1 (C16): the exactly-one-of route-source check runs here, before any
    // file is read and before validate(), so an `rds` HCM (route_config: None)
    // survives validate()'s inline-route walk (which early-returns on None).
    bootstrap::check_route_sources(&bootstrap)?;
    // 21 D1 (C16; L6 6a): the parse-time-only endpoint-source check — reject an
    // inline `load_assignment` on a `type: EDS` cluster (stricter than Envoy).
    // Runs here, before validate() and before any EDS file is read, so it does
    // not false-positive the post-merge loaded state (validate_cluster tolerates
    // an EDS cluster carrying both load_assignment and eds_cluster_config).
    bootstrap::check_endpoint_sources(&bootstrap)?;
    bootstrap::validate(&mut bootstrap)?;
    Ok(bootstrap)
}

/// 18 D3 / 19 D3 / 20 D3: read + parse + merge the CDS, LDS, and RDS files
/// (ADR-0048/0049/0050/0051/0052). Called by envoy-bin AFTER `parse_bootstrap`
/// and BEFORE any consumer iterates clusters or listeners. No-op when neither
/// `dynamic_resources.cds_config` / `lds_config` is configured AND no HCM uses
/// `rds`. ALL failures are fatal (the L4 reconciliation — envoy-rust never
/// warn-and-serves a broken CDS/LDS/RDS file; recorded divergence vs Envoy,
/// BEHAVIOR_CONTRACT).
///
/// §5.7 merge ordering: the CDS branch merges dynamic CLUSTERS into
/// `dynamic_clusters` BEFORE the single post-merge re-validation, so that when
/// that re-validation checks a dynamic LISTENER's route→cluster references, the
/// dynamic cluster is already visible via `all_clusters()`. The LDS branch then
/// merges dynamic LISTENERS. AFTER the LDS merge, `check_route_sources` is
/// re-run over the MERGED (static + dynamic-LDS) listener set — while rds-HCMs
/// still have `route_config: None` — so an LDS-supplied HCM with neither/both
/// route source fails (MissingRouteSource / AmbiguousRouteSource) BEFORE any RDS
/// file is read (M20-T1-c). The RDS pass then walks every HCM across the
/// effective listener set, reads + parses + name-selects each `rds` file's
/// RouteConfiguration, and populates the HCM's `route_config` (§5.3 uniform
/// downstream shape). Because CDS merged FIRST, an RDS route to a CDS-supplied
/// cluster resolves; a route to a cluster in NEITHER list is fatal (L7
/// UnknownCluster) at the post-merge re-validation. Exactly ONE
/// `bootstrap::validate()` runs after ALL branches + the RDS pass — never one
/// validate per branch — and it now walks the rds-populated `route_config`s.
/// That single re-validation is gated on
/// `dynamic_clusters.is_some() || dynamic_listeners.is_some() || had_rds_hcm`,
/// so an rds-only bootstrap (no CDS/LDS) still re-validates its now-populated
/// inline route table.
///
/// L6 recorded divergence: a dynamic-listener route to a cluster in NEITHER the
/// static nor the dynamic list is FATAL here (`UnknownCluster`), diverging from
/// Envoy (which would start and 503 at runtime). This is intentional and
/// pre-ratified (defer-then-revalidate).
///
/// M18-1 on-error mutation caveat: this function mutates `dynamic_clusters` /
/// `dynamic_listeners` IN PLACE before the post-merge `validate()`. If that
/// validate (or a later branch) errors, those fields stay populated — the
/// caller MUST treat any error as fatal-startup and discard the bootstrap, NOT
/// retry against the partially-mutated value.
///
/// Deliberately NOT called by `parse_bootstrap`: `parse_bootstrap` is the fuzz
/// target and must stay pure (no file I/O).
pub fn load_dynamic_resources(bootstrap: &mut Bootstrap) -> Result<(), ConfigError> {
    // ---- CDS branch (phase 18, ADR-0048/0049) ----
    // Clone the path early and drop the `&bootstrap` borrow before the later
    // `&mut bootstrap` mutation.
    let cds_path = bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.cds_config.as_ref())
        .map(|cs| cs.path_config_source.path.clone());
    if let Some(path) = cds_path {
        let contents =
            std::fs::read_to_string(&path).map_err(|source| ConfigError::CdsFileError {
                path: path.clone(),
                source,
            })?;
        let parsed = cds::parse_cds_file(&path, &contents)?;
        // L9 (ADR-0049): static wins on name collision — the dynamic duplicate is
        // skipped with a warning, mirroring Envoy's "skipped N unmodified cluster(s)".
        let mut dynamic = Vec::with_capacity(parsed.len());
        for cluster in parsed {
            if bootstrap
                .static_resources
                .clusters
                .iter()
                .any(|c| c.name == cluster.name)
            {
                tracing::warn!(cluster = %cluster.name, "CDS cluster collides with a static cluster; static wins (skipped)");
                continue;
            }
            // Intra-file duplicates: first wins, warn, skip.
            if dynamic.iter().any(|c: &Cluster| c.name == cluster.name) {
                tracing::warn!(cluster = %cluster.name, "duplicate cluster in CDS file; first wins (skipped)");
                continue;
            }
            dynamic.push(cluster);
        }
        bootstrap.dynamic_clusters = Some(dynamic);
    }

    // ---- LDS branch (phase 19, ADR-0050; §6.2 L1/L4/L7) ----
    let lds_path = bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.lds_config.as_ref())
        .map(|cs| cs.path_config_source.path.clone());
    if let Some(path) = lds_path {
        let contents =
            std::fs::read_to_string(&path).map_err(|source| ConfigError::LdsFileError {
                path: path.clone(),
                source,
            })?;
        let parsed = lds::parse_lds_file(&path, &contents)?;
        // L7: static wins on listener name collision; intra-file first wins.
        let mut dynamic = Vec::with_capacity(parsed.len());
        for listener in parsed {
            if bootstrap
                .static_resources
                .listeners
                .iter()
                .any(|l| l.name == listener.name)
            {
                tracing::warn!(listener = %listener.name, "LDS listener collides with a static listener; static wins (skipped)");
                continue;
            }
            if dynamic.iter().any(|l: &Listener| l.name == listener.name) {
                tracing::warn!(listener = %listener.name, "duplicate listener in LDS file; first wins (skipped)");
                continue;
            }
            dynamic.push(listener);
        }
        bootstrap.dynamic_listeners = Some(dynamic);
    }

    // ---- M20-T1-c: re-check route-source cardinality over the MERGED set ----
    // Runs AFTER the LDS merge but BEFORE the RDS pass, while rds-HCMs still
    // have `route_config: None` — so an LDS-supplied HCM with neither route
    // source (MissingRouteSource) or both (AmbiguousRouteSource) fails here,
    // and the RDS population below cannot falsely trip the "both" arm.
    bootstrap::check_route_sources(bootstrap)?;

    // ---- 21 D3 Step 4: re-check endpoint-source cardinality over the MERGED set ----
    // A CDS-supplied cluster could itself be a malformed `type: EDS` carrying an
    // inline `load_assignment` (the CDS file bypasses `parse_bootstrap`'s
    // endpoint-source check). Runs AFTER the CDS merge but BEFORE the EDS pass
    // populates anything — so a CDS cluster's bad endpoint-source state fails
    // here (`LoadAssignmentOnEdsCluster`) and the populated `(Eds, Some, Some)`
    // post-merge shape (which `validate_cluster` tolerates) is not yet present.
    // `check_endpoint_sources` walks `all_clusters()` (static + dynamic-CDS).
    bootstrap::check_endpoint_sources(bootstrap)?;

    // ---- 20 D3: RDS pass — load + name-select + populate route_config ----
    // Walk every HCM across the EFFECTIVE listener set (static + dynamic-LDS).
    // The HCMs must be MUTATED (route_config populated), so a disjoint two-field
    // mutable borrow is taken (rather than the immutable `all_listeners()`).
    // The borrow ends when the loop completes; `had_rds_hcm` is a Copy `bool`,
    // so nothing into `bootstrap` is held past the loop. CDS merged FIRST, so an
    // RDS route to a CDS cluster resolves at the post-merge re-validation; a
    // route to a cluster in NEITHER list is fatal there (L7 UnknownCluster).
    let (static_listeners, dynamic_listeners) = (
        &mut bootstrap.static_resources.listeners,
        &mut bootstrap.dynamic_listeners,
    );
    let mut had_rds_hcm = false;
    for listener in static_listeners
        .iter_mut()
        .chain(dynamic_listeners.iter_mut().flatten())
    {
        for chain in &mut listener.filter_chains {
            for filter in &mut chain.filters {
                let Some(bootstrap::TypedConfig::HttpConnectionManager(hcm)) =
                    filter.typed_config.as_mut()
                else {
                    continue;
                };
                let Some(rds) = hcm.rds.as_ref() else {
                    continue;
                };
                had_rds_hcm = true;
                let path = rds.config_source.path_config_source.path.clone();
                let name = rds.route_config_name.clone();
                let contents =
                    std::fs::read_to_string(&path).map_err(|source| ConfigError::RdsFileError {
                        path: path.clone(),
                        source,
                    })?;
                let mut parsed = rds::parse_rds_file(&path, &contents)?;
                // L6: route_config_name must name a resource in the file.
                let selected = parsed
                    .iter()
                    .position(|rc| rc.name == name)
                    .map(|i| parsed.remove(i))
                    .ok_or(ConfigError::RdsRouteConfigNotFound { name, path })?;
                // §5.3: populate route_config for the uniform downstream shape.
                hcm.route_config = Some(selected);
            }
        }
    }

    // ---- 21 D3: EDS pass (ADR-0053/0054; §6.2 L1/L4/L8; §5.7) ----
    // Walk every cluster across the EFFECTIVE set (static + CDS-merged dynamic);
    // for each `type: EDS` cluster, read its file, name-select the
    // ClusterLoadAssignment by service_name-or-cluster-name (L8), and populate
    // the effective `load_assignment` (§5.3 uniform downstream shape). Runs AFTER
    // the CDS merge so a CDS-supplied cluster that is ALSO type: EDS gets its
    // endpoints loaded (composition-ready; §4 defers the bilateral fixture). The
    // post-merge validate() below re-validates the populated endpoints. NO
    // dynamic_resources block is required — a purely-static EDS cluster (fixture
    // 0029) triggers this pass too (C16). The split-borrow of the static +
    // dynamic cluster collections ends with the loop; `had_eds_cluster` is a
    // Copy `bool`, so the `&mut` borrow is released before `validate(bootstrap)`.
    let mut had_eds_cluster = false;
    {
        let (static_clusters, dynamic_clusters) = (
            &mut bootstrap.static_resources.clusters,
            &mut bootstrap.dynamic_clusters,
        );
        for cluster in static_clusters
            .iter_mut()
            .chain(dynamic_clusters.iter_mut().flatten())
        {
            if cluster.cluster_type != ClusterType::Eds {
                continue;
            }
            had_eds_cluster = true;
            let eds = cluster
                .eds_cluster_config
                .as_ref()
                .expect("EDS cluster has eds_cluster_config — validated at parse");
            let path = eds.eds_config.path_config_source.path.clone();
            // L8: service_name-or-cluster-name selection key.
            let select_name = eds
                .service_name
                .clone()
                .unwrap_or_else(|| cluster.name.clone());
            let contents =
                std::fs::read_to_string(&path).map_err(|source| ConfigError::EdsFileError {
                    path: path.clone(),
                    source,
                })?;
            let mut parsed = eds::parse_eds_file(&path, &contents)?;
            let selected = parsed
                .iter()
                .position(|la| la.cluster_name == select_name)
                .map(|i| parsed.remove(i))
                .ok_or(ConfigError::EdsClusterLoadAssignmentNotFound {
                    name: select_name,
                    path,
                })?;
            // §5.3: populate load_assignment for the uniform downstream shape.
            cluster.load_assignment = Some(selected);
        }
    }

    // ---- §5.7: ONE post-merge re-validation after ALL merges + the RDS + EDS passes ----
    // cds_configured_but_unloaded() / lds_configured_but_unloaded() are now
    // false (both side-fields are Some when their config source is set), so the
    // deferred cluster-reference checks (UnknownCluster + the H2-from-H1 gate)
    // and the deferred NoRuntime gate re-enforce against the full effective
    // state. A dynamic-listener route may reference a dynamic cluster (resolved
    // because CDS merged above); a reference to a cluster in NEITHER list is
    // fatal (the L6 recorded divergence vs Envoy's runtime-503). The gate also
    // fires for `had_eds_cluster` (C16) so a purely-static EDS bootstrap — which
    // has NO dynamic_resources/rds — still re-validates its now-populated
    // (EDS-loaded) `load_assignment` (e.g. empty-endpoints → EmptyClusterEndpoints).
    if bootstrap.dynamic_clusters.is_some()
        || bootstrap.dynamic_listeners.is_some()
        || had_rds_hcm
        || had_eds_cluster
    {
        bootstrap::validate(bootstrap)?;
    }
    Ok(())
}
