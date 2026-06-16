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

/// 26 Task 4 (ADR-0066): the file-based RDS hot-reload pipeline's pure prefix —
/// steps 1-4 of the §6.2-LOCKED reload sequence, producing a validated candidate
/// `RouteConfiguration` WITHOUT touching any live state. The watcher
/// (`envoy-http1::rds_watcher`) calls this OUTSIDE its write lock; only the
/// final atomic `store_route_config` swap (step 5) is its concern.
///
/// Steps, with the failure class each error maps to (the watcher classifies on
/// the `ConfigError` variant):
///  1. **read** `path` — IO error → [`ConfigError::RdsFileError`] (update_failure).
///  2. **parse** via [`parse_rds_file`] — malformed → [`ConfigError::RdsParseError`]
///     (update_failure).
///  3. **select** the `RouteConfiguration` whose `.name == route_config_name` —
///     absent → [`ConfigError::RdsRouteConfigNotFound`] (update_rejected).
///  4. **revalidate** every `RouteAction::Route` reference against the live
///     cluster set via `known_cluster` — a reference to a cluster NOT present →
///     [`ConfigError::UnknownCluster`] (update_rejected). This is the recorded
///     warm-reject divergence vs Envoy (ADR-0066): envoy-rust's request path
///     `.expect()`s cluster existence, so installing an unknown-cluster route
///     would panic — we reject the reload and keep the last-good table instead.
///
/// `known_cluster` is a predicate over cluster names rather than a
/// `&ClusterManager` deliberately: `envoy-cluster` depends on `envoy-config`, so
/// taking the manager here would form a dependency cycle. The watcher (which
/// owns the `Arc<ClusterManager>`) supplies `|name| cluster_mgr.get(name).is_some()`.
pub fn reparse_and_select_route_config(
    path: &std::path::Path,
    route_config_name: &str,
    known_cluster: &dyn Fn(&str) -> bool,
) -> Result<RouteConfiguration, crate::ConfigError> {
    let path_str = path.display().to_string();
    // Step 1: re-read.
    let contents =
        std::fs::read_to_string(path).map_err(|source| crate::ConfigError::RdsFileError {
            path: path_str.clone(),
            source,
        })?;
    // Step 2: re-parse (RdsParseError on malformed YAML / bad envelope).
    let mut parsed = parse_rds_file(&path_str, &contents)?;
    // Step 3: name-select (RdsRouteConfigNotFound if absent).
    let selected = parsed
        .iter()
        .position(|rc| rc.name == route_config_name)
        .map(|i| parsed.remove(i))
        .ok_or_else(|| crate::ConfigError::RdsRouteConfigNotFound {
            name: route_config_name.to_string(),
            path: path_str.clone(),
        })?;
    // Step 4: revalidate route→cluster references against the live cluster set.
    //
    // This re-validation is DELIBERATELY PARTIAL. It checks ONLY the
    // cluster-EXISTENCE reference: a route to a cluster absent from the live
    // cluster set → `UnknownCluster` → warm-rejected. That check is
    // non-negotiable because the request path `.expect()`s cluster existence at
    // `crates/envoy-http1/src/hcm.rs:818` — installing an unknown-cluster route
    // would PANIC the proxy.
    //
    // It does NOT re-validate the phase-20 `Http2ClusterFromHttp1Listener`
    // (H1/AUTO-listener × H2-only-cluster) reachability gate. That gate is
    // deferred on reload, consistent with the project-wide OPEN ADR-0028
    // deferral: H1×H2 dispatch is unimplemented project-wide, so an H1→H2-only
    // route misnegotiates silently at request time rather than panicking —
    // unlike the unknown-cluster case it does not threaten proxy stability.
    // Threading the listener codec into the watch target for a full
    // re-validation is deferred with ADR-0028.
    //
    // DirectResponse routes reference no cluster.
    for vh in &selected.virtual_hosts {
        for route in &vh.routes {
            if let crate::RouteAction::Route(ar) = &route.action
                && !known_cluster(&ar.cluster)
            {
                return Err(crate::ConfigError::UnknownCluster(ar.cluster.clone()));
            }
        }
    }
    Ok(selected)
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

    // ---- 26 Task 4: reparse_and_select_route_config (the reload pipeline's
    // steps 1-4: read + parse + select + revalidate). ----

    // An RDS file body naming one RouteConfiguration `rc_name` whose single
    // route forwards to `cluster`. Used to exercise the happy path + the
    // route_config_name-not-found / unknown-cluster reject classes.
    fn rds_body(rc_name: &str, cluster: &str) -> String {
        format!(
            r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: {rc_name}
  virtual_hosts:
  - name: backend
    domains: ["*"]
    routes:
    - match: {{ prefix: "/" }}
      route: {{ cluster: {cluster} }}
"#
        )
    }

    fn write_temp(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("rds.yaml");
        std::fs::write(&path, contents).expect("write rds file");
        (dir, path)
    }

    #[test]
    fn reparse_happy_path_reads_selects_and_validates() {
        let (_dir, path) = write_temp(&rds_body("local_route", "known_cluster"));
        // known-cluster predicate: only "known_cluster" exists.
        let rc = reparse_and_select_route_config(&path, "local_route", &|name| {
            name == "known_cluster"
        })
        .expect("happy reload");
        assert_eq!(rc.name, "local_route");
        assert_eq!(rc.virtual_hosts.len(), 1);
    }

    #[test]
    fn reparse_io_error_when_file_missing() {
        // step 1: an unreadable/missing file → RdsFileError (the update_failure class).
        let missing = std::path::Path::new("/no/such/rds/file.yaml");
        let err = reparse_and_select_route_config(missing, "local_route", &|_| true).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::RdsFileError { .. }),
            "expected RdsFileError, got: {err:?}"
        );
    }

    #[test]
    fn reparse_parse_error_on_malformed_yaml() {
        // step 2: malformed YAML → RdsParseError (the update_failure class).
        let (_dir, path) = write_temp("resources: [unclosed");
        let err = reparse_and_select_route_config(&path, "local_route", &|_| true).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::RdsParseError { .. }),
            "expected RdsParseError, got: {err:?}"
        );
    }

    #[test]
    fn reparse_route_config_name_not_found_rejects() {
        // step 3: the requested name is absent from the envelope →
        // RdsRouteConfigNotFound (the update_rejected class).
        let (_dir, path) = write_temp(&rds_body("other_route", "known_cluster"));
        let err =
            reparse_and_select_route_config(&path, "local_route", &|_| true).unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::RdsRouteConfigNotFound { name, .. }
                if name == "local_route"),
            "expected RdsRouteConfigNotFound for local_route, got: {err:?}"
        );
    }

    #[test]
    fn reparse_unknown_cluster_route_rejects() {
        // step 4: the selected table references a cluster NOT in the live set →
        // UnknownCluster (the update_rejected class — the recorded warm-reject
        // divergence vs Envoy, ADR-0066).
        let (_dir, path) = write_temp(&rds_body("local_route", "ghost_cluster"));
        let err = reparse_and_select_route_config(&path, "local_route", &|name| {
            name == "known_cluster"
        })
        .unwrap_err();
        assert!(
            matches!(&err, crate::ConfigError::UnknownCluster(c) if c == "ghost_cluster"),
            "expected UnknownCluster(ghost_cluster), got: {err:?}"
        );
    }

    #[test]
    fn reparse_direct_response_route_needs_no_cluster() {
        // A direct_response route references no cluster, so revalidation passes
        // regardless of the known-cluster predicate.
        let yaml = r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: local_route
  virtual_hosts:
  - name: backend
    domains: ["*"]
    routes:
    - match: { prefix: "/" }
      direct_response: { status: 200, body: { inline_string: "ok" } }
"#;
        let (_dir, path) = write_temp(yaml);
        let rc = reparse_and_select_route_config(&path, "local_route", &|_| false)
            .expect("direct_response needs no cluster");
        assert_eq!(rc.name, "local_route");
    }
}
