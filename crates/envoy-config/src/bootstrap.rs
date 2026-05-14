//! Bootstrap schema — the phase-01 `envoy.yaml` surface. See SPEC §D1 and
//! ADR-0008. All structs derive `Debug` + `Deserialize` and carry
//! `#[serde(deny_unknown_fields)]` except `Node`, which is deliberately open
//! (SPEC §D1 inline comment).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    #[serde(default)]
    pub node: Option<Node>,
    #[serde(default)]
    pub admin: Option<Admin>,
    #[serde(default)]
    pub static_resources: StaticResources,
}

// NOTE: Node deliberately omits `deny_unknown_fields`. Upstream Envoy's Node
// also carries metadata, locality, user_agent_*, extensions, client_features,
// listening_addresses, dynamic_parameters. Phase 01 accepts id + cluster and
// silently ignores the rest. When xDS (§9 family) lands, Node is either moved
// or tightened under a new ADR that names the fields then semantically
// load-bearing. (See SPEC §6 signpost 8.)
#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    pub cluster: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admin {
    pub address: Address,

    /// 06.1 NEW (per ADR-0026 parse-and-ignore pattern; SPEC §3 D5.a).
    /// Optional admin-side access log path; envoy-rust does not inspect or
    /// honor this field. Stored so fixtures with upstream Envoy admin
    /// configs that include it round-trip cleanly through the parser.
    /// Admin-side access logging defers indefinitely from 06.1.
    #[serde(default)]
    pub access_log_path: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticResources {
    #[serde(default)]
    pub listeners: Vec<Listener>,
    #[serde(default)]
    pub clusters: Vec<Cluster>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cluster {
    pub name: String,
    #[serde(rename = "type")]
    pub cluster_type: ClusterType,
    pub lb_policy: LbPolicy,
    pub load_assignment: LoadAssignment,
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    /// 05.4 NEW per ADR-0024: optional DNS lookup family override for
    /// STRICT_DNS / LOGICAL_DNS clusters. Defaults to None, which lets
    /// the upstream Envoy honor its proto default (AUTO). envoy-rust does
    /// NOT consume this field at runtime in 05.4; only the upstream Envoy
    /// side observes the V4_ONLY knob via per-fixture envoy.yaml edits
    /// (D2 of phase 05.4 — see SPEC §3 D2).
    #[serde(default)]
    pub dns_lookup_family: Option<DnsLookupFamily>,
    /// 05.3 NEW per SPEC §3 D2.a: cluster-side typed_extension_protocol_options
    /// carrying the upstreams.http.v3.HttpProtocolOptions extension. Defaults
    /// to None, which projects to UpstreamProtocol::Http1 at envoy-cluster
    /// from_bootstrap time (envoy-cluster Task 5) — backwards-compat with all
    /// phase-04 clusters. The validator enforces:
    ///   - @type URL literal "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions"
    ///   - mutual exclusion of explicit_http_config arms
    ///   - RFC 7540 range checks on http2_protocol_options (delegated to
    ///     validate_http2_protocol_options_ranges; same checks as listener-side).
    #[serde(default)]
    pub typed_extension_protocol_options: Option<TypedExtensionProtocolOptions>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ClusterType {
    /// Static cluster type — endpoints' `address` fields are literal IPs
    /// (parsed via `SocketAddr::from_str` at cluster-build time in
    /// `envoy-cluster::from_bootstrap`).
    Static,
    /// STRICT_DNS cluster type — endpoints' `address` fields are DNS names
    /// (resolved via `tokio::net::lookup_host` at cluster-build time in
    /// `envoy-cluster::from_bootstrap`; the resolved `SocketAddr`s are
    /// cached for the cluster's lifetime, matching Envoy v1.33's STRICT_DNS
    /// semantics with default `dns_refresh_rate`). 05.1 NEW per ADR-0023;
    /// `LOGICAL_DNS` deferred to a later phase.
    StrictDns,
}

/// DNS lookup family for STRICT_DNS / LOGICAL_DNS clusters. Mirrors Envoy
/// v1.33's `Cluster.DnsLookupFamily` proto enum (3 variants: V4_ONLY /
/// V6_ONLY / AUTO; v1.33 does not have V4_PREFERRED or ALL — those land
/// in later Envoy versions). 05.4 NEW per ADR-0024; parsed-and-stored
/// only — envoy-rust's `tokio::net::lookup_host` resolution path returns
/// the system-stack default and does NOT filter by family at runtime.
/// Whichever later phase needs the runtime filter lands it then.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum DnsLookupFamily {
    V4Only,
    V6Only,
    Auto,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LbPolicy {
    RoundRobin,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoadAssignment {
    pub cluster_name: String,
    pub endpoints: Vec<LocalityLbEndpoints>,
}

/// Cluster-side typed_extension_protocol_options (Envoy's mechanism for
/// per-cluster protocol-extension config). 05.3 NEW per SPEC §3 D2.a.
/// The single recognized key is the upstreams.http.v3.HttpProtocolOptions
/// extension; the validator additionally rejects unknown @type URLs and
/// mutually-exclusive explicit_http_config arms.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TypedExtensionProtocolOptions {
    #[serde(rename = "envoy.extensions.upstreams.http.v3.HttpProtocolOptions")]
    pub http_protocol_options: HttpProtocolOptions,
}

/// The upstreams.http.v3.HttpProtocolOptions typed-extension. Carries the
/// `@type` URL (validated literal) + the `explicit_http_config` oneof.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpProtocolOptions {
    #[serde(rename = "@type")]
    pub type_url: String,
    pub explicit_http_config: ExplicitHttpConfig,
}

/// Envoy's `ExplicitHttpConfig` is a oneof: either http_protocol_options
/// (H1 arm; empty in 05.3 — see Http1ProtocolOptions) or
/// http2_protocol_options (H2 arm; reuses 05.2 D2.b's Http2ProtocolOptions
/// unchanged). The validator (`validate` fn) enforces mutual
/// exclusion via ConfigError::MutuallyExclusiveExplicitHttpConfig.
#[derive(Debug, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct ExplicitHttpConfig {
    #[serde(default)]
    pub http_protocol_options: Option<Http1ProtocolOptions>,
    #[serde(default)]
    pub http2_protocol_options: Option<Http2ProtocolOptions>,
}

/// H1 arm of ExplicitHttpConfig. Empty in 05.3; future fields like
/// chunk_encoding / allow_chunked_length / enable_trailers defer per
/// SPEC §4 to whichever phase first needs cluster-side H1 protocol-tuning.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Http1ProtocolOptions {}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalityLbEndpoints {
    pub lb_endpoints: Vec<LbEndpoint>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LbEndpoint {
    pub endpoint: Endpoint,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub address: Address,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    #[allow(dead_code)]
    pub name: String,
    pub address: Address,
    pub filter_chains: Vec<FilterChain>,
    /// 05.4 NEW per ADR-0026: optional listener filters declared by the
    /// upstream Envoy `envoy.yaml`. Parse-and-ignore: stored as opaque
    /// `serde_yaml::Value`s; envoy-rust does NOT execute listener filters
    /// (SNI dispatch lives at the rustls layer per phase 03.2). The field
    /// is accepted purely so envoy.yaml fixtures including a
    /// `listener_filters: [...]` block do not trigger `deny_unknown_fields`
    /// rejection on any path that parses envoy.yaml through envoy-config.
    #[serde(default)]
    pub listener_filters: Vec<serde_yaml::Value>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub socket_address: SocketAddress,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SocketAddress {
    pub address: String,
    pub port_value: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    #[serde(default)]
    pub filter_chain_match: Option<FilterChainMatch>,
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    pub filters: Vec<NetworkFilter>,
}

/// Filter-chain matcher (phase 03.2 portion). Selects which filter chain a
/// connection routes to; for phase 03.2, only `server_names` (TLS SNI) is
/// supported. Empty / missing `server_names` is the catch-all (validator
/// enforces "at most one catch-all per listener").
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChainMatch {
    /// SNI values this filter chain matches. Empty Vec = catch-all. The
    /// validator (Task 2) rejects two filter chains declaring the same SNI
    /// (case-insensitive) and rejects multiple catch-all chains per listener.
    #[serde(default)]
    pub server_names: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkFilter {
    pub name: String,
    #[serde(default)]
    pub typed_config: Option<TypedConfig>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy")]
    TcpProxy(TcpProxyConfig),
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager"
    )]
    HttpConnectionManager(HttpConnectionManagerConfig),
}

// 06.2 Task 5 — access-log schema additions per SPEC §3 D2.2.

/// AccessLog — one entry in an HCM's `access_log:` block. The shape mirrors
/// Envoy's `envoy.config.accesslog.v3.AccessLog`: `name` selects the logger
/// extension (only `envoy.access_loggers.file` is accepted in 06.2; the
/// validator rejects anything else) and `typed_config` carries the
/// extension-specific payload via a `@type`-tagged enum. Future
/// observability-family phases extend `AccessLogTypedConfig` rather than
/// reshaping `AccessLog`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AccessLog {
    pub name: String,
    pub typed_config: AccessLogTypedConfig,
}

/// AccessLogTypedConfig — the `@type`-tagged envelope for an AccessLog
/// entry's `typed_config`. Single variant in 06.2 (file access logger);
/// the enum exists so future observability phases can add stdout / gRPC /
/// OpenTelemetry loggers without reshaping the schema. Unknown `@type`
/// URLs are rejected by serde at deserialization time (surfaces as
/// `ConfigError::Yaml`); see `rejects_hcm_with_unsupported_access_log_type_url`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum AccessLogTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog")]
    FileAccessLog(FileAccessLog),
}

/// FileAccessLog — typed_config payload for the file access logger. 06.2
/// consumes only `path`; format-string customization (`log_format`,
/// `json_format`, …) is OUT of scope per parent-06 SPEC §4 + 06.2 SPEC §4
/// (the emitter uses the default Envoy v3 format string). Empty paths
/// are rejected by the validator (`ConfigError::InvalidAccessLogPath`).
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileAccessLog {
    pub path: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TcpProxyConfig {
    /// Required by Envoy for access-log attribution; accepted by envoy-rust and
    /// unused until phase 06 (access logs). Carrying it through the parser now
    /// keeps fixture YAMLs identical across upstream-Envoy and envoy-rust.
    pub stat_prefix: String,
    pub cluster: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransportSocket {
    /// Phase 03 accepts only `"envoy.transport_sockets.tls"`; the validator
    /// rejects any other name. Future phases may add raw_buffer / quic / etc.
    pub name: String,
    pub typed_config: TransportSocketTypedConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TransportSocketTypedConfig {
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext"
    )]
    Downstream(DownstreamTlsContext),
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext"
    )]
    Upstream(UpstreamTlsContext),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DownstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
    /// Server Name sent in the ClientHello server_name extension. Phase 03
    /// requires this on every UpstreamTlsContext (no auto_sni). The validator
    /// rejects an empty string.
    pub sni: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonTlsContext {
    #[serde(default)]
    pub tls_certificates: Vec<TlsCertificate>,
    #[serde(default)]
    pub validation_context: Option<CertificateValidationContext>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TlsCertificate {
    pub certificate_chain: DataSource,
    pub private_key: DataSource,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CertificateValidationContext {
    pub trusted_ca: DataSource,
}

/// Phase 03 supports `filename` only. Phase 04.1 adds `inline_string` for the
/// HCM `direct_response.body` use-case (small inline payloads). `inline_bytes`,
/// `environment_variable`, and `secret_ref` are deferred to later phases.
///
/// Schema-level both fields are `Option<String>` with `#[serde(default)]`. The
/// "exactly one of {filename, inline_string} is `Some`" invariant — and any
/// per-callsite restriction (e.g. TLS still requires `filename`) — is enforced
/// by the validator (Task 2 of phase 04.1), not by serde.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub inline_string: Option<String>,
}

/// HCM (HTTP Connection Manager) typed-config. Phase 04.1 carries the minimal
/// subset needed for the `direct_response` happy path: stat_prefix, codec_type,
/// route_config, http_filters. Upstream Envoy's HCM has dozens more fields
/// (access_log, tracing, http_protocol_options, idle_timeout, ...); all
/// deferred per SPEC §4.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectionManagerConfig {
    pub stat_prefix: String,
    pub codec_type: CodecType,

    /// 05.2 NEW: listener-side HTTP/2 protocol tuning (per SPEC §3 D2.b).
    /// Optional; absent means "use h2-crate defaults". Validator runs the
    /// RFC 7540 range checks at parse time only when `Some`.
    #[serde(default)]
    pub http2_protocol_options: Option<Http2ProtocolOptions>,

    /// 06.2 NEW: per-listener access-log entries. `#[serde(default)]` is
    /// load-bearing because `HttpConnectionManagerConfig` carries
    /// `#[serde(deny_unknown_fields)]` and the 5 existing HCM-bearing
    /// fixtures (0007/0008/0009/0010/0011) do not declare an `access_log:`
    /// block — without the default they would fail to parse. The
    /// validator (`validate_access_logs`) rejects non-file loggers
    /// (`UnsupportedAccessLogType`) and empty paths
    /// (`InvalidAccessLogPath`).
    #[serde(default)]
    pub access_log: Vec<AccessLog>,

    pub route_config: RouteConfiguration,
    pub http_filters: Vec<HttpFilter>,
}

/// HCM codec_type. Phase 04.1 wire-supports HTTP1 only (Task 10's HCM rejects
/// the others at construction time); AUTO/HTTP2/HTTP3 parse but do not yet
/// dispatch.
#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum CodecType {
    AUTO,
    HTTP1,
    HTTP2,
    HTTP3,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpFilter {
    pub name: String,
    pub typed_config: HttpFilterTypedConfig,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum HttpFilterTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router")]
    Router(RouterConfig),

    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation"
    )]
    HeaderMutation(HeaderMutationConfig),
}

/// Empty in 04.1; Envoy's Router has many fields (suppress_envoy_headers,
/// dynamic_stats, start_child_span, ...); all deferred per SPEC §4.
#[derive(Debug, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct RouterConfig {}

/// `envoy.extensions.filters.http.header_mutation.v3.HeaderMutation` config.
/// The HeaderMutation filter appends/overwrites request and response headers.
/// Phase 07.2.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationConfig {
    pub mutations: Mutations,
}

/// The request-side and response-side mutation lists. Both default to empty
/// (`mutations: {}` is legal — a no-op filter).
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mutations {
    #[serde(default)]
    pub request_mutations: Vec<HeaderMutationEntry>,
    #[serde(default)]
    pub response_mutations: Vec<HeaderMutationEntry>,
}

/// One mutation entry. Envoy's proto is a `oneof` (append / remove); 07.2
/// supports only `append`. `#[serde(deny_unknown_fields)]` rejects `remove`
/// (and any other oneof arm) at parse time.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationEntry {
    pub append: HeaderValueOption,
}

/// `HeaderValueOption` — a header key/value plus the append action.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderValueOption {
    pub header: HeaderValue,
    pub append_action: AppendAction,
}

/// `HeaderValue` — the literal header key + value.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderValue {
    pub key: String,
    pub value: String,
}

/// `AppendAction` — Envoy's wire form uses SCREAMING_SNAKE_CASE. 07.2 supports
/// `APPEND_IF_EXISTS_OR_ADD` + `OVERWRITE_IF_EXISTS_OR_ADD` at runtime; the two
/// unsupported variants parse at the schema level so serde does not emit a
/// generic "unknown variant" error — the Task 2 validator rejects them with the
/// typed `ConfigError::UnsupportedHeaderMutationAppendAction` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppendAction {
    AppendIfExistsOrAdd,
    OverwriteIfExistsOrAdd,
    AddIfAbsent,
    OverwriteIfExists,
}

/// HTTP/2 protocol-level tuning knobs, listener-side. Subset of Envoy's
/// `envoy.config.core.v3.Http2ProtocolOptions`. Phase 05.2 ships 4 optional
/// `u32` fields per parent-05 SPEC §6 signpost 2; further fields
/// (allow_connect, allow_metadata, hpack_table_size,
/// override_stream_error_on_invalid_http_message, connection_keepalive, ...)
/// default to RFC-conformant values via the `h2` crate and defer until a
/// fixture or h2spec test forces them. Validator-checked range constraints
/// per RFC 7540 §6.5.2 / §6.9.1 / §6.9.2 land in `validate_hcm`.
#[derive(Debug, Default, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Http2ProtocolOptions {
    /// SETTINGS_MAX_CONCURRENT_STREAMS. h2-crate default: 100. No upper bound
    /// per RFC 7540; zero is valid (peer would refuse all stream creation).
    #[serde(default)]
    pub max_concurrent_streams: Option<u32>,

    /// SETTINGS_INITIAL_WINDOW_SIZE. h2-crate default: 65535. Range
    /// [0, 2^31 - 1] per RFC 7540 §6.9.2.
    #[serde(default)]
    pub initial_stream_window_size: Option<u32>,

    /// Connection-level initial window size. h2-crate default: 65535. Range
    /// [0, 2^31 - 1] per RFC 7540 §6.9.1.
    #[serde(default)]
    pub initial_connection_window_size: Option<u32>,

    /// SETTINGS_MAX_FRAME_SIZE. h2-crate default: 16384. Range
    /// [16384, 16777215] per RFC 7540 §6.5.2.
    #[serde(default)]
    pub max_frame_size: Option<u32>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteConfiguration {
    pub name: String,
    pub virtual_hosts: Vec<VirtualHost>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VirtualHost {
    pub name: String,
    pub domains: Vec<String>,
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// The match predicate (path + headers; populated by 04.1 + 04.2).
    pub r#match: RouteMatch,

    /// 04.3 NEW: the action to dispatch on a matched request.
    pub action: RouteAction,
}

/// 04.3 NEW (under SPEC §3 D2): the action variant a route's HCM router
/// invocation dispatches into. Discrimination is by field-name oneof at the
/// route map level — the route's peer keys are `direct_response: { ... }` OR
/// `route: { ... }`, not nested under a single `action:` key. The hand-rolled
/// `impl<'de> Deserialize` for `Route` (below) detects which peer key is
/// present and constructs the matching variant; both-present and
/// neither-present are errors.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteAction {
    /// Direct-response action — write a static body downstream. Phase 04.1 carryover.
    DirectResponse(DirectResponse),

    /// Route-to-cluster action — proxy through to the named cluster. Phase 04.3 NEW.
    Route(RouteAction_Route),
}

/// 04.3 NEW (under SPEC §3 D2). Names the cluster to forward the matched
/// request to. Future route-action knobs (timeout, retries, weighted clusters,
/// host-rewrite, header manipulations) are deferred (SPEC §4 non-goals).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types)]
pub struct RouteAction_Route {
    pub cluster: String,
}

/// 04.3 NEW: hand-rolled because Envoy's `Route` schema uses a field-name
/// oneof for the action variant — `direct_response: { ... }` and `route: { ... }`
/// are peers of `match: { ... }` at the same map level, not nested under a
/// shared discriminator key. `#[serde(tag = "...")]` doesn't model field-name
/// discrimination, and `#[serde(untagged)]` would silently pick the first
/// parsing variant. Mirrors the 04.2 HeaderMatcher visitor pattern.
impl<'de> serde::Deserialize<'de> for Route {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Route;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a Route map with `match` and exactly one of `direct_response` or `route`",
                )
            }

            fn visit_map<M>(self, mut map: M) -> Result<Route, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut r#match: Option<RouteMatch> = None;
                let mut direct_response: Option<DirectResponse> = None;
                let mut route_action: Option<RouteAction_Route> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "match" => {
                            if r#match.is_some() {
                                return Err(M::Error::duplicate_field("match"));
                            }
                            r#match = Some(map.next_value::<RouteMatch>()?);
                        }
                        "direct_response" => {
                            if direct_response.is_some() {
                                return Err(M::Error::duplicate_field("direct_response"));
                            }
                            direct_response = Some(map.next_value::<DirectResponse>()?);
                        }
                        "route" => {
                            if route_action.is_some() {
                                return Err(M::Error::duplicate_field("route"));
                            }
                            route_action = Some(map.next_value::<RouteAction_Route>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(
                                other,
                                &["match", "direct_response", "route"],
                            ));
                        }
                    }
                }

                let r#match = r#match.ok_or_else(|| M::Error::missing_field("match"))?;
                let action = match (direct_response, route_action) {
                    (Some(_), Some(_)) => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response` or `route`; \
                             both are present",
                        ));
                    }
                    (None, None) => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response` or `route`; \
                             neither is present",
                        ));
                    }
                    (Some(dr), None) => RouteAction::DirectResponse(dr),
                    (None, Some(ar)) => RouteAction::Route(ar),
                };

                Ok(Route { r#match, action })
            }
        }

        deserializer.deserialize_map(V)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteMatch {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponse {
    pub status: u16,
    pub body: DataSource,
}

/// Half-open i64 range. Validator rejects start >= end with
/// ConfigError::InvalidInt64Range. Phase 04.2.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Int64Range {
    pub start: i64,
    pub end: i64,
}

/// Reference to a regex pattern. Held both as the original String (for
/// re-serialization / equality / debugging) and the compiled Arc<regex::Regex>
/// (for cheap clone + zero-cost matching). The compiled form is *not* a serde
/// field; it's filled in by the envoy-config validator after deserialization.
/// Phase 04.2 (under ADR-0021).
///
/// PartialEq compares only the `regex: String` field. The compiled regex has
/// no stable equality (regex::Regex doesn't impl PartialEq), and PartialEq is
/// useful for assert_eq! shape comparisons in tests where pre-validate values
/// (compiled == None) and post-validate values (compiled == Some) should be
/// considered equal if they came from the same pattern.
#[derive(Debug, Clone)]
pub struct SafeRegex {
    pub regex: String,
    /// Filled in by the validator (`crate::bootstrap::validate`). At
    /// deserialization time this is None; after a successful validate() call
    /// it's Some(Arc<regex::Regex>). Consumers (the route walker in HCM via
    /// HeaderMatcher::matches) take the .as_ref().expect("validator ensured
    /// compiled") shape, mirroring phase 02.1's "validator ensured cluster
    /// present" precedent.
    pub compiled: Option<std::sync::Arc<regex::Regex>>,
}

impl PartialEq for SafeRegex {
    fn eq(&self, other: &Self) -> bool {
        self.regex == other.regex
    }
}

/// Hand-rolled Deserialize: only reads `regex: String`; sets `compiled: None`.
/// The validator extension (Task 5) fills the compiled form.
///
/// The hand-rolled form (rather than `#[derive(Deserialize)] +
/// `#[serde(skip, default)]`) is chosen to establish the visitor pattern that
/// Tasks 3 and 4 reuse for `StringMatcher` and `HeaderMatcher` field-name
/// oneof discrimination — where `#[serde(untagged)]` would silently pick the
/// first parsing variant and `#[serde(tag = "...")]` only models a fixed
/// discriminator-key shape. Landing the visitor pattern here establishes the
/// template + makes the two-phase init contract explicit: this visitor always
/// produces `compiled: None`; the validator fills it.
impl<'de> serde::Deserialize<'de> for SafeRegex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = SafeRegex;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a SafeRegex map with a `regex: String` field")
            }
            fn visit_map<M>(self, mut map: M) -> Result<SafeRegex, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut regex: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "regex" => {
                            if regex.is_some() {
                                return Err(M::Error::duplicate_field("regex"));
                            }
                            regex = Some(map.next_value::<String>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, &["regex"]));
                        }
                    }
                }
                let regex = regex.ok_or_else(|| M::Error::missing_field("regex"))?;
                Ok(SafeRegex {
                    regex,
                    compiled: None,
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}

/// Envoy's modern generic StringMatcher (proto:
/// `envoy.type.matcher.v3.StringMatcher`). Field-name oneof shape: the
/// discriminator is *which* of `exact` / `prefix` / `suffix` / `safe_regex` /
/// `contains` is the present key. `ignore_case` is a peer of the mode key
/// (not a per-variant field) controlling case sensitivity of the value match.
/// Defaults to false. Has no effect on the SafeRegex variant per Envoy proto
/// (regex callers express case insensitivity via the `(?i)` inline flag).
/// Phase 04.2.
#[derive(Debug, Clone, PartialEq)]
pub struct StringMatcher {
    pub mode: StringMatcherMode,
    pub ignore_case: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringMatcherMode {
    /// `exact: <string>`.
    Exact(String),
    /// `prefix: <string>`.
    Prefix(String),
    /// `suffix: <string>`.
    Suffix(String),
    /// `safe_regex: { regex: "<pattern>" }`.
    SafeRegex(SafeRegex),
    /// `contains: <string>` — substring match. Only reachable through
    /// HeaderMatcherMode::StringMatch(StringMatcher::Contains(...)); there is
    /// no top-level HeaderMatcherMode::ContainsMatch (Envoy v1.33.0 only
    /// supports Contains via the modern string_match field; SPEC §6 signpost 8).
    Contains(String),
}

/// Hand-rolled Deserialize for the field-name oneof shape — same template
/// Task 2 established for `SafeRegex`. Tasks 3 and 4 share this approach
/// because `#[serde(untagged)]` would silently pick the first parsing variant
/// and `#[serde(tag = "...")]` only models a fixed discriminator-key shape;
/// neither fits Envoy's "exactly one of N mode keys" semantics.
impl<'de> serde::Deserialize<'de> for StringMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        const MODE_KEYS: &[&str] = &["exact", "prefix", "suffix", "safe_regex", "contains"];
        const ALL_KEYS: &[&str] = &[
            "exact",
            "prefix",
            "suffix",
            "safe_regex",
            "contains",
            "ignore_case",
        ];

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StringMatcher;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a StringMatcher map with exactly one mode key plus optional ignore_case",
                )
            }
            fn visit_map<M>(self, mut map: M) -> Result<StringMatcher, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut mode: Option<StringMatcherMode> = None;
                let mut ignore_case: Option<bool> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "exact" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Exact(map.next_value::<String>()?));
                        }
                        "prefix" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Prefix(map.next_value::<String>()?));
                        }
                        "suffix" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Suffix(map.next_value::<String>()?));
                        }
                        "safe_regex" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode =
                                Some(StringMatcherMode::SafeRegex(map.next_value::<SafeRegex>()?));
                        }
                        "contains" => {
                            if mode.is_some() {
                                return Err(M::Error::custom(
                                    "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                                ));
                            }
                            mode = Some(StringMatcherMode::Contains(map.next_value::<String>()?));
                        }
                        "ignore_case" => {
                            if ignore_case.is_some() {
                                return Err(M::Error::duplicate_field("ignore_case"));
                            }
                            ignore_case = Some(map.next_value::<bool>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, ALL_KEYS));
                        }
                    }
                }
                let mode = mode.ok_or_else(|| {
                    M::Error::custom(format!(
                        "StringMatcher: missing mode key (expected one of {MODE_KEYS:?})"
                    ))
                })?;
                Ok(StringMatcher {
                    mode,
                    ignore_case: ignore_case.unwrap_or(false),
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}

/// One header-matching predicate. AND-combined with sibling HeaderMatchers
/// in `RouteMatch.headers` per Envoy v1.33.0 default `headers_match_options:
/// ALL`. Phase 04.2.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderMatcher {
    /// Header name. Matched case-insensitively against the request's header
    /// names per HTTP/1.1 RFC 7230 §3.2. Empty string is rejected by the
    /// validator with ConfigError::EmptyHeaderName.
    pub name: String,
    /// The mode discriminator. The Envoy proto uses field-name oneof shape
    /// (the discriminator is *which* of the seven mode fields is present);
    /// serde tagged-enum doesn't directly model this, so the parsed form goes
    /// through a hand-rolled Deserialize impl that inspects the YAML mapping
    /// keys and dispatches to the matching variant. SPEC §6 signpost 1.
    pub mode: HeaderMatcherMode,
    /// If true, the entire mode-specific match result is inverted (XOR after
    /// the mode match runs, before AND-combination across sibling
    /// HeaderMatchers). SPEC §6 signpost 5.
    pub invert_match: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderMatcherMode {
    /// `exact_match: <string>` — value equals literal (case-sensitive on the
    /// value; the header name match is always case-insensitive per HTTP/1.1).
    ExactMatch(String),
    /// `prefix_match: <string>` — value starts with literal.
    PrefixMatch(String),
    /// `suffix_match: <string>` — value ends with literal.
    SuffixMatch(String),
    /// `safe_regex_match: { regex: "<pattern>" }` — value matches the regex.
    /// Compiled at config-load time into Arc<regex::Regex>; the validator
    /// rejects unparseable patterns with ConfigError::InvalidRegex.
    SafeRegexMatch(SafeRegex),
    /// `range_match: { start: <i64>, end: <i64> }` — value parses as i64
    /// (decimal) and falls in [start, end). Non-parseable values fail the
    /// match (NOT an error). SPEC §6 signpost 6.
    RangeMatch(Int64Range),
    /// `present_match: <bool>` — header presence (true) or "no presence
    /// requirement" (false; SPEC §6 signpost 7 for the subtle false semantics).
    PresentMatch(bool),
    /// `string_match: <StringMatcher>` — Envoy's modern generic tagged-union
    /// (the only path to Contains; SPEC §6 signpost 8).
    StringMatch(StringMatcher),
}

impl<'de> serde::Deserialize<'de> for HeaderMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        const MODE_KEYS: &[&str] = &[
            "exact_match",
            "prefix_match",
            "suffix_match",
            "safe_regex_match",
            "range_match",
            "present_match",
            "string_match",
        ];
        const ALL_KEYS: &[&str] = &[
            "name",
            "exact_match",
            "prefix_match",
            "suffix_match",
            "safe_regex_match",
            "range_match",
            "present_match",
            "string_match",
            "invert_match",
        ];

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = HeaderMatcher;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a HeaderMatcher map with `name`, exactly one mode key, and optional invert_match",
                )
            }
            fn visit_map<M>(self, mut map: M) -> Result<HeaderMatcher, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut name: Option<String> = None;
                let mut mode: Option<HeaderMatcherMode> = None;
                let mut invert_match: Option<bool> = None;

                fn set_mode<E: Error>(
                    slot: &mut Option<HeaderMatcherMode>,
                    new: HeaderMatcherMode,
                ) -> Result<(), E> {
                    if slot.is_some() {
                        return Err(E::custom(
                            "HeaderMatcher: multiple mode keys (each variant is mutually exclusive)",
                        ));
                    }
                    *slot = Some(new);
                    Ok(())
                }

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if name.is_some() {
                                return Err(M::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value::<String>()?);
                        }
                        "exact_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::ExactMatch(map.next_value::<String>()?),
                        )?,
                        "prefix_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::PrefixMatch(map.next_value::<String>()?),
                        )?,
                        "suffix_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::SuffixMatch(map.next_value::<String>()?),
                        )?,
                        "safe_regex_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::SafeRegexMatch(map.next_value::<SafeRegex>()?),
                        )?,
                        "range_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::RangeMatch(map.next_value::<Int64Range>()?),
                        )?,
                        "present_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::PresentMatch(map.next_value::<bool>()?),
                        )?,
                        "string_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::StringMatch(map.next_value::<StringMatcher>()?),
                        )?,
                        "invert_match" => {
                            if invert_match.is_some() {
                                return Err(M::Error::duplicate_field("invert_match"));
                            }
                            invert_match = Some(map.next_value::<bool>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, ALL_KEYS));
                        }
                    }
                }

                let name = name.ok_or_else(|| M::Error::missing_field("name"))?;
                let mode = mode.ok_or_else(|| {
                    M::Error::custom(format!(
                        "HeaderMatcher: missing mode key (expected one of {MODE_KEYS:?})"
                    ))
                })?;
                Ok(HeaderMatcher {
                    name,
                    mode,
                    invert_match: invert_match.unwrap_or(false),
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}

pub(crate) fn validate(bootstrap: &mut Bootstrap) -> Result<(), crate::ConfigError> {
    if bootstrap.static_resources.listeners.len() > 1 {
        return Err(crate::ConfigError::TooManyListeners(
            bootstrap.static_resources.listeners.len(),
        ));
    }
    if bootstrap.admin.is_none() && bootstrap.static_resources.listeners.is_empty() {
        return Err(crate::ConfigError::NoRuntime);
    }

    // Per-cluster invariants.
    for cluster in &bootstrap.static_resources.clusters {
        if cluster.load_assignment.cluster_name != cluster.name {
            return Err(crate::ConfigError::LoadAssignmentNameMismatch {
                cluster: cluster.name.clone(),
                assignment: cluster.load_assignment.cluster_name.clone(),
            });
        }
        let total_endpoints: usize = cluster
            .load_assignment
            .endpoints
            .iter()
            .map(|le| le.lb_endpoints.len())
            .sum();
        if total_endpoints == 0 {
            return Err(crate::ConfigError::EmptyClusterEndpoints(
                cluster.name.clone(),
            ));
        }
        if let Some(ts) = cluster.transport_socket.as_ref() {
            if ts.name != crate::TLS_TRANSPORT_SOCKET {
                return Err(crate::ConfigError::UnknownTransportSocketName(
                    ts.name.clone(),
                ));
            }
            match &ts.typed_config {
                TransportSocketTypedConfig::Upstream(ctx) => {
                    if !ctx.common_tls_context.tls_certificates.is_empty() {
                        return Err(crate::ConfigError::EmptyTlsCertificates { side: "cluster" });
                    }
                    let vc = ctx
                        .common_tls_context
                        .validation_context
                        .as_ref()
                        .ok_or(crate::ConfigError::MissingValidationContext)?;
                    validate_data_source(
                        &vc.trusted_ca,
                        "validation_context.trusted_ca",
                        Required::Filename,
                    )?;
                    if ctx.sni.is_empty() {
                        return Err(crate::ConfigError::EmptyUpstreamSni);
                    }
                }
                TransportSocketTypedConfig::Downstream(_) => {
                    return Err(crate::ConfigError::MismatchedTransportSocketDirection {
                        side: "cluster",
                        got: "DownstreamTlsContext",
                    });
                }
            }
        }
        // 05.3 NEW per SPEC §3 D2.a: validate cluster-side
        // typed_extension_protocol_options.
        if let Some(teo) = &cluster.typed_extension_protocol_options {
            const EXPECTED_TYPE_URL: &str =
                "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions";
            if teo.http_protocol_options.type_url != EXPECTED_TYPE_URL {
                return Err(crate::ConfigError::UnsupportedTypedConfigUrl {
                    got: teo.http_protocol_options.type_url.clone(),
                    expected: EXPECTED_TYPE_URL,
                });
            }
            let ehc = &teo.http_protocol_options.explicit_http_config;
            if ehc.http_protocol_options.is_some() && ehc.http2_protocol_options.is_some() {
                return Err(crate::ConfigError::MutuallyExclusiveExplicitHttpConfig {
                    cluster: cluster.name.clone(),
                });
            }
            if let Some(h2_opts) = &ehc.http2_protocol_options {
                validate_http2_protocol_options_ranges(h2_opts)?;
            }
        }
    }

    // Per-listener invariants.
    for listener in &mut bootstrap.static_resources.listeners {
        for chain in &mut listener.filter_chains {
            // Snapshot per-chain TLS state for `validate_hcm`'s D2.a check
            // (phase 05.2 SPEC §3 — reject `codec_type: HTTP2` on TLS chains).
            // Mirrors the predicate just below: a chain is "TLS" iff its
            // `transport_socket.name == TLS_TRANSPORT_SOCKET`.
            let chain_has_tls = chain
                .transport_socket
                .as_ref()
                .is_some_and(|ts| ts.name == crate::TLS_TRANSPORT_SOCKET);
            if let Some(ts) = chain.transport_socket.as_ref() {
                if ts.name != crate::TLS_TRANSPORT_SOCKET {
                    return Err(crate::ConfigError::UnknownTransportSocketName(
                        ts.name.clone(),
                    ));
                }
                match &ts.typed_config {
                    TransportSocketTypedConfig::Downstream(ctx) => {
                        if ctx.common_tls_context.tls_certificates.is_empty() {
                            return Err(crate::ConfigError::EmptyTlsCertificates {
                                side: "listener",
                            });
                        }
                        for tls_cert in &ctx.common_tls_context.tls_certificates {
                            validate_data_source(
                                &tls_cert.certificate_chain,
                                "tls_certificate.certificate_chain",
                                Required::Filename,
                            )?;
                            validate_data_source(
                                &tls_cert.private_key,
                                "tls_certificate.private_key",
                                Required::Filename,
                            )?;
                        }
                    }
                    TransportSocketTypedConfig::Upstream(_) => {
                        return Err(crate::ConfigError::MismatchedTransportSocketDirection {
                            side: "listener",
                            got: "UpstreamTlsContext",
                        });
                    }
                }
            }
            for filter in &mut chain.filters {
                match filter.name.as_str() {
                    crate::ECHO_FILTER => {
                        if filter.typed_config.is_some() {
                            return Err(crate::ConfigError::UnexpectedTypedConfig(
                                crate::ECHO_FILTER,
                            ));
                        }
                    }
                    crate::TCP_PROXY_FILTER => {
                        // TypedConfig is multi-variant (TcpProxy, HCM). The
                        // HCM-on-tcp_proxy-name shape is misconfiguration; we
                        // reject it as MissingTypedConfig (preserving the
                        // pre-04.1 error surface — the typed_config under the
                        // tcp_proxy name is not a TcpProxyConfig).
                        // Read-only: TCP_PROXY validation does not mutate typed_config
                        // (unlike HCM_FILTER's `as_mut()` arm below — `as_ref()` reborrows
                        // a `&mut filter` as `&filter` which Rust permits).
                        let typed = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::TCP_PROXY_FILTER),
                        )?;
                        let TypedConfig::TcpProxy(tp) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(
                                crate::TCP_PROXY_FILTER,
                            ));
                        };
                        let cluster_name = tp.cluster.clone();
                        if !bootstrap
                            .static_resources
                            .clusters
                            .iter()
                            .any(|c| c.name == cluster_name)
                        {
                            return Err(crate::ConfigError::UnknownCluster(cluster_name));
                        }
                    }
                    crate::HCM_FILTER => {
                        let typed = filter
                            .typed_config
                            .as_mut()
                            .ok_or(crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER))?;
                        let TypedConfig::HttpConnectionManager(hcm) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER));
                        };
                        validate_hcm(
                            hcm,
                            &bootstrap.static_resources.clusters,
                            chain_has_tls,
                            &listener.name,
                        )?;
                    }
                    _ => {
                        return Err(crate::ConfigError::UnsupportedFilter(
                            filter.name.clone(),
                            crate::ECHO_FILTER,
                        ));
                    }
                }
            }
        }

        // 03.2: cross-chain rules within each listener. Task 4's
        // envoy-tls::DownstreamTls::from_listener trusts these guarantees per
        // SPEC §3 D1 — it does not re-check them.

        // Rule 1: overlapping SNI. Walk each chain's server_names; build a
        // HashSet<String> of lowercased SNIs; reject on duplicate.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for chain in &listener.filter_chains {
            if let Some(m) = chain.filter_chain_match.as_ref() {
                for sni in &m.server_names {
                    let lower = sni.to_lowercase();
                    if !seen.insert(lower.clone()) {
                        return Err(crate::ConfigError::MultipleListenersWithOverlappingSni {
                            listener: listener.name.clone(),
                            sni: lower,
                        });
                    }
                }
            }
        }

        // Rule 2: at most one catch-all (empty server_names) chain per listener.
        // A missing filter_chain_match counts as catch-all (matches every connection).
        let catch_all_count = listener
            .filter_chains
            .iter()
            .filter(|c| {
                c.filter_chain_match
                    .as_ref()
                    .map(|m| m.server_names.is_empty())
                    .unwrap_or(true)
            })
            .count();
        if catch_all_count > 1 {
            return Err(crate::ConfigError::MultipleCatchAllFilterChains {
                listener: listener.name.clone(),
            });
        }

        // Rule 3: don't mix TLS and plaintext chains. Only fires when ≥ 2
        // chains exist; mixing requires `tls_inspector` (deferred).
        if listener.filter_chains.len() >= 2 {
            let tls_count = listener
                .filter_chains
                .iter()
                .filter(|c| c.transport_socket.is_some())
                .count();
            if tls_count > 0 && tls_count < listener.filter_chains.len() {
                return Err(
                    crate::ConfigError::MixedTlsAndPlaintextFilterChainsOnListener {
                        listener: listener.name.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

/// Validate a fully-parsed `HttpConnectionManagerConfig` against the phase-04.1
/// surface. SPEC §3 D2 enumerates the rejections this function fires.
///
/// `chain_has_tls` is the enclosing `filter_chain.transport_socket.is_some()`
/// folded down to a bool; phase 05.2 SPEC §3 D2.a uses it to reject
/// `codec_type: HTTP2` on TLS chains (TLS+ALPN+H2 deferred per parent-05 SPEC §4).
///
/// `listener_name` is the enclosing listener's `name` field; phase 06.3 D14.3
/// uses it to name the offending listener in `ConfigError::Http2ClusterFromHttp1Listener`.
fn validate_hcm(
    hcm: &mut HttpConnectionManagerConfig,
    clusters: &[Cluster],
    chain_has_tls: bool,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    // codec_type: AUTO, HTTP1, and HTTP2 are runtime-supported. HTTP3 is
    // rejected pending future work. HTTP2 over TLS is rejected separately
    // below per phase 05.2 SPEC §3 D2.a (TLS+ALPN+H2 deferred per parent-05
    // SPEC §4); the per-chain TLS state is plumbed in via `chain_has_tls`.
    match hcm.codec_type {
        CodecType::AUTO | CodecType::HTTP1 | CodecType::HTTP2 => {}
        CodecType::HTTP3 => {
            return Err(crate::ConfigError::UnsupportedCodecType {
                got: hcm.codec_type,
            });
        }
    }

    // 05.2 NEW — D2.a TLS+HTTP2 rejection.
    if matches!(hcm.codec_type, CodecType::HTTP2) && chain_has_tls {
        return Err(crate::ConfigError::Http2OverTlsNotSupported);
    }

    // 05.2 NEW — D2.b: validate Http2ProtocolOptions ranges per RFC 7540
    // §6.5.2 / §6.9.1 / §6.9.2. Run only if Some; absent = h2-crate defaults.
    // 05.3: hoisted to validate_http2_protocol_options_ranges free function.
    if let Some(opts) = &hcm.http2_protocol_options {
        validate_http2_protocol_options_ranges(opts)?;
    }

    // 06.2 NEW — D2.2: validate access_log entries (name allow-list +
    // non-empty path). Hoisted to a free function for symmetry with the
    // http2_protocol_options pattern.
    validate_access_logs(&hcm.access_log)?;

    // http_filters: cardinality + name + Router-terminal — 07.1 D4.1.
    validate_http_filters(&hcm.http_filters, listener_name)?;

    // route_config: walk virtual_hosts → routes.
    if hcm.route_config.virtual_hosts.is_empty() {
        return Err(crate::ConfigError::EmptyVirtualHosts {
            route_config: hcm.route_config.name.clone(),
        });
    }
    for vh in &mut hcm.route_config.virtual_hosts {
        if vh.domains.is_empty() {
            return Err(crate::ConfigError::EmptyDomains {
                virtual_host: vh.name.clone(),
            });
        }
        for d in &vh.domains {
            if d != "*" && !is_valid_dns_name(d) {
                return Err(crate::ConfigError::UnsupportedDomainMatcher { domain: d.clone() });
            }
        }
        if vh.routes.is_empty() {
            return Err(crate::ConfigError::EmptyRoutes {
                virtual_host: vh.name.clone(),
            });
        }
        for r in &mut vh.routes {
            // RouteMatch: exactly one of {prefix, path} is Some.
            match (&r.r#match.prefix, &r.r#match.path) {
                (Some(_), None) | (None, Some(_)) => {}
                (Some(_), Some(_)) => {
                    return Err(crate::ConfigError::UnsupportedRouteMatcher {
                        matcher: "both prefix and path are set",
                    });
                }
                (None, None) => {
                    return Err(crate::ConfigError::UnsupportedRouteMatcher {
                        matcher: "neither prefix nor path is set",
                    });
                }
            }
            // 04.3: dispatch on the action variant. DirectResponse keeps its
            // 04.1 status-range + body-shape checks. The Route(_) arm has no
            // validator obligation in Task 1; Task 2 wires UnknownCluster.
            match &r.action {
                RouteAction::DirectResponse(dr) => {
                    if !(100..=599).contains(&dr.status) {
                        return Err(crate::ConfigError::InvalidStatusCode { status: dr.status });
                    }
                    validate_data_source(&dr.body, "direct_response.body", Required::InlineString)?;
                }
                RouteAction::Route(ar) => {
                    // 04.3 NEW: check the cluster reference against declared clusters.
                    // ConfigError::UnknownCluster is the 02.1-landed variant reused here
                    // per SPEC §3 D2.
                    if !clusters.iter().any(|c| c.name == ar.cluster) {
                        return Err(crate::ConfigError::UnknownCluster(ar.cluster.clone()));
                    }
                    // 06.3 D14.3 NEW: H1-listener × H2-cluster reachability gate.
                    // Closes 05.3 REVIEW I1 per parent-06 SPEC §3 D14.3.
                    if matches!(hcm.codec_type, CodecType::HTTP1 | CodecType::AUTO) {
                        let cluster_ref = clusters
                            .iter()
                            .find(|c| c.name == ar.cluster)
                            .expect("UnknownCluster check above guarantees presence");
                        if let Some(teo) = &cluster_ref.typed_extension_protocol_options
                            && teo
                                .http_protocol_options
                                .explicit_http_config
                                .http2_protocol_options
                                .is_some()
                        {
                            return Err(crate::ConfigError::Http2ClusterFromHttp1Listener {
                                listener: listener_name.to_string(),
                                cluster: ar.cluster.clone(),
                            });
                        }
                    }
                }
            }

            // 04.2 NEW: walk the headers Vec.
            for hm in &mut r.r#match.headers {
                validate_header_matcher(hm)?;
            }
        }
    }
    Ok(())
}

/// Validate the http_filters list of an HCM listener.
///
/// Enforces: (a) at least one filter, (b) exactly one Router, (c) Router
/// at the terminus, (d) name/typed_config consistency on every entry.
///
/// At 07.1 the only typed_config variant is `Router`; the
/// name/typed_config consistency check is currently dead-code-defensive
/// (the schema's `HttpFilterTypedConfig` enum is closed and serde's
/// `deny_unknown_fields` rejects unknown variants at parse time). The
/// check is retained so 07.2's HeaderMutation arm slots in without a
/// validator rewrite.
///
/// Replaces the pre-07.1 cardinality gate at lines 1335-1346 of this
/// file. Mirrors 05.2's `validate_h2_protocol_options` /
/// 06.3's `Http2ClusterFromHttp1Listener` listener-name-threaded
/// validator shape.
pub(crate) fn validate_http_filters(
    filters: &[crate::HttpFilter],
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if filters.is_empty() {
        return Err(crate::ConfigError::EmptyHttpFilters {
            listener: listener_name.to_string(),
        });
    }

    let router_name = "envoy.filters.http.router";
    let last_index = filters.len() - 1;
    let mut router_positions: Vec<usize> = Vec::new();

    for (i, f) in filters.iter().enumerate() {
        match &f.typed_config {
            crate::HttpFilterTypedConfig::Router(_) => {
                if f.name != router_name {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                router_positions.push(i);
            }
            crate::HttpFilterTypedConfig::HeaderMutation(cfg) => {
                if f.name != "envoy.filters.http.header_mutation" {
                    return Err(crate::ConfigError::UnsupportedHttpFilter {
                        name: f.name.clone(),
                    });
                }
                validate_header_mutation_entries(&cfg.mutations.request_mutations, listener_name)?;
                validate_header_mutation_entries(&cfg.mutations.response_mutations, listener_name)?;
            }
        }
    }

    if router_positions.len() > 1 {
        return Err(crate::ConfigError::DuplicateRouterFilter {
            listener: listener_name.to_string(),
        });
    }
    if router_positions.is_empty() {
        return Err(crate::ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: last_index,
        });
    }
    let router_position = router_positions[0];
    if router_position != last_index {
        return Err(crate::ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: router_position,
        });
    }
    Ok(())
}

/// Validate one HeaderMutation mutations list (request_mutations or
/// response_mutations). Per-entry: non-empty `header.key` + RFC 7230 token
/// set + `append_action` in the supported subset. `position` in each error is
/// the entry index within `entries`. Phase 07.2.
fn validate_header_mutation_entries(
    entries: &[crate::HeaderMutationEntry],
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    for (entry_idx, entry) in entries.iter().enumerate() {
        let key = &entry.append.header.key;
        if key.is_empty() {
            return Err(crate::ConfigError::EmptyHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
            });
        }
        if !is_valid_rfc7230_token(key) {
            return Err(crate::ConfigError::InvalidHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
                key: key.clone(),
            });
        }
        match entry.append.append_action {
            crate::AppendAction::AppendIfExistsOrAdd
            | crate::AppendAction::OverwriteIfExistsOrAdd => {
                // supported.
            }
            crate::AppendAction::AddIfAbsent => {
                return Err(crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "ADD_IF_ABSENT".to_string(),
                });
            }
            crate::AppendAction::OverwriteIfExists => {
                return Err(crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "OVERWRITE_IF_EXISTS".to_string(),
                });
            }
        }
    }
    Ok(())
}

/// RFC 7230 §3.2.6 `token` validation: a header field name is a non-empty
/// sequence of `tchar`. No existing helper in `envoy-config` covers this
/// (the 04.2 HeaderMatcher work does case-insensitive name *matching*, not
/// token-set *validation*) — landed inline here per 07.2 SPEC §6 signpost 1.
fn is_valid_rfc7230_token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// 06.2 Task 5 — validate an HCM's `access_log` Vec.
///
/// Two rejections enforced here (see SPEC §3 D2.2):
///   1. `name` allow-list: only `envoy.access_loggers.file` is accepted.
///      Anything else surfaces as `ConfigError::UnsupportedAccessLogType`.
///      The `@type` URL allow-list is enforced by serde's tagged-enum
///      deserialization on `AccessLogTypedConfig` (unknown URLs surface as
///      `ConfigError::Yaml`); this validator does NOT re-check the URL.
///   2. Non-empty path: `FileAccessLog.path` must not be the empty string.
///      Empty paths surface as `ConfigError::InvalidAccessLogPath`. The
///      sink-side `FileSink::new` would also fail on `""`, but rejecting
///      at parse time gives a clearer diagnostic.
///
/// Mutates nothing; returns the first error encountered (validator-wide
/// convention).
fn validate_access_logs(access_logs: &[AccessLog]) -> Result<(), crate::ConfigError> {
    for entry in access_logs {
        if entry.name != "envoy.access_loggers.file" {
            return Err(crate::ConfigError::UnsupportedAccessLogType {
                actual: entry.name.clone(),
            });
        }
        match &entry.typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                if cfg.path.is_empty() {
                    return Err(crate::ConfigError::InvalidAccessLogPath);
                }
            }
        }
    }
    Ok(())
}

/// Validate RFC 7540 wire-format range constraints on Http2ProtocolOptions
/// fields. Hoisted from validate_hcm at 05.3 Task 3 so the listener-side
/// (validate_hcm) and cluster-side (validate's typed_extension walk) sites
/// share the same range checks. Mutates nothing; returns ConfigError on
/// out-of-range values.
fn validate_http2_protocol_options_ranges(
    opts: &Http2ProtocolOptions,
) -> Result<(), crate::ConfigError> {
    const MAX_FRAME_SIZE_RANGE: (u32, u32) = (16384, 16_777_215);
    const WINDOW_SIZE_RANGE: (u32, u32) = (0, (1u32 << 31) - 1);

    if let Some(v) = opts.max_frame_size
        && !(MAX_FRAME_SIZE_RANGE.0..=MAX_FRAME_SIZE_RANGE.1).contains(&v)
    {
        return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
            field: "max_frame_size",
            value: v,
            range: MAX_FRAME_SIZE_RANGE,
        });
    }
    if let Some(v) = opts.initial_stream_window_size
        && v > WINDOW_SIZE_RANGE.1
    {
        return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
            field: "initial_stream_window_size",
            value: v,
            range: WINDOW_SIZE_RANGE,
        });
    }
    if let Some(v) = opts.initial_connection_window_size
        && v > WINDOW_SIZE_RANGE.1
    {
        return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
            field: "initial_connection_window_size",
            value: v,
            range: WINDOW_SIZE_RANGE,
        });
    }
    // max_concurrent_streams has no upper bound per RFC 7540 §6.5.2;
    // zero is valid. No range check.
    Ok(())
}

/// Per-callsite restriction marker for `validate_data_source`.
///
/// Private to this module; the `ConfigError::UnsupportedDataSource.requires`
/// field stays `&'static str` (public-API-stable) and is populated via
/// [`Required::as_str`].
#[derive(Debug, Clone, Copy)]
enum Required {
    Filename,
    InlineString,
}

impl Required {
    fn as_str(self) -> &'static str {
        match self {
            Self::Filename => "filename",
            Self::InlineString => "inline_string",
        }
    }
}

/// Validate a `DataSource` against a per-callsite restriction.
///
/// Cardinality: exactly one of `{filename, inline_string}` is `Some`.
/// `requires` selects which side must be set; the other side must not be.
fn validate_data_source(
    ds: &DataSource,
    field: &'static str,
    requires: Required,
) -> Result<(), crate::ConfigError> {
    let has_file = ds.filename.is_some();
    let has_inline = ds.inline_string.is_some();
    if has_file == has_inline {
        // both Some, or both None
        return Err(crate::ConfigError::UnsupportedDataSource {
            field,
            requires: requires.as_str(),
        });
    }
    let ok = match requires {
        Required::Filename => has_file,
        Required::InlineString => has_inline,
    };
    if !ok {
        return Err(crate::ConfigError::UnsupportedDataSource {
            field,
            requires: requires.as_str(),
        });
    }
    Ok(())
}

/// Returns true if `name` is a syntactically valid DNS name per RFC 1123 LDH
/// rule. Wildcard prefixes (`*.example.com`) return false in 04.1.
fn is_valid_dns_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 {
        return false;
    }
    if name.starts_with('*') {
        return false; // wildcard prefix deferred
    }
    name.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// Validate a single [`HeaderMatcher`] and, for SafeRegex modes (top-level or
/// nested via [`StringMatcher`]), compile the regex pattern into
/// `Arc<regex::Regex>` stored back on [`SafeRegex::compiled`]. Phase 04.2
/// (under ADR-0021).
fn validate_header_matcher(hm: &mut HeaderMatcher) -> Result<(), crate::ConfigError> {
    if hm.name.is_empty() {
        return Err(crate::ConfigError::EmptyHeaderName);
    }
    match &mut hm.mode {
        HeaderMatcherMode::ExactMatch(_)
        | HeaderMatcherMode::PrefixMatch(_)
        | HeaderMatcherMode::SuffixMatch(_)
        | HeaderMatcherMode::PresentMatch(_) => {}
        HeaderMatcherMode::SafeRegexMatch(sr) => compile_safe_regex(sr)?,
        HeaderMatcherMode::RangeMatch(r) => {
            if r.start >= r.end {
                return Err(crate::ConfigError::InvalidInt64Range {
                    start: r.start,
                    end: r.end,
                });
            }
        }
        HeaderMatcherMode::StringMatch(sm) => match &mut sm.mode {
            StringMatcherMode::Exact(_)
            | StringMatcherMode::Prefix(_)
            | StringMatcherMode::Suffix(_)
            | StringMatcherMode::Contains(_) => {}
            StringMatcherMode::SafeRegex(sr) => compile_safe_regex(sr)?,
        },
    }
    Ok(())
}

/// Compile `sr.regex` into a [`regex::Regex`] and store it on
/// `sr.compiled`. Returns [`crate::ConfigError::InvalidRegex`] on failure.
fn compile_safe_regex(sr: &mut SafeRegex) -> Result<(), crate::ConfigError> {
    match regex::Regex::new(&sr.regex) {
        Ok(re) => {
            sr.compiled = Some(std::sync::Arc::new(re));
            Ok(())
        }
        Err(e) => Err(crate::ConfigError::InvalidRegex {
            regex: sr.regex.clone(),
            source: e,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#;

    #[test]
    fn parses_phase00_minimal_into_bootstrap() {
        let b: Bootstrap = serde_yaml::from_str(MINIMAL).expect("valid YAML");
        assert!(b.node.is_none());
        assert!(b.admin.is_none());
        assert_eq!(b.static_resources.listeners.len(), 1);
        let sock = &b.static_resources.listeners[0].address.socket_address;
        assert_eq!(sock.address, "0.0.0.0");
        assert_eq!(sock.port_value, 10000);
        assert_eq!(b.static_resources.clusters.len(), 0);
    }

    const ADMIN_ONLY: &str = r#"
node:
  id: envoy-rust-phase-01-subject
  cluster: envoy-rust-phase-01

admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901

static_resources:
  listeners: []
  clusters: []
"#;

    #[test]
    fn parses_admin_only_bootstrap() {
        let b: Bootstrap = serde_yaml::from_str(ADMIN_ONLY).expect("valid YAML");
        let node = b.node.expect("node present");
        assert_eq!(node.id, "envoy-rust-phase-01-subject");
        assert_eq!(node.cluster, "envoy-rust-phase-01");
        let admin = b.admin.expect("admin present");
        assert_eq!(admin.address.socket_address.address, "127.0.0.1");
        assert_eq!(admin.address.socket_address.port_value, 9901);
        assert_eq!(b.static_resources.listeners.len(), 0);
        assert_eq!(b.static_resources.clusters.len(), 0);
    }

    #[test]
    fn parses_minimal_bootstrap() {
        let b = crate::parse_bootstrap(MINIMAL).expect("valid");
        assert_eq!(b.static_resources.listeners.len(), 1);
        assert_eq!(
            b.static_resources.listeners[0]
                .address
                .socket_address
                .port_value,
            10000
        );
    }

    // --- Positive parses ---

    #[test]
    fn parses_bootstrap_with_node_admin_empty_resources() {
        let yaml = r#"
node:
  id: id-1
  cluster: cluster-1
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.node.as_ref().unwrap().id, "id-1");
        assert_eq!(
            b.admin.as_ref().unwrap().address.socket_address.port_value,
            9901
        );
        assert!(b.static_resources.listeners.is_empty());
        assert!(b.static_resources.clusters.is_empty());
    }

    #[test]
    fn parses_bootstrap_with_admin_only() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert!(b.node.is_none());
        assert!(b.admin.is_some());
        assert!(b.static_resources.listeners.is_empty());
    }

    #[test]
    fn parses_bootstrap_with_single_endpoint_cluster() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
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
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.static_resources.clusters.len(), 1);
        assert_eq!(b.static_resources.clusters[0].name, "backend");
    }

    #[test]
    fn accepts_node_with_unmodeled_field() {
        // Node deliberately omits deny_unknown_fields (SPEC §D1 inline comment).
        // Upstream Envoy's Node also carries metadata + locality + etc.
        let yaml = r#"
node:
  id: id-1
  cluster: cluster-1
  metadata: { labels: { tier: edge } }
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.node.as_ref().unwrap().id, "id-1");
    }

    // --- Negative validation ---

    #[test]
    fn rejects_unknown_filter_name() {
        // Phase 02.1 widens the validator allow-list from {echo} to
        // {echo, tcp_proxy}. Pick a filter name that sits outside this
        // allow-list (rbac lands in phase 09's network-filter family).
        let yaml = MINIMAL.replace("envoy.filters.network.echo", "envoy.filters.network.rbac");
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedFilter(_, _)),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_empty_listeners_with_no_admin() {
        let yaml = "static_resources:\n  listeners: []\n";
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
    }

    #[test]
    fn rejects_bootstrap_with_neither_admin_nor_listener() {
        // Same as rejects_empty_listeners_with_no_admin but via an empty doc.
        let err = crate::parse_bootstrap("{}").expect_err("must reject");
        assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
    }

    #[test]
    fn rejects_multiple_listeners() {
        let yaml = r#"
static_resources:
  listeners:
    - name: a
      address: { socket_address: { address: 0.0.0.0, port_value: 1 } }
      filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
    - name: b
      address: { socket_address: { address: 0.0.0.0, port_value: 2 } }
      filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::TooManyListeners(2)),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_malformed_yaml() {
        let err = crate::parse_bootstrap("::: not yaml :::").expect_err("must fail");
        assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");
    }

    // --- deny_unknown_fields regressions (SPEC §D1 + phase-00 N2 closure) ---

    fn assert_unknown_field(err: crate::ConfigError) {
        let debug_str = format!("{err:?}");
        let display_str = format!("{err}");
        let debug_full = format!("{err:#?}");
        let contains_unknown = debug_str.contains("unknown field")
            || display_str.contains("unknown field")
            || debug_full.contains("unknown field");
        assert!(
            contains_unknown,
            "expected `unknown field` in error; got debug_str={}, display_str={}, debug_full={}",
            debug_str, display_str, debug_full
        );
    }

    #[test]
    fn rejects_unknown_bootstrap_field() {
        let yaml = format!("{MINIMAL}\nbogus_field: true\n");
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_admin_field() {
        let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
  bogus: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_cluster_field() {
        let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources:
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
                      port_value: 10001
      bogus: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_listener_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      bogus_listener_field: true
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    // --- N2 closure: 5 deeper structs (STATE.md lines 87–90) ---

    #[test]
    fn rejects_unknown_static_resources_field() {
        let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources:
  bogus_sr_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_address_field() {
        let yaml = r#"
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
    bogus_addr_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_socket_address_field() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
      bogus_sa_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_filter_chain_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters: [{ name: envoy.filters.network.echo }]
          bogus_fc_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_network_filter_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              bogus_nf_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn parses_bootstrap_with_tcp_proxy_filter() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
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
                      port_value: 10001
"#;
        let b: Bootstrap = serde_yaml::from_str(yaml).expect("valid YAML");
        let filter = &b.static_resources.listeners[0].filter_chains[0].filters[0];
        assert_eq!(filter.name, "envoy.filters.network.tcp_proxy");
        match filter.typed_config.as_ref().expect("typed_config present") {
            TypedConfig::TcpProxy(tp) => {
                assert_eq!(tp.stat_prefix, "ingress_tcp");
                assert_eq!(tp.cluster, "backend");
            }
            other => panic!("unexpected typed_config variant: {other:?}"),
        }
    }

    #[test]
    fn rejects_typed_config_unknown_type_url() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.not_tcp_proxy.v3.NotTcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("@type"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }

    #[test]
    fn rejects_unknown_tcp_proxy_config_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
                idle_timeout: 0s
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(msg.contains("unknown field"), "got {msg}");
    }

    #[test]
    fn parses_bootstrap_with_round_robin_multi_endpoint_cluster() {
        let yaml = r#"
static_resources:
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
                      port_value: 10001
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10003
  listeners: []
"#;
        let b: Bootstrap = serde_yaml::from_str(yaml).expect("valid YAML");
        assert_eq!(b.static_resources.clusters.len(), 1);
        let c = &b.static_resources.clusters[0];
        assert_eq!(c.name, "backend");
        assert!(matches!(c.cluster_type, ClusterType::Static));
        assert!(matches!(c.lb_policy, LbPolicy::RoundRobin));
        assert_eq!(c.load_assignment.cluster_name, "backend");
        assert_eq!(c.load_assignment.endpoints.len(), 1);
        assert_eq!(c.load_assignment.endpoints[0].lb_endpoints.len(), 3);
        assert_eq!(
            c.load_assignment.endpoints[0].lb_endpoints[2]
                .endpoint
                .address
                .socket_address
                .port_value,
            10003
        );
    }

    #[test]
    fn rejects_cluster_type_logical_dns() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: LOGICAL_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("LOGICAL_DNS"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }

    #[test]
    fn rejects_tcp_proxy_without_typed_config() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
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
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingTypedConfig(_)),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_lb_policy_least_request() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: LEAST_REQUEST
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("LEAST_REQUEST"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }

    #[test]
    fn rejects_echo_with_typed_config() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
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
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnexpectedTypedConfig(_)),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_tcp_proxy_naming_missing_cluster() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
                cluster: nonexistent
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
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnknownCluster(ref s) if s == "nonexistent"),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_load_assignment_cluster_name_mismatch() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: drift
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::LoadAssignmentNameMismatch { ref cluster, ref assignment }
                    if cluster == "backend" && assignment == "drift"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_empty_lb_endpoints() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints: []
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::EmptyClusterEndpoints(ref s) if s == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_malformed_endpoint_address() {
        // Parse-layer *acceptance*: serde sees a valid Address/SocketAddress
        // shape (address: String, port_value: u16). The SocketAddr parse
        // failure surfaces in envoy-cluster::from_bootstrap at construction
        // time (see envoy-cluster Task 7's ClusterError::EndpointParse test).
        let yaml = r#"
static_resources:
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
                      address: not-a-host
                      port_value: 10001
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let b = crate::parse_bootstrap(yaml).expect("serde accepts; SocketAddr parse defers");
        assert_eq!(
            b.static_resources.clusters[0].load_assignment.endpoints[0].lb_endpoints[0]
                .endpoint
                .address
                .socket_address
                .address,
            "not-a-host",
        );
    }

    #[test]
    fn rejects_unknown_load_assignment_field() {
        let yaml = r#"
static_resources:
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
                      port_value: 10001
        bogus_la_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_locality_lb_endpoints_field() {
        let yaml = r#"
static_resources:
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
                      port_value: 10001
            bogus_lle_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_lb_endpoint_field() {
        let yaml = r#"
static_resources:
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
                      port_value: 10001
                bogus_lbe_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_endpoint_field() {
        let yaml = r#"
static_resources:
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
                      port_value: 10001
                  bogus_ep_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn fuzz_corpus_seeds_parse_or_reject_cleanly() {
        let root = env!("CARGO_MANIFEST_DIR");
        // Seeds expected to parse + validate successfully.
        for fname in &[
            "fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml",
            "fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml",
            "fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml",
            "fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml",
            "fuzz/corpus/parse_bootstrap/tls_multi_cert_sni.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml",
            "fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml",
            "fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml",
            "fuzz/corpus/parse_bootstrap/admin_with_stats_route.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml",
        ] {
            let path = format!("{root}/{fname}");
            let yaml =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            crate::parse_bootstrap(&yaml).unwrap_or_else(|e| panic!("parse {path}: {e}"));
        }
        // Seeds expected to reject cleanly (parse_bootstrap returns Err, not panic).
        for fname in &[
            "fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml",
            "fuzz/corpus/parse_bootstrap/tls_overlapping_sni_reject.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml",
        ] {
            let path = format!("{root}/{fname}");
            let yaml =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            assert!(
                crate::parse_bootstrap(&yaml).is_err(),
                "{path} was expected to reject, but parsed",
            );
        }
        // The minimal.yaml seed is the phase-00 admin-only baseline; assert
        // it still parses (regression gate against schema additions breaking
        // baseline acceptance).
        let minimal = format!("{root}/fuzz/corpus/parse_bootstrap/minimal.yaml");
        let yaml =
            std::fs::read_to_string(&minimal).unwrap_or_else(|e| panic!("read {minimal}: {e}"));
        crate::parse_bootstrap(&yaml).unwrap_or_else(|e| panic!("parse {minimal}: {e}"));
    }

    #[test]
    fn parses_listener_with_downstream_tls_context() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
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
                      port_value: 10001
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let chain = &bootstrap.static_resources.listeners[0].filter_chains[0];
        let ts = chain
            .transport_socket
            .as_ref()
            .expect("transport_socket present");
        assert_eq!(ts.name, "envoy.transport_sockets.tls");
        match &ts.typed_config {
            crate::TransportSocketTypedConfig::Downstream(ctx) => {
                let certs = &ctx.common_tls_context.tls_certificates;
                assert_eq!(certs.len(), 1);
                assert_eq!(
                    certs[0].certificate_chain.filename.as_deref(),
                    Some("/tmp/leaf.pem")
                );
                assert_eq!(
                    certs[0].private_key.filename.as_deref(),
                    Some("/tmp/leaf.key")
                );
            }
            other => panic!("unexpected typed_config: {other:?}"),
        }
    }

    #[test]
    fn parses_cluster_with_upstream_tls_context() {
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let cluster = &bootstrap.static_resources.clusters[0];
        let ts = cluster
            .transport_socket
            .as_ref()
            .expect("transport_socket present");
        assert_eq!(ts.name, "envoy.transport_sockets.tls");
        match &ts.typed_config {
            crate::TransportSocketTypedConfig::Upstream(ctx) => {
                assert_eq!(ctx.sni, "envoy-rust.test");
                let vc = ctx
                    .common_tls_context
                    .validation_context
                    .as_ref()
                    .expect("validation_context present");
                assert_eq!(vc.trusted_ca.filename.as_deref(), Some("/tmp/ca.pem"));
                assert!(ctx.common_tls_context.tls_certificates.is_empty());
            }
            other => panic!("unexpected typed_config: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_field_in_downstream_tls_context() {
        // require_client_certificate is mTLS-shaped and out of phase 03 per
        // SPEC §4. deny_unknown_fields on DownstreamTlsContext rejects it at
        // parse time.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              require_client_certificate: false
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown field");
        let msg = format!("{err}");
        assert!(
            msg.contains("require_client_certificate") || msg.contains("unknown field"),
            "expected unknown-field error, got: {msg}",
        );
    }

    #[test]
    fn rejects_unknown_field_in_common_tls_context() {
        // alpn_protocols is a phase-04 surface; phase 03 fixtures do not
        // include it (SPEC §6 signpost 14). deny_unknown_fields on
        // CommonTlsContext rejects it.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                alpn_protocols: ["h2"]
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown field");
        let msg = format!("{err}");
        assert!(
            msg.contains("alpn_protocols") || msg.contains("unknown field"),
            "expected unknown-field error, got: {msg}",
        );
    }

    // The phase-03.1 test `rejects_unknown_field_in_data_source` was removed in
    // phase 04.1 Task 1: `inline_string` is now a recognized DataSource field
    // (used by `direct_response.body`), so the YAML it asserted-rejects now
    // parses cleanly. The "exactly one of {filename, inline_string} set" rule
    // — and the per-callsite restriction that TLS still requires `filename` —
    // is the validator's responsibility (phase 04.1 Task 2), and Task 2's
    // validator tests subsume the regression coverage.

    #[test]
    fn rejects_unknown_transport_socket_name() {
        // Phase 03 only accepts "envoy.transport_sockets.tls".
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.raw_buffer
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnknownTransportSocketName(ref n) if n == "envoy.transport_sockets.raw_buffer"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_downstream_tls_context_on_cluster() {
        // DownstreamTlsContext on a cluster's transport_socket → MismatchedTransportSocketDirection.
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
          common_tls_context:
            tls_certificates:
              - certificate_chain:
                  filename: /tmp/leaf.pem
                private_key:
                  filename: /tmp/leaf.key
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::MismatchedTransportSocketDirection {
                    side: "cluster",
                    got: "DownstreamTlsContext",
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_tls_context_on_listener() {
        // UpstreamTlsContext on a listener's filter_chain.transport_socket →
        // MismatchedTransportSocketDirection.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
              sni: envoy-rust.test
              common_tls_context:
                validation_context:
                  trusted_ca:
                    filename: /tmp/ca.pem
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::MismatchedTransportSocketDirection {
                    side: "listener",
                    got: "UpstreamTlsContext",
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_downstream_with_empty_tls_certificates() {
        // Downstream side requires ≥1 cert; empty → EmptyTlsCertificates { side: "listener" }.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates: []
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::EmptyTlsCertificates { side: "listener" }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_with_tls_certificates() {
        // Upstream side requires 0 certs (mTLS deferred); non-empty →
        // EmptyTlsCertificates { side: "cluster" } (variant naming is asymmetric:
        // "Empty" on listener means too-few, on cluster means too-many; the
        // side discriminator carries the meaning).
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context:
            tls_certificates:
              - certificate_chain:
                  filename: /tmp/client.pem
                private_key:
                  filename: /tmp/client.key
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::EmptyTlsCertificates { side: "cluster" }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_without_validation_context() {
        // No insecure-skip in phase 03 (SPEC §4) — validation_context required.
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context: {}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingValidationContext),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_with_empty_sni() {
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: ""
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::EmptyUpstreamSni),
            "got {err:?}"
        );
    }

    #[test]
    fn parses_filter_chain_with_server_names() {
        let yaml = r#"
filter_chain_match:
  server_names: ["a.example.com", "b.example.com"]
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        let m = chain.filter_chain_match.expect("has filter_chain_match");
        assert_eq!(
            m.server_names,
            vec!["a.example.com".to_string(), "b.example.com".to_string()]
        );
    }

    #[test]
    fn parses_filter_chain_without_filter_chain_match() {
        // Existing 03.1 / 02.2 shape — no filter_chain_match key.
        let yaml = r#"
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        assert!(chain.filter_chain_match.is_none());
    }

    #[test]
    fn parses_filter_chain_match_with_empty_server_names() {
        // `filter_chain_match: {}` is the catch-all shape.
        let yaml = r#"
filter_chain_match: {}
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        let m = chain.filter_chain_match.expect("has filter_chain_match");
        assert!(m.server_names.is_empty());
    }

    #[test]
    fn parses_filter_chain_match_with_explicit_empty_server_names_list() {
        let yaml = r#"
filter_chain_match:
  server_names: []
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        let m = chain.filter_chain_match.expect("has filter_chain_match");
        assert!(m.server_names.is_empty());
    }

    #[test]
    fn rejects_filter_chain_match_unknown_field() {
        // deny_unknown_fields discipline: an unrecognized key under filter_chain_match fails.
        let yaml = r#"
filter_chain_match:
  destination_port: 443
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let err = serde_yaml::from_str::<FilterChain>(yaml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("destination_port") || msg.contains("unknown field"),
            "expected unknown-field error, got {msg}"
        );
    }

    // --- Phase 03.2 Task 2: cross-chain validator rules ---
    //
    // These tests use the existing `crate::parse_bootstrap` boundary (which
    // performs parse + validate together) instead of the PLAN's bare
    // `bootstrap.validate()` call: phase 03 keeps `validate` crate-private,
    // and every existing validator test in this file already routes through
    // `parse_bootstrap`. Documented as a "test API adaptation" deviation in
    // the Task 2 PROGRESS.md entry.

    #[test]
    fn parses_listener_with_multi_chain_sni_routing() {
        // Happy path: two filter chains, each carrying its own DownstreamTlsContext
        // and disjoint server_names. Validator accepts.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        crate::parse_bootstrap(yaml).expect("parses + validates");
    }

    #[test]
    fn parses_filter_chain_with_empty_server_names_validator() {
        // Single catch-all chain with empty server_names. Accepted by validator.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: [] }
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        crate::parse_bootstrap(yaml).expect("parses + validates");
    }

    #[test]
    fn rejects_filter_chains_with_overlapping_sni() {
        // Two chains both declare server_names: ["a.example.com"].
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject overlapping SNI");
        assert!(
            matches!(
                err,
                crate::ConfigError::MultipleListenersWithOverlappingSni { ref listener, ref sni }
                    if listener == "tcp_listener" && sni == "a.example.com"
            ),
            "expected MultipleListenersWithOverlappingSni, got {err:?}"
        );
    }

    #[test]
    fn rejects_filter_chains_with_overlapping_sni_case_insensitive() {
        // Chain A "a.example.com"; chain B "A.Example.com". Match is case-insensitive.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["A.Example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject case-insensitive overlap");
        assert!(
            matches!(
                err,
                crate::ConfigError::MultipleListenersWithOverlappingSni { ref listener, .. }
                    if listener == "tcp_listener"
            ),
            "expected MultipleListenersWithOverlappingSni, got {err:?}"
        );
    }

    #[test]
    fn rejects_multiple_catch_all_filter_chains() {
        // Two chains both have empty server_names.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: [] }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: {}
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject multiple catch-all chains");
        assert!(
            matches!(
                err,
                crate::ConfigError::MultipleCatchAllFilterChains { ref listener }
                    if listener == "tcp_listener"
            ),
            "expected MultipleCatchAllFilterChains, got {err:?}"
        );
    }

    #[test]
    fn rejects_mixed_tls_and_plaintext_filter_chains() {
        // One TLS chain, one plaintext chain on the same listener.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject mixed TLS and plaintext");
        assert!(
            matches!(
                err,
                crate::ConfigError::MixedTlsAndPlaintextFilterChainsOnListener { ref listener }
                    if listener == "tcp_listener"
            ),
            "expected MixedTlsAndPlaintextFilterChainsOnListener, got {err:?}"
        );
    }

    #[test]
    fn parses_listener_with_hcm_direct_response() {
        let yaml = r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
"#;
        let bs: Bootstrap = serde_yaml::from_str(yaml).expect("parses");
        let listener = &bs.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let TypedConfig::HttpConnectionManager(hcm) = filter.typed_config.as_ref().unwrap() else {
            panic!("expected HCM variant");
        };
        assert_eq!(hcm.stat_prefix, "ingress_http");
        assert!(matches!(hcm.codec_type, CodecType::HTTP1));
        assert_eq!(hcm.route_config.virtual_hosts.len(), 1);
        let vh = &hcm.route_config.virtual_hosts[0];
        assert_eq!(vh.domains, vec!["*".to_string()]);
        let route = &vh.routes[0];
        assert_eq!(route.r#match.prefix.as_deref(), Some("/"));
        let dr = match &route.action {
            RouteAction::DirectResponse(dr) => dr,
            other => panic!("expected DirectResponse, got {other:?}"),
        };
        assert_eq!(dr.status, 200);
        assert_eq!(dr.body.inline_string.as_deref(), Some("ok\n"));
        assert_eq!(hcm.http_filters.len(), 1);
        assert_eq!(hcm.http_filters[0].name, "envoy.filters.http.router");
    }

    #[test]
    fn parses_route_with_path_matcher() {
        let yaml = r#"
prefix: ~
path: "/exact"
"#;
        let m: RouteMatch = serde_yaml::from_str(yaml).expect("parses");
        assert!(m.prefix.is_none());
        assert_eq!(m.path.as_deref(), Some("/exact"));
    }

    #[test]
    fn parses_data_source_with_inline_string() {
        let yaml = r#"
inline_string: "hello"
"#;
        let ds: DataSource = serde_yaml::from_str(yaml).expect("parses");
        assert!(ds.filename.is_none());
        assert_eq!(ds.inline_string.as_deref(), Some("hello"));
    }

    #[test]
    fn parses_data_source_with_filename() {
        let yaml = r#"
filename: "/tmp/cert.pem"
"#;
        let ds: DataSource = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(ds.filename.as_deref(), Some("/tmp/cert.pem"));
        assert!(ds.inline_string.is_none());
    }

    #[test]
    fn rejects_unknown_field_in_hcm_config() {
        // The original sentinel was `access_log`, chosen pre-06.2 because
        // it was not yet recognized by `HttpConnectionManagerConfig`. 06.2
        // Task 5 added `access_log` to the schema; swapping to a fresh
        // not-an-HCM-field sentinel preserves the test's intent (verify
        // that `deny_unknown_fields` still rejects unrecognized fields).
        let yaml = r#"
stat_prefix: ingress_http
codec_type: HTTP1
bogus_hcm_field: 1
route_config:
  name: r
  virtual_hosts: []
http_filters: []
"#;
        let res: Result<HttpConnectionManagerConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            res.is_err(),
            "deny_unknown_fields should reject bogus_hcm_field"
        );
        let err = res.err().unwrap().to_string();
        assert!(
            err.contains("bogus_hcm_field") || err.contains("unknown field"),
            "error mentions unknown field: {err}"
        );
    }

    #[test]
    fn rejects_unknown_field_in_route_match() {
        let yaml = r#"
prefix: "/"
case_sensitive: true
"#;
        let res: Result<RouteMatch, _> = serde_yaml::from_str(yaml);
        assert!(
            res.is_err(),
            "deny_unknown_fields should reject case_sensitive"
        );
    }

    // --- phase 04.1 Task 2: HCM validator tests ---

    fn parse_then_validate(yaml: &str) -> Result<Bootstrap, crate::ConfigError> {
        let mut bs: Bootstrap = serde_yaml::from_str(yaml)?;
        validate(&mut bs)?;
        Ok(bs)
    }

    fn make_hcm_listener_yaml(hcm_block: &str) -> String {
        format!(
            r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
{}
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 0 }}
"#,
            hcm_block
        )
    }

    const VALID_ROUTER_FILTER: &str = r#"
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;

    // Pre-Task-2 `rejects_codec_type_http2` deleted: phase 05.2 Task 2 flipped
    // codec_type: HTTP2 from "rejected" to "accepted on plaintext listeners".
    // The new positive test `parses_hcm_with_codec_type_http2` (above, defined
    // alongside `rejects_hcm_with_codec_type_http2_on_tls_listener`) replaces
    // it; the HTTP3 rejection test below remains the lone codec_type negative.

    #[test]
    fn rejects_codec_type_http3() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP3
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject HTTP3");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedCodecType {
                    got: CodecType::HTTP3
                }
            ),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_unsupported_http_filter() {
        let hcm = r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.lua
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;
        // The @type IS the router (the only schema arm), but `name` is "lua" — validator rejects.
        let yaml = make_hcm_listener_yaml(hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject non-router name");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedHttpFilter { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_route_match_with_both_prefix_and_path() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/x", path: "/y" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject both prefix and path");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedRouteMatcher { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_route_match_with_neither_prefix_nor_path() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{}}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject empty match");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedRouteMatcher { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_direct_response_with_filename_body() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ filename: "/tmp/x" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err =
            parse_then_validate(&yaml).expect_err("should reject filename in direct_response");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedDataSource {
                    field: "direct_response.body",
                    requires: "inline_string"
                }
            ),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_direct_response_with_invalid_status() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 99
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject status < 100");
        assert!(
            matches!(err, crate::ConfigError::InvalidStatusCode { status: 99 }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_empty_virtual_hosts() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts: []
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject empty virtual_hosts");
        assert!(
            matches!(err, crate::ConfigError::EmptyVirtualHosts { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_empty_domains() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: []
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject empty domains");
        assert!(
            matches!(err, crate::ConfigError::EmptyDomains { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn parses_int64_range() {
        let yaml = r#"
start: 1
end: 100
"#;
        let r: Int64Range = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(r.start, 1);
        assert_eq!(r.end, 100);
    }

    #[test]
    fn rejects_unknown_field_in_int64_range() {
        let yaml = r#"
start: 1
end: 100
step: 5
"#;
        let res: Result<Int64Range, _> = serde_yaml::from_str(yaml);
        assert!(res.is_err(), "deny_unknown_fields should reject `step`");
    }

    #[test]
    fn parses_safe_regex() {
        let yaml = r#"
regex: "^v[0-9]+$"
"#;
        let sr: SafeRegex = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(sr.regex, "^v[0-9]+$");
        assert!(sr.compiled.is_none(), "compiled set to None pre-validate");
    }

    #[test]
    fn safe_regex_partial_eq_compares_only_regex_string() {
        let a = SafeRegex {
            regex: "x".into(),
            compiled: None,
        };
        let b = SafeRegex {
            regex: "x".into(),
            compiled: Some(std::sync::Arc::new(regex::Regex::new("x").unwrap())),
        };
        assert_eq!(a, b, "compiled field is opaque to PartialEq");
    }

    #[test]
    fn parses_string_matcher_exact() {
        let yaml = r#"
exact: "foo"
"#;
        let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(sm.mode, StringMatcherMode::Exact("foo".into()));
        assert!(!sm.ignore_case);
    }

    #[test]
    fn parses_string_matcher_contains_with_ignore_case() {
        let yaml = r#"
contains: "beta"
ignore_case: true
"#;
        let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(sm.mode, StringMatcherMode::Contains("beta".into()));
        assert!(sm.ignore_case);
    }

    #[test]
    fn parses_string_matcher_safe_regex() {
        let yaml = r#"
safe_regex:
  regex: "^v[0-9]+$"
"#;
        let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
        match sm.mode {
            StringMatcherMode::SafeRegex(sr) => {
                assert_eq!(sr.regex, "^v[0-9]+$");
                assert!(sr.compiled.is_none());
            }
            other => panic!("expected SafeRegex variant, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_string_matcher_mode_key() {
        let yaml = r#"
weird: "x"
"#;
        let res: Result<StringMatcher, _> = serde_yaml::from_str(yaml);
        assert!(res.is_err(), "unknown mode key should error");
        let err = res.err().unwrap().to_string();
        assert!(
            err.contains("weird") || err.contains("unknown"),
            "error mentions unknown key: {err}"
        );
    }

    #[test]
    fn rejects_two_string_matcher_mode_keys() {
        let yaml = r#"
exact: "a"
prefix: "b"
"#;
        let res: Result<StringMatcher, _> = serde_yaml::from_str(yaml);
        let err = res
            .expect_err("two mode keys should be rejected (each variant is mutually exclusive)")
            .to_string();
        assert!(
            err.contains("multiple mode keys") || err.contains("mutually exclusive"),
            "error should mention mutual exclusivity: {err}"
        );
    }

    #[test]
    fn parses_header_matcher_exact() {
        let yaml = r#"
name: "x-foo"
exact_match: "bar"
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(m.name, "x-foo");
        assert_eq!(m.mode, HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.invert_match);
    }

    #[test]
    fn parses_header_matcher_with_invert_match_true() {
        let yaml = r#"
name: "x-foo"
exact_match: "bar"
invert_match: true
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert!(m.invert_match);
    }

    #[test]
    fn parses_header_matcher_present_match_true() {
        let yaml = r#"
name: "authorization"
present_match: true
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(m.mode, HeaderMatcherMode::PresentMatch(true));
    }

    #[test]
    fn parses_header_matcher_string_match_contains() {
        let yaml = r#"
name: "x-tag"
string_match:
  contains: "beta"
  ignore_case: true
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        match m.mode {
            HeaderMatcherMode::StringMatch(sm) => {
                assert_eq!(sm.mode, StringMatcherMode::Contains("beta".into()));
                assert!(sm.ignore_case);
            }
            other => panic!("expected StringMatch variant, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_header_matcher_mode_key() {
        let yaml = r#"
name: "x-foo"
weird_match: "bar"
"#;
        let res: Result<HeaderMatcher, _> = serde_yaml::from_str(yaml);
        let err = res.expect_err("unknown mode key should error").to_string();
        assert!(
            err.contains("weird_match") || err.contains("unknown"),
            "error mentions unknown key: {err}"
        );
    }

    #[test]
    fn rejects_two_header_matcher_mode_keys() {
        let yaml = r#"
name: "x-foo"
exact_match: "a"
prefix_match: "b"
"#;
        let res: Result<HeaderMatcher, _> = serde_yaml::from_str(yaml);
        let err = res
            .expect_err("two mode keys should be rejected (each variant is mutually exclusive)")
            .to_string();
        assert!(
            err.contains("multiple mode keys") || err.contains("mutually exclusive"),
            "error should mention mutual exclusivity: {err}"
        );
    }

    // --- phase 04.2 Task 5: HeaderMatcher validator tests ---

    #[test]
    fn rejects_empty_header_name() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: ""
                                exact_match: "bar"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(
            matches!(err, crate::ConfigError::EmptyHeaderName),
            "expected EmptyHeaderName, got {err:?}"
        );
    }

    #[test]
    fn rejects_invalid_regex_in_safe_regex_match() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                safe_regex_match:
                                  regex: "[unclosed"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(
            matches!(err, crate::ConfigError::InvalidRegex { .. }),
            "expected InvalidRegex, got {err:?}"
        );
    }

    #[test]
    fn rejects_invalid_regex_in_string_match_safe_regex() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                string_match:
                                  safe_regex:
                                    regex: "(?P<oops"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(matches!(err, crate::ConfigError::InvalidRegex { .. }));
    }

    #[test]
    fn rejects_invalid_int64_range_start_eq_end() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                range_match: { start: 100, end: 100 }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidInt64Range {
                    start: 100,
                    end: 100
                }
            ),
            "expected InvalidInt64Range {{100,100}}, got {err:?}"
        );
    }

    #[test]
    fn rejects_invalid_int64_range_start_gt_end() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                range_match: { start: 200, end: 100 }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(matches!(err, crate::ConfigError::InvalidInt64Range { .. }));
    }

    #[test]
    fn validator_compiles_safe_regex_match_into_arc() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                safe_regex_match:
                                  regex: "^v[0-9]+$"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("not HCM");
        };
        let header_matcher = &hcm.route_config.virtual_hosts[0].routes[0].r#match.headers[0];
        let HeaderMatcherMode::SafeRegexMatch(sr) = &header_matcher.mode else {
            panic!("not SafeRegexMatch");
        };
        assert!(sr.compiled.is_some(), "validator should have compiled");
    }

    #[test]
    fn validator_accepts_all_seven_modes() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - { name: "h1", exact_match: "x" }
                              - { name: "h2", prefix_match: "p" }
                              - { name: "h3", suffix_match: "s" }
                              - { name: "h4", safe_regex_match: { regex: "^v[0-9]+$" } }
                              - { name: "h5", range_match: { start: 1, end: 100 } }
                              - { name: "h6", present_match: true }
                              - { name: "h7", string_match: { contains: "c", ignore_case: true } }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("not HCM");
        };
        assert_eq!(
            hcm.route_config.virtual_hosts[0].routes[0]
                .r#match
                .headers
                .len(),
            7
        );
    }

    #[test]
    fn validator_accepts_empty_headers_vec() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        crate::parse_bootstrap(&yaml).expect("parses + validates");
    }

    #[test]
    fn validator_accepts_invert_match_true() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                exact_match: "bar"
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        crate::parse_bootstrap(&yaml).expect("parses + validates");
    }

    #[test]
    fn validator_compiles_string_match_safe_regex_into_arc() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-tag"
                                string_match:
                                  safe_regex:
                                    regex: "^beta$"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("not HCM");
        };
        let header_matcher = &hcm.route_config.virtual_hosts[0].routes[0].r#match.headers[0];
        let HeaderMatcherMode::StringMatch(sm) = &header_matcher.mode else {
            panic!("not StringMatch");
        };
        let StringMatcherMode::SafeRegex(sr) = &sm.mode else {
            panic!("not SafeRegex");
        };
        assert!(sr.compiled.is_some(), "nested regex should be compiled");
    }

    #[test]
    fn parses_route_match_with_headers_vec_and_invert_match_default() {
        let yaml = r#"
prefix: "/api/"
headers:
  - name: "x-foo"
    exact_match: "bar"
  - name: "x-version"
    range_match: { start: 1, end: 100 }
"#;
        let rm: RouteMatch = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(rm.prefix.as_deref(), Some("/api/"));
        assert_eq!(rm.headers.len(), 2);
        assert_eq!(rm.headers[0].name, "x-foo");
        assert!(!rm.headers[0].invert_match);
        assert_eq!(
            rm.headers[0].mode,
            HeaderMatcherMode::ExactMatch("bar".into())
        );
        assert_eq!(rm.headers[1].name, "x-version");
        assert_eq!(
            rm.headers[1].mode,
            HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 })
        );
    }

    // --- 04.3 Task 1: RouteAction parse-shape tests ---

    /// Build the full bootstrap YAML scaffolding around the given routes block
    /// and clusters block. The 5 RouteAction parse-shape tests vary only in
    /// these two slots.
    fn route_action_yaml(routes: &str, clusters: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  listeners:
    - name: hcm_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        {routes}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:{clusters}
"#
        )
    }

    const BACKEND_CLUSTER: &str = r#"
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }"#;

    const NO_CLUSTERS: &str = " []";

    /// Helper to drill into the parsed bootstrap and return the first route's
    /// action. Panics on any structural mismatch (these tests are about
    /// RouteAction shape; non-shape failures should fail loudly).
    fn first_route_action(b: &Bootstrap) -> &RouteAction {
        let listener = &b.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let typed = filter.typed_config.as_ref().expect("typed_config present");
        let hcm = match typed {
            TypedConfig::HttpConnectionManager(hcm) => hcm,
            other => panic!("expected HCM typed_config, got {other:?}"),
        };
        &hcm.route_config.virtual_hosts[0].routes[0].action
    }

    #[test]
    fn parses_route_with_direct_response_action() {
        let routes = r#"- match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }"#;
        let yaml = route_action_yaml(routes, NO_CLUSTERS);
        let b = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let action = first_route_action(&b);
        match action {
            RouteAction::DirectResponse(dr) => {
                assert_eq!(dr.status, 200);
                assert_eq!(dr.body.inline_string.as_deref(), Some("ok\n"));
            }
            other => panic!("expected DirectResponse, got {other:?}"),
        }
    }

    #[test]
    fn parses_route_with_route_action() {
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: backend }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let b = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let action = first_route_action(&b);
        match action {
            RouteAction::Route(ar) => {
                assert_eq!(ar.cluster, "backend");
            }
            other => panic!("expected Route(_), got {other:?}"),
        }
    }

    #[test]
    fn rejects_route_with_both_direct_response_and_route() {
        let routes = r#"- match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                          route: { cluster: backend }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let err = crate::parse_bootstrap(&yaml).expect_err("rejects both peers present");
        let msg = err.to_string();
        assert!(
            msg.contains("direct_response"),
            "msg should mention direct_response; got: {msg}"
        );
        assert!(
            msg.contains("route"),
            "msg should mention route; got: {msg}"
        );
        assert!(
            msg.contains("exactly one"),
            "msg should mention `exactly one`; got: {msg}"
        );
    }

    #[test]
    fn rejects_route_with_neither_direct_response_nor_route() {
        let routes = r#"- match: { prefix: "/" }"#;
        let yaml = route_action_yaml(routes, NO_CLUSTERS);
        let err = crate::parse_bootstrap(&yaml).expect_err("rejects neither peer present");
        let msg = err.to_string();
        assert!(
            msg.contains("direct_response"),
            "msg should mention direct_response; got: {msg}"
        );
        assert!(
            msg.contains("route"),
            "msg should mention route; got: {msg}"
        );
        assert!(
            msg.contains("exactly one"),
            "msg should mention `exactly one`; got: {msg}"
        );
    }

    #[test]
    fn rejects_route_with_unknown_top_level_key() {
        let routes = r#"- match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                          unknown_route_field: surprise"#;
        let yaml = route_action_yaml(routes, NO_CLUSTERS);
        let err = crate::parse_bootstrap(&yaml).expect_err("rejects unknown top-level Route key");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("unknown"),
            "msg should mention `unknown` (case-insensitive); got: {msg}"
        );
        assert!(
            msg.contains("unknown_route_field"),
            "msg should mention the offending key `unknown_route_field`; got: {msg}"
        );
    }

    // --- 04.3 Task 2: validator UnknownCluster reuse for RouteAction::Route ---

    #[test]
    fn parses_route_with_cluster_action() {
        // Happy path: route references the declared `backend` cluster.
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: backend }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        crate::parse_bootstrap(&yaml).expect("parses + validates");
    }

    #[test]
    fn rejects_hcm_route_with_unknown_cluster() {
        // The route references `nonexistent`; only `backend` is declared.
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: nonexistent }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject unknown cluster");
        assert!(
            matches!(&err, crate::ConfigError::UnknownCluster(name) if name == "nonexistent"),
            "expected UnknownCluster(\"nonexistent\"); got: {err:?}"
        );
    }

    #[test]
    fn rejects_hcm_route_with_empty_cluster_name() {
        // Empty cluster names are treated as just another unknown reference;
        // no cluster declares an empty name.
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: "" }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject empty cluster name");
        assert!(
            matches!(&err, crate::ConfigError::UnknownCluster(name) if name.is_empty()),
            "expected UnknownCluster(\"\"); got: {err:?}"
        );
    }

    #[test]
    fn parses_cluster_with_type_strict_dns() {
        // 05.1 NEW: ClusterType gains StrictDns variant. The serde tag STRICT_DNS
        // maps mechanically via the existing #[serde(rename_all = "SCREAMING_SNAKE_CASE")].
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(
            matches!(c.cluster_type, ClusterType::StrictDns),
            "expected ClusterType::StrictDns, got {:?}",
            c.cluster_type,
        );
        assert_eq!(c.name, "backend");
        assert_eq!(
            c.load_assignment.endpoints[0].lb_endpoints[0]
                .endpoint
                .address
                .socket_address
                .address,
            "localhost",
        );
    }

    #[test]
    fn parses_cluster_with_type_static_unchanged() {
        // 05.1 NEW: regression guard — the existing STATIC parse path stays
        // unchanged after StrictDns lands. (Phase-02.1 REVIEW I3 originally
        // requested this discriminator test; the positive Static runtime test
        // lands separately in envoy-cluster as static_cluster_constructs_with_literal_ip.)
        let yaml = r#"
static_resources:
  listeners: []
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
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(
            matches!(c.cluster_type, ClusterType::Static),
            "expected ClusterType::Static, got {:?}",
            c.cluster_type,
        );
    }

    #[test]
    fn rejects_cluster_with_type_logical_dns() {
        // 05.1 NEW: documents the ADR-0023 LOGICAL_DNS deferral at the parser surface.
        // serde rejects with an "unknown variant" error naming LOGICAL_DNS. If a
        // future phase lifts the deferral, this test gets renamed to
        // parses_cluster_with_type_logical_dns and the assertion flips.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: LOGICAL_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: example.com
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject LOGICAL_DNS");
        let s = err.to_string();
        assert!(
            s.contains("LOGICAL_DNS") || s.contains("unknown variant"),
            "expected LOGICAL_DNS unknown-variant error, got: {s}",
        );
    }

    #[test]
    fn rejects_cluster_with_unknown_type_value() {
        // 05.1 NEW: covers the deny_unknown_fields-equivalent posture on the
        // variant tag — any tag that isn't STATIC or STRICT_DNS rejects.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: WEIRD_TYPE
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject WEIRD_TYPE");
        let s = err.to_string();
        assert!(
            s.contains("WEIRD_TYPE") || s.contains("unknown variant"),
            "expected WEIRD_TYPE unknown-variant error, got: {s}",
        );
    }

    #[test]
    fn parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment() {
        // 05.1 NEW: verifies that DNS-name endpoints are stored as raw strings at
        // config-parse time (resolution lands at runtime in envoy-cluster's
        // from_bootstrap, NOT at parse time). Two endpoints with the same DNS name
        // but different ports parse cleanly into the Vec<LbEndpoint>.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7001
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(matches!(c.cluster_type, ClusterType::StrictDns));
        let lbe = &c.load_assignment.endpoints[0].lb_endpoints;
        assert_eq!(lbe.len(), 2);
        assert_eq!(lbe[0].endpoint.address.socket_address.address, "localhost");
        assert_eq!(lbe[0].endpoint.address.socket_address.port_value, 7000);
        assert_eq!(lbe[1].endpoint.address.socket_address.address, "localhost");
        assert_eq!(lbe[1].endpoint.address.socket_address.port_value, 7001);
    }

    #[test]
    fn validates_strict_dns_cluster_does_not_require_literal_ip_endpoints() {
        // 05.1 NEW: explicit assertion that envoy-config's validator passes the
        // parse stage for STRICT_DNS clusters even though the endpoint address is
        // a DNS name (not a literal IP). The runtime-side endpoint parse via
        // SocketAddr::from_str (which would fail on "host.docker.internal") lives
        // in envoy-cluster's from_bootstrap, NOT in envoy-config's validator —
        // and envoy-cluster's STRICT_DNS arm uses tokio::net::lookup_host instead
        // of SocketAddr::from_str on the StrictDns path, so the DNS-name endpoint
        // is fine end-to-end.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: host.docker.internal
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        // The validator passes the parse stage cleanly; runtime resolution is
        // out of scope for this test (envoy-cluster's from_bootstrap is not
        // invoked here).
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(matches!(c.cluster_type, ClusterType::StrictDns));
        assert_eq!(
            c.load_assignment.endpoints[0].lb_endpoints[0]
                .endpoint
                .address
                .socket_address
                .address,
            "host.docker.internal",
        );
    }

    /// 05.4 NEW (D1, ADR-0024): Cluster gains `dns_lookup_family: Option<DnsLookupFamily>`.
    /// The field is parsed-and-stored on envoy-rust's typed Cluster struct; runtime
    /// non-consumption is deliberate per ADR-0024 (only the upstream Envoy side
    /// observes the V4_ONLY knob via the D2 envoy.yaml edit).
    #[test]
    fn parses_cluster_with_dns_lookup_family_v4_only() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: "host.docker.internal", port_value: 9001 } }
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses");
        assert_eq!(bootstrap.static_resources.clusters.len(), 1);
        let c = &bootstrap.static_resources.clusters[0];
        assert!(matches!(c.cluster_type, ClusterType::StrictDns));
        assert_eq!(c.dns_lookup_family, Some(DnsLookupFamily::V4Only));
    }

    /// 05.4 NEW (D3, ADR-0026): Listener gains `listener_filters: Vec<serde_yaml::Value>`
    /// parse-and-ignore field. envoy-rust never executes listener filters by design
    /// (phase 03.2 chose to put SNI dispatch at the rustls layer); the field is
    /// purely for upstream-Envoy `envoy.yaml` parseability. New pattern in
    /// envoy-config — see ADR-0026.
    #[test]
    fn parses_listener_with_tls_inspector_listener_filter() {
        let yaml = r#"
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: "0.0.0.0", port_value: 0 } }
      listener_filters:
        - name: envoy.filters.listener.tls_inspector
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.listener.tls_inspector.v3.TlsInspector
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: "/tmp/leaf.pem" }
                    private_key:       { filename: "/tmp/leaf.key" }
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
                  address: { socket_address: { address: "127.0.0.1", port_value: 9001 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses");
        assert_eq!(bootstrap.static_resources.listeners.len(), 1);
        let listener = &bootstrap.static_resources.listeners[0];
        assert_eq!(listener.listener_filters.len(), 1);
        // Smoke-check the opaque value contains the tls_inspector filter name.
        let filter_yaml =
            serde_yaml::to_string(&listener.listener_filters[0]).expect("filter serialises back");
        assert!(
            filter_yaml.contains("envoy.filters.listener.tls_inspector"),
            "filter yaml should contain tls_inspector name: {filter_yaml:?}"
        );
    }

    // --- phase 05.2 Task 2: codec_type: HTTP2 accept-flip + TLS-rejection ---

    #[test]
    fn parses_hcm_with_codec_type_http2() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("expected HCM");
        };
        assert!(matches!(hcm.codec_type, CodecType::HTTP2));
    }

    #[test]
    fn rejects_hcm_with_codec_type_http2_on_tls_listener() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: tls_h2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/cert.pem }
                    private_key: { filename: /tmp/key.pem }
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject TLS+HTTP2");
        assert!(
            matches!(err, crate::ConfigError::Http2OverTlsNotSupported),
            "expected Http2OverTlsNotSupported, got {err:?}"
        );
    }

    // --- phase 05.2 Task 3: Http2ProtocolOptions struct + RFC 7540 ranges ---

    #[test]
    fn parses_hcm_http2_protocol_options_default() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else {
            panic!();
        };
        assert!(hcm.http2_protocol_options.is_none());
    }

    #[test]
    fn parses_hcm_http2_protocol_options_all_fields() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                http2_protocol_options:
                  max_concurrent_streams: 50
                  initial_stream_window_size: 131072
                  initial_connection_window_size: 262144
                  max_frame_size: 32768
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else {
            panic!();
        };
        let opts = hcm.http2_protocol_options.as_ref().expect("present");
        assert_eq!(opts.max_concurrent_streams, Some(50));
        assert_eq!(opts.initial_stream_window_size, Some(131072));
        assert_eq!(opts.initial_connection_window_size, Some(262144));
        assert_eq!(opts.max_frame_size, Some(32768));
    }

    #[test]
    fn rejects_http2_protocol_options_max_frame_size_too_small() {
        let yaml = http2_options_yaml(/* max_frame_size = */ Some(1024), None, None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        match err {
            crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                field,
                value,
                range,
            } => {
                assert_eq!(field, "max_frame_size");
                assert_eq!(value, 1024);
                assert_eq!(range, (16384, 16777215));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_http2_protocol_options_max_frame_size_too_large() {
        let yaml = http2_options_yaml(Some(17_000_000), None, None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "max_frame_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_initial_stream_window_size_too_large() {
        // 2^31 = 2147483648 is one above the max.
        let yaml = http2_options_yaml(None, Some(2_147_483_648), None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "initial_stream_window_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_initial_connection_window_size_too_large() {
        let yaml = http2_options_yaml(None, None, Some(2_147_483_648), None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "initial_connection_window_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_unknown_field() {
        // hpack_table_size is a real Envoy field; envoy-rust 05.2 doesn't ship
        // it. The struct's deny_unknown_fields rejects.
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                http2_protocol_options:
                  hpack_table_size: 4096
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn fuzz_corpus_hcm_codec_http2_seed_parses() {
        // Sanity-check that the new fuzz seed parses cleanly through the
        // serde + validator pipeline. First per-seed content-asserting
        // corpus-walk test in this file; the cohort-level pattern lives
        // in `fuzz_corpus_seeds_parse_or_reject_cleanly` (~line 2274).
        let yaml = include_str!("../fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml");
        let bs = crate::parse_bootstrap(yaml).expect("seed must parse");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else {
            panic!();
        };
        assert!(matches!(hcm.codec_type, CodecType::HTTP2));
        let opts = hcm
            .http2_protocol_options
            .as_ref()
            .expect("seed has options");
        assert_eq!(opts.max_concurrent_streams, Some(100));
    }

    #[test]
    fn fuzz_corpus_cluster_http2_protocol_options_seed_parses() {
        // Mirrors the existing fuzz_corpus_hcm_codec_http2_seed_parses
        // pattern. The seed exercises the cluster-side
        // typed_extension_protocol_options accept-path; the fuzzer never runs
        // the H2 codec or the runtime cluster construction
        // (`parse_bootstrap` only exercises serde + the validator).
        let yaml =
            include_str!("../fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml");
        crate::parse_bootstrap(yaml).expect("seed parses cleanly");
    }

    /// Builds a minimal HCM `codec_type: HTTP2` bootstrap with the given
    /// http2_protocol_options field values. Helper for the 4 range-rejection
    /// tests above. Each Option<u32> argument controls one field.
    fn http2_options_yaml(
        max_frame_size: Option<u32>,
        initial_stream_window_size: Option<u32>,
        initial_connection_window_size: Option<u32>,
        max_concurrent_streams: Option<u32>,
    ) -> String {
        let mut opts_block = String::from("                http2_protocol_options:\n");
        if let Some(v) = max_frame_size {
            opts_block.push_str(&format!("                  max_frame_size: {v}\n"));
        }
        if let Some(v) = initial_stream_window_size {
            opts_block.push_str(&format!(
                "                  initial_stream_window_size: {v}\n"
            ));
        }
        if let Some(v) = initial_connection_window_size {
            opts_block.push_str(&format!(
                "                  initial_connection_window_size: {v}\n"
            ));
        }
        if let Some(v) = max_concurrent_streams {
            opts_block.push_str(&format!("                  max_concurrent_streams: {v}\n"));
        }
        format!(
            r#"
node: {{ id: x, cluster: y }}
static_resources:
  listeners:
    - name: h2
      address: {{ socket_address: {{ address: 0.0.0.0, port_value: 9000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
{opts_block}                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
        )
    }

    #[test]
    fn parses_cluster_with_typed_extension_protocol_options_http2() {
        // 06.3 D14.3: changed from codec_type HTTP1 → HTTP2 so the H2 cluster
        // target remains valid under the new H1×H2 reachability gate. The
        // purpose of this test is to verify typed_extension_protocol_options
        // parsing, not codec negotiation; HTTP2 listener + H2 cluster is the
        // correct canonical shape.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_concurrent_streams: 100
              initial_stream_window_size: 65535
              initial_connection_window_size: 65535
              max_frame_size: 16384
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        let teo = cluster
            .typed_extension_protocol_options
            .as_ref()
            .expect("typed_extension_protocol_options present");
        let h2 = teo
            .http_protocol_options
            .explicit_http_config
            .http2_protocol_options
            .as_ref()
            .expect("http2 arm present");
        assert_eq!(h2.max_concurrent_streams, Some(100));
        assert_eq!(h2.max_frame_size, Some(16384));
    }

    #[test]
    fn parses_cluster_with_typed_extension_protocol_options_http1() {
        // The H1 arm of explicit_http_config is the empty Http1ProtocolOptions.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        let teo = cluster
            .typed_extension_protocol_options
            .as_ref()
            .expect("teo present");
        assert!(
            teo.http_protocol_options
                .explicit_http_config
                .http_protocol_options
                .is_some()
        );
        assert!(
            teo.http_protocol_options
                .explicit_http_config
                .http2_protocol_options
                .is_none()
        );
    }

    #[test]
    fn rejects_cluster_with_both_http1_and_http2_in_explicit_http_config() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects mutual");
        assert!(
            matches!(
                err,
                crate::ConfigError::MutuallyExclusiveExplicitHttpConfig { ref cluster }
                    if cluster == "backend"
            ),
            "expected MutuallyExclusiveExplicitHttpConfig {{cluster: backend}}, got {err:?}"
        );
    }

    #[test]
    fn rejects_cluster_with_wrong_typed_config_url() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.config.core.v3.Http2ProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects wrong URL");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedTypedConfigUrl { .. }),
            "expected UnsupportedTypedConfigUrl, got {err:?}"
        );
    }

    #[test]
    fn rejects_cluster_http2_protocol_options_max_frame_size_too_small() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_frame_size: 1024
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects out-of-range");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                    field: "max_frame_size",
                    value: 1024,
                    ..
                }
            ),
            "expected Http2ProtocolOptionsOutOfRange, got {err:?}"
        );
    }

    #[test]
    fn parses_cluster_without_typed_extension_protocol_options_defaults_to_http1() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        assert!(cluster.typed_extension_protocol_options.is_none());
    }

    #[test]
    fn rejects_cluster_with_unknown_typed_extension_key() {
        // Key other than HttpProtocolOptions; serde deny_unknown_fields rejects.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.UnknownExtension":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.UnknownExtension
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects unknown key");
        // serde "unknown field" on TypedExtensionProtocolOptions surfaces as
        // ConfigError::Yaml (the deny_unknown_fields path on the
        // TypedExtensionProtocolOptions wrapper struct).
        assert!(
            matches!(err, crate::ConfigError::Yaml(_)),
            "expected serde Yaml error for unknown typed-extension key, got {err:?}"
        );
    }

    #[test]
    fn parses_cluster_with_strict_dns_and_http2_protocol_options_combined() {
        // Load-bearing for fixture 0010 (Task 10) which combines exactly these
        // two surfaces.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses STRICT_DNS + H2 combined");
        let cluster = &bs.static_resources.clusters[0];
        assert!(matches!(cluster.cluster_type, ClusterType::StrictDns));
        assert!(cluster.typed_extension_protocol_options.is_some());
    }

    // --- 06.1 Task 9: Admin.access_log_path parse-and-ignore (ADR-0026) ---

    #[test]
    fn parses_admin_with_access_log_path() {
        let yaml = r#"
node: { id: t, cluster: t }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
  access_log_path: /var/log/envoy_admin.log
static_resources: { listeners: [], clusters: [] }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse OK");
        let admin = bootstrap.admin.expect("admin present");
        assert_eq!(
            admin.access_log_path,
            Some("/var/log/envoy_admin.log".to_string())
        );
    }

    #[test]
    fn parses_admin_without_access_log_path() {
        let yaml = r#"
node: { id: t, cluster: t }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources: { listeners: [], clusters: [] }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse OK");
        let admin = bootstrap.admin.expect("admin present");
        assert_eq!(admin.access_log_path, None);
    }

    #[test]
    fn rejects_admin_with_unknown_field() {
        let yaml = r#"
node: { id: t, cluster: t }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
  profile_path: /tmp
static_resources: { listeners: [], clusters: [] }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("unknown field rejected");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("profile_path") || msg.contains("unknown field"),
            "diagnostic should mention the unknown field; got: {msg}"
        );
    }

    // ----- 06.2 Task 5: access_log schema tests -----

    fn hcm_with_access_log_yaml(access_log_block: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: t }}
static_resources:
  listeners:
    - name: l1
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
{access_log_block}
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
            access_log_block = access_log_block
        )
    }

    #[test]
    fn parses_hcm_with_file_access_log() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let listener = &bootstrap.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let hcm = match &filter.typed_config {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        assert_eq!(hcm.access_log.len(), 1);
        assert_eq!(hcm.access_log[0].name, "envoy.access_loggers.file");
        match &hcm.access_log[0].typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                assert_eq!(cfg.path, "/tmp/access.log");
            }
        }
    }

    #[test]
    fn parses_hcm_with_no_access_log_block() {
        let yaml = hcm_with_access_log_yaml("");
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0]
            .typed_config
        {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        assert!(hcm.access_log.is_empty());
    }

    #[test]
    fn parses_hcm_with_empty_access_log_array() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log: []
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0]
            .typed_config
        {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        assert!(hcm.access_log.is_empty());
    }

    #[test]
    fn rejects_hcm_with_unsupported_access_log_name() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.stdout
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        match err {
            crate::ConfigError::UnsupportedAccessLogType { actual } => {
                assert_eq!(actual, "envoy.access_loggers.stdout");
            }
            other => panic!("expected UnsupportedAccessLogType; got {:?}", other),
        }
    }

    #[test]
    fn rejects_hcm_with_unsupported_access_log_type_url() {
        // The serde-tagged `@type` enum rejects unknown URLs at
        // deserialization time (wrapped as ConfigError::Yaml).
        // The test accepts either error path.
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.unknown.v3.UnknownAccessLog
                      path: /tmp/access.log
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        match err {
            crate::ConfigError::Yaml(_) => {}
            crate::ConfigError::UnsupportedAccessLogType { .. } => {}
            other => panic!("expected Yaml or UnsupportedAccessLogType; got {:?}", other),
        }
    }

    #[test]
    fn rejects_hcm_with_empty_access_log_path() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: ""
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(err, crate::ConfigError::InvalidAccessLogPath));
    }

    // --- 06.3 Task 2: D14.3 H1-listener × H2-cluster parse-time validator gate ---

    /// Positive: codec_type HTTP1 + cluster with NO typed_extension_protocol_options →
    /// validator accepts. The default-H1 cluster is always reachable from an H1 listener.
    #[test]
    fn validates_h1_listener_with_h1_cluster_passes() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b).expect("H1 listener + H1 cluster (no teo) must be accepted");
    }

    /// Positive: codec_type HTTP2 + cluster with typed_extension_protocol_options carrying
    /// http2_protocol_options → validator accepts. H2×H2 is the canonical H2 path.
    #[test]
    fn validates_h2_listener_with_h2_cluster_passes() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b).expect("H2 listener + H2 cluster must be accepted");
    }

    /// Positive: codec_type HTTP2 + cluster with NO typed_extension_protocol_options →
    /// validator accepts. Per 05.3 D4, an H2 listener proxying to an H1 cluster MUST
    /// keep working — the gate is H1/AUTO × H2-cluster only, not H2 × H1-cluster.
    #[test]
    fn validates_h2_listener_with_h1_cluster_passes() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b).expect("H2 listener + H1 cluster (no teo) must be accepted");
    }

    /// Negative: codec_type HTTP1 + cluster carries http2_protocol_options →
    /// validator returns ConfigError::Http2ClusterFromHttp1Listener.
    /// Closes 05.3 REVIEW I1 per ADR-0028 option-(B) deferral gate.
    #[test]
    fn rejects_h1_listener_with_h2_cluster() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&mut b).unwrap_err();
        match err {
            crate::ConfigError::Http2ClusterFromHttp1Listener {
                ref listener,
                ref cluster,
            } => {
                assert_eq!(listener, "ingress_http");
                assert_eq!(cluster, "backend");
            }
            other => panic!(
                "expected Http2ClusterFromHttp1Listener {{listener: ingress_http, cluster: backend}}, got {:?}",
                other
            ),
        }
    }

    /// Negative: codec_type AUTO + cluster carries http2_protocol_options →
    /// same rejection as HTTP1. AUTO is treated as H1-only per parent §4 of the spec.
    #[test]
    fn rejects_auto_listener_with_h2_cluster() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: AUTO
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&mut b).unwrap_err();
        match err {
            crate::ConfigError::Http2ClusterFromHttp1Listener {
                ref listener,
                ref cluster,
            } => {
                assert_eq!(listener, "ingress_http");
                assert_eq!(cluster, "backend");
            }
            other => panic!(
                "expected Http2ClusterFromHttp1Listener {{listener: ingress_http, cluster: backend}}, got {:?}",
                other
            ),
        }
    }

    /// Carve-out: TCP-proxy listener (no codec_type, no HCM) + cluster carrying
    /// http2_protocol_options → validator accepts. The H1×H2 gate is HCM-scoped only;
    /// TCP-proxy routes don't undergo codec negotiation on the listener side.
    #[test]
    fn tcp_proxy_listener_with_h2_cluster_unaffected() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_tcp
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
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
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b)
            .expect("TCP-proxy listener + H2 cluster must be accepted (gate is HCM-only)");
    }

    #[test]
    fn validate_http_filters_accepts_single_router() {
        let filters = vec![crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }];
        let result = super::validate_http_filters(&filters, "ingress_http");
        assert!(result.is_ok(), "single Router passes; got {result:?}");
    }

    #[test]
    fn validate_http_filters_rejects_empty_list() {
        let filters: Vec<crate::HttpFilter> = Vec::new();
        let err =
            super::validate_http_filters(&filters, "ingress_http").expect_err("empty list rejects");
        match err {
            crate::ConfigError::EmptyHttpFilters { listener } => {
                assert_eq!(listener, "ingress_http");
            }
            other => panic!("expected EmptyHttpFilters, got {other:?}"),
        }
    }

    #[test]
    fn validate_http_filters_rejects_duplicate_router() {
        let router = || crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        };
        let filters = vec![router(), router()];
        let err = super::validate_http_filters(&filters, "ingress_http")
            .expect_err("duplicate Router rejects");
        match err {
            crate::ConfigError::DuplicateRouterFilter { listener } => {
                assert_eq!(listener, "ingress_http");
            }
            other => panic!("expected DuplicateRouterFilter, got {other:?}"),
        }
    }

    #[test]
    fn validate_http_filters_rejects_name_typed_config_mismatch() {
        let filters = vec![crate::HttpFilter {
            name: "envoy.filters.http.fault".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }];
        let err = super::validate_http_filters(&filters, "ingress_http")
            .expect_err("name/typed_config mismatch rejects");
        match err {
            crate::ConfigError::UnsupportedHttpFilter { name } => {
                assert_eq!(name, "envoy.filters.http.fault");
            }
            other => panic!("expected UnsupportedHttpFilter, got {other:?}"),
        }
    }

    #[test]
    fn validate_http_filters_listener_name_propagates() {
        let filters: Vec<crate::HttpFilter> = Vec::new();
        let err = super::validate_http_filters(&filters, "custom_listener_42")
            .expect_err("empty list rejects");
        assert!(format!("{err:?}").contains("custom_listener_42"));
    }

    #[test]
    fn validate_http_filters_duplicate_router_takes_precedence_over_router_not_terminal() {
        let router = || crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        };
        let filters = vec![router(), router(), router()];
        let err =
            super::validate_http_filters(&filters, "ingress_http").expect_err("3 Routers rejects");
        assert!(matches!(
            err,
            crate::ConfigError::DuplicateRouterFilter { .. }
        ));
    }

    #[test]
    fn validate_http_filters_accepts_existing_fixture_shape() {
        let filters = vec![crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }];
        super::validate_http_filters(&filters, "ingress_http")
            .expect("pre-07.1 fixture filter-chain shape stays valid");
    }

    mod header_mutation_schema_tests {
        use crate::{AppendAction, HeaderMutationConfig};

        fn parse(yaml: &str) -> Result<HeaderMutationConfig, serde_yaml::Error> {
            serde_yaml::from_str(yaml)
        }

        #[test]
        fn minimal_request_only_mutations_parse() {
            let cfg = parse(
                "mutations:\n  request_mutations:\n    - append:\n        header:\n          key: x-foo\n          value: bar\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect("request-only parses");
            assert_eq!(cfg.mutations.request_mutations.len(), 1);
            assert_eq!(cfg.mutations.response_mutations.len(), 0);
            let e = &cfg.mutations.request_mutations[0];
            assert_eq!(e.append.header.key, "x-foo");
            assert_eq!(e.append.header.value, "bar");
            assert_eq!(e.append.append_action, AppendAction::AppendIfExistsOrAdd);
        }

        #[test]
        fn minimal_response_only_mutations_parse() {
            let cfg = parse(
                "mutations:\n  response_mutations:\n    - append:\n        header:\n          key: x-resp\n          value: stamp\n        append_action: OVERWRITE_IF_EXISTS_OR_ADD\n",
            )
            .expect("response-only parses");
            assert_eq!(cfg.mutations.request_mutations.len(), 0);
            assert_eq!(cfg.mutations.response_mutations.len(), 1);
            assert_eq!(
                cfg.mutations.response_mutations[0].append.append_action,
                AppendAction::OverwriteIfExistsOrAdd
            );
        }

        #[test]
        fn both_request_and_response_mutations_parse() {
            let cfg = parse(
                "mutations:\n  request_mutations:\n    - append:\n        header:\n          key: x-req\n          value: a\n        append_action: APPEND_IF_EXISTS_OR_ADD\n  response_mutations:\n    - append:\n        header:\n          key: x-resp\n          value: b\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect("both parse");
            assert_eq!(cfg.mutations.request_mutations.len(), 1);
            assert_eq!(cfg.mutations.response_mutations.len(), 1);
            assert_eq!(
                cfg.mutations.request_mutations[0].append.header.key,
                "x-req"
            );
            assert_eq!(
                cfg.mutations.response_mutations[0].append.header.key,
                "x-resp"
            );
        }

        #[test]
        fn empty_mutations_parse_via_serde_default() {
            let cfg = parse("mutations: {}\n").expect("empty mutations parse");
            assert_eq!(cfg.mutations.request_mutations, Vec::new());
            assert_eq!(cfg.mutations.response_mutations, Vec::new());
        }

        #[test]
        fn multiple_entries_parse() {
            let cfg = parse(
                "mutations:\n  request_mutations:\n    - append:\n        header: { key: x-a, value: '1' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n    - append:\n        header: { key: x-b, value: '2' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n    - append:\n        header: { key: x-c, value: '3' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect("3 entries parse");
            assert_eq!(cfg.mutations.request_mutations.len(), 3);
        }

        #[test]
        fn both_supported_append_actions_parse() {
            for (yaml_val, expect) in [
                ("APPEND_IF_EXISTS_OR_ADD", AppendAction::AppendIfExistsOrAdd),
                (
                    "OVERWRITE_IF_EXISTS_OR_ADD",
                    AppendAction::OverwriteIfExistsOrAdd,
                ),
            ] {
                let cfg = parse(&format!(
                    "mutations:\n  request_mutations:\n    - append:\n        header: {{ key: k, value: v }}\n        append_action: {yaml_val}\n"
                ))
                .expect("supported action parses");
                assert_eq!(
                    cfg.mutations.request_mutations[0].append.append_action,
                    expect
                );
            }
        }

        #[test]
        fn unsupported_append_actions_parse_at_schema_level() {
            // ADD_IF_ABSENT / OVERWRITE_IF_EXISTS parse at the schema layer; the
            // Task 2 validator rejects them. Present in the enum so serde does not
            // emit a generic "unknown variant" error.
            for (yaml_val, expect) in [
                ("ADD_IF_ABSENT", AppendAction::AddIfAbsent),
                ("OVERWRITE_IF_EXISTS", AppendAction::OverwriteIfExists),
            ] {
                let cfg = parse(&format!(
                    "mutations:\n  request_mutations:\n    - append:\n        header: {{ key: k, value: v }}\n        append_action: {yaml_val}\n"
                ))
                .expect("unsupported action still parses at schema level");
                assert_eq!(
                    cfg.mutations.request_mutations[0].append.append_action,
                    expect
                );
            }
        }

        #[test]
        fn unknown_field_rejects() {
            let err = parse("mutations:\n  request_mutations: []\n  bogus_key: 1\n")
                .expect_err("unknown field rejects");
            assert!(format!("{err}").contains("bogus_key") || format!("{err}").contains("unknown"));
        }

        #[test]
        fn missing_mutations_field_rejects() {
            parse("not_mutations: {}\n").expect_err("missing `mutations` rejects");
        }

        #[test]
        fn missing_key_field_rejects() {
            parse(
                "mutations:\n  request_mutations:\n    - append:\n        header: { value: v }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect_err("missing header.key rejects");
        }

        #[test]
        fn missing_value_field_rejects() {
            parse(
                "mutations:\n  request_mutations:\n    - append:\n        header: { key: k }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect_err("missing header.value rejects");
        }

        #[test]
        fn unknown_at_type_url_rejects_on_http_filter() {
            // The tagged-enum on an unknown @type tag rejects.
            let err: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(
                "\"@type\": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation.unknown\n",
            );
            err.expect_err("unknown @type rejects");
        }
    }

    mod header_mutation_validator_tests {
        use crate::{
            AppendAction, HeaderMutationConfig, HeaderMutationEntry, HeaderValue,
            HeaderValueOption, HttpFilter, HttpFilterTypedConfig, Mutations, RouterConfig,
        };

        fn entry(key: &str, value: &str, action: AppendAction) -> HeaderMutationEntry {
            HeaderMutationEntry {
                append: HeaderValueOption {
                    header: HeaderValue {
                        key: key.to_string(),
                        value: value.to_string(),
                    },
                    append_action: action,
                },
            }
        }

        fn header_mutation_filter(
            request_mutations: Vec<HeaderMutationEntry>,
            response_mutations: Vec<HeaderMutationEntry>,
        ) -> HttpFilter {
            HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: HttpFilterTypedConfig::HeaderMutation(HeaderMutationConfig {
                    mutations: Mutations {
                        request_mutations,
                        response_mutations,
                    },
                }),
            }
        }

        fn router_filter() -> HttpFilter {
            HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }
        }

        #[test]
        fn header_mutation_with_all_supported_entries_passes() {
            let filters = vec![
                header_mutation_filter(
                    vec![
                        entry("x-a", "1", AppendAction::AppendIfExistsOrAdd),
                        entry("x-b", "2", AppendAction::OverwriteIfExistsOrAdd),
                    ],
                    vec![
                        entry("x-c", "3", AppendAction::AppendIfExistsOrAdd),
                        entry("x-d", "4", AppendAction::OverwriteIfExistsOrAdd),
                    ],
                ),
                router_filter(),
            ];
            super::validate_http_filters(&filters, "ingress_http").expect("supported entries pass");
        }

        #[test]
        fn empty_key_rejects() {
            let filters = vec![
                header_mutation_filter(
                    vec![entry("", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("empty key rejects")
            {
                crate::ConfigError::EmptyHeaderMutationKey { listener, position } => {
                    assert_eq!(listener, "ingress_http");
                    assert_eq!(position, 0);
                }
                other => panic!("expected EmptyHeaderMutationKey, got {other:?}"),
            }
        }

        #[test]
        fn invalid_token_in_key_rejects() {
            let filters = vec![
                header_mutation_filter(
                    vec![entry("x bad", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("invalid token rejects")
            {
                crate::ConfigError::InvalidHeaderMutationKey {
                    listener,
                    position,
                    key,
                } => {
                    assert_eq!(listener, "ingress_http");
                    assert_eq!(position, 0);
                    assert_eq!(key, "x bad");
                }
                other => panic!("expected InvalidHeaderMutationKey, got {other:?}"),
            }
        }

        #[test]
        fn add_if_absent_rejects() {
            let filters = vec![
                header_mutation_filter(vec![entry("x-a", "v", AppendAction::AddIfAbsent)], vec![]),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("ADD_IF_ABSENT rejects")
            {
                crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener,
                    position,
                    action,
                } => {
                    assert_eq!(listener, "ingress_http");
                    assert_eq!(position, 0);
                    assert_eq!(action, "ADD_IF_ABSENT");
                }
                other => panic!("expected UnsupportedHeaderMutationAppendAction, got {other:?}"),
            }
        }

        #[test]
        fn overwrite_if_exists_rejects() {
            let filters = vec![
                header_mutation_filter(
                    vec![],
                    vec![entry("x-a", "v", AppendAction::OverwriteIfExists)],
                ),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("OVERWRITE_IF_EXISTS rejects")
            {
                crate::ConfigError::UnsupportedHeaderMutationAppendAction { action, .. } => {
                    assert_eq!(action, "OVERWRITE_IF_EXISTS");
                }
                other => panic!("expected UnsupportedHeaderMutationAppendAction, got {other:?}"),
            }
        }

        #[test]
        fn router_not_terminal_still_rejects_under_header_mutation_chain() {
            // [Router, HeaderMutation] — Router first; the 07.1 Task 4 validator
            // still fires RouterNotTerminal.
            let filters = vec![
                router_filter(),
                header_mutation_filter(
                    vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("Router-not-terminal rejects")
            {
                crate::ConfigError::RouterNotTerminal { position, .. } => {
                    assert_eq!(position, 0)
                }
                other => panic!("expected RouterNotTerminal, got {other:?}"),
            }
        }

        #[test]
        fn duplicate_router_rejects_under_header_mutation_chain() {
            let filters = vec![
                header_mutation_filter(
                    vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
                router_filter(),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("duplicate Router rejects")
            {
                crate::ConfigError::DuplicateRouterFilter { .. } => {}
                other => panic!("expected DuplicateRouterFilter, got {other:?}"),
            }
        }

        #[test]
        fn name_typed_config_mismatch_rejects() {
            let mut f = header_mutation_filter(vec![], vec![]);
            f.name = "envoy.filters.http.fault".to_string();
            let filters = vec![f, router_filter()];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("name/typed_config mismatch rejects")
            {
                crate::ConfigError::UnsupportedHttpFilter { name } => {
                    assert_eq!(name, "envoy.filters.http.fault");
                }
                other => panic!("expected UnsupportedHttpFilter, got {other:?}"),
            }
        }
    }
}
