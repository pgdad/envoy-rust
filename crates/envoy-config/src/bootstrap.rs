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
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ClusterType {
    Static,
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
#[derive(Debug, Deserialize, PartialEq)]
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
}

/// Empty in 04.1; Envoy's Router has many fields (suppress_envoy_headers,
/// dynamic_stats, start_child_span, ...); all deferred per SPEC §4.
#[derive(Debug, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct RouterConfig {}

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

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Route {
    #[serde(rename = "match")]
    pub r#match: RouteMatch,
    pub direct_response: DirectResponse,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteMatch {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponse {
    pub status: u16,
    pub body: DataSource,
}

pub(crate) fn validate(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
    let listeners = &bootstrap.static_resources.listeners;
    let clusters = &bootstrap.static_resources.clusters;
    if listeners.len() > 1 {
        return Err(crate::ConfigError::TooManyListeners(listeners.len()));
    }
    if bootstrap.admin.is_none() && listeners.is_empty() {
        return Err(crate::ConfigError::NoRuntime);
    }

    // Per-cluster invariants.
    for cluster in clusters {
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
                    if ctx.common_tls_context.validation_context.is_none() {
                        return Err(crate::ConfigError::MissingValidationContext);
                    }
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
    }

    // Per-listener invariants.
    for listener in listeners {
        for chain in &listener.filter_chains {
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
                    }
                    TransportSocketTypedConfig::Upstream(_) => {
                        return Err(crate::ConfigError::MismatchedTransportSocketDirection {
                            side: "listener",
                            got: "UpstreamTlsContext",
                        });
                    }
                }
            }
            for filter in &chain.filters {
                match filter.name.as_str() {
                    crate::ECHO_FILTER => {
                        if filter.typed_config.is_some() {
                            return Err(crate::ConfigError::UnexpectedTypedConfig(
                                crate::ECHO_FILTER,
                            ));
                        }
                    }
                    crate::TCP_PROXY_FILTER => {
                        // Phase 04.1: TypedConfig now has multiple variants
                        // (TcpProxy, HttpConnectionManager). Match instead of
                        // irrefutable destructuring; the HCM-on-tcp_proxy-name
                        // case is rejected as a missing typed_config (the
                        // tcp_proxy filter requires its own typed_config; an
                        // HCM typed_config under the tcp_proxy name is
                        // misconfiguration). Task 2 will tighten this with a
                        // dedicated error variant; here we keep the same error
                        // surface as before.
                        let typed = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::TCP_PROXY_FILTER),
                        )?;
                        let TypedConfig::TcpProxy(tp) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(
                                crate::TCP_PROXY_FILTER,
                            ));
                        };
                        if !clusters.iter().any(|c| c.name == tp.cluster) {
                            return Err(crate::ConfigError::UnknownCluster(tp.cluster.clone()));
                        }
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
        assert_eq!(route.direct_response.status, 200);
        assert_eq!(
            route.direct_response.body.inline_string.as_deref(),
            Some("ok\n")
        );
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
        let yaml = r#"
stat_prefix: ingress_http
codec_type: HTTP1
access_log: []
route_config:
  name: r
  virtual_hosts: []
http_filters: []
"#;
        let res: Result<HttpConnectionManagerConfig, _> = serde_yaml::from_str(yaml);
        assert!(res.is_err(), "deny_unknown_fields should reject access_log");
        let err = res.err().unwrap().to_string();
        assert!(
            err.contains("access_log") || err.contains("unknown field"),
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
}
