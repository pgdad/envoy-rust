//! 21 D2 (ADR-0053/0054): the EDS file parser. Mirrors cds.rs/lds.rs/rds.rs — the
//! `@type`-tagged envelope with ClusterLoadAssignment resources (envoy-rust parses
//! these into the existing `LoadAssignment` struct, reused verbatim). Always-YAML
//! (serde_yaml, regardless of extension — the ADR-0049 decision-1 posture; the
//! Envoy-side container path is structurally .yaml). The named-resource selection
//! (service_name-or-cluster-name) happens at merge time (lib.rs), not here.
//! M19-1/M20-T6-a (the xds_file.rs consolidation, now N=4) DEFERRED per PLAN C18.

use serde::Deserialize;

use crate::bootstrap::LoadAssignment;

/// The EDS file envelope. `deny_unknown_fields` is deliberately NOT applied to
/// the envelope itself: Envoy's DiscoveryResponse carries fields envoy-rust
/// ignores (`version_info`, `type_url`, `nonce`, ...) — accept-and-ignore keeps
/// real-world EDS files loadable. The per-resource payload (LoadAssignment) keeps
/// its `deny_unknown_fields` strictness (the L4 reconciliation: envoy-rust is
/// STRICTER than Envoy on unknown resource fields).
#[derive(Debug, Deserialize)]
struct EdsFile {
    #[serde(default)]
    resources: Vec<EdsResource>,
}

/// One `@type`-tagged resource. The tagged-enum-on-`@type` pattern is ADR-0014's;
/// EDS files carry ClusterLoadAssignment resources only, so a non-ClusterLoadAssignment
/// `@type` (or an absent one) fails to match the single variant and rejects loudly
/// during deserialization — exactly the L1 requirement. envoy-rust's name for
/// `ClusterLoadAssignment` is the existing `LoadAssignment` struct, reused verbatim.
#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum EdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment")]
    ClusterLoadAssignment(LoadAssignment),
}

/// Parse an EDS file's contents into the list of ClusterLoadAssignments
/// (envoy-rust's `LoadAssignment`).
///
/// Like `parse_rds_file` (and UNLIKE `parse_cds_file`), this does NOT validate
/// the assignments here: the name-selection (service_name-or-cluster-name) and
/// the per-cluster endpoint validation run at merge time inside
/// `load_dynamic_resources` (Task 3), against the merged state.
///
/// `path` is used only for error context (it is NOT opened here); on any parse
/// failure the error carries the real path so operators can find the offending
/// file.
pub fn parse_eds_file(
    path: &str,
    contents: &str,
) -> Result<Vec<LoadAssignment>, crate::ConfigError> {
    let file: EdsFile =
        serde_yaml::from_str(contents).map_err(|e| crate::ConfigError::EdsParseError {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    Ok(file
        .resources
        .into_iter()
        .map(|EdsResource::ClusterLoadAssignment(la)| la)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal working EDS file: the bare `resources:` list, one @type-tagged
    // ClusterLoadAssignment carrying cluster_name + one endpoint (numeric IP).
    const MINIMAL_EDS: &str = r#"
resources:
- "@type": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment
  cluster_name: eds_backend
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;

    #[test]
    fn eds_parse_bare_resources_envelope() {
        // (a) bare `resources:` envelope: one @type-tagged ClusterLoadAssignment.
        let las = parse_eds_file("/x.yaml", MINIMAL_EDS).unwrap();
        assert_eq!(las.len(), 1);
        assert_eq!(las[0].cluster_name, "eds_backend");
        assert_eq!(las[0].endpoints.len(), 1);
    }

    #[test]
    fn eds_parse_discovery_response_envelope_with_version_info() {
        // (b) full DiscoveryResponse shape (version_info + resources) is also
        // accepted; version_info is accept-and-ignore.
        let yaml = format!("version_info: \"v1\"\n{}", MINIMAL_EDS.trim_start());
        let las = parse_eds_file("/x.yaml", &yaml).unwrap();
        assert_eq!(las.len(), 1);
        assert_eq!(las[0].cluster_name, "eds_backend");
        assert_eq!(las[0].endpoints.len(), 1);
    }

    #[test]
    fn eds_parse_multiple_cluster_load_assignments() {
        // (c) multiple ClusterLoadAssignments in one file (name-selection is
        // Task 3, not here — the parser returns them all, in order).
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment
  cluster_name: eds_backend_a
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address: { address: 127.0.0.1, port_value: 8124 }
- "@type": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment
  cluster_name: eds_backend_b
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address: { address: 127.0.0.2, port_value: 8125 }
"#;
        let las = parse_eds_file("/x.yaml", yaml).unwrap();
        assert_eq!(las.len(), 2);
        assert_eq!(las[0].cluster_name, "eds_backend_a");
        assert_eq!(las[1].cluster_name, "eds_backend_b");
    }

    #[test]
    fn eds_parse_rejects_non_cluster_load_assignment_at_type() {
        // (d) a resource tagged with a Cluster @type inside an EDS file is rejected.
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: not_a_cluster_load_assignment
"#;
        let err = parse_eds_file("/x.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::EdsParseError { .. }),
            "expected EdsParseError, got: {err:?}"
        );
    }

    #[test]
    fn eds_parse_rejects_malformed_yaml() {
        // (e) malformed YAML → EdsParseError carrying the path.
        let err = parse_eds_file("/etc/eds.yaml", "resources: [unclosed").unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::EdsParseError { path, .. }
                if path == "/etc/eds.yaml"),
            "expected EdsParseError carrying the path, got: {err:?}"
        );
    }

    #[test]
    fn eds_parse_rejects_resource_without_at_type() {
        // (f) missing `@type` → EdsParseError mentioning @type.
        let yaml = r#"
resources:
- cluster_name: eds_backend
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;
        let err = parse_eds_file("/x.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::EdsParseError { message, .. }
                if message.contains("@type")),
            "expected EdsParseError mentioning @type, got: {err:?}"
        );
    }
}
