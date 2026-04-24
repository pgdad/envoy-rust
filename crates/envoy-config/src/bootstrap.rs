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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cluster {
    pub name: String,
    // Phase 02 extends with type, lb_policy, load_assignment, etc.
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    #[allow(dead_code)]
    pub name: String,
    pub address: Address,
    pub filter_chains: Vec<FilterChain>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub socket_address: SocketAddress,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocketAddress {
    pub address: String,
    pub port_value: u16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    pub filters: Vec<NetworkFilter>,
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

pub(crate) fn validate(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
    let listeners = &bootstrap.static_resources.listeners;
    if listeners.len() > 1 {
        return Err(crate::ConfigError::TooManyListeners(listeners.len()));
    }
    if bootstrap.admin.is_none() && listeners.is_empty() {
        return Err(crate::ConfigError::NoRuntime);
    }
    for listener in listeners {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                if filter.name != crate::ECHO_FILTER {
                    return Err(crate::ConfigError::UnsupportedFilter(
                        filter.name.clone(),
                        crate::ECHO_FILTER,
                    ));
                }
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
    fn parses_bootstrap_with_clusters_stub() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: cluster_0
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.static_resources.clusters.len(), 1);
        assert_eq!(b.static_resources.clusters[0].name, "cluster_0");
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
    fn rejects_non_echo_filter() {
        let yaml = MINIMAL.replace(
            "envoy.filters.network.echo",
            "envoy.filters.network.tcp_proxy",
        );
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
    - name: cluster_0
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
"#;
        let b: Bootstrap = serde_yaml::from_str(yaml).expect("valid YAML");
        let filter = &b.static_resources.listeners[0].filter_chains[0].filters[0];
        assert_eq!(filter.name, "envoy.filters.network.tcp_proxy");
        match filter.typed_config.as_ref().expect("typed_config present") {
            TypedConfig::TcpProxy(tp) => {
                assert_eq!(tp.stat_prefix, "ingress_tcp");
                assert_eq!(tp.cluster, "backend");
            }
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
}
