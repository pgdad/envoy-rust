//! 19 D2 (ADR-0050 / §6.2 L1): the LDS file parser — the filesystem xDS
//! transport's Listener-resource envelope. Mirrors cds.rs (phase 18): the
//! bare `resources:` list AND the full DiscoveryResponse shape are accepted;
//! `version_info` is accept-and-ignore; each resource MUST carry
//! `@type: type.googleapis.com/envoy.config.listener.v3.Listener` (the
//! ADR-0014 internally-tagged pattern); parsing is always-YAML regardless of
//! file extension (the ADR-0049 decision-1 posture).
//!
//! UNLIKE parse_cds_file (which runs validate_cluster per resource), this
//! parser does NOT validate listeners: listener validation needs the cluster
//! list (route→cluster references) and MUST run against the MERGED cluster
//! list (the §5.7 ordering invariant) — it happens at the post-merge
//! re-validation inside load_dynamic_resources, not here.

use serde::Deserialize;

use crate::bootstrap::Listener;

/// The LDS file envelope. `deny_unknown_fields` is deliberately NOT applied to
/// the envelope itself: Envoy's DiscoveryResponse carries fields envoy-rust
/// ignores (`version_info`, `type_url`, `nonce`, ...) — accept-and-ignore keeps
/// real-world LDS files loadable. The per-resource payload (Listener) keeps its
/// `deny_unknown_fields` strictness (the L4 reconciliation: envoy-rust is
/// STRICTER than Envoy on unknown resource fields).
#[derive(Debug, Deserialize)]
struct LdsFile {
    #[serde(default)]
    resources: Vec<LdsResource>,
}

/// One `@type`-tagged resource. The tagged-enum-on-`@type` pattern is ADR-0014's;
/// LDS files carry Listener resources only, so a non-Listener `@type` (or an
/// absent one) fails to match the single variant and rejects loudly during
/// deserialization — exactly the L1 requirement.
#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum LdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.listener.v3.Listener")]
    Listener(Listener),
}

/// Parse an LDS file's contents into the dynamic listener list.
///
/// UNLIKE `parse_cds_file`, this does NOT validate the listeners: listener
/// validation needs the cluster list (route→cluster references) and MUST run
/// against the MERGED cluster list (the §5.7 ordering invariant). That happens
/// in the post-merge re-validation inside `load_dynamic_resources` (Task 3),
/// not here.
///
/// `path` is used only for error context (it is NOT opened here); on any parse
/// failure the error carries the real path so operators can find the offending
/// file.
pub fn parse_lds_file(path: &str, contents: &str) -> Result<Vec<Listener>, crate::ConfigError> {
    let file: LdsFile =
        serde_yaml::from_str(contents).map_err(|e| crate::ConfigError::LdsParseError {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    Ok(file
        .resources
        .into_iter()
        .map(|LdsResource::Listener(l)| l)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal working LDS file: the bare `resources:` list, one @type-tagged
    // Listener carrying name/address/filter_chains with an HCM network filter.
    const MINIMAL_LDS: &str = r#"
resources:
- "@type": type.googleapis.com/envoy.config.listener.v3.Listener
  name: dynamic_listener
  address:
    socket_address: { address: 0.0.0.0, port_value: 10000 }
  filter_chains:
  - filters:
    - name: envoy.filters.network.http_connection_manager
      typed_config:
        "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
        stat_prefix: ingress_http
        codec_type: AUTO
        route_config:
          name: local_route
          virtual_hosts:
          - name: backend
            domains: ["*"]
            routes:
            - match: { prefix: "/" }
              route: { cluster: some_cluster }
        http_filters:
        - name: envoy.filters.http.router
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;

    #[test]
    fn parses_bare_resources_envelope() {
        // L1: the bare `resources:` list (the minimal working shape Envoy accepts).
        let listeners = parse_lds_file("test.yaml", MINIMAL_LDS).unwrap();
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0].name, "dynamic_listener");
    }

    #[test]
    fn parses_discovery_response_envelope_with_version_info() {
        // L1: the full DiscoveryResponse shape (version_info + resources) is also
        // accepted; version_info is accept-and-ignore.
        let yaml = format!("version_info: \"1\"\n{}", MINIMAL_LDS.trim_start());
        let listeners = parse_lds_file("test.yaml", &yaml).unwrap();
        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0].name, "dynamic_listener");
    }

    #[test]
    fn rejects_resource_without_at_type() {
        // L1: @type per resource is REQUIRED (mirrors Envoy's
        // "missing @type in Any" rejection).
        let yaml = r#"
resources:
- name: dynamic_listener
  address:
    socket_address: { address: 0.0.0.0, port_value: 10000 }
  filter_chains: []
"#;
        let err = parse_lds_file("test.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::LdsParseError { message, .. }
                if message.contains("@type")),
            "expected LdsParseError mentioning @type, got: {err:?}"
        );
    }

    #[test]
    fn rejects_resource_with_wrong_at_type() {
        // A Cluster @type inside an LDS file is rejected (LDS carries Listeners only).
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: not_a_listener
"#;
        let err = parse_lds_file("test.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::LdsParseError { .. }),
            "expected LdsParseError, got: {err:?}"
        );
    }

    #[test]
    fn rejects_malformed_yaml() {
        let err = parse_lds_file("/etc/lds.yaml", "resources: [unclosed").unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::LdsParseError { path, .. }
                if path == "/etc/lds.yaml"),
            "expected LdsParseError carrying the path, got: {err:?}"
        );
    }

    #[test]
    fn rejects_unknown_fields_in_resource() {
        // L4 reconciliation (ADR-0049): envoy-rust is STRICTER than Envoy here —
        // the Listener schema is deny_unknown_fields, so an unknown field inside
        // the resource rejects through the tagged enum. The listener is otherwise
        // well-formed so the ONLY rejection trigger is the unknown field.
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.listener.v3.Listener
  name: dynamic_listener
  bogus_field: 1
  address:
    socket_address: { address: 0.0.0.0, port_value: 10000 }
  filter_chains: []
"#;
        let err = parse_lds_file("test.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::LdsParseError { message, .. }
                if message.contains("unknown field")),
            "expected LdsParseError with 'unknown field', got: {err:?}"
        );
    }

    #[test]
    fn parses_empty_resources_list() {
        // An empty `resources:` list parses to an empty Vec (no error — the
        // merge handles emptiness).
        let listeners = parse_lds_file("test.yaml", "resources: []").unwrap();
        assert!(listeners.is_empty());
    }
}
