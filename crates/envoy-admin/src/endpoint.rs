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
        // 21 D5: strip the query string so /config_dump?include_eds routes to
        // ConfigDump (Envoy's admin does the same; surfaces the
        // EndpointsConfigDump bilaterally — L5). No existing fixture uses a
        // query string, so this is inert for the established endpoints.
        let path = path.split('?').next().unwrap_or(path);
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
    /// 21 D5 (ADR-0053/0054; §6.2 L5): the `EndpointsConfigDump` entry, emitted
    /// ONLY when some cluster is `type: EDS` (with a populated `load_assignment`)
    /// — fixtures 0014/0026/0027/0028 (no EDS cluster) stay untouched. File-based
    /// EDS endpoints land under `static_endpoint_configs[].endpoint_config` (file
    /// config is "static" config-dump-wise — L5), NOT
    /// `dynamic_endpoint_configs[]`. There is NO `version_info`/`last_updated`
    /// key. Pushed AFTER the Clusters entry / BEFORE the Listeners entry, matching
    /// Envoy's `?include_eds` order (Clusters[1]/Endpoints[2]/Listeners[3]).
    /// Envoy gates this section behind `?include_eds`; envoy-rust strips the query
    /// string and emits unconditionally-when-EDS (a recorded narrowing) — the
    /// per-side `configs[]` index mismatch is reconciled in the harness.
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.EndpointsConfigDump")]
    Endpoints {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        static_endpoint_configs: Vec<StaticEndpointConfigEntry<'a>>,
    },
    /// Phase 19 D5 (ADR-0050 §6.2 L5): the `ListenersConfigDump` entry, emitted
    /// ONLY when `dynamic_resources.lds_config` is configured (fixtures 0014 +
    /// 0026 untouched). Pushed AFTER the Clusters entry — Envoy v1.33's verified
    /// `configs[]` order is Bootstrap[0], Clusters[1], Listeners[2]. Envoy's LDS
    /// dump nests the listener under `dynamic_listeners[].active_state.listener`
    /// (a DIFFERENT shape from the CDS dump's flat
    /// `dynamic_active_clusters[].cluster`); there is NO `version_info` key for
    /// file-based LDS.
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.ListenersConfigDump")]
    Listeners {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        static_listeners: Vec<StaticListenerEntry<'a>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dynamic_listeners: Vec<DynamicListenerEntry<'a>>,
    },
    /// 20 D5 (ADR-0051/0052; §6.2 L5): the `RoutesConfigDump` entry, emitted ONLY
    /// when some HCM uses `rds` (fixtures 0014/0026/0027 untouched — their HCMs
    /// carry inline route_config). Envoy ALSO emits this section (static_route_configs
    /// for inline routes) and positions it at configs[4] after a ScopedRoutesConfigDump;
    /// envoy-rust narrows to rds-only and pushes it after the (conditional) Listeners
    /// entry. NO `version_info` key (the RDS file carried none). The per-side configs[]
    /// index mismatch (envoy [4] vs envoy-rust [2/3]) is reconciled in the harness (Task 6).
    #[serde(rename = "type.googleapis.com/envoy.admin.v3.RoutesConfigDump")]
    Routes {
        #[serde(skip_serializing_if = "Vec::is_empty")]
        dynamic_route_configs: Vec<DynamicRouteConfigEntry<'a>>,
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

/// Phase 21 D5 (L5): one entry in `EndpointsConfigDump.static_endpoint_configs`.
/// Envoy shape: `{"endpoint_config": {...}}` (no `last_updated` — file-based EDS
/// is "static" config-dump-wise). The nested `endpoint_config` carries its own
/// `@type` + the resolved `ClusterLoadAssignment` body.
#[derive(Serialize)]
pub(crate) struct StaticEndpointConfigEntry<'a> {
    pub(crate) endpoint_config: ClusterLoadAssignmentBody<'a>,
}

/// Phase 21 D5: the `ClusterLoadAssignment` body nested inside a
/// `StaticEndpointConfigEntry`. Carries the inner `@type` tag Envoy's
/// `google.protobuf.Any`-projection puts on the `endpoint_config` object, plus
/// the resolved `cluster_name` + `endpoints` borrowed from the cluster's
/// (EDS-populated) `LoadAssignment`. Borrowed fields (no `Clone` cascade — same
/// idiom as `TaggedCluster`); the `LoadAssignment` reuse keeps the `endpoints`
/// serialization byte-identical to the inline-CLA shape.
#[derive(Serialize)]
pub(crate) struct ClusterLoadAssignmentBody<'a> {
    #[serde(rename = "@type")]
    pub(crate) type_url: &'static str,
    pub(crate) cluster_name: &'a str,
    pub(crate) endpoints: &'a Vec<envoy_config::LocalityLbEndpoints>,
}

/// The `@type` URL for the nested `endpoint_config` object inside
/// `EndpointsConfigDump`.
const CLUSTER_LOAD_ASSIGNMENT_TYPE_URL: &str =
    "type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment";

/// Phase 19 D5: one static listener inside `ListenersConfigDump`. Envoy shape:
/// `{"listener": {...}, "last_updated": "..."}`.
#[derive(Serialize)]
pub(crate) struct StaticListenerEntry<'a> {
    pub(crate) listener: TaggedListener<'a>,
    pub(crate) last_updated: String,
}

/// Phase 19 D5: one dynamically-loaded listener. Envoy shape:
/// `{"name": ..., "active_state": {...}}` — the LDS dump nests the listener one
/// level deeper than the CDS dump (under `active_state`).
#[derive(Serialize)]
pub(crate) struct DynamicListenerEntry<'a> {
    pub(crate) name: &'a str,
    pub(crate) active_state: ListenerActiveState<'a>,
}

/// Phase 19 D5: the `active_state` nesting inside a `DynamicListenerEntry`
/// (L5 ✧: `listener` + `last_updated`; NO `version_info` for file-based LDS).
#[derive(Serialize)]
pub(crate) struct ListenerActiveState<'a> {
    pub(crate) listener: TaggedListener<'a>,
    pub(crate) last_updated: String,
}

/// Phase 19 D5: a `Listener` serialized with the inner `@type` tag Envoy's
/// `google.protobuf.Any`-projection carries on the nested `listener` object.
/// Mirrors `TaggedCluster`: a flatten on a NESTED struct (not on the
/// internally-tagged outer enum's variant content), so it sidesteps serde's
/// flatten+internally-tagged-enum limitation.
#[derive(Serialize)]
pub(crate) struct TaggedListener<'a> {
    #[serde(rename = "@type")]
    pub(crate) type_url: &'static str,
    #[serde(flatten)]
    pub(crate) listener: &'a envoy_config::Listener,
}

/// The `@type` URL for the nested `listener` object inside `ListenersConfigDump`.
const LISTENER_TYPE_URL: &str = "type.googleapis.com/envoy.config.listener.v3.Listener";

/// Phase 20 D5: one dynamically-loaded (RDS) route configuration. Envoy shape:
/// `{"route_config": {...}, "last_updated": "..."}`.
#[derive(Serialize)]
pub(crate) struct DynamicRouteConfigEntry<'a> {
    pub(crate) route_config: TaggedRouteConfig<'a>,
    pub(crate) last_updated: String,
}

/// Phase 20 D5: a `RouteConfiguration` serialized with the inner `@type` tag.
/// Mirrors `TaggedCluster`/`TaggedListener` (flatten on a NESTED struct).
#[derive(Serialize)]
pub(crate) struct TaggedRouteConfig<'a> {
    #[serde(rename = "@type")]
    pub(crate) type_url: &'static str,
    #[serde(flatten)]
    pub(crate) route_config: &'a envoy_config::RouteConfiguration,
}

/// The `@type` URL for the nested `route_config` object inside `RoutesConfigDump`.
const ROUTE_CONFIG_TYPE_URL: &str = "type.googleapis.com/envoy.config.route.v3.RouteConfiguration";

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
    // 21 D5 (ADR-0053/0054; §6.2 L5): emit EndpointsConfigDump ONLY when some
    // cluster is `type: EDS` (with a populated load_assignment) — conditional
    // emission; fixtures 0014/0026/0027/0028 (no EDS cluster) untouched. Uses
    // static_endpoint_configs (file-based EDS is "static" config-dump-wise — L5);
    // pushed after the (conditional) Clusters entry / before Listeners (Envoy's
    // ?include_eds order Clusters[1]/Endpoints[2]/Listeners[3]). On a bootstrap
    // with cds_config it lands at configs[2]; with no cds, configs[1]. envoy-rust
    // emits it unconditional of ?include_eds (a recorded narrowing); the per-side
    // index mismatch is reconciled in the harness.
    let static_endpoint_configs: Vec<StaticEndpointConfigEntry> = bootstrap
        .all_clusters()
        .filter(|c| c.cluster_type == envoy_config::ClusterType::Eds)
        .filter_map(|c| {
            c.load_assignment
                .as_ref()
                .map(|la| StaticEndpointConfigEntry {
                    endpoint_config: ClusterLoadAssignmentBody {
                        type_url: CLUSTER_LOAD_ASSIGNMENT_TYPE_URL,
                        cluster_name: &la.cluster_name,
                        endpoints: &la.endpoints,
                    },
                })
        })
        .collect();
    if !static_endpoint_configs.is_empty() {
        configs.push(ConfigDumpEntry::Endpoints {
            static_endpoint_configs,
        });
    }
    // Phase 19 D5 (ADR-0050 §6.2 L5): emit the ListenersConfigDump entry ONLY
    // when `dynamic_resources.lds_config` is configured. Pushed AFTER the
    // Clusters entry ⇒ `configs[2]` when both are present (Envoy v1.33's
    // verified order: Bootstrap[0], Clusters[1], Listeners[2]). Empty listener
    // lists serialize to omitted keys via the variant's `skip_serializing_if`.
    if bootstrap
        .dynamic_resources
        .as_ref()
        .and_then(|dr| dr.lds_config.as_ref())
        .is_some()
    {
        let static_listeners = bootstrap
            .static_resources
            .listeners
            .iter()
            .map(|listener| StaticListenerEntry {
                listener: TaggedListener {
                    type_url: LISTENER_TYPE_URL,
                    listener,
                },
                last_updated: last_updated.clone(),
            })
            .collect();
        let dynamic_listeners = bootstrap
            .dynamic_listeners
            .iter()
            .flatten()
            .map(|listener| DynamicListenerEntry {
                name: &listener.name,
                active_state: ListenerActiveState {
                    listener: TaggedListener {
                        type_url: LISTENER_TYPE_URL,
                        listener,
                    },
                    last_updated: last_updated.clone(),
                },
            })
            .collect();
        configs.push(ConfigDumpEntry::Listeners {
            static_listeners,
            dynamic_listeners,
        });
    }
    // 20 D5 (ADR-0051/0052; §6.2 L5): emit RoutesConfigDump ONLY when some HCM
    // uses rds. Pushed after the (conditional) Listeners entry — on fixture 0028
    // (cds yes, lds no) it lands at configs[2]; the per-side index mismatch vs
    // Envoy's configs[4] is reconciled in the harness.
    // 26 Task 6: render the RoutesConfigDump through the LIVE, swappable route
    // table (read via `HCMConfig::current_route_config()`) so the dump reflects
    // the HOT-RELOADED table after an RDS reload, not the frozen startup
    // bootstrap snapshot. For each rds HCM we look up its live handle by
    // `rds.route_config_name`; if found we render that live `Arc` snapshot, else
    // we fall back to the bootstrap `route_config` borrow (the empty-handle path
    // used by tests and non-rds-watch processes — a defensive no-op in
    // production where every rds HCM always has a handle).
    //
    // Because the entries borrow `&RouteConfiguration`, we first materialize an
    // OWNED Vec of snapshots (`RouteSnapshot`) that lives until serialization,
    // then build the borrowing entries from it. The live arm owns an `Arc`; the
    // fallback arm borrows from `bootstrap` (which outlives this whole fn) —
    // `RouteConfiguration` is not `Clone`, so a borrow is the only fallback.
    enum RouteSnapshot<'a> {
        Live(std::sync::Arc<envoy_config::RouteConfiguration>),
        Bootstrap(&'a envoy_config::RouteConfiguration),
    }
    impl RouteSnapshot<'_> {
        fn as_ref(&self) -> &envoy_config::RouteConfiguration {
            match self {
                RouteSnapshot::Live(arc) => arc.as_ref(),
                RouteSnapshot::Bootstrap(rc) => rc,
            }
        }
    }
    let route_snapshots: Vec<RouteSnapshot<'_>> = bootstrap
        .all_listeners()
        .flat_map(|l| l.filter_chains.iter())
        .flat_map(|c| c.filters.iter())
        .filter_map(|f| match f.typed_config.as_ref() {
            Some(envoy_config::TypedConfig::HttpConnectionManager(hcm)) if hcm.rds.is_some() => {
                let name = &hcm.rds.as_ref().unwrap().route_config_name;
                // First-wins by `route_config_name`: each rds HCM maps to exactly
                // one live handle, and `route_config_name` is unique across a valid
                // bootstrap, so the linear `find` over the tiny Vec is unambiguous.
                if let Some((_, handle)) = handler
                    .live_route_configs()
                    .iter()
                    .find(|(n, _)| n == name)
                {
                    // Live: read-once the swappable table (the reloaded one).
                    Some(RouteSnapshot::Live(handle.current_route_config()))
                } else {
                    // Fallback: the bootstrap snapshot borrow. In production every
                    // rds HCM has a live handle, so a miss against a NON-empty
                    // handle set means the wiring drifted (e.g. a name mismatch
                    // between envoy-bin and here) and the dump would silently show
                    // the STALE startup table — the exact failure this task removes.
                    // Surface it rather than fall back silently. (An empty handle
                    // set is the legitimate tests / non-rds-watch path — no warn.)
                    if !handler.live_route_configs().is_empty() {
                        tracing::warn!(
                            route_config_name = %name,
                            "no live RDS route-table handle for an rds HCM; /config_dump \
                             falling back to the startup bootstrap snapshot (table may be stale)"
                        );
                    }
                    hcm.route_config.as_ref().map(RouteSnapshot::Bootstrap)
                }
            }
            _ => None,
        })
        .collect();
    let dynamic_route_configs: Vec<DynamicRouteConfigEntry> = route_snapshots
        .iter()
        .map(|snap| DynamicRouteConfigEntry {
            route_config: TaggedRouteConfig {
                type_url: ROUTE_CONFIG_TYPE_URL,
                route_config: snap.as_ref(),
            },
            last_updated: last_updated.clone(),
        })
        .collect();
    if !dynamic_route_configs.is_empty() {
        configs.push(ConfigDumpEntry::Routes {
            dynamic_route_configs,
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
        .all_listeners()
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
            Vec::new(),
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
            Vec::new(),
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
mod listeners_config_dump_tests {
    //! Phase 19 Task 5 — D5 (ADR-0050 §6.2 L5): the `ListenersConfigDump`
    //! `/config_dump` entry, emitted CONDITIONALLY (only when
    //! `dynamic_resources.lds_config` is configured) and pushed AFTER the
    //! `ClustersConfigDump` entry — Envoy v1.33's verified `configs[]` order is
    //! Bootstrap[0], Clusters[1], Listeners[2]. Test groups: (a) conditional
    //! emission — a plain bootstrap and a `cds_config`-only bootstrap render NO
    //! ListenersConfigDump entry (fixture-0014 + fixture-0026 regression
    //! shapes); (b) the entry with a dynamic listener present (`configs[2]`
    //! shape: outer `@type`, nested `dynamic_listeners[].active_state.listener`
    //! — a DIFFERENT nesting from the CDS dump's flat
    //! `dynamic_active_clusters[].cluster`; NO `version_info` key); (c)
    //! empty-key omission (zero static listeners ⇒ no `static_listeners` key);
    //! (d) the BootstrapConfigDump never shows `dynamic_listeners` (the
    //! `#[serde(skip)]` separation, §5.5).

    use super::AdminEndpoint;
    use crate::config::AdminConfig;
    use crate::handler::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::{Address, Admin, Bootstrap, SocketAddress};
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    /// A single listener named `dynamic_listener`, the L5 fixture shape. Parsed
    /// standalone (not via the bootstrap path) so the test can inject it into
    /// `dynamic_listeners` directly.
    const DYNAMIC_LISTENER: &str = "\
name: dynamic_listener
address:
  socket_address:
    address: 0.0.0.0
    port_value: 10000
filter_chains: []
";

    /// A static listener named `static_listener`, used by the
    /// static_listeners-key-presence test (group c inverse).
    const STATIC_LISTENER: &str = "\
name: static_listener
address:
  socket_address:
    address: 0.0.0.0
    port_value: 10001
filter_chains: []
";

    /// Bootstrap WITH BOTH `cds_config` and `lds_config` configured (triggers
    /// the conditional Clusters AND Listeners emission — the Listeners entry
    /// must land at `configs[2]`, AFTER Clusters). The `path`s are never read at
    /// render time — the loaded resources live in `dynamic_clusters` /
    /// `dynamic_listeners`.
    const CDS_LDS_BOOTSTRAP: &str = "\
node:
  id: t
  cluster: c
dynamic_resources:
  cds_config:
    path_config_source:
      path: /etc/cds.yaml
  lds_config:
    path_config_source:
      path: /etc/lds.yaml
static_resources:
  listeners: []
  clusters: []
";

    /// Bootstrap with ONLY `cds_config` (the fixture-0026 regression shape — it
    /// renders exactly Bootstrap[0] + Clusters[1], NO Listeners).
    const CDS_ONLY_BOOTSTRAP: &str = "\
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

    fn parse_listener(yaml: &str) -> envoy_config::Listener {
        serde_yaml::from_str(yaml).expect("listener yaml parses")
    }

    fn parse_cluster(yaml: &str) -> envoy_config::Cluster {
        serde_yaml::from_str(yaml).expect("cluster yaml parses")
    }

    /// A STRICT_DNS cluster named `dynamic_backend` — used to populate
    /// `dynamic_clusters` so the Clusters entry is present and the Listeners
    /// entry must order AFTER it.
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

    /// Build a handler from an already-constructed `Bootstrap` (mirrors
    /// `clusters_config_dump_tests::handler_from_bootstrap`).
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
            Vec::new(),
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

    const LISTENERS_TYPE: &str = "type.googleapis.com/envoy.admin.v3.ListenersConfigDump";

    fn has_listeners_entry(value: &serde_json::Value) -> bool {
        value
            .get("configs")
            .and_then(|c| c.as_array())
            .unwrap()
            .iter()
            .any(|e| e.get("@type").and_then(|v| v.as_str()) == Some(LISTENERS_TYPE))
    }

    // (a) conditional emission — a PLAIN bootstrap (no dynamic_resources)
    // renders NO ListenersConfigDump entry (fixture-0014 regression shape).
    #[test]
    fn plain_bootstrap_emits_no_listeners_config_dump() {
        let bootstrap = parse_bootstrap(
            "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n",
        );
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        assert!(
            !has_listeners_entry(&value),
            "plain bootstrap ⇒ no ListenersConfigDump entry; got {value}"
        );
    }

    // (a) conditional emission — a CDS-ONLY bootstrap renders exactly
    // Bootstrap[0] + Clusters[1] and NO ListenersConfigDump (fixture-0026
    // regression shape: the CDS-only topology is untouched).
    #[test]
    fn cds_only_bootstrap_emits_no_listeners_config_dump() {
        let mut bootstrap = parse_bootstrap(CDS_ONLY_BOOTSTRAP);
        bootstrap.dynamic_clusters = Some(vec![parse_cluster(DYNAMIC_BACKEND_CLUSTER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(
            configs.len(),
            2,
            "cds-only ⇒ exactly Bootstrap[0] + Clusters[1]; got {value}"
        );
        assert_eq!(
            configs[1].get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.ClustersConfigDump")
        );
        assert!(
            !has_listeners_entry(&value),
            "cds-only ⇒ no ListenersConfigDump entry"
        );
    }

    // (b) with a dynamic listener (+ dynamic cluster) present ⇒ THREE entries;
    // the order lock: Clusters BEFORE Listeners. configs[2] is the
    // ListenersConfigDump with the nested active_state shape and NO version_info.
    #[test]
    fn dynamic_listener_emits_listeners_config_dump_at_configs_2() {
        let mut bootstrap = parse_bootstrap(CDS_LDS_BOOTSTRAP);
        bootstrap.dynamic_clusters = Some(vec![parse_cluster(DYNAMIC_BACKEND_CLUSTER)]);
        bootstrap.dynamic_listeners = Some(vec![parse_listener(DYNAMIC_LISTENER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(configs.len(), 3, "cds+lds ⇒ three entries; got {value}");
        // Order lock (L5): Clusters BEFORE Listeners.
        assert_eq!(
            configs[1].get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.ClustersConfigDump"),
            "configs[1] must be ClustersConfigDump (order lock)"
        );
        let entry = &configs[2];
        assert_eq!(
            entry.get("@type").and_then(|v| v.as_str()),
            Some(LISTENERS_TYPE)
        );
        assert_eq!(
            entry
                .pointer("/dynamic_listeners/0/name")
                .and_then(|v| v.as_str()),
            Some("dynamic_listener")
        );
        // The active_state nesting (DIFFERENT from the CDS flat shape).
        assert_eq!(
            entry
                .pointer("/dynamic_listeners/0/active_state/listener/@type")
                .and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.config.listener.v3.Listener")
        );
        assert_eq!(
            entry
                .pointer("/dynamic_listeners/0/active_state/listener/name")
                .and_then(|v| v.as_str()),
            Some("dynamic_listener")
        );
        // last_updated lives inside active_state and is ISO-8601-shaped.
        let last_updated = entry
            .pointer("/dynamic_listeners/0/active_state/last_updated")
            .and_then(|v| v.as_str())
            .expect("active_state.last_updated is a string");
        assert!(!last_updated.is_empty(), "last_updated non-empty");
        assert!(
            last_updated.contains('T') && last_updated.ends_with('Z'),
            "last_updated ISO-8601-shaped; got {last_updated:?}"
        );
        // L5 ✧: NO version_info key anywhere in active_state.
        assert!(
            entry
                .pointer("/dynamic_listeners/0/active_state/version_info")
                .is_none(),
            "file-based LDS active_state must carry NO version_info key; entry was {entry}"
        );
    }

    // (c) empty-key omission (L5): zero static listeners ⇒ NO static_listeners
    // key (fixture 0027 has zero static listeners → ABSENT, not []).
    #[test]
    fn zero_static_listeners_omits_static_listeners_key() {
        let mut bootstrap = parse_bootstrap(CDS_LDS_BOOTSTRAP);
        bootstrap.dynamic_listeners = Some(vec![parse_listener(DYNAMIC_LISTENER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        let entry = configs
            .iter()
            .find(|e| e.get("@type").and_then(|v| v.as_str()) == Some(LISTENERS_TYPE))
            .expect("ListenersConfigDump entry present");
        assert!(
            entry.get("static_listeners").is_none(),
            "zero static listeners ⇒ static_listeners key omitted; entry was {entry}"
        );
        // Inverse cheap check: dynamic_listeners key IS present.
        assert!(entry.get("dynamic_listeners").is_some());
    }

    // (c, inverse) a static listener present + lds_config configured ⇒
    // static_listeners key present, carrying it (with the active_state-free
    // static shape: {"listener": {...}, "last_updated": ...}).
    #[test]
    fn static_listener_present_emits_static_listeners_key() {
        let mut bootstrap = parse_bootstrap(CDS_LDS_BOOTSTRAP);
        bootstrap.static_resources.listeners = vec![parse_listener(STATIC_LISTENER)];
        bootstrap.dynamic_listeners = Some(vec![parse_listener(DYNAMIC_LISTENER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        let entry = configs
            .iter()
            .find(|e| e.get("@type").and_then(|v| v.as_str()) == Some(LISTENERS_TYPE))
            .expect("ListenersConfigDump entry present");
        assert_eq!(
            entry
                .pointer("/static_listeners/0/listener/name")
                .and_then(|v| v.as_str()),
            Some("static_listener")
        );
        assert_eq!(
            entry
                .pointer("/static_listeners/0/listener/@type")
                .and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.config.listener.v3.Listener")
        );
    }

    // (d) §5.5 separation: the BootstrapConfigDump entry never shows the loaded
    // dynamic listeners (dynamic_listeners is #[serde(skip)] on Bootstrap).
    #[test]
    fn bootstrap_entry_never_shows_dynamic_listeners() {
        let mut bootstrap = parse_bootstrap(CDS_LDS_BOOTSTRAP);
        bootstrap.dynamic_listeners = Some(vec![parse_listener(DYNAMIC_LISTENER)]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let boot = &value.get("configs").and_then(|c| c.as_array()).unwrap()[0];
        // static_resources.listeners stays empty (the loaded dynamic_listeners
        // are #[serde(skip)] — structurally excluded from the BootstrapConfigDump).
        let static_listeners = boot
            .pointer("/bootstrap/static_resources/listeners")
            .and_then(|v| v.as_array())
            .expect("static_resources.listeners is an array");
        assert!(
            static_listeners.is_empty(),
            "loaded dynamic listeners must NOT appear in the BootstrapConfigDump"
        );
        // And there is no `dynamic_listeners` key anywhere in the bootstrap subtree.
        assert!(
            boot.pointer("/bootstrap/dynamic_listeners").is_none(),
            "dynamic_listeners is #[serde(skip)] — must be absent"
        );
    }
}

#[cfg(test)]
mod routes_config_dump_tests {
    //! Phase 20 Task 5 — D5 (ADR-0051/ADR-0052 §6.2 L5): the `RoutesConfigDump`
    //! `/config_dump` entry, emitted CONDITIONALLY (only when some HCM carries
    //! `rds: Some(...)`). Test groups: (a) conditional emission — an rds HCM with
    //! `route_config` populated renders a RoutesConfigDump entry; (b) inertness —
    //! an inline-route HCM (no rds) renders NO RoutesConfigDump entry; (c)
    //! ordering — on a cds+rds (no lds) bootstrap the Routes entry lands at
    //! `configs[2]`; on a cds+lds+rds bootstrap it lands at `configs[3]`.

    use super::AdminEndpoint;
    use crate::config::AdminConfig;
    use crate::handler::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::{
        Address, Admin, Bootstrap, CodecType, FilterChain, HttpConnectionManagerConfig, Listener,
        NetworkFilter, Rds, RouteConfiguration, SocketAddress, TypedConfig, VirtualHost,
    };
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    /// Build a handler from an already-constructed `Bootstrap`.
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
            Vec::new(),
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

    const ROUTES_TYPE: &str = "type.googleapis.com/envoy.admin.v3.RoutesConfigDump";

    fn has_routes_entry(value: &serde_json::Value) -> bool {
        value
            .get("configs")
            .and_then(|c| c.as_array())
            .unwrap()
            .iter()
            .any(|e| e.get("@type").and_then(|v| v.as_str()) == Some(ROUTES_TYPE))
    }

    /// Bootstrap WITH `dynamic_resources.cds_config` but NO `lds_config`.
    /// Used for the ordering test: Bootstrap[0] + Clusters[1] + Routes[2].
    const CDS_ONLY_BOOTSTRAP: &str = "\
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

    /// Bootstrap WITH BOTH `cds_config` AND `lds_config`.
    /// Used for the cds+lds+rds ordering test: Bootstrap[0] + Clusters[1] +
    /// Listeners[2] + Routes[3].
    const CDS_LDS_BOOTSTRAP: &str = "\
node:
  id: t
  cluster: c
dynamic_resources:
  cds_config:
    path_config_source:
      path: /etc/cds.yaml
  lds_config:
    path_config_source:
      path: /etc/lds.yaml
static_resources:
  listeners: []
  clusters: []
";

    /// Build a `Listener` whose single filter chain has an HCM with
    /// `rds: Some(...)` AND `route_config: Some(...)` (the post-load state).
    fn rds_listener(route_name: &str) -> Listener {
        let route_config = RouteConfiguration {
            name: route_name.to_string(),
            virtual_hosts: vec![VirtualHost {
                name: "local".to_string(),
                domains: vec!["*".to_string()],
                routes: vec![],
                include_attempt_count_in_response: false,
            }],
            validate_clusters: None,
        };
        let hcm = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: CodecType::AUTO,
            http2_protocol_options: None,
            route_config: Some(route_config),
            rds: Some(Rds {
                route_config_name: route_name.to_string(),
                config_source: envoy_config::ConfigSource {
                    path_config_source: envoy_config::PathConfigSource {
                        path: "/etc/rds.yaml".to_string(),
                    },
                    resource_api_version: None,
                },
            }),
            http_filters: vec![],
            access_log: vec![],
        };
        Listener {
            name: "rds_listener".to_string(),
            address: Address {
                socket_address: SocketAddress {
                    address: "0.0.0.0".to_string(),
                    port_value: 10000,
                },
            },
            filter_chains: vec![FilterChain {
                filters: vec![NetworkFilter {
                    name: "envoy.filters.network.http_connection_manager".to_string(),
                    typed_config: Some(TypedConfig::HttpConnectionManager(hcm)),
                }],
                filter_chain_match: None,
                transport_socket: None,
            }],
            listener_filters: vec![],
        }
    }

    /// Build a `Listener` whose HCM has NO `rds` (inline route_config — the
    /// fixture-0014/0026/0027 shape).
    fn inline_route_listener() -> Listener {
        let route_config = RouteConfiguration {
            name: "inline_route".to_string(),
            virtual_hosts: vec![],
            validate_clusters: None,
        };
        let hcm = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: CodecType::AUTO,
            http2_protocol_options: None,
            route_config: Some(route_config),
            rds: None,
            http_filters: vec![],
            access_log: vec![],
        };
        Listener {
            name: "inline_listener".to_string(),
            address: Address {
                socket_address: SocketAddress {
                    address: "0.0.0.0".to_string(),
                    port_value: 10000,
                },
            },
            filter_chains: vec![FilterChain {
                filters: vec![NetworkFilter {
                    name: "envoy.filters.network.http_connection_manager".to_string(),
                    typed_config: Some(TypedConfig::HttpConnectionManager(hcm)),
                }],
                filter_chain_match: None,
                transport_socket: None,
            }],
            listener_filters: vec![],
        }
    }

    // (a) conditional emission: an rds HCM with populated route_config ⇒ a
    // RoutesConfigDump entry appears, with the correct route name and no
    // version_info key.
    #[test]
    fn rds_hcm_emits_routes_config_dump() {
        let mut bootstrap = parse_bootstrap(CDS_ONLY_BOOTSTRAP);
        bootstrap.static_resources.listeners = vec![rds_listener("local_route")];
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        assert!(
            has_routes_entry(&value),
            "rds HCM ⇒ RoutesConfigDump entry present; got {value}"
        );
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        let entry = configs
            .iter()
            .find(|e| e.get("@type").and_then(|v| v.as_str()) == Some(ROUTES_TYPE))
            .expect("RoutesConfigDump entry present");
        // The route_config name must match.
        assert_eq!(
            entry
                .pointer("/dynamic_route_configs/0/route_config/name")
                .and_then(|v| v.as_str()),
            Some("local_route"),
            "route_config.name == local_route; entry was {entry}"
        );
        // The inner @type tag must be present (L5 shape).
        assert_eq!(
            entry
                .pointer("/dynamic_route_configs/0/route_config/@type")
                .and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.config.route.v3.RouteConfiguration"),
            "route_config.@type correct; entry was {entry}"
        );
        // last_updated is an ISO-8601 string.
        let last_updated = entry
            .pointer("/dynamic_route_configs/0/last_updated")
            .and_then(|v| v.as_str())
            .expect("last_updated is a string");
        assert!(
            last_updated.contains('T') && last_updated.ends_with('Z'),
            "last_updated ISO-8601-shaped; got {last_updated:?}"
        );
        // L5 ✧: NO version_info key in the dynamic_route_configs entry.
        assert!(
            entry
                .pointer("/dynamic_route_configs/0/version_info")
                .is_none(),
            "file-based RDS must carry NO version_info key; entry was {entry}"
        );
    }

    // (b) inertness: inline-route HCMs (no rds) ⇒ NO RoutesConfigDump entry.
    // Fixtures 0014/0026/0027 use inline routes — this test guards their shape.
    #[test]
    fn inline_route_hcm_emits_no_routes_config_dump() {
        let mut bootstrap = parse_bootstrap(CDS_ONLY_BOOTSTRAP);
        bootstrap.static_resources.listeners = vec![inline_route_listener()];
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        assert!(
            !has_routes_entry(&value),
            "inline-route HCM (no rds) ⇒ NO RoutesConfigDump entry; got {value}"
        );
    }

    // (b) inertness: a plain bootstrap with NO dynamic_resources and NO rds ⇒
    // exactly ONE entry (the Bootstrap entry only).
    #[test]
    fn plain_bootstrap_emits_no_routes_config_dump() {
        let bootstrap = parse_bootstrap(
            "node:\n  id: t\n  cluster: c\nstatic_resources:\n  listeners: []\n  clusters: []\n",
        );
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        assert!(
            !has_routes_entry(&value),
            "plain bootstrap ⇒ no RoutesConfigDump entry; got {value}"
        );
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(configs.len(), 1, "plain bootstrap ⇒ single entry");
    }

    // (c) ordering: cds + rds (no lds) ⇒ Bootstrap[0] + Clusters[1] + Routes[2].
    #[test]
    fn cds_rds_bootstrap_routes_at_configs_2() {
        let mut bootstrap = parse_bootstrap(CDS_ONLY_BOOTSTRAP);
        bootstrap.static_resources.listeners = vec![rds_listener("local_route")];
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(
            configs.len(),
            3,
            "cds+rds (no lds) ⇒ three entries; got {value}"
        );
        assert_eq!(
            configs[0].get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.BootstrapConfigDump"),
            "configs[0] must be Bootstrap"
        );
        assert_eq!(
            configs[1].get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.ClustersConfigDump"),
            "configs[1] must be Clusters"
        );
        assert_eq!(
            configs[2].get("@type").and_then(|v| v.as_str()),
            Some(ROUTES_TYPE),
            "configs[2] must be Routes (no lds ⇒ no Listeners in between)"
        );
    }

    // (c) ordering: cds + lds + rds ⇒ Bootstrap[0] + Clusters[1] + Listeners[2]
    // + Routes[3].
    #[test]
    fn cds_lds_rds_bootstrap_routes_at_configs_3() {
        let mut bootstrap = parse_bootstrap(CDS_LDS_BOOTSTRAP);
        bootstrap.dynamic_listeners = Some(vec![rds_listener("local_route")]);
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        assert_eq!(configs.len(), 4, "cds+lds+rds ⇒ four entries; got {value}");
        assert_eq!(
            configs[1].get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.ClustersConfigDump"),
            "configs[1] must be Clusters"
        );
        assert_eq!(
            configs[2].get("@type").and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.admin.v3.ListenersConfigDump"),
            "configs[2] must be Listeners"
        );
        assert_eq!(
            configs[3].get("@type").and_then(|v| v.as_str()),
            Some(ROUTES_TYPE),
            "configs[3] must be Routes (after Listeners)"
        );
    }

    // 26 Task 6: a `RouteConfiguration` with a recognizable vhost name/domain
    // marker so initial-vs-reloaded tables are distinguishable in the dump.
    fn marker_route_config(route_name: &str, marker: &str) -> RouteConfiguration {
        RouteConfiguration {
            name: route_name.to_string(),
            virtual_hosts: vec![VirtualHost {
                name: marker.to_string(),
                domains: vec![format!("{marker}.example")],
                // Empty routes ⇒ NO cluster references ⇒ validation passes
                // against the empty ClusterManager in HCMConfig::from_config.
                routes: vec![],
                include_attempt_count_in_response: false,
            }],
            validate_clusters: None,
        }
    }

    // 26 Task 6: build a Listener whose single rds HCM carries the given initial
    // route_config (the bootstrap snapshot), so the renderer's bootstrap walk
    // finds the rds HCM and looks up the live handle by route_config_name.
    fn rds_listener_with_route(route_name: &str, route_config: RouteConfiguration) -> Listener {
        let hcm = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: CodecType::AUTO,
            http2_protocol_options: None,
            route_config: Some(route_config),
            rds: Some(Rds {
                route_config_name: route_name.to_string(),
                config_source: envoy_config::ConfigSource {
                    path_config_source: envoy_config::PathConfigSource {
                        path: "/etc/rds.yaml".to_string(),
                    },
                    resource_api_version: None,
                },
            }),
            http_filters: vec![],
            access_log: vec![],
        };
        Listener {
            name: "rds_listener".to_string(),
            address: Address {
                socket_address: SocketAddress {
                    address: "0.0.0.0".to_string(),
                    port_value: 10000,
                },
            },
            filter_chains: vec![FilterChain {
                filters: vec![NetworkFilter {
                    name: "envoy.filters.network.http_connection_manager".to_string(),
                    typed_config: Some(TypedConfig::HttpConnectionManager(hcm)),
                }],
                filter_chain_match: None,
                transport_socket: None,
            }],
            listener_filters: vec![],
        }
    }

    // 26 Task 6 (TDD core): `/config_dump`'s RoutesConfigDump must render the
    // LIVE, hot-reloaded route table read through the swappable
    // `HCMConfig::current_route_config()` handle — NOT the startup bootstrap
    // snapshot. After an RDS reload calls `store_route_config`, a subsequent
    // `/config_dump` must reflect the NEW table. (Before this task the renderer
    // read the bootstrap copy, so the post-swap assertion FAILED.)
    #[tokio::test]
    async fn config_dump_reflects_hot_reloaded_route_table() {
        // 1. Build the rds HCM config with an INITIAL distinguishing table.
        let initial = marker_route_config("local_route", "vh_initial");
        let hcm_cfg = HttpConnectionManagerConfig {
            stat_prefix: "ingress_http".to_string(),
            codec_type: CodecType::AUTO,
            http2_protocol_options: None,
            route_config: Some(marker_route_config("local_route", "vh_initial")),
            rds: Some(Rds {
                route_config_name: "local_route".to_string(),
                config_source: envoy_config::ConfigSource {
                    path_config_source: envoy_config::PathConfigSource {
                        path: "/etc/rds.yaml".to_string(),
                    },
                    resource_api_version: None,
                },
            }),
            // A terminal Router filter is required for a valid HCM chain.
            http_filters: vec![envoy_config::HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: envoy_config::HttpFilterTypedConfig::Router(
                    envoy_config::RouterConfig {},
                ),
            }],
            access_log: vec![],
        };

        // 2. Construct the live, swappable HCMConfig (validation against an
        //    empty ClusterManager passes because the table has empty routes).
        let registry = Arc::new(StatsRegistry::new());
        let hcm_config: Arc<envoy_http1::HCMConfig> = Arc::new(
            envoy_http1::HCMConfig::from_config(
                &hcm_cfg,
                Arc::new(ClusterManager::empty()),
                registry,
                None,
            )
            .await
            .expect("HCMConfig::from_config"),
        );

        // 3. Build a Bootstrap whose listener carries the SAME rds HCM (so the
        //    renderer's bootstrap walk finds it) and an AdminHandler wired with
        //    the live route-table source keyed by route_config_name.
        let mut bootstrap = parse_bootstrap(CDS_ONLY_BOOTSTRAP);
        bootstrap.static_resources.listeners =
            vec![rds_listener_with_route("local_route", initial)];

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
        let handler_registry = Arc::new(StatsRegistry::new());
        let drain = Arc::new(envoy_listener::DrainState::new(&handler_registry));
        let handler = AdminHandler::new(
            cfg,
            handler_registry,
            Arc::new(bootstrap),
            Arc::new(ClusterManager::empty()),
            Instant::now(),
            BTreeMap::new(),
            drain,
            vec![("local_route".to_string(), Arc::clone(&hcm_config))],
        );

        // 4. Render — must reflect the INITIAL table (marker `vh_initial`).
        let value = dump_value(&handler);
        let configs = value.get("configs").and_then(|c| c.as_array()).unwrap();
        let entry = configs
            .iter()
            .find(|e| e.get("@type").and_then(|v| v.as_str()) == Some(ROUTES_TYPE))
            .expect("RoutesConfigDump entry present");
        assert_eq!(
            entry
                .pointer("/dynamic_route_configs/0/route_config/virtual_hosts/0/name")
                .and_then(|v| v.as_str()),
            Some("vh_initial"),
            "initial render must reflect the live handle's initial table; entry was {entry}"
        );

        // 5. Swap the live table (simulates an RDS reload).
        hcm_config.store_route_config(Arc::new(marker_route_config("local_route", "vh_reloaded")));

        // 6. Render AGAIN — must reflect the RELOADED table (marker
        //    `vh_reloaded`). This is the assertion that FAILS before this task
        //    (the renderer read the frozen bootstrap copy) and PASSES after.
        let value2 = dump_value(&handler);
        let configs2 = value2.get("configs").and_then(|c| c.as_array()).unwrap();
        let entry2 = configs2
            .iter()
            .find(|e| e.get("@type").and_then(|v| v.as_str()) == Some(ROUTES_TYPE))
            .expect("RoutesConfigDump entry present after reload");
        assert_eq!(
            entry2
                .pointer("/dynamic_route_configs/0/route_config/virtual_hosts/0/name")
                .and_then(|v| v.as_str()),
            Some("vh_reloaded"),
            "post-reload render must reflect the HOT-RELOADED table, not the \
             bootstrap snapshot; entry was {entry2}"
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

    // 19 D3: `/listeners` iterates `all_listeners()` (static + dynamic), so a
    // bootstrap carrying `dynamic_listeners: Some(vec![dyn_l])` surfaces the
    // dynamic listener in the body.
    #[test]
    fn listeners_body_includes_dynamic_listeners() {
        use crate::config::AdminConfig;
        use crate::handler::AdminHandler;
        use envoy_cluster::ClusterManager;
        use envoy_config::{Address, Admin, Bootstrap, SocketAddress};
        use envoy_stats::StatsRegistry;
        use std::collections::BTreeMap;
        use std::sync::Arc;
        use std::time::Instant;

        // A bootstrap with ZERO static listeners; the LDS-supplied listener
        // `dyn_l` lives only in `dynamic_listeners`.
        let mut bootstrap: Bootstrap = serde_yaml::from_str(TINY_BOOTSTRAP).expect("yaml parses");
        let dyn_l: envoy_config::Listener = serde_yaml::from_str(
            "name: dyn_l
address:
  socket_address:
    address: 127.0.0.1
    port_value: 7777
filter_chains:
- filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      \"@type\": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: d
      cluster: c
",
        )
        .expect("dynamic listener parses");
        bootstrap.dynamic_listeners = Some(vec![dyn_l]);

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
        let handler = AdminHandler::new(
            cfg,
            registry,
            Arc::new(bootstrap),
            Arc::new(ClusterManager::empty()),
            Instant::now(),
            BTreeMap::new(),
            drain,
            Vec::new(),
        );

        let resp = AdminEndpoint::Listeners.render_with(&handler);
        let body = std::str::from_utf8(&resp.body).unwrap();
        assert!(
            body.contains("dyn_l::127.0.0.1:7777\n"),
            "dynamic listener must appear in /listeners body; body was: {body:?}"
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
            Vec::new(),
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

#[cfg(test)]
mod endpoints_config_dump_tests {
    //! Phase 21 Task 5 — D5 (ADR-0053/0054; §6.2 L5): the `EndpointsConfigDump`
    //! `/config_dump` entry (`static_endpoint_configs`, file-based EDS is
    //! "static" config-dump-wise), emitted CONDITIONALLY (only when some cluster
    //! is `type: EDS` with a populated `load_assignment`) and pushed AFTER the
    //! `ClustersConfigDump` entry / BEFORE the `ListenersConfigDump` entry —
    //! Envoy's `?include_eds` order is Clusters[1]/Endpoints[2]/Listeners[3].
    //! Plus the admin query-string strip so `/config_dump?include_eds` routes to
    //! `ConfigDump` (Envoy's admin does the same). Test groups: (a) query-strip;
    //! (b) conditional emission with an EDS cluster present; (c) inertness — only
    //! STATIC/STRICT_DNS clusters ⇒ NO EndpointsConfigDump entry; (d) ordering.

    use super::{AdminEndpoint, Dispatch};
    use crate::config::AdminConfig;
    use crate::handler::AdminHandler;
    use envoy_cluster::ClusterManager;
    use envoy_config::{Address, Admin, Bootstrap, SocketAddress};
    use envoy_stats::StatsRegistry;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Instant;

    /// An EDS cluster named `eds_backend`. The `load_assignment` is the
    /// resolved CLA that `load_dynamic_resources` would populate from the EDS
    /// file (here the test parses it inline — render-time only reads it). The
    /// CLA `cluster_name` is `eds_backend` (service_name unset ⇒ equals the
    /// cluster name — L8). The endpoint `address` is a NUMERIC IP (L1).
    const EDS_CLUSTER: &str = "\
name: eds_backend
type: EDS
lb_policy: ROUND_ROBIN
eds_cluster_config:
  eds_config:
    path_config_source:
      path: /etc/eds.yaml
load_assignment:
  cluster_name: eds_backend
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address:
            address: 127.0.0.1
            port_value: 8080
";

    /// A STATIC cluster named `static_backend` (no EDS) — used by the inertness
    /// test (only STATIC/STRICT_DNS ⇒ no EndpointsConfigDump entry).
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

    /// A STRICT_DNS cluster named `dns_backend` (no EDS) — second inertness arm.
    const DNS_BACKEND_CLUSTER: &str = "\
name: dns_backend
type: STRICT_DNS
lb_policy: ROUND_ROBIN
load_assignment:
  cluster_name: dns_backend
  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address:
            address: backend.example.com
            port_value: 8080
";

    /// Bootstrap WITH `cds_config` configured (so the ClustersConfigDump entry
    /// lands at configs[1] and the EndpointsConfigDump entry must order AFTER it
    /// at configs[2]).
    const CDS_BOOTSTRAP: &str = "\
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

    /// Bootstrap with NO `dynamic_resources` (so the EndpointsConfigDump entry
    /// lands at configs[1], directly after the Bootstrap entry).
    const PLAIN_BOOTSTRAP: &str = "\
node:
  id: t
  cluster: c
static_resources:
  listeners: []
  clusters: []
";

    fn parse_cluster(yaml: &str) -> envoy_config::Cluster {
        serde_yaml::from_str(yaml).expect("cluster yaml parses")
    }

    fn parse_bootstrap(yaml: &str) -> Bootstrap {
        serde_yaml::from_str(yaml).expect("bootstrap yaml parses")
    }

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
            Vec::new(),
        )
    }

    fn dump_value(handler: &AdminHandler) -> serde_json::Value {
        let resp = AdminEndpoint::ConfigDump.render_with(handler);
        let body_str = std::str::from_utf8(&resp.body).expect("utf-8");
        serde_json::from_str(body_str).expect("valid JSON")
    }

    const ENDPOINTS_TYPE: &str = "type.googleapis.com/envoy.admin.v3.EndpointsConfigDump";

    fn endpoints_entry_index(value: &serde_json::Value) -> Option<usize> {
        value
            .get("configs")
            .and_then(|c| c.as_array())
            .unwrap()
            .iter()
            .position(|e| e.get("@type").and_then(|v| v.as_str()) == Some(ENDPOINTS_TYPE))
    }

    // (a) query-strip: /config_dump?include_eds routes to ConfigDump, not 404.
    #[test]
    fn query_string_strips_to_config_dump() {
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/config_dump?include_eds"),
            Dispatch::Endpoint(AdminEndpoint::ConfigDump)
        ));
        // The bare path still resolves (no regression).
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/config_dump"),
            Dispatch::Endpoint(AdminEndpoint::ConfigDump)
        ));
        // A genuinely-unknown path with a query string still 404s.
        assert!(matches!(
            AdminEndpoint::dispatch("GET", "/nope?x=1"),
            Dispatch::NotFound
        ));
    }

    // (b) conditional emission: an EDS cluster (load_assignment populated) ⇒
    // an EndpointsConfigDump entry carrying static_endpoint_configs[0].
    #[test]
    fn eds_cluster_emits_endpoints_config_dump() {
        let mut bootstrap = parse_bootstrap(PLAIN_BOOTSTRAP);
        bootstrap.static_resources.clusters = vec![parse_cluster(EDS_CLUSTER)];
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        let idx = endpoints_entry_index(&value).expect("EndpointsConfigDump entry present");
        let entry = &value.get("configs").and_then(|c| c.as_array()).unwrap()[idx];
        // outer @type ends with EndpointsConfigDump.
        assert!(
            entry
                .get("@type")
                .and_then(|v| v.as_str())
                .unwrap()
                .ends_with("EndpointsConfigDump")
        );
        // static_endpoint_configs[0].endpoint_config carries the nested @type +
        // the resolved CLA cluster_name.
        assert_eq!(
            entry
                .pointer("/static_endpoint_configs/0/endpoint_config/@type")
                .and_then(|v| v.as_str()),
            Some("type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment")
        );
        assert_eq!(
            entry
                .pointer("/static_endpoint_configs/0/endpoint_config/cluster_name")
                .and_then(|v| v.as_str()),
            Some("eds_backend")
        );
        // The resolved endpoints are carried through.
        assert_eq!(
            entry
                .pointer("/static_endpoint_configs/0/endpoint_config/endpoints/0/lb_endpoints/0/endpoint/address/socket_address/address")
                .and_then(|v| v.as_str()),
            Some("127.0.0.1")
        );
        // No version_info / last_updated on the EndpointsConfigDump (L5).
        assert!(entry.get("version_info").is_none());
        assert!(
            entry
                .pointer("/static_endpoint_configs/0/last_updated")
                .is_none()
        );
    }

    // (c) inertness: only STATIC/STRICT_DNS clusters ⇒ NO EndpointsConfigDump.
    #[test]
    fn non_eds_clusters_emit_no_endpoints_config_dump() {
        let mut bootstrap = parse_bootstrap(PLAIN_BOOTSTRAP);
        bootstrap.static_resources.clusters = vec![
            parse_cluster(STATIC_BACKEND_CLUSTER),
            parse_cluster(DNS_BACKEND_CLUSTER),
        ];
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        assert!(
            endpoints_entry_index(&value).is_none(),
            "no EDS cluster ⇒ no EndpointsConfigDump entry; value was {value}"
        );
        // The plain bootstrap with no EDS cluster is a single Bootstrap entry.
        assert_eq!(
            value
                .get("configs")
                .and_then(|c| c.as_array())
                .unwrap()
                .len(),
            1
        );
    }

    // (d) ordering: with cds_config + an EDS cluster ⇒ Endpoints at configs[2]
    // (Bootstrap[0], Clusters[1], Endpoints[2]); with NO cds ⇒ configs[1].
    #[test]
    fn endpoints_entry_orders_after_clusters() {
        // cds present ⇒ Endpoints at configs[2].
        let mut bootstrap = parse_bootstrap(CDS_BOOTSTRAP);
        bootstrap.static_resources.clusters = vec![parse_cluster(EDS_CLUSTER)];
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        assert_eq!(
            endpoints_entry_index(&value),
            Some(2),
            "with cds_config the EndpointsConfigDump lands at configs[2]; value was {value}"
        );

        // no cds ⇒ Endpoints at configs[1].
        let mut bootstrap = parse_bootstrap(PLAIN_BOOTSTRAP);
        bootstrap.static_resources.clusters = vec![parse_cluster(EDS_CLUSTER)];
        let handler = handler_from_bootstrap(bootstrap);
        let value = dump_value(&handler);
        assert_eq!(
            endpoints_entry_index(&value),
            Some(1),
            "with no cds_config the EndpointsConfigDump lands at configs[1]; value was {value}"
        );
    }
}
