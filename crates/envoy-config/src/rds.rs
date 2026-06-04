//! 20 D2 (ADR-0051/0052): the RDS file parser. Mirrors lds.rs/cds.rs — the
//! `@type`-tagged envelope with RouteConfiguration resources. Always-YAML
//! (serde_yaml, regardless of extension — the ADR-0049 decision-1 posture).
//! The named-resource selection (route_config_name) happens at merge time
//! (lib.rs), not here. M19-1 (the xds_file.rs consolidation) deferred per PLAN C17.
//!
//! UNLIKE parse_cds_file (which runs validate_cluster per resource), this
//! parser does NOT validate route configurations: RC validation needs the
//! cluster list (route→cluster references) and MUST run against the MERGED
//! cluster list (the §5.7 ordering invariant) — it happens at the post-merge
//! re-validation inside load_dynamic_resources (Task 3), not here.

use serde::Deserialize;

use crate::bootstrap::RouteConfiguration;

/// The RDS file envelope. `deny_unknown_fields` is deliberately NOT applied to
/// the envelope itself: Envoy's DiscoveryResponse carries fields envoy-rust
/// ignores (`version_info`, `type_url`, `nonce`, ...) — accept-and-ignore keeps
/// real-world RDS files loadable. The per-resource payload (RouteConfiguration)
/// keeps its `deny_unknown_fields` strictness (the L4 reconciliation: envoy-rust is
/// STRICTER than Envoy on unknown resource fields).
#[derive(Debug, Deserialize)]
struct RdsFile {
    #[serde(default)]
    resources: Vec<RdsResource>,
}

/// One `@type`-tagged resource. The tagged-enum-on-`@type` pattern is ADR-0014's;
/// RDS files carry RouteConfiguration resources only, so a non-RouteConfiguration
/// `@type` (or an absent one) fails to match the single variant and rejects loudly
/// during deserialization — exactly the L1 requirement.
#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum RdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.route.v3.RouteConfiguration")]
    RouteConfiguration(RouteConfiguration),
}

/// Parse an RDS file's contents into the dynamic route configuration list.
///
/// UNLIKE `parse_cds_file`, this does NOT validate the route configurations:
/// RC validation needs the cluster list (route→cluster references) and MUST run
/// against the MERGED cluster list (the §5.7 ordering invariant). That happens
/// in the post-merge re-validation inside `load_dynamic_resources` (Task 3),
/// not here.
///
/// `path` is used only for error context (it is NOT opened here); on any parse
/// failure the error carries the real path so operators can find the offending
/// file.
pub fn parse_rds_file(
    path: &str,
    contents: &str,
) -> Result<Vec<RouteConfiguration>, crate::ConfigError> {
    let file: RdsFile =
        serde_yaml::from_str(contents).map_err(|e| crate::ConfigError::RdsParseError {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    Ok(file
        .resources
        .into_iter()
        .map(|RdsResource::RouteConfiguration(rc)| rc)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal working RDS file: the bare `resources:` list, one @type-tagged
    // RouteConfiguration carrying name/virtual_hosts with one virtual host.
    const MINIMAL_RDS: &str = r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: local_route
  virtual_hosts:
  - name: backend
    domains: ["*"]
    routes:
    - match: { prefix: "/" }
      route: { cluster: some_cluster }
"#;

    #[test]
    fn rds_parse_bare_resources_envelope() {
        // (a) bare `resources:` envelope: one @type-tagged RouteConfiguration.
        let rcs = parse_rds_file("/x.yaml", MINIMAL_RDS).unwrap();
        assert_eq!(rcs.len(), 1);
        assert_eq!(rcs[0].name, "local_route");
        assert_eq!(rcs[0].virtual_hosts.len(), 1);
    }

    #[test]
    fn rds_parse_discovery_response_envelope_with_version_info() {
        // (b) full DiscoveryResponse shape (version_info + resources) is also
        // accepted; version_info is accept-and-ignore.
        let yaml = format!("version_info: \"v1\"\n{}", MINIMAL_RDS.trim_start());
        let rcs = parse_rds_file("/x.yaml", &yaml).unwrap();
        assert_eq!(rcs.len(), 1);
        assert_eq!(rcs[0].name, "local_route");
        assert_eq!(rcs[0].virtual_hosts.len(), 1);
    }

    #[test]
    fn rds_parse_multiple_route_configurations() {
        // (c) multiple RouteConfigurations in one file.
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: route_one
  virtual_hosts:
  - name: backend
    domains: ["*"]
    routes:
    - match: { prefix: "/" }
      route: { cluster: cluster_a }
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: route_two
  virtual_hosts:
  - name: backend
    domains: ["api.example.com"]
    routes:
    - match: { prefix: "/api" }
      route: { cluster: cluster_b }
"#;
        let rcs = parse_rds_file("/x.yaml", yaml).unwrap();
        assert_eq!(rcs.len(), 2);
        assert_eq!(rcs[0].name, "route_one");
        assert_eq!(rcs[1].name, "route_two");
    }

    #[test]
    fn rds_parse_rejects_non_route_configuration_at_type() {
        // (d) a resource tagged with a Cluster @type inside an RDS file is rejected.
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: not_a_route_config
"#;
        let err = parse_rds_file("/x.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::RdsParseError { .. }),
            "expected RdsParseError, got: {err:?}"
        );
    }

    #[test]
    fn rds_parse_rejects_malformed_yaml() {
        // (e) malformed YAML → RdsParseError carrying the path.
        let err = parse_rds_file("/etc/rds.yaml", "resources: [unclosed").unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::RdsParseError { path, .. }
                if path == "/etc/rds.yaml"),
            "expected RdsParseError carrying the path, got: {err:?}"
        );
    }

    #[test]
    fn rds_parse_rejects_resource_without_at_type() {
        // (f) missing `@type` → RdsParseError mentioning @type.
        let yaml = r#"
resources:
- name: local_route
  virtual_hosts:
  - name: backend
    domains: ["*"]
    routes:
    - match: { prefix: "/" }
      route: { cluster: some_cluster }
"#;
        let err = parse_rds_file("/x.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::RdsParseError { message, .. }
                if message.contains("@type")),
            "expected RdsParseError mentioning @type, got: {err:?}"
        );
    }
}
