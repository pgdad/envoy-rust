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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkFilter {
    pub name: String,
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
}
