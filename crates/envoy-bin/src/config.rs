use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    pub static_resources: StaticResources,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticResources {
    pub listeners: Vec<Listener>,
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

/// The only network filter name envoy-rust recognizes in phase 00.
pub const ECHO_FILTER: &str = "envoy.filters.network.echo";

pub fn parse_bootstrap(yaml: &str) -> Result<Bootstrap> {
    let bootstrap: Bootstrap = serde_yaml::from_str(yaml).context("parsing bootstrap YAML")?;
    validate(&bootstrap)?;
    Ok(bootstrap)
}

fn validate(bootstrap: &Bootstrap) -> Result<()> {
    let listeners = &bootstrap.static_resources.listeners;
    if listeners.is_empty() {
        bail!("bootstrap has no listeners; phase 00 requires exactly one");
    }
    if listeners.len() > 1 {
        bail!(
            "bootstrap has {} listeners; phase 00 supports exactly one",
            listeners.len()
        );
    }
    for listener in listeners {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                if filter.name != ECHO_FILTER {
                    bail!(
                        "unsupported network filter '{}'; phase 00 accepts only '{}'",
                        filter.name,
                        ECHO_FILTER,
                    );
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
    fn parses_minimal_bootstrap() {
        let b = parse_bootstrap(MINIMAL).expect("valid YAML");
        let port = b.static_resources.listeners[0]
            .address
            .socket_address
            .port_value;
        assert_eq!(port, 10000);
        assert_eq!(
            b.static_resources.listeners[0]
                .address
                .socket_address
                .address,
            "0.0.0.0"
        );
    }

    #[test]
    fn rejects_non_echo_filter() {
        let yaml = MINIMAL.replace(
            "envoy.filters.network.echo",
            "envoy.filters.network.tcp_proxy",
        );
        let err = parse_bootstrap(&yaml).expect_err("must reject tcp_proxy");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unsupported network filter"),
            "unexpected error message: {msg}",
        );
    }

    #[test]
    fn rejects_empty_listeners() {
        let yaml = "static_resources:\n  listeners: []\n";
        let err = parse_bootstrap(yaml).expect_err("must reject empty listeners");
        assert!(format!("{err:#}").contains("no listeners"));
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
        let err = parse_bootstrap(yaml).expect_err("must reject 2 listeners");
        assert!(format!("{err:#}").contains("phase 00 supports exactly one"));
    }

    #[test]
    fn rejects_malformed_yaml() {
        let err = parse_bootstrap("::: not yaml :::").expect_err("parser must fail");
        assert!(format!("{err:#}").contains("parsing bootstrap YAML"));
    }

    // Regression for REVIEW.md M3: `#[serde(deny_unknown_fields)]` on
    // `Bootstrap` must surface a typo'd top-level key as a parse error rather
    // than silently dropping it.
    #[test]
    fn rejects_unknown_bootstrap_field() {
        let yaml = format!("{MINIMAL}\nbogus_field: true\n");
        let err = parse_bootstrap(&yaml).expect_err("must reject unknown top-level field");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unknown field"),
            "unexpected error message: {msg}",
        );
    }

    // Regression for REVIEW.md M3 on a nested struct (`Listener`). Exercises
    // that serde walks the tree and `deny_unknown_fields` fires at depth, not
    // only at the bootstrap root.
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
        let err = parse_bootstrap(yaml).expect_err("must reject unknown listener field");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unknown field"),
            "unexpected error message: {msg}",
        );
    }
}
