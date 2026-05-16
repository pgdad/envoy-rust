//! `AdminEndpoint` enum + per-endpoint response builders. Exact-match path
//! routing only in 06.1 per cross-sub-phase architectural rule 5.

use bytes::{Bytes, BytesMut};
use envoy_stats::StatsRegistry;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminEndpoint {
    /// `GET /ready` — returns 200 "LIVE\n" once the server has bound its
    /// listeners. Phase-08's drain semantics introduce 503 PRE_INITIALIZING
    /// and 503 DRAINING states; in 06.1 the endpoint always returns 200.
    Ready,

    /// `GET /stats` — returns 200 with body in plain-text "name: value\n"
    /// per-line format (matches Envoy's default `/stats` format under
    /// `format=` absence).
    Stats,

    /// `GET /stats/prometheus` — returns 200 with body in Prometheus
    /// text-exposition format per envoy_stats::prometheus::write_exposition.
    StatsPrometheus,

    /// `GET /config_dump` — returns 200 with body
    /// `{ "configs": [BootstrapConfigDump] }` rendered as pretty JSON. Phase
    /// 08.1 D6. xDS-derived ConfigDump entries (Clusters/Listeners/Routes/
    /// Secrets) are deferred to the xDS family and explicitly land on
    /// `allowlist_envoy_only` per BEHAVIOR_CONTRACT.
    ConfigDump,

    /// `GET /server_info` — returns 200 with body shaped per upstream
    /// Envoy's `envoy.admin.v3.ServerInfo`: top-level JSON object with
    /// `version`, `state`, `hot_restart_version`, `command_line_options`,
    /// `node`, `uptime_current_epoch_seconds`, `uptime_all_epochs_seconds`.
    /// Phase 08.1 D5 emits `state` as the constant `"LIVE"` per SPEC §5.4;
    /// 08.2's D5e patches the value-binding source from this constant to a
    /// `DrainState`-derived match.
    ServerInfo,

    /// `GET /clusters` — returns 200 `text/plain` with two lines per cluster
    /// in name-deterministic order: `<name>::observability_name::<name>` and
    /// `<name>::default_priority::endpoints`. Phase 08.1 D7. Per architecture
    /// decision lock-in #10 (PROGRESS Task 1 preamble), the 08.1 emission is
    /// limited to those two lines per cluster; upstream Envoy's per-endpoint
    /// numeric counters (success/error/timeout) are deferred and absorbed by
    /// `allowlist_envoy_only_lines` at fixture 0014. See BEHAVIOR_CONTRACT §
    /// "Admin endpoint body shapes".
    Clusters,

    /// `GET /listeners` — returns 200 `text/plain` with one line per listener
    /// in name-deterministic order: `<listener_name>::<address>:<port>`.
    /// Phase 08.1 D8. Reads from `handler.bootstrap().static_resources.listeners`
    /// — the 08.1 listener set is statically declared (xDS-derived listeners
    /// land in §9 family). See BEHAVIOR_CONTRACT § "Admin endpoint body shapes".
    Listeners,
}

/// Method-aware dispatch result. Introduced at phase 08.1 D4 to give every
/// endpoint a structurally-declared 405-method-allowlist surface (closes 06.1
/// REVIEW M1 structurally). 08.2 POST endpoints plug in additively via new
/// `AdminEndpoint` variants whose `allowed_method` returns `"POST"`; no
/// further refactor of `Dispatch` is needed.
#[derive(Debug, PartialEq, Eq)]
pub enum Dispatch {
    Endpoint(AdminEndpoint),
    NotFound,
    MethodNotAllowed { allow: &'static str },
}

impl AdminEndpoint {
    /// Exact-match URL path lookup. Returns `None` for unknown paths
    /// (caller produces 404). Case-sensitive per Envoy v1.33.
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            "/ready" => Some(AdminEndpoint::Ready),
            "/stats" => Some(AdminEndpoint::Stats),
            "/stats/prometheus" => Some(AdminEndpoint::StatsPrometheus),
            "/config_dump" => Some(AdminEndpoint::ConfigDump),
            "/server_info" => Some(AdminEndpoint::ServerInfo),
            "/clusters" => Some(AdminEndpoint::Clusters),
            "/listeners" => Some(AdminEndpoint::Listeners),
            _ => None,
        }
    }

    /// The HTTP method this endpoint accepts. 08.1's 4 new GET endpoints
    /// (ConfigDump, ServerInfo, Clusters, Listeners) declare `"GET"` here;
    /// 08.2's POST endpoints will declare `"POST"`.
    pub fn allowed_method(&self) -> &'static str {
        match self {
            AdminEndpoint::Ready
            | AdminEndpoint::Stats
            | AdminEndpoint::StatsPrometheus
            | AdminEndpoint::ConfigDump
            | AdminEndpoint::ServerInfo
            | AdminEndpoint::Clusters
            | AdminEndpoint::Listeners => "GET",
        }
    }

    /// Method-aware dispatch. Returns:
    /// - `Endpoint(e)` on a method+path match,
    /// - `NotFound` on an unknown path (regardless of method),
    /// - `MethodNotAllowed { allow }` on a known path with the wrong method.
    pub fn dispatch(method: &str, path: &str) -> Dispatch {
        match AdminEndpoint::from_path(path) {
            None => Dispatch::NotFound,
            Some(endpoint) => {
                let allow = endpoint.allowed_method();
                if method == allow {
                    Dispatch::Endpoint(endpoint)
                } else {
                    Dispatch::MethodNotAllowed { allow }
                }
            }
        }
    }

    /// Render the response for this endpoint. Reads the registry only on
    /// the `Stats` / `StatsPrometheus` arms; `Ready` ignores the registry.
    ///
    /// Phase 08.1 D6: this is the registry-only render path retained for the
    /// 06.1 endpoints. New endpoints introduced in 08.1 (ConfigDump and the
    /// Tasks 7-9 cohort) need handler-scoped state and dispatch through
    /// [`AdminEndpoint::render_with`] instead. Calling `render` on `ConfigDump`
    /// is a programming error — the dispatch path in `handler.rs` routes
    /// `ConfigDump` through `render_with` exclusively.
    pub fn render(&self, registry: &StatsRegistry) -> envoy_http1::Response {
        match self {
            AdminEndpoint::Ready => Self::render_ready(),
            AdminEndpoint::Stats => Self::render_stats(registry),
            AdminEndpoint::StatsPrometheus => Self::render_stats_prometheus(registry),
            AdminEndpoint::ConfigDump => unreachable!(
                "ConfigDump requires handler-scoped state; dispatch via AdminEndpoint::render_with"
            ),
            AdminEndpoint::ServerInfo => unreachable!(
                "ServerInfo requires handler-scoped state; dispatch via AdminEndpoint::render_with"
            ),
            AdminEndpoint::Clusters => unreachable!(
                "Clusters requires handler-scoped state; dispatch via AdminEndpoint::render_with"
            ),
            AdminEndpoint::Listeners => unreachable!(
                "Listeners requires handler-scoped state; dispatch via AdminEndpoint::render_with"
            ),
        }
    }

    /// Phase 08.1 D6 introduces `render_with(&AdminHandler)` to reach
    /// handler-scoped state (`Arc<Bootstrap>`, `ClusterManager`,
    /// `start_instant`, `command_line_options`). The existing
    /// [`AdminEndpoint::render`] carries forward for `/ready`, `/stats`, and
    /// `/stats/prometheus`; new endpoints add explicit arms here. Tasks 7/8/9
    /// add `ServerInfo`, `Clusters`, `Listeners`.
    pub fn render_with(&self, handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
        match self {
            AdminEndpoint::ConfigDump => render_config_dump(handler),
            AdminEndpoint::ServerInfo => render_server_info(handler),
            AdminEndpoint::Clusters => render_clusters(handler),
            AdminEndpoint::Listeners => render_listeners(handler),
            // 06.1 endpoints carry forward through the registry-only path.
            _ => self.render(handler.registry()),
        }
    }

    fn render_ready() -> envoy_http1::Response {
        let body = Bytes::from_static(b"LIVE\n");
        envoy_http1::Response {
            status: 200,
            reason: Some("OK"),
            headers: vec![
                ("content-type".to_string(), "text/plain".to_string()),
                ("content-length".to_string(), body.len().to_string()),
            ],
            body,
        }
    }

    fn render_stats(registry: &StatsRegistry) -> envoy_http1::Response {
        let mut buf = BytesMut::new();
        for (name, handle) in registry.snapshot() {
            use envoy_stats::StatHandle;
            use std::fmt::Write as _;
            match handle {
                StatHandle::Counter(c) => {
                    let _ = writeln!(&mut buf, "{name}: {}", c.value());
                }
                StatHandle::Gauge(g) => {
                    let _ = writeln!(&mut buf, "{name}: {}", g.value());
                }
            }
        }
        let body = buf.freeze();
        envoy_http1::Response {
            status: 200,
            reason: Some("OK"),
            headers: vec![
                ("content-type".to_string(), "text/plain".to_string()),
                ("content-length".to_string(), body.len().to_string()),
            ],
            body,
        }
    }

    fn render_stats_prometheus(registry: &StatsRegistry) -> envoy_http1::Response {
        let mut buf = BytesMut::new();
        envoy_stats::prometheus::write_exposition(registry, &mut buf);
        let body = buf.freeze();
        envoy_http1::Response {
            status: 200,
            reason: Some("OK"),
            headers: vec![
                // Mirror upstream Envoy 1.33's `/stats/prometheus`
                // content-type verbatim: `text/plain; charset=UTF-8`.
                // (The Prometheus-spec value `text/plain; version=0.0.4;
                // charset=utf-8` is what Prometheus exposition strictly
                // documents, but upstream Envoy emits the un-versioned
                // form; envoy-rust mirrors per D-3.3 doctrine — empirical
                // verification landed in 06.1 fixture 0011.)
                (
                    "content-type".to_string(),
                    "text/plain; charset=UTF-8".to_string(),
                ),
                ("content-length".to_string(), body.len().to_string()),
            ],
            body,
        }
    }
}

/// Phase 08.1 D6: top-level body envelope for `/config_dump`. Mirrors upstream
/// Envoy's `envoy.admin.v3.ConfigDump` shape: a `configs` array of
/// per-resource-type entries. The body type is lifetime-parameterized so the
/// renderer can borrow `&Bootstrap` from the `Arc<Bootstrap>` cached on the
/// handler (avoiding a `Bootstrap`-wide `Clone` cascade — PLAN lock-in #1).
#[derive(Serialize)]
pub(crate) struct ConfigDumpBody<'a> {
    pub configs: Vec<ConfigDumpEntry<'a>>,
}

/// Phase 08.1 D6: one entry in the `/config_dump` `configs` array. Serializes
/// the `@type` tag externally per upstream Envoy's `google.protobuf.Any` JSON
/// projection convention. envoy-rust emits exactly one `Bootstrap` entry in
/// 08.1; xDS-derived entries (`ClustersConfigDump`, `ListenersConfigDump`,
/// `RoutesConfigDump`, `SecretsConfigDump`) are deferred to the xDS family
/// and land on `allowlist_envoy_only` per BEHAVIOR_CONTRACT §"Admin endpoint
/// body shapes".
#[derive(Serialize)]
#[serde(tag = "@type")]
pub(crate) enum ConfigDumpEntry<'a> {
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")]
    Bootstrap {
        bootstrap: &'a envoy_config::Bootstrap,
        last_updated: String,
    },
}

/// Phase 08.1 D6: render `/config_dump` as pretty JSON. Borrows the cached
/// `Bootstrap` from the handler; the `last_updated` timestamp is the wall
/// clock at render time formatted via [`envoy_accesslog::format_iso8601`].
pub(crate) fn render_config_dump(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let body = ConfigDumpBody {
        configs: vec![ConfigDumpEntry::Bootstrap {
            bootstrap: handler.bootstrap(),
            last_updated: envoy_accesslog::format_iso8601(std::time::SystemTime::now()),
        }],
    };
    let body_bytes = serde_json::to_vec_pretty(&body)
        .expect("ConfigDumpBody serializes (all subtypes derive Serialize per Task 4)");
    envoy_http1::Response {
        // Task 1's `reason_for_status(200)` supplies "OK" at serialize time;
        // leave `reason: None` per PLAN lock-in #7.
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Bytes::from(body_bytes),
    }
}

/// Phase 08.1 D5: top-level body envelope for `/server_info`. Mirrors upstream
/// Envoy's `envoy.admin.v3.ServerInfo` JSON projection. The body type is
/// lifetime-parameterized so the renderer can borrow `&Bootstrap.node` and the
/// `command_line_options` `BTreeMap` from the handler — same borrowed-reference
/// shape as `ConfigDumpBody<'a>` (PLAN lock-in #1, no `Clone` cascade).
///
/// `state` is a `&'static str` literal at 08.1 — SPEC §5.4 binds it to the
/// constant `"LIVE"`. 08.2's D5e patches the value-binding source from this
/// constant to a `DrainState`-derived match; the struct shape is locked.
///
/// `hot_restart_version` is `&'static str = "disabled"` — envoy-rust does
/// NOT implement hot restart. `uptime_current_epoch_seconds` equals
/// `uptime_all_epochs_seconds` for the same reason (current epoch is the only
/// epoch).
#[derive(Serialize)]
pub(crate) struct ServerInfoBody<'a> {
    pub version: &'a str,
    pub state: &'static str,
    pub hot_restart_version: &'static str,
    pub command_line_options: &'a std::collections::BTreeMap<String, serde_yaml::Value>,
    // `Bootstrap.node` is `Option<Node>` (parse-time optional per envoy-config's
    // bootstrap schema). Borrow as `Option<&Node>` so a missing `node` block
    // serializes to JSON `null` rather than failing — the SPEC contract for
    // `/server_info.node` is "value-exact from the parsed bootstrap".
    pub node: Option<&'a envoy_config::Node>,
    pub uptime_current_epoch_seconds: u64,
    pub uptime_all_epochs_seconds: u64,
}

/// Phase 08.1 D5: render `/server_info` as pretty JSON. Borrows the `Node`
/// subtree from the handler's cached `Bootstrap` and the
/// `command_line_options` map (constructed once at handler-init time per PLAN
/// lock-in #7). Uptime is computed from `handler.start_instant()`.
pub(crate) fn render_server_info(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let uptime = handler.start_instant().elapsed().as_secs();
    let body = ServerInfoBody {
        version: concat!("envoy-rust ", env!("CARGO_PKG_VERSION")),
        // SPEC §5.4: 08.1 hardcodes the constant "LIVE"; 08.2 D5e patches the
        // value-binding source to a DrainState-derived match.
        state: "LIVE",
        // envoy-rust does not implement hot restart.
        hot_restart_version: "disabled",
        command_line_options: handler.command_line_options(),
        node: handler.bootstrap().node.as_ref(),
        uptime_current_epoch_seconds: uptime,
        uptime_all_epochs_seconds: uptime,
    };
    let body_bytes = serde_json::to_vec_pretty(&body)
        .expect("ServerInfoBody serializes (all subtypes derive Serialize)");
    envoy_http1::Response {
        // Task 1's `reason_for_status(200)` supplies "OK" at serialize time;
        // leave `reason: None` per PLAN lock-in #7.
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: Bytes::from(body_bytes),
    }
}

/// Phase 08.1 D7: render `/clusters` as plain text per Envoy v1.33's
/// `/clusters` default format. Emits two lines per cluster:
///
///   `<name>::observability_name::<name>`
///   `<name>::default_priority::endpoints`
///
/// Per architecture-decision lock-in #10 (PROGRESS Task 1 preamble), 08.1
/// emits ONLY these two lines per cluster — the per-endpoint numeric-counter
/// lines (success/error/timeout) that upstream Envoy adds are deferred and
/// absorbed by the fixture's `allowlist_envoy_only_lines` at fixture 0014.
///
/// Cluster output order is deterministic by name (sorted in
/// [`envoy_cluster::ClusterManager::clusters`]).
pub(crate) fn render_clusters(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    use std::fmt::Write as _;
    let mut body = String::new();
    for cluster in handler.cluster_manager().clusters() {
        let name = cluster.name();
        let _ = writeln!(&mut body, "{name}::observability_name::{name}");
        let _ = writeln!(&mut body, "{name}::default_priority::endpoints");
    }
    envoy_http1::Response {
        // Task 1's `reason_for_status(200)` supplies "OK" at serialize time;
        // leave `reason: None` per PLAN lock-in #7 (consistent with
        // `render_config_dump` + `render_server_info`).
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "text/plain".to_string())],
        body: Bytes::from(body),
    }
}

/// Phase 08.1 D8: render `/listeners` as plain text. Emits one line per
/// listener in name-deterministic order:
///
///   `<listener_name>::<address>:<port>`
///
/// The 08.1 listener set is statically declared in the parsed `Bootstrap`
/// (xDS-derived listeners absent until §9 family). Sort key is the
/// `Listener.name` field; this is enforced at the renderer rather than at the
/// `Bootstrap` parse layer because `static_resources.listeners` is a `Vec`
/// that preserves declaration order. Deterministic ordering is required by
/// BEHAVIOR_CONTRACT's `/listeners` row + architecture-decision lock-in #11.
pub(crate) fn render_listeners(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    use std::fmt::Write as _;
    // Address is a struct (single `socket_address` field), not an enum —
    // direct field access, no `match`. SocketAddress carries `address: String`
    // + `port_value: u16`.
    let mut lines: Vec<(String, String)> = handler
        .bootstrap()
        .static_resources
        .listeners
        .iter()
        .map(|l| {
            (
                l.name.clone(),
                format!(
                    "{}:{}",
                    l.address.socket_address.address, l.address.socket_address.port_value
                ),
            )
        })
        .collect();
    lines.sort_by(|a, b| a.0.cmp(&b.0));
    let mut body = String::new();
    for (name, addr) in &lines {
        let _ = writeln!(&mut body, "{name}::{addr}");
    }
    envoy_http1::Response {
        // Task 1's `reason_for_status(200)` supplies "OK" at serialize time;
        // leave `reason: None` per PLAN lock-in #7 (consistent with
        // `render_config_dump` + `render_server_info` + `render_clusters`).
        status: 200,
        reason: None,
        headers: vec![("content-type".to_string(), "text/plain".to_string())],
        body: Bytes::from(body),
    }
}

/// Render a 404 for unknown admin paths. Used by `AdminHandler::handle_inner`
/// when `from_path` returns `None`.
pub(crate) fn render_404() -> envoy_http1::Response {
    let body = Bytes::from_static(b"unknown admin endpoint\n");
    envoy_http1::Response {
        status: 404,
        reason: Some("Not Found"),
        headers: vec![
            ("content-type".to_string(), "text/plain".to_string()),
            ("content-length".to_string(), body.len().to_string()),
        ],
        body,
    }
}

/// Render a 405 for method-not-allowed responses. Used by
/// `AdminHandler::handle_inner` via `Dispatch::MethodNotAllowed { allow }`.
///
/// Phase 08.1 D4 widens the previously-fixed `Allow:` header value to a
/// per-endpoint dynamic value sourced from the `Dispatch::MethodNotAllowed`
/// arm. The body is regenerated dynamically too — closes 06.1 REVIEW M1
/// structurally: every endpoint variant declares its own allowed method.
pub(crate) fn render_405(allow: &'static str) -> envoy_http1::Response {
    let body = Bytes::from(format!("Method not allowed. Allow: {allow}\n"));
    envoy_http1::Response {
        status: 405,
        // Task 1's reason_for_status renders "Method Not Allowed" when reason
        // is None; leaving it None lets the helper supply the canonical text.
        reason: None,
        headers: vec![
            ("content-type".to_string(), "text/plain".to_string()),
            ("content-length".to_string(), body.len().to_string()),
            ("allow".to_string(), allow.to_string()),
        ],
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_path_ready_matches_exact() {
        assert_eq!(
            AdminEndpoint::from_path("/ready"),
            Some(AdminEndpoint::Ready)
        );
        assert_eq!(AdminEndpoint::from_path("/ready/"), None);
        assert_eq!(AdminEndpoint::from_path("/Ready"), None);
        assert_eq!(AdminEndpoint::from_path("/ready/foo"), None);
    }

    #[test]
    fn from_path_stats_matches_exact() {
        assert_eq!(
            AdminEndpoint::from_path("/stats"),
            Some(AdminEndpoint::Stats)
        );
    }

    #[test]
    fn from_path_stats_prometheus_matches_exact() {
        assert_eq!(
            AdminEndpoint::from_path("/stats/prometheus"),
            Some(AdminEndpoint::StatsPrometheus)
        );
    }

    #[test]
    fn from_path_unknown_returns_none() {
        // Task 9 promoted `/listeners` from "unknown" → `AdminEndpoint::Listeners`,
        // closing the 08.1 endpoint surface (all 7 GET-only variants now known).
        // Re-target the unknown-path probe to `/nope` (genuinely unknown across
        // 08.1 and 08.2). The empty-path and `/` cases stay unknown.
        assert_eq!(AdminEndpoint::from_path("/nope"), None);
        assert_eq!(AdminEndpoint::from_path(""), None);
        assert_eq!(AdminEndpoint::from_path("/"), None);
    }

    #[test]
    fn render_ready_returns_200_live() {
        let reg = StatsRegistry::new();
        let resp = AdminEndpoint::Ready.render(&reg);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, Some("OK"));
        assert_eq!(&resp.body[..], b"LIVE\n");
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "content-type" && v == "text/plain")
        );
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "content-length" && v == "5")
        );
    }

    #[test]
    fn render_stats_text_format() {
        let reg = StatsRegistry::new();
        let c = reg
            .register_counter("listener.foo.downstream_cx_total")
            .unwrap();
        c.add(7);
        let resp = AdminEndpoint::Stats.render(&reg);
        assert_eq!(resp.status, 200);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        assert!(body_str.contains("listener.foo.downstream_cx_total: 7\n"));
    }

    #[test]
    fn render_stats_prometheus_format() {
        let reg = StatsRegistry::new();
        let c = reg
            .register_counter("listener.foo.downstream_cx_total")
            .unwrap();
        c.add(7);
        let resp = AdminEndpoint::StatsPrometheus.render(&reg);
        assert_eq!(resp.status, 200);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        assert!(body_str.contains("# TYPE envoy_listener_foo_downstream_cx_total counter\n"));
        assert!(body_str.contains("envoy_listener_foo_downstream_cx_total 7\n"));
    }

    #[test]
    fn render_response_carries_correct_content_type() {
        let reg = StatsRegistry::new();
        let stats = AdminEndpoint::Stats.render(&reg);
        assert!(
            stats
                .headers
                .iter()
                .any(|(k, v)| k == "content-type" && v == "text/plain")
        );

        let prom = AdminEndpoint::StatsPrometheus.render(&reg);
        assert!(
            prom.headers
                .iter()
                .any(|(k, v)| k == "content-type" && v == "text/plain; charset=UTF-8")
        );
    }

    #[test]
    fn render_404_body_and_status() {
        let r = render_404();
        assert_eq!(r.status, 404);
        assert_eq!(r.reason, Some("Not Found"));
        assert_eq!(&r.body[..], b"unknown admin endpoint\n");
    }

    #[test]
    fn render_405_carries_allow_get_header() {
        let r = render_405("GET");
        assert_eq!(r.status, 405);
        assert!(r.headers.iter().any(|(k, v)| k == "allow" && v == "GET"));
    }
}

#[cfg(test)]
mod config_dump_tests {
    //! Phase 08.1 Task 6 — D6: `/config_dump` endpoint coverage. Six tests:
    //! two dispatch-shape tests (GET routes to `ConfigDump`; POST returns 405)
    //! and four body-shape tests (200 + `application/json`; valid JSON with a
    //! top-level `configs` array; one `BootstrapConfigDump` entry; the
    //! `bootstrap` subtree carries the parsed `node.id`).

    use super::{AdminEndpoint, Dispatch};
    use crate::config::AdminConfig;
    use crate::handler::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::{Address, Admin, Bootstrap, SocketAddress};
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    pub(super) fn handler_with_bootstrap(yaml: &str) -> AdminHandler {
        let bootstrap: Bootstrap = serde_yaml::from_str(yaml).expect("yaml parses");
        let admin = Admin {
            address: Address {
                socket_address: SocketAddress {
                    address: "127.0.0.1".to_string(),
                    port_value: 0,
                },
            },
            access_log_path: None,
        };
        let cfg = Arc::new(AdminConfig::from_envoy_config(&admin).expect("AdminConfig"));
        AdminHandler::new(
            cfg,
            Arc::new(StatsRegistry::new()),
            Arc::new(bootstrap),
            Arc::new(ClusterManager::empty()),
            Instant::now(),
            BTreeMap::new(),
        )
    }

    /// Phase 08.1 Task 9: hoisted to `pub(super)` so sibling test modules
    /// (`server_info_tests`, `clusters_tests`, `listeners_tests`) share one
    /// source for the minimal valid bootstrap YAML. Pre-Task-9 each sibling
    /// inlined the same literal; closes Task 7 review M2 carryforward.
    pub(super) const TINY_BOOTSTRAP: &str =
        "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";

    #[test]
    fn config_dump_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/config_dump"),
            Dispatch::Endpoint(AdminEndpoint::ConfigDump)
        ));
    }

    #[test]
    fn config_dump_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/config_dump"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn config_dump_renders_200_with_application_json() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        assert_eq!(resp.status, 200);
        let ct = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert_eq!(ct, Some("application/json"));
    }

    #[test]
    fn config_dump_body_is_valid_json_with_configs_array() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).expect("utf-8");
        let value: serde_json::Value = serde_json::from_str(body_str).expect("valid JSON");
        assert!(value.get("configs").and_then(|c| c.as_array()).is_some());
    }

    #[test]
    fn config_dump_body_has_bootstrap_config_dump_entry() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        let value: serde_json::Value = serde_json::from_str(body_str).unwrap();
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(configs.len(), 1);
        let entry = &configs[0];
        assert_eq!(
            entry.get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")
        );
        assert!(entry.get("bootstrap").is_some());
        assert!(entry.get("last_updated").and_then(|v| v.as_str()).is_some());
    }

    #[test]
    fn config_dump_bootstrap_subtree_carries_node_id() {
        let yaml = "node:\n  id: my-node-id\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ConfigDump.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        let value: serde_json::Value = serde_json::from_str(body_str).unwrap();
        let node_id = value
            .pointer("/configs/0/bootstrap/node/id")
            .and_then(|v| v.as_str());
        assert_eq!(node_id, Some("my-node-id"));
    }
}

#[cfg(test)]
mod server_info_tests {
    //! Phase 08.1 Task 7 — D5: `/server_info` endpoint coverage. Seven tests:
    //! two dispatch-shape tests (GET routes to `ServerInfo`; POST returns 405)
    //! and five body-shape tests (200 + `application/json`; required keys;
    //! `state == "LIVE"` constant per SPEC §5.4; `node` subtree carries the
    //! parsed `node.id`; uptime is non-negative).

    use super::config_dump_tests::{TINY_BOOTSTRAP, handler_with_bootstrap}; // reuse Task 6 helper + hoisted YAML literal
    use super::{AdminEndpoint, Dispatch};

    #[test]
    fn server_info_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/server_info"),
            Dispatch::Endpoint(AdminEndpoint::ServerInfo)
        ));
    }

    #[test]
    fn server_info_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/server_info"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn server_info_renders_200_with_application_json() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        assert_eq!(resp.status, 200);
        let ct = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert_eq!(ct, Some("application/json"));
    }

    #[test]
    fn server_info_body_has_required_keys() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let body_str = std::str::from_utf8(&resp.body).unwrap();
        let value: serde_json::Value = serde_json::from_str(body_str).unwrap();
        let obj = value.as_object().expect("top-level object");
        for key in &[
            "version",
            "state",
            "hot_restart_version",
            "command_line_options",
            "node",
            "uptime_current_epoch_seconds",
            "uptime_all_epochs_seconds",
        ] {
            assert!(obj.contains_key(*key), "missing key: {key}");
        }
    }

    #[test]
    fn server_info_state_is_live_at_phase_08_1() {
        // SPEC §5.4: 08.1 emits the constant "LIVE". 08.2's D5e patches the
        // value-binding source from this constant to a DrainState-derived match.
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let value: serde_json::Value =
            serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        assert_eq!(value.get("state").and_then(|v| v.as_str()), Some("LIVE"));
    }

    #[test]
    fn server_info_node_subtree_carries_id() {
        let yaml = "node:\n  id: my-id\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let handler = handler_with_bootstrap(yaml);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let value: serde_json::Value =
            serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        assert_eq!(
            value.pointer("/node/id").and_then(|v| v.as_str()),
            Some("my-id")
        );
    }

    #[test]
    fn server_info_uptime_is_non_negative() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::ServerInfo.render_with(&handler);
        let value: serde_json::Value =
            serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        let uptime = value
            .get("uptime_current_epoch_seconds")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert!(
            uptime < 60,
            "fresh handler uptime should be small; got {uptime}"
        );
    }
}

#[cfg(test)]
mod clusters_tests {
    //! Phase 08.1 Task 8 — D7: `/clusters` endpoint coverage. Four tests:
    //! two dispatch-shape tests (GET routes to `Clusters`; POST returns 405)
    //! and two body-shape tests (200 + `text/plain`; empty cluster set
    //! renders an empty body).

    use super::config_dump_tests::{TINY_BOOTSTRAP, handler_with_bootstrap};
    use super::{AdminEndpoint, Dispatch};

    #[test]
    fn clusters_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/clusters"),
            Dispatch::Endpoint(AdminEndpoint::Clusters)
        ));
    }

    #[test]
    fn clusters_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/clusters"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn clusters_renders_200_with_text_plain() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::Clusters.render_with(&handler);
        assert_eq!(resp.status, 200);
        let ct = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert!(ct.unwrap_or("").starts_with("text/plain"));
    }

    #[test]
    fn clusters_body_is_empty_for_zero_clusters() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::Clusters.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        assert_eq!(body, "", "empty cluster set renders empty body");
    }
}

#[cfg(test)]
mod listeners_tests {
    //! Phase 08.1 Task 9 — D8: `/listeners` endpoint coverage. Six tests:
    //! two dispatch-shape tests (GET routes to `Listeners`; POST returns 405)
    //! and four body-shape tests (200 + `text/plain`; empty listener set
    //! renders empty body; non-empty bootstrap emits one
    //! `<name>::<addr>:<port>` line per listener; output is deterministic
    //! by-name regardless of declaration order).

    use super::config_dump_tests::{TINY_BOOTSTRAP, handler_with_bootstrap};
    use super::{AdminEndpoint, Dispatch};

    /// Two-listener bootstrap with `zebra` declared BEFORE `alpha`. Used to
    /// exercise both the populated-body emission and the sorted-by-name
    /// determinism asserted by BEHAVIOR_CONTRACT and architecture lock-in
    /// #11. Each listener carries a single trivial TCP-proxy filter chain so
    /// the bootstrap parses cleanly through `envoy-config`.
    const TWO_LISTENERS_BOOTSTRAP: &str = "\
node:
  id: t
  cluster: c
static_resources:
  listeners:
  - name: zebra
    address:
      socket_address:
        address: 127.0.0.1
        port_value: 9001
    filter_chains:
    - filters:
      - name: envoy.filters.network.tcp_proxy
        typed_config:
          \"@type\": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
          stat_prefix: z
          cluster: c
  - name: alpha
    address:
      socket_address:
        address: 0.0.0.0
        port_value: 8080
    filter_chains:
    - filters:
      - name: envoy.filters.network.tcp_proxy
        typed_config:
          \"@type\": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
          stat_prefix: a
          cluster: c
  clusters: []
";

    #[test]
    fn listeners_path_dispatches_on_get() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/listeners"),
            Dispatch::Endpoint(AdminEndpoint::Listeners)
        ));
    }

    #[test]
    fn listeners_405_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/listeners"),
            Dispatch::MethodNotAllowed { allow: "GET" }
        ));
    }

    #[test]
    fn listeners_renders_200_with_text_plain() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        assert_eq!(resp.status, 200);
        let ct = resp
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str());
        assert!(ct.unwrap_or("").starts_with("text/plain"));
    }

    #[test]
    fn listeners_body_is_empty_for_zero_listeners() {
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        assert_eq!(body, "", "empty listener set renders empty body");
    }

    #[test]
    fn listeners_body_emits_name_address_port_per_listener() {
        let handler = handler_with_bootstrap(TWO_LISTENERS_BOOTSTRAP);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        // Both listeners present with their `<name>::<addr>:<port>` shape.
        assert!(
            body.contains("alpha::0.0.0.0:8080\n"),
            "missing alpha line; body was: {body:?}"
        );
        assert!(
            body.contains("zebra::127.0.0.1:9001\n"),
            "missing zebra line; body was: {body:?}"
        );
    }

    #[test]
    fn listeners_body_is_sorted_by_name() {
        // TWO_LISTENERS_BOOTSTRAP declares zebra BEFORE alpha. Renderer must
        // sort by name (deterministic per BEHAVIOR_CONTRACT + architecture
        // lock-in #11) so `alpha` appears first in the body.
        let handler = handler_with_bootstrap(TWO_LISTENERS_BOOTSTRAP);
        let resp = AdminEndpoint::Listeners.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        let alpha_pos = body.find("alpha::").expect("alpha line present");
        let zebra_pos = body.find("zebra::").expect("zebra line present");
        assert!(
            alpha_pos < zebra_pos,
            "alpha should sort before zebra; body was: {body:?}"
        );
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::{AdminEndpoint, Dispatch};

    #[test]
    fn get_known_path_returns_endpoint() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/ready"),
            Dispatch::Endpoint(AdminEndpoint::Ready)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/stats"),
            Dispatch::Endpoint(AdminEndpoint::Stats)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/stats/prometheus"),
            Dispatch::Endpoint(AdminEndpoint::StatsPrometheus)
        ));
        // Task 8 opportunistic close of Task 7 review M1: extend coverage to all
        // 6 dispatchable endpoints. Tasks 6/7/8 added `ConfigDump`/`ServerInfo`/
        // `Clusters`; this expansion guards against any future variant being
        // added to `from_path` without a corresponding dispatch-test row.
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/config_dump"),
            Dispatch::Endpoint(AdminEndpoint::ConfigDump)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/server_info"),
            Dispatch::Endpoint(AdminEndpoint::ServerInfo)
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/clusters"),
            Dispatch::Endpoint(AdminEndpoint::Clusters)
        ));
        // Task 9 adds the 7th and final 08.1 GET variant.
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/listeners"),
            Dispatch::Endpoint(AdminEndpoint::Listeners)
        ));
    }

    #[test]
    fn unknown_path_returns_not_found_regardless_of_method() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/nope"),
            Dispatch::NotFound
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/nope"),
            Dispatch::NotFound
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("DELETE", "/"),
            Dispatch::NotFound
        ));
    }

    #[test]
    fn known_path_wrong_method_returns_method_not_allowed_with_get_in_allow() {
        match AdminEndpoint::dispatch("POST", "/ready") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
        match AdminEndpoint::dispatch("PUT", "/stats") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
        match AdminEndpoint::dispatch("DELETE", "/stats/prometheus") {
            Dispatch::MethodNotAllowed { allow } => assert_eq!(allow, "GET"),
            other => panic!("expected MethodNotAllowed; got {other:?}"),
        }
    }

    #[test]
    fn method_match_is_case_sensitive_exact() {
        // Envoy's admin API treats HTTP method names case-sensitively (uppercase
        // canonical per RFC 7230). Mixed-case methods are NOT recognized.
        assert!(matches!(
            AdminEndpoint::dispatch("get", "/ready"),
            Dispatch::MethodNotAllowed { .. }
        ));
        assert!(matches!(
            AdminEndpoint::dispatch("Get", "/ready"),
            Dispatch::MethodNotAllowed { .. }
        ));
    }

    #[test]
    fn each_endpoint_declares_its_allowed_method() {
        // Compile-time tautology: if any variant fails to declare ALLOWED, this
        // fails to compile.
        assert_eq!(AdminEndpoint::Ready.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::Stats.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::StatsPrometheus.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::ConfigDump.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::ServerInfo.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::Clusters.allowed_method(), "GET");
        assert_eq!(AdminEndpoint::Listeners.allowed_method(), "GET");
    }

    #[test]
    fn dispatch_is_disjoint_from_from_path() {
        // from_path is retained as a thin convenience but does NOT route through
        // dispatch. Direct unit test that both surfaces remain available.
        assert!(AdminEndpoint::from_path("/ready").is_some());
        assert!(AdminEndpoint::from_path("/nope").is_none());
    }
}
