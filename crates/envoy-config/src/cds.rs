//! 18 D2: file-based CDS parsing (the xDS-family filesystem transport opener;
//! ADR-0048/ADR-0049). Parses the path_config_source file's DiscoveryResponse-
//! shaped envelope into Vec<Cluster> using the existing Cluster serde schema
//! (the ADR-0014 YAML-native shim extended by one envelope — NO protos/prost).
//!
//! Envelope shape (L1, empirically verified vs Envoy v1.33):
//!   - bare `resources:` list OR full DiscoveryResponse (`version_info` +
//!     `resources`) — both accepted; version_info is accept-and-ignore.
//!   - each resource MUST carry `@type: type.googleapis.com/envoy.config.cluster.v3.Cluster`
//!     (mirrors Envoy's "missing @type in Any" rejection).
//!   - parsed with serde_yaml regardless of file extension (envoy-rust is more
//!     lenient than Envoy's extension-driven parser selection — recorded
//!     divergence, ADR-0049/BEHAVIOR_CONTRACT).
//!
//! This module does NO file I/O: it parses contents handed to it. The reader
//! (`load_dynamic_resources`, Task 3) owns the filesystem read and additionally
//! runs cross-cluster collision checking + post-merge route-reference
//! re-validation. Keeping I/O out of here preserves the `parse_bootstrap` fuzz
//! target's purity.

use serde::Deserialize;

use crate::bootstrap::Cluster;

/// The CDS file envelope. `deny_unknown_fields` is deliberately NOT applied to
/// the envelope itself: Envoy's DiscoveryResponse carries fields envoy-rust
/// ignores (`version_info`, `type_url`, `nonce`, ...) — accept-and-ignore keeps
/// real-world CDS files loadable. The per-resource payload (Cluster) keeps its
/// `deny_unknown_fields` strictness (the L4 reconciliation: envoy-rust is
/// STRICTER than Envoy on unknown resource fields).
#[derive(Debug, Deserialize)]
struct CdsFile {
    #[serde(default)]
    resources: Vec<CdsResource>,
}

/// One `@type`-tagged resource. The tagged-enum-on-`@type` pattern is ADR-0014's
/// (`TypedConfig` uses the same shape); CDS files carry Cluster resources only,
/// so a non-Cluster `@type` (or an absent one) fails to match the single variant
/// and rejects loudly during deserialization — exactly the L1 requirement.
#[derive(Debug, Deserialize)]
#[serde(tag = "@type")]
enum CdsResource {
    #[serde(rename = "type.googleapis.com/envoy.config.cluster.v3.Cluster")]
    Cluster(Cluster),
}

/// Parse a CDS file's contents into the dynamic cluster list. Every cluster
/// passes the same per-cluster validation static clusters do (SPEC D2,
/// via `bootstrap::validate_cluster`) — the caller (`load_dynamic_resources`,
/// Task 3) additionally runs collision checking and post-merge route-reference
/// re-validation.
///
/// `path` is used only for error context (it is NOT opened here); on any parse
/// or validation failure the error carries the real path so operators can find
/// the offending file.
pub fn parse_cds_file(path: &str, contents: &str) -> Result<Vec<Cluster>, crate::ConfigError> {
    let file: CdsFile =
        serde_yaml::from_str(contents).map_err(|e| crate::ConfigError::CdsParseError {
            path: path.to_string(),
            message: e.to_string(),
        })?;
    let clusters: Vec<Cluster> = file
        .resources
        .into_iter()
        .map(|CdsResource::Cluster(c)| c)
        .collect();
    for cluster in &clusters {
        crate::bootstrap::validate_cluster(cluster)?;
    }
    Ok(clusters)
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE on schema adaptation (Task-2 Step 1): the existing `Cluster` schema
    // requires `lb_policy` (no `#[serde(default)]`), so every fixture below
    // carries `lb_policy: ROUND_ROBIN` — the spec's draft YAML omitted it. The
    // test INTENTS are unchanged; only the field spellings track the real
    // schema.
    const MINIMAL_CDS: &str = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  lb_policy: ROUND_ROBIN
  dns_lookup_family: V4_ONLY
  load_assignment:
    cluster_name: dynamic_backend
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;

    #[test]
    fn parses_bare_resources_envelope() {
        // L1: the bare `resources:` list (the minimal working shape Envoy accepts).
        let clusters = parse_cds_file("test.yaml", MINIMAL_CDS).unwrap();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].name, "dynamic_backend");
    }

    #[test]
    fn parses_discovery_response_envelope_with_version_info() {
        // L1: the full DiscoveryResponse shape (version_info + resources) is also
        // accepted; version_info is accept-and-ignore.
        let yaml = format!("version_info: \"1\"\n{}", MINIMAL_CDS.trim_start());
        let clusters = parse_cds_file("test.yaml", &yaml).unwrap();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].name, "dynamic_backend");
    }

    #[test]
    fn rejects_resource_without_at_type() {
        // L1: @type per resource is REQUIRED (mirrors Envoy's
        // "missing @type in Any" rejection).
        let yaml = r#"
resources:
- name: dynamic_backend
  type: STRICT_DNS
  lb_policy: ROUND_ROBIN
  load_assignment: { cluster_name: dynamic_backend, endpoints: [] }
"#;
        assert!(parse_cds_file("test.yaml", yaml).is_err());
    }

    #[test]
    fn rejects_resource_with_wrong_at_type() {
        // A Listener @type inside a CDS file is rejected (CDS carries Clusters only).
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.listener.v3.Listener
  name: not_a_cluster
"#;
        assert!(parse_cds_file("test.yaml", yaml).is_err());
    }

    #[test]
    fn rejects_malformed_yaml() {
        assert!(parse_cds_file("test.yaml", "resources: [unclosed").is_err());
    }

    #[test]
    fn rejects_unknown_fields_in_resource() {
        // L4 reconciliation (ADR-0049): envoy-rust is STRICTER than Envoy here —
        // Envoy warn-accepts unknown resource fields; envoy-rust's Cluster schema
        // is deny_unknown_fields → reject-fatal. Recorded divergence.
        //
        // The cluster is otherwise FULLY VALID (real endpoints, matching
        // cluster_name, all mandatory fields present) so that the only rejection
        // trigger is the unknown field — not EmptyClusterEndpoints or any other
        // per-cluster validation error.
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  lb_policy: ROUND_ROBIN
  dns_lookup_family: V4_ONLY
  this_field_does_not_exist: true
  load_assignment:
    cluster_name: dynamic_backend
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;
        let err = parse_cds_file("test.yaml", yaml).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::CdsParseError { message, .. }
                if message.contains("unknown field")),
            "expected CdsParseError with 'unknown field', got: {err:?}"
        );
    }

    #[test]
    fn dynamic_clusters_pass_the_same_per_cluster_validation_as_static() {
        // SPEC D2: dynamic clusters go through the SAME validation gauntlet.
        // A cluster whose load_assignment.cluster_name mismatches its name is
        // rejected (the existing LoadAssignmentNameMismatch invariant).
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: dynamic_backend
  type: STRICT_DNS
  lb_policy: ROUND_ROBIN
  load_assignment:
    cluster_name: WRONG_NAME
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;
        assert!(parse_cds_file("test.yaml", yaml).is_err());
    }

    #[test]
    fn json_content_is_accepted() {
        // L1: serde_yaml parses JSON (JSON is a YAML subset); envoy-rust accepts
        // JSON-syntax CDS files regardless of extension (the recorded narrow
        // leniency divergence vs Envoy's extension-driven parser selection).
        let json = r#"{"resources": [{"@type": "type.googleapis.com/envoy.config.cluster.v3.Cluster", "name": "dynamic_backend", "type": "STRICT_DNS", "lb_policy": "ROUND_ROBIN", "dns_lookup_family": "V4_ONLY", "load_assignment": {"cluster_name": "dynamic_backend", "endpoints": [{"lb_endpoints": [{"endpoint": {"address": {"socket_address": {"address": "127.0.0.1", "port_value": 8124}}}}]}]}}]}"#;
        let clusters = parse_cds_file("test.json", json).unwrap();
        assert_eq!(clusters.len(), 1);
    }
}
