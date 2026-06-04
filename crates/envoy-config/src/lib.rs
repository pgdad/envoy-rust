#![forbid(unsafe_code)]

//! Phase 01 config surface for envoy-rust. Owns the `Bootstrap` type tree and
//! the `parse_bootstrap` entrypoint. See `docs/envoy-rust/DECISIONS.md`
//! ADR-0008 for the extraction rationale.

pub mod bootstrap;
pub mod cds;
pub mod lds;
pub mod matcher;

pub use bootstrap::{
    AccessLog, AccessLogTypedConfig, Action, Address, Admin, AppendAction, AttemptOutcome,
    Bootstrap, CertificateValidationContext, CircuitBreakers, Cluster, ClusterType, CodecType,
    CommonLbConfig, CommonTlsContext, ConfigSource, DataSource, DenominatorType, DirectResponse,
    DnsLookupFamily, DownstreamTlsContext, DynamicResources, Endpoint, ExplicitHttpConfig,
    FaultAbort, FaultConfig, FileAccessLog, FilterChain, FilterChainMatch, FractionalPercent,
    HeaderMatcher, HeaderMatcherMode, HeaderMutationConfig, HeaderMutationEntry, HeaderValue,
    HeaderValueOption, HealthCheck, Http1ProtocolOptions, Http2ProtocolOptions,
    HttpConnectionManagerConfig, HttpFilter, HttpFilterTypedConfig, HttpHealthCheck,
    HttpProtocolOptions, HttpStatus, Int64Range, LbEndpoint, LbPolicy, Listener, LoadAssignment,
    LocalRateLimitConfig, LocalityLbEndpoints, Mutations, NetworkFilter, Node, OutlierDetection,
    PathConfigSource, Percent, Permission, PermissionSet, Policy, Principal, PrincipalSet,
    RbacConfig, Rds, RetryConfig, RetryOn, RetryPolicy, Route, RouteAction, RouteAction_Route,
    RouteConfiguration, RouteMatch, RouterConfig, RoutingPriority, Rules, SafeRegex, SocketAddress,
    StaticResources, StringMatcher, StringMatcherMode, TcpProxyConfig, Thresholds, TlsCertificate,
    TokenBucket, TransportSocket, TransportSocketTypedConfig, TypedConfig,
    TypedExtensionProtocolOptions, UpstreamTlsContext, VirtualHost, parse_duration,
};
pub use cds::parse_cds_file;
pub use lds::parse_lds_file;

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

    /// 10: RBAC filter has no policies (rules.policies is empty).
    #[error("HCM listener {listener:?}: RBAC filter has no policies (rules.policies is empty)")]
    EmptyRbacPolicies { listener: String },

    /// 10: RBAC policy has no permissions.
    #[error("HCM listener {listener:?}: RBAC policy {policy_name:?} has no permissions")]
    EmptyRbacPolicyPermissions {
        listener: String,
        policy_name: String,
    },

    /// 10: RBAC policy has no principals.
    #[error("HCM listener {listener:?}: RBAC policy {policy_name:?} has no principals")]
    EmptyRbacPolicyPrincipals {
        listener: String,
        policy_name: String,
    },

    /// 10: RBAC policy has an empty Permission set
    /// (`Permission::AndRules` or `Permission::OrRules` with empty `rules`).
    #[error(
        "HCM listener {listener:?}: RBAC policy {policy_name:?} has an empty Permission set at {path}"
    )]
    EmptyRbacPermissionSet {
        listener: String,
        policy_name: String,
        path: String,
    },

    /// 10: RBAC policy has an empty Principal set
    /// (`Principal::AndIds` or `Principal::OrIds` with empty `ids`).
    #[error(
        "HCM listener {listener:?}: RBAC policy {policy_name:?} has an empty Principal set at {path}"
    )]
    EmptyRbacPrincipalSet {
        listener: String,
        policy_name: String,
        path: String,
    },

    /// 10: RBAC policy Permission/Principal tree exceeds RBAC_TREE_MAX_DEPTH.
    /// Defense-in-depth bound at parse time; the runtime evaluator inherits it.
    #[error(
        "HCM listener {listener:?}: RBAC policy {policy_name:?} Permission/Principal tree exceeds RBAC_TREE_MAX_DEPTH ({depth} > 16)"
    )]
    RbacTreeTooDeep {
        listener: String,
        policy_name: String,
        depth: u32,
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

    /// 12.1: cluster has more than one `health_checks` entry (phase-12 supports 0 or 1).
    #[error(
        "cluster '{cluster}' has more than one health_checks entry; phase 12 supports at most one"
    )]
    UnsupportedMultipleHealthChecks { cluster: String },

    /// 12.1: cluster's health check has no `http_health_check` (TCP/gRPC/custom defer).
    #[error(
        "cluster '{cluster}' health check is not an http_health_check; phase 12 supports HTTP health checks only"
    )]
    UnsupportedHealthCheckType { cluster: String },

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

    /// 12.1: `http_health_check.path` is empty.
    #[error("cluster '{cluster}' http_health_check.path must be non-empty")]
    EmptyHealthCheckPath { cluster: String },

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
}

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap, ConfigError> {
    let mut bootstrap: Bootstrap = serde_yaml::from_str(yaml)?;
    // 20 D1 (C16): the exactly-one-of route-source check runs here, before any
    // file is read and before validate(), so an `rds` HCM (route_config: None)
    // survives validate()'s inline-route walk (which early-returns on None).
    bootstrap::check_route_sources(&bootstrap)?;
    bootstrap::validate(&mut bootstrap)?;
    Ok(bootstrap)
}

/// 18 D3 / 19 D3: read + parse + merge the CDS and LDS files (ADR-0048/0049/0050).
/// Called by envoy-bin AFTER `parse_bootstrap` and BEFORE any consumer iterates
/// clusters or listeners. No-op when neither `dynamic_resources.cds_config` nor
/// `dynamic_resources.lds_config` is configured. ALL failures are fatal (the L4
/// reconciliation — envoy-rust never warn-and-serves a broken CDS/LDS file;
/// recorded divergence vs Envoy, BEHAVIOR_CONTRACT).
///
/// §5.7 merge ordering: the CDS branch merges dynamic CLUSTERS into
/// `dynamic_clusters` BEFORE the single post-merge re-validation, so that when
/// that re-validation checks a dynamic LISTENER's route→cluster references, the
/// dynamic cluster is already visible via `all_clusters()`. The LDS branch then
/// merges dynamic LISTENERS, and exactly ONE `bootstrap::validate()` runs after
/// BOTH branches — never one validate per branch.
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

    // ---- §5.7: ONE post-merge re-validation after BOTH merges ----
    // cds_configured_but_unloaded() / lds_configured_but_unloaded() are now
    // false (both side-fields are Some when their config source is set), so the
    // deferred cluster-reference checks (UnknownCluster + the H2-from-H1 gate)
    // and the deferred NoRuntime gate re-enforce against the full effective
    // state. A dynamic-listener route may reference a dynamic cluster (resolved
    // because CDS merged above); a reference to a cluster in NEITHER list is
    // fatal (the L6 recorded divergence vs Envoy's runtime-503).
    if bootstrap.dynamic_clusters.is_some() || bootstrap.dynamic_listeners.is_some() {
        bootstrap::validate(bootstrap)?;
    }
    Ok(())
}
