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

    /// Phase 08.2 D9: `POST /drain_listeners` — invokes `DrainState::drain()`
    /// and returns 200 OK with an empty body. Effect-only endpoint: the
    /// listener accept loops observe the `drain_signal()` notify and start
    /// draining within tens of microseconds. Sticky per parent-08 SPEC §5.6
    /// — repeat POSTs are idempotent (the CAS at `DrainState::drain` fails
    /// silently against an already-`Draining` state).
    DrainListeners,

    /// Phase 08.2 D10a: `POST /healthcheck/fail` — invokes
    /// `DrainState::fail_healthcheck()` and returns 200 OK with an empty
    /// body. Flips `/ready` to 503 (per parent-08 SPEC §5.5 wire-state
    /// mapping). `/server_info.state` stays `"LIVE"` (server-state is
    /// independent of healthcheck-failure).
    HealthcheckFail,

    /// Phase 08.2 D10b: `POST /healthcheck/ok` — invokes
    /// `DrainState::ok_healthcheck()` and returns 200 OK with an empty body.
    /// Restores `HealthcheckFailing → Live`. Sticky-drain: a POST to
    /// `/healthcheck/ok` AFTER `/drain_listeners` does NOT un-drain (the
    /// `HealthcheckFailing → Live` `compare_exchange` fails silently against
    /// the `Draining` state).
    HealthcheckOk,
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
            // 08.2 D9 / D10 — three POST endpoints. Method-arm filtering
            // happens in `dispatch`; `from_path` resolves the path only.
            "/drain_listeners" => Some(AdminEndpoint::DrainListeners),
            "/healthcheck/fail" => Some(AdminEndpoint::HealthcheckFail),
            "/healthcheck/ok" => Some(AdminEndpoint::HealthcheckOk),
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
            // 08.2 D9 / D10 — effect-only POST endpoints.
            AdminEndpoint::DrainListeners
            | AdminEndpoint::HealthcheckFail
            | AdminEndpoint::HealthcheckOk => "POST",
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
            // Phase 08.2 Task 5 (D-ready): `Ready` now requires the
            // handler-scoped `DrainState` to compute the response shape
            // (200 LIVE / 503 Service Unavailable / 503 DRAINING). The
            // registry-only path can no longer satisfy this and must
            // dispatch through `render_with`.
            AdminEndpoint::Ready => unreachable!(
                "Ready requires handler-scoped DrainState; dispatch via AdminEndpoint::render_with"
            ),
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
            // 08.2 D9 / D10 — POST endpoints need DrainState, which the
            // registry-only render path does not carry. The dispatch path in
            // `handler.rs` routes these variants through `render_with` (Task 4
            // wires `handler.drain()`); reaching here is a programming error.
            AdminEndpoint::DrainListeners => unreachable!(
                "DrainListeners requires DrainState; dispatch via AdminEndpoint::render_with"
            ),
            AdminEndpoint::HealthcheckFail => unreachable!(
                "HealthcheckFail requires DrainState; dispatch via AdminEndpoint::render_with"
            ),
            AdminEndpoint::HealthcheckOk => unreachable!(
                "HealthcheckOk requires DrainState; dispatch via AdminEndpoint::render_with"
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
            // Phase 08.2 Task 5 (D-ready): `/ready` widens to a drain-aware
            // response. Routes through the handler-aware path so the
            // renderer can read `handler.drain().current()` to select the
            // status / reason / body (200 LIVE, 503 Service Unavailable, or
            // 503 DRAINING per parent-08 SPEC §5.5).
            AdminEndpoint::Ready => render_ready_with(handler),
            AdminEndpoint::ConfigDump => render_config_dump(handler),
            AdminEndpoint::ServerInfo => render_server_info(handler),
            AdminEndpoint::Clusters => render_clusters(handler),
            AdminEndpoint::Listeners => render_listeners(handler),
            // 08.2 D9 / D10 — the three POST endpoints route through the
            // `handler.drain()` accessor (08.2 D13b, Task 4). Each render fn
            // invokes the corresponding `DrainState` method (drain /
            // fail_healthcheck / ok_healthcheck) and returns 200 OK with an
            // empty body via the shared `empty_200_ok()` helper.
            AdminEndpoint::DrainListeners => render_drain_listeners(handler.drain()),
            AdminEndpoint::HealthcheckFail => render_healthcheck_fail(handler.drain()),
            AdminEndpoint::HealthcheckOk => render_healthcheck_ok(handler.drain()),
            // Registry-only endpoints (`/stats`, `/stats/prometheus`) carry
            // forward through the original `render` path.
            _ => self.render(handler.registry()),
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
    /// Phase 18 D5 (ADR-0049 L5): the `ClustersConfigDump` entry, emitted ONLY
    /// when `dynamic_resources.cds_config` is configured (fixture 0014 stays
    /// single-entry — its config_dump shape is untouched). Keys mirror Envoy's
    /// proto3-JSON shape: empty lists are OMITTED entirely (`skip_serializing_if`),
    /// and there is NO `version_info` key (the CDS file carried none → proto3
    /// JSON omits empty fields). When present this entry lands at `configs[1]`,
    /// AFTER the Bootstrap entry, matching Envoy v1.33's ordering.
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.ClustersConfigDump")]
    Clusters {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        static_clusters: Vec<StaticClusterEntry<'a>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dynamic_active_clusters: Vec<DynamicClusterEntry<'a>>,
    },
}

/// Phase 18 D5: one static cluster inside `ClustersConfigDump`. Envoy shape:
/// `{"cluster": {...}}` (no `last_updated` on static-config entries).
#[derive(Serialize)]
pub(crate) struct StaticClusterEntry<'a> {
    pub(crate) cluster: TaggedCluster<'a>,
}

/// Phase 18 D5: one dynamically-loaded cluster. Envoy shape:
/// `{"cluster": {...}, "last_updated": "..."}`.
#[derive(Serialize)]
pub(crate) struct DynamicClusterEntry<'a> {
    pub(crate) cluster: TaggedCluster<'a>,
    pub(crate) last_updated: String,
}

/// Phase 18 D5: a `Cluster` serialized with the inner `@type` tag Envoy's
/// `google.protobuf.Any`-projection carries on the nested `cluster` object.
/// `#[serde(flatten)]` merges the full cluster config alongside the `@type`
/// key. This is a flatten on a NESTED struct (not on the internally-tagged
/// outer enum's variant content), so it does not trip serde's
/// flatten+internally-tagged-enum limitation — verified compiling + the
/// emitted JSON carries both `@type` and the full cluster fields.
#[derive(Serialize)]
pub(crate) struct TaggedCluster<'a> {
    #[serde(rename = "@type")]
    pub(crate) type_url: &'static str,
    #[serde(flatten)]
    pub(crate) cluster: &'a envoy_config::Cluster,
}

/// The `@type` URL for the nested `cluster` object inside `ClustersConfigDump`.
const CLUSTER_TYPE_URL: &str = "type.googleapis.com/envoy.config.cluster.v3.Cluster";

/// Phase 08.2 Task 5 (D-ready): drain-aware `/ready` response. Widens the
/// 06.1 hardcoded 200 `"LIVE\n"` shape to a three-arm match on
/// `handler.drain().current()` per parent-08 SPEC §5.5 wire-state mapping:
///
/// - `Live` → 200 OK, body `"LIVE\n"`
/// - `HealthcheckFailing` → 503 Service Unavailable, body `"Service Unavailable\n"`
/// - `Draining` → 503 Service Unavailable, body `"DRAINING\n"`
///
/// All three shapes carry `content-type: text/plain` + a `content-length`
/// matching the body length (the established admin response convention; the
/// 06.1 `render_ready` shape did the same). The `reason` field is set
/// explicitly (`Some("OK")` / `Some("Service Unavailable")`) — the 06.1 shape
/// set it too; we preserve that for the post-Task-5 surface so the wire-line
/// reason phrase comes from the renderer rather than falling through to
/// `reason_for_status`.
///
/// Dispatched exclusively through `AdminEndpoint::render_with` (the
/// registry-only `render` path's `Ready` arm is `unreachable!()` post-Task-5
/// for the same reason as the other handler-scoped endpoints).
pub(crate) fn render_ready_with(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    use envoy_listener::DrainStage;
    let (status, reason, body): (u16, &'static str, Bytes) = match handler.drain().current() {
        DrainStage::Live => (200, "OK", Bytes::from_static(b"LIVE\n")),
        DrainStage::HealthcheckFailing => (
            503,
            "Service Unavailable",
            Bytes::from_static(b"Service Unavailable\n"),
        ),
        DrainStage::Draining => (
            503,
            "Service Unavailable",
            Bytes::from_static(b"DRAINING\n"),
        ),
    };
    envoy_http1::Response {
        status,
        reason: Some(reason),
        headers: vec![
            ("content-type".to_string(), "text/plain".to_string()),
            ("content-length".to_string(), body.len().to_string()),
        ],
        body,
    }
}

/// Phase 08.1 D6: render `/config_dump` as pretty JSON. Borrows the cached
/// `Bootstrap` from the handler; the `last_updated` timestamp is the wall
/// clock at render time formatted via [`envoy_accesslog::format_iso8601`].
pub(crate) fn render_config_dump(handler: &crate::handler::AdminHandler) -> envoy_http1::Response {
    let bootstrap = handler.bootstrap();
    // Single render-time wall clock, shared by the Bootstrap entry and (when
    // emitted) the dynamic-cluster `last_updated` fields — same source/format.
    let last_updated = envoy_accesslog::format_iso8601(std::time::SystemTime::now());
    let mut configs = vec![ConfigDumpEntry::Bootstrap {
        bootstrap,
        last_updated: last_updated.clone(),
    }];
    // Phase 18 D5 (ADR-0049 L5): emit the ClustersConfigDump entry ONLY when
    // `dynamic_resources.cds_config` is configured. Pushed AFTER the Bootstrap
    // entry ⇒ `configs[1]` (matching Envoy v1.33's ordering). Empty cluster
    // lists serialize to omitted keys via the variant's `skip_serializing_if`.
    if bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.cds_config.as_ref())
        .is_some()
    {
        let static_clusters = bootstrap
            .static_resources
            .clusters
            .iter()
            .map(|cluster| StaticClusterEntry {
                cluster: TaggedCluster {
                    type_url: CLUSTER_TYPE_URL,
                    cluster,
                },
            })
            .collect();
        let dynamic_active_clusters = bootstrap
            .dynamic_clusters
            .iter()
            .flatten()
            .map(|cluster| DynamicClusterEntry {
                cluster: TaggedCluster {
                    type_url: CLUSTER_TYPE_URL,
                    cluster,
                },
                last_updated: last_updated.clone(),
            })
            .collect();
        configs.push(ConfigDumpEntry::Clusters {
            static_clusters,
            dynamic_active_clusters,
        });
    }
    let body = ConfigDumpBody { configs };
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
    use envoy_listener::DrainStage;
    let uptime = handler.start_instant().elapsed().as_secs();
    // Phase 08.2 D5e: rebind the `state` value source from the 08.1 literal
    // "LIVE" to a DrainState-derived match (parent-08 SPEC §5.5 wire-state
    // mapping). Per upstream Envoy semantics + parent-08 SPEC §5.5,
    // `/server_info.state` is INDEPENDENT of healthcheck-failure: the
    // `HealthcheckFailing` stage maps to "LIVE" here (only `/ready` flips
    // to 503 under HealthcheckFailing). Only the `Draining` stage flips
    // `/server_info.state` to "DRAINING".
    let state = match handler.drain().current() {
        DrainStage::Live | DrainStage::HealthcheckFailing => "LIVE",
        DrainStage::Draining => "DRAINING",
    };
    let body = ServerInfoBody {
        version: concat!("envoy-rust ", env!("CARGO_PKG_VERSION")),
        state,
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

/// Phase 08.2 D9: `/drain_listeners` POST endpoint. Invokes `DrainState::drain()`
/// and returns 200 OK with an empty body. Side effect: triggers the
/// `drain_signal()` notify; the listener accept loops observe and start
/// draining within tens of microseconds. Sticky — repeat POSTs are idempotent
/// (per parent-08 SPEC §5.6 + 08.2 SPEC §3 D11 sticky-drain).
///
/// Reachable from Task 4 onward via `render_with`'s `DrainListeners` arm
/// (`handler.drain()`-routed) AND from the colocated `drain_admin_tests` unit
/// tests. Task 3's `#[allow(dead_code)]` was removed at Task 4 once the
/// dispatch arm started invoking this fn.
pub(crate) fn render_drain_listeners(drain: &envoy_listener::DrainState) -> envoy_http1::Response {
    drain.drain();
    empty_200_ok()
}

/// Phase 08.2 D10a: `/healthcheck/fail` POST endpoint. Invokes
/// `DrainState::fail_healthcheck()` and returns 200 OK empty body. Reachable
/// from Task 4 via `render_with`'s `HealthcheckFail` arm.
pub(crate) fn render_healthcheck_fail(drain: &envoy_listener::DrainState) -> envoy_http1::Response {
    drain.fail_healthcheck();
    empty_200_ok()
}

/// Phase 08.2 D10b: `/healthcheck/ok` POST endpoint. Invokes
/// `DrainState::ok_healthcheck()` and returns 200 OK empty body. Sticky-drain:
/// if state is already `Draining`, this is a no-op (the underlying
/// `compare_exchange` from `HealthcheckFailing → Live` fails silently; state
/// stays `Draining`). Reachable from Task 4 via `render_with`'s
/// `HealthcheckOk` arm.
pub(crate) fn render_healthcheck_ok(drain: &envoy_listener::DrainState) -> envoy_http1::Response {
    drain.ok_healthcheck();
    empty_200_ok()
}

/// Shared 200 OK empty-body response shape for the 3 D9/D10 POST endpoints.
/// `content-length: 0` per the established admin response convention; no
/// `content-type` (no body — content-type is moot per RFC 7231 §3.1.1.5).
/// Reachable from Task 4 onward via the 3 `render_*` callers above.
fn empty_200_ok() -> envoy_http1::Response {
    envoy_http1::Response {
        status: 200,
        reason: Some("OK"),
        headers: vec![("content-length".to_string(), "0".to_string())],
        body: Bytes::new(),
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
        // Phase 08.2 Task 5 (D-ready): `Ready` now dispatches through
        // `render_with` (the registry-only path's `Ready` arm became
        // `unreachable!()`). This 06.1-era test was updated to route
        // through the new handler-aware path; the Live-stage assertion is
        // preserved (default `DrainState::new` → `DrainStage::Live` →
        // 200 OK "LIVE\n"). Per-stage coverage (HealthcheckFailing,
        // Draining) lives in the colocated `ready_drain_tests` submodule.
        use super::config_dump_tests::{TINY_BOOTSTRAP, handler_with_bootstrap};
        let handler = handler_with_bootstrap(TINY_BOOTSTRAP);
        let resp = AdminEndpoint::Ready.render_with(&handler);
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
        let registry = Arc::new(StatsRegistry::new());
        // Phase 08.2 Task 4 (D13b): every `AdminHandler::new` call site adds
        // the trailing `Arc<DrainState>` arg. The shared helper here covers
        // the 08.1 endpoint-task test cohort (config_dump / server_info /
        // clusters / listeners); the DrainState constructed here is
        // never observed by those tests (they read bootstrap / cluster /
        // listener state, not drain state), so a fresh per-call DrainState
        // is sufficient.
        let drain = Arc::new(envoy_listener::DrainState::new(&registry));
        AdminHandler::new(
            cfg,
            registry,
            Arc::new(bootstrap),
            Arc::new(ClusterManager::empty()),
            Instant::now(),
            BTreeMap::new(),
            drain,
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
mod clusters_config_dump_tests {
    //! Phase 18 Task 5 — D5 (ADR-0049 L5): the `ClustersConfigDump`
    //! `/config_dump` entry, emitted CONDITIONALLY (only when
    //! `dynamic_resources.cds_config` is configured). Four test groups:
    //! (a) conditional emission — no `dynamic_resources` ⇒ exactly one entry
    //! (the fixture-0014 single-entry regression shape); (b) the entry with a
    //! dynamic cluster present (`configs[1]` shape: outer `@type`, inner
    //! `cluster.@type`, `cluster.name`, ISO-8601 `last_updated`); (c) empty-key
    //! omission (zero static clusters ⇒ no `static_clusters` key; a static
    //! cluster present ⇒ the key is present); (d) the BootstrapConfigDump shows
    //! `dynamic_resources` but NOT the loaded clusters (the `#[serde(skip)]`
    //! `dynamic_clusters` separation, §5.5).

    use super::AdminEndpoint;
    use crate::config::AdminConfig;
    use crate::handler::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::{Address, Admin, Bootstrap, SocketAddress};
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    /// A single STRICT_DNS cluster named `dynamic_backend`, the L5 fixture
    /// shape. Parsed standalone (not via the bootstrap path) so the test can
    /// inject it into `dynamic_clusters` directly.
    const DYNAMIC_BACKEND_CLUSTER: &str = "\
name: dynamic_backend
type: STRICT_DNS
lb_policy: ROUND_ROBIN
load_assignment:
  cluster_name: dynamic_backend
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address:
            address: backend.example.com
            port_value: 8080
";

    /// A static cluster (type STATIC) named `static_backend`, used by the
    /// static_clusters-key-presence test (group c inverse).
    const STATIC_BACKEND_CLUSTER: &str = "\
name: static_backend
type: STATIC
lb_policy: ROUND_ROBIN
load_assignment:
  cluster_name: static_backend
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address:
            address: 127.0.0.1
            port_value: 9000
";

    /// Bootstrap WITH `dynamic_resources.cds_config` configured (triggers the
    /// conditional ClustersConfigDump emission). The `path` is never read at
    /// render time — the loaded clusters live in `dynamic_clusters`.
    const DR_BOOTSTRAP: &str = "\
node:
  id: t
  cluster: c
dynamic_resources:
  cds_config:
    path_config_source:
      path: /etc/cds.yaml
static_resources:
  listeners: []
  clusters: []
";

    fn parse_cluster(yaml: &str) -> envoy_config::Cluster {
        serde_yaml::from_str(yaml).expect("cluster yaml parses")
    }

    /// Build a handler from an already-constructed `Bootstrap` (mirrors
    /// `config_dump_tests::handler_with_bootstrap`, but takes the owned
    /// `Bootstrap` so a test can populate `dynamic_clusters` first).
    fn handler_from_bootstrap(bootstrap: Bootstrap) -> AdminHandler {
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
        let registry = Arc::new(StatsRegistry::new());
        let drain = Arc::new(envoy_listener::DrainState::new(&registry));
        AdminHandler::new(
            cfg,
            registry,
            Arc::new(bootstrap),
            Arc::new(ClusterManager::empty()),
            Instant::now(),
            BTreeMap::new(),
            drain,
        )
    }

    fn parse_bootstrap(yaml: &str) -> Bootstrap {
        serde_yaml::from_str(yaml).expect("bootstrap yaml parses")
    }

    fn dump_value(handler: &AdminHandler) -> serde_json::Value {
        let resp = AdminEndpoint::ConfigDump.render_with(handler);
        let body_str = std::str::from_utf8(&resp.body).expect("utf-8");
        serde_json::from_str(body_str).expect("valid JSON")
    }

    // (a) conditional emission: no dynamic_resources ⇒ exactly ONE entry.
    #[test]
    fn no_dynamic_resources_emits_single_bootstrap_entry() {
        let bootstrap = parse_bootstrap(
            "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n",
        );
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(configs.len(), 1, "no dynamic_resources ⇒ single entry");
        assert_eq!(
            configs[0].get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.BootstrapConfigDump")
        );
    }

    // (b) with a dynamic cluster present ⇒ TWO entries; configs[1] is the
    // ClustersConfigDump with the expected nested shape.
    #[test]
    fn dynamic_cluster_emits_clusters_config_dump_at_configs_1() {
        let mut bootstrap = parse_bootstrap(DR_BOOTSTRAP);
        bootstrap.dynamic_clusters = Some(vec![parse_cluster(DYNAMIC_BACKEND_CLUSTER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(configs.len(), 2, "dynamic_resources ⇒ two entries");
        let entry = &configs[1];
        assert_eq!(
            entry.get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.ClustersConfigDump")
        );
        assert_eq!(
            entry
                .pointer("/dynamic_active_clusters/0/cluster/name")
                .and_then(|v| v.as_str()),
            Some("dynamic_backend")
        );
        assert_eq!(
            entry
                .pointer("/dynamic_active_clusters/0/cluster/@type")
                .and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.config.cluster.v3.Cluster")
        );
        // last_updated parses as a non-empty ISO-8601 string (same format the
        // BootstrapConfigDump entry uses: format_iso8601).
        let last_updated = entry
            .pointer("/dynamic_active_clusters/0/last_updated")
            .and_then(|v| v.as_str())
            .expect("last_updated is a string");
        assert!(!last_updated.is_empty(), "last_updated non-empty");
        // RFC-3339 / ISO-8601 parseable (the BootstrapConfigDump format).
        assert!(
            last_updated.contains('T') && last_updated.ends_with('Z'),
            "last_updated ISO-8601-shaped; got {last_updated:?}"
        );
    }

    // (c) empty-key omission (L5): zero static clusters ⇒ NO static_clusters key.
    #[test]
    fn zero_static_clusters_omits_static_clusters_key() {
        let mut bootstrap = parse_bootstrap(DR_BOOTSTRAP);
        bootstrap.dynamic_clusters = Some(vec![parse_cluster(DYNAMIC_BACKEND_CLUSTER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let entry = &value.get("configs").and_then(|c| c.as_array()).unwrap()[1];
        assert!(
            entry.get("static_clusters").is_none(),
            "zero static clusters ⇒ static_clusters key omitted; entry was {entry}"
        );
        // Inverse cheap check: dynamic_active_clusters key IS present.
        assert!(entry.get("dynamic_active_clusters").is_some());
    }

    // (c, inverse) a static cluster present + dynamic_resources configured ⇒
    // static_clusters key present, carrying it.
    #[test]
    fn static_cluster_present_emits_static_clusters_key() {
        let mut bootstrap = parse_bootstrap(DR_BOOTSTRAP);
        bootstrap.static_resources.clusters = vec![parse_cluster(STATIC_BACKEND_CLUSTER)];
        bootstrap.dynamic_clusters = Some(vec![parse_cluster(DYNAMIC_BACKEND_CLUSTER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let entry = &value.get("configs").and_then(|c| c.as_array()).unwrap()[1];
        assert_eq!(
            entry
                .pointer("/static_clusters/0/cluster/name")
                .and_then(|v| v.as_str()),
            Some("static_backend")
        );
        assert_eq!(
            entry
                .pointer("/static_clusters/0/cluster/@type")
                .and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.config.cluster.v3.Cluster")
        );
    }

    // (d) §5.5 separation: the BootstrapConfigDump entry shows dynamic_resources
    // but NOT the loaded clusters (dynamic_clusters is #[serde(skip)]).
    #[test]
    fn bootstrap_entry_shows_dynamic_resources_not_loaded_clusters() {
        let mut bootstrap = parse_bootstrap(DR_BOOTSTRAP);
        bootstrap.dynamic_clusters = Some(vec![parse_cluster(DYNAMIC_BACKEND_CLUSTER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let boot = &value.get("configs").and_then(|c| c.as_array()).unwrap()[0];
        // BootstrapConfigDump carries the dynamic_resources subtree.
        assert!(
            boot.pointer("/bootstrap/dynamic_resources/cds_config/path_config_source/path")
                .and_then(|v| v.as_str())
                .is_some(),
            "BootstrapConfigDump shows dynamic_resources; entry was {boot}"
        );
        // ...but static_resources.clusters stays empty (the loaded
        // dynamic_clusters are #[serde(skip)] — structurally excluded).
        let static_clusters = boot
            .pointer("/bootstrap/static_resources/clusters")
            .and_then(|v| v.as_array())
            .expect("static_resources.clusters is an array");
        assert!(
            static_clusters.is_empty(),
            "loaded dynamic clusters must NOT appear in the BootstrapConfigDump"
        );
        // And there is no `dynamic_clusters` key anywhere in the bootstrap subtree.
        assert!(
            boot.pointer("/bootstrap/dynamic_clusters").is_none(),
            "dynamic_clusters is #[serde(skip)] — must be absent"
        );
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

#[cfg(test)]
mod drain_admin_tests {
    //! Phase 08.2 Task 3 — D9 + D10: three POST admin endpoints
    //! (`/drain_listeners`, `/healthcheck/fail`, `/healthcheck/ok`). Nine
    //! tests: 4 dispatch-shape tests (per-path POST routing + GET-405 for
    //! `/drain_listeners`); 3 render-side-effect tests (each render fn
    //! returns 200 OK empty body AND flips the underlying `DrainState`);
    //! 1 sticky-drain regression test (`/healthcheck/ok` AFTER `/drain_listeners`
    //! is a no-op — state stays `Draining`); 1 allowed-method declaration
    //! tautology covering all 3 variants.

    use super::{
        AdminEndpoint, Dispatch, render_drain_listeners, render_healthcheck_fail,
        render_healthcheck_ok,
    };

    #[test]
    fn drain_listeners_path_dispatches_on_post() {
        let dispatch = AdminEndpoint::dispatch("POST", "/drain_listeners");
        assert!(matches!(
            dispatch,
            Dispatch::Endpoint(AdminEndpoint::DrainListeners)
        ));
    }

    #[test]
    fn drain_listeners_405_on_get() {
        let dispatch = AdminEndpoint::dispatch("GET", "/drain_listeners");
        assert!(matches!(
            dispatch,
            Dispatch::MethodNotAllowed { allow: "POST" }
        ));
    }

    #[test]
    fn healthcheck_fail_path_dispatches_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/healthcheck/fail"),
            Dispatch::Endpoint(AdminEndpoint::HealthcheckFail)
        ));
    }

    #[test]
    fn healthcheck_ok_path_dispatches_on_post() {
        assert!(matches!(
            AdminEndpoint::dispatch("POST", "/healthcheck/ok"),
            Dispatch::Endpoint(AdminEndpoint::HealthcheckOk)
        ));
    }

    #[test]
    fn drain_listeners_render_returns_200_empty_body_and_invokes_drain() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        let resp = render_drain_listeners(&drain);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, Some("OK"));
        assert!(resp.body.is_empty(), "200 OK body must be empty");
        assert_eq!(drain.current(), envoy_listener::DrainStage::Draining);
    }

    #[test]
    fn healthcheck_fail_render_returns_200_empty_body_and_flips_state() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        let resp = render_healthcheck_fail(&drain);
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_empty());
        assert_eq!(
            drain.current(),
            envoy_listener::DrainStage::HealthcheckFailing
        );
    }

    #[test]
    fn healthcheck_ok_render_returns_200_empty_body_and_restores_live() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        drain.fail_healthcheck();
        let resp = render_healthcheck_ok(&drain);
        assert_eq!(resp.status, 200);
        assert!(resp.body.is_empty());
        assert_eq!(drain.current(), envoy_listener::DrainStage::Live);
    }

    #[test]
    fn healthcheck_ok_after_drain_is_noop_via_render_fn() {
        let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
        let drain = envoy_listener::DrainState::new(&registry);
        drain.drain();
        let resp = render_healthcheck_ok(&drain);
        assert_eq!(resp.status, 200);
        assert_eq!(
            drain.current(),
            envoy_listener::DrainStage::Draining,
            "sticky drain: ok_healthcheck after drain must NOT un-drain"
        );
    }

    #[test]
    fn each_drain_endpoint_declares_post_allowed_method() {
        assert_eq!(AdminEndpoint::DrainListeners.allowed_method(), "POST");
        assert_eq!(AdminEndpoint::HealthcheckFail.allowed_method(), "POST");
        assert_eq!(AdminEndpoint::HealthcheckOk.allowed_method(), "POST");
    }
}

#[cfg(test)]
mod ready_drain_tests {
    //! Phase 08.2 Task 5 — D5e + D-ready: `/server_info` state-source rebind
    //! and `/ready` drain-aware response. Five tests: two `server_info`
    //! state-source tests (Draining → "DRAINING"; HealthcheckFailing →
    //! "LIVE" — server-state is INDEPENDENT of healthcheck-failure per
    //! parent-08 SPEC §5.5) and three `ready` response-shape tests
    //! (Live → 200 LIVE; Draining → 503 DRAINING; HealthcheckFailing →
    //! 503 Service Unavailable).
    //!
    //! Test helper `test_handler_with_drain(drain)` mirrors the existing
    //! `handler_with_bootstrap` helper in `config_dump_tests` but accepts
    //! a pre-constructed `Arc<DrainState>` so the test can drive the
    //! underlying state transitions BEFORE invoking the render fn.

    use super::config_dump_tests::TINY_BOOTSTRAP;
    use super::{AdminEndpoint, render_server_info};
    use crate::config::AdminConfig;
    use crate::handler::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::{Address, Admin, Bootstrap, SocketAddress};
    use envoy_listener::DrainState;
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    fn test_handler_with_drain(drain: Arc<DrainState>) -> AdminHandler {
        let bootstrap: Bootstrap = serde_yaml::from_str(TINY_BOOTSTRAP).expect("yaml parses");
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
        let registry = Arc::new(StatsRegistry::new());
        AdminHandler::new(
            cfg,
            registry,
            Arc::new(bootstrap),
            Arc::new(ClusterManager::empty()),
            Instant::now(),
            BTreeMap::new(),
            drain,
        )
    }

    #[test]
    fn server_info_state_is_draining_when_drain_state_is_draining() {
        let registry = Arc::new(StatsRegistry::new());
        let drain = Arc::new(DrainState::new(&registry));
        drain.drain();
        let handler = test_handler_with_drain(Arc::clone(&drain));
        let resp = render_server_info(&handler);
        let value: serde_json::Value =
            serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        assert_eq!(
            value.get("state").and_then(|v| v.as_str()),
            Some("DRAINING")
        );
    }

    #[test]
    fn server_info_state_is_live_when_drain_state_is_healthcheck_failing() {
        // Envoy's server-state is INDEPENDENT of healthcheck-failure per
        // parent-08 SPEC §5.5 — `/server_info.state` stays "LIVE" while
        // `/ready` flips to 503.
        let registry = Arc::new(StatsRegistry::new());
        let drain = Arc::new(DrainState::new(&registry));
        drain.fail_healthcheck();
        let handler = test_handler_with_drain(Arc::clone(&drain));
        let resp = render_server_info(&handler);
        let value: serde_json::Value =
            serde_json::from_str(std::str::from_utf8(&resp.body).unwrap()).unwrap();
        assert_eq!(value.get("state").and_then(|v| v.as_str()), Some("LIVE"));
    }

    #[test]
    fn ready_returns_200_live_when_drain_state_is_live() {
        let registry = Arc::new(StatsRegistry::new());
        let drain = Arc::new(DrainState::new(&registry));
        let handler = test_handler_with_drain(Arc::clone(&drain));
        let resp = AdminEndpoint::Ready.render_with(&handler);
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, Some("OK"));
        assert_eq!(&resp.body[..], b"LIVE\n");
    }

    #[test]
    fn ready_returns_503_draining_when_drain_state_is_draining() {
        let registry = Arc::new(StatsRegistry::new());
        let drain = Arc::new(DrainState::new(&registry));
        drain.drain();
        let handler = test_handler_with_drain(Arc::clone(&drain));
        let resp = AdminEndpoint::Ready.render_with(&handler);
        assert_eq!(resp.status, 503);
        assert_eq!(resp.reason, Some("Service Unavailable"));
        assert_eq!(&resp.body[..], b"DRAINING\n");
    }

    #[test]
    fn ready_returns_503_service_unavailable_when_drain_state_is_healthcheck_failing() {
        let registry = Arc::new(StatsRegistry::new());
        let drain = Arc::new(DrainState::new(&registry));
        drain.fail_healthcheck();
        let handler = test_handler_with_drain(Arc::clone(&drain));
        let resp = AdminEndpoint::Ready.render_with(&handler);
        assert_eq!(resp.status, 503);
        assert_eq!(resp.reason, Some("Service Unavailable"));
        assert_eq!(&resp.body[..], b"Service Unavailable\n");
    }
}
