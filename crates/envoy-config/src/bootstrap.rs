//! Bootstrap schema — the phase-01 `envoy.yaml` surface. See SPEC §D1 and
//! ADR-0008. All structs derive `Debug` + `Deserialize` and carry
//! `#[serde(deny_unknown_fields)]` except `Node`, which is deliberately open
//! (SPEC §D1 inline comment).

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    #[serde(default)]
    pub node: Option<Node>,
    #[serde(default)]
    pub admin: Option<Admin>,
    #[serde(default)]
    pub static_resources: StaticResources,
    /// 18 D1: file-based CDS (the xDS-family opener; ADR-0048/ADR-0049).
    /// Only `cds_config.path_config_source` is supported; everything else
    /// in the upstream DynamicResources proto is rejected loudly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_resources: Option<DynamicResources>,
    /// 18 D3: clusters loaded from the CDS file by `load_dynamic_resources`.
    /// `None` = not loaded yet (parse_bootstrap leaves it None — the fuzz
    /// target does no I/O); `Some(vec)` = loaded (possibly empty).
    /// NOT serialized: the BootstrapConfigDump must show the bootstrap as
    /// parsed from disk (SPEC §5.5 config_dump separation); dynamic clusters
    /// surface in the ClustersConfigDump entry instead.
    #[serde(skip)]
    pub dynamic_clusters: Option<Vec<Cluster>>,
    /// 19 D3: listeners loaded from the LDS file by `load_dynamic_resources`.
    /// `None` = not loaded yet (parse_bootstrap leaves it None — the fuzz
    /// target does no I/O); `Some(vec)` = loaded (possibly empty).
    /// NOT serialized: the BootstrapConfigDump must show the bootstrap as
    /// parsed from disk (SPEC §5.5 config_dump separation); dynamic listeners
    /// surface in the ListenersConfigDump entry instead.
    #[serde(skip)]
    pub dynamic_listeners: Option<Vec<Listener>>,
}

impl Bootstrap {
    /// 18 D3: the effective cluster list — static clusters followed by
    /// dynamically-loaded (CDS) clusters. Every downstream consumer
    /// (validators, ClusterManager, pools, health, TLS) iterates THIS,
    /// never `static_resources.clusters` directly (SPEC §5.3: dynamic
    /// clusters are full Clusters, indistinguishable downstream).
    pub fn all_clusters(&self) -> impl Iterator<Item = &Cluster> {
        self.static_resources
            .clusters
            .iter()
            .chain(self.dynamic_clusters.iter().flatten())
    }

    /// 18 D1/D3: true iff a CDS config source is configured but
    /// `load_dynamic_resources` has not run yet. While true, cluster-reference
    /// validation DEFERS (the references may resolve against the CDS file);
    /// `load_dynamic_resources` re-validates with full enforcement.
    pub(crate) fn cds_configured_but_unloaded(&self) -> bool {
        self.dynamic_resources
            .as_ref()
            .and_then(|dr| dr.cds_config.as_ref())
            .is_some()
            && self.dynamic_clusters.is_none()
    }

    /// 19 D3: the effective listener list — static listeners followed by
    /// dynamically-loaded (LDS) listeners. Every consumer that previously
    /// iterated `static_resources.listeners` goes through THIS instead
    /// (dynamic listeners are full Listeners, indistinguishable downstream).
    pub fn all_listeners(&self) -> impl Iterator<Item = &Listener> {
        self.static_resources
            .listeners
            .iter()
            .chain(self.dynamic_listeners.iter().flatten())
    }

    /// 19 D1: true iff `lds_config` is configured but `load_dynamic_resources`
    /// has not yet populated `dynamic_listeners` (the NoRuntime-gate deferral
    /// predicate; mirrors `cds_configured_but_unloaded`).
    pub(crate) fn lds_configured_but_unloaded(&self) -> bool {
        self.dynamic_resources
            .as_ref()
            .and_then(|dr| dr.lds_config.as_ref())
            .is_some()
            && self.dynamic_listeners.is_none()
    }
}

/// 18 D1: `dynamic_resources` — the CDS and LDS filesystem transports at this
/// phase (ADR-0048, ADR-0050). `ads_config` / `api_config_source` /
/// `watched_directory` are deliberately NOT fields: deny_unknown_fields rejects
/// them loudly (SPEC §4 deferral ledger).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DynamicResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cds_config: Option<ConfigSource>,
    /// 19 D1 (ADR-0050): file-based LDS. Reuses ConfigSource/PathConfigSource
    /// verbatim (resource-type-agnostic). ads_config / api_config_source /
    /// watched_directory remain rejected by deny_unknown_fields (deferred, SPEC §4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lds_config: Option<ConfigSource>,
}

/// 18 D1: a ConfigSource restricted to the filesystem transport.
/// `api_config_source` (gRPC/REST) / `ads` are NOT fields (rejected; deferred
/// to the gRPC-xDS phase, which also supersedes ADR-0014 per ADR-0048).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConfigSource {
    pub path_config_source: PathConfigSource,
    /// L8: optional; Envoy defaults it. Accept "V3" or absent; reject others
    /// (validate(), UnsupportedResourceApiVersion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_api_version: Option<String>,
}

/// 18 D1: `path_config_source` — the file path. `watched_directory` is NOT a
/// field (rejected; deferred with file watching per SPEC §4).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PathConfigSource {
    pub path: String,
}

/// 21 D1 (ADR-0053/0054): `eds_cluster_config` — an EDS cluster's endpoint
/// source. `eds_config` reuses the phase-18 `ConfigSource`/`PathConfigSource`
/// verbatim (filesystem transport only; `api_config_source`/`ads`/
/// `watched_directory` stay rejected by `ConfigSource`'s own
/// `deny_unknown_fields`).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EdsClusterConfig {
    pub eds_config: ConfigSource,
    /// Selects the `ClusterLoadAssignment` by name; defaults to the cluster
    /// name (L8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
}

/// 20 D1 (ADR-0051/0052): RDS — a route table loaded from a file. `config_source`
/// reuses the phase-18 `ConfigSource`/`PathConfigSource` verbatim (filesystem
/// transport only; `api_config_source`/`ads`/`watched_directory` stay rejected by
/// `ConfigSource`'s own `deny_unknown_fields`). `resource_api_version` is optional
/// INSIDE `config_source` (L1) — it is NOT a field of `Rds`.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rds {
    pub route_config_name: String,
    pub config_source: ConfigSource,
}

// NOTE: Node deliberately omits `deny_unknown_fields`. Upstream Envoy's Node
// also carries metadata, locality, user_agent_*, extensions, client_features,
// listening_addresses, dynamic_parameters. Phase 01 accepts id + cluster and
// silently ignores the rest. Phase 18 (file-based CDS, ADR-0048/ADR-0049)
// consumed the xDS reservation by adding `Bootstrap.dynamic_resources`; Node
// itself remains open — Envoy requires `node.id` + `node.cluster` when CDS is
// configured, but phase 18 parses without enforcing this. The gRPC-xDS phase
// tightens or moves Node under a future ADR. (See SPEC §6 signpost 8.)
#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub cluster: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admin {
    pub address: Address,

    /// 06.1 NEW (per ADR-0026 parse-and-ignore pattern; SPEC §3 D5.a).
    /// Optional admin-side access log path; envoy-rust does not inspect or
    /// honor this field. Stored so fixtures with upstream Envoy admin
    /// configs that include it round-trip cleanly through the parser.
    /// Admin-side access logging defers indefinitely from 06.1.
    #[serde(default)]
    pub access_log_path: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticResources {
    #[serde(default)]
    pub listeners: Vec<Listener>,
    #[serde(default)]
    pub clusters: Vec<Cluster>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cluster {
    pub name: String,
    #[serde(rename = "type")]
    pub cluster_type: ClusterType,
    pub lb_policy: LbPolicy,
    /// 21 D1 (ADR-0053/0054): the inline endpoint list. EXACTLY ONE of
    /// `load_assignment` (inline; STATIC/STRICT_DNS) or `eds_cluster_config`
    /// (file; EDS) per cluster, consistent with `cluster_type` (enforced at
    /// validation — §5.8). After `load_dynamic_resources` populates an EDS
    /// cluster's `load_assignment` from its file, it is `Some` (§5.3);
    /// downstream endpoint construction reads it uniformly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_assignment: Option<LoadAssignment>,
    /// 21 D1: EDS — endpoints loaded from a file (reuses `ConfigSource`
    /// verbatim). EXACTLY ONE of this or `load_assignment` per cluster (§5.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eds_cluster_config: Option<EdsClusterConfig>,
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    /// 05.4 NEW per ADR-0024: optional DNS lookup family override for
    /// STRICT_DNS / LOGICAL_DNS clusters. Defaults to None, which lets
    /// the upstream Envoy honor its proto default (AUTO). envoy-rust does
    /// NOT consume this field at runtime in 05.4; only the upstream Envoy
    /// side observes the V4_ONLY knob via per-fixture envoy.yaml edits
    /// (D2 of phase 05.4 — see SPEC §3 D2).
    #[serde(default)]
    pub dns_lookup_family: Option<DnsLookupFamily>,
    /// 05.3 NEW per SPEC §3 D2.a: cluster-side typed_extension_protocol_options
    /// carrying the upstreams.http.v3.HttpProtocolOptions extension. Defaults
    /// to None, which projects to UpstreamProtocol::Http1 at envoy-cluster
    /// from_bootstrap time (envoy-cluster Task 5) — backwards-compat with all
    /// phase-04 clusters. The validator enforces:
    ///   - @type URL literal "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions"
    ///   - mutual exclusion of explicit_http_config arms
    ///   - RFC 7540 range checks on http2_protocol_options (delegated to
    ///     validate_http2_protocol_options_ranges; same checks as listener-side).
    #[serde(default)]
    pub typed_extension_protocol_options: Option<TypedExtensionProtocolOptions>,
    /// 12.1 (parent-12 D1): OPTIONAL active HTTP health checks. Phase-12 supports
    /// exactly 0 or 1, HTTP-only (validator-enforced). Empty ⇒ the cluster's
    /// endpoints are implicitly healthy and `pick()` is phase-02 round-robin
    /// (the §5.4 inert-when-unconfigured invariant).
    #[serde(default)]
    pub health_checks: Vec<HealthCheck>,
    /// 12.1 (parent-12 D1): OPTIONAL common LB config; phase-12 consumes only
    /// `healthy_panic_threshold`.
    #[serde(default)]
    pub common_lb_config: Option<CommonLbConfig>,
    /// 13.1 D1 (parent-13 D1): per-cluster circuit-breaker configuration.
    /// `None` means defaults (the §5.4 default-enabled-pool reads
    /// `max_connections: 1024` per upstream Envoy v1.33). See `CircuitBreakers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circuit_breakers: Option<CircuitBreakers>,
    /// 62 D1 (ADR-0119): OPTIONAL upstream HTTP protocol options. Phase-62
    /// consumes only `common_http_protocol_options.idle_timeout` — the
    /// upstream keep-alive pool's per-connection idle timeout, previously the
    /// hard-coded `DEFAULT_IDLE_TIMEOUT` (60s) in `envoy-http1`'s `H1Pool`.
    /// `None` (or a `None` inner `idle_timeout`) preserves that 60s default
    /// byte-for-byte, so the regression suite is unchanged. See
    /// `CommonHttpProtocolOptions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub common_http_protocol_options: Option<CommonHttpProtocolOptions>,
    /// 14.1 D1 (parent-14 D1): per-cluster outlier-detection configuration.
    /// `None` (the §5.3 inert-when-unconfigured invariant — preserves 21-fixture
    /// regression-equivalence).
    #[serde(default)]
    pub outlier_detection: Option<OutlierDetection>,
    /// 28 D1 (ADR-0069/0070): OPTIONAL RING_HASH tuning. `None` when the key is
    /// absent. Per upstream Envoy, a present `ring_hash_lb_config` on a
    /// non-RING_HASH cluster is accepted-and-ignored (validation is gated to
    /// RING_HASH clusters — see `validate_cluster`). Parsed-and-stored only at
    /// phase-28 Task 3; the ring-build + pick logic lands in later tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ring_hash_lb_config: Option<RingHashLbConfig>,
    /// 29 D1 (ADR-0071/0072): OPTIONAL MAGLEV tuning. `None` when absent. A present
    /// `maglev_lb_config` on a non-MAGLEV cluster is accepted-and-ignored (validation
    /// gated to MAGLEV clusters — see `validate_cluster`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maglev_lb_config: Option<MaglevLbConfig>,
    /// 30 D1 (ADR-0073/0074): OPTIONAL subset-LB tuning. `None` when absent. Per
    /// §A divergence #1 / ADR-0074, NOTHING about this config is startup-fatal
    /// (Envoy boots for empty selectors / empty keys / uncovered default_subset /
    /// DEFAULT_SUBSET-without-default — consequences are request-time). No
    /// `validate_cluster` block; accept-all. See `LbSubsetConfig`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lb_subset_config: Option<LbSubsetConfig>,
}

/// 62 D1 (ADR-0119): upstream `common_http_protocol_options`, Envoy's
/// `core.v3.HttpProtocolOptions`. Phase-62 parses-and-stores only
/// `idle_timeout` — the upstream keep-alive connection idle timeout consumed
/// by `envoy-http1`'s `H1Pool` (per-cluster). Like every other duration in this
/// schema it is a free-form scalar parsed via `parse_duration` (`"<N>s"` /
/// `"<N>ms"` / `"<N>us"`) at pool-build time, NOT at deserialize. Absent (or
/// present-but-null) → the `H1Pool` keeps its `DEFAULT_IDLE_TIMEOUT` (60s), so
/// the default path is byte-identical and the regression suite is untouched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct CommonHttpProtocolOptions {
    /// Upstream per-connection idle timeout (Envoy Duration string, e.g.
    /// `"30s"`). `None` → `H1Pool` uses its 60s default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_timeout: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ClusterType {
    /// Static cluster type — endpoints' `address` fields are literal IPs
    /// (parsed via `SocketAddr::from_str` at cluster-build time in
    /// `envoy-cluster::from_bootstrap`).
    Static,
    /// STRICT_DNS cluster type — endpoints' `address` fields are DNS names
    /// (resolved via `tokio::net::lookup_host` at cluster-build time in
    /// `envoy-cluster::from_bootstrap`; the resolved `SocketAddr`s are
    /// cached for the cluster's lifetime, matching Envoy v1.33's STRICT_DNS
    /// semantics with default `dns_refresh_rate`). 05.1 NEW per ADR-0023;
    /// `LOGICAL_DNS` deferred to a later phase.
    StrictDns,
    /// 21 D1 (ADR-0053/0054): endpoints loaded from a ClusterLoadAssignment file
    /// via `eds_cluster_config` (initial-load-only). Endpoints are resolved
    /// numeric socket addresses (STATIC semantics — L1), populated by
    /// `load_dynamic_resources`. The `rename_all = "SCREAMING_SNAKE_CASE"` maps
    /// this variant to `EDS` (no per-variant rename needed).
    Eds,
}

/// DNS lookup family for STRICT_DNS / LOGICAL_DNS clusters. Mirrors Envoy
/// v1.33's `Cluster.DnsLookupFamily` proto enum (3 variants: V4_ONLY /
/// V6_ONLY / AUTO; v1.33 does not have V4_PREFERRED or ALL — those land
/// in later Envoy versions). 05.4 NEW per ADR-0024; parsed-and-stored
/// only — envoy-rust's `tokio::net::lookup_host` resolution path returns
/// the system-stack default and does NOT filter by family at runtime.
/// Whichever later phase needs the runtime filter lands it then.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum DnsLookupFamily {
    V4Only,
    V6Only,
    Auto,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum LbPolicy {
    RoundRobin,
    /// 28 D1 (ADR-0069/0070): consistent-hashing ring LB. The config surface
    /// (`ring_hash_lb_config`) lands in phase-28 Task 3; the ring-build + pick
    /// logic lands in later tasks.
    RingHash,
    /// 29 D1 (ADR-0071/0072): Maglev consistent-hashing LB. The table build +
    /// lookup land in later phase-29 tasks (maglev.rs).
    Maglev,
}

/// 29 D1 (ADR-0071/0072): MAGLEV LB tuning knobs. Mirrors Envoy v1.33's
/// `Cluster.MaglevLbConfig`. `table_size` default 65537 (Envoy proto default),
/// must be prime, max 5000011 (validated in `validate_cluster`, Task 3). A
/// present `maglev_lb_config` on a non-MAGLEV cluster is accepted-and-ignored
/// (validation gated to MAGLEV clusters — Envoy parity + the ring precedent).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MaglevLbConfig {
    #[serde(default = "default_maglev_table_size")]
    pub table_size: u64,
}

fn default_maglev_table_size() -> u64 {
    65537
}

/// 30 D1 (ADR-0073/0074): subset-LB fallback policy when a request's
/// metadata_match selects no subset. Mirrors Envoy v1.33's
/// `Cluster.LbSubsetConfig.LbSubsetFallbackPolicy` (the 3 cluster-level
/// variants). Defaults to `NO_FALLBACK` (Envoy proto default).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LbSubsetFallbackPolicy {
    #[default]
    NoFallback,
    AnyEndpoint,
    DefaultSubset,
}

/// 30 D1 (ADR-0073/0074): one subset selector — a set of metadata keys whose
/// distinct value-tuples across endpoints define the cluster's subsets. Mirrors
/// Envoy v1.33's `Cluster.LbSubsetConfig.LbSubsetSelector` (keys only; the
/// per-selector fallback fields are out of scope for phase-30). An empty
/// `keys: []` is accepted (NOT fatal — ADR-0074 §A divergence #1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LbSubsetSelector {
    #[serde(default)]
    pub keys: Vec<String>,
}

/// 30 D1 (ADR-0073/0074): MAGLEV-style optional subset-LB tuning. NO validator is
/// fatal (§6.2/ADR-0074: Envoy boots for empty selectors / empty keys / uncovered
/// default_subset / DEFAULT_SUBSET-without-default — consequences are request-time).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct LbSubsetConfig {
    #[serde(default)]
    pub fallback_policy: LbSubsetFallbackPolicy,
    #[serde(default)]
    pub subset_selectors: Vec<LbSubsetSelector>,
    /// Envoy `LbSubsetConfig.default_subset` is a `google.protobuf.Struct` — a FLAT
    /// key→value map (`{ stage: prod }`), NOT a `core.v3.Metadata` (ADR-0075). Scalar
    /// values are stringified (Envoy Struct values are `google.protobuf.Value`).
    #[serde(
        default,
        deserialize_with = "deserialize_opt_flat_struct",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_subset: Option<std::collections::BTreeMap<String, String>>,
}

/// 28 D1 (ADR-0069): RING_HASH LB tuning knobs. Mirrors Envoy v1.33's
/// `Cluster.RingHashLbConfig`. Sub-field serde defaults match Envoy's proto
/// defaults (minimum_ring_size 1024, maximum_ring_size 8_388_608,
/// hash_function XX_HASH), so a present-but-empty `ring_hash_lb_config: {}`
/// yields the documented defaults. Validation (see `validate_cluster`) is
/// gated to RING_HASH clusters and is all-fatal (ADR-0049).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RingHashLbConfig {
    #[serde(default = "default_minimum_ring_size")]
    pub minimum_ring_size: u64,
    #[serde(default = "default_maximum_ring_size")]
    pub maximum_ring_size: u64,
    #[serde(default)]
    pub hash_function: HashFunction,
}

fn default_minimum_ring_size() -> u64 {
    1024
}

fn default_maximum_ring_size() -> u64 {
    8_388_608
}

/// 28 D1 (ADR-0070): RING_HASH hash function. Both Envoy v1.33 wire values are
/// RECOGNIZED at parse time so the phase-28 XX_HASH-only narrowing surfaces as a
/// precise `ConfigError::UnsupportedHashFunction` (a documented divergence — see
/// ADR-0070) rather than an opaque serde unknown-variant error. `MURMUR_HASH_2`
/// is rejected in `validate_cluster`; a truly unknown value still fails at serde
/// parse. Default = `XxHash` (the Envoy proto default).
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum HashFunction {
    #[default]
    XxHash,
    /// Recognized (a real Envoy enum value) but REJECTED in validation — the
    /// phase-28 XX_HASH-only narrowing (ADR-0070). Explicit rename: serde's
    /// SCREAMING_SNAKE_CASE emits `MURMUR_HASH2` (no underscore before the
    /// trailing digit), but the Envoy wire name is `MURMUR_HASH_2`.
    #[serde(rename = "MURMUR_HASH_2")]
    MurmurHash2,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LoadAssignment {
    pub cluster_name: String,
    pub endpoints: Vec<LocalityLbEndpoints>,
}

/// Cluster-side typed_extension_protocol_options (Envoy's mechanism for
/// per-cluster protocol-extension config). 05.3 NEW per SPEC §3 D2.a.
/// The single recognized key is the upstreams.http.v3.HttpProtocolOptions
/// extension; the validator additionally rejects unknown @type URLs and
/// mutually-exclusive explicit_http_config arms.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TypedExtensionProtocolOptions {
    #[serde(rename = "envoy.extensions.upstreams.http.v3.HttpProtocolOptions")]
    pub http_protocol_options: HttpProtocolOptions,
}

/// The upstreams.http.v3.HttpProtocolOptions typed-extension. Carries the
/// `@type` URL (validated literal) + the `explicit_http_config` oneof.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpProtocolOptions {
    #[serde(rename = "@type")]
    pub type_url: String,
    pub explicit_http_config: ExplicitHttpConfig,
}

/// Envoy's `ExplicitHttpConfig` is a oneof: either http_protocol_options
/// (H1 arm; empty in 05.3 — see Http1ProtocolOptions) or
/// http2_protocol_options (H2 arm; reuses 05.2 D2.b's Http2ProtocolOptions
/// unchanged). The validator (`validate` fn) enforces mutual
/// exclusion via ConfigError::MutuallyExclusiveExplicitHttpConfig.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct ExplicitHttpConfig {
    #[serde(default)]
    pub http_protocol_options: Option<Http1ProtocolOptions>,
    #[serde(default)]
    pub http2_protocol_options: Option<Http2ProtocolOptions>,
}

/// H1 arm of ExplicitHttpConfig. Empty in 05.3; future fields like
/// chunk_encoding / allow_chunked_length / enable_trailers defer per
/// SPEC §4 to whichever phase first needs cluster-side H1 protocol-tuning.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Http1ProtocolOptions {}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalityLbEndpoints {
    pub lb_endpoints: Vec<LbEndpoint>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LbEndpoint {
    pub endpoint: Endpoint,
    /// 30 D1 (ADR-0073/0074): the endpoint's `envoy.lb` filter-metadata slice,
    /// used by subset LB. Absent ⇒ `None`. See `LbMetadata`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<LbMetadata>,
}

/// 30 D1 (ADR-0073/0074): the `envoy.lb` filter-metadata slice used by subset LB.
/// Mirrors Envoy `core.v3.Metadata.filter_metadata["envoy.lb"]` — a map of
/// string key → string value. Other filter_metadata namespaces are parsed and
/// ignored (they belong to other consumers). Ordered map for deterministic
/// subset-key tuples (BTreeMap).
/// NOTE: `Serialize` emits the internal flat shape (`envoy_lb`), NOT the Envoy
/// wire shape — `LbMetadata` is not round-trip symmetric (deserialize-only `from` shim).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(from = "MetadataWire")]
pub struct LbMetadata {
    /// the `envoy.lb` namespace map (empty when absent).
    pub envoy_lb: std::collections::BTreeMap<String, String>,
}

/// Private wire mirror of Envoy's `core.v3.Metadata`: `filter_metadata` is a map
/// of namespace → (map of key → value). `From<MetadataWire>` pulls the
/// `"envoy.lb"` namespace and stringifies each scalar value (§6.2: only strings
/// are observed in practice, but non-string scalars are coerced via a permissive
/// `to_string`; non-scalar values are skipped — parse-and-ignore, never error).
#[derive(Deserialize)]
struct MetadataWire {
    #[serde(default)]
    filter_metadata:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, serde_yaml::Value>>,
}

impl From<MetadataWire> for LbMetadata {
    fn from(wire: MetadataWire) -> Self {
        let envoy_lb = wire
            .filter_metadata
            .get("envoy.lb")
            .map(|ns| {
                ns.iter()
                    .filter_map(|(k, v)| stringify_scalar(v).map(|s| (k.clone(), s)))
                    .collect()
            })
            .unwrap_or_default();
        LbMetadata { envoy_lb }
    }
}

/// Coerce a YAML scalar to its string form for the `envoy.lb` map. Strings pass
/// through verbatim; other scalars (bool / number) stringify permissively;
/// non-scalar values (sequence / mapping / null / tagged) are dropped (None).
fn stringify_scalar(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// 30 (ADR-0075): deserialize `LbSubsetConfig.default_subset` — a flat
/// `google.protobuf.Struct` (`{ <key>: <value> }`). Scalar values are stringified
/// (reusing `stringify_scalar`); non-scalars are dropped (permissive, never errors).
fn deserialize_opt_flat_struct<'de, D>(
    deserializer: D,
) -> Result<Option<std::collections::BTreeMap<String, String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let opt: Option<std::collections::BTreeMap<String, serde_yaml::Value>> =
        Option::deserialize(deserializer)?;
    Ok(opt.map(|m| {
        m.iter()
            .filter_map(|(k, v)| stringify_scalar(v).map(|s| (k.clone(), s)))
            .collect()
    }))
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub address: Address,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Listener {
    pub name: String,
    pub address: Address,
    pub filter_chains: Vec<FilterChain>,
    /// 05.4 NEW per ADR-0026: optional listener filters declared by the
    /// upstream Envoy `envoy.yaml`. Parse-and-ignore: stored as opaque
    /// `serde_yaml::Value`s; envoy-rust does NOT execute listener filters
    /// (SNI dispatch lives at the rustls layer per phase 03.2). The field
    /// is accepted purely so envoy.yaml fixtures including a
    /// `listener_filters: [...]` block do not trigger `deny_unknown_fields`
    /// rejection on any path that parses envoy.yaml through envoy-config.
    #[serde(default)]
    pub listener_filters: Vec<serde_yaml::Value>,
    /// Envoy `Listener.enable_reuse_port` (bool, **default true** upstream since
    /// v1.15 for TCP). Parse-and-store: envoy-config only records it; the data
    /// plane (`envoy-bin`) consumes it to decide whether to bind one
    /// `SO_REUSEPORT` socket per worker (spreading the accept path + RX softirq
    /// across cores) or a single accepting socket. Defaulting to `true` matches
    /// Envoy so an `enable_reuse_port`-bearing bootstrap round-trips; a bootstrap
    /// that omits it keeps the field at `true` without any wire-observable change
    /// (the option is transparent to clients).
    #[serde(default = "default_enable_reuse_port")]
    pub enable_reuse_port: bool,
}

fn default_enable_reuse_port() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub socket_address: SocketAddress,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SocketAddress {
    pub address: String,
    pub port_value: u16,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChain {
    #[serde(default)]
    pub filter_chain_match: Option<FilterChainMatch>,
    #[serde(default)]
    pub transport_socket: Option<TransportSocket>,
    pub filters: Vec<NetworkFilter>,
}

/// Filter-chain matcher (phase 03.2 portion). Selects which filter chain a
/// connection routes to; for phase 03.2, only `server_names` (TLS SNI) is
/// supported. Empty / missing `server_names` is the catch-all (validator
/// enforces "at most one catch-all per listener").
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterChainMatch {
    /// SNI values this filter chain matches. Empty Vec = catch-all. The
    /// validator (Task 2) rejects two filter chains declaring the same SNI
    /// (case-insensitive) and rejects multiple catch-all chains per listener.
    #[serde(default)]
    pub server_names: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkFilter {
    pub name: String,
    #[serde(default)]
    pub typed_config: Option<TypedConfig>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy")]
    TcpProxy(TcpProxyConfig),
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager"
    )]
    HttpConnectionManager(HttpConnectionManagerConfig),
    /// 66 (ADR-0123): the Network-filters family opener.
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config"
    )]
    DirectResponse(DirectResponseConfig),
    /// 67.1 (ADR-0128 / ADR-0129): the Network-filters family's first
    /// NON-TERMINAL filter.
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC")]
    NetworkRbac(NetworkRbacConfig),
}

// 06.2 Task 5 — access-log schema additions per SPEC §3 D2.2.

/// AccessLog — one entry in an HCM's `access_log:` block. The shape mirrors
/// Envoy's `envoy.config.accesslog.v3.AccessLog`: `name` selects the logger
/// extension (only `envoy.access_loggers.file` is accepted in 06.2; the
/// validator rejects anything else) and `typed_config` carries the
/// extension-specific payload via a `@type`-tagged enum. Future
/// observability-family phases extend `AccessLogTypedConfig` rather than
/// reshaping `AccessLog`.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AccessLog {
    pub name: String,
    pub typed_config: AccessLogTypedConfig,
    /// Phase 70: the per-record emission predicate (`AccessLogFilter` oneof).
    /// Absent → the sink logs every record (today's behavior). Present → the
    /// record is emitted to this sink only when the filter matches.
    #[serde(default)]
    pub filter: Option<AccessLogFilter>,
}

/// Models `envoy.config.accesslog.v3.AccessLogFilter` — the per-record emission
/// predicate carried by an `AccessLog` entry. This phase models ONLY the
/// `status_code_filter` arm; future filter-family phases add further `Option`
/// arms here rather than reshaping the type. Cardinality (exactly one arm set)
/// is enforced by `validate_access_logs` (`ConfigError::AmbiguousAccessLogFilter`),
/// NOT by serde — mirroring the `SubstitutionFormatString` oneof precedent above.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AccessLogFilter {
    pub status_code_filter: Option<StatusCodeFilter>,
}

/// Models `envoy.config.accesslog.v3.StatusCodeFilter` — the `AccessLogFilter`
/// arm that compares the response status code against a configured value.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StatusCodeFilter {
    pub comparison: ComparisonFilter,
}

/// Models `envoy.config.accesslog.v3.ComparisonFilter` — the `{op, value}` pair
/// evaluated against the response status code.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComparisonFilter {
    pub op: ComparisonOp,
    pub value: RuntimeUInt32,
}

/// The upstream `ComparisonFilter.Op` enum. Exactly `{EQ, GE, LE}` (measured:
/// NE / bogus REJECTED). serde has no catch-all → any other token is a fatal
/// deserialize error (parity with upstream's unknown-enum rejection); see
/// `rejects_status_code_filter_unknown_op`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
pub enum ComparisonOp {
    #[serde(rename = "EQ")]
    Eq,
    #[serde(rename = "GE")]
    Ge,
    #[serde(rename = "LE")]
    Le,
}

/// Models `envoy.config.core.v3.RuntimeUInt32`. `runtime_key` is PGV-mandatory
/// (`min_len 1`) but RTDS-inert here (no runtime subsystem) — the comparison
/// always uses `default_value`. An empty `runtime_key` is rejected by
/// `validate_access_logs` (`ConfigError::EmptyStatusCodeFilterRuntimeKey`).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuntimeUInt32 {
    pub default_value: u32,
    pub runtime_key: String,
}

/// AccessLogTypedConfig — the `@type`-tagged envelope for an AccessLog
/// entry's `typed_config`. Single variant in 06.2 (file access logger);
/// the enum exists so future observability phases can add stdout / gRPC /
/// OpenTelemetry loggers without reshaping the schema. Unknown `@type`
/// URLs are rejected by serde at deserialization time (surfaces as
/// `ConfigError::Yaml`); see `rejects_hcm_with_unsupported_access_log_type_url`.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum AccessLogTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog")]
    FileAccessLog(FileAccessLog),
}

/// FileAccessLog — typed_config payload for the file access logger. `path`
/// names the sink; empty paths are rejected by the validator
/// (`ConfigError::InvalidAccessLogPath`).
///
/// Phase 32 (ADR-0079) adds the modern format-customization path
/// `log_format.text_format_source.inline_string`: an Envoy command-operator
/// format string that is compiled (`envoy_accesslog::parse_format`) at
/// config-load time and rejected boot-fatally
/// (`ConfigError::InvalidAccessLogFormat`) when malformed. The deprecated
/// `text_format` / top-level `format` paths and `json_format` remain OUT of
/// scope (deferred); when `log_format` is absent the emitter uses the default
/// Envoy v3 format string.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileAccessLog {
    pub path: String,
    #[serde(default)]
    pub log_format: Option<SubstitutionFormatString>,
}

/// Models `envoy.config.core.v3.SubstitutionFormatString` — the
/// `{text_format_source | json_format}` oneof (phase 38, ADR-0092). Exactly one
/// arm must be set; the cardinality is enforced by `validate_access_logs`
/// (`ConfigError::AmbiguousLogFormat`), not by serde. `json_format` is a
/// `google.protobuf.Struct` modelled as a `BTreeMap<String,String>` — the keys
/// emit in sorted (UTF-8-byte) order, matching v1.33.0's `json_format` wire
/// order (ADR-0092 §A); each value is a command-operator format string compiled
/// per-value via `envoy_accesslog::parse_format`. The deprecated `text_format`
/// scalar + `json_format_options`/`content_type` are deferred; `omit_empty_values`
/// is supported (ADR-0096 — the absent-operator sentinel swap).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionFormatString {
    #[serde(default)]
    pub text_format_source: Option<DataSourceInline>,
    #[serde(default)]
    pub json_format: Option<std::collections::BTreeMap<String, JsonFormatValue>>,
    /// `omit_empty_values` (ADR-0096): when `true`, the command-operator engine
    /// renders an absent operator as the EMPTY STRING `""` instead of the `-`
    /// sentinel in MULTI-SEGMENT values, for BOTH `text_format` and `json_format`,
    /// recursively. It does NOT drop keys/entries (§A), and the single-operator
    /// json-typed path (`encode_single_op` → `null`) is UNAFFECTED (§C). A plain
    /// `bool` that composes with either arm; `deny_unknown_fields` rejects typos.
    #[serde(default)]
    pub omit_empty_values: bool,
}

/// A `json_format` value — a single `google.protobuf.Struct` value (ADR-0094
/// §A–§D). Recursive: a command-operator format string (`Format`), a `bool` or
/// `null` literal (emitted native-typed per §D), a nested object (keys SORTED at
/// every level, §A — a `BTreeMap`), or a list (CONFIG order preserved, §B — a
/// `Vec`). NUMERIC literals are NOT accepted: a YAML number matches no
/// `#[serde(untagged)]` arm → boot-reject (ADR-0094 §D / CF-39-1 — the
/// protobuf-`double` JSON formatting `1e+06`/`"1.5"` rabbit hole is deferred).
///
/// `#[serde(untagged)]` arm ORDER is load-bearing: YAML `null/~` → `Null`,
/// `true/false` → `Bool`, a string → `Format`, a sequence → `Array`, a map →
/// `Object`. A bare number deserializes to none of these arms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonFormatValue {
    /// YAML `null` / `~` → native JSON `null` (ADR-0094 §D).
    Null,
    /// YAML `true` / `false` → native JSON `true` / `false` (ADR-0094 §D).
    Bool(bool),
    /// YAML string → a command-operator format string (compiled per-leaf, §C).
    Format(String),
    /// YAML sequence → an ordered list (config order, §B).
    Array(Vec<JsonFormatValue>),
    /// YAML map → a nested object (keys sorted per level, §A).
    Object(std::collections::BTreeMap<String, JsonFormatValue>),
}

/// Models the `inline_string` arm of an `envoy.config.core.v3.DataSource`. The
/// file/environment-variable DataSource arms are out of scope; only an inline
/// command-operator format string is accepted.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSourceInline {
    pub inline_string: String,
}

/// Models `envoy.extensions.filters.network.direct_response.v3.Config`.
///
/// `response` is OPTIONAL: upstream Envoy validates a `direct_response` filter
/// with the field omitted (`rc=0`) and then writes zero bytes before closing.
/// `None` therefore means "empty payload", not "invalid config" (phase-66 SPEC
/// §0 R-0.7).
///
/// Only the `inline_string` DataSource arm is modeled. Upstream Envoy also
/// accepts `inline_bytes` and `filename`; envoy-rust rejects both LOUDLY via
/// `DataSourceInline`'s `deny_unknown_fields` — the ADR-0049 decision-2 (b)
/// fail-loud posture. Recorded divergence, carry-forward CF-66-1 (ADR-0123).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponseConfig {
    #[serde(default)]
    pub response: Option<DataSourceInline>,
}

/// Is `name` a TERMINAL network filter — one that must be the LAST filter in
/// its chain?
///
/// Every network filter envoy-rust supports today is terminal, empirically
/// confirmed against `envoyproxy/envoy:v1.33.0` (phase-66 SPEC §0 R-0.6). This
/// is written as a per-name predicate rather than a `chain.filters.len() <= 1`
/// check so that the first NON-terminal network filter (`sni_cluster`, network
/// `rbac`) drops in without re-litigating the rule. See ADR-0123.
fn is_terminal_network_filter(name: &str) -> bool {
    matches!(
        name,
        crate::ECHO_FILTER
            | crate::TCP_PROXY_FILTER
            | crate::HCM_FILTER
            | crate::DIRECT_RESPONSE_FILTER
    )
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TcpProxyConfig {
    /// Required by Envoy for access-log attribution; accepted by envoy-rust and
    /// unused until phase 06 (access logs). Carrying it through the parser now
    /// keeps fixture YAMLs identical across upstream-Envoy and envoy-rust.
    pub stat_prefix: String,
    pub cluster: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransportSocket {
    /// Phase 03 accepts only `"envoy.transport_sockets.tls"`; the validator
    /// rejects any other name. Future phases may add raw_buffer / quic / etc.
    pub name: String,
    pub typed_config: TransportSocketTypedConfig,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum TransportSocketTypedConfig {
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext"
    )]
    Downstream(DownstreamTlsContext),
    #[serde(
        rename = "type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext"
    )]
    Upstream(UpstreamTlsContext),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DownstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpstreamTlsContext {
    pub common_tls_context: CommonTlsContext,
    /// Server Name sent in the ClientHello server_name extension. Phase 03
    /// requires this on every UpstreamTlsContext (no auto_sni). The validator
    /// rejects an empty string.
    pub sni: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonTlsContext {
    #[serde(default)]
    pub tls_certificates: Vec<TlsCertificate>,
    #[serde(default)]
    pub validation_context: Option<CertificateValidationContext>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TlsCertificate {
    pub certificate_chain: DataSource,
    pub private_key: DataSource,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CertificateValidationContext {
    pub trusted_ca: DataSource,
}

/// Phase 03 supports `filename` only. Phase 04.1 adds `inline_string` for the
/// HCM `direct_response.body` use-case (small inline payloads). `inline_bytes`,
/// `environment_variable`, and `secret_ref` are deferred to later phases.
///
/// Schema-level both fields are `Option<String>` with `#[serde(default)]`. The
/// "exactly one of {filename, inline_string} is `Some`" invariant — and any
/// per-callsite restriction (e.g. TLS still requires `filename`) — is enforced
/// by the validator (Task 2 of phase 04.1), not by serde.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub inline_string: Option<String>,
}

/// HCM (HTTP Connection Manager) typed-config. Phase 04.1 carries the minimal
/// subset needed for the `direct_response` happy path: stat_prefix, codec_type,
/// route_config, http_filters. Upstream Envoy's HCM has dozens more fields
/// (access_log, tracing, http_protocol_options, idle_timeout, ...); all
/// deferred per SPEC §4.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectionManagerConfig {
    pub stat_prefix: String,
    pub codec_type: CodecType,

    /// 05.2 NEW: listener-side HTTP/2 protocol tuning (per SPEC §3 D2.b).
    /// Optional; absent means "use h2-crate defaults". Validator runs the
    /// RFC 7540 range checks at parse time only when `Some`.
    #[serde(default)]
    pub http2_protocol_options: Option<Http2ProtocolOptions>,

    /// 06.2 NEW: per-listener access-log entries. `#[serde(default)]` is
    /// load-bearing because `HttpConnectionManagerConfig` carries
    /// `#[serde(deny_unknown_fields)]` and the 5 existing HCM-bearing
    /// fixtures (0007/0008/0009/0010/0011) do not declare an `access_log:`
    /// block — without the default they would fail to parse. The
    /// validator (`validate_access_logs`) rejects non-file loggers
    /// (`UnsupportedAccessLogType`) and empty paths
    /// (`InvalidAccessLogPath`).
    #[serde(default)]
    pub access_log: Vec<AccessLog>,

    /// 20 D1 (ADR-0051/0052): the inline route table. EXACTLY ONE of
    /// `route_config` (inline) or `rds` (file) per HCM (enforced at parse time —
    /// §5.8). After load_dynamic_resources populates an rds HCM's route_config
    /// from its file, both are Some (the loaded state — §5.3); downstream
    /// dispatch reads route_config uniformly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_config: Option<RouteConfiguration>,
    /// 20 D1: RDS — route configuration loaded from a file (reuses ConfigSource).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rds: Option<Rds>,
    pub http_filters: Vec<HttpFilter>,
}

/// HCM codec_type. Phase 04.1 wire-supports HTTP1 only (Task 10's HCM rejects
/// the others at construction time); AUTO/HTTP2/HTTP3 parse but do not yet
/// dispatch.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum CodecType {
    AUTO,
    HTTP1,
    HTTP2,
    HTTP3,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpFilter {
    pub name: String,
    pub typed_config: HttpFilterTypedConfig,
}

/// `envoy.extensions.filters.http.fault.v3.HTTPFault` config (abort path).
/// Phase 11 supports the abort block + optional header-match gate; delay,
/// response_rate_limit, max_active_faults, and downstream-controlled faults
/// all defer per phase-11 SPEC §4 (rejected by `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultConfig {
    pub abort: FaultAbort,
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,
}

/// `envoy.extensions.filters.http.fault.v3.FaultAbort` (abort block).
/// `grpc_status` + `header_abort` defer per SPEC §4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FaultAbort {
    pub http_status: u16,
    pub percentage: FractionalPercent,
}

/// `envoy.type.v3.FractionalPercent`. A general shared config type (the first
/// percent type in envoy-config); authored to be reusable by future filters
/// that take a fractional percentage.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FractionalPercent {
    pub numerator: u32,
    #[serde(default = "default_denominator")]
    pub denominator: DenominatorType,
}

impl FractionalPercent {
    /// Phase-11 deterministic select: `true` iff 100% (`numerator ==
    /// denominator.value()`), `false` iff 0% (`numerator == 0`). The validator
    /// (`validate_fault_config`) guarantees `numerator ∈ {0, denominator.value()}`,
    /// so this is a pure boolean — no per-request randomness, no PRNG. Fractional
    /// percentage defers per SPEC §4 + §5.6.
    pub fn selects_deterministic(&self) -> bool {
        self.numerator == self.denominator.value()
    }
}

/// `envoy.type.v3.FractionalPercent.DenominatorType`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DenominatorType {
    Hundred,
    TenThousand,
    Million,
}

impl DenominatorType {
    /// The integer denominator this variant represents.
    pub fn value(self) -> u32 {
        match self {
            DenominatorType::Hundred => 100,
            DenominatorType::TenThousand => 10_000,
            DenominatorType::Million => 1_000_000,
        }
    }
}

fn default_denominator() -> DenominatorType {
    DenominatorType::Hundred
}

/// `envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication` config
/// (phase 22, minimum-viable per ADR-0055/SPEC §3). RS256, inline `local_jwks`,
/// a single `provider_name` per matched rule, default `Authorization: Bearer`
/// extraction, `iss`/`aud`/`exp`/`nbf` validation. Deferred fields
/// (`requirement_map`, `bypass_cors_preflight`, `strip_failure_response`, …)
/// are rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtAuthnConfig {
    pub providers: std::collections::BTreeMap<String, JwtProvider>,
    #[serde(default)]
    pub rules: Vec<RequirementRule>,
}

/// One JWT provider. `local_jwks` reuses the existing `DataSource`
/// (inline_string only at phase-22 scope — the validator enforces it).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtProvider {
    pub issuer: String,
    #[serde(default)]
    pub audiences: Vec<String>,
    pub local_jwks: DataSource,
    /// Envoy default `false` ⇒ strip `Authorization` upstream on success
    /// (§6.2 L6). Deferred: remote_jwks, from_headers/params/cookies,
    /// forward_payload_header, payload_in_metadata, claim_to_headers,
    /// clock_skew_seconds, jwks_cache_duration, jwt_cache_config.
    #[serde(default)]
    pub forward: bool,
}

/// One `{ match: RouteMatch, requires: JwtRequirement }` rule. `match` reuses
/// the 04.x `RouteMatch`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RequirementRule {
    pub r#match: RouteMatch,
    pub requires: JwtRequirement,
}

/// Minimum-viable single-provider requirement. Deferred: `requires_any`,
/// `requires_all`, `allow_missing`, `allow_missing_or_failed`, named
/// requirements (rejected by `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct JwtRequirement {
    pub provider_name: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "@type", deny_unknown_fields)]
pub enum HttpFilterTypedConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.router.v3.Router")]
    Router(RouterConfig),

    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation"
    )]
    HeaderMutation(HeaderMutationConfig),

    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit"
    )]
    LocalRateLimit(LocalRateLimitConfig),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC")]
    Rbac(RbacConfig),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.fault.v3.HTTPFault")]
    Fault(FaultConfig),

    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication"
    )]
    JwtAuthn(JwtAuthnConfig),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors")]
    Cors(CorsConfig),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy")]
    Csrf(CsrfPolicy),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer")]
    Buffer(Buffer),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig")]
    CdnLoop(CdnLoopConfig),

    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config")]
    SetMetadata(SetMetadataConfig),

    #[serde(
        rename = "type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config"
    )]
    HeaderToMetadata(HeaderToMetadataConfig),
}

impl HttpFilterTypedConfig {
    /// The canonical `HttpFilter.name` each `typed_config` variant requires.
    /// `validate_http_filters` rejects any name/typed_config mismatch with
    /// `ConfigError::UnsupportedHttpFilter` before running the per-filter
    /// validators.
    fn expected_name(&self) -> &'static str {
        match self {
            Self::Router(_) => "envoy.filters.http.router",
            Self::HeaderMutation(_) => "envoy.filters.http.header_mutation",
            Self::LocalRateLimit(_) => "envoy.filters.http.local_ratelimit",
            Self::Rbac(_) => "envoy.filters.http.rbac",
            Self::Fault(_) => "envoy.filters.http.fault",
            Self::JwtAuthn(_) => "envoy.filters.http.jwt_authn",
            Self::Cors(_) => "envoy.filters.http.cors",
            Self::Csrf(_) => "envoy.filters.http.csrf",
            Self::Buffer(_) => "envoy.filters.http.buffer",
            Self::CdnLoop(_) => "envoy.filters.http.cdn_loop",
            Self::SetMetadata(_) => "envoy.filters.http.set_metadata",
            Self::HeaderToMetadata(_) => "envoy.filters.http.header_to_metadata",
        }
    }
}

/// Empty in 04.1; Envoy's Router has many fields (suppress_envoy_headers,
/// dynamic_stats, start_child_span, ...); all deferred per SPEC §4.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct RouterConfig {}

// ---------------------------------------------------------------------------
// 23 D1: per-route typed_per_filter_config types
// ---------------------------------------------------------------------------

/// 23 D1: per-route filter configuration map, keyed by filter name
/// (`envoy.filters.http.cors`, …) → a `@type`-tagged per-filter config.
/// The per-route counterpart to the filter-chain `HttpFilterTypedConfig`.
/// The `Cors` (phase 23) and `Csrf` (phase 24) variants are registered +
/// consumed; future filters slot in additively (§6.3 anti-pattern: no
/// untested stub variants).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "@type")]
pub enum PerFilterConfig {
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy")]
    Cors(CorsPolicy),
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy")]
    Csrf(CsrfPolicy),
    #[serde(rename = "type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute")]
    Buffer(BufferPerRoute),
}

/// `envoy.config.core.v3.RuntimeFractionalPercent`. A `default_value`
/// percentage plus an optional `runtime_key`. The csrf filter's `filter_enabled`
/// is of this type (REQUIRED — §6.2/ADR-0061 L1). envoy-rust honors only the
/// deterministic 0%/100% `default_value`; a present `runtime_key` is rejected
/// (no RTDS runtime layer — ADR-0061 L6, the ADR-0049 all-fatal posture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFractionalPercent {
    pub default_value: FractionalPercent,
    #[serde(default)]
    pub runtime_key: Option<String>,
}

/// `envoy.extensions.filters.http.csrf.v3.CsrfPolicy` (phase 24, minimum-viable
/// per ADR-0060/0061). The SAME message is used at the filter-chain level
/// (`HttpFilterTypedConfig::Csrf`) AND the per-route level (`PerFilterConfig::Csrf`)
/// — ADR-0061 L1. `filter_enabled` is REQUIRED (no `#[serde(default)]`).
/// `additional_origins` reuses the 04.x `StringMatcher` and is matched against
/// the scheme-stripped `host[:port]` source origin (ADR-0061 L3). The deferred
/// `shadow_enabled` is rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsrfPolicy {
    pub filter_enabled: RuntimeFractionalPercent,
    #[serde(default)]
    pub additional_origins: Vec<StringMatcher>,
}

/// `envoy.extensions.filters.http.buffer.v3.Buffer` — the chain-level buffer
/// config. `max_request_bytes` is a proto `UInt32Value` accepted on the wire as
/// a plain integer (ADR-0063 finding 2); modeled as a REQUIRED non-Option u32
/// (absent → serde missing-field error → fatal at startup, the ADR-0049
/// all-fatal posture; `0` is a valid limit → reject iff body.len() > 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Buffer {
    pub max_request_bytes: u32,
}

/// `envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig` (phase 31,
/// ADR-0077 §6.2). The CDN-Loop filter appends this proxy's `cdn_id` to the
/// `CDN-Loop` request header and rejects requests whose count of this `cdn_id`
/// already exceeds `max_allowed_occurrences` (RFC 8586 loop detection).
///
/// `cdn_id` is REQUIRED and must be a non-empty bare RFC-7230 token (validated
/// at startup by `validate_cdn_loop_config` — empty → `CdnLoopEmptyCdnId`, any
/// non-tchar → `CdnLoopInvalidCdnId`; all boot-fatal per ADR-0049, matching
/// Envoy's own boot rejection). `max_allowed_occurrences` is a proto `uint32`
/// accepted as a plain integer; absent → 0 (`#[serde(default)]`), meaning the
/// request is rejected the moment this `cdn_id` appears at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CdnLoopConfig {
    pub cdn_id: String,
    #[serde(default)]
    pub max_allowed_occurrences: u32,
}

/// `envoy.extensions.filters.http.set_metadata.v3.Config` (phase 33,
/// §A1-LOCKED). The modern repeated `metadata` form. Each entry merges a flat
/// string→string `value` map into the request's dynamic metadata under
/// `metadata_namespace`. String-only MVP (non-string values rejected by serde
/// — the §2.2 deferral). `allow_overwrite` (Envoy default false) governs
/// whether existing keys in the namespace are overwritten.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetMetadataConfig {
    pub metadata: Vec<MetadataEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataEntry {
    pub metadata_namespace: String,
    pub value: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub allow_overwrite: bool,
}

/// `envoy.extensions.filters.http.header_to_metadata.v3.Config` (phase 34, §A1-LOCKED).
/// Request-side, string-only subset: each rule extracts a request header into dynamic
/// metadata. `cookie`/`remove`/`encode`/non-STRING `type` are §2.2-deferred → rejected
/// by `deny_unknown_fields` / the `HeaderToMetadataType` enum (boot-fatal, stricter than Envoy).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderToMetadataConfig {
    pub request_rules: Vec<HeaderToMetadataRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderToMetadataRule {
    pub header: String,
    #[serde(default)]
    pub on_header_present: Option<HeaderToMetadataKeyValue>,
    #[serde(default)]
    pub on_header_missing: Option<HeaderToMetadataKeyValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeaderToMetadataKeyValue {
    #[serde(default = "default_h2m_namespace")]
    pub metadata_namespace: String,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub r#type: HeaderToMetadataType,
}

fn default_h2m_namespace() -> String {
    "envoy.filters.http.header_to_metadata".to_string()
}

/// §A3: MVP models only STRING. A `type: NUMBER | PROTOBUF_VALUE` (the §2.2 non-string-Value
/// deferral) fails deserialization → boot-fatal (stricter than Envoy).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HeaderToMetadataType {
    #[default]
    String,
}

/// `envoy.extensions.filters.http.buffer.v3.BufferPerRoute` — the per-route
/// override. Envoy's `override` oneof is `{ disabled: bool; buffer: Buffer }`
/// (ADR-0063 finding 3); modeled as two fields where `disabled: true` bypasses
/// the filter for the route and `buffer` lowers/overrides the limit. An empty
/// `{}` (neither set) falls back to the chain-level base at apply time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BufferPerRoute {
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub buffer: Option<Buffer>,
}

/// 23 D1: the per-route CORS policy (attached via `typed_per_filter_config`).
/// `allow_origin_string_match` reuses the 04.x `StringMatcher`. Deferred
/// fields (`filter_enabled`, `shadow_enabled`, `allow_private_network_access`,
/// `forward_not_matching_preflights`) are rejected by `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorsPolicy {
    pub allow_origin_string_match: Vec<StringMatcher>,
    #[serde(default)]
    pub allow_methods: Option<String>,
    #[serde(default)]
    pub allow_headers: Option<String>,
    #[serde(default)]
    pub expose_headers: Option<String>,
    #[serde(default)]
    pub max_age: Option<String>,
    #[serde(default)]
    pub allow_credentials: Option<bool>,
}

/// 23 D5: the near-empty filter-chain `Cors` entry (declares the filter present).
/// The actual policy lives per-route in `CorsPolicy`. The deferred
/// `filter_enabled`/`shadow_enabled` runtime knobs are rejected by
/// `deny_unknown_fields`. When used inside `HttpFilterTypedConfig` (Task 4),
/// the `@type` field is consumed by the enum's `#[serde(tag = "@type")]` before
/// this struct sees it — so `deny_unknown_fields` here correctly rejects any
/// unexpected CORS filter-chain config fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CorsConfig {}

// ---------------------------------------------------------------------------

/// `envoy.extensions.filters.http.header_mutation.v3.HeaderMutation` config.
/// The HeaderMutation filter appends/overwrites request and response headers.
/// Phase 07.2.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationConfig {
    pub mutations: Mutations,
}

/// Configuration for `envoy.filters.http.local_ratelimit` (phase 09).
///
/// Minimum-viable surface per phase-09 SPEC §3 D1: filter-chain config only;
/// no per-route variation; no descriptors; no per-downstream-connection
/// scope; no runtime fractional overrides. The 5 upstream-Envoy fields
/// (`descriptors`, `local_rate_limit_per_downstream_connection`,
/// `filter_enabled`, `filter_enforced`, `request_headers_to_add_when_not_enforced`)
/// are explicitly NOT modeled at the 09 baseline; serde
/// `deny_unknown_fields` rejects them.
///
/// `response_headers_to_add` reuses the 07.2-landed
/// `HeaderValueOption { header: HeaderValue, append_action: AppendAction }`
/// type. Upstream Envoy v1.33's
/// `envoy.extensions.filters.http.local_ratelimit.v3.LocalRateLimit.response_headers_to_add`
/// is `repeated config.core.v3.HeaderValueOption` — the same proto type as
/// HeaderMutation uses — so each entry must carry an `append_action`
/// (typically `APPEND_IF_EXISTS_OR_ADD`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LocalRateLimitConfig {
    pub stat_prefix: String,
    pub token_bucket: TokenBucket,
    #[serde(default)]
    pub response_headers_to_add: Vec<HeaderValueOption>,
    #[serde(default = "default_status")]
    pub status: HttpStatus,
}

/// Token-bucket parameters for the `LocalRateLimit` filter. `fill_interval`
/// is deserialized as a free-form YAML scalar and parsed to `Duration` at
/// validate-time via `parse_duration` (supports `"<N>s"` / `"<N>ms"` /
/// `"<N>us"` shapes per upstream Envoy v1.33's documented Duration formats).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TokenBucket {
    pub max_tokens: u32,
    pub tokens_per_fill: u32,
    pub fill_interval: serde_yaml::Value,
}

/// HTTP status code for the synthesized rate-limited response. Phase 09
/// accepts `code: 429` only; the validator rejects any other value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpStatus {
    pub code: u16,
}

fn default_status() -> HttpStatus {
    HttpStatus { code: 429 }
}

/// Configuration for `envoy.filters.http.rbac` (phase 10).
///
/// Minimum-viable surface per phase-10 SPEC §3 D1: filter-chain config only;
/// header-based Permission/Principal types + combinators only. The 3 phase-10
/// deferred upstream-Envoy fields (`shadow_rules`, `shadow_rules_stat_prefix`,
/// `track_per_rule_stats`) are NOT modeled; `deny_unknown_fields` rejects them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RbacConfig {
    pub rules: Rules,
}

/// 67.1 D1: `envoy.extensions.filters.network.rbac.v3.RBAC` — the NETWORK (L4)
/// RBAC filter. Distinct from `RbacConfig` above, which is the HTTP filter's
/// config (`envoy.filters.http.rbac`); the two share the `Rules` / `Policy` /
/// `Permission` / `Principal` trees but nothing else.
///
/// `stat_prefix` is REQUIRED and non-empty (upstream proto constraint; SPEC
/// R-3). `rules` is OPTIONAL, and `None` means the filter is **INERT** — the
/// connection is allowed and NEITHER counter increments (SPEC R-4, measured).
/// Do NOT materialise a default `Rules { action: ALLOW }`: that would tick
/// `allowed` and produce a stat divergence with no body divergence.
///
/// `deny_unknown_fields` is what rejects `shadow_rules` /
/// `shadow_rules_stat_prefix` loudly (CF-67-1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkRbacConfig {
    pub stat_prefix: String,
    #[serde(default)]
    pub rules: Option<Rules>,
}

/// The RBAC policy tree at the filter-config level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    #[serde(default = "default_action")]
    pub action: Action,
    #[serde(default)]
    pub policies: std::collections::BTreeMap<String, Policy>,
}

/// RBAC top-level action. `Log` (audit-only, never enforce) defers per
/// phase-10 SPEC §4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Action {
    Allow,
    Deny,
}

fn default_action() -> Action {
    Action::Allow
}

/// One named RBAC policy. `condition` / `checked_condition` (CEL) defer per
/// phase-10 SPEC §4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub permissions: Vec<Permission>,
    pub principals: Vec<Principal>,
}

/// RBAC `metadata` matcher (phase 35). Reads a single-segment dynamic-metadata
/// path. `filter` is the namespace (the producer's `metadata_namespace`). The MVP models
/// a single `path` segment + a string-only `value` (multi-segment path + non-string_match
/// value are stricter-than-Envoy boot-fatal). `value` is REQUIRED.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataMatcher {
    pub filter: String,
    pub path: Vec<MetadataPathSegment>,
    pub value: ValueMatcher,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPathSegment {
    pub key: String,
}

/// Envoy `type.matcher.v3.PathMatcher` (phase 37). The only in-scope rule is
/// `path` (a `StringMatcher`); RBAC `url_path` matches the request path with the
/// `?query` stripped (ADR-0090 §B). A thin derived struct — the required `path`
/// field + `deny_unknown_fields` + the inner StringMatcher's "missing mode key"
/// error make an empty/path-less/unknown-subkey `PathMatcher` boot-fatal,
/// matching Envoy (ADR-0090 §D); no hand-rolled visitor needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathMatcher {
    pub path: StringMatcher,
}

/// Envoy `type.matcher.v3.ValueMatcher`. The hand-rolled "exactly one key" Deserialize
/// accepts `string_match` and `present_match`; any other oneof key (`null_match`,
/// `bool_match`, …) → `unknown_field` → boot-fatal (stricter than Envoy, which accepts
/// them). Mirrors the `Permission`/`StringMatcher` visitor template.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ValueMatcher {
    #[serde(rename = "string_match")]
    StringMatch(StringMatcher),
    /// §A1 (phase 36): match on KEY PRESENCE. Semantics `match = present && want`
    /// (`present_match: false` NEVER matches — NOT the HeaderMatcher `present_match` precedent).
    #[serde(rename = "present_match")]
    PresentMatch(bool),
}

/// Generates the hand-rolled "map with exactly one key" `Deserialize` impl
/// shared by the externally-tagged oneof enums below (`ValueMatcher`,
/// `Permission`, `Principal`). `serde_yaml` 0.9 cannot deserialize
/// externally-tagged enums from plain YAML maps (`{any: true}` — it expects
/// YAML `!Tag` syntax), so each enum visits a map, requires exactly one key,
/// and dispatches on it (the 04.2 `HeaderMatcher` visitor template).
///
/// Arguments:
///   - the enum type;
///   - the type-name literal spliced verbatim into the two cardinality error
///     strings (`"<name>: expected one map key, got none"` /
///     `"<name>: expected exactly one map key, got more"`);
///   - the WHOLE `expecting` format string as a literal (`{KEYS:?}` names the
///     generated key slice, bound as an explicit format argument — macro
///     hygiene keeps a call-site literal from implicitly capturing it);
///   - the map-binding identifier the construction expressions use (hygiene
///     again: a binding introduced inside the macro body would be invisible
///     to call-site expressions);
///   - the `"key" => construction-expr` dispatch arms; the keys, in order,
///     also form the `KEYS` slice reported by `unknown_field`/`expecting`.
///
/// Every emitted error string is byte-identical to the pre-macro hand-rolled
/// impls (tests assert on these strings).
macro_rules! impl_single_key_oneof {
    (
        $ty:ident,
        $name:literal,
        $expecting:literal,
        $map:ident: [ $($key:literal => $ctor:expr),+ $(,)? ]
    ) => {
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                use serde::de::{Error, MapAccess, Visitor};
                use std::fmt;

                const KEYS: &[&str] = &[$($key),+];

                struct V;
                impl<'de> Visitor<'de> for V {
                    type Value = $ty;
                    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                        write!(f, $expecting, KEYS = KEYS)
                    }
                    fn visit_map<M>(self, mut $map: M) -> Result<$ty, M::Error>
                    where
                        M: MapAccess<'de>,
                    {
                        let key: String = $map.next_key()?.ok_or_else(|| {
                            M::Error::custom(concat!($name, ": expected one map key, got none"))
                        })?;
                        let value = match key.as_str() {
                            $($key => $ctor,)+
                            other => return Err(M::Error::unknown_field(other, KEYS)),
                        };
                        if $map.next_key::<String>()?.is_some() {
                            return Err(M::Error::custom(concat!(
                                $name,
                                ": expected exactly one map key, got more",
                            )));
                        }
                        Ok(value)
                    }
                }
                deserializer.deserialize_map(V)
            }
        }
    };
}

impl_single_key_oneof!(
    ValueMatcher,
    "ValueMatcher",
    "a ValueMatcher map with exactly one of {KEYS:?} as key",
    map: [
        "string_match" => ValueMatcher::StringMatch(map.next_value::<StringMatcher>()?),
        "present_match" => ValueMatcher::PresentMatch(map.next_value::<bool>()?),
    ]
);

/// Envoy `config.core.v3.CidrRange` — an IP prefix. `address_prefix` is a bare
/// IP string (`"10.0.0.0"`) parsed to `IpAddr`; `prefix_len` is the mask width.
///
/// WIRE-SHAPE (67.2 PLAN-VERIFY X-1, ADR-0133; measured against
/// `envoyproxy/envoy:v1.33.0` with `--mode validate`): `prefix_len` is Envoy's
/// `UInt32Value`, which upstream accepts as EITHER a bare integer
/// (`prefix_len: 24`) OR the wrapper (`prefix_len: {value: 24}`). envoy-rust
/// models it as a bare `u8` and REJECTS the wrapper — matching the codebase's
/// established UInt32Value posture (`Buffer::max_request_bytes`, ADR-0063) and
/// the ADR-0049 fail-loud stance. `prefix_len` is REQUIRED (absent → serde
/// missing-field → fatal); upstream defaults an absent `prefix_len` to 0, a
/// documented divergence with no differential observable (67.2 ships no fixture).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CidrRange {
    pub address_prefix: std::net::IpAddr,
    pub prefix_len: u8,
}

/// Canonicalise an `IpAddr` the way `CidrRange` matching does: an
/// IPv4-mapped-IPv6 address (`::ffff:a.b.c.d`) collapses to its 4-byte IPv4
/// form (ADR-0133), everything else is left as-is. This MUST be the single
/// canonicalisation rule shared by `validate` and `contains` — if the two
/// disagree on an address's family, `validate` can size `prefix_len` against a
/// 16-byte width while `contains` indexes a 4-byte octet array, which panics
/// `prefix_match` out of bounds (phase-67.2 REVIEW C-1).
fn canonical_ip(ip: std::net::IpAddr) -> std::net::IpAddr {
    match ip {
        std::net::IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => std::net::IpAddr::V4(v4),
            None => std::net::IpAddr::V6(v6),
        },
        v4 => v4,
    }
}

impl CidrRange {
    /// Validate the prefix width against the **canonical** address family:
    /// ≤ 32 for IPv4, ≤ 128 for IPv6. `Err(detail)` is mapped to
    /// `ConfigError::InvalidCidrRange` by the L4 allow-list walk
    /// (`validate_l4_permission` / `_principal`).
    ///
    /// The family is taken AFTER `canonical_ip`, so an IPv4-mapped-IPv6
    /// `address_prefix` such as `"::ffff:127.0.0.0"` is bounded at 32 — the same
    /// width `contains`/`prefix_match` will actually index. Sizing against the
    /// raw (pre-canonical) IPv6 family accepted `prefix_len` up to 128 on a range
    /// that `contains` then indexes as 4 bytes, panicking the data plane on the
    /// first matching connection (phase-67.2 REVIEW C-1). Rejecting the over-wide
    /// mapped prefix fail-loud at config load closes that validate/contains
    /// disagreement.
    pub(crate) fn validate(&self) -> Result<(), String> {
        let (max, family) = match canonical_ip(self.address_prefix) {
            std::net::IpAddr::V4(_) => (32u8, "IPv4"),
            std::net::IpAddr::V6(_) => (128u8, "IPv6"),
        };
        if self.prefix_len > max {
            return Err(format!(
                "prefix_len {} exceeds {} for {}",
                self.prefix_len, max, family
            ));
        }
        Ok(())
    }

    /// Does `addr` fall inside this prefix? IPv4-mapped-IPv6 addresses are
    /// canonicalised to IPv4 first (ADR-0133), so `::ffff:127.0.0.1` matches an
    /// IPv4 `127.0.0.0/8` range — upstream Envoy's behavior. After
    /// canonicalisation, a cross-family comparison never matches.
    pub fn contains(&self, addr: &std::net::IpAddr) -> bool {
        match (canonical_ip(self.address_prefix), canonical_ip(*addr)) {
            (std::net::IpAddr::V4(net), std::net::IpAddr::V4(a)) => {
                prefix_match(&net.octets(), &a.octets(), self.prefix_len)
            }
            (std::net::IpAddr::V6(net), std::net::IpAddr::V6(a)) => {
                prefix_match(&net.octets(), &a.octets(), self.prefix_len)
            }
            _ => false,
        }
    }
}

/// Compare the leading `prefix_len` bits of two same-length octet slices.
/// `prefix_len == 0` matches everything; a full-byte boundary skips the mask.
fn prefix_match(net: &[u8], addr: &[u8], prefix_len: u8) -> bool {
    let bits = prefix_len as usize;
    let full = bits / 8;
    // Defensive bounds guard (phase-67.2 REVIEW N-1). The family-consistent
    // `validate` above is the real fix — a prefix wider than its canonical
    // address family can no longer reach here from config. This guard keeps the
    // data-plane invariant "must never panic" true unconditionally, even for a
    // future caller that constructs a `CidrRange` without validating: an over-wide
    // or cross-length prefix simply never matches instead of indexing out of
    // bounds. A silent bail (not `debug_assert!`) so `contains` cannot panic in
    // debug builds either — the whole point of C-1's fix.
    if full > net.len() || full > addr.len() {
        return false;
    }
    if net[..full] != addr[..full] {
        return false;
    }
    let rem = bits % 8;
    if rem == 0 {
        return true;
    }
    let mask = 0xff_u8 << (8 - rem);
    (net[full] & mask) == (addr[full] & mask)
}

/// RBAC Permission. Only header-based + combinators land at phase 10;
/// `url_path`, `destination_ip`, `destination_port[_range]`, `metadata`,
/// `requested_server_name[_matcher]`, `uri_template` defer per phase-10 SPEC §4.
///
/// Deserialize is hand-rolled because `serde_yaml` 0.9 does not support
/// externally-tagged enums via plain YAML maps (`{any: true}`) — it expects
/// YAML `!Tag` syntax. The hand-rolled impl mirrors the 04.2 `HeaderMatcher`
/// pattern: visit a map with exactly one key, dispatch to the matching variant.
/// Serialize derive is retained (produces `{"any":true}` for JSON, which the
/// 08.1 roundtrip path uses).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Permission {
    #[serde(rename = "any")]
    Any(bool),
    #[serde(rename = "header")]
    Header(HeaderMatcher),
    #[serde(rename = "and_rules")]
    AndRules(PermissionSet),
    #[serde(rename = "or_rules")]
    OrRules(PermissionSet),
    #[serde(rename = "not_rule")]
    NotRule(Box<Permission>),
    #[serde(rename = "metadata")]
    Metadata(MetadataMatcher),
    /// Phase 37: `url_path` condition (Envoy `type.matcher.v3.PathMatcher`).
    /// Matches the request path with the `?query` stripped (ADR-0090 §B).
    #[serde(rename = "url_path")]
    UrlPath(PathMatcher),
    /// 67.2: L4 destination IP prefix. Evaluated against `local_addr.ip()` by the
    /// network engine; REJECTED fail-loud by the HTTP filter (ADR-0133).
    #[serde(rename = "destination_ip")]
    DestinationIp(CidrRange),
    /// 67.2: L4 destination port. Plain `uint32` upstream with PGV `lte: 65535`,
    /// so a bare `u16` is exactly faithful (rejects the wrapper AND > 65535).
    #[serde(rename = "destination_port")]
    DestinationPort(u16),
}

impl_single_key_oneof!(
    Permission,
    "Permission",
    "an RBAC Permission map with exactly one of {KEYS:?} as key",
    map: [
        "any" => Permission::Any(map.next_value::<bool>()?),
        "header" => Permission::Header(map.next_value::<HeaderMatcher>()?),
        "and_rules" => Permission::AndRules(map.next_value::<PermissionSet>()?),
        "or_rules" => Permission::OrRules(map.next_value::<PermissionSet>()?),
        "not_rule" => Permission::NotRule(Box::new(map.next_value::<Permission>()?)),
        "metadata" => Permission::Metadata(map.next_value::<MetadataMatcher>()?),
        "url_path" => Permission::UrlPath(map.next_value::<PathMatcher>()?),
        "destination_ip" => Permission::DestinationIp(map.next_value::<CidrRange>()?),
        "destination_port" => Permission::DestinationPort(map.next_value::<u16>()?),
    ]
);

/// Wrapper for `Permission::AndRules` / `Permission::OrRules` sub-rule lists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PermissionSet {
    pub rules: Vec<Permission>,
}

/// RBAC Principal. Only header-based + combinators land at phase 10;
/// `authenticated`, `source_ip`, `direct_remote_ip`, `remote_ip`, `url_path`,
/// `metadata`, `filter_state` defer per phase-10 SPEC §4.
///
/// Deserialize is hand-rolled per the `Permission` rationale above.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Principal {
    #[serde(rename = "any")]
    Any(bool),
    #[serde(rename = "header")]
    Header(HeaderMatcher),
    #[serde(rename = "and_ids")]
    AndIds(PrincipalSet),
    #[serde(rename = "or_ids")]
    OrIds(PrincipalSet),
    #[serde(rename = "not_id")]
    NotId(Box<Principal>),
    #[serde(rename = "metadata")]
    Metadata(MetadataMatcher),
    /// Phase 37: `url_path` condition (Envoy `type.matcher.v3.PathMatcher`).
    /// Matches the request path with the `?query` stripped (ADR-0090 §A/§B —
    /// Envoy's `Principal` carries `url_path` too).
    #[serde(rename = "url_path")]
    UrlPath(PathMatcher),
    /// 67.2: the downstream connection source IP (`peer_addr.ip()`). `remote_ip`
    /// and `source_ip` coincide with this today (no listener filters, ADR-0133).
    #[serde(rename = "direct_remote_ip")]
    DirectRemoteIp(CidrRange),
    #[serde(rename = "remote_ip")]
    RemoteIp(CidrRange),
    /// Deprecated alias of `direct_remote_ip` upstream (emits a deprecation
    /// warning); identical evaluation here. ADR-0133 / SPEC X-2.
    #[serde(rename = "source_ip")]
    SourceIp(CidrRange),
}

impl_single_key_oneof!(
    Principal,
    "Principal",
    "an RBAC Principal map with exactly one of {KEYS:?} as key",
    map: [
        "any" => Principal::Any(map.next_value::<bool>()?),
        "header" => Principal::Header(map.next_value::<HeaderMatcher>()?),
        "and_ids" => Principal::AndIds(map.next_value::<PrincipalSet>()?),
        "or_ids" => Principal::OrIds(map.next_value::<PrincipalSet>()?),
        "not_id" => Principal::NotId(Box::new(map.next_value::<Principal>()?)),
        "metadata" => Principal::Metadata(map.next_value::<MetadataMatcher>()?),
        "url_path" => Principal::UrlPath(map.next_value::<PathMatcher>()?),
        "direct_remote_ip" => Principal::DirectRemoteIp(map.next_value::<CidrRange>()?),
        "remote_ip" => Principal::RemoteIp(map.next_value::<CidrRange>()?),
        "source_ip" => Principal::SourceIp(map.next_value::<CidrRange>()?),
    ]
);

/// Wrapper for `Principal::AndIds` / `Principal::OrIds` sub-id lists. Field
/// name is `ids` (not `rules`) per upstream proto.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrincipalSet {
    pub ids: Vec<Principal>,
}

/// Defense-in-depth bound on Permission/Principal tree recursion at parse
/// time; the runtime evaluator at `envoy_filter::rbac` inherits the bound.
/// Per phase-10 SPEC §3 D2.
pub(crate) const RBAC_TREE_MAX_DEPTH: u32 = 16;

/// The request-side and response-side mutation lists. Both default to empty
/// (`mutations: {}` is legal — a no-op filter).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mutations {
    #[serde(default)]
    pub request_mutations: Vec<HeaderMutationEntry>,
    #[serde(default)]
    pub response_mutations: Vec<HeaderMutationEntry>,
}

/// One mutation entry. Envoy's proto is a `oneof` (append / remove); 07.2
/// supports only `append`. `#[serde(deny_unknown_fields)]` rejects `remove`
/// (and any other oneof arm) at parse time.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderMutationEntry {
    pub append: HeaderValueOption,
}

/// `HeaderValueOption` — a header key/value plus the append action.
///
/// `Clone` added at phase 09 to allow the LocalRateLimitConfig
/// (which embeds `Vec<HeaderValueOption>` in `response_headers_to_add`)
/// to derive `Clone`. The change is additive; HeaderMutation call sites
/// don't clone HeaderValueOption.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderValueOption {
    pub header: HeaderValue,
    pub append_action: AppendAction,
}

/// `HeaderValue` — the literal header key + value.
///
/// `Clone` added at phase 09 — see `HeaderValueOption` doc-comment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HeaderValue {
    pub key: String,
    pub value: String,
}

/// `AppendAction` — Envoy's wire form uses SCREAMING_SNAKE_CASE. 07.2 supports
/// `APPEND_IF_EXISTS_OR_ADD` + `OVERWRITE_IF_EXISTS_OR_ADD` at runtime; the two
/// unsupported variants parse at the schema level so serde does not emit a
/// generic "unknown variant" error — the Task 2 validator rejects them with the
/// typed `ConfigError::UnsupportedHeaderMutationAppendAction` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppendAction {
    AppendIfExistsOrAdd,
    OverwriteIfExistsOrAdd,
    AddIfAbsent,
    OverwriteIfExists,
}

/// HTTP/2 protocol-level tuning knobs, listener-side. Subset of Envoy's
/// `envoy.config.core.v3.Http2ProtocolOptions`. Phase 05.2 ships 4 optional
/// `u32` fields per parent-05 SPEC §6 signpost 2; further fields
/// (allow_connect, allow_metadata, hpack_table_size,
/// override_stream_error_on_invalid_http_message, connection_keepalive, ...)
/// default to RFC-conformant values via the `h2` crate and defer until a
/// fixture or h2spec test forces them. Validator-checked range constraints
/// per RFC 7540 §6.5.2 / §6.9.1 / §6.9.2 land in `validate_hcm`.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Http2ProtocolOptions {
    /// SETTINGS_MAX_CONCURRENT_STREAMS. h2-crate default: 100. No upper bound
    /// per RFC 7540; zero is valid (peer would refuse all stream creation).
    #[serde(default)]
    pub max_concurrent_streams: Option<u32>,

    /// SETTINGS_INITIAL_WINDOW_SIZE. h2-crate default: 65535. Range
    /// [0, 2^31 - 1] per RFC 7540 §6.9.2.
    #[serde(default)]
    pub initial_stream_window_size: Option<u32>,

    /// Connection-level initial window size. h2-crate default: 65535. Range
    /// [0, 2^31 - 1] per RFC 7540 §6.9.1.
    #[serde(default)]
    pub initial_connection_window_size: Option<u32>,

    /// SETTINGS_MAX_FRAME_SIZE. h2-crate default: 16384. Range
    /// [16384, 16777215] per RFC 7540 §6.5.2.
    #[serde(default)]
    pub max_frame_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteConfiguration {
    pub name: String,
    pub virtual_hosts: Vec<VirtualHost>,
    /// 18 L12b (ADR-0049): parse-and-accept. Envoy requires `validate_clusters:
    /// false` on a static route_config that references CDS-supplied clusters
    /// (else it exits: "route: unknown cluster"). envoy-rust parses the field so
    /// the identical fixture configs load, but does NOT honor its literal
    /// runtime-503 semantics — envoy-rust's own reference validation defers
    /// while CDS is configured-but-unloaded and re-enforces post-merge
    /// (Bootstrap::cds_configured_but_unloaded). A route to a cluster in
    /// NEITHER list still fails startup (recorded divergence, BEHAVIOR_CONTRACT).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate_clusters: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VirtualHost {
    pub name: String,
    pub domains: Vec<String>,
    pub routes: Vec<Route>,
    /// 16.1 D1 (phase-16 §6.2 L6): gate for `x-envoy-attempt-count` response
    /// header. Absent → false (header suppressed).
    #[serde(default)]
    pub include_attempt_count_in_response: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// 41 NEW: the route's optional config `name`. Empty (`""`) = unnamed.
    /// Rendered by the `%ROUTE_NAME%` access-log operator (NAMED → the name,
    /// unnamed → absent). Hand-wired into the Deserialize/Serialize below.
    pub name: String,

    /// The match predicate (path + headers; populated by 04.1 + 04.2).
    pub r#match: RouteMatch,

    /// 04.3 NEW: the action to dispatch on a matched request.
    pub action: RouteAction,

    /// 23 D1: per-route filter configuration (e.g. the CORS policy). Empty
    /// when the route carries no `typed_per_filter_config:` map.
    pub typed_per_filter_config: std::collections::BTreeMap<String, PerFilterConfig>,
}

/// 04.3 NEW (under SPEC §3 D2): the action variant a route's HCM router
/// invocation dispatches into. Discrimination is by field-name oneof at the
/// route map level — the route's peer keys are `direct_response: { ... }` OR
/// `route: { ... }`, not nested under a single `action:` key. The hand-rolled
/// `impl<'de> Deserialize` for `Route` (below) detects which peer key is
/// present and constructs the matching variant; both-present and
/// neither-present are errors.
#[derive(Debug, Clone, PartialEq)]
pub enum RouteAction {
    /// Direct-response action — write a static body downstream. Phase 04.1 carryover.
    DirectResponse(DirectResponse),

    /// Route-to-cluster action — proxy through to the named cluster. Phase 04.3 NEW.
    Route(RouteAction_Route),
}

/// 04.3 NEW (under SPEC §3 D2). Names the cluster to forward the matched
/// request to. Future route-action knobs (timeout, weighted clusters,
/// host-rewrite, header manipulations) are deferred (SPEC §4 non-goals).
/// Phase 16 adds `retry_policy` (§6.2 L3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[allow(non_camel_case_types)]
pub struct RouteAction_Route {
    pub cluster: String,
    /// 16.1 D1: optional per-route retry policy. Absent → no retries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    /// 28 Task 4: route-level hash policies for RING_HASH LB. Absent → empty Vec
    /// (the regression-equivalence default; every pre-existing route parses
    /// unchanged). The MVP supports only the `header` specifier; unsupported
    /// specifiers are recognized at parse and rejected as fatal by
    /// `validate_hash_policy` (request-path wiring lands in Task 6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hash_policy: Vec<HashPolicy>,
    /// 30 Task 3 (ADR-0074): optional route-level `metadata_match` — Envoy's
    /// `core.v3.Metadata` (same nested `filter_metadata."envoy.lb"` wire shape as
    /// endpoint `metadata`). Used by subset LB to narrow the eligible endpoint
    /// set. Absent ⇒ `None` (no narrowing). See `LbMetadata`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_match: Option<LbMetadata>,
}

/// 28 Task 4: one entry of Envoy's repeated `RouteAction.hash_policy` field.
/// Each entry is a `oneof policy_specifier`. The phase-28 MVP supports only the
/// `header` specifier; the other known Envoy specifiers (`cookie`,
/// `connection_properties`, `query_parameter`, `filter_state`) are RECOGNIZED at
/// parse time (so the narrowing surfaces as a precise
/// `ConfigError::UnsupportedHashPolicy` via `validate_hash_policy` rather than an
/// opaque serde unknown-field error — mirroring the cluster `HashFunction`
/// recognize-then-reject style). A truly unknown specifier key still fails at
/// serde parse (`deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct HashPolicy {
    /// The `header` specifier — hash on a request header value. The only
    /// MVP-supported source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<HashPolicyHeader>,
    /// Recognized-but-unsupported specifier (rejected by `validate_hash_policy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<serde_yaml::Value>,
    /// Recognized-but-unsupported specifier (rejected by `validate_hash_policy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_properties: Option<serde_yaml::Value>,
    /// Recognized-but-unsupported specifier (rejected by `validate_hash_policy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_parameter: Option<serde_yaml::Value>,
    /// Recognized-but-unsupported specifier (rejected by `validate_hash_policy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_state: Option<serde_yaml::Value>,
}

/// 28 Task 4: the `header` hash-policy source. Mirrors Envoy's
/// `RouteAction.HashPolicy.Header` — hash on the value of the named request
/// header. Deferred sub-fields (e.g. `regex_rewrite`) are rejected by
/// `deny_unknown_fields`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct HashPolicyHeader {
    pub header_name: String,
}

/// 28 Task 4: validate a single route `hash_policy` entry. The MVP supports only
/// the `header` specifier; any other recognized specifier
/// (`cookie` / `connection_properties` / `query_parameter` / `filter_state`) is
/// rejected with a precise fatal `ConfigError::UnsupportedHashPolicy` (ADR-0049
/// all-fatal posture). A policy carrying NO specifier at all is also rejected (an
/// empty oneof can never produce a hash key). Header selection logic lands in
/// Task 6; this validator owns only the config-surface narrowing.
fn validate_hash_policy(hp: &HashPolicy) -> Result<(), crate::ConfigError> {
    let unsupported = if hp.cookie.is_some() {
        Some("cookie")
    } else if hp.connection_properties.is_some() {
        Some("connection_properties")
    } else if hp.query_parameter.is_some() {
        Some("query_parameter")
    } else if hp.filter_state.is_some() {
        Some("filter_state")
    } else if hp.header.is_none() {
        // No specifier set at all — an empty oneof.
        Some("<none>")
    } else {
        None
    };
    if let Some(specifier) = unsupported {
        return Err(crate::ConfigError::UnsupportedHashPolicy {
            specifier: specifier.to_string(),
        });
    }
    Ok(())
}

/// 16.1 D1 (phase-16 §6.2 L3): per-route retry policy shape as parsed from
/// the Envoy YAML config. Deferred fields (`per_try_timeout`, `retry_back_off`,
/// etc.) are rejected automatically by `deny_unknown_fields`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RetryPolicy {
    /// Comma-separated condition tokens (e.g. `"5xx"`, `"connect-failure,5xx"`).
    #[serde(default)]
    pub retry_on: String,
    /// Maximum number of retries. Envoy default 1; resolved at RetryConfig::from (Task 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_retries: Option<u32>,
    /// Additional HTTP status codes to retry on (beyond those named in retry_on).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retriable_status_codes: Vec<u32>,
}

/// 16.1 D2: outcome of a single upstream attempt, used by `RetryConfig::is_retriable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// A response was received (status code is meaningful).
    Response,
    /// The upstream TCP/TLS connection could not be established.
    ConnectFailure,
    /// The upstream reset the connection (e.g. RST_STREAM / TCP RST).
    Reset,
}

/// 16.1 D2: bitmask of enabled `retry_on` conditions parsed from
/// `RetryPolicy::retry_on`. Unknown tokens are silently ignored (L2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetryOn {
    /// `5xx` — any response in 500..=599.
    pub on_5xx: bool,
    /// `gateway-error` — 502, 503, or 504 ONLY (L1).
    pub on_gateway_error: bool,
    /// `connect-failure` — TCP/TLS connect failed.
    pub on_connect_failure: bool,
    /// `reset` — upstream reset the connection.
    pub on_reset: bool,
    /// `retriable-status-codes` — status codes listed in `retriable_status_codes`.
    pub on_retriable_status_codes: bool,
}

/// 16.1 D2: resolved retry configuration derived from a parsed `RetryPolicy`.
/// Downstream crates (envoy-http1, envoy-http2) use this in Tasks 4/5 to drive
/// the retry loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryConfig {
    /// Parsed set of retry conditions.
    pub on: RetryOn,
    /// Maximum retry count (Envoy default 1, §6.2 L3).
    pub num_retries: u32,
    /// Additional HTTP status codes beyond those named in `on` (from
    /// `RetryPolicy::retriable_status_codes`).
    pub retriable_status_codes: Vec<u32>,
}

impl RetryConfig {
    /// 16 Task 4 (§6.2 L7): exponential back-off delay between retry attempts.
    /// Base 25ms, doubling per attempt, capped at 250ms. `attempt` is the
    /// 1-based number of the attempt that just FAILED (i.e. the delay before
    /// attempt N+1): attempt 1 → 25ms, attempt 2 → 50ms, attempt 3 → 100ms,
    /// attempt 4 → 200ms, attempt 5+ → 250ms (cap). Returns `None` for
    /// `attempt == 0` (overflow/safety guard — there is no delay "before"
    /// the first attempt). No jitter (deterministic; timing is never asserted
    /// differentially per L7). Shared by the H1 (Task 4) and H2 (Task 5)
    /// retry loops so the back-off schedule has a single source of truth.
    pub fn backoff(attempt: u32) -> Option<std::time::Duration> {
        if attempt == 0 {
            return None;
        }
        // 25ms * 2^(attempt-1), saturating, capped at 250ms.
        let shift = attempt - 1;
        let ms = if shift >= 4 {
            250
        } else {
            (25u64 << shift).min(250)
        };
        Some(std::time::Duration::from_millis(ms))
    }

    /// Classify whether `status`/`outcome` satisfies any enabled retry condition.
    pub fn is_retriable(&self, status: u16, outcome: AttemptOutcome) -> bool {
        match outcome {
            AttemptOutcome::ConnectFailure => self.on.on_connect_failure,
            AttemptOutcome::Reset => self.on.on_reset,
            AttemptOutcome::Response => {
                (self.on.on_5xx && (500..=599).contains(&status))
                    || (self.on.on_gateway_error && matches!(status, 502..=504))
                    || (self.on.on_retriable_status_codes
                        && self.retriable_status_codes.contains(&(status as u32)))
            }
        }
    }
}

impl From<&RetryPolicy> for RetryConfig {
    /// Build a `RetryConfig` from a parsed `RetryPolicy`.
    ///
    /// Unknown `retry_on` tokens are silently ignored (§6.2 L2).
    /// `num_retries` defaults to 1 when absent (§6.2 L3).
    fn from(p: &RetryPolicy) -> Self {
        let mut on = RetryOn::default();
        for tok in p
            .retry_on
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            match tok {
                "5xx" => on.on_5xx = true,
                "gateway-error" => on.on_gateway_error = true,
                "connect-failure" => on.on_connect_failure = true,
                "reset" => on.on_reset = true,
                "retriable-status-codes" => on.on_retriable_status_codes = true,
                _ => {} // L2: unrecognized tokens silently ignored
            }
        }
        RetryConfig {
            on,
            num_retries: p.num_retries.unwrap_or(1),
            retriable_status_codes: p.retriable_status_codes.clone(),
        }
    }
}

/// 04.3 NEW: hand-rolled because Envoy's `Route` schema uses a field-name
/// oneof for the action variant — `direct_response: { ... }` and `route: { ... }`
/// are peers of `match: { ... }` at the same map level, not nested under a
/// shared discriminator key. `#[serde(tag = "...")]` doesn't model field-name
/// discrimination, and `#[serde(untagged)]` would silently pick the first
/// parsing variant. Mirrors the 04.2 HeaderMatcher visitor pattern.
impl<'de> serde::Deserialize<'de> for Route {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Route;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a Route map with `match` and exactly one of `direct_response` or `route`",
                )
            }

            fn visit_map<M>(self, mut map: M) -> Result<Route, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut name: Option<String> = None;
                let mut r#match: Option<RouteMatch> = None;
                let mut direct_response: Option<DirectResponse> = None;
                let mut route_action: Option<RouteAction_Route> = None;
                let mut typed_per_filter_config: Option<
                    std::collections::BTreeMap<String, PerFilterConfig>,
                > = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if name.is_some() {
                                return Err(M::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value::<String>()?);
                        }
                        "match" => {
                            if r#match.is_some() {
                                return Err(M::Error::duplicate_field("match"));
                            }
                            r#match = Some(map.next_value::<RouteMatch>()?);
                        }
                        "direct_response" => {
                            if direct_response.is_some() {
                                return Err(M::Error::duplicate_field("direct_response"));
                            }
                            direct_response = Some(map.next_value::<DirectResponse>()?);
                        }
                        "route" => {
                            if route_action.is_some() {
                                return Err(M::Error::duplicate_field("route"));
                            }
                            route_action = Some(map.next_value::<RouteAction_Route>()?);
                        }
                        "typed_per_filter_config" => {
                            if typed_per_filter_config.is_some() {
                                return Err(M::Error::duplicate_field("typed_per_filter_config"));
                            }
                            typed_per_filter_config = Some(
                                map.next_value::<std::collections::BTreeMap<
                                    String,
                                    PerFilterConfig,
                                >>()?,
                            );
                        }
                        other => {
                            return Err(M::Error::unknown_field(
                                other,
                                &[
                                    "name",
                                    "match",
                                    "direct_response",
                                    "route",
                                    "typed_per_filter_config",
                                ],
                            ));
                        }
                    }
                }

                let r#match = r#match.ok_or_else(|| M::Error::missing_field("match"))?;
                let action = match (direct_response, route_action) {
                    (Some(_), Some(_)) => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response` or `route`; \
                             both are present",
                        ));
                    }
                    (None, None) => {
                        return Err(M::Error::custom(
                            "Route must carry exactly one of `direct_response` or `route`; \
                             neither is present",
                        ));
                    }
                    (Some(dr), None) => RouteAction::DirectResponse(dr),
                    (None, Some(ar)) => RouteAction::Route(ar),
                };

                Ok(Route {
                    name: name.unwrap_or_default(),
                    r#match,
                    action,
                    typed_per_filter_config: typed_per_filter_config.unwrap_or_default(),
                })
            }
        }

        deserializer.deserialize_map(V)
    }
}

impl serde::Serialize for Route {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let len = 2
            + usize::from(!self.name.is_empty())
            + usize::from(!self.typed_per_filter_config.is_empty());
        let mut map = serializer.serialize_map(Some(len))?;
        if !self.name.is_empty() {
            map.serialize_entry("name", &self.name)?;
        }
        map.serialize_entry("match", &self.r#match)?;
        match &self.action {
            RouteAction::DirectResponse(dr) => map.serialize_entry("direct_response", dr)?,
            RouteAction::Route(ar) => map.serialize_entry("route", ar)?,
        }
        if !self.typed_per_filter_config.is_empty() {
            map.serialize_entry("typed_per_filter_config", &self.typed_per_filter_config)?;
        }
        map.end()
    }
}

impl serde::Serialize for RouteAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // RouteAction is an internal discriminator; when serialized inline
        // as part of Route, Route::serialize emits the field key directly.
        // This impl covers any direct serialization use.
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            RouteAction::DirectResponse(dr) => map.serialize_entry("direct_response", dr)?,
            RouteAction::Route(ar) => map.serialize_entry("route", ar)?,
        }
        map.end()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RouteMatch {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub headers: Vec<HeaderMatcher>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectResponse {
    pub status: u16,
    pub body: DataSource,
}

/// Half-open i64 range. Validator rejects start >= end with
/// ConfigError::InvalidInt64Range. Phase 04.2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Int64Range {
    pub start: i64,
    pub end: i64,
}

/// 12.1 (parent-12 D1): per-cluster active HTTP health check. Phase-12 supports
/// exactly 0 or 1 entry per cluster, HTTP-only (the validator rejects >1 and
/// non-HTTP checkers). Reuses `parse_duration` for `timeout`/`interval` and
/// `Int64Range` for `expected_statuses`. The probe task that consumes this lands
/// in 12.2 (the `envoy-health` crate).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheck {
    /// Per-probe response timeout; parsed via `parse_duration` (integer s/ms/us).
    pub timeout: String,
    /// Interval between probes; parsed via `parse_duration`.
    pub interval: String,
    /// Consecutive successes to mark an endpoint Healthy.
    pub healthy_threshold: u32,
    /// Consecutive failures to mark an endpoint Unhealthy.
    pub unhealthy_threshold: u32,
    /// The HTTP checker. Optional at the schema level so a config omitting it
    /// (or carrying a deferred `custom_health_check`, which `deny_unknown_fields`
    /// rejects) surfaces as `ConfigError::UnsupportedHealthCheckType` at
    /// validate time rather than a bare serde missing-field error. The validator
    /// requires at most one of http/tcp/grpc present (see `grpc_health_check`).
    #[serde(default)]
    pub http_health_check: Option<HttpHealthCheck>,
    /// 68 (ADR-0136): the TCP checker. Optional at the schema level, alongside
    /// `http_health_check`/`grpc_health_check`; the validator rejects more than
    /// one present (the upstream `health_checker` oneof, `MultipleHealthCheckers`)
    /// and NEITHER present (`UnsupportedHealthCheckType`).
    #[serde(default)]
    pub tcp_health_check: Option<TcpHealthCheck>,
    /// 69 (ADR-0138/0139): the gRPC checker. Optional at the schema level,
    /// alongside `http_health_check`/`tcp_health_check`; the validator
    /// rejects more than one present (the upstream `health_checker` oneof)
    /// and requires the cluster to be H2 when this is set.
    #[serde(default)]
    pub grpc_health_check: Option<GrpcHealthCheck>,
}

/// 69 (ADR-0138/0139): the gRPC checker sub-message
/// (`envoy.config.core.v3.HealthCheck.GrpcHealthCheck`). All fields optional:
/// `service_name` empty ⇒ probe the OVERALL server (gRPC service name "").
/// `initial_metadata` accepted for schema completeness; the probe does not
/// thread it (MINIMAL support per SPEC §2.2 — unobservable in fixture 0075).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct GrpcHealthCheck {
    pub service_name: String,
    pub authority: String,
    pub initial_metadata: Vec<HeaderValueOption>,
}

/// 12.1 (parent-12 D1): the HTTP health-check probe shape.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpHealthCheck {
    /// REQUIRED probe path (e.g. `/healthz`); validator rejects empty.
    pub path: String,
    /// OPTIONAL `:authority`/`Host` on the probe; defaults to the cluster name
    /// per upstream (§6.2 item-5). Consumed by the 12.2 probe task.
    #[serde(default)]
    pub host: Option<String>,
    /// OPTIONAL accepted status ranges; default = exactly 200 (§6.2 item-5).
    /// Reuses `Int64Range` (half-open `[start, end)`).
    #[serde(default)]
    pub expected_statuses: Vec<Int64Range>,
}

/// 68 (ADR-0136/0137): the active TCP health-check probe shape
/// (`envoy.config.core.v3.HealthCheck.TcpHealthCheck`). Empty ⇒ connection-only.
/// `send` (optional) is written once after connect; `receive` (repeated) is
/// scanned as a contiguous substring in the inbound bytes (ADR-0137 PV-3).
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields, default)]
pub struct TcpHealthCheck {
    pub send: Option<HealthCheckPayload>,
    pub receive: Vec<HealthCheckPayload>,
}

/// 68 (ADR-0136/0137): a `tcp_health_check` `send`/`receive` payload — an
/// `envoy.config.core.v3.HealthCheck.Payload` oneof `{ text: <hex> | binary:
/// <base64> }`. Modeled as two serde Options (the bootstrap oneof-as-two-Options
/// precedent); `decode()` yields the raw bytes fail-loud (ADR-0137 PV-1).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HealthCheckPayload {
    /// Hex-encoded bytes (upstream `Payload.text`). Odd-length / non-hex → fatal.
    #[serde(default)]
    pub text: Option<String>,
    /// Base64-encoded bytes (upstream `Payload.binary`).
    #[serde(default)]
    pub binary: Option<String>,
}

/// 68: `HealthCheckPayload::decode` failure — mapped to a `ConfigError` by the
/// validator (Task 3) and `.expect()`ed by the probe (Task 4, defense-in-depth,
/// the `parse_duration` precedent).
#[derive(Debug, PartialEq)]
pub enum PayloadDecodeError {
    /// `text` was odd-length or contained a non-hex digit. Carries the offending string.
    InvalidHex(String),
    /// `binary` was not valid base64. Carries the offending string.
    InvalidBase64(String),
    /// Neither `text` nor `binary` set, OR both set (the `Payload` oneof requires exactly one).
    Empty,
}

impl HealthCheckPayload {
    /// 68 (ADR-0137 PV-1): decode to raw bytes. Native fail-loud (byte-parity waived).
    pub fn decode(&self) -> Result<Vec<u8>, PayloadDecodeError> {
        match (&self.text, &self.binary) {
            (Some(hex), None) => {
                decode_hex(hex).ok_or_else(|| PayloadDecodeError::InvalidHex(hex.clone()))
            }
            (None, Some(b64)) => {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .map_err(|_| PayloadDecodeError::InvalidBase64(b64.clone()))
            }
            _ => Err(PayloadDecodeError::Empty),
        }
    }
}

/// 68: hand-rolled hex decode (the `from_str_radix` precedent at
/// `crates/envoy-http1/src/client.rs:631`). Returns `None` on odd length or a
/// non-hex digit. Kept private; `HealthCheckPayload::decode` is the entry point.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Some(out)
}

/// 12.1 (parent-12 D1): the subset of `common_lb_config` phase-12 consumes.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonLbConfig {
    /// Default 50% per upstream; `{ value: 0 }` disables panic routing.
    #[serde(default)]
    pub healthy_panic_threshold: Option<Percent>,
}

/// 12.1 (parent-12 D1): upstream `type.v3.Percent { value: double }` (§6.2 item-3).
/// Distinct from the phase-11 `FractionalPercent` (numerator/denominator).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Percent {
    pub value: f64,
}

/// 13.1 D1 (parent-13 D1): per-cluster circuit-breaker thresholds. Phase-13
/// added `thresholds[0].{priority?, max_connections?}`; phase-15 added
/// `max_pending_requests` (accepts `0` only); phase-17 added `max_requests`,
/// `max_retries`, and `track_remaining`. The still-deferred fields
/// (`max_connection_pools`, `retry_budget`) are rejected by `deny_unknown_fields`.
/// The validator at `validate_circuit_breakers` (Task 2) enforces at-most-one
/// entry + DEFAULT-only priority + non-zero max_connections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CircuitBreakers {
    #[serde(default)]
    pub thresholds: Vec<Thresholds>,
}

/// 13.1 D1: a single circuit-breaker threshold entry. See `CircuitBreakers`.
/// 15 D1: `max_pending_requests` added — accepts `0` ONLY (the no-queue carve-out;
/// matches Envoy's reject-on-establish behavior per ADR-0043 §6.2 finding 1).
/// `max_pending_requests > 0` (the pending-request queue) is rejected by the validator
/// and deferred.
/// 17 D1: `max_requests`, `max_retries`, `track_remaining` added (circuit-breaker budget
/// caps). `0` is a VALID config for both caps (always-open-breaker semantic per ADR-0047
/// §L1/L2). Envoy defaults (max_retries 3 / max_requests 1024) are resolved at
/// `Cluster::from_bootstrap` (Task 3), NOT here — schema keeps `Option<u32>`.
/// Still-deferred fields (`max_connection_pools`, `retry_budget`) remain rejected by
/// `deny_unknown_fields`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<RoutingPriority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pending_requests: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_requests: Option<u32>, // 17 D1: request-budget cap (0 = always-open; L2)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>, // 17 D1: retry-budget cap (0 = always-open; L1)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_remaining: Option<bool>, // 17 D1: emit remaining_* gauges (L8)
}

/// 13.1 D1: Envoy `RoutingPriority` enum. Phase-13 supports DEFAULT only; the
/// validator rejects HIGH explicitly (the only other variant in upstream
/// Envoy v1.33). Serializes/deserializes as `"DEFAULT"` / `"HIGH"` per the
/// upstream proto JSON enum convention.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingPriority {
    Default,
    High,
}

/// 14.1 D1 (parent-14 D1): per-cluster outlier-detection configuration.
/// `None` means outlier detection is disabled for the cluster — the per-endpoint
/// `EndpointEjection` state machine is NOT constructed, `Cluster::pick()` short-
/// circuits to the 12.1 health-only filter, and no outlier-detection stats register.
///
/// Phase-14 minimum-viable scope: `consecutive_5xx` + `consecutive_gateway_failure`
/// detectors only. The following parent-§4 deferred fields are rejected by
/// `deny_unknown_fields` per ADR-0041 §6.2 item-1 (Envoy v1.33.0 accepts them; envoy-rust
/// at phase-14 scope does not):
///   - `success_rate_*` (success_rate_minimum_hosts, success_rate_request_volume,
///     success_rate_stdev_factor)
///   - `failure_percentage_*` (failure_percentage_threshold,
///     failure_percentage_minimum_hosts, failure_percentage_request_volume)
///   - `consecutive_local_origin_failure`
///   - `split_external_local_origin_errors`
///   - `enforcing_*` (enforcing_consecutive_5xx, enforcing_consecutive_gateway_failure,
///     enforcing_success_rate, enforcing_failure_percentage, etc.)
///   - `max_ejection_time` + `max_ejection_time_jitter`
///
/// Envoy v3 defaults (§6.2 item-1, captured at parent-14 state-2 split commit):
/// `consecutive_5xx=5, consecutive_gateway_failure=5, interval=10s,
/// base_ejection_time=30s, max_ejection_percent=10`.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutlierDetection {
    /// Threshold of consecutive 5xx responses that triggers an ejection (Envoy default 5).
    /// `None` ⇒ the detector is treated as not-configured (no ejections from this detector).
    #[serde(default)]
    pub consecutive_5xx: Option<u32>,
    /// Threshold of consecutive 502/503/504 responses that triggers an ejection
    /// (Envoy default 5). Sibling of `consecutive_5xx`. `None` ⇒ disabled.
    #[serde(default)]
    pub consecutive_gateway_failure: Option<u32>,
    /// Interval between sweeper runs (Envoy default `10s`). Parsed via
    /// `parse_duration` (integer s / ms / us; sub-second decimals rejected).
    #[serde(default)]
    pub interval: Option<String>,
    /// Base ejection duration applied at first ejection (Envoy default `30s`). Parsed
    /// via `parse_duration`. Phase-14 does NOT implement Envoy's documented
    /// `base_ejection_time * num_ejections` multiplier — at minimum-viable scope the
    /// effective ejection-duration is exactly `base_ejection_time` regardless of
    /// repeat count (the multiplier defers per parent SPEC §4; §6.2 item-5 finding).
    #[serde(default)]
    pub base_ejection_time: Option<String>,
    /// Maximum percentage of a cluster's endpoints that may be simultaneously ejected
    /// (Envoy default 10). Range `0..=100` enforced by `validate_outlier_detection`.
    /// `0` disables ejection entirely (cap == 0 ⇒ every threshold-crossing increments
    /// `ejections_overflow`).
    #[serde(default)]
    pub max_ejection_percent: Option<u32>,
}

/// Reference to a regex pattern. Held both as the original String (for
/// re-serialization / equality / debugging) and the compiled Arc<regex::Regex>
/// (for cheap clone + zero-cost matching). The compiled form is *not* a serde
/// field; it's filled in by the envoy-config validator after deserialization.
/// Phase 04.2 (under ADR-0021).
///
/// PartialEq compares only the `regex: String` field. The compiled regex has
/// no stable equality (regex::Regex doesn't impl PartialEq), and PartialEq is
/// useful for assert_eq! shape comparisons in tests where pre-validate values
/// (compiled == None) and post-validate values (compiled == Some) should be
/// considered equal if they came from the same pattern.
#[derive(Debug, Clone)]
pub struct SafeRegex {
    pub regex: String,
    /// Filled in by the validator (`crate::bootstrap::validate`). At
    /// deserialization time this is None; after a successful validate() call
    /// it's Some(Arc<regex::Regex>). Consumers (the route walker in HCM via
    /// HeaderMatcher::matches) take the .as_ref().expect("validator ensured
    /// compiled") shape, mirroring phase 02.1's "validator ensured cluster
    /// present" precedent.
    pub compiled: Option<std::sync::Arc<regex::Regex>>,
}

impl PartialEq for SafeRegex {
    fn eq(&self, other: &Self) -> bool {
        self.regex == other.regex
    }
}

/// Hand-rolled Deserialize: only reads `regex: String`; sets `compiled: None`.
/// The validator extension (Task 5) fills the compiled form.
///
/// The hand-rolled form (rather than `#[derive(Deserialize)] +
/// `#[serde(skip, default)]`) is chosen to establish the visitor pattern that
/// Tasks 3 and 4 reuse for `StringMatcher` and `HeaderMatcher` field-name
/// oneof discrimination — where `#[serde(untagged)]` would silently pick the
/// first parsing variant and `#[serde(tag = "...")]` only models a fixed
/// discriminator-key shape. Landing the visitor pattern here establishes the
/// template + makes the two-phase init contract explicit: this visitor always
/// produces `compiled: None`; the validator fills it.
impl<'de> serde::Deserialize<'de> for SafeRegex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = SafeRegex;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a SafeRegex map with a `regex: String` field")
            }
            fn visit_map<M>(self, mut map: M) -> Result<SafeRegex, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut regex: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "regex" => {
                            if regex.is_some() {
                                return Err(M::Error::duplicate_field("regex"));
                            }
                            regex = Some(map.next_value::<String>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, &["regex"]));
                        }
                    }
                }
                let regex = regex.ok_or_else(|| M::Error::missing_field("regex"))?;
                Ok(SafeRegex {
                    regex,
                    compiled: None,
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}

impl serde::Serialize for SafeRegex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("regex", &self.regex)?;
        map.end()
    }
}

/// Envoy's modern generic StringMatcher (proto:
/// `envoy.type.matcher.v3.StringMatcher`). Field-name oneof shape: the
/// discriminator is *which* of `exact` / `prefix` / `suffix` / `safe_regex` /
/// `contains` is the present key. `ignore_case` is a peer of the mode key
/// (not a per-variant field) controlling case sensitivity of the value match.
/// Defaults to false. Has no effect on the SafeRegex variant per Envoy proto
/// (regex callers express case insensitivity via the `(?i)` inline flag).
/// Phase 04.2.
#[derive(Debug, Clone, PartialEq)]
pub struct StringMatcher {
    pub mode: StringMatcherMode,
    pub ignore_case: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringMatcherMode {
    /// `exact: <string>`.
    Exact(String),
    /// `prefix: <string>`.
    Prefix(String),
    /// `suffix: <string>`.
    Suffix(String),
    /// `safe_regex: { regex: "<pattern>" }`.
    SafeRegex(SafeRegex),
    /// `contains: <string>` — substring match. Only reachable through
    /// HeaderMatcherMode::StringMatch(StringMatcher::Contains(...)); there is
    /// no top-level HeaderMatcherMode::ContainsMatch (Envoy v1.33.0 only
    /// supports Contains via the modern string_match field; SPEC §6 signpost 8).
    Contains(String),
}

/// Hand-rolled Deserialize for the field-name oneof shape — same template
/// Task 2 established for `SafeRegex`. Tasks 3 and 4 share this approach
/// because `#[serde(untagged)]` would silently pick the first parsing variant
/// and `#[serde(tag = "...")]` only models a fixed discriminator-key shape;
/// neither fits Envoy's "exactly one of N mode keys" semantics.
impl<'de> serde::Deserialize<'de> for StringMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        const MODE_KEYS: &[&str] = &["exact", "prefix", "suffix", "safe_regex", "contains"];
        const ALL_KEYS: &[&str] = &[
            "exact",
            "prefix",
            "suffix",
            "safe_regex",
            "contains",
            "ignore_case",
        ];

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StringMatcher;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a StringMatcher map with exactly one mode key plus optional ignore_case",
                )
            }
            fn visit_map<M>(self, mut map: M) -> Result<StringMatcher, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut mode: Option<StringMatcherMode> = None;
                let mut ignore_case: Option<bool> = None;

                fn set_mode<E: Error>(
                    slot: &mut Option<StringMatcherMode>,
                    new: StringMatcherMode,
                ) -> Result<(), E> {
                    if slot.is_some() {
                        return Err(E::custom(
                            "StringMatcher: multiple mode keys (each variant is mutually exclusive)",
                        ));
                    }
                    *slot = Some(new);
                    Ok(())
                }

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "exact" => set_mode(
                            &mut mode,
                            StringMatcherMode::Exact(map.next_value::<String>()?),
                        )?,
                        "prefix" => set_mode(
                            &mut mode,
                            StringMatcherMode::Prefix(map.next_value::<String>()?),
                        )?,
                        "suffix" => set_mode(
                            &mut mode,
                            StringMatcherMode::Suffix(map.next_value::<String>()?),
                        )?,
                        "safe_regex" => set_mode(
                            &mut mode,
                            StringMatcherMode::SafeRegex(map.next_value::<SafeRegex>()?),
                        )?,
                        "contains" => set_mode(
                            &mut mode,
                            StringMatcherMode::Contains(map.next_value::<String>()?),
                        )?,
                        "ignore_case" => {
                            if ignore_case.is_some() {
                                return Err(M::Error::duplicate_field("ignore_case"));
                            }
                            ignore_case = Some(map.next_value::<bool>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, ALL_KEYS));
                        }
                    }
                }
                let mode = mode.ok_or_else(|| {
                    M::Error::custom(format!(
                        "StringMatcher: missing mode key (expected one of {MODE_KEYS:?})"
                    ))
                })?;
                Ok(StringMatcher {
                    mode,
                    ignore_case: ignore_case.unwrap_or(false),
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}

impl serde::Serialize for StringMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        // 2 entries: 1 for mode key + 1 for ignore_case (always emitted per
        // lock-in #8: Serialize mirrors the YAML input verbatim; bool fields
        // emit unconditionally regardless of value).
        let mut map = serializer.serialize_map(Some(2))?;
        match &self.mode {
            StringMatcherMode::Exact(v) => map.serialize_entry("exact", v)?,
            StringMatcherMode::Prefix(v) => map.serialize_entry("prefix", v)?,
            StringMatcherMode::Suffix(v) => map.serialize_entry("suffix", v)?,
            StringMatcherMode::SafeRegex(sr) => map.serialize_entry("safe_regex", sr)?,
            StringMatcherMode::Contains(v) => map.serialize_entry("contains", v)?,
        }
        map.serialize_entry("ignore_case", &self.ignore_case)?;
        map.end()
    }
}

/// One header-matching predicate. AND-combined with sibling HeaderMatchers
/// in `RouteMatch.headers` per Envoy v1.33.0 default `headers_match_options:
/// ALL`. Phase 04.2.
#[derive(Debug, Clone, PartialEq)]
pub struct HeaderMatcher {
    /// Header name. Matched case-insensitively against the request's header
    /// names per HTTP/1.1 RFC 7230 §3.2. Empty string is rejected by the
    /// validator with ConfigError::EmptyHeaderName.
    pub name: String,
    /// The mode discriminator. The Envoy proto uses field-name oneof shape
    /// (the discriminator is *which* of the seven mode fields is present);
    /// serde tagged-enum doesn't directly model this, so the parsed form goes
    /// through a hand-rolled Deserialize impl that inspects the YAML mapping
    /// keys and dispatches to the matching variant. SPEC §6 signpost 1.
    pub mode: HeaderMatcherMode,
    /// If true, the entire mode-specific match result is inverted (XOR after
    /// the mode match runs, before AND-combination across sibling
    /// HeaderMatchers). SPEC §6 signpost 5.
    pub invert_match: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderMatcherMode {
    /// `exact_match: <string>` — value equals literal (case-sensitive on the
    /// value; the header name match is always case-insensitive per HTTP/1.1).
    ExactMatch(String),
    /// `prefix_match: <string>` — value starts with literal.
    PrefixMatch(String),
    /// `suffix_match: <string>` — value ends with literal.
    SuffixMatch(String),
    /// `safe_regex_match: { regex: "<pattern>" }` — value matches the regex.
    /// Compiled at config-load time into Arc<regex::Regex>; the validator
    /// rejects unparseable patterns with ConfigError::InvalidRegex.
    SafeRegexMatch(SafeRegex),
    /// `range_match: { start: <i64>, end: <i64> }` — value parses as i64
    /// (decimal) and falls in [start, end). Non-parseable values fail the
    /// match (NOT an error). SPEC §6 signpost 6.
    RangeMatch(Int64Range),
    /// `present_match: <bool>` — header presence (true) or "no presence
    /// requirement" (false; SPEC §6 signpost 7 for the subtle false semantics).
    PresentMatch(bool),
    /// `string_match: <StringMatcher>` — Envoy's modern generic tagged-union
    /// (the only path to Contains; SPEC §6 signpost 8).
    StringMatch(StringMatcher),
}

impl<'de> serde::Deserialize<'de> for HeaderMatcher {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{Error, MapAccess, Visitor};
        use std::fmt;

        const MODE_KEYS: &[&str] = &[
            "exact_match",
            "prefix_match",
            "suffix_match",
            "safe_regex_match",
            "range_match",
            "present_match",
            "string_match",
        ];
        const ALL_KEYS: &[&str] = &[
            "name",
            "exact_match",
            "prefix_match",
            "suffix_match",
            "safe_regex_match",
            "range_match",
            "present_match",
            "string_match",
            "invert_match",
        ];

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = HeaderMatcher;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "a HeaderMatcher map with `name`, exactly one mode key, and optional invert_match",
                )
            }
            fn visit_map<M>(self, mut map: M) -> Result<HeaderMatcher, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut name: Option<String> = None;
                let mut mode: Option<HeaderMatcherMode> = None;
                let mut invert_match: Option<bool> = None;

                fn set_mode<E: Error>(
                    slot: &mut Option<HeaderMatcherMode>,
                    new: HeaderMatcherMode,
                ) -> Result<(), E> {
                    if slot.is_some() {
                        return Err(E::custom(
                            "HeaderMatcher: multiple mode keys (each variant is mutually exclusive)",
                        ));
                    }
                    *slot = Some(new);
                    Ok(())
                }

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "name" => {
                            if name.is_some() {
                                return Err(M::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value::<String>()?);
                        }
                        "exact_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::ExactMatch(map.next_value::<String>()?),
                        )?,
                        "prefix_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::PrefixMatch(map.next_value::<String>()?),
                        )?,
                        "suffix_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::SuffixMatch(map.next_value::<String>()?),
                        )?,
                        "safe_regex_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::SafeRegexMatch(map.next_value::<SafeRegex>()?),
                        )?,
                        "range_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::RangeMatch(map.next_value::<Int64Range>()?),
                        )?,
                        "present_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::PresentMatch(map.next_value::<bool>()?),
                        )?,
                        "string_match" => set_mode(
                            &mut mode,
                            HeaderMatcherMode::StringMatch(map.next_value::<StringMatcher>()?),
                        )?,
                        "invert_match" => {
                            if invert_match.is_some() {
                                return Err(M::Error::duplicate_field("invert_match"));
                            }
                            invert_match = Some(map.next_value::<bool>()?);
                        }
                        other => {
                            return Err(M::Error::unknown_field(other, ALL_KEYS));
                        }
                    }
                }

                let name = name.ok_or_else(|| M::Error::missing_field("name"))?;
                let mode = mode.ok_or_else(|| {
                    M::Error::custom(format!(
                        "HeaderMatcher: missing mode key (expected one of {MODE_KEYS:?})"
                    ))
                })?;
                Ok(HeaderMatcher {
                    name,
                    mode,
                    invert_match: invert_match.unwrap_or(false),
                })
            }
        }
        deserializer.deserialize_map(V)
    }
}

impl serde::Serialize for HeaderMatcher {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        // 3 entries: name + mode key + invert_match (always emitted per
        // lock-in #8: Serialize mirrors the YAML input verbatim; bool fields
        // emit unconditionally regardless of value).
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("name", &self.name)?;
        match &self.mode {
            HeaderMatcherMode::ExactMatch(v) => map.serialize_entry("exact_match", v)?,
            HeaderMatcherMode::PrefixMatch(v) => map.serialize_entry("prefix_match", v)?,
            HeaderMatcherMode::SuffixMatch(v) => map.serialize_entry("suffix_match", v)?,
            HeaderMatcherMode::SafeRegexMatch(sr) => map.serialize_entry("safe_regex_match", sr)?,
            HeaderMatcherMode::RangeMatch(r) => map.serialize_entry("range_match", r)?,
            HeaderMatcherMode::PresentMatch(b) => map.serialize_entry("present_match", b)?,
            HeaderMatcherMode::StringMatch(sm) => map.serialize_entry("string_match", sm)?,
        }
        map.serialize_entry("invert_match", &self.invert_match)?;
        map.end()
    }
}

pub(crate) fn validate(bootstrap: &mut Bootstrap) -> Result<(), crate::ConfigError> {
    // 19 D3 (Correction 1): the single-listener limitation applies to the
    // MERGED list (static + dynamic together ≤ 1; the pre-existing limitation
    // is preserved, not lifted — SPEC §4).
    let total_listeners = bootstrap.all_listeners().count();
    if total_listeners > 1 {
        return Err(crate::ConfigError::TooManyListeners(total_listeners));
    }
    // 19 D3 (Correction 2): the no-runtime gate DEFERS while lds_config is
    // configured-but-unloaded (listeners may arrive from the LDS file); the
    // post-merge re-validation (load_dynamic_resources → validate()) re-enforces.
    if bootstrap.admin.is_none() && total_listeners == 0 && !bootstrap.lds_configured_but_unloaded()
    {
        return Err(crate::ConfigError::NoRuntime);
    }

    // 18 D1 (L8, ADR-0049): resource_api_version must be "V3" or absent. 19 D1:
    // the same check now covers the new lds_config field — iterate both
    // Option<ConfigSource> sources.
    for cs in [
        bootstrap
            .dynamic_resources
            .as_ref()
            .and_then(|dr| dr.cds_config.as_ref()),
        bootstrap
            .dynamic_resources
            .as_ref()
            .and_then(|dr| dr.lds_config.as_ref()),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(v) = cs.resource_api_version.as_deref()
            && v != "V3"
        {
            return Err(crate::ConfigError::UnsupportedResourceApiVersion(
                v.to_string(),
            ));
        }
    }

    // 18 D1/D3: while CDS is configured-but-unloaded, cluster-reference checks
    // DEFER (the references may resolve against the CDS file once
    // load_dynamic_resources runs). Captured here as a `bool` before the
    // `&mut` listener loop below so the reference-check sites can read it
    // without conflicting with the listener borrow.
    let defer_cluster_refs = bootstrap.cds_configured_but_unloaded();

    // 18 D3: snapshot the EFFECTIVE cluster list (static + dynamic) BEFORE the
    // `&mut bootstrap.static_resources.listeners` loop below — the reference
    // checks (UnknownCluster + the H2-from-H1 gate) must resolve against the
    // merged list so that, post-CDS-load re-validation, a route to a legitimately
    // CDS-loaded cluster does not wrongly fail. Collected as `&Cluster` refs
    // (NOT cloned) — see the borrow note below.
    // When `dynamic_clusters` is None this is exactly `static_resources.clusters`
    // (existing behavior unchanged).
    //
    // We borrow the two cluster fields DIRECTLY (not via `all_clusters(&self)`):
    // `static_resources.clusters` and `dynamic_clusters` are fields disjoint from
    // `static_resources.listeners`, so the borrow checker permits this immutable
    // snapshot to coexist with the `&mut listeners` loop below (a whole-`&self`
    // borrow via `all_clusters()` would NOT — it would conflict). Holds `&Cluster`
    // refs, so no `Cluster: Clone` requirement.
    let effective_clusters: Vec<&Cluster> = bootstrap
        .static_resources
        .clusters
        .iter()
        .chain(bootstrap.dynamic_clusters.iter().flatten())
        .collect();

    // Per-cluster invariants. Each static cluster runs the same gauntlet as
    // dynamically-loaded (CDS) clusters — `validate_cluster` is the single
    // source of truth shared by `validate()` and `cds::parse_cds_file`
    // (18 D2 / SPEC D2; Task-2 Step 3b extraction).
    for cluster in &bootstrap.static_resources.clusters {
        validate_cluster(cluster)?;
    }

    // Per-listener invariants.
    //
    // 19 D3 (§5.3/§5.7): dynamic listeners go through the SAME validation
    // gauntlet as static listeners (HCM shape, route→cluster references against
    // the merged `effective_clusters` snapshot, TLS checks, the H2-from-H1
    // gate). At parse time `dynamic_listeners` is None (the chain is empty); at
    // the post-merge re-validation inside `load_dynamic_resources` it covers the
    // LDS-supplied listeners. `static_resources.listeners` and
    // `dynamic_listeners` are disjoint `Bootstrap` fields (and disjoint from the
    // cluster fields already borrowed by `effective_clusters`), so this chained
    // `&mut` iterator borrows cleanly. When `dynamic_listeners` is None the
    // chain is exactly the static loop (existing behavior unchanged).
    let (static_listeners, dynamic_listeners) = (
        &mut bootstrap.static_resources.listeners,
        &mut bootstrap.dynamic_listeners,
    );
    for listener in static_listeners
        .iter_mut()
        .chain(dynamic_listeners.iter_mut().flatten())
    {
        for (chain_index, chain) in listener.filter_chains.iter_mut().enumerate() {
            // Snapshot per-chain TLS state for `validate_hcm`'s D2.a check
            // (phase 05.2 SPEC §3 — reject `codec_type: HTTP2` on TLS chains).
            // Mirrors the predicate just below: a chain is "TLS" iff its
            // `transport_socket.name == TLS_TRANSPORT_SOCKET`.
            let chain_has_tls = chain
                .transport_socket
                .as_ref()
                .is_some_and(|ts| ts.name == crate::TLS_TRANSPORT_SOCKET);
            if let Some(ts) = chain.transport_socket.as_ref() {
                if ts.name != crate::TLS_TRANSPORT_SOCKET {
                    return Err(crate::ConfigError::UnknownTransportSocketName(
                        ts.name.clone(),
                    ));
                }
                match &ts.typed_config {
                    TransportSocketTypedConfig::Downstream(ctx) => {
                        if ctx.common_tls_context.tls_certificates.is_empty() {
                            return Err(crate::ConfigError::EmptyTlsCertificates {
                                side: "listener",
                            });
                        }
                        for tls_cert in &ctx.common_tls_context.tls_certificates {
                            validate_data_source(
                                &tls_cert.certificate_chain,
                                "tls_certificate.certificate_chain",
                                Required::Filename,
                            )?;
                            validate_data_source(
                                &tls_cert.private_key,
                                "tls_certificate.private_key",
                                Required::Filename,
                            )?;
                        }
                    }
                    TransportSocketTypedConfig::Upstream(_) => {
                        return Err(crate::ConfigError::MismatchedTransportSocketDirection {
                            side: "listener",
                            got: "UpstreamTlsContext",
                        });
                    }
                }
            }
            // 66 (ADR-0123): terminal-filter pre-pass. Runs BEFORE the mutating
            // per-filter loop for two reasons: (a) that loop borrows
            // `chain.filters` mutably (the HCM arm calls `as_mut()`), and we
            // need the length + index here; (b) it reproduces upstream Envoy's
            // error precedence — a chain of [direct_response, echo] reports the
            // TERMINAL error even when the trailing filter is itself malformed.
            let chain_len = chain.filters.len();
            for (idx, filter) in chain.filters.iter().enumerate() {
                if is_terminal_network_filter(&filter.name) && idx + 1 != chain_len {
                    return Err(crate::ConfigError::NetworkFilterNotTerminal {
                        name: filter.name.clone(),
                        position: idx + 1,
                        chain_len,
                    });
                }
            }
            // 67.1 D2 (SPEC R-1): the BILATERAL dual — a non-empty chain must
            // END in a terminal filter. Placed AFTER the scan above so the
            // terminal-not-last error WINS when a chain violates both rules at
            // once (`[echo, rbac]`) — measured upstream precedence, SPEC R-5.
            // An empty `filters: []` chain is ACCEPTED (SPEC R-7, upstream
            // parity, closes M66-5): `last()` is None and this check no-ops.
            //
            // M-4, a diagnostic consequence worth stating: because this check
            // precedes the per-filter allow-list loop below, a chain ending in an
            // UNKNOWN filter name reports `NetworkFilterChainNotTerminated` rather
            // than `UnsupportedFilter` — an unknown name is, by definition, not
            // terminal. Both are fail-loud rejections, so this is a loss of
            // diagnostic precision, not of correctness. Pinned by
            // `rejects_unknown_filter_name`.
            if let Some(last) = chain.filters.last()
                && !is_terminal_network_filter(&last.name)
            {
                return Err(crate::ConfigError::NetworkFilterChainNotTerminated {
                    listener: listener.name.clone(),
                    chain_index,
                    last_filter: last.name.clone(),
                });
            }
            // 67.3 D5/D6 (ADR-0135): the plaintext `[non-terminal, tcp_proxy]`
            // composition is now SUPPORTED (the establishment/data split — a
            // terminal filter connects upstream and relays a server-first banner
            // BEFORE the chain's first-byte gate). Only the TLS-DOWNSTREAM case
            // stays fail-loud: the D6 probe measured that upstream Envoy
            // establishes the upstream at raw-TCP accept (before the handshake) and
            // takes the RBAC verdict on the first DECRYPTED byte — an ordering
            // envoy-rust's TLS handler does not yet reproduce. Owner: CF-67-7.
            //
            // (67.1's fail-loud rejection covered ALL such chains; ADR-0135 narrows
            // it to `chain.transport_socket.is_some()`. Upstream Envoy accepts the
            // config either way, so the TLS rejection is a recorded divergence,
            // ADR-0049 decision-2 (b) — strictly better than a wrong-ordering
            // runtime for a composition we cannot yet reproduce.)
            //
            // Only `tcp_proxy` needs this. `echo` / `http_connection_manager` do no
            // establishment-time work (measured), and `direct_response` bypasses the
            // chain in `envoy-bin` (ADR-0132 decision 2).
            //
            // Placed AFTER both terminal-position checks above so their errors keep
            // winning: `[echo, rbac, tcp_proxy]` still reports terminal-not-last.
            if chain_len >= 2
                && let Some(last) = chain.filters.last()
                && last.name == crate::TCP_PROXY_FILTER
                && chain.transport_socket.is_some()
            {
                let non_terminal = &chain.filters[chain_len - 2];
                return Err(
                    crate::ConfigError::UnsupportedNetworkFilterChainComposition {
                        listener: listener.name.clone(),
                        chain_index,
                        non_terminal: non_terminal.name.clone(),
                        terminal: last.name.clone(),
                    },
                );
            }
            for filter in &mut chain.filters {
                match filter.name.as_str() {
                    crate::ECHO_FILTER => {
                        if filter.typed_config.is_some() {
                            return Err(crate::ConfigError::UnexpectedTypedConfig(
                                crate::ECHO_FILTER,
                            ));
                        }
                    }
                    crate::TCP_PROXY_FILTER => {
                        // TypedConfig is multi-variant (TcpProxy, HCM). The
                        // HCM-on-tcp_proxy-name shape is misconfiguration; we
                        // reject it as MissingTypedConfig (preserving the
                        // pre-04.1 error surface — the typed_config under the
                        // tcp_proxy name is not a TcpProxyConfig).
                        // Read-only: TCP_PROXY validation does not mutate typed_config
                        // (unlike HCM_FILTER's `as_mut()` arm below — `as_ref()` reborrows
                        // a `&mut filter` as `&filter` which Rust permits).
                        let typed = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::TCP_PROXY_FILTER),
                        )?;
                        let TypedConfig::TcpProxy(tp) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(
                                crate::TCP_PROXY_FILTER,
                            ));
                        };
                        let cluster_name = tp.cluster.clone();
                        // 18 D1/D3: defer the reference check while CDS is
                        // configured-but-unloaded; load_dynamic_resources
                        // re-enforces post-merge.
                        if !defer_cluster_refs
                            && !effective_clusters.iter().any(|c| c.name == cluster_name)
                        {
                            return Err(crate::ConfigError::UnknownCluster(cluster_name));
                        }
                    }
                    crate::HCM_FILTER => {
                        let typed = filter
                            .typed_config
                            .as_mut()
                            .ok_or(crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER))?;
                        let TypedConfig::HttpConnectionManager(hcm) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(crate::HCM_FILTER));
                        };
                        validate_hcm(
                            hcm,
                            &effective_clusters,
                            chain_has_tls,
                            &listener.name,
                            defer_cluster_refs,
                        )?;
                    }
                    crate::DIRECT_RESPONSE_FILTER => {
                        // 66 (ADR-0123): the payload lives in typed_config; the
                        // filter is meaningless without it. `response` inside it
                        // is optional (empty payload) per SPEC §0 R-0.7.
                        let typed = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::DIRECT_RESPONSE_FILTER),
                        )?;
                        let TypedConfig::DirectResponse(_) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(
                                crate::DIRECT_RESPONSE_FILTER,
                            ));
                        };
                    }
                    crate::NETWORK_RBAC_FILTER => {
                        // 67.1 D1: read-only — the rbac arm does not mutate
                        // typed_config, so `as_ref()` (not `as_mut()`).
                        let typed = filter.typed_config.as_ref().ok_or(
                            crate::ConfigError::MissingTypedConfig(crate::NETWORK_RBAC_FILTER),
                        )?;
                        let TypedConfig::NetworkRbac(cfg) = typed else {
                            return Err(crate::ConfigError::MissingTypedConfig(
                                crate::NETWORK_RBAC_FILTER,
                            ));
                        };
                        validate_network_rbac_config(cfg, &listener.name)?;
                    }
                    _ => {
                        return Err(crate::ConfigError::UnsupportedFilter(
                            filter.name.clone(),
                            crate::ECHO_FILTER,
                        ));
                    }
                }
            }
        }

        // 03.2: cross-chain rules within each listener. Task 4's
        // envoy-tls::DownstreamTls::from_listener trusts these guarantees per
        // SPEC §3 D1 — it does not re-check them.

        // Rule 1: overlapping SNI. Walk each chain's server_names; build a
        // HashSet<String> of lowercased SNIs; reject on duplicate.
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for chain in &listener.filter_chains {
            if let Some(m) = chain.filter_chain_match.as_ref() {
                for sni in &m.server_names {
                    let lower = sni.to_lowercase();
                    if !seen.insert(lower.clone()) {
                        return Err(crate::ConfigError::MultipleListenersWithOverlappingSni {
                            listener: listener.name.clone(),
                            sni: lower,
                        });
                    }
                }
            }
        }

        // Rule 2: at most one catch-all (empty server_names) chain per listener.
        // A missing filter_chain_match counts as catch-all (matches every connection).
        let catch_all_count = listener
            .filter_chains
            .iter()
            .filter(|c| {
                c.filter_chain_match
                    .as_ref()
                    .map(|m| m.server_names.is_empty())
                    .unwrap_or(true)
            })
            .count();
        if catch_all_count > 1 {
            return Err(crate::ConfigError::MultipleCatchAllFilterChains {
                listener: listener.name.clone(),
            });
        }

        // Rule 3: don't mix TLS and plaintext chains. Only fires when ≥ 2
        // chains exist; mixing requires `tls_inspector` (deferred).
        if listener.filter_chains.len() >= 2 {
            let tls_count = listener
                .filter_chains
                .iter()
                .filter(|c| c.transport_socket.is_some())
                .count();
            if tls_count > 0 && tls_count < listener.filter_chains.len() {
                return Err(
                    crate::ConfigError::MixedTlsAndPlaintextFilterChainsOnListener {
                        listener: listener.name.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

/// 29 (ADR-0072): primality test for `MaglevLbConfig.table_size` (bounded
/// ≤ 5000011, so trial division to sqrt(n) is cheap — sqrt(5000011) ≈ 2236).
/// `0`/`1` are not prime.
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n.is_multiple_of(2) {
        return n == 2;
    }
    let mut d = 3u64;
    while d * d <= n {
        if n.is_multiple_of(d) {
            return false;
        }
        d += 2;
    }
    true
}

/// Run the per-cluster invariant gauntlet on a single cluster. Extracted from
/// `validate()`'s static-cluster loop (Task-2 Step 3b) so that `validate()` and
/// `cds::parse_cds_file` share ONE source of truth: dynamically-loaded (CDS)
/// clusters pass exactly the same checks static clusters do (18 D2 / SPEC D2).
///
/// This covers only the genuinely per-cluster invariants. Cross-cluster checks
/// (duplicate names, cluster-reference resolution from listeners) live outside
/// this function — they require the full cluster list and listener context, so
/// they stay in `validate()` / `validate_hcm` / `load_dynamic_resources`.
pub(crate) fn validate_cluster(cluster: &Cluster) -> Result<(), crate::ConfigError> {
    // 62 D1 (ADR-0119): fail-closed on a malformed upstream idle_timeout, like
    // Envoy's Duration validation. Absent (or absent inner field) is the 60s
    // default and validates trivially — the regression-equivalence path.
    if let Some(opts) = cluster.common_http_protocol_options.as_ref()
        && let Some(raw) = opts.idle_timeout.as_ref()
        && parse_positive_duration(raw).is_none()
    {
        return Err(crate::ConfigError::InvalidClusterIdleTimeout {
            cluster: cluster.name.clone(),
        });
    }
    // 28 D1 (ADR-0069/0070): RING_HASH sub-config validation. Gated to
    // RING_HASH clusters — upstream Envoy accepts-and-ignores a
    // `ring_hash_lb_config` on a non-RING_HASH cluster, so we don't validate it
    // there. Runs BEFORE the EDS early-return below so a RING_HASH EDS cluster's
    // sub-config is still checked at parse time. Both rejections are all-fatal
    // (ADR-0049): MURMUR_HASH_2 is the phase-28 XX_HASH-only narrowing (ADR-0070).
    if cluster.lb_policy == LbPolicy::RingHash
        && let Some(cfg) = cluster.ring_hash_lb_config.as_ref()
    {
        if cfg.hash_function == HashFunction::MurmurHash2 {
            return Err(crate::ConfigError::UnsupportedHashFunction {
                cluster: cluster.name.clone(),
            });
        }
        if cfg.minimum_ring_size > cfg.maximum_ring_size {
            return Err(crate::ConfigError::RingSizeInversion {
                cluster: cluster.name.clone(),
                minimum: cfg.minimum_ring_size,
                maximum: cfg.maximum_ring_size,
            });
        }
    }
    // 29 D1 (ADR-0072): MAGLEV sub-config validation. Gated to MAGLEV clusters —
    // upstream Envoy accepts-and-ignores a `maglev_lb_config` on a non-MAGLEV
    // cluster (the phase-28 ring precedent), so we don't validate it there. Like
    // the RING_HASH block this runs BEFORE the EDS early-return below. Both
    // rejections are all-fatal (ADR-0049). Over-max is checked FIRST so the
    // bounded `is_prime` trial loop never runs on a huge value.
    const MAGLEV_MAX_TABLE_SIZE: u64 = 5_000_011;
    if cluster.lb_policy == LbPolicy::Maglev
        && let Some(cfg) = cluster.maglev_lb_config.as_ref()
    {
        if cfg.table_size > MAGLEV_MAX_TABLE_SIZE {
            return Err(crate::ConfigError::MaglevTableSizeTooLarge {
                cluster: cluster.name.clone(),
                table_size: cfg.table_size,
            });
        }
        if !is_prime(cfg.table_size) {
            return Err(crate::ConfigError::MaglevTableSizeNotPrime {
                cluster: cluster.name.clone(),
                table_size: cfg.table_size,
            });
        }
    }
    // 21 D1 (§5.8; L6): exactly-one-of-and-consistent endpoint source. At PARSE
    // time an EDS cluster has `load_assignment: None` + `eds_cluster_config:
    // Some`. Post-merge the EDS cluster carries BOTH `Some` (the loaded state) —
    // the `(true, _, true)` arm tolerates it. The `LoadAssignmentOnEdsCluster`
    // reject (an inline `load_assignment` on EDS, L6 6a) is NOT here: it would
    // false-positive the post-merge loaded `(Eds, Some, Some)` state. It lives
    // in the parse-time-only `check_endpoint_sources` pass instead.
    let is_eds = cluster.cluster_type == ClusterType::Eds;
    let la_some = cluster.load_assignment.is_some();
    let eds_some = cluster.eds_cluster_config.is_some();
    match (is_eds, la_some, eds_some) {
        (true, _, false) => {
            return Err(crate::ConfigError::MissingEdsClusterConfig {
                cluster: cluster.name.clone(),
            });
        }
        (false, false, _) => {
            return Err(crate::ConfigError::MissingLoadAssignment {
                cluster: cluster.name.clone(),
            });
        }
        (false, _, true) => {
            return Err(crate::ConfigError::EdsConfigOnNonEdsCluster {
                cluster: cluster.name.clone(),
            });
        }
        _ => {}
    }
    // 21 D1: endpoint checks run only when `load_assignment` is present (an EDS
    // cluster pre-merge has `None` — its endpoints are validated post-merge,
    // after `load_dynamic_resources` populates `load_assignment`; this function
    // re-runs then, so the transport_socket / typed_extension / health-check /
    // circuit-breaker / outlier checks below are not skipped permanently).
    let Some(la) = cluster.load_assignment.as_ref() else {
        return Ok(());
    };
    // The name-mismatch check applies to INLINE (non-EDS) clusters only: an EDS
    // cluster's populated CLA `cluster_name` equals `service_name` (L8), NOT the
    // cluster name — re-checking against `cluster.name` would falsely reject.
    if !is_eds && la.cluster_name != cluster.name {
        return Err(crate::ConfigError::LoadAssignmentNameMismatch {
            cluster: cluster.name.clone(),
            assignment: la.cluster_name.clone(),
        });
    }
    let total_endpoints: usize = la.endpoints.iter().map(|le| le.lb_endpoints.len()).sum();
    if total_endpoints == 0 {
        return Err(crate::ConfigError::EmptyClusterEndpoints(
            cluster.name.clone(),
        ));
    }
    if let Some(ts) = cluster.transport_socket.as_ref() {
        if ts.name != crate::TLS_TRANSPORT_SOCKET {
            return Err(crate::ConfigError::UnknownTransportSocketName(
                ts.name.clone(),
            ));
        }
        match &ts.typed_config {
            TransportSocketTypedConfig::Upstream(ctx) => {
                if !ctx.common_tls_context.tls_certificates.is_empty() {
                    return Err(crate::ConfigError::EmptyTlsCertificates { side: "cluster" });
                }
                let vc = ctx
                    .common_tls_context
                    .validation_context
                    .as_ref()
                    .ok_or(crate::ConfigError::MissingValidationContext)?;
                validate_data_source(
                    &vc.trusted_ca,
                    "validation_context.trusted_ca",
                    Required::Filename,
                )?;
                if ctx.sni.is_empty() {
                    return Err(crate::ConfigError::EmptyUpstreamSni);
                }
            }
            TransportSocketTypedConfig::Downstream(_) => {
                return Err(crate::ConfigError::MismatchedTransportSocketDirection {
                    side: "cluster",
                    got: "DownstreamTlsContext",
                });
            }
        }
    }
    // 05.3 NEW per SPEC §3 D2.a: validate cluster-side
    // typed_extension_protocol_options.
    if let Some(teo) = &cluster.typed_extension_protocol_options {
        const EXPECTED_TYPE_URL: &str =
            "type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions";
        if teo.http_protocol_options.type_url != EXPECTED_TYPE_URL {
            return Err(crate::ConfigError::UnsupportedTypedConfigUrl {
                got: teo.http_protocol_options.type_url.clone(),
                expected: EXPECTED_TYPE_URL,
            });
        }
        let ehc = &teo.http_protocol_options.explicit_http_config;
        if ehc.http_protocol_options.is_some() && ehc.http2_protocol_options.is_some() {
            return Err(crate::ConfigError::MutuallyExclusiveExplicitHttpConfig {
                cluster: cluster.name.clone(),
            });
        }
        if let Some(h2_opts) = &ehc.http2_protocol_options {
            validate_http2_protocol_options_ranges(h2_opts)?;
        }
    }
    // 12.1: validate the cluster's active-HC config (HTTP-only, 0-or-1) +
    // common_lb_config panic threshold.
    validate_health_checks(cluster)?;
    validate_circuit_breakers(cluster)?; // 13.1 D2
    validate_outlier_detection(cluster)?; // 14.1 D2
    Ok(())
}

/// Validate a fully-parsed `HttpConnectionManagerConfig` against the phase-04.1
/// surface. SPEC §3 D2 enumerates the rejections this function fires.
///
/// `chain_has_tls` is the enclosing `filter_chain.transport_socket.is_some()`
/// folded down to a bool; phase 05.2 SPEC §3 D2.a uses it to reject
/// `codec_type: HTTP2` on TLS chains (TLS+ALPN+H2 deferred per parent-05 SPEC §4).
///
/// `listener_name` is the enclosing listener's `name` field; phase 06.3 D14.3
/// uses it to name the offending listener in `ConfigError::Http2ClusterFromHttp1Listener`.
fn validate_hcm(
    hcm: &mut HttpConnectionManagerConfig,
    clusters: &[&Cluster],
    chain_has_tls: bool,
    listener_name: &str,
    defer_cluster_refs: bool,
) -> Result<(), crate::ConfigError> {
    // codec_type: AUTO, HTTP1, and HTTP2 are runtime-supported. HTTP3 is
    // rejected pending future work. HTTP2 over TLS is rejected separately
    // below per phase 05.2 SPEC §3 D2.a (TLS+ALPN+H2 deferred per parent-05
    // SPEC §4); the per-chain TLS state is plumbed in via `chain_has_tls`.
    match hcm.codec_type {
        CodecType::AUTO | CodecType::HTTP1 | CodecType::HTTP2 => {}
        CodecType::HTTP3 => {
            return Err(crate::ConfigError::UnsupportedCodecType {
                got: hcm.codec_type,
            });
        }
    }

    // 05.2 NEW — D2.a TLS+HTTP2 rejection.
    if matches!(hcm.codec_type, CodecType::HTTP2) && chain_has_tls {
        return Err(crate::ConfigError::Http2OverTlsNotSupported);
    }

    // 05.2 NEW — D2.b: validate Http2ProtocolOptions ranges per RFC 7540
    // §6.5.2 / §6.9.1 / §6.9.2. Run only if Some; absent = h2-crate defaults.
    // 05.3: hoisted to validate_http2_protocol_options_ranges free function.
    if let Some(opts) = &hcm.http2_protocol_options {
        validate_http2_protocol_options_ranges(opts)?;
    }

    // 06.2 NEW — D2.2: validate access_log entries (name allow-list +
    // non-empty path). Hoisted to a free function for symmetry with the
    // http2_protocol_options pattern.
    validate_access_logs(&hcm.access_log)?;

    // http_filters: cardinality + name + Router-terminal — 07.1 D4.1.
    validate_http_filters(&hcm.http_filters, listener_name)?;

    // 20 D1 (C16): inline route_config validation runs only when present.
    // An rds HCM has route_config: None at parse time (the route table is
    // populated post-merge by load_dynamic_resources, then re-validated). The
    // exactly-one-of check ran at parse time (check_route_sources); this fn
    // never re-checks cardinality, so the post-merge both-Some state is valid.
    let Some(route_config) = hcm.route_config.as_mut() else {
        return Ok(()); // rds HCM, pre-load — nothing inline to validate yet
    };

    // 23 D3 (ADR-0058 / L7): build the set of http_filter names present in
    // this HCM's filter chain ONCE, before the route walk. A route carrying a
    // `typed_per_filter_config` key that is NOT in this set is rejected as
    // startup-fatal (PerRouteConfigForAbsentFilter). Upstream Envoy
    // accepts-and-ignores; envoy-rust diverges to the stricter ADR-0049
    // all-fatal posture (recorded divergence, BEHAVIOR_CONTRACT).
    // Co-located here with the post-merge route walk so that RDS HCMs are
    // checked against their post-merge route table (when route_config is None
    // pre-merge the function already returned Ok above).
    let present_http_filter_names: std::collections::BTreeSet<&str> =
        hcm.http_filters.iter().map(|hf| hf.name.as_str()).collect();

    // route_config: walk virtual_hosts → routes.
    if route_config.virtual_hosts.is_empty() {
        return Err(crate::ConfigError::EmptyVirtualHosts {
            route_config: route_config.name.clone(),
        });
    }
    for vh in &mut route_config.virtual_hosts {
        if vh.domains.is_empty() {
            return Err(crate::ConfigError::EmptyDomains {
                virtual_host: vh.name.clone(),
            });
        }
        for d in &vh.domains {
            if d != "*" && !is_valid_dns_name(d) {
                return Err(crate::ConfigError::UnsupportedDomainMatcher { domain: d.clone() });
            }
        }
        if vh.routes.is_empty() {
            return Err(crate::ConfigError::EmptyRoutes {
                virtual_host: vh.name.clone(),
            });
        }
        for r in &mut vh.routes {
            // RouteMatch: exactly one of {prefix, path} is Some.
            validate_route_match_cardinality(r.r#match.prefix.is_some(), r.r#match.path.is_some())?;
            // 04.3: dispatch on the action variant. DirectResponse keeps its
            // 04.1 status-range + body-shape checks. The Route(_) arm has no
            // validator obligation in Task 1; Task 2 wires UnknownCluster.
            match &r.action {
                RouteAction::DirectResponse(dr) => {
                    if !(100..=599).contains(&dr.status) {
                        return Err(crate::ConfigError::InvalidStatusCode { status: dr.status });
                    }
                    validate_data_source(&dr.body, "direct_response.body", Required::InlineString)?;
                }
                RouteAction::Route(ar) => {
                    // 04.3 NEW: check the cluster reference against declared clusters.
                    // ConfigError::UnknownCluster is the 02.1-landed variant reused here
                    // per SPEC §3 D2.
                    // 18 D1/D3: defer the reference check while CDS is
                    // configured-but-unloaded; load_dynamic_resources re-enforces.
                    if !defer_cluster_refs && !clusters.iter().any(|c| c.name == ar.cluster) {
                        return Err(crate::ConfigError::UnknownCluster(ar.cluster.clone()));
                    }
                    // 06.3 D14.3 NEW: H1-listener × H2-cluster reachability gate.
                    // Closes 05.3 REVIEW I1 per parent-06 SPEC §3 D14.3.
                    // 18 D1/D3: the cluster lookup may not resolve when the
                    // reference was deferred (CDS unloaded) — the gate is
                    // skipped for deferred references and re-enforced at the
                    // Task-3 re-validation, so use `if let Some(..)` rather
                    // than `.expect(..)`.
                    if matches!(hcm.codec_type, CodecType::HTTP1 | CodecType::AUTO)
                        && let Some(cluster_ref) = clusters.iter().find(|c| c.name == ar.cluster)
                        && let Some(teo) = &cluster_ref.typed_extension_protocol_options
                        && teo
                            .http_protocol_options
                            .explicit_http_config
                            .http2_protocol_options
                            .is_some()
                    {
                        return Err(crate::ConfigError::Http2ClusterFromHttp1Listener {
                            listener: listener_name.to_string(),
                            cluster: ar.cluster.clone(),
                        });
                    }
                }
            }

            // 23 D3: reject any typed_per_filter_config key not in
            // the HCM's http_filters name set (PerRouteConfigForAbsentFilter).
            for filter_name in r.typed_per_filter_config.keys() {
                if !present_http_filter_names.contains(filter_name.as_str()) {
                    return Err(crate::ConfigError::PerRouteConfigForAbsentFilter {
                        filter: filter_name.clone(),
                    });
                }
            }

            // 24 D3 (ADR-0061 L6): a route-level csrf override is also subject to
            // the deterministic-only / no-runtime-key gate. Runs AFTER the
            // absent-filter check above, so a csrf override with NO csrf chain
            // filter hits PerRouteConfigForAbsentFilter (not validate_csrf_config).
            for pfc in r.typed_per_filter_config.values() {
                if let crate::PerFilterConfig::Csrf(p) = pfc {
                    validate_csrf_config(p, listener_name)?;
                }
            }

            // 04.2 NEW: walk the headers Vec.
            for hm in &mut r.r#match.headers {
                validate_header_matcher(hm)?;
            }

            // 16.1 D2: validate per-route retry_policy.
            validate_retry_policy(r)?;

            // 28 Task 4: validate per-route hash_policy entries — the MVP
            // supports only the `header` specifier; unsupported specifiers are
            // rejected as fatal (ADR-0049). Only the route-to-cluster action
            // carries hash policies.
            if let RouteAction::Route(route_action) = &r.action {
                for hp in &route_action.hash_policy {
                    validate_hash_policy(hp)?;
                }
            }
        }
    }
    Ok(())
}

/// 20 D1 (C16, ADR-0051/0052): the parse-time exactly-one-of route-source pass.
/// Walks every HCM across the given listeners' filter chains and rejects any HCM
/// that declares NEITHER `route_config` (inline) NOR `rds` (file)
/// (`MissingRouteSource`) or BOTH (`AmbiguousRouteSource`) — §5.8 / L9.
///
/// Runs at PARSE time (from `parse_bootstrap`, before any file is read) rather
/// than inside `validate()`: after `load_dynamic_resources` populates an `rds`
/// HCM's `route_config`, BOTH `route_config` AND `rds` are `Some` (the loaded
/// state — §5.3), and the post-merge `validate()` does NOT re-check cardinality.
/// Factored out as a standalone crate-visible helper so the Task-3 merge pass can
/// re-run it over the merged (static + dynamic LDS) listener set.
pub(crate) fn check_route_sources(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
    for listener in bootstrap.all_listeners() {
        for chain in &listener.filter_chains {
            for filter in &chain.filters {
                let Some(TypedConfig::HttpConnectionManager(hcm)) = &filter.typed_config else {
                    continue;
                };
                match (hcm.route_config.is_some(), hcm.rds.is_some()) {
                    (false, false) => {
                        return Err(crate::ConfigError::MissingRouteSource {
                            stat_prefix: hcm.stat_prefix.clone(),
                        });
                    }
                    (true, true) => {
                        return Err(crate::ConfigError::AmbiguousRouteSource {
                            stat_prefix: hcm.stat_prefix.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// 21 D1 (ADR-0053/0054; L6 6a): the parse-time-only exactly-one-of endpoint-
/// source pass — reject an inline `load_assignment` on a `type: EDS` cluster
/// (stricter than Envoy, which accepts-and-ignores). This runs in
/// `parse_bootstrap` (and is re-callable over the merged static+CDS cluster set
/// in Task 3) BEFORE any EDS file is read, so it does NOT false-positive the
/// post-merge loaded `(Eds, Some, Some)` state (which `validate_cluster`
/// tolerates). Mirrors phase-20's `check_route_sources`.
pub(crate) fn check_endpoint_sources(bootstrap: &Bootstrap) -> Result<(), crate::ConfigError> {
    for cluster in bootstrap.all_clusters() {
        if cluster.cluster_type == ClusterType::Eds && cluster.load_assignment.is_some() {
            return Err(crate::ConfigError::LoadAssignmentOnEdsCluster {
                cluster: cluster.name.clone(),
            });
        }
    }
    Ok(())
}

/// Validate the http_filters list of an HCM listener.
///
/// Enforces: (a) at least one filter, (b) exactly one Router, (c) Router
/// at the terminus, (d) name/typed_config consistency on every entry.
///
/// At 07.1 the only typed_config variant is `Router`; the
/// name/typed_config consistency check is currently dead-code-defensive
/// (the schema's `HttpFilterTypedConfig` enum is closed and serde's
/// `deny_unknown_fields` rejects unknown variants at parse time). The
/// check is retained so 07.2's HeaderMutation arm slots in without a
/// validator rewrite.
///
/// Replaces the pre-07.1 cardinality gate at lines 1335-1346 of this
/// file. Mirrors 05.2's `validate_h2_protocol_options` /
/// 06.3's `Http2ClusterFromHttp1Listener` listener-name-threaded
/// validator shape.
pub(crate) fn validate_http_filters(
    filters: &[crate::HttpFilter],
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if filters.is_empty() {
        return Err(crate::ConfigError::EmptyHttpFilters {
            listener: listener_name.to_string(),
        });
    }

    let last_index = filters.len() - 1;
    let mut router_positions: Vec<usize> = Vec::new();

    for (i, f) in filters.iter().enumerate() {
        // Name/typed_config consistency gate, shared by every filter kind.
        // Each arm below already ran this check FIRST (before its per-filter
        // validator), so hoisting it preserves error ordering.
        if f.name != f.typed_config.expected_name() {
            return Err(crate::ConfigError::UnsupportedHttpFilter {
                name: f.name.clone(),
            });
        }
        match &f.typed_config {
            crate::HttpFilterTypedConfig::Router(_) => {
                router_positions.push(i);
            }
            crate::HttpFilterTypedConfig::HeaderMutation(cfg) => {
                validate_header_mutation_entries(&cfg.mutations.request_mutations, listener_name)?;
                validate_header_mutation_entries(&cfg.mutations.response_mutations, listener_name)?;
            }
            crate::HttpFilterTypedConfig::LocalRateLimit(cfg) => {
                validate_local_rate_limit_config(cfg, listener_name)?;
            }
            crate::HttpFilterTypedConfig::Rbac(cfg) => {
                validate_rbac_config(cfg, listener_name)?;
            }
            crate::HttpFilterTypedConfig::Fault(cfg) => {
                validate_fault_config(cfg, listener_name)?;
            }
            crate::HttpFilterTypedConfig::JwtAuthn(cfg) => {
                validate_jwt_authn_config(cfg, listener_name)?;
            }
            crate::HttpFilterTypedConfig::Cors(_cfg) => {
                // CorsConfig is empty (near-zero; no per-filter-chain fields to validate);
                // name/typed_config consistency check above is the sole gate.
            }
            crate::HttpFilterTypedConfig::Csrf(cfg) => {
                validate_csrf_config(cfg, listener_name)?;
            }
            crate::HttpFilterTypedConfig::Buffer(_cfg) => {
                // Buffer.max_request_bytes is a required u32 (serde-enforced;
                // absent/malformed → fatal parse error); `0` is a valid limit.
                // No further validation (ADR-0063 — NO stats, NO new ConfigError).
            }
            crate::HttpFilterTypedConfig::CdnLoop(cfg) => {
                validate_cdn_loop_config(cfg, listener_name)?;
            }
            crate::HttpFilterTypedConfig::SetMetadata(cfg) => {
                validate_set_metadata_config(cfg, listener_name)?;
            }
            crate::HttpFilterTypedConfig::HeaderToMetadata(cfg) => {
                validate_header_to_metadata_config(cfg, listener_name)?;
            }
        }
    }

    if router_positions.len() > 1 {
        return Err(crate::ConfigError::DuplicateRouterFilter {
            listener: listener_name.to_string(),
        });
    }
    if router_positions.is_empty() {
        return Err(crate::ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: last_index,
        });
    }
    let router_position = router_positions[0];
    if router_position != last_index {
        return Err(crate::ConfigError::RouterNotTerminal {
            listener: listener_name.to_string(),
            position: router_position,
        });
    }
    Ok(())
}

/// Validate one HeaderMutation mutations list (request_mutations or
/// response_mutations). Per-entry: non-empty `header.key` + RFC 7230 token
/// set + `append_action` in the supported subset. `position` in each error is
/// the entry index within `entries`. Phase 07.2.
fn validate_header_mutation_entries(
    entries: &[crate::HeaderMutationEntry],
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    for (entry_idx, entry) in entries.iter().enumerate() {
        let key = &entry.append.header.key;
        if key.is_empty() {
            return Err(crate::ConfigError::EmptyHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
            });
        }
        if !is_valid_rfc7230_token(key) {
            return Err(crate::ConfigError::InvalidHeaderMutationKey {
                listener: listener_name.to_string(),
                position: entry_idx,
                key: key.clone(),
            });
        }
        match entry.append.append_action {
            crate::AppendAction::AppendIfExistsOrAdd
            | crate::AppendAction::OverwriteIfExistsOrAdd => {
                // supported.
            }
            crate::AppendAction::AddIfAbsent => {
                return Err(crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "ADD_IF_ABSENT".to_string(),
                });
            }
            crate::AppendAction::OverwriteIfExists => {
                return Err(crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener: listener_name.to_string(),
                    position: entry_idx,
                    action: "OVERWRITE_IF_EXISTS".to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Validate one LocalRateLimit filter config. Phase 09 (SPEC §3 D2):
///   - stat_prefix non-empty
///   - token_bucket.max_tokens > 0
///   - token_bucket.fill_interval parses to a Duration > 0
///   - status.code == 429 (phase 09 accepts 429 only)
fn validate_local_rate_limit_config(
    cfg: &crate::LocalRateLimitConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.stat_prefix.is_empty() {
        return Err(crate::ConfigError::EmptyLocalRateLimitStatPrefix {
            listener: listener_name.to_string(),
        });
    }
    if cfg.token_bucket.max_tokens == 0 {
        return Err(crate::ConfigError::TokenBucketMaxTokensMustBePositive {
            listener: listener_name.to_string(),
        });
    }
    let fill = cfg.token_bucket.fill_interval.as_str().ok_or_else(|| {
        crate::ConfigError::InvalidTokenBucketFillInterval {
            listener: listener_name.to_string(),
            message: "fill_interval must be a string like \"60s\" / \"250ms\" / \"500us\""
                .to_string(),
        }
    })?;
    let dur =
        parse_duration(fill).map_err(|msg| crate::ConfigError::InvalidTokenBucketFillInterval {
            listener: listener_name.to_string(),
            message: msg,
        })?;
    if dur.is_zero() {
        return Err(crate::ConfigError::InvalidTokenBucketFillInterval {
            listener: listener_name.to_string(),
            message: "fill_interval must be > 0".to_string(),
        });
    }
    if cfg.status.code != 429 {
        return Err(crate::ConfigError::UnsupportedLocalRateLimitStatusCode {
            listener: listener_name.to_string(),
            code: cfg.status.code,
        });
    }
    Ok(())
}

/// Validate one HTTP RBAC filter config (`envoy.filters.http.rbac`, phase 10).
/// The tree walk itself lives in [`validate_rbac_rules`], shared with the
/// network filter (67.1 D1).
fn validate_rbac_config(
    cfg: &crate::RbacConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    validate_rbac_rules(&cfg.rules, listener_name)
}

/// Validate an RBAC policy tree. SHARED by the HTTP filter
/// (`validate_rbac_config`) and the NETWORK filter
/// (`validate_network_rbac_config`, 67.1 D1). Phase 10 (SPEC §3 D2):
///   - rules.policies non-empty
///   - per-policy permissions + principals non-empty
///   - recursive: empty AndRules/OrRules/AndIds/OrIds rejected
///   - recursive: depth ≤ RBAC_TREE_MAX_DEPTH
///
/// Its six `ConfigError` variants are scope-neutral (`listener {listener:?}`,
/// not `HCM listener`) precisely because both filters raise them — ADR-0130.
fn validate_rbac_rules(
    rules: &crate::Rules,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if rules.policies.is_empty() {
        return Err(crate::ConfigError::EmptyRbacPolicies {
            listener: listener_name.to_string(),
        });
    }
    for (policy_name, policy) in rules.policies.iter() {
        if policy.permissions.is_empty() {
            return Err(crate::ConfigError::EmptyRbacPolicyPermissions {
                listener: listener_name.to_string(),
                policy_name: policy_name.clone(),
            });
        }
        if policy.principals.is_empty() {
            return Err(crate::ConfigError::EmptyRbacPolicyPrincipals {
                listener: listener_name.to_string(),
                policy_name: policy_name.clone(),
            });
        }
        for (idx, perm) in policy.permissions.iter().enumerate() {
            validate_permission_tree(
                perm,
                listener_name,
                policy_name,
                &format!("permissions[{idx}]"),
                1,
            )?;
        }
        for (idx, prin) in policy.principals.iter().enumerate() {
            validate_principal_tree(
                prin,
                listener_name,
                policy_name,
                &format!("principals[{idx}]"),
                1,
            )?;
        }
    }
    Ok(())
}

/// 67.1 D1: validate one NETWORK `rbac` filter config.
///   - `stat_prefix` non-empty (SPEC R-3);
///   - `rules: None` ⇒ INERT, nothing more to check (SPEC R-4);
///   - `rules: Some(_)` ⇒ the shared RBAC tree validations (empty sets, depth).
fn validate_network_rbac_config(
    cfg: &crate::NetworkRbacConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.stat_prefix.is_empty() {
        return Err(crate::ConfigError::EmptyNetworkRbacStatPrefix {
            listener: listener_name.to_string(),
        });
    }
    let Some(rules) = cfg.rules.as_ref() else {
        return Ok(()); // SPEC R-4: rules omitted ⇒ INERT.
    };
    validate_rbac_rules(rules, listener_name)?;
    // 67.1 D3 (CF-67-4): the L4 leaf allow-list. Runs AFTER the shared tree
    // validation, which bounds depth at RBAC_TREE_MAX_DEPTH — so these
    // recursions are stack-safe. `67.2` widens the allow-list.
    //
    // REVIEW.md I-2, stated once so nobody re-derives it backwards: because
    // `validate_rbac_rules` runs FIRST, a `metadata` leaf that is *structurally*
    // malformed (empty `filter`, multi-segment `path`) raises
    // `RbacMetadataMatcherInvalid` — NOT `UnsupportedNetworkRbacMatcher`. Only a
    // *well-formed* `metadata` leaf reaches the L4 walk below and is rejected as
    // unevaluable. Both messages are scope-neutral, so neither says "HCM listener"
    // for this filter, which has no HCM. Do NOT swap the order to make the L4
    // error win: this order IS the depth bound, pinned by
    // `network_rbac_depth_bound_precedes_the_l4_walk`.
    for (policy_name, policy) in rules.policies.iter() {
        for (idx, perm) in policy.permissions.iter().enumerate() {
            validate_l4_permission(
                perm,
                listener_name,
                policy_name,
                &format!("permissions[{idx}]"),
            )?;
        }
        for (idx, prin) in policy.principals.iter().enumerate() {
            validate_l4_principal(
                prin,
                listener_name,
                policy_name,
                &format!("principals[{idx}]"),
            )?;
        }
    }
    Ok(())
}

/// Phase 22: validate a jwt_authn filter config (minimum-viable). All errors
/// are config-load-time fatal (ADR-0049 all-fatal posture).
pub(crate) fn validate_jwt_authn_config(
    cfg: &crate::JwtAuthnConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.providers.is_empty() {
        return Err(crate::ConfigError::JwtAuthnNoProviders {
            listener: listener_name.to_string(),
        });
    }
    for (name, provider) in &cfg.providers {
        let jwks = provider
            .local_jwks
            .inline_string
            .as_deref()
            .ok_or_else(|| crate::ConfigError::JwtAuthnInvalidJwks {
                listener: listener_name.to_string(),
                provider: name.clone(),
            })?;
        envoy_jwt::JwkSet::parse(jwks).map_err(|_| crate::ConfigError::JwtAuthnInvalidJwks {
            listener: listener_name.to_string(),
            provider: name.clone(),
        })?;
    }
    for rule in &cfg.rules {
        if !cfg.providers.contains_key(&rule.requires.provider_name) {
            return Err(crate::ConfigError::JwtAuthnUnknownProvider {
                listener: listener_name.to_string(),
                provider_name: rule.requires.provider_name.clone(),
            });
        }
        // RouteMatch structural validity: exactly one of prefix/path must be set.
        validate_route_match_cardinality(
            rule.r#match.prefix.is_some(),
            rule.r#match.path.is_some(),
        )?;
    }
    Ok(())
}

/// RouteMatch structural validity, shared by `validate_http_connection_manager`
/// and `validate_jwt_authn_config`: exactly one of `prefix`/`path` must be set.
fn validate_route_match_cardinality(
    prefix_set: bool,
    path_set: bool,
) -> Result<(), crate::ConfigError> {
    match (prefix_set, path_set) {
        (true, false) | (false, true) => Ok(()),
        (true, true) => Err(crate::ConfigError::UnsupportedRouteMatcher {
            matcher: "both prefix and path are set",
        }),
        (false, false) => Err(crate::ConfigError::UnsupportedRouteMatcher {
            matcher: "neither prefix nor path is set",
        }),
    }
}

/// Phase 11: validate the fault filter config. Rejects invalid abort status
/// codes, out-of-range percentages, and (per phase-11 deterministic-only scope)
/// fractional percentages. The optional `headers` gate reuses the 04.2
/// `HeaderMatcher` (no parse-time validation beyond deserialize).
fn validate_fault_config(
    cfg: &crate::FaultConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if !(100..=599).contains(&cfg.abort.http_status) {
        return Err(crate::ConfigError::InvalidFaultAbortStatus {
            listener: listener_name.to_string(),
            status: cfg.abort.http_status,
        });
    }
    let denominator = cfg.abort.percentage.denominator.value();
    let numerator = cfg.abort.percentage.numerator;
    // Out-of-range check FIRST: numerator > denominator is an operator typo,
    // reported distinctly from the fractional rejection.
    if numerator > denominator {
        return Err(crate::ConfigError::FaultPercentageOutOfRange {
            listener: listener_name.to_string(),
            numerator,
            denominator,
        });
    }
    // Deterministic-only: numerator must be 0 (0%) or == denominator (100%).
    if numerator != 0 && numerator != denominator {
        return Err(crate::ConfigError::UnsupportedFractionalFaultPercentage {
            listener: listener_name.to_string(),
            numerator,
            denominator,
        });
    }
    Ok(())
}

/// 24 D3 (ADR-0061 L6): validate a `CsrfPolicy.filter_enabled`. envoy-rust honors
/// only deterministic 0%/100% gating (the phase-11 fault precedent) and has no
/// RTDS runtime layer, so a present `runtime_key` or a fractional `default_value`
/// is startup-fatal. The SAME message is used at the filter-chain level
/// (`HttpFilterTypedConfig::Csrf`) and the per-route level (`PerFilterConfig::Csrf`),
/// so this single validator gates both call sites.
fn validate_csrf_config(
    cfg: &crate::CsrfPolicy,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    let fe = &cfg.filter_enabled;
    if fe.runtime_key.is_some() {
        return Err(
            crate::ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled {
                listener: listener_name.to_string(),
            },
        );
    }
    let p = &fe.default_value;
    // Deterministic-only: numerator must be 0 (0%) or == denominator (100%).
    if p.numerator != 0 && p.numerator != p.denominator.value() {
        return Err(
            crate::ConfigError::UnsupportedNonDeterministicCsrfFilterEnabled {
                listener: listener_name.to_string(),
            },
        );
    }
    Ok(())
}

/// RFC 7230 §3.2.6 `tchar` (ADR-0077 §A.4):
///   "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." /
///   "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA
///
/// Duplicated here (not imported) so envoy-config does NOT take a dependency on
/// envoy-filter, which carries its own private `is_tchar` for the runtime
/// CDN-Loop header parser. The two sets are intentionally identical — both
/// trace to RFC 7230 + ADR-0077 §A.4. Comma (`,`) and space (` `) are NOT
/// tchars, so a comma-joined or space-containing `cdn_id` is rejected here.
const fn is_cdn_id_tchar(b: u8) -> bool {
    matches!(b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+'
        | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
        | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
}

/// phase 31 Task 2 (ADR-0077 §6.2-LOCKED): validate a `cdn_loop` filter config.
/// A valid `cdn_id` is a non-empty bare RFC-7230 token (1+ tchars). Both checks
/// are boot-fatal (ADR-0049 — Envoy itself rejects these at boot):
///
/// - empty `cdn_id` → `CdnLoopEmptyCdnId`
/// - `cdn_id` carrying any non-RFC-7230-tchar (comma, space, `@`, …) →
///   `CdnLoopInvalidCdnId`
fn validate_cdn_loop_config(
    cfg: &crate::CdnLoopConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    if cfg.cdn_id.is_empty() {
        return Err(crate::ConfigError::CdnLoopEmptyCdnId {
            listener: listener_name.to_string(),
        });
    }
    if !cfg.cdn_id.bytes().all(is_cdn_id_tchar) {
        return Err(crate::ConfigError::CdnLoopInvalidCdnId {
            listener: listener_name.to_string(),
            cdn_id: cfg.cdn_id.clone(),
        });
    }
    Ok(())
}

fn validate_set_metadata_config(
    cfg: &crate::SetMetadataConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    for entry in &cfg.metadata {
        if entry.metadata_namespace.is_empty() {
            return Err(crate::ConfigError::SetMetadataEmptyNamespace {
                listener: listener_name.to_string(),
            });
        }
    }
    Ok(())
}

fn validate_header_to_metadata_config(
    cfg: &crate::HeaderToMetadataConfig,
    listener_name: &str,
) -> Result<(), crate::ConfigError> {
    let bad = |detail: String| crate::ConfigError::HeaderToMetadataInvalidRule {
        listener: listener_name.to_string(),
        detail,
    };
    for rule in &cfg.request_rules {
        if rule.header.is_empty() {
            return Err(bad("a rule has an empty `header`".into()));
        }
        if rule.on_header_present.is_none() && rule.on_header_missing.is_none() {
            return Err(bad(format!(
                "rule for header `{}` has neither on_header_present nor on_header_missing",
                rule.header
            )));
        }
        for kv in [
            rule.on_header_present.as_ref(),
            rule.on_header_missing.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            if kv.key.is_empty() {
                return Err(bad(format!(
                    "rule for header `{}` has an empty `key`",
                    rule.header
                )));
            }
        }
        if rule
            .on_header_missing
            .as_ref()
            .is_some_and(|miss| miss.value.is_none())
        {
            return Err(bad(format!(
                "on_header_missing for header `{}` requires a `value`",
                rule.header
            )));
        }
    }
    Ok(())
}

/// Generates the recursive RBAC condition-tree validator; `Permission` and
/// `Principal` are structurally symmetric, differing only in the enum type
/// (with its combinator variant names), the set-field name (`rules` vs `ids`),
/// the path-segment format strings, and the empty-set error variant — all
/// passed as arguments. Path-segment literals are passed WHOLE (with explicit
/// `path =`/`idx =` bindings, since macro hygiene keeps a call-site literal
/// from implicitly capturing macro-body bindings) so the emitted path strings
/// stay byte-identical to the pre-macro hand-written pair.
///
/// Per-arm semantics (shared by both expansions):
///   - depth > `RBAC_TREE_MAX_DEPTH` → `RbacTreeTooDeep` (phase-10 SPEC §3 D2);
///   - `Any`/`Header` are leaves with no semantic check beyond serde;
///   - empty and/or set → the per-type empty-set error;
///   - phase 35: real `metadata` matcher validation (non-empty filter +
///     single-segment path) — `RbacMetadataMatcherInvalid` (a leaf, no recursion);
///   - phase 37: `url_path` is a leaf (mirroring the `Header(_)` arm). The inner
///     StringMatcher's SafeRegex compiles at RBAC lowering (`lower_permission`),
///     not here — this path is an immutable borrow (ADR-0090 §D; the
///     validate_metadata_matcher note below).
macro_rules! define_rbac_tree_validator {
    (
        $fn_name:ident,
        $node:ident,
        $and:ident | $or:ident,
        $not:ident,
        $set_field:ident,
        $set_seg:literal,
        $not_seg:literal,
        $empty_err:ident,
        extra_leaves: [ $($leaf:ident),* $(,)? ]
    ) => {
        fn $fn_name(
            node: &crate::$node,
            listener_name: &str,
            policy_name: &str,
            path: &str,
            depth: u32,
        ) -> Result<(), crate::ConfigError> {
            if depth > RBAC_TREE_MAX_DEPTH {
                return Err(crate::ConfigError::RbacTreeTooDeep {
                    listener: listener_name.to_string(),
                    policy_name: policy_name.to_string(),
                    depth,
                });
            }
            match node {
                crate::$node::Any(_) => Ok(()),
                crate::$node::Header(_) => Ok(()),
                crate::$node::$and(set) | crate::$node::$or(set) => {
                    if set.$set_field.is_empty() {
                        return Err(crate::ConfigError::$empty_err {
                            listener: listener_name.to_string(),
                            policy_name: policy_name.to_string(),
                            path: path.to_string(),
                        });
                    }
                    for (idx, child) in set.$set_field.iter().enumerate() {
                        $fn_name(
                            child,
                            listener_name,
                            policy_name,
                            &format!($set_seg, path = path, idx = idx),
                            depth + 1,
                        )?;
                    }
                    Ok(())
                }
                crate::$node::$not(child) => $fn_name(
                    child,
                    listener_name,
                    policy_name,
                    &format!($not_seg, path = path),
                    depth + 1,
                ),
                crate::$node::Metadata(m) => {
                    validate_metadata_matcher(m, listener_name, policy_name, path)
                }
                crate::$node::UrlPath(_) => Ok(()),
                // 67.2: the connection-level leaves are asymmetric between the two
                // enums (3 Principal-only, 2 Permission-only), so this shared macro
                // body cannot name them uniformly — each instantiation passes its
                // own set via `extra_leaves`. They are non-recursive leaves; the
                // tree validator only bounds depth + rejects empty sets, so `Ok(())`
                // is correct (CidrRange width is checked by the L4 walk). ADR-0133.
                $( crate::$node::$leaf(_) => Ok(()), )*
            }
        }
    };
}

define_rbac_tree_validator!(
    validate_permission_tree,
    Permission,
    AndRules | OrRules,
    NotRule,
    rules,
    "{path}.rules[{idx}]",
    "{path}.not_rule",
    EmptyRbacPermissionSet,
    extra_leaves: [DestinationIp, DestinationPort]
);

define_rbac_tree_validator!(
    validate_principal_tree,
    Principal,
    AndIds | OrIds,
    NotId,
    ids,
    "{path}.ids[{idx}]",
    "{path}.not_id",
    EmptyRbacPrincipalSet,
    extra_leaves: [DirectRemoteIp, RemoteIp, SourceIp]
);

/// 67.1 D3 (CF-67-4): reject every `Permission` leaf a NETWORK (L4) rbac filter
/// cannot evaluate. `any` is admitted; `and_rules` / `or_rules` / `not_rule`
/// recurse. `header` (parity), `url_path` and `metadata` (fail-loud) are
/// rejected.
///
/// EXHAUSTIVE, with no `_ =>` catch-all: when `67.2` adds the connection-level
/// arms, this function MUST fail to compile until they are classified. Never add
/// a catch-all here.
///
/// Depth is NOT re-checked: `validate_rbac_rules` runs first and bounds the tree
/// at `RBAC_TREE_MAX_DEPTH`, so this recursion is stack-safe by construction.
fn validate_l4_permission(
    node: &crate::Permission,
    listener_name: &str,
    policy_name: &str,
    path: &str,
) -> Result<(), crate::ConfigError> {
    let reject = |arm: &'static str| {
        Err(crate::ConfigError::UnsupportedNetworkRbacMatcher {
            listener: listener_name.to_string(),
            policy_name: policy_name.to_string(),
            arm,
            path: path.to_string(),
        })
    };
    match node {
        crate::Permission::Any(_) => Ok(()),
        crate::Permission::Header(_) => reject("header"),
        crate::Permission::Metadata(_) => reject("metadata"),
        crate::Permission::UrlPath(_) => reject("url_path"),
        // 67.2 D4: the connection-level arms are ADMITTED at L4 (ADR-0133); their
        // CidrRange width is validated here (the tree validator only bounds depth).
        crate::Permission::DestinationPort(_) => Ok(()),
        crate::Permission::DestinationIp(cidr) => {
            cidr.validate()
                .map_err(|detail| crate::ConfigError::InvalidCidrRange {
                    listener: listener_name.to_string(),
                    policy_name: policy_name.to_string(),
                    path: path.to_string(),
                    detail,
                })
        }
        crate::Permission::AndRules(set) | crate::Permission::OrRules(set) => {
            for (idx, child) in set.rules.iter().enumerate() {
                validate_l4_permission(
                    child,
                    listener_name,
                    policy_name,
                    &format!("{path}.rules[{idx}]"),
                )?;
            }
            Ok(())
        }
        crate::Permission::NotRule(child) => validate_l4_permission(
            child,
            listener_name,
            policy_name,
            &format!("{path}.not_rule"),
        ),
    }
}

/// 67.1 D3 (CF-67-4): the `Principal` twin of [`validate_l4_permission`]. The
/// set-wrapper field is `ids` (not `rules`) and the negation arm is `not_id`,
/// per the upstream proto — which is exactly why the shared
/// `define_rbac_tree_validator!` macro cannot express these verdicts.
/// EXHAUSTIVE, no catch-all: see [`validate_l4_permission`].
fn validate_l4_principal(
    node: &crate::Principal,
    listener_name: &str,
    policy_name: &str,
    path: &str,
) -> Result<(), crate::ConfigError> {
    let reject = |arm: &'static str| {
        Err(crate::ConfigError::UnsupportedNetworkRbacMatcher {
            listener: listener_name.to_string(),
            policy_name: policy_name.to_string(),
            arm,
            path: path.to_string(),
        })
    };
    match node {
        crate::Principal::Any(_) => Ok(()),
        crate::Principal::Header(_) => reject("header"),
        crate::Principal::Metadata(_) => reject("metadata"),
        crate::Principal::UrlPath(_) => reject("url_path"),
        // 67.2 D4: the three source-IP arms are ADMITTED at L4 (ADR-0133); they
        // share one CidrRange-width check (evaluated against `peer_addr.ip()`).
        crate::Principal::DirectRemoteIp(cidr)
        | crate::Principal::RemoteIp(cidr)
        | crate::Principal::SourceIp(cidr) => {
            cidr.validate()
                .map_err(|detail| crate::ConfigError::InvalidCidrRange {
                    listener: listener_name.to_string(),
                    policy_name: policy_name.to_string(),
                    path: path.to_string(),
                    detail,
                })
        }
        crate::Principal::AndIds(set) | crate::Principal::OrIds(set) => {
            for (idx, child) in set.ids.iter().enumerate() {
                validate_l4_principal(
                    child,
                    listener_name,
                    policy_name,
                    &format!("{path}.ids[{idx}]"),
                )?;
            }
            Ok(())
        }
        crate::Principal::NotId(child) => {
            validate_l4_principal(child, listener_name, policy_name, &format!("{path}.not_id"))
        }
    }
}

/// Phase 35: validate a single RBAC `metadata` matcher (shared by the Permission
/// and Principal tree validators). Two boot-fatal rules (ADR-0086 §A4/A5):
///   - empty `filter` (Envoy: PGV min_len 1);
///   - `path.len() != 1` — Envoy accepts a multi-segment/nested path, but
///     envoy-rust's flat string-only metadata store cannot resolve one, so we
///     reject it (stricter than Envoy). Also catches the empty `path: []` case.
///
/// NOTE: the inner `value` StringMatcher's SafeRegex is NOT compiled here — the
/// RBAC validation path (`validate_http_filters` → `validate_rbac_config` →
/// these tree validators) is an immutable borrow, so it cannot mutate
/// `SafeRegex::compiled`. This matches the pre-existing `Permission::Header`
/// arm, which likewise does not compile its SafeRegex on the RBAC path.
fn validate_metadata_matcher(
    m: &crate::MetadataMatcher,
    listener_name: &str,
    policy_name: &str,
    path: &str,
) -> Result<(), crate::ConfigError> {
    let bad = |detail: String| crate::ConfigError::RbacMetadataMatcherInvalid {
        listener: listener_name.to_string(),
        policy_name: policy_name.to_string(),
        path: path.to_string(),
        detail,
    };
    if m.filter.is_empty() {
        return Err(bad("metadata matcher `filter` must not be empty".into()));
    }
    if m.path.len() != 1 {
        return Err(bad(format!(
            "metadata matcher path must have exactly one segment (got {}); multi-segment/nested paths are deferred",
            m.path.len()
        )));
    }
    Ok(())
}

/// Hand-rolled Duration string parser covering upstream Envoy v1.33's
/// documented Duration shapes (`"<N>s"` / `"<N>ms"` / `"<N>us"`). Returns
/// the parsed `Duration` on success; an error message on failure. Lands
/// inline here per phase-09 SPEC §5.2's no-foundations-grant posture
/// (no `humantime` / `humantime-serde` pull).
pub fn parse_duration(s: &str) -> Result<std::time::Duration, String> {
    if s.is_empty() {
        return Err("empty duration string".to_string());
    }
    // Order matters: "ms" / "us" before "s" because the longer suffixes share
    // the trailing 's' character.
    if let Some(num) = s.strip_suffix("ms") {
        let n: u64 = num
            .parse()
            .map_err(|e| format!("invalid millisecond value {num:?}: {e}"))?;
        return Ok(std::time::Duration::from_millis(n));
    }
    if let Some(num) = s.strip_suffix("us") {
        let n: u64 = num
            .parse()
            .map_err(|e| format!("invalid microsecond value {num:?}: {e}"))?;
        return Ok(std::time::Duration::from_micros(n));
    }
    if let Some(num) = s.strip_suffix("s") {
        let n: u64 = num
            .parse()
            .map_err(|e| format!("invalid second value {num:?}: {e}"))?;
        return Ok(std::time::Duration::from_secs(n));
    }
    Err(format!(
        "unsupported duration unit in {s:?} (expected suffix s / ms / us)"
    ))
}

/// Parse a duration string via [`parse_duration`] and additionally require it
/// to be non-zero. Returns `None` on a parse failure or a zero duration;
/// callers map `None` to their own error variant.
fn parse_positive_duration(raw: &str) -> Option<std::time::Duration> {
    parse_duration(raw).ok().filter(|d| !d.is_zero())
}

/// RFC 7230 §3.2.6 `token` validation: a header field name is a non-empty
/// sequence of `tchar`. Delegates to [`is_cdn_id_tchar`] — the one RFC 7230
/// `tchar` set in this crate.
fn is_valid_rfc7230_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_cdn_id_tchar)
}

/// 12.1 (parent-12 D2): validate a cluster's `health_checks` + `common_lb_config`.
/// Returns the first error encountered (validator-wide convention). HTTP/TCP/gRPC
/// checkers supported (custom is rejected); the schema's `Option<_>` checker
/// fields surface a missing checker as `UnsupportedHealthCheckType` and more
/// than one as `MultipleHealthCheckers` (`deny_unknown_fields` rejects unknown
/// checker keys, e.g. `custom_health_check`).
fn validate_health_checks(cluster: &Cluster) -> Result<(), crate::ConfigError> {
    if cluster.health_checks.len() > 1 {
        return Err(crate::ConfigError::UnsupportedMultipleHealthChecks {
            cluster: cluster.name.clone(),
        });
    }
    if let Some(hc) = cluster.health_checks.first() {
        // 69 (ADR-0139): the upstream `health_checker` oneof — more than one
        // of http/tcp/grpc present is fatal. Generalizes the phase-68
        // both-http-and-tcp check.
        let n_set = [
            hc.http_health_check.is_some(),
            hc.tcp_health_check.is_some(),
            hc.grpc_health_check.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if n_set > 1 {
            return Err(crate::ConfigError::MultipleHealthCheckers {
                cluster: cluster.name.clone(),
            });
        }
        // None present → unsupported (custom deferred). Precedence preserved.
        if n_set == 0 {
            return Err(crate::ConfigError::UnsupportedHealthCheckType {
                cluster: cluster.name.clone(),
            });
        }
        // 69 (ADR-0139): grpc_health_check requires the cluster's upstream to
        // support HTTP/2 (real Envoy makes this load-fatal).
        if hc.grpc_health_check.is_some() {
            let is_h2 = cluster
                .typed_extension_protocol_options
                .as_ref()
                .is_some_and(|teo| {
                    teo.http_protocol_options
                        .explicit_http_config
                        .http2_protocol_options
                        .is_some()
                });
            if !is_h2 {
                return Err(crate::ConfigError::GrpcHealthCheckRequiresHttp2 {
                    cluster: cluster.name.clone(),
                });
            }
        }
        // Shared threshold + timing validation (both checker types).
        if hc.healthy_threshold < 1 {
            return Err(crate::ConfigError::InvalidHealthCheckThreshold {
                cluster: cluster.name.clone(),
                field: "healthy_threshold",
            });
        }
        if hc.unhealthy_threshold < 1 {
            return Err(crate::ConfigError::InvalidHealthCheckThreshold {
                cluster: cluster.name.clone(),
                field: "unhealthy_threshold",
            });
        }
        for (field, raw) in [("timeout", &hc.timeout), ("interval", &hc.interval)] {
            if parse_positive_duration(raw).is_none() {
                return Err(crate::ConfigError::InvalidHealthCheckTiming {
                    cluster: cluster.name.clone(),
                    field,
                });
            }
        }
        // Per-checker-type validation.
        if let Some(http) = &hc.http_health_check {
            if http.path.is_empty() {
                return Err(crate::ConfigError::EmptyHealthCheckPath {
                    cluster: cluster.name.clone(),
                });
            }
            for range in &http.expected_statuses {
                if range.start >= range.end {
                    return Err(crate::ConfigError::InvalidInt64Range {
                        start: range.start,
                        end: range.end,
                    });
                }
            }
        }
        if let Some(tcp) = &hc.tcp_health_check {
            let validate_payload = |p: &HealthCheckPayload| match p.decode() {
                Ok(_) => Ok(()),
                Err(PayloadDecodeError::InvalidHex(value)) => {
                    Err(crate::ConfigError::InvalidHealthCheckPayloadHex {
                        cluster: cluster.name.clone(),
                        value,
                    })
                }
                Err(PayloadDecodeError::InvalidBase64(value)) => {
                    Err(crate::ConfigError::InvalidHealthCheckPayloadBase64 {
                        cluster: cluster.name.clone(),
                        value,
                    })
                }
                Err(PayloadDecodeError::Empty) => {
                    Err(crate::ConfigError::EmptyHealthCheckPayload {
                        cluster: cluster.name.clone(),
                    })
                }
            };
            if let Some(send) = &tcp.send {
                validate_payload(send)?;
            }
            for recv in &tcp.receive {
                validate_payload(recv)?;
            }
        }
    }
    if let Some(clb) = &cluster.common_lb_config
        && let Some(p) = &clb.healthy_panic_threshold
        && !(0.0..=100.0).contains(&p.value)
    {
        return Err(crate::ConfigError::InvalidPanicThreshold {
            cluster: cluster.name.clone(),
            value: p.value,
        });
    }
    Ok(())
}

/// 13.1 D2 (parent-13 D2): validate a cluster's `circuit_breakers` block.
/// At phase-13 scope: at-most-one thresholds entry; DEFAULT priority only (or absent);
/// non-zero `max_connections`. Phase-13-deferred threshold fields per parent SPEC §4
/// are rejected by `deny_unknown_fields` automatically at parse time.
///
/// 17 D1 note: `max_requests: 0` and `max_retries: 0` are intentionally NOT rejected
/// here, in contrast to `max_connections: 0`. The distinction is semantic:
///   - `max_connections: 0` has no defined "always-open" meaning in the connection-pool
///     model (phase-13 rationale: a zero connection cap is a misconfiguration).
///   - `max_requests: 0` / `max_retries: 0` are the always-open-breaker configs
///     (ADR-0047 §L1/L2): they disable the budget cap entirely, which is a deliberate
///     and valid operator choice (fixture 0025 relies on this).
fn validate_circuit_breakers(cluster: &Cluster) -> Result<(), crate::ConfigError> {
    let Some(cb) = cluster.circuit_breakers.as_ref() else {
        return Ok(());
    };
    if cb.thresholds.len() > 1 {
        return Err(
            crate::ConfigError::UnsupportedMultipleCircuitBreakerThresholds {
                cluster: cluster.name.clone(),
            },
        );
    }
    if let Some(t) = cb.thresholds.first() {
        if let Some(priority) = t.priority
            && priority != crate::RoutingPriority::Default
        {
            return Err(crate::ConfigError::UnsupportedCircuitBreakerPriority {
                cluster: cluster.name.clone(),
                priority,
            });
        }
        if let Some(value) = t.max_connections
            && value == 0
        {
            return Err(crate::ConfigError::InvalidMaxConnections {
                cluster: cluster.name.clone(),
                value,
            });
        }
        if let Some(value) = t.max_pending_requests
            && value > 0
        {
            return Err(crate::ConfigError::UnsupportedNonZeroMaxPendingRequests {
                cluster: cluster.name.clone(),
                value,
            });
        }
        // max_requests and max_retries: no semantic rejection — 0 is "always-open"
        // (ADR-0047 §L1/L2). Envoy defaults are resolved later at Cluster::from_bootstrap.
    }
    Ok(())
}

/// 14.1 D2 (parent-14 D2): validate a cluster's `outlier_detection` block.
/// Returns the first error encountered (validator-wide convention). Reuses
/// `parse_duration` (`bootstrap.rs:2401`) for `interval` + `base_ejection_time`.
/// Phase-14-deferred sibling fields (success_rate_*, failure_percentage_*,
/// consecutive_local_origin_failure, split_external_local_origin_errors,
/// enforcing_*, max_ejection_time, max_ejection_time_jitter) are rejected
/// automatically at parse time by `deny_unknown_fields` per ADR-0041 §6.2 item-1.
fn validate_outlier_detection(cluster: &Cluster) -> Result<(), crate::ConfigError> {
    let Some(od) = cluster.outlier_detection.as_ref() else {
        return Ok(());
    };
    if let Some(v) = od.consecutive_5xx
        && v < 1
    {
        return Err(crate::ConfigError::InvalidOutlierDetectionThreshold {
            cluster: cluster.name.clone(),
            field: "consecutive_5xx",
        });
    }
    if let Some(v) = od.consecutive_gateway_failure
        && v < 1
    {
        return Err(crate::ConfigError::InvalidOutlierDetectionThreshold {
            cluster: cluster.name.clone(),
            field: "consecutive_gateway_failure",
        });
    }
    for (field, raw_opt) in [
        ("interval", od.interval.as_deref()),
        ("base_ejection_time", od.base_ejection_time.as_deref()),
    ] {
        if let Some(raw) = raw_opt
            && parse_positive_duration(raw).is_none()
        {
            return Err(crate::ConfigError::InvalidOutlierDetectionTiming {
                cluster: cluster.name.clone(),
                field,
            });
        }
    }
    if let Some(v) = od.max_ejection_percent
        && v > 100
    {
        return Err(crate::ConfigError::InvalidMaxEjectionPercent {
            cluster: cluster.name.clone(),
            value: v,
        });
    }
    Ok(())
}

/// 16.1 D2: validate a route's `retry_policy` block.
///
/// `retry_on` tokens are accept-and-ignore (§6.2 L2), so this validator is
/// currently infallible. It exists so future semantic rejections have a home
/// and so the validator surface is symmetric with `validate_circuit_breakers`
/// and `validate_outlier_detection`.
fn validate_retry_policy(_route: &Route) -> Result<(), crate::ConfigError> {
    Ok(())
}

/// 06.2 Task 5 — validate an HCM's `access_log` Vec.
///
/// Rejections enforced here (see SPEC §3 D2.2; item 3 added by phase 70):
///   1. `name` allow-list: only `envoy.access_loggers.file` is accepted.
///      Anything else surfaces as `ConfigError::UnsupportedAccessLogType`.
///      The `@type` URL allow-list is enforced by serde's tagged-enum
///      deserialization on `AccessLogTypedConfig` (unknown URLs surface as
///      `ConfigError::Yaml`); this validator does NOT re-check the URL.
///   2. Non-empty path: `FileAccessLog.path` must not be the empty string.
///      Empty paths surface as `ConfigError::InvalidAccessLogPath`. The
///      sink-side `FileSink::new` would also fail on `""`, but rejecting
///      at parse time gives a clearer diagnostic.
///   3. Phase 70 — `AccessLog.filter` (`AccessLogFilter` oneof) cardinality:
///      when a `filter` is present it must set EXACTLY ONE arm. Zero arms
///      (`filter: {}`) surfaces as `ConfigError::AmbiguousAccessLogFilter`;
///      the >1 branch is unreachable while only one arm exists but is written
///      to stay correct as future phases add arms. This mirrors the
///      `SubstitutionFormatString` / `AmbiguousLogFormat` oneof precedent —
///      the cardinality lives here, not in serde.
///   4. Phase 70 — non-empty `runtime_key`: a `status_code_filter`'s
///      `comparison.value.runtime_key` must not be `""`. Upstream enforces
///      PGV `min_len 1`; the key is RTDS-inert here (the comparison always
///      reads `default_value`), but load-time parity requires the rejection.
///      Surfaces as `ConfigError::EmptyStatusCodeFilterRuntimeKey`.
///
/// Mutates nothing; returns the first error encountered (validator-wide
/// convention).
fn validate_access_logs(access_logs: &[AccessLog]) -> Result<(), crate::ConfigError> {
    for entry in access_logs {
        if entry.name != "envoy.access_loggers.file" {
            return Err(crate::ConfigError::UnsupportedAccessLogType {
                actual: entry.name.clone(),
            });
        }
        if let Some(filter) = &entry.filter {
            // Count the set oneof arms (this phase has exactly one arm).
            let set_arms = [filter.status_code_filter.is_some()]
                .iter()
                .filter(|set| **set)
                .count();
            if set_arms != 1 {
                return Err(crate::ConfigError::AmbiguousAccessLogFilter {
                    detail: if set_arms == 0 {
                        "no filter variant is set".into()
                    } else {
                        "more than one filter variant is set".into()
                    },
                });
            }
            if let Some(scf) = &filter.status_code_filter
                && scf.comparison.value.runtime_key.is_empty()
            {
                return Err(crate::ConfigError::EmptyStatusCodeFilterRuntimeKey);
            }
        }
        match &entry.typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                if cfg.path.is_empty() {
                    return Err(crate::ConfigError::InvalidAccessLogPath);
                }
                if let Some(fmt) = &cfg.log_format {
                    match (&fmt.text_format_source, &fmt.json_format) {
                        (Some(ds), None) => {
                            envoy_accesslog::parse_format(&ds.inline_string).map_err(|e| {
                                crate::ConfigError::InvalidAccessLogFormat {
                                    detail: e.to_string(),
                                }
                            })?;
                        }
                        (None, Some(map)) => {
                            // Empty map is VALID (ADR-0092 §E → emits `{}\n`);
                            // recurse the tree, validating each `Format` LEAF
                            // operator at any depth (ADR-0094 §G).
                            for value in map.values() {
                                validate_json_format_value(value)?;
                            }
                        }
                        (Some(_), Some(_)) => {
                            return Err(crate::ConfigError::AmbiguousLogFormat {
                                detail: "both text_format_source and json_format are set".into(),
                            });
                        }
                        (None, None) => {
                            return Err(crate::ConfigError::AmbiguousLogFormat {
                                detail: "neither text_format_source nor json_format is set".into(),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Recursively validate a `json_format` value tree (ADR-0094 §G). Every `Format`
/// LEAF (a command-operator string) at any depth is parsed via the EXISTING
/// `envoy_accesslog::parse_format`; a malformed/unknown operator surfaces as the
/// EXISTING `ConfigError::InvalidAccessLogFormat`. `Object`/`Array` recurse;
/// `Bool`/`Null` literal leaves are no-ops. No new `ConfigError` variant.
fn validate_json_format_value(value: &JsonFormatValue) -> Result<(), crate::ConfigError> {
    match value {
        JsonFormatValue::Format(s) => {
            envoy_accesslog::parse_format(s).map_err(|e| {
                crate::ConfigError::InvalidAccessLogFormat {
                    detail: e.to_string(),
                }
            })?;
        }
        JsonFormatValue::Object(map) => {
            for v in map.values() {
                validate_json_format_value(v)?;
            }
        }
        JsonFormatValue::Array(items) => {
            for v in items {
                validate_json_format_value(v)?;
            }
        }
        JsonFormatValue::Bool(_) | JsonFormatValue::Null => {}
    }
    Ok(())
}

/// Validate RFC 7540 wire-format range constraints on Http2ProtocolOptions
/// fields. Hoisted from validate_hcm at 05.3 Task 3 so the listener-side
/// (validate_hcm) and cluster-side (validate's typed_extension walk) sites
/// share the same range checks. Mutates nothing; returns ConfigError on
/// out-of-range values.
fn validate_http2_protocol_options_ranges(
    opts: &Http2ProtocolOptions,
) -> Result<(), crate::ConfigError> {
    const MAX_FRAME_SIZE_RANGE: (u32, u32) = (16384, 16_777_215);
    const WINDOW_SIZE_RANGE: (u32, u32) = (0, (1u32 << 31) - 1);

    if let Some(v) = opts.max_frame_size
        && !(MAX_FRAME_SIZE_RANGE.0..=MAX_FRAME_SIZE_RANGE.1).contains(&v)
    {
        return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
            field: "max_frame_size",
            value: v,
            range: MAX_FRAME_SIZE_RANGE,
        });
    }
    if let Some(v) = opts.initial_stream_window_size
        && v > WINDOW_SIZE_RANGE.1
    {
        return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
            field: "initial_stream_window_size",
            value: v,
            range: WINDOW_SIZE_RANGE,
        });
    }
    if let Some(v) = opts.initial_connection_window_size
        && v > WINDOW_SIZE_RANGE.1
    {
        return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
            field: "initial_connection_window_size",
            value: v,
            range: WINDOW_SIZE_RANGE,
        });
    }
    // max_concurrent_streams has no upper bound per RFC 7540 §6.5.2;
    // zero is valid. No range check.
    Ok(())
}

/// Per-callsite restriction marker for `validate_data_source`.
///
/// Private to this module; the `ConfigError::UnsupportedDataSource.requires`
/// field stays `&'static str` (public-API-stable) and is populated via
/// [`Required::as_str`].
#[derive(Debug, Clone, Copy)]
enum Required {
    Filename,
    InlineString,
}

impl Required {
    fn as_str(self) -> &'static str {
        match self {
            Self::Filename => "filename",
            Self::InlineString => "inline_string",
        }
    }
}

/// Validate a `DataSource` against a per-callsite restriction.
///
/// Cardinality: exactly one of `{filename, inline_string}` is `Some`.
/// `requires` selects which side must be set; the other side must not be.
fn validate_data_source(
    ds: &DataSource,
    field: &'static str,
    requires: Required,
) -> Result<(), crate::ConfigError> {
    let has_file = ds.filename.is_some();
    let has_inline = ds.inline_string.is_some();
    if has_file == has_inline {
        // both Some, or both None
        return Err(crate::ConfigError::UnsupportedDataSource {
            field,
            requires: requires.as_str(),
        });
    }
    let ok = match requires {
        Required::Filename => has_file,
        Required::InlineString => has_inline,
    };
    if !ok {
        return Err(crate::ConfigError::UnsupportedDataSource {
            field,
            requires: requires.as_str(),
        });
    }
    Ok(())
}

/// Returns true if `name` is a syntactically valid DNS name per RFC 1123 LDH
/// rule. Wildcard prefixes (`*.example.com`) return false in 04.1.
fn is_valid_dns_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 253 {
        return false;
    }
    if name.starts_with('*') {
        return false; // wildcard prefix deferred
    }
    name.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// Validate a single [`HeaderMatcher`] and, for SafeRegex modes (top-level or
/// nested via [`StringMatcher`]), compile the regex pattern into
/// `Arc<regex::Regex>` stored back on [`SafeRegex::compiled`]. Phase 04.2
/// (under ADR-0021).
fn validate_header_matcher(hm: &mut HeaderMatcher) -> Result<(), crate::ConfigError> {
    if hm.name.is_empty() {
        return Err(crate::ConfigError::EmptyHeaderName);
    }
    match &mut hm.mode {
        HeaderMatcherMode::ExactMatch(_)
        | HeaderMatcherMode::PrefixMatch(_)
        | HeaderMatcherMode::SuffixMatch(_)
        | HeaderMatcherMode::PresentMatch(_) => {}
        HeaderMatcherMode::SafeRegexMatch(sr) => compile_safe_regex(sr)?,
        HeaderMatcherMode::RangeMatch(r) => {
            if r.start >= r.end {
                return Err(crate::ConfigError::InvalidInt64Range {
                    start: r.start,
                    end: r.end,
                });
            }
        }
        HeaderMatcherMode::StringMatch(sm) => match &mut sm.mode {
            StringMatcherMode::Exact(_)
            | StringMatcherMode::Prefix(_)
            | StringMatcherMode::Suffix(_)
            | StringMatcherMode::Contains(_) => {}
            StringMatcherMode::SafeRegex(sr) => compile_safe_regex(sr)?,
        },
    }
    Ok(())
}

/// Compile `sr.regex` into a [`regex::Regex`] and store it on
/// `sr.compiled`. Returns [`crate::ConfigError::InvalidRegex`] on failure.
fn compile_safe_regex(sr: &mut SafeRegex) -> Result<(), crate::ConfigError> {
    match regex::Regex::new(&sr.regex) {
        Ok(re) => {
            sr.compiled = Some(std::sync::Arc::new(re));
            Ok(())
        }
        Err(e) => Err(crate::ConfigError::InvalidRegex {
            regex: sr.regex.clone(),
            source: e,
        }),
    }
}

impl HeaderMatcher {
    /// Compile any SafeRegex reachable from this matcher (top-level `safe_regex_match` or a
    /// nested `string_match.safe_regex`) into `SafeRegex::compiled`. §A4 (phase 36) — the
    /// RBAC lowering path calls this on its owned clone (the route-config walk uses
    /// `validate_header_matcher`, UNCHANGED). Boot-fatal `InvalidRegex` on a bad pattern.
    pub fn compile_safe_regexes(&mut self) -> Result<(), crate::ConfigError> {
        match &mut self.mode {
            HeaderMatcherMode::SafeRegexMatch(sr) => compile_safe_regex(sr),
            HeaderMatcherMode::StringMatch(sm) => sm.compile_safe_regex(),
            _ => Ok(()),
        }
    }
}

impl StringMatcher {
    /// Compile the SafeRegex mode (if any) into `SafeRegex::compiled`. §A4.
    pub fn compile_safe_regex(&mut self) -> Result<(), crate::ConfigError> {
        if let StringMatcherMode::SafeRegex(sr) = &mut self.mode {
            compile_safe_regex(sr)?;
        }
        Ok(())
    }
}

impl ValueMatcher {
    /// Compile any SafeRegex reachable from this value matcher. §A4. `present_match` → no-op.
    pub fn compile_safe_regexes(&mut self) -> Result<(), crate::ConfigError> {
        match self {
            ValueMatcher::StringMatch(sm) => sm.compile_safe_regex(),
            ValueMatcher::PresentMatch(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Correctly-named terminal router filter, shared by the per-filter
    /// validator test submodules.
    fn router_filter() -> HttpFilter {
        HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
        }
    }

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

    #[test]
    fn parses_minimal_bootstrap() {
        let b = crate::parse_bootstrap(MINIMAL).expect("valid");
        assert_eq!(b.static_resources.listeners.len(), 1);
        assert_eq!(
            b.static_resources.listeners[0]
                .address
                .socket_address
                .port_value,
            10000
        );
    }

    // --- Positive parses ---

    #[test]
    fn parses_bootstrap_with_node_admin_empty_resources() {
        let yaml = r#"
node:
  id: id-1
  cluster: cluster-1
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.node.as_ref().unwrap().id, "id-1");
        assert_eq!(
            b.admin.as_ref().unwrap().address.socket_address.port_value,
            9901
        );
        assert!(b.static_resources.listeners.is_empty());
        assert!(b.static_resources.clusters.is_empty());
    }

    #[test]
    fn parses_bootstrap_with_admin_only() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert!(b.node.is_none());
        assert!(b.admin.is_some());
        assert!(b.static_resources.listeners.is_empty());
    }

    #[test]
    fn parses_bootstrap_with_single_endpoint_cluster() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.static_resources.clusters.len(), 1);
        assert_eq!(b.static_resources.clusters[0].name, "backend");
    }

    #[test]
    fn accepts_node_with_unmodeled_field() {
        // Node deliberately omits deny_unknown_fields (SPEC §D1 inline comment).
        // Upstream Envoy's Node also carries metadata + locality + etc.
        let yaml = r#"
node:
  id: id-1
  cluster: cluster-1
  metadata: { labels: { tier: edge } }
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid");
        assert_eq!(b.node.as_ref().unwrap().id, "id-1");
    }

    // --- Negative validation ---

    #[test]
    fn rejects_unknown_filter_name() {
        // Phase 02.1 widens the validator allow-list from {echo} to
        // {echo, tcp_proxy}. Pick a filter name that sits outside the
        // allow-list.
        //
        // 67.1: this test used to use `envoy.filters.network.rbac` as its
        // "unknown" name. That name is now KNOWN (D1), so `sni_cluster` — still
        // unsupported — takes its place. It sits BEFORE a terminal `echo`
        // because every unknown name is by definition non-terminal, and a chain
        // ENDING in a non-terminal filter is rejected by the bilateral
        // chain-termination rule (D2) before the per-filter allow-list is ever
        // consulted. The assertion itself is unchanged.
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address: { address: 0.0.0.0, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.sni_cluster
            - name: envoy.filters.network.echo
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedFilter(_, _)),
            "got {err:?}"
        );
    }

    // --- 66 (ADR-0123): `envoy.filters.network.direct_response` schema ---

    #[test]
    fn direct_response_filter_parses_inline_string() {
        // Pure schema test: `parse_bootstrap` also validates, and the validate
        // arm does not exist until Task 2.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "hi\n"
"#;
        let bs: Bootstrap = serde_yaml::from_str(yaml).expect("deserializes");
        let f = &bs.static_resources.listeners[0].filter_chains[0].filters[0];
        assert_eq!(f.name, crate::DIRECT_RESPONSE_FILTER);
        let Some(TypedConfig::DirectResponse(dr)) = &f.typed_config else {
            panic!("expected DirectResponse typed_config");
        };
        assert_eq!(dr.response.as_ref().unwrap().inline_string, "hi\n");
    }

    #[test]
    fn direct_response_response_field_is_optional() {
        // Upstream Envoy validates `rc=0` with `response` omitted, and writes
        // zero bytes then closes. Phase-66 SPEC §0 R-0.7.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
"#;
        let bs: Bootstrap = serde_yaml::from_str(yaml).expect("deserializes");
        let f = &bs.static_resources.listeners[0].filter_chains[0].filters[0];
        let Some(TypedConfig::DirectResponse(dr)) = &f.typed_config else {
            panic!("expected DirectResponse typed_config");
        };
        assert!(dr.response.is_none());
    }

    #[test]
    fn direct_response_rejects_inline_bytes_and_filename() {
        // CF-66-1 (ADR-0123 §2.2): envoy-rust supports only the `inline_string`
        // DataSource arm and rejects the others LOUDLY. `DataSourceInline` is
        // `deny_unknown_fields`, so serde raises "unknown field" at deserialize
        // time — before validation runs at all.
        for arm in [r#"inline_bytes: "aGVsbG8=""#, r#"filename: "/tmp/x""#] {
            let yaml = format!(
                r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response: {{ {arm} }}
"#
            );
            let err = serde_yaml::from_str::<Bootstrap>(&yaml)
                .expect_err("must reject non-inline_string DataSource arm");
            assert!(
                err.to_string().contains("unknown field"),
                "expected an unknown-field rejection, got: {err}"
            );
        }
    }

    #[test]
    fn direct_response_filter_validates() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "hi\n"
"#;
        crate::parse_bootstrap(yaml).expect("direct_response must validate");
    }

    #[test]
    fn direct_response_without_typed_config_is_rejected() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must require typed_config");
        assert!(
            matches!(
                err,
                crate::ConfigError::MissingTypedConfig(crate::DIRECT_RESPONSE_FILTER)
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn direct_response_with_wrong_typed_config_variant_is_rejected() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: x
                cluster: nope
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("wrong variant must be rejected");
        assert!(
            matches!(
                err,
                crate::ConfigError::MissingTypedConfig(crate::DIRECT_RESPONSE_FILTER)
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn terminal_network_filter_must_be_last() {
        // SPEC §0 R-0.6: upstream Envoy rejects this exact shape.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "x"
            - name: envoy.filters.network.echo
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("terminal filter not last");
        match err {
            crate::ConfigError::NetworkFilterNotTerminal {
                name,
                position,
                chain_len,
            } => {
                assert_eq!(name, crate::DIRECT_RESPONSE_FILTER);
                assert_eq!(position, 1);
                assert_eq!(chain_len, 2);
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn echo_is_also_terminal() {
        // All four supported network filters are terminal upstream (R-0.6).
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
            - name: envoy.filters.network.direct_response
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config
                response:
                  inline_string: "x"
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("echo must be terminal too");
        assert!(
            matches!(err, crate::ConfigError::NetworkFilterNotTerminal { ref name, .. } if name == crate::ECHO_FILTER),
            "got {err:?}"
        );
    }

    #[test]
    fn single_terminal_filter_chain_is_accepted() {
        // Regression guard for R-0.8: every existing config is single-filter.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
"#;
        crate::parse_bootstrap(yaml).expect("single-filter chain must still validate");
    }

    #[test]
    fn is_terminal_network_filter_covers_all_four_supported_names() {
        for n in [
            crate::ECHO_FILTER,
            crate::TCP_PROXY_FILTER,
            crate::HCM_FILTER,
            crate::DIRECT_RESPONSE_FILTER,
        ] {
            assert!(is_terminal_network_filter(n), "{n} must be terminal");
        }
        assert!(!is_terminal_network_filter(
            "envoy.filters.network.sni_cluster"
        ));
    }

    // --- 67.1 D1 (ADR-0128 / ADR-0129): `envoy.filters.network.rbac` schema ---

    /// Build a bootstrap whose first filter is `rbac` with the given
    /// typed_config body (subsequent lines already indented to 16 spaces),
    /// followed by a terminal `echo`.
    fn network_rbac_yaml(typed_body: &str) -> String {
        format!(
            r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                {typed_body}
            - name: envoy.filters.network.echo
"#
        )
    }

    fn network_rbac_bootstrap(typed_body: &str) -> crate::Bootstrap {
        serde_yaml::from_str(&network_rbac_yaml(typed_body)).expect("parses")
    }

    /// 67.1 D1: a `[rbac, echo]` chain parses, and the rbac filter's
    /// typed_config deserializes into `TypedConfig::NetworkRbac` with `rules`
    /// present.
    #[test]
    fn parses_network_rbac_filter_with_any_matcher() {
        let mut b = network_rbac_bootstrap(
            "stat_prefix: rbac_probe\n                rules:\n                  action: DENY\n                  policies:\n                    p0:\n                      permissions: [{ any: true }]\n                      principals: [{ any: true }]",
        );
        validate(&mut b).expect("validates");
        let f = &b.static_resources.listeners[0].filter_chains[0].filters[0];
        assert_eq!(f.name, crate::NETWORK_RBAC_FILTER);
        let Some(crate::TypedConfig::NetworkRbac(cfg)) = f.typed_config.as_ref() else {
            panic!(
                "expected TypedConfig::NetworkRbac, got {:?}",
                f.typed_config
            );
        };
        assert_eq!(cfg.stat_prefix, "rbac_probe");
        let rules = cfg.rules.as_ref().expect("rules present");
        assert_eq!(rules.action, crate::Action::Deny);
        assert_eq!(rules.policies.len(), 1);
    }

    /// 67.1 D1 / SPEC R-3: `rules` is OPTIONAL. `stat_prefix` alone validates.
    #[test]
    fn parses_network_rbac_filter_with_rules_omitted() {
        let mut b = network_rbac_bootstrap("stat_prefix: norules");
        validate(&mut b).expect("rules is optional (SPEC R-3)");
        let f = &b.static_resources.listeners[0].filter_chains[0].filters[0];
        let Some(crate::TypedConfig::NetworkRbac(cfg)) = f.typed_config.as_ref() else {
            panic!("expected NetworkRbac");
        };
        assert!(cfg.rules.is_none(), "rules: None => INERT (SPEC R-4)");
    }

    /// 67.1 D1 / SPEC R-3: an EMPTY `stat_prefix` is rejected (upstream PGV
    /// `min_len 1`). Note upstream accepts `" "`, so the check is `.is_empty()`,
    /// not `.trim().is_empty()`.
    #[test]
    fn rejects_network_rbac_with_empty_stat_prefix() {
        let mut b = network_rbac_bootstrap(r#"stat_prefix: """#);
        let err = validate(&mut b).expect_err("empty stat_prefix rejected");
        assert!(
            matches!(err, crate::ConfigError::EmptyNetworkRbacStatPrefix { ref listener } if listener == "l0"),
            "got {err:?}",
        );
    }

    /// 67.1 D1: a MISSING `stat_prefix` is a serde missing-field error.
    #[test]
    fn rejects_network_rbac_with_missing_stat_prefix() {
        let yaml = network_rbac_yaml("rules: { policies: {} }");
        let err =
            serde_yaml::from_str::<crate::Bootstrap>(&yaml).expect_err("stat_prefix is required");
        assert!(err.to_string().contains("stat_prefix"), "got {err}");
    }

    /// 67.1 D1 / CF-67-1: `shadow_rules` is rejected LOUDLY by
    /// `deny_unknown_fields`.
    #[test]
    fn rejects_network_rbac_shadow_rules_field() {
        let yaml =
            network_rbac_yaml("stat_prefix: sp\n                shadow_rules: { policies: {} }");
        let err = serde_yaml::from_str::<crate::Bootstrap>(&yaml)
            .expect_err("shadow_rules is not modeled (CF-67-1)");
        assert!(err.to_string().contains("shadow_rules"), "got {err}");
    }

    /// 67.1 D1: the rbac filter without a typed_config is rejected.
    #[test]
    fn rejects_network_rbac_without_typed_config() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
            - name: envoy.filters.network.echo
"#;
        let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
        let err = validate(&mut b).expect_err("typed_config required");
        assert!(
            matches!(
                err,
                crate::ConfigError::MissingTypedConfig(crate::NETWORK_RBAC_FILTER)
            ),
            "got {err:?}",
        );
    }

    /// 67.1 D1 / SPEC R-10: `rbac` is NON-TERMINAL — it must NOT be in
    /// `is_terminal_network_filter`. Its ABSENCE from that predicate IS its
    /// non-terminality. If a future edit adds it, `[rbac, echo]` starts failing
    /// with `NetworkFilterNotTerminal` and this test catches it.
    #[test]
    fn network_rbac_is_not_a_terminal_filter() {
        assert!(!is_terminal_network_filter(crate::NETWORK_RBAC_FILTER));
    }

    /// 67.1 W-1 (ADR-0130): the six RBAC tree/empty-set errors are shared
    /// between the HTTP filter and the NETWORK filter, so their message must NOT
    /// claim "HCM listener" — a network `rbac` filter has no HCM. §7.4 permits
    /// differing error text between the proxies, so this is an internal-quality
    /// guarantee, not a parity one.
    #[test]
    fn shared_rbac_errors_do_not_claim_hcm_scope() {
        let rendered = [
            crate::ConfigError::EmptyRbacPolicies {
                listener: "l0".into(),
            }
            .to_string(),
            crate::ConfigError::EmptyRbacPolicyPermissions {
                listener: "l0".into(),
                policy_name: "p".into(),
            }
            .to_string(),
            crate::ConfigError::EmptyRbacPolicyPrincipals {
                listener: "l0".into(),
                policy_name: "p".into(),
            }
            .to_string(),
            crate::ConfigError::EmptyRbacPermissionSet {
                listener: "l0".into(),
                policy_name: "p".into(),
                path: "permissions[0]".into(),
            }
            .to_string(),
            crate::ConfigError::EmptyRbacPrincipalSet {
                listener: "l0".into(),
                policy_name: "p".into(),
                path: "principals[0]".into(),
            }
            .to_string(),
            crate::ConfigError::RbacTreeTooDeep {
                listener: "l0".into(),
                policy_name: "p".into(),
                depth: 17,
            }
            .to_string(),
        ];
        for msg in rendered {
            assert!(!msg.contains("HCM listener"), "leaked HCM scope: {msg}");
            assert!(
                msg.starts_with(r#"listener "l0""#),
                "unexpected prefix: {msg}"
            );
        }
    }

    /// 67.1 W-1: a network rbac filter with an empty `policies` map surfaces the
    /// SHARED `EmptyRbacPolicies` error, now correctly scoped.
    #[test]
    fn network_rbac_empty_policies_uses_shared_error() {
        let mut b =
            network_rbac_bootstrap("stat_prefix: sp\n                rules: { policies: {} }");
        let err = validate(&mut b).expect_err("empty policies rejected");
        assert!(
            matches!(err, crate::ConfigError::EmptyRbacPolicies { ref listener } if listener == "l0"),
            "got {err:?}",
        );
        assert!(!err.to_string().contains("HCM"), "got {err}");
    }

    // --- 67.1 D2: the BILATERAL chain-termination rule ---

    /// 67.1 D2 / SPEC R-1: a chain whose LAST filter is non-terminal is
    /// REJECTED. The bilateral dual of `NetworkFilterNotTerminal`.
    #[test]
    fn rejects_chain_whose_last_filter_is_non_terminal() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#;
        let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
        let err = validate(&mut b).expect_err("[rbac] alone must be rejected");
        assert!(
            matches!(
                err,
                crate::ConfigError::NetworkFilterChainNotTerminated {
                    ref listener,
                    chain_index: 0,
                    ref last_filter,
                } if listener == "l0" && last_filter == crate::NETWORK_RBAC_FILTER
            ),
            "got {err:?}",
        );
    }

    /// Builds `[<prefix filters>, tcp_proxy(cluster=backend)]` plus a `backend`
    /// cluster, for the ADR-0132 decision-4 composition tests below.
    fn chain_before_tcp_proxy_yaml(prefix_filters: &str) -> String {
        format!(
            r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
      filter_chains:
        - filters:
{prefix_filters}
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: {{ address: 127.0.0.1, port_value: 1 }}
"#
        )
    }

    const RBAC_FILTER_YAML: &str = r#"            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp"#;

    /// A TLS-downstream variant of [`chain_before_tcp_proxy_yaml`]: the single
    /// filter_chain gains a `transport_socket` carrying a `DownstreamTlsContext`
    /// with a (filename-only, wire-shape-valid) leaf cert, so the chain passes the
    /// TLS cert-content validation and reaches the composition check. (67.3 D6.)
    fn chain_before_tcp_proxy_yaml_tls(prefix_filters: &str) -> String {
        let base = chain_before_tcp_proxy_yaml(prefix_filters);
        base.replace(
            "      filter_chains:\n        - filters:",
            "      filter_chains:\n        - transport_socket:\n            name: envoy.transport_sockets.tls\n            typed_config:\n              \"@type\": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext\n              common_tls_context:\n                tls_certificates:\n                  - certificate_chain: { filename: /tmp/leaf.pem }\n                    private_key: { filename: /tmp/leaf.key }\n          filters:",
        )
    }

    /// 67.3 D5/D6 (ADR-0135): the plaintext `[rbac, tcp_proxy]` composition is now
    /// SUPPORTED (the establishment/data split — `tcp_proxy` connects upstream and
    /// relays a server-first banner BEFORE the chain's first-byte gate). It must
    /// VALIDATE.
    #[test]
    fn plaintext_rbac_before_tcp_proxy_is_now_accepted() {
        let yaml = chain_before_tcp_proxy_yaml(RBAC_FILTER_YAML);
        let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
        validate(&mut b).expect("plaintext [rbac, tcp_proxy] is supported from 67.3");
    }

    /// 67.3 D6 (ADR-0135, MEASURED): on a TLS-DOWNSTREAM listener upstream Envoy
    /// establishes the upstream at raw-TCP accept and defers the RBAC verdict to
    /// the first DECRYPTED byte — an ordering envoy-rust's TLS handler does not yet
    /// reproduce. That composition stays fail-loud (owner CF-67-7).
    #[test]
    fn tls_rbac_before_tcp_proxy_is_still_rejected() {
        let yaml = chain_before_tcp_proxy_yaml_tls(RBAC_FILTER_YAML);
        let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
        let err = validate(&mut b).expect_err("TLS [rbac, tcp_proxy] stays rejected until CF-67-7");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedNetworkFilterChainComposition {
                    ref listener,
                    chain_index: 0,
                    ref non_terminal,
                    ref terminal,
                } if listener == "l0"
                    && non_terminal == crate::NETWORK_RBAC_FILTER
                    && terminal == crate::TCP_PROXY_FILTER
            ),
            "got {err:?}",
        );
        // Never silent: the message names its owning carry-forward.
        assert!(
            err.to_string().contains("CF-67-7"),
            "message must name its owner; got {err}"
        );
    }

    /// ADR-0132 decision 4 does NOT reject `tcp_proxy` — only the COMPOSITION.
    /// A lone `tcp_proxy` chain (fixture `0003`) must still validate.
    #[test]
    fn lone_tcp_proxy_chain_is_still_accepted() {
        let yaml = chain_before_tcp_proxy_yaml("");
        let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
        validate(&mut b).expect("a lone tcp_proxy chain stays valid");
    }

    /// 67.1 D2 / SPEC R-5: ERROR PRECEDENCE. `[echo, rbac, tcp_proxy]` violates the
    /// terminal-not-last rule (echo is terminal but not last). That error fires
    /// from the in-order scan regardless of 67.3's composition narrowing (the
    /// plaintext chain no longer hits the composition rule at all — its
    /// `transport_socket` is None — so terminal-not-last is the sole violation).
    #[test]
    fn terminal_not_last_wins_for_echo_rbac_tcp_proxy() {
        let prefix = format!("            - name: envoy.filters.network.echo\n{RBAC_FILTER_YAML}");
        let yaml = chain_before_tcp_proxy_yaml(&prefix);
        let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
        let err = validate(&mut b).expect_err("[echo, rbac, tcp_proxy] must be rejected");
        assert!(
            matches!(
                err,
                crate::ConfigError::NetworkFilterNotTerminal { ref name, .. }
                    if name == crate::ECHO_FILTER
            ),
            "got {err:?}",
        );
    }

    /// 67.1 D2 / SPEC R-5: ERROR PRECEDENCE. `[echo, rbac]` violates BOTH rules
    /// (echo is terminal-but-not-last; rbac is non-terminal-but-last). Upstream
    /// reports the TERMINAL error. An in-order scan reproduces that naturally.
    /// If a future edit moves the chain-termination check BEFORE the terminal
    /// scan, this test catches it.
    #[test]
    fn terminal_not_last_error_wins_over_chain_not_terminated() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
            - name: envoy.filters.network.rbac
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
                stat_prefix: sp
"#;
        let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
        let err = validate(&mut b).expect_err("rejected");
        assert!(
            matches!(err, crate::ConfigError::NetworkFilterNotTerminal { ref name, .. }
                     if name == crate::ECHO_FILTER),
            "terminal-not-last must WIN; got {err:?}",
        );
    }

    /// 67.1 D2 / SPEC R-7: an EMPTY `filters: []` chain is ACCEPTED — measured
    /// parity with upstream Envoy (`configuration OK`). This CLOSES M66-5. The
    /// chain-termination rule applies only to NON-EMPTY chains.
    #[test]
    fn empty_filter_chain_is_accepted() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: { address: 127.0.0.1, port_value: 10000 }
      filter_chains:
        - filters: []
"#;
        let mut b: crate::Bootstrap = serde_yaml::from_str(yaml).expect("parses");
        validate(&mut b).expect("empty chain is upstream parity (SPEC R-7)");
    }

    /// 67.1 D2 / SPEC R-1: `[rbac, echo]` — the happy chain — validates.
    #[test]
    fn accepts_non_terminal_rbac_before_terminal_echo() {
        let mut b = network_rbac_bootstrap("stat_prefix: sp");
        validate(&mut b).expect("[rbac, echo] is valid (SPEC R-1)");
    }

    /// 67.1 D2: every existing single-terminal-filter chain still validates.
    #[test]
    fn single_terminal_filter_chains_still_validate() {
        for name in [crate::ECHO_FILTER, crate::DIRECT_RESPONSE_FILTER] {
            let typed = if name == crate::DIRECT_RESPONSE_FILTER {
                "\n              typed_config:\n                \"@type\": type.googleapis.com/envoy.extensions.filters.network.direct_response.v3.Config\n                response: { inline_string: \"x\" }"
            } else {
                ""
            };
            let yaml = format!(
                r#"
static_resources:
  listeners:
    - name: l0
      address:
        socket_address: {{ address: 127.0.0.1, port_value: 10000 }}
      filter_chains:
        - filters:
            - name: {name}{typed}
"#
            );
            let mut b: crate::Bootstrap = serde_yaml::from_str(&yaml).expect("parses");
            validate(&mut b).unwrap_or_else(|e| panic!("{name} chain must validate: {e}"));
        }
    }

    // --- 67.1 D3 (CF-67-4): the L4 leaf allow-list ---

    /// 67.1 D3 / CF-67-4 / SPEC R-6: a `header` matcher in a NETWORK rbac config
    /// is REJECTED — PARITY with upstream Envoy, which rejects it at config load.
    #[test]
    fn network_rbac_rejects_header_permission() {
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  action: ALLOW\n                  policies:\n                    p0:\n                      permissions: [{ header: { name: \":path\", exact_match: \"/x\" } }]\n                      principals: [{ any: true }]",
        );
        let err = validate(&mut b).expect_err("header rejected at L4");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedNetworkRbacMatcher {
                ref policy_name, arm: "header", ref path, ..
            } if policy_name == "p0" && path == "permissions[0]"),
            "got {err:?}",
        );
    }

    /// 67.1 D3 / CF-67-4 / SPEC R-6: `url_path` is a deliberate FAIL-LOUD
    /// divergence (ADR-0049 decision-2 (b)) — upstream ACCEPTS it, but it can
    /// never match at L4.
    #[test]
    fn network_rbac_rejects_url_path_principal_fail_loud() {
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ any: true }]\n                      principals: [{ url_path: { path: { exact: \"/x\" } } }]",
        );
        let err = validate(&mut b).expect_err("url_path rejected at L4");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedNetworkRbacMatcher {
                arm: "url_path", ref path, ..
            } if path == "principals[0]"),
            "got {err:?}",
        );
    }

    /// 67.1 D3 / CF-67-4: `metadata` — same fail-loud posture.
    #[test]
    fn network_rbac_rejects_metadata_permission_fail_loud() {
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ metadata: { filter: f, path: [{ key: k }], value: { string_match: { exact: v } } } }]\n                      principals: [{ any: true }]",
        );
        let err = validate(&mut b).expect_err("metadata rejected at L4");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedNetworkRbacMatcher {
                    arm: "metadata",
                    ..
                }
            ),
            "got {err:?}",
        );
    }

    /// 67.1 D3: the walk RECURSES — a rejected leaf nested under combinators is
    /// still caught, and the reported `path` names the exact position.
    #[test]
    fn network_rbac_rejects_header_nested_under_combinators() {
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions:\n                        - not_rule:\n                            or_rules:\n                              rules:\n                                - any: true\n                                - header: { name: x, exact_match: y }\n                      principals: [{ any: true }]",
        );
        let err = validate(&mut b).expect_err("nested header rejected");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedNetworkRbacMatcher {
                arm: "header", ref path, ..
            } if path == "permissions[0].not_rule.rules[1]"),
            "got {err:?}",
        );
    }

    /// 67.1 D3: `any` + every combinator is ACCEPTED, arbitrarily nested. This
    /// is the whole matcher surface `67.1` ships; `67.2` widens it.
    #[test]
    fn network_rbac_accepts_any_and_all_combinators() {
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  action: DENY\n                  policies:\n                    p0:\n                      permissions:\n                        - and_rules:\n                            rules:\n                              - any: true\n                              - not_rule: { any: false }\n                      principals:\n                        - or_ids:\n                            ids:\n                              - any: true\n                              - not_id: { any: false }",
        );
        validate(&mut b).expect("any + combinators are the 67.1 surface");
    }

    #[test]
    fn cidr_range_contains_ipv4() {
        let c: crate::CidrRange =
            serde_yaml::from_str("address_prefix: 10.1.2.0\nprefix_len: 24").unwrap();
        assert!(c.contains(&"10.1.2.5".parse().unwrap()));
        assert!(c.contains(&"10.1.2.255".parse().unwrap()));
        assert!(!c.contains(&"10.1.3.0".parse().unwrap()));
        assert!(!c.contains(&"11.1.2.5".parse().unwrap()));
    }

    #[test]
    fn cidr_range_boundary_prefix_lengths_v4() {
        // /0 matches everything; /32 is an exact host match.
        let all: crate::CidrRange =
            serde_yaml::from_str("address_prefix: 0.0.0.0\nprefix_len: 0").unwrap();
        assert!(all.contains(&"203.0.113.9".parse().unwrap()));
        let host: crate::CidrRange =
            serde_yaml::from_str("address_prefix: 192.0.2.7\nprefix_len: 32").unwrap();
        assert!(host.contains(&"192.0.2.7".parse().unwrap()));
        assert!(!host.contains(&"192.0.2.8".parse().unwrap()));
    }

    #[test]
    fn cidr_range_ipv6_and_128() {
        // IPv6 literals ending in `::` must be QUOTED in YAML: an unquoted plain
        // scalar ending in a colon-before-newline is scanned as a mapping key.
        let c: crate::CidrRange =
            serde_yaml::from_str("address_prefix: \"2001:db8::\"\nprefix_len: 32").unwrap();
        assert!(c.contains(&"2001:db8:1234::1".parse().unwrap()));
        assert!(!c.contains(&"2001:db9::1".parse().unwrap()));
        let host: crate::CidrRange =
            serde_yaml::from_str("address_prefix: \"::1\"\nprefix_len: 128").unwrap();
        assert!(host.contains(&"::1".parse().unwrap()));
        assert!(!host.contains(&"::2".parse().unwrap()));
    }

    #[test]
    fn cidr_range_ipv4_mapped_ipv6_peer_matches_ipv4_range() {
        // ADR-0133: a v4-mapped-v6 peer is canonicalised to IPv4 before matching.
        let c: crate::CidrRange =
            serde_yaml::from_str("address_prefix: 127.0.0.0\nprefix_len: 8").unwrap();
        assert!(c.contains(&"::ffff:127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn cidr_range_cross_family_never_matches() {
        let v4: crate::CidrRange =
            serde_yaml::from_str("address_prefix: 10.0.0.0\nprefix_len: 8").unwrap();
        assert!(!v4.contains(&"2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn cidr_range_validate_rejects_over_wide_prefix() {
        let bad_v4: crate::CidrRange =
            serde_yaml::from_str("address_prefix: 10.0.0.0\nprefix_len: 33").unwrap();
        assert!(bad_v4.validate().is_err());
        let bad_v6: crate::CidrRange =
            serde_yaml::from_str("address_prefix: \"2001:db8::\"\nprefix_len: 129").unwrap();
        assert!(bad_v6.validate().is_err());
        let ok: crate::CidrRange =
            serde_yaml::from_str("address_prefix: 10.0.0.0\nprefix_len: 32").unwrap();
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn cidr_range_validate_rejects_ipv4_mapped_ipv6_over_wide_prefix() {
        // phase-67.2 REVIEW C-1 regression. `IpAddr::from_str` parses an
        // IPv4-mapped-IPv6 literal as `IpAddr::V6`, but `contains` canonicalises
        // it to a 4-byte IPv4 before indexing. `validate` must therefore size the
        // prefix against the CANONICAL (IPv4, ≤32) family — NOT the raw IPv6
        // (≤128) family — or it accepts a width `contains`/`prefix_match` then
        // indexes out of bounds, panicking the data plane on the first connection.
        for prefix_len in [33u8, 39, 40, 64, 128] {
            let c: crate::CidrRange = serde_yaml::from_str(&format!(
                "address_prefix: \"::ffff:127.0.0.0\"\nprefix_len: {prefix_len}"
            ))
            .unwrap();
            assert!(
                c.validate().is_err(),
                "v4-mapped-v6 prefix /{prefix_len} must be rejected (canonical IPv4 caps at /32)"
            );
            // And — the load-bearing half — evaluating the arm must not panic,
            // regardless of validation, for either an IPv4 or a v4-mapped peer.
            let _ = c.contains(&"127.0.0.1".parse().unwrap());
            let _ = c.contains(&"::ffff:127.0.0.1".parse().unwrap());
        }
        // A mapped prefix within the canonical IPv4 width still validates and
        // matches (equivalence with the plain-IPv4 spelling is preserved).
        let ok: crate::CidrRange =
            serde_yaml::from_str("address_prefix: \"::ffff:127.0.0.0\"\nprefix_len: 8").unwrap();
        assert!(ok.validate().is_ok());
        assert!(ok.contains(&"127.0.0.42".parse().unwrap()));
        assert!(ok.contains(&"::ffff:127.0.0.42".parse().unwrap()));
        assert!(!ok.contains(&"10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn cidr_range_contains_never_panics_for_validated_prefixes() {
        // phase-67.2 REVIEW I-1: `contains` is data-plane-only, so the
        // `parse_bootstrap` fuzzer (deserialize + validate) can never reach it —
        // gate (d) was structurally blind to C-1. This is the missing coverage: an
        // exhaustive property check that for EVERY `validate`-passing `CidrRange`
        // and a representative sweep of peer/local addresses (v4, v6, and the
        // v4-mapped-v6 forms that triggered C-1), `contains` returns a bool and
        // never panics. It fails (panics with an index-out-of-bounds) against the
        // pre-C-1 `validate`, and passes once `validate` uses the canonical family.
        let prefixes = [
            "0.0.0.0",
            "127.0.0.0",
            "10.1.2.0",
            "255.255.255.255",
            "::",
            "::1",
            "2001:db8::",
            "::ffff:0.0.0.0",
            "::ffff:127.0.0.0",
            "::ffff:255.255.255.255",
            "ffff::",
        ];
        let addrs: Vec<std::net::IpAddr> = [
            "0.0.0.0",
            "127.0.0.1",
            "10.1.2.5",
            "255.255.255.255",
            "::",
            "::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.5",
            "fe80::1",
        ]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();

        for p in prefixes {
            for prefix_len in 0u8..=128 {
                let cidr = crate::CidrRange {
                    address_prefix: p.parse().unwrap(),
                    prefix_len,
                };
                // Only validated prefixes reach the data plane — mirror that gate.
                if cidr.validate().is_err() {
                    continue;
                }
                for addr in &addrs {
                    // The assertion is "does not panic"; the bool value itself is
                    // covered by the direct-match tests above.
                    let _ = cidr.contains(addr);
                }
            }
        }
    }

    #[test]
    fn cidr_range_rejects_unknown_field_and_wrapper_prefix_len() {
        // deny_unknown_fields + bare-u8 prefix_len (ADR-0133 divergence: Envoy also
        // accepts `prefix_len: {value: N}`, envoy-rust rejects it fail-loud).
        assert!(
            serde_yaml::from_str::<crate::CidrRange>(
                "address_prefix: 10.0.0.0\nprefix_len: 8\nextra: 1"
            )
            .is_err()
        );
        assert!(
            serde_yaml::from_str::<crate::CidrRange>(
                "address_prefix: 10.0.0.0\nprefix_len: {value: 8}"
            )
            .is_err()
        );
    }

    /// 67.2 D2/D4: the connection-level arms now EXIST, deserialize, and pass the
    /// widened L4 allow-list. (Supersedes `..._do_not_exist_yet`, which pinned the
    /// pre-67.2 unknown-key rejection.)
    #[test]
    fn network_rbac_accepts_connection_matcher_arms() {
        for arm in ["direct_remote_ip", "remote_ip", "source_ip"] {
            let mut b = network_rbac_bootstrap(&format!(
                "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{{ any: true }}]\n                      principals: [{{ {arm}: {{ address_prefix: 10.0.0.0, prefix_len: 8 }} }}]"
            ));
            validate(&mut b).unwrap_or_else(|e| panic!("{arm} must validate: {e:?}"));
        }
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ destination_port: 8080 }, { destination_ip: { address_prefix: 127.0.0.0, prefix_len: 8 } }]\n                      principals: [{ any: true }]",
        );
        validate(&mut b).expect("destination_port + destination_ip must validate");
    }

    /// 67.2 D1/D4: an out-of-range prefix in a network rbac CidrRange is rejected
    /// with `InvalidCidrRange`, naming the policy + path.
    #[test]
    fn network_rbac_rejects_invalid_cidr_prefix_len() {
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ any: true }]\n                      principals: [{ direct_remote_ip: { address_prefix: 10.0.0.0, prefix_len: 99 } }]",
        );
        let err = validate(&mut b).expect_err("prefix_len 99 on IPv4 is invalid");
        assert!(
            matches!(err, crate::ConfigError::InvalidCidrRange { ref policy_name, ref path, .. }
                if policy_name == "p0" && path == "principals[0]"),
            "got {err:?}",
        );
    }

    #[test]
    fn network_rbac_rejects_ipv4_mapped_ipv6_over_wide_cidr_at_config_load() {
        // phase-67.2 REVIEW C-1, end-to-end through the real config path
        // (`validate(&mut bootstrap)`, not just `CidrRange::validate`). An
        // IPv4-mapped-IPv6 address_prefix with prefix_len > 32 must be rejected
        // fail-loud at load — otherwise it validates and later panics the data
        // plane. Before the fix, `validate` accepted this (raw IPv6 → ≤128).
        let mut b = network_rbac_bootstrap(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{ any: true }]\n                      principals: [{ direct_remote_ip: { address_prefix: \"::ffff:127.0.0.0\", prefix_len: 40 } }]",
        );
        let err = validate(&mut b).expect_err("v4-mapped-v6 /40 must be rejected at load");
        assert!(
            matches!(err, crate::ConfigError::InvalidCidrRange { ref policy_name, ref path, .. }
                if policy_name == "p0" && path == "principals[0]"),
            "got {err:?}",
        );
    }

    /// 67.1 D3: depth is bounded BEFORE the L4 walk recurses, so
    /// `RbacTreeTooDeep` wins over `UnsupportedNetworkRbacMatcher` on a deep tree
    /// with a bad leaf. This pins the ordering that keeps the L4 walk stack-safe.
    #[test]
    fn network_rbac_depth_bound_precedes_the_l4_walk() {
        let mut inner = String::from("{ header: { name: x, exact_match: y } }");
        for _ in 0..20 {
            inner = format!("{{ not_rule: {inner} }}");
        }
        let mut b = network_rbac_bootstrap(&format!(
            "stat_prefix: sp\n                rules:\n                  policies:\n                    p0:\n                      permissions: [{inner}]\n                      principals: [{{ any: true }}]"
        ));
        let err = validate(&mut b).expect_err("too deep");
        assert!(
            matches!(err, crate::ConfigError::RbacTreeTooDeep { .. }),
            "depth bound must run first; got {err:?}",
        );
    }

    /// 67.1 / REVIEW.md I-2. `validate_rbac_rules` runs BEFORE the L4 walk, so a
    /// STRUCTURALLY-invalid `metadata` leaf raises `RbacMetadataMatcherInvalid`,
    /// not `UnsupportedNetworkRbacMatcher` — reachable from a network `rbac`
    /// filter, which has NO HCM. The message must therefore be scope-neutral.
    ///
    /// REVERT THE MESSAGE TO `"HCM listener {listener:?}: …"` AND THIS TEST FAILS.
    #[test]
    fn structurally_invalid_metadata_leaf_reports_a_scope_neutral_listener_error() {
        let mut b = network_rbac_bootstrap(
            r#"stat_prefix: sp
                rules:
                  policies:
                    p0:
                      permissions: [{ metadata: { filter: "", path: [{ key: k }], value: { string_match: { exact: v } } } }]
                      principals: [{ any: true }]"#,
        );
        let err = validate(&mut b).expect_err("an empty metadata `filter` is malformed");
        assert!(
            matches!(err, crate::ConfigError::RbacMetadataMatcherInvalid { .. }),
            "the structural tree validator runs first, not the L4 walk; got {err:?}",
        );
        let msg = err.to_string();
        assert!(
            !msg.contains("HCM listener"),
            "a network rbac filter has no HCM; got {msg}",
        );
        assert!(msg.starts_with("listener "), "got {msg}");
    }

    /// The other half of I-2's ordering claim: a WELL-FORMED `metadata` leaf gets
    /// past `validate_rbac_rules` and is then rejected by the L4 allow-list walk.
    /// Together with the test above this pins which validator owns which input.
    #[test]
    fn well_formed_metadata_leaf_is_rejected_by_the_l4_walk_instead() {
        let mut b = network_rbac_bootstrap(
            r#"stat_prefix: sp
                rules:
                  policies:
                    p0:
                      permissions: [{ metadata: { filter: f, path: [{ key: k }], value: { string_match: { exact: v } } } }]
                      principals: [{ any: true }]"#,
        );
        let err = validate(&mut b).expect_err("metadata cannot be evaluated at L4");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedNetworkRbacMatcher {
                    arm: "metadata",
                    ..
                }
            ),
            "got {err:?}",
        );
    }

    /// 67.1 D2 — closes M66-6. The pre-pass iterates
    /// `static_listeners.chain(dynamic_listeners)`, so BOTH terminal rules apply
    /// to LDS-loaded dynamic listeners. Phase 66 landed the terminal rule with
    /// no dynamic-listener test; this is it.
    #[test]
    fn network_filter_terminal_rules_apply_to_dynamic_lds_listeners() {
        let listener_yaml = r#"
name: dyn0
address:
  socket_address: { address: 127.0.0.1, port_value: 10000 }
filter_chains:
  - filters:
      - name: envoy.filters.network.rbac
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.rbac.v3.RBAC
          stat_prefix: sp
"#;
        let dynamic: crate::Listener = serde_yaml::from_str(listener_yaml).expect("parses");
        let mut b: crate::Bootstrap =
            serde_yaml::from_str("static_resources: { listeners: [] }").expect("parses");
        b.dynamic_listeners = Some(vec![dynamic]);
        let err = validate(&mut b).expect_err("a dynamic listener's [rbac]-only chain is rejected");
        assert!(
            matches!(err, crate::ConfigError::NetworkFilterChainNotTerminated { ref listener, .. }
                     if listener == "dyn0"),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_empty_listeners_with_no_admin() {
        let yaml = "static_resources:\n  listeners: []\n";
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
    }

    #[test]
    fn rejects_bootstrap_with_neither_admin_nor_listener() {
        // Same as rejects_empty_listeners_with_no_admin but via an empty doc.
        let err = crate::parse_bootstrap("{}").expect_err("must reject");
        assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
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
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::TooManyListeners(2)),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_malformed_yaml() {
        let err = crate::parse_bootstrap("::: not yaml :::").expect_err("must fail");
        assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");
    }

    // --- deny_unknown_fields regressions (SPEC §D1 + phase-00 N2 closure) ---

    fn assert_unknown_field(err: crate::ConfigError) {
        let debug_str = format!("{err:?}");
        let display_str = format!("{err}");
        let debug_full = format!("{err:#?}");
        let contains_unknown = debug_str.contains("unknown field")
            || display_str.contains("unknown field")
            || debug_full.contains("unknown field");
        assert!(
            contains_unknown,
            "expected `unknown field` in error; got debug_str={}, display_str={}, debug_full={}",
            debug_str, display_str, debug_full
        );
    }

    #[test]
    fn rejects_unknown_bootstrap_field() {
        let yaml = format!("{MINIMAL}\nbogus_field: true\n");
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_admin_field() {
        let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
  bogus: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_cluster_field() {
        let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
      bogus: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

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
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    // --- N2 closure: 5 deeper structs (STATE.md lines 87–90) ---

    #[test]
    fn rejects_unknown_static_resources_field() {
        let yaml = r#"
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources:
  bogus_sr_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_address_field() {
        let yaml = r#"
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
    bogus_addr_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_socket_address_field() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
      bogus_sa_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_filter_chain_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters: [{ name: envoy.filters.network.echo }]
          bogus_fc_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_network_filter_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              bogus_nf_field: 1
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn parses_bootstrap_with_tcp_proxy_filter() {
        let yaml = r#"
static_resources:
  listeners:
    - name: listener_0
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b: Bootstrap = serde_yaml::from_str(yaml).expect("valid YAML");
        let filter = &b.static_resources.listeners[0].filter_chains[0].filters[0];
        assert_eq!(filter.name, "envoy.filters.network.tcp_proxy");
        match filter.typed_config.as_ref().expect("typed_config present") {
            TypedConfig::TcpProxy(tp) => {
                assert_eq!(tp.stat_prefix, "ingress_tcp");
                assert_eq!(tp.cluster, "backend");
            }
            other => panic!("unexpected typed_config variant: {other:?}"),
        }
    }

    #[test]
    fn rejects_typed_config_unknown_type_url() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.not_tcp_proxy.v3.NotTcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("@type"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }

    #[test]
    fn rejects_unknown_tcp_proxy_config_field() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
                idle_timeout: 0s
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(msg.contains("unknown field"), "got {msg}");
    }

    #[test]
    fn parses_bootstrap_with_round_robin_multi_endpoint_cluster() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10002
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10003
  listeners: []
"#;
        let b: Bootstrap = serde_yaml::from_str(yaml).expect("valid YAML");
        assert_eq!(b.static_resources.clusters.len(), 1);
        let c = &b.static_resources.clusters[0];
        assert_eq!(c.name, "backend");
        assert!(matches!(c.cluster_type, ClusterType::Static));
        assert!(matches!(c.lb_policy, LbPolicy::RoundRobin));
        assert_eq!(c.load_assignment.as_ref().unwrap().cluster_name, "backend");
        assert_eq!(c.load_assignment.as_ref().unwrap().endpoints.len(), 1);
        assert_eq!(
            c.load_assignment.as_ref().unwrap().endpoints[0]
                .lb_endpoints
                .len(),
            3
        );
        assert_eq!(
            c.load_assignment.as_ref().unwrap().endpoints[0].lb_endpoints[2]
                .endpoint
                .address
                .socket_address
                .port_value,
            10003
        );
    }

    #[test]
    fn rejects_cluster_type_logical_dns() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: LOGICAL_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("LOGICAL_DNS"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }

    #[test]
    fn rejects_tcp_proxy_without_typed_config() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingTypedConfig(_)),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_lb_policy_least_request() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: LEAST_REQUEST
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("LEAST_REQUEST"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }

    // ---- Phase 28 Task 3: RING_HASH lb_policy + ring_hash_lb_config ----

    #[test]
    fn parses_lb_policy_ring_hash() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: RING_HASH
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        assert!(
            matches!(c.lb_policy, LbPolicy::RingHash),
            "got {:?}",
            c.lb_policy
        );
    }

    #[test]
    fn parses_ring_hash_lb_config_minimum_ring_size() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: RING_HASH
      ring_hash_lb_config:
        minimum_ring_size: 1024
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        let cfg = c
            .ring_hash_lb_config
            .as_ref()
            .expect("ring_hash_lb_config present");
        assert_eq!(cfg.minimum_ring_size, 1024);
    }

    #[test]
    fn ring_hash_lb_config_absent_is_none() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: RING_HASH
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        assert!(c.ring_hash_lb_config.is_none());
    }

    #[test]
    fn ring_hash_lb_config_empty_applies_defaults() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: RING_HASH
      ring_hash_lb_config: {}
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        let cfg = c
            .ring_hash_lb_config
            .as_ref()
            .expect("ring_hash_lb_config present");
        assert_eq!(cfg.minimum_ring_size, 1024);
        assert_eq!(cfg.maximum_ring_size, 8388608);
        assert!(matches!(cfg.hash_function, HashFunction::XxHash));
    }

    #[test]
    fn rejects_hash_function_murmur_hash_2() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: RING_HASH
      ring_hash_lb_config:
        hash_function: MURMUR_HASH_2
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedHashFunction { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_hash_function_bogus_value() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: RING_HASH
      ring_hash_lb_config:
        hash_function: BOGUS_HASH
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown variant") || msg.contains("BOGUS_HASH"),
            "expected serde tagged-enum rejection; got {msg}",
        );
    }

    #[test]
    fn rejects_ring_size_inversion() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: RING_HASH
      ring_hash_lb_config:
        minimum_ring_size: 4096
        maximum_ring_size: 1024
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::RingSizeInversion { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn accepts_ring_hash_lb_config_on_non_ring_hash_cluster() {
        // Upstream Envoy accepts-and-ignores ring_hash_lb_config on a
        // non-RING_HASH cluster; envoy-rust matches that (validation gated to
        // RING_HASH clusters). Even an otherwise-invalid sub-config (here a
        // ring-size inversion) parses without error on a ROUND_ROBIN cluster.
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      ring_hash_lb_config:
        minimum_ring_size: 4096
        maximum_ring_size: 1024
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML — accept-and-ignore");
        let c = &b.static_resources.clusters[0];
        assert!(matches!(c.lb_policy, LbPolicy::RoundRobin));
        assert!(c.ring_hash_lb_config.is_some());
    }

    // ---- Phase 29 Task 2: MAGLEV lb_policy + maglev_lb_config ----

    #[test]
    fn parses_lb_policy_maglev() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        assert!(
            matches!(c.lb_policy, LbPolicy::Maglev),
            "got {:?}",
            c.lb_policy
        );
    }

    #[test]
    fn maglev_lb_config_empty_applies_default_table_size() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      maglev_lb_config: {}
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        let cfg = c
            .maglev_lb_config
            .as_ref()
            .expect("maglev_lb_config present");
        assert_eq!(cfg.table_size, 65537);
    }

    #[test]
    fn maglev_lb_config_absent_is_none() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        assert!(c.maglev_lb_config.is_none());
    }

    #[test]
    fn maglev_lb_config_explicit_table_size() {
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      maglev_lb_config:
        table_size: 65537
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        let cfg = c
            .maglev_lb_config
            .as_ref()
            .expect("maglev_lb_config present");
        assert_eq!(cfg.table_size, 65537);
    }

    #[test]
    fn accepts_maglev_lb_config_on_non_maglev_cluster() {
        // Upstream Envoy accepts-and-ignores maglev_lb_config on a non-MAGLEV
        // cluster; envoy-rust matches that (validation gated to MAGLEV clusters,
        // Task 3 — ADR-0072). Even an otherwise-invalid sub-config (here a
        // non-prime table_size: 100, which a MAGLEV cluster would reject as
        // startup-fatal) parses without error on a ROUND_ROBIN cluster — proving
        // the validation is genuinely gated (mirrors
        // accepts_ring_hash_lb_config_on_non_ring_hash_cluster).
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      maglev_lb_config:
        table_size: 100
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML — accept-and-ignore");
        let c = &b.static_resources.clusters[0];
        assert!(matches!(c.lb_policy, LbPolicy::RoundRobin));
        assert!(c.maglev_lb_config.is_some());
    }

    // ---- Phase 29 Task 3: MAGLEV table_size validation (prime/over-max, gated) ----

    #[test]
    fn rejects_non_prime_maglev_table_size() {
        // ADR-0072: a MAGLEV cluster's non-prime table_size is startup-fatal
        // (Envoy: "The table size of maglev must be prime number").
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      maglev_lb_config:
        table_size: 100
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MaglevTableSizeNotPrime { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_over_max_maglev_table_size() {
        // ADR-0072: a MAGLEV cluster's table_size > 5000011 is startup-fatal
        // (PGV: value must be less than or equal to 5000011).
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      maglev_lb_config:
        table_size: 5000012
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MaglevTableSizeTooLarge { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn accepts_max_maglev_table_size() {
        // ADR-0072: exactly 5000011 (prime, == the maximum) boots.
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      maglev_lb_config:
        table_size: 5000011
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML — 5000011 is prime and == max");
        let c = &b.static_resources.clusters[0];
        let cfg = c
            .maglev_lb_config
            .as_ref()
            .expect("maglev_lb_config present");
        assert_eq!(cfg.table_size, 5_000_011);
    }

    #[test]
    fn accepts_default_prime_maglev_table_size() {
        // ADR-0072: the default table_size (65537, applied for `maglev_lb_config: {}`)
        // is prime and within bounds — boots.
        let yaml = r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
      maglev_lb_config: {}
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let b = crate::parse_bootstrap(yaml).expect("valid YAML — default 65537 is prime");
        let c = &b.static_resources.clusters[0];
        let cfg = c
            .maglev_lb_config
            .as_ref()
            .expect("maglev_lb_config present");
        assert_eq!(cfg.table_size, 65537);
    }

    #[test]
    fn is_prime_helper() {
        // ADR-0072: primality test for MaglevLbConfig.table_size.
        assert!(super::is_prime(65537));
        assert!(super::is_prime(5_000_011));
        assert!(super::is_prime(2));
        assert!(super::is_prime(3));
        assert!(!super::is_prime(100));
        assert!(!super::is_prime(9));
        assert!(!super::is_prime(0));
        assert!(!super::is_prime(1));
    }

    #[test]
    fn rejects_echo_with_typed_config() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.echo
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnexpectedTypedConfig(_)),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_tcp_proxy_naming_missing_cluster() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
                cluster: nonexistent
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnknownCluster(ref s) if s == "nonexistent"),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_load_assignment_cluster_name_mismatch() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: drift
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::LoadAssignmentNameMismatch { ref cluster, ref assignment }
                    if cluster == "backend" && assignment == "drift"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_empty_lb_endpoints() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints: []
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::EmptyClusterEndpoints(ref s) if s == "backend"),
            "got {err:?}",
        );
    }

    #[test]
    fn rejects_malformed_endpoint_address() {
        // Parse-layer *acceptance*: serde sees a valid Address/SocketAddress
        // shape (address: String, port_value: u16). The SocketAddr parse
        // failure surfaces in envoy-cluster::from_bootstrap at construction
        // time (see envoy-cluster Task 7's ClusterError::EndpointParse test).
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: not-a-host
                      port_value: 10001
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let b = crate::parse_bootstrap(yaml).expect("serde accepts; SocketAddr parse defers");
        assert_eq!(
            b.static_resources.clusters[0]
                .load_assignment
                .as_ref()
                .unwrap()
                .endpoints[0]
                .lb_endpoints[0]
                .endpoint
                .address
                .socket_address
                .address,
            "not-a-host",
        );
    }

    #[test]
    fn rejects_unknown_load_assignment_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
        bogus_la_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_locality_lb_endpoints_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
            bogus_lle_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_lb_endpoint_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                bogus_lbe_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn rejects_unknown_endpoint_field() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
                  bogus_ep_field: 1
  listeners: []
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn fuzz_corpus_seeds_parse_or_reject_cleanly() {
        let root = env!("CARGO_MANIFEST_DIR");
        // Seeds expected to parse + validate successfully.
        for fname in &[
            "fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml",
            "fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml",
            "fuzz/corpus/parse_bootstrap/tls_downstream_single_cert.yaml",
            "fuzz/corpus/parse_bootstrap/tls_upstream_validation_context.yaml",
            "fuzz/corpus/parse_bootstrap/tls_multi_cert_sni.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_direct_response_happy.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_route_to_cluster.yaml",
            "fuzz/corpus/parse_bootstrap/route_with_header_matchers.yaml",
            "fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml",
            "fuzz/corpus/parse_bootstrap/admin_with_stats_route.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_access_log_file.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_header_mutation_filter.yaml",
            "fuzz/corpus/parse_bootstrap/admin_multi_endpoint_bootstrap.yaml",
            "fuzz/corpus/parse_bootstrap/admin_healthcheck_bootstrap.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_local_rate_limit_filter.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_rbac_filter.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_fault_filter.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_jwt_authn_filter.yaml",
            "fuzz/corpus/parse_bootstrap/cluster_health_check.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_upstream_active_health_check.yaml",
            "fuzz/corpus/parse_bootstrap/cluster_circuit_breakers.yaml",
            "fuzz/corpus/parse_bootstrap/cluster_outlier_detection.yaml", // 14.1 D8.2
            "fuzz/corpus/parse_bootstrap/route_retry_policy.yaml",        // 16 Task 9
            "fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml",
            "fuzz/corpus/parse_bootstrap/dynamic_resources_cds.yaml", // 18 Task 9
            "fuzz/corpus/parse_bootstrap/dynamic_resources_lds.yaml", // 19 Task 9
            "fuzz/corpus/parse_bootstrap/hcm_rds_route_config.yaml",  // 20 Task 9
            "fuzz/corpus/parse_bootstrap/cluster_eds.yaml",           // 21 Task 9
            "fuzz/corpus/parse_bootstrap/route_cors_typed_per_filter_config.yaml", // 23 Task 8
            "fuzz/corpus/parse_bootstrap/route_csrf_typed_per_filter_config.yaml", // 24 Task 5
            "fuzz/corpus/parse_bootstrap/cluster_ring_hash_lb.yaml",  // 28 Task 9
            "fuzz/corpus/parse_bootstrap/cluster_maglev_lb.yaml",     // 29 Task 8
            "fuzz/corpus/parse_bootstrap/cluster_lb_subset.yaml",     // 30 Task 9
            "fuzz/corpus/parse_bootstrap/http_filter_cdn_loop.yaml",  // 31 Task 6
        ] {
            let path = format!("{root}/{fname}");
            let yaml =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            crate::parse_bootstrap(&yaml).unwrap_or_else(|e| panic!("parse {path}: {e}"));
        }
        // Seeds expected to reject cleanly (parse_bootstrap returns Err, not panic).
        for fname in &[
            "fuzz/corpus/parse_bootstrap/tls_malformed_at_type.yaml",
            "fuzz/corpus/parse_bootstrap/tls_overlapping_sni_reject.yaml",
            "fuzz/corpus/parse_bootstrap/hcm_invalid_codec_type.yaml",
        ] {
            let path = format!("{root}/{fname}");
            let yaml =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
            assert!(
                crate::parse_bootstrap(&yaml).is_err(),
                "{path} was expected to reject, but parsed",
            );
        }
        // The minimal.yaml seed is the phase-00 admin-only baseline; assert
        // it still parses (regression gate against schema additions breaking
        // baseline acceptance).
        let minimal = format!("{root}/fuzz/corpus/parse_bootstrap/minimal.yaml");
        let yaml =
            std::fs::read_to_string(&minimal).unwrap_or_else(|e| panic!("read {minimal}: {e}"));
        crate::parse_bootstrap(&yaml).unwrap_or_else(|e| panic!("parse {minimal}: {e}"));
    }

    #[test]
    fn parses_listener_with_downstream_tls_context() {
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let chain = &bootstrap.static_resources.listeners[0].filter_chains[0];
        let ts = chain
            .transport_socket
            .as_ref()
            .expect("transport_socket present");
        assert_eq!(ts.name, "envoy.transport_sockets.tls");
        match &ts.typed_config {
            crate::TransportSocketTypedConfig::Downstream(ctx) => {
                let certs = &ctx.common_tls_context.tls_certificates;
                assert_eq!(certs.len(), 1);
                assert_eq!(
                    certs[0].certificate_chain.filename.as_deref(),
                    Some("/tmp/leaf.pem")
                );
                assert_eq!(
                    certs[0].private_key.filename.as_deref(),
                    Some("/tmp/leaf.key")
                );
            }
            other => panic!("unexpected typed_config: {other:?}"),
        }
    }

    #[test]
    fn parses_cluster_with_upstream_tls_context() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let cluster = &bootstrap.static_resources.clusters[0];
        let ts = cluster
            .transport_socket
            .as_ref()
            .expect("transport_socket present");
        assert_eq!(ts.name, "envoy.transport_sockets.tls");
        match &ts.typed_config {
            crate::TransportSocketTypedConfig::Upstream(ctx) => {
                assert_eq!(ctx.sni, "envoy-rust.test");
                let vc = ctx
                    .common_tls_context
                    .validation_context
                    .as_ref()
                    .expect("validation_context present");
                assert_eq!(vc.trusted_ca.filename.as_deref(), Some("/tmp/ca.pem"));
                assert!(ctx.common_tls_context.tls_certificates.is_empty());
            }
            other => panic!("unexpected typed_config: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_field_in_downstream_tls_context() {
        // require_client_certificate is mTLS-shaped and out of phase 03 per
        // SPEC §4. deny_unknown_fields on DownstreamTlsContext rejects it at
        // parse time.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              require_client_certificate: false
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown field");
        let msg = format!("{err}");
        assert!(
            msg.contains("require_client_certificate") || msg.contains("unknown field"),
            "expected unknown-field error, got: {msg}",
        );
    }

    #[test]
    fn rejects_unknown_field_in_common_tls_context() {
        // alpn_protocols is a phase-04 surface; phase 03 fixtures do not
        // include it (SPEC §6 signpost 14). deny_unknown_fields on
        // CommonTlsContext rejects it.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                alpn_protocols: ["h2"]
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject unknown field");
        let msg = format!("{err}");
        assert!(
            msg.contains("alpn_protocols") || msg.contains("unknown field"),
            "expected unknown-field error, got: {msg}",
        );
    }

    // The phase-03.1 test `rejects_unknown_field_in_data_source` was removed in
    // phase 04.1 Task 1: `inline_string` is now a recognized DataSource field
    // (used by `direct_response.body`), so the YAML it asserted-rejects now
    // parses cleanly. The "exactly one of {filename, inline_string} set" rule
    // — and the per-callsite restriction that TLS still requires `filename` —
    // is the validator's responsibility (phase 04.1 Task 2), and Task 2's
    // validator tests subsume the regression coverage.

    #[test]
    fn rejects_unknown_transport_socket_name() {
        // Phase 03 only accepts "envoy.transport_sockets.tls".
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.raw_buffer
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain:
                      filename: /tmp/leaf.pem
                    private_key:
                      filename: /tmp/leaf.key
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::UnknownTransportSocketName(ref n) if n == "envoy.transport_sockets.raw_buffer"),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_downstream_tls_context_on_cluster() {
        // DownstreamTlsContext on a cluster's transport_socket → MismatchedTransportSocketDirection.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
          common_tls_context:
            tls_certificates:
              - certificate_chain:
                  filename: /tmp/leaf.pem
                private_key:
                  filename: /tmp/leaf.key
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::MismatchedTransportSocketDirection {
                    side: "cluster",
                    got: "DownstreamTlsContext",
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_tls_context_on_listener() {
        // UpstreamTlsContext on a listener's filter_chain.transport_socket →
        // MismatchedTransportSocketDirection.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
              sni: envoy-rust.test
              common_tls_context:
                validation_context:
                  trusted_ca:
                    filename: /tmp/ca.pem
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::MismatchedTransportSocketDirection {
                    side: "listener",
                    got: "UpstreamTlsContext",
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_downstream_with_empty_tls_certificates() {
        // Downstream side requires ≥1 cert; empty → EmptyTlsCertificates { side: "listener" }.
        let yaml = r#"
static_resources:
  listeners:
    - name: l
      address:
        socket_address:
          address: 0.0.0.0
          port_value: 10000
      filter_chains:
        - transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates: []
          filters: []
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::EmptyTlsCertificates { side: "listener" }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_with_tls_certificates() {
        // Upstream side requires 0 certs (mTLS deferred); non-empty →
        // EmptyTlsCertificates { side: "cluster" } (variant naming is asymmetric:
        // "Empty" on listener means too-few, on cluster means too-many; the
        // side discriminator carries the meaning).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context:
            tls_certificates:
              - certificate_chain:
                  filename: /tmp/client.pem
                private_key:
                  filename: /tmp/client.key
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::EmptyTlsCertificates { side: "cluster" }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_without_validation_context() {
        // No insecure-skip in phase 03 (SPEC §4) — validation_context required.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: envoy-rust.test
          common_tls_context: {}
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingValidationContext),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_upstream_with_empty_sni() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 9443
      transport_socket:
        name: envoy.transport_sockets.tls
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.UpstreamTlsContext
          sni: ""
          common_tls_context:
            validation_context:
              trusted_ca:
                filename: /tmp/ca.pem
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::EmptyUpstreamSni),
            "got {err:?}"
        );
    }

    #[test]
    fn parses_filter_chain_with_server_names() {
        let yaml = r#"
filter_chain_match:
  server_names: ["a.example.com", "b.example.com"]
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        let m = chain.filter_chain_match.expect("has filter_chain_match");
        assert_eq!(
            m.server_names,
            vec!["a.example.com".to_string(), "b.example.com".to_string()]
        );
    }

    #[test]
    fn parses_filter_chain_without_filter_chain_match() {
        // Existing 03.1 / 02.2 shape — no filter_chain_match key.
        let yaml = r#"
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        assert!(chain.filter_chain_match.is_none());
    }

    #[test]
    fn parses_filter_chain_match_with_empty_server_names() {
        // `filter_chain_match: {}` is the catch-all shape.
        let yaml = r#"
filter_chain_match: {}
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        let m = chain.filter_chain_match.expect("has filter_chain_match");
        assert!(m.server_names.is_empty());
    }

    #[test]
    fn parses_filter_chain_match_with_explicit_empty_server_names_list() {
        let yaml = r#"
filter_chain_match:
  server_names: []
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let chain: FilterChain = serde_yaml::from_str(yaml).expect("parse");
        let m = chain.filter_chain_match.expect("has filter_chain_match");
        assert!(m.server_names.is_empty());
    }

    #[test]
    fn rejects_filter_chain_match_unknown_field() {
        // deny_unknown_fields discipline: an unrecognized key under filter_chain_match fails.
        let yaml = r#"
filter_chain_match:
  destination_port: 443
filters:
  - name: envoy.filters.network.tcp_proxy
    typed_config:
      "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
      stat_prefix: ingress_tcp
      cluster: backend
"#;
        let err = serde_yaml::from_str::<FilterChain>(yaml).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("destination_port") || msg.contains("unknown field"),
            "expected unknown-field error, got {msg}"
        );
    }

    // --- Phase 03.2 Task 2: cross-chain validator rules ---
    //
    // These tests use the existing `crate::parse_bootstrap` boundary (which
    // performs parse + validate together) instead of the PLAN's bare
    // `bootstrap.validate()` call: phase 03 keeps `validate` crate-private,
    // and every existing validator test in this file already routes through
    // `parse_bootstrap`. Documented as a "test API adaptation" deviation in
    // the Task 2 PROGRESS.md entry.

    #[test]
    fn parses_listener_with_multi_chain_sni_routing() {
        // Happy path: two filter chains, each carrying its own DownstreamTlsContext
        // and disjoint server_names. Validator accepts.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        crate::parse_bootstrap(yaml).expect("parses + validates");
    }

    #[test]
    fn parses_filter_chain_with_empty_server_names_validator() {
        // Single catch-all chain with empty server_names. Accepted by validator.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: [] }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        crate::parse_bootstrap(yaml).expect("parses + validates");
    }

    #[test]
    fn rejects_filter_chains_with_overlapping_sni() {
        // Two chains both declare server_names: ["a.example.com"].
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject overlapping SNI");
        assert!(
            matches!(
                err,
                crate::ConfigError::MultipleListenersWithOverlappingSni { ref listener, ref sni }
                    if listener == "tcp_listener" && sni == "a.example.com"
            ),
            "expected MultipleListenersWithOverlappingSni, got {err:?}"
        );
    }

    #[test]
    fn rejects_filter_chains_with_overlapping_sni_case_insensitive() {
        // Chain A "a.example.com"; chain B "A.Example.com". Match is case-insensitive.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["A.Example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-b.pem }
                    private_key:       { filename: /tmp/leaf-b.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject case-insensitive overlap");
        assert!(
            matches!(
                err,
                crate::ConfigError::MultipleListenersWithOverlappingSni { ref listener, .. }
                    if listener == "tcp_listener"
            ),
            "expected MultipleListenersWithOverlappingSni, got {err:?}"
        );
    }

    #[test]
    fn rejects_multiple_catch_all_filter_chains() {
        // Two chains both have empty server_names.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: [] }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: {}
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject multiple catch-all chains");
        assert!(
            matches!(
                err,
                crate::ConfigError::MultipleCatchAllFilterChains { ref listener }
                    if listener == "tcp_listener"
            ),
            "expected MultipleCatchAllFilterChains, got {err:?}"
        );
    }

    #[test]
    fn rejects_mixed_tls_and_plaintext_filter_chains() {
        // One TLS chain, one plaintext chain on the same listener.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 10010 } }
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/leaf-a.pem }
                    private_key:       { filename: /tmp/leaf-a.key }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
        - filter_chain_match: { server_names: ["b.example.com"] }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 8080 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject mixed TLS and plaintext");
        assert!(
            matches!(
                err,
                crate::ConfigError::MixedTlsAndPlaintextFilterChainsOnListener { ref listener }
                    if listener == "tcp_listener"
            ),
            "expected MixedTlsAndPlaintextFilterChainsOnListener, got {err:?}"
        );
    }

    #[test]
    fn parses_listener_with_hcm_direct_response() {
        let yaml = r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 0 }
"#;
        let bs: Bootstrap = serde_yaml::from_str(yaml).expect("parses");
        let listener = &bs.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let TypedConfig::HttpConnectionManager(hcm) = filter.typed_config.as_ref().unwrap() else {
            panic!("expected HCM variant");
        };
        assert_eq!(hcm.stat_prefix, "ingress_http");
        assert!(matches!(hcm.codec_type, CodecType::HTTP1));
        assert_eq!(hcm.route_config.as_ref().unwrap().virtual_hosts.len(), 1);
        let vh = &hcm.route_config.as_ref().unwrap().virtual_hosts[0];
        assert_eq!(vh.domains, vec!["*".to_string()]);
        let route = &vh.routes[0];
        assert_eq!(route.r#match.prefix.as_deref(), Some("/"));
        let dr = match &route.action {
            RouteAction::DirectResponse(dr) => dr,
            other => panic!("expected DirectResponse, got {other:?}"),
        };
        assert_eq!(dr.status, 200);
        assert_eq!(dr.body.inline_string.as_deref(), Some("ok\n"));
        assert_eq!(hcm.http_filters.len(), 1);
        assert_eq!(hcm.http_filters[0].name, "envoy.filters.http.router");
    }

    #[test]
    fn parses_route_with_path_matcher() {
        let yaml = r#"
prefix: ~
path: "/exact"
"#;
        let m: RouteMatch = serde_yaml::from_str(yaml).expect("parses");
        assert!(m.prefix.is_none());
        assert_eq!(m.path.as_deref(), Some("/exact"));
    }

    #[test]
    fn parses_data_source_with_inline_string() {
        let yaml = r#"
inline_string: "hello"
"#;
        let ds: DataSource = serde_yaml::from_str(yaml).expect("parses");
        assert!(ds.filename.is_none());
        assert_eq!(ds.inline_string.as_deref(), Some("hello"));
    }

    #[test]
    fn parses_data_source_with_filename() {
        let yaml = r#"
filename: "/tmp/cert.pem"
"#;
        let ds: DataSource = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(ds.filename.as_deref(), Some("/tmp/cert.pem"));
        assert!(ds.inline_string.is_none());
    }

    #[test]
    fn rejects_unknown_field_in_hcm_config() {
        // The original sentinel was `access_log`, chosen pre-06.2 because
        // it was not yet recognized by `HttpConnectionManagerConfig`. 06.2
        // Task 5 added `access_log` to the schema; swapping to a fresh
        // not-an-HCM-field sentinel preserves the test's intent (verify
        // that `deny_unknown_fields` still rejects unrecognized fields).
        let yaml = r#"
stat_prefix: ingress_http
codec_type: HTTP1
bogus_hcm_field: 1
route_config:
  name: r
  virtual_hosts: []
http_filters: []
"#;
        let res: Result<HttpConnectionManagerConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            res.is_err(),
            "deny_unknown_fields should reject bogus_hcm_field"
        );
        let err = res.err().unwrap().to_string();
        assert!(
            err.contains("bogus_hcm_field") || err.contains("unknown field"),
            "error mentions unknown field: {err}"
        );
    }

    #[test]
    fn rejects_unknown_field_in_route_match() {
        let yaml = r#"
prefix: "/"
case_sensitive: true
"#;
        let res: Result<RouteMatch, _> = serde_yaml::from_str(yaml);
        assert!(
            res.is_err(),
            "deny_unknown_fields should reject case_sensitive"
        );
    }

    // --- phase 04.1 Task 2: HCM validator tests ---

    fn parse_then_validate(yaml: &str) -> Result<Bootstrap, crate::ConfigError> {
        let mut bs: Bootstrap = serde_yaml::from_str(yaml)?;
        validate(&mut bs)?;
        Ok(bs)
    }

    fn make_hcm_listener_yaml(hcm_block: &str) -> String {
        format!(
            r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
{}
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 0 }}
"#,
            hcm_block
        )
    }

    const VALID_ROUTER_FILTER: &str = r#"
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;

    // Pre-Task-2 `rejects_codec_type_http2` deleted: phase 05.2 Task 2 flipped
    // codec_type: HTTP2 from "rejected" to "accepted on plaintext listeners".
    // The new positive test `parses_hcm_with_codec_type_http2` (above, defined
    // alongside `rejects_hcm_with_codec_type_http2_on_tls_listener`) replaces
    // it; the HTTP3 rejection test below remains the lone codec_type negative.

    #[test]
    fn rejects_codec_type_http3() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP3
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject HTTP3");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedCodecType {
                    got: CodecType::HTTP3
                }
            ),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_unsupported_http_filter() {
        let hcm = r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.lua
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;
        // The @type IS the router (the only schema arm), but `name` is "lua" — validator rejects.
        let yaml = make_hcm_listener_yaml(hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject non-router name");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedHttpFilter { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_route_match_with_both_prefix_and_path() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/x", path: "/y" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject both prefix and path");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedRouteMatcher { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_route_match_with_neither_prefix_nor_path() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{}}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject empty match");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedRouteMatcher { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_direct_response_with_filename_body() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ filename: "/tmp/x" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err =
            parse_then_validate(&yaml).expect_err("should reject filename in direct_response");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedDataSource {
                    field: "direct_response.body",
                    requires: "inline_string"
                }
            ),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_direct_response_with_invalid_status() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 99
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject status < 100");
        assert!(
            matches!(err, crate::ConfigError::InvalidStatusCode { status: 99 }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_empty_virtual_hosts() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts: []
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject empty virtual_hosts");
        assert!(
            matches!(err, crate::ConfigError::EmptyVirtualHosts { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn rejects_empty_domains() {
        let hcm = format!(
            r#"
                stat_prefix: x
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: []
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok" }}
{}"#,
            VALID_ROUTER_FILTER
        );
        let yaml = make_hcm_listener_yaml(&hcm);
        let err = parse_then_validate(&yaml).expect_err("should reject empty domains");
        assert!(
            matches!(err, crate::ConfigError::EmptyDomains { .. }),
            "got: {:?}",
            err
        );
    }

    #[test]
    fn parses_int64_range() {
        let yaml = r#"
start: 1
end: 100
"#;
        let r: Int64Range = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(r.start, 1);
        assert_eq!(r.end, 100);
    }

    #[test]
    fn rejects_unknown_field_in_int64_range() {
        let yaml = r#"
start: 1
end: 100
step: 5
"#;
        let res: Result<Int64Range, _> = serde_yaml::from_str(yaml);
        assert!(res.is_err(), "deny_unknown_fields should reject `step`");
    }

    #[test]
    fn parses_safe_regex() {
        let yaml = r#"
regex: "^v[0-9]+$"
"#;
        let sr: SafeRegex = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(sr.regex, "^v[0-9]+$");
        assert!(sr.compiled.is_none(), "compiled set to None pre-validate");
    }

    #[test]
    fn safe_regex_partial_eq_compares_only_regex_string() {
        let a = SafeRegex {
            regex: "x".into(),
            compiled: None,
        };
        let b = SafeRegex {
            regex: "x".into(),
            compiled: Some(std::sync::Arc::new(regex::Regex::new("x").unwrap())),
        };
        assert_eq!(a, b, "compiled field is opaque to PartialEq");
    }

    #[test]
    fn parses_string_matcher_exact() {
        let yaml = r#"
exact: "foo"
"#;
        let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(sm.mode, StringMatcherMode::Exact("foo".into()));
        assert!(!sm.ignore_case);
    }

    #[test]
    fn parses_string_matcher_contains_with_ignore_case() {
        let yaml = r#"
contains: "beta"
ignore_case: true
"#;
        let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(sm.mode, StringMatcherMode::Contains("beta".into()));
        assert!(sm.ignore_case);
    }

    #[test]
    fn parses_string_matcher_safe_regex() {
        let yaml = r#"
safe_regex:
  regex: "^v[0-9]+$"
"#;
        let sm: StringMatcher = serde_yaml::from_str(yaml).expect("parses");
        match sm.mode {
            StringMatcherMode::SafeRegex(sr) => {
                assert_eq!(sr.regex, "^v[0-9]+$");
                assert!(sr.compiled.is_none());
            }
            other => panic!("expected SafeRegex variant, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_string_matcher_mode_key() {
        let yaml = r#"
weird: "x"
"#;
        let res: Result<StringMatcher, _> = serde_yaml::from_str(yaml);
        assert!(res.is_err(), "unknown mode key should error");
        let err = res.err().unwrap().to_string();
        assert!(
            err.contains("weird") || err.contains("unknown"),
            "error mentions unknown key: {err}"
        );
    }

    #[test]
    fn rejects_two_string_matcher_mode_keys() {
        let yaml = r#"
exact: "a"
prefix: "b"
"#;
        let res: Result<StringMatcher, _> = serde_yaml::from_str(yaml);
        let err = res
            .expect_err("two mode keys should be rejected (each variant is mutually exclusive)")
            .to_string();
        assert!(
            err.contains("multiple mode keys") || err.contains("mutually exclusive"),
            "error should mention mutual exclusivity: {err}"
        );
    }

    #[test]
    fn parses_header_matcher_exact() {
        let yaml = r#"
name: "x-foo"
exact_match: "bar"
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(m.name, "x-foo");
        assert_eq!(m.mode, HeaderMatcherMode::ExactMatch("bar".into()));
        assert!(!m.invert_match);
    }

    #[test]
    fn parses_header_matcher_with_invert_match_true() {
        let yaml = r#"
name: "x-foo"
exact_match: "bar"
invert_match: true
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert!(m.invert_match);
    }

    #[test]
    fn parses_header_matcher_present_match_true() {
        let yaml = r#"
name: "authorization"
present_match: true
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(m.mode, HeaderMatcherMode::PresentMatch(true));
    }

    #[test]
    fn parses_header_matcher_string_match_contains() {
        let yaml = r#"
name: "x-tag"
string_match:
  contains: "beta"
  ignore_case: true
"#;
        let m: HeaderMatcher = serde_yaml::from_str(yaml).expect("parses");
        match m.mode {
            HeaderMatcherMode::StringMatch(sm) => {
                assert_eq!(sm.mode, StringMatcherMode::Contains("beta".into()));
                assert!(sm.ignore_case);
            }
            other => panic!("expected StringMatch variant, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_header_matcher_mode_key() {
        let yaml = r#"
name: "x-foo"
weird_match: "bar"
"#;
        let res: Result<HeaderMatcher, _> = serde_yaml::from_str(yaml);
        let err = res.expect_err("unknown mode key should error").to_string();
        assert!(
            err.contains("weird_match") || err.contains("unknown"),
            "error mentions unknown key: {err}"
        );
    }

    #[test]
    fn rejects_two_header_matcher_mode_keys() {
        let yaml = r#"
name: "x-foo"
exact_match: "a"
prefix_match: "b"
"#;
        let res: Result<HeaderMatcher, _> = serde_yaml::from_str(yaml);
        let err = res
            .expect_err("two mode keys should be rejected (each variant is mutually exclusive)")
            .to_string();
        assert!(
            err.contains("multiple mode keys") || err.contains("mutually exclusive"),
            "error should mention mutual exclusivity: {err}"
        );
    }

    // --- phase 04.2 Task 5: HeaderMatcher validator tests ---

    #[test]
    fn rejects_empty_header_name() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: ""
                                exact_match: "bar"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(
            matches!(err, crate::ConfigError::EmptyHeaderName),
            "expected EmptyHeaderName, got {err:?}"
        );
    }

    #[test]
    fn rejects_invalid_regex_in_safe_regex_match() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                safe_regex_match:
                                  regex: "[unclosed"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(
            matches!(err, crate::ConfigError::InvalidRegex { .. }),
            "expected InvalidRegex, got {err:?}"
        );
    }

    #[test]
    fn rejects_invalid_regex_in_string_match_safe_regex() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                string_match:
                                  safe_regex:
                                    regex: "(?P<oops"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(matches!(err, crate::ConfigError::InvalidRegex { .. }));
    }

    #[test]
    fn rejects_invalid_int64_range_start_eq_end() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                range_match: { start: 100, end: 100 }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidInt64Range {
                    start: 100,
                    end: 100
                }
            ),
            "expected InvalidInt64Range {{100,100}}, got {err:?}"
        );
    }

    #[test]
    fn rejects_invalid_int64_range_start_gt_end() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                range_match: { start: 200, end: 100 }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let err = parse_then_validate(&yaml).expect_err("validator rejects");
        assert!(matches!(err, crate::ConfigError::InvalidInt64Range { .. }));
    }

    #[test]
    fn validator_compiles_safe_regex_match_into_arc() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-version"
                                safe_regex_match:
                                  regex: "^v[0-9]+$"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("not HCM");
        };
        let header_matcher = &hcm.route_config.as_ref().unwrap().virtual_hosts[0].routes[0]
            .r#match
            .headers[0];
        let HeaderMatcherMode::SafeRegexMatch(sr) = &header_matcher.mode else {
            panic!("not SafeRegexMatch");
        };
        assert!(sr.compiled.is_some(), "validator should have compiled");
    }

    #[test]
    fn validator_accepts_all_seven_modes() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - { name: "h1", exact_match: "x" }
                              - { name: "h2", prefix_match: "p" }
                              - { name: "h3", suffix_match: "s" }
                              - { name: "h4", safe_regex_match: { regex: "^v[0-9]+$" } }
                              - { name: "h5", range_match: { start: 1, end: 100 } }
                              - { name: "h6", present_match: true }
                              - { name: "h7", string_match: { contains: "c", ignore_case: true } }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("not HCM");
        };
        assert_eq!(
            hcm.route_config.as_ref().unwrap().virtual_hosts[0].routes[0]
                .r#match
                .headers
                .len(),
            7
        );
    }

    #[test]
    fn validator_accepts_empty_headers_vec() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        crate::parse_bootstrap(&yaml).expect("parses + validates");
    }

    #[test]
    fn validator_accepts_invert_match_true() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-foo"
                                exact_match: "bar"
                                invert_match: true
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        crate::parse_bootstrap(&yaml).expect("parses + validates");
    }

    #[test]
    fn validator_compiles_string_match_safe_regex_into_arc() {
        let yaml = make_hcm_listener_yaml(
            r#"
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match:
                            prefix: "/"
                            headers:
                              - name: "x-tag"
                                string_match:
                                  safe_regex:
                                    regex: "^beta$"
                          direct_response:
                            status: 200
                            body: { inline_string: "ok" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#,
        );
        let bs = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("not HCM");
        };
        let header_matcher = &hcm.route_config.as_ref().unwrap().virtual_hosts[0].routes[0]
            .r#match
            .headers[0];
        let HeaderMatcherMode::StringMatch(sm) = &header_matcher.mode else {
            panic!("not StringMatch");
        };
        let StringMatcherMode::SafeRegex(sr) = &sm.mode else {
            panic!("not SafeRegex");
        };
        assert!(sr.compiled.is_some(), "nested regex should be compiled");
    }

    // --- phase 36 Task 3: public SafeRegex compile helpers ---

    #[test]
    fn header_matcher_compile_safe_regexes_compiles_and_rejects() {
        let mut ok = HeaderMatcher {
            name: "x".into(),
            mode: HeaderMatcherMode::SafeRegexMatch(SafeRegex {
                regex: "^(prod|staging)$".into(),
                compiled: None,
            }),
            invert_match: false,
        };
        ok.compile_safe_regexes().expect("valid regex compiles");
        assert!(matches!(&ok.mode, HeaderMatcherMode::SafeRegexMatch(sr) if sr.compiled.is_some()));
        let mut bad = HeaderMatcher {
            name: "x".into(),
            mode: HeaderMatcherMode::SafeRegexMatch(SafeRegex {
                regex: "(".into(),
                compiled: None,
            }),
            invert_match: false,
        };
        assert!(matches!(
            bad.compile_safe_regexes(),
            Err(crate::ConfigError::InvalidRegex { .. })
        ));
    }

    #[test]
    fn value_matcher_compile_safe_regexes_compiles_string_and_noops_present() {
        let mut v = ValueMatcher::StringMatch(StringMatcher {
            mode: StringMatcherMode::SafeRegex(SafeRegex {
                regex: "^(prod|staging)$".into(),
                compiled: None,
            }),
            ignore_case: false,
        });
        v.compile_safe_regexes().expect("compiles");
        if let ValueMatcher::StringMatch(sm) = &v {
            assert!(matches!(&sm.mode, StringMatcherMode::SafeRegex(sr) if sr.compiled.is_some()));
        }
        // present_match has nothing to compile → Ok.
        ValueMatcher::PresentMatch(true)
            .compile_safe_regexes()
            .expect("noop ok");
    }

    #[test]
    fn parses_route_match_with_headers_vec_and_invert_match_default() {
        let yaml = r#"
prefix: "/api/"
headers:
  - name: "x-foo"
    exact_match: "bar"
  - name: "x-version"
    range_match: { start: 1, end: 100 }
"#;
        let rm: RouteMatch = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(rm.prefix.as_deref(), Some("/api/"));
        assert_eq!(rm.headers.len(), 2);
        assert_eq!(rm.headers[0].name, "x-foo");
        assert!(!rm.headers[0].invert_match);
        assert_eq!(
            rm.headers[0].mode,
            HeaderMatcherMode::ExactMatch("bar".into())
        );
        assert_eq!(rm.headers[1].name, "x-version");
        assert_eq!(
            rm.headers[1].mode,
            HeaderMatcherMode::RangeMatch(Int64Range { start: 1, end: 100 })
        );
    }

    // --- 04.3 Task 1: RouteAction parse-shape tests ---

    /// Build the full bootstrap YAML scaffolding around the given routes block
    /// and clusters block. The 5 RouteAction parse-shape tests vary only in
    /// these two slots.
    fn route_action_yaml(routes: &str, clusters: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  listeners:
    - name: hcm_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        {routes}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:{clusters}
"#
        )
    }

    const BACKEND_CLUSTER: &str = r#"
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint: { address: { socket_address: { address: 127.0.0.1, port_value: 9001 } } }"#;

    const NO_CLUSTERS: &str = " []";

    /// Helper to drill into the parsed bootstrap and return the first route's
    /// action. Panics on any structural mismatch (these tests are about
    /// RouteAction shape; non-shape failures should fail loudly).
    fn first_route_action(b: &Bootstrap) -> &RouteAction {
        let listener = &b.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let typed = filter.typed_config.as_ref().expect("typed_config present");
        let hcm = match typed {
            TypedConfig::HttpConnectionManager(hcm) => hcm,
            other => panic!("expected HCM typed_config, got {other:?}"),
        };
        &hcm.route_config.as_ref().unwrap().virtual_hosts[0].routes[0].action
    }

    #[test]
    fn parses_route_with_direct_response_action() {
        let routes = r#"- match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }"#;
        let yaml = route_action_yaml(routes, NO_CLUSTERS);
        let b = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let action = first_route_action(&b);
        match action {
            RouteAction::DirectResponse(dr) => {
                assert_eq!(dr.status, 200);
                assert_eq!(dr.body.inline_string.as_deref(), Some("ok\n"));
            }
            other => panic!("expected DirectResponse, got {other:?}"),
        }
    }

    #[test]
    fn parses_route_with_route_action() {
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: backend }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let b = crate::parse_bootstrap(&yaml).expect("parses + validates");
        let action = first_route_action(&b);
        match action {
            RouteAction::Route(ar) => {
                assert_eq!(ar.cluster, "backend");
            }
            other => panic!("expected Route(_), got {other:?}"),
        }
    }

    #[test]
    fn rejects_route_with_both_direct_response_and_route() {
        let routes = r#"- match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                          route: { cluster: backend }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let err = crate::parse_bootstrap(&yaml).expect_err("rejects both peers present");
        let msg = err.to_string();
        assert!(
            msg.contains("direct_response"),
            "msg should mention direct_response; got: {msg}"
        );
        assert!(
            msg.contains("route"),
            "msg should mention route; got: {msg}"
        );
        assert!(
            msg.contains("exactly one"),
            "msg should mention `exactly one`; got: {msg}"
        );
    }

    #[test]
    fn rejects_route_with_neither_direct_response_nor_route() {
        let routes = r#"- match: { prefix: "/" }"#;
        let yaml = route_action_yaml(routes, NO_CLUSTERS);
        let err = crate::parse_bootstrap(&yaml).expect_err("rejects neither peer present");
        let msg = err.to_string();
        assert!(
            msg.contains("direct_response"),
            "msg should mention direct_response; got: {msg}"
        );
        assert!(
            msg.contains("route"),
            "msg should mention route; got: {msg}"
        );
        assert!(
            msg.contains("exactly one"),
            "msg should mention `exactly one`; got: {msg}"
        );
    }

    #[test]
    fn rejects_route_with_unknown_top_level_key() {
        let routes = r#"- match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                          unknown_route_field: surprise"#;
        let yaml = route_action_yaml(routes, NO_CLUSTERS);
        let err = crate::parse_bootstrap(&yaml).expect_err("rejects unknown top-level Route key");
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("unknown"),
            "msg should mention `unknown` (case-insensitive); got: {msg}"
        );
        assert!(
            msg.contains("unknown_route_field"),
            "msg should mention the offending key `unknown_route_field`; got: {msg}"
        );
    }

    // --- 04.3 Task 2: validator UnknownCluster reuse for RouteAction::Route ---

    #[test]
    fn parses_route_with_cluster_action() {
        // Happy path: route references the declared `backend` cluster.
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: backend }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        crate::parse_bootstrap(&yaml).expect("parses + validates");
    }

    #[test]
    fn rejects_hcm_route_with_unknown_cluster() {
        // The route references `nonexistent`; only `backend` is declared.
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: nonexistent }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject unknown cluster");
        assert!(
            matches!(&err, crate::ConfigError::UnknownCluster(name) if name == "nonexistent"),
            "expected UnknownCluster(\"nonexistent\"); got: {err:?}"
        );
    }

    #[test]
    fn rejects_hcm_route_with_empty_cluster_name() {
        // Empty cluster names are treated as just another unknown reference;
        // no cluster declares an empty name.
        let routes = r#"- match: { prefix: "/" }
                          route: { cluster: "" }"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject empty cluster name");
        assert!(
            matches!(&err, crate::ConfigError::UnknownCluster(name) if name.is_empty()),
            "expected UnknownCluster(\"\"); got: {err:?}"
        );
    }

    // --- 18 D1 (ADR-0049): dynamic_resources schema + deferred cluster-ref validation ---

    /// Fixture-0026 topology: `node` + one HCM listener whose route_config carries
    /// `validate_clusters: false` and a single route to cluster `dynamic_backend`,
    /// with NO `clusters:` key (the cluster will be supplied by the CDS file). The
    /// listener satisfies the NoRuntime check, so no `admin` block is needed.
    const HCM_LISTENER_TO_DYNAMIC_BACKEND: &str = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: hcm_listener
      address: { socket_address: { address: 127.0.0.1, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  validate_clusters: false
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: dynamic_backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router"#;

    #[test]
    fn bootstrap_parses_dynamic_resources_cds_path_config_source() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    resource_api_version: V3
    path_config_source:
      path: /tmp/cds.yaml
"#;
        let b = crate::parse_bootstrap(yaml).unwrap();
        let dr = b.dynamic_resources.as_ref().unwrap();
        let cs = dr.cds_config.as_ref().unwrap();
        assert_eq!(cs.path_config_source.path, "/tmp/cds.yaml");
        assert_eq!(cs.resource_api_version.as_deref(), Some("V3"));
    }

    // --- 20 D1 (ADR-0051/0052): rds schema + exactly-one-of route source ---

    /// Build a one-HCM-listener bootstrap whose HCM body carries the given
    /// `route_body` lines (i.e. some combination of `route_config:`/`rds:`),
    /// spliced verbatim into the typed_config. The listener satisfies NoRuntime,
    /// so no admin block is needed.
    fn rds_schema_yaml(route_body: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  listeners:
    - name: hcm_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
{route_body}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
        )
    }

    /// Pull the first HCM from the first static listener (test convenience).
    fn first_hcm(b: &Bootstrap) -> &HttpConnectionManagerConfig {
        for filter in &b.static_resources.listeners[0].filter_chains[0].filters {
            if let Some(TypedConfig::HttpConnectionManager(hcm)) = &filter.typed_config {
                return hcm;
            }
        }
        panic!("no HCM on first static listener");
    }

    // (a) the `Rds` struct parses; route_config is None.
    #[test]
    fn rds_schema_parses_rds_block() {
        let yaml = rds_schema_yaml(
            "                rds:\n                  route_config_name: local_route\n                  config_source:\n                    path_config_source:\n                      path: /x",
        );
        let b = crate::parse_bootstrap(&yaml).unwrap();
        let hcm = first_hcm(&b);
        assert!(hcm.route_config.is_none());
        assert_eq!(
            hcm.rds,
            Some(crate::Rds {
                route_config_name: "local_route".to_string(),
                config_source: crate::ConfigSource {
                    path_config_source: crate::PathConfigSource {
                        path: "/x".to_string()
                    },
                    resource_api_version: None,
                },
            })
        );
    }

    // (b) resource_api_version optional inside rds.config_source.
    #[test]
    fn rds_schema_resource_api_version_optional() {
        let yaml = rds_schema_yaml(
            "                rds:\n                  route_config_name: local_route\n                  config_source:\n                    path_config_source:\n                      path: /x\n                    resource_api_version: V3",
        );
        let b = crate::parse_bootstrap(&yaml).unwrap();
        let hcm = first_hcm(&b);
        let rds = hcm.rds.as_ref().unwrap();
        assert_eq!(
            rds.config_source.resource_api_version.as_deref(),
            Some("V3")
        );
    }

    // (c) inline route_config still parses to Some; rds is None (regression).
    #[test]
    fn rds_schema_inline_route_config_still_parses() {
        let yaml = rds_schema_yaml(
            "                route_config:\n                  name: rc\n                  virtual_hosts:\n                    - name: vh\n                      domains: [\"*\"]\n                      routes:\n                        - match: { prefix: \"/\" }\n                          direct_response: { status: 200, body: { inline_string: ok } }",
        );
        let b = crate::parse_bootstrap(&yaml).unwrap();
        let hcm = first_hcm(&b);
        assert!(hcm.route_config.is_some());
        assert!(hcm.rds.is_none());
    }

    // (d) neither → MissingRouteSource.
    #[test]
    fn rds_schema_missing_route_source_rejected() {
        let yaml = rds_schema_yaml("");
        let err = crate::parse_bootstrap(&yaml).expect_err("neither source must reject");
        assert!(
            matches!(&err, crate::ConfigError::MissingRouteSource { stat_prefix } if stat_prefix == "ingress_http"),
            "expected MissingRouteSource; got: {err:?}"
        );
    }

    // (e) both → AmbiguousRouteSource.
    #[test]
    fn rds_schema_ambiguous_route_source_rejected() {
        let yaml = rds_schema_yaml(
            "                route_config:\n                  name: rc\n                  virtual_hosts:\n                    - name: vh\n                      domains: [\"*\"]\n                      routes:\n                        - match: { prefix: \"/\" }\n                          direct_response: { status: 200, body: { inline_string: ok } }\n                rds:\n                  route_config_name: local_route\n                  config_source:\n                    path_config_source:\n                      path: /x",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("both sources must reject");
        assert!(
            matches!(&err, crate::ConfigError::AmbiguousRouteSource { stat_prefix } if stat_prefix == "ingress_http"),
            "expected AmbiguousRouteSource; got: {err:?}"
        );
    }

    // (f) unknown field inside rds rejected (deny_unknown_fields on Rds).
    #[test]
    fn rds_schema_unknown_field_in_rds_rejected() {
        let yaml = rds_schema_yaml(
            "                rds:\n                  route_config_name: x\n                  config_source:\n                    path_config_source:\n                      path: /x\n                  ads: {}",
        );
        assert!(
            crate::parse_bootstrap(&yaml).is_err(),
            "unknown field `ads` inside rds must reject"
        );
    }

    // (g) deferred ConfigSource surfaces still rejected inside rds.config_source.
    #[test]
    fn rds_schema_deferred_config_source_fields_rejected() {
        for deferred in [
            "api_config_source: { api_type: GRPC }",
            "ads: {}",
            "watched_directory: { path: /w }",
        ] {
            let yaml = rds_schema_yaml(&format!(
                "                rds:\n                  route_config_name: x\n                  config_source:\n                    path_config_source:\n                      path: /x\n                    {deferred}"
            ));
            assert!(
                crate::parse_bootstrap(&yaml).is_err(),
                "deferred config_source field `{deferred}` must reject"
            );
        }
    }

    #[test]
    fn dynamic_resources_rejects_deferred_fields() {
        // ads_config / api_config_source / watched_directory all rejected loudly
        // by deny_unknown_fields (SPEC §4 deferral ledger). NOTE: 19 D1 (ADR-0050)
        // promoted lds_config to a real field — it is no longer in this list (its
        // parse + the surviving deferred-field rejections live in the 19-D1 tests).
        let field = "ads_config: { api_type: GRPC }";
        let yaml = format!(
            "node: {{ id: t, cluster: t }}\nadmin: {{ address: {{ socket_address: {{ address: 0.0.0.0, port_value: 0 }} }} }}\ndynamic_resources:\n  {field}"
        );
        assert!(
            crate::parse_bootstrap(&yaml).is_err(),
            "{field} should reject"
        );
        // api_config_source on the ConfigSource:
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    api_config_source: { api_type: GRPC }
"#;
        assert!(crate::parse_bootstrap(yaml).is_err());
        // watched_directory on PathConfigSource:
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    path_config_source:
      path: /tmp/cds.yaml
      watched_directory: { path: /tmp }
"#;
        assert!(crate::parse_bootstrap(yaml).is_err());
    }

    #[test]
    fn resource_api_version_v3_or_absent_accepted_others_rejected() {
        // L8: absent + V3 accepted; V2 / garbage rejected (UnsupportedResourceApiVersion).
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    resource_api_version: V2
    path_config_source: { path: /tmp/cds.yaml }
"#;
        let err = crate::parse_bootstrap(yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnsupportedResourceApiVersion(ref v) if v == "V2")
        );
        // Absent resource_api_version is accepted.
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    path_config_source: { path: /tmp/cds.yaml }
"#;
        assert!(crate::parse_bootstrap(yaml).is_ok());
    }

    #[test]
    fn route_config_parses_validate_clusters_field() {
        // L12b: parse-and-accept (Envoy requires `validate_clusters: false` on a
        // route_config referencing CDS clusters; configs are identical on both sides).
        let yaml = r#"
name: local_route
validate_clusters: false
virtual_hosts: []
"#;
        let rc: RouteConfiguration = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rc.validate_clusters, Some(false));
    }

    #[test]
    fn route_to_unknown_cluster_deferred_when_dynamic_resources_configured_unloaded() {
        // The fixture-0026 topology: zero static clusters + a route to a cluster the
        // CDS file will supply. parse_bootstrap (which cannot do I/O) must ACCEPT this
        // — the reference check defers until load_dynamic_resources re-validates.
        let yaml = format!(
            "{HCM_LISTENER_TO_DYNAMIC_BACKEND}\ndynamic_resources:\n  cds_config:\n    path_config_source: {{ path: /tmp/cds.yaml }}\n"
        );
        assert!(crate::parse_bootstrap(&yaml).is_ok());
    }

    #[test]
    fn route_to_unknown_cluster_still_rejected_without_dynamic_resources() {
        // Regression: the deferral ONLY applies when dynamic_resources.cds_config is
        // configured. The existing UnknownCluster behavior is unchanged otherwise.
        let yaml = HCM_LISTENER_TO_DYNAMIC_BACKEND; // no dynamic_resources block
        let err = crate::parse_bootstrap(yaml).unwrap_err();
        assert!(matches!(err, crate::ConfigError::UnknownCluster(ref c) if c == "dynamic_backend"));
    }

    #[test]
    fn tcp_proxy_to_unknown_cluster_deferred_when_dynamic_resources_configured_unloaded() {
        // The deferral covers BOTH reference-check sites: the tcp_proxy cluster ref
        // also defers while CDS is configured-but-unloaded.
        let yaml = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
                cluster: dynamic_backend
dynamic_resources:
  cds_config:
    path_config_source: { path: /tmp/cds.yaml }
"#;
        assert!(crate::parse_bootstrap(yaml).is_ok());
        // Regression: without dynamic_resources, the tcp_proxy ref still rejects.
        let yaml_no_dr = r#"
node: { id: t, cluster: c }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 10000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress
                cluster: dynamic_backend
"#;
        let err = crate::parse_bootstrap(yaml_no_dr).unwrap_err();
        assert!(matches!(err, crate::ConfigError::UnknownCluster(ref c) if c == "dynamic_backend"));
    }

    // --- 18 D3 (ADR-0049): load_dynamic_resources — read + parse + merge + re-validate ---

    /// The Task-2 minimal CDS file shape: one `dynamic_backend` cluster with a
    /// 127.0.0.1:8124 endpoint.
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

    /// Build a complete bootstrap YAML: the fixture-0026 HCM listener with a
    /// route to the named cluster, NO static clusters, and a dynamic_resources
    /// block pointing at `cds_path`.
    fn bootstrap_yaml_with_cds_route_to(cds_path: &str, cluster: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  listeners:
    - name: hcm_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  validate_clusters: false
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: {cluster} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
dynamic_resources:
  cds_config:
    path_config_source: {{ path: {cds_path} }}
"#
        )
    }

    /// Route to `dynamic_backend` (the CDS-supplied cluster) — the happy-path
    /// helper. The route genuinely references the CDS-loaded cluster so the
    /// post-merge re-validation exercises deferred-reference resolution.
    fn bootstrap_yaml_with_cds_route(cds_path: &str) -> String {
        bootstrap_yaml_with_cds_route_to(cds_path, "dynamic_backend")
    }

    /// Build a bootstrap that statically defines `cluster_name` (with the given
    /// port), a route to it, AND a dynamic_resources block at `cds_path`.
    fn bootstrap_yaml_with_static_and_cds(cluster_name: &str, port: u16, cds_path: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  clusters:
    - name: {cluster_name}
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: {cluster_name}
        endpoints:
        - lb_endpoints:
          - endpoint:
              address:
                socket_address: {{ address: 127.0.0.1, port_value: {port} }}
  listeners:
    - name: hcm_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  validate_clusters: false
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: {cluster_name} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
dynamic_resources:
  cds_config:
    path_config_source: {{ path: {cds_path} }}
"#
        )
    }

    /// Extract the single endpoint port_value of a cluster (test convenience).
    fn single_endpoint_port(cluster: &Cluster) -> u16 {
        cluster.load_assignment.as_ref().unwrap().endpoints[0].lb_endpoints[0]
            .endpoint
            .address
            .socket_address
            .port_value
    }

    #[test]
    fn load_dynamic_resources_happy_path() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        std::fs::write(&cds_path, MINIMAL_CDS).unwrap();
        let yaml = bootstrap_yaml_with_cds_route(cds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        assert!(b.dynamic_clusters.is_none());
        crate::load_dynamic_resources(&mut b).unwrap();
        assert_eq!(b.dynamic_clusters.as_ref().unwrap().len(), 1);
        assert_eq!(b.all_clusters().count(), 1);
    }

    #[test]
    fn dynamic_cluster_resolves_deferred_route_reference() {
        // Closes the Task-1 review finding: the route to `dynamic_backend` is
        // deferred at parse_bootstrap (no static cluster), and RESOLVES against
        // the CDS-loaded cluster at load_dynamic_resources (no UnknownCluster).
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        std::fs::write(&cds_path, MINIMAL_CDS).unwrap();
        let yaml = bootstrap_yaml_with_cds_route(cds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        // The route references dynamic_backend, supplied only by the CDS file.
        crate::load_dynamic_resources(&mut b).expect("deferred ref must resolve post-load");
        assert!(b.all_clusters().any(|c| c.name == "dynamic_backend"));
    }

    #[test]
    fn load_is_noop_without_dynamic_resources() {
        let yaml = r#"
node: { id: t, cluster: c }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
"#;
        let mut b = crate::parse_bootstrap(yaml).unwrap();
        crate::load_dynamic_resources(&mut b).unwrap();
        assert!(b.dynamic_clusters.is_none());
    }

    #[test]
    fn missing_cds_file_is_fatal() {
        let yaml = bootstrap_yaml_with_cds_route("/nonexistent/cds.yaml");
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(matches!(err, crate::ConfigError::CdsFileError { .. }));
    }

    #[test]
    fn malformed_cds_file_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        std::fs::write(&cds_path, "resources: [unclosed").unwrap();
        let yaml = bootstrap_yaml_with_cds_route(cds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        assert!(crate::load_dynamic_resources(&mut b).is_err());
    }

    #[test]
    fn static_dynamic_collision_static_wins() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        std::fs::write(&cds_path, MINIMAL_CDS).unwrap(); // dynamic_backend → 8124
        let yaml =
            bootstrap_yaml_with_static_and_cds("dynamic_backend", 8123, cds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        crate::load_dynamic_resources(&mut b).unwrap();
        // The dynamic duplicate is SKIPPED (no error); static wins.
        assert_eq!(b.dynamic_clusters.as_ref().unwrap().len(), 0);
        assert_eq!(b.all_clusters().count(), 1);
        assert_eq!(single_endpoint_port(b.all_clusters().next().unwrap()), 8123);
    }

    #[test]
    fn unresolved_route_reference_fatal_after_load() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        std::fs::write(&cds_path, MINIMAL_CDS).unwrap(); // defines dynamic_backend only
        let yaml = bootstrap_yaml_with_cds_route_to(cds_path.to_str().unwrap(), "no_such_cluster");
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(matches!(err, crate::ConfigError::UnknownCluster(ref c) if c == "no_such_cluster"));
    }

    // ── 19 D3: LDS load branch + §5.7 merge ordering ──────────────────────────

    /// An LDS file body (the `resources:` envelope with one @type-tagged
    /// Listener named `lds_listener`, an HCM routing `/` to `route_cluster`).
    /// `valid_http_filters = true` emits a valid router-terminated chain;
    /// `false` emits an EMPTY `http_filters: []` (parses, but `validate_hcm`
    /// rejects it — proving the per-listener validation loop covers dynamic
    /// listeners). `http_filters` is a required field on the HCM struct, so it
    /// must be PRESENT (an empty list) rather than omitted, which would fail at
    /// parse time, not validation.
    fn lds_file(route_cluster: &str, valid_http_filters: bool) -> String {
        let http_filters = if valid_http_filters {
            r#"        http_filters:
        - name: envoy.filters.http.router
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
        } else {
            "        http_filters: []\n"
        };
        format!(
            r#"
resources:
- "@type": type.googleapis.com/envoy.config.listener.v3.Listener
  name: lds_listener
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 10000 }}
  filter_chains:
  - filters:
    - name: envoy.filters.network.http_connection_manager
      typed_config:
        "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
        stat_prefix: ingress_http
        codec_type: HTTP1
        route_config:
          name: rc
          validate_clusters: false
          virtual_hosts:
          - name: vh
            domains: ["*"]
            routes:
            - match: {{ prefix: "/" }}
              route: {{ cluster: {route_cluster} }}
{http_filters}"#
        )
    }

    /// An LDS file defining a listener with the GIVEN name (collision tests).
    fn lds_file_named(listener_name: &str) -> String {
        format!(
            r#"
resources:
- "@type": type.googleapis.com/envoy.config.listener.v3.Listener
  name: {listener_name}
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 10001 }}
  filter_chains:
  - filters:
    - name: envoy.filters.network.tcp_proxy
      typed_config:
        "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
        stat_prefix: t
        cluster: static_c
"#
        )
    }

    /// A bootstrap with a static `static_c` cluster, ZERO static listeners, an
    /// admin block (satisfies NoRuntime), and an `lds_config` at `lds_path`.
    /// (CDS is left unconfigured.)
    fn bootstrap_yaml_with_lds(lds_path: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}
static_resources:
  clusters:
    - name: static_c
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: static_c
        endpoints:
        - lb_endpoints:
          - endpoint:
              address:
                socket_address: {{ address: 127.0.0.1, port_value: 8200 }}
  listeners: []
dynamic_resources:
  lds_config:
    path_config_source: {{ path: {lds_path} }}
"#
        )
    }

    /// A bootstrap with NO admin and ZERO static listeners (so NoRuntime is in
    /// play), a static `static_c` cluster, and an `lds_config` at `lds_path`.
    fn bootstrap_yaml_with_lds_no_admin(lds_path: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  clusters:
    - name: static_c
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: static_c
        endpoints:
        - lb_endpoints:
          - endpoint:
              address:
                socket_address: {{ address: 127.0.0.1, port_value: 8200 }}
  listeners: []
dynamic_resources:
  lds_config:
    path_config_source: {{ path: {lds_path} }}
"#
        )
    }

    /// A bootstrap with BOTH cds_config and lds_config: a static `static_c`
    /// cluster, zero static listeners, admin, CDS at `cds_path`, LDS at
    /// `lds_path`. (§5.7 composition — a dynamic listener may route to a
    /// dynamic cluster.)
    fn bootstrap_yaml_with_cds_and_lds(cds_path: &str, lds_path: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}
static_resources:
  clusters: []
  listeners: []
dynamic_resources:
  cds_config:
    path_config_source: {{ path: {cds_path} }}
  lds_config:
    path_config_source: {{ path: {lds_path} }}
"#
        )
    }

    /// A bootstrap with ONE static listener named `x` (a trivial tcp_proxy
    /// chain), a static `static_c` cluster, admin, and an `lds_config` at
    /// `lds_path` (collision test — static `x` vs LDS `x`).
    fn bootstrap_yaml_static_listener_and_lds(static_listener: &str, lds_path: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}
static_resources:
  clusters:
    - name: static_c
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: static_c
        endpoints:
        - lb_endpoints:
          - endpoint:
              address:
                socket_address: {{ address: 127.0.0.1, port_value: 8200 }}
  listeners:
    - name: {static_listener}
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: t
                cluster: static_c
dynamic_resources:
  lds_config:
    path_config_source: {{ path: {lds_path} }}
"#
        )
    }

    // (a) the LDS branch loads.
    #[test]
    fn lds_branch_loads_listener() {
        let dir = tempfile::tempdir().unwrap();
        let lds_path = dir.path().join("lds.yaml");
        std::fs::write(&lds_path, lds_file("static_c", true)).unwrap();
        let yaml = bootstrap_yaml_with_lds(lds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        assert!(b.dynamic_listeners.is_none());
        crate::load_dynamic_resources(&mut b).unwrap();
        assert_eq!(b.dynamic_listeners.as_ref().unwrap().len(), 1);
        assert_eq!(b.all_listeners().count(), 1);
        assert_eq!(b.all_listeners().next().unwrap().name, "lds_listener");
    }

    // (b) missing LDS file is fatal.
    #[test]
    fn missing_lds_file_is_fatal() {
        let yaml = bootstrap_yaml_with_lds("/nonexistent/lds.yaml");
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(matches!(err, crate::ConfigError::LdsFileError { .. }));
    }

    // (c) malformed LDS file is fatal.
    #[test]
    fn malformed_lds_file_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let lds_path = dir.path().join("lds.yaml");
        std::fs::write(&lds_path, "resources: [unclosed").unwrap();
        let yaml = bootstrap_yaml_with_lds(lds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(matches!(err, crate::ConfigError::LdsParseError { .. }));
    }

    // (d) the §5.7 composition resolves: a dynamic listener routes to a dynamic
    // cluster (CDS merged BEFORE the single post-merge re-validation).
    #[test]
    fn dynamic_listener_route_to_dynamic_cluster_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        // MINIMAL_CDS defines cluster `dynamic_backend`.
        std::fs::write(&cds_path, MINIMAL_CDS).unwrap();
        let lds_path = dir.path().join("lds.yaml");
        // The LDS listener routes to `dynamic_backend` — supplied ONLY by CDS.
        std::fs::write(&lds_path, lds_file("dynamic_backend", true)).unwrap();
        let yaml =
            bootstrap_yaml_with_cds_and_lds(cds_path.to_str().unwrap(), lds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        crate::load_dynamic_resources(&mut b)
            .expect("dynamic-listener route to a dynamic cluster must resolve (§5.7)");
        assert_eq!(b.all_listeners().count(), 1);
        assert!(b.all_clusters().any(|c| c.name == "dynamic_backend"));
    }

    // (e) unresolved dynamic-listener route is fatal (L6): a route to a cluster
    // in NEITHER list → UnknownCluster, NOT a panic.
    #[test]
    fn dynamic_listener_unresolved_route_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let lds_path = dir.path().join("lds.yaml");
        std::fs::write(&lds_path, lds_file("nope", true)).unwrap();
        let yaml = bootstrap_yaml_with_lds(lds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnknownCluster(ref c) if c == "nope"),
            "got {err:?}"
        );
    }

    // (f) listener name collision — static wins (L7).
    #[test]
    fn lds_listener_name_collision_static_wins() {
        let dir = tempfile::tempdir().unwrap();
        let lds_path = dir.path().join("lds.yaml");
        std::fs::write(&lds_path, lds_file_named("x")).unwrap();
        let yaml = bootstrap_yaml_static_listener_and_lds("x", lds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        crate::load_dynamic_resources(&mut b).unwrap();
        // The dynamic duplicate is SKIPPED (no error); static wins.
        assert_eq!(b.dynamic_listeners.as_ref().unwrap().len(), 0);
        assert_eq!(b.all_listeners().count(), 1);
    }

    // (g) dynamic listeners go through per-listener validation: an LDS listener
    // with an invalid HCM (empty http_filters) → the existing validate_hcm
    // error (EmptyHttpFilters), proving the per-listener loop covers dynamic
    // listeners. (The HCM otherwise parses cleanly — the ONLY rejection trigger
    // is the empty filter list, surfaced at the post-merge re-validation.)
    #[test]
    fn dynamic_listener_invalid_hcm_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let lds_path = dir.path().join("lds.yaml");
        // Empty http_filters → validate_hcm rejects (EmptyHttpFilters).
        std::fs::write(&lds_path, lds_file("static_c", false)).unwrap();
        let yaml = bootstrap_yaml_with_lds(lds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::EmptyHttpFilters { ref listener } if listener == "lds_listener"),
            "expected EmptyHttpFilters from the per-listener loop, got {err:?}"
        );
    }

    // (h) post-merge NoRuntime enforcement: no admin + 0 static listeners + an
    // EMPTY LDS resources list → NoRuntime (deferred at parse, enforced post-merge).
    #[test]
    fn empty_lds_no_admin_is_no_runtime_post_merge() {
        let dir = tempfile::tempdir().unwrap();
        let lds_path = dir.path().join("lds.yaml");
        std::fs::write(&lds_path, "resources: []").unwrap();
        let yaml = bootstrap_yaml_with_lds_no_admin(lds_path.to_str().unwrap());
        // parse_bootstrap succeeds: the NoRuntime gate DEFERS while
        // lds_configured_but_unloaded.
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
    }

    // ── 20 D3: RDS load pass (load_dynamic_resources populates route_config) ──

    /// An RDS file body: one @type-tagged RouteConfiguration named `rc_name`
    /// routing `prefix` → `cluster`. With `empty_vhosts = true` the
    /// RouteConfiguration carries an EMPTY `virtual_hosts` list (parses; the
    /// post-merge re-validation rejects it via EmptyVirtualHosts).
    fn rds_file(rc_name: &str, prefix: &str, cluster: &str, empty_vhosts: bool) -> String {
        if empty_vhosts {
            return format!(
                r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: {rc_name}
  virtual_hosts: []
"#
            );
        }
        format!(
            r#"
resources:
- "@type": type.googleapis.com/envoy.config.route.v3.RouteConfiguration
  name: {rc_name}
  virtual_hosts:
  - name: vh
    domains: ["*"]
    routes:
    - match: {{ prefix: "{prefix}" }}
      route: {{ cluster: {cluster} }}
"#
        )
    }

    /// A bootstrap with ONE static listener whose HCM uses `rds`
    /// (route_config_name + config_source.path = `rds_path`), a static
    /// `static_backend` cluster, and admin. CDS/LDS unconfigured.
    fn bootstrap_yaml_with_rds(route_config_name: &str, rds_path: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}
static_resources:
  clusters:
    - name: static_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: static_backend
        endpoints:
        - lb_endpoints:
          - endpoint:
              address:
                socket_address: {{ address: 127.0.0.1, port_value: 8300 }}
  listeners:
    - name: rds_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                rds:
                  route_config_name: {route_config_name}
                  config_source:
                    path_config_source: {{ path: {rds_path} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
        )
    }

    /// A bootstrap with NO static clusters, NO static listeners, admin, an
    /// `rds`-using static listener, AND a `cds_config` at `cds_path` (§5.7
    /// composition — an RDS route to a CDS-supplied cluster).
    fn bootstrap_yaml_with_rds_and_cds(
        route_config_name: &str,
        rds_path: &str,
        cds_path: &str,
    ) -> String {
        format!(
            r#"
node: {{ id: t, cluster: c }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 0 }} }} }}
static_resources:
  clusters: []
  listeners:
    - name: rds_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                rds:
                  route_config_name: {route_config_name}
                  config_source:
                    path_config_source: {{ path: {rds_path} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
dynamic_resources:
  cds_config:
    path_config_source: {{ path: {cds_path} }}
"#
        )
    }

    // (a) the RDS pass loads + populates route_config (name-selected).
    #[test]
    fn rds_pass_loads_and_populates_route_config() {
        let dir = tempfile::tempdir().unwrap();
        let rds_path = dir.path().join("rds.yaml");
        std::fs::write(
            &rds_path,
            rds_file("local_route", "/static", "static_backend", false),
        )
        .unwrap();
        let yaml = bootstrap_yaml_with_rds("local_route", rds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        // Before the load: rds is Some, route_config is None.
        assert!(first_hcm(&b).rds.is_some());
        assert!(first_hcm(&b).route_config.is_none());
        crate::load_dynamic_resources(&mut b).unwrap();
        let hcm = first_hcm(&b);
        // route_config is now populated (§5.3 uniform shape); rds remains Some.
        let rc = hcm.route_config.as_ref().expect("route_config populated");
        assert_eq!(rc.name, "local_route");
        assert!(hcm.rds.is_some());
    }

    // (b) a missing RDS file is fatal (L4) → RdsFileError.
    #[test]
    fn missing_rds_file_is_fatal() {
        let yaml = bootstrap_yaml_with_rds("local_route", "/nonexistent/rds.yaml");
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::RdsFileError { .. }),
            "got {err:?}"
        );
    }

    // (c) a malformed RDS file is fatal (L4) → RdsParseError.
    #[test]
    fn malformed_rds_file_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let rds_path = dir.path().join("rds.yaml");
        std::fs::write(&rds_path, "resources: [unclosed").unwrap();
        let yaml = bootstrap_yaml_with_rds("local_route", rds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::RdsParseError { .. }),
            "got {err:?}"
        );
    }

    // (d) route_config_name matching no resource in the file is fatal (L6)
    //     → RdsRouteConfigNotFound. The file defines `other_route`.
    #[test]
    fn rds_route_config_name_mismatch_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let rds_path = dir.path().join("rds.yaml");
        std::fs::write(
            &rds_path,
            rds_file("other_route", "/static", "static_backend", false),
        )
        .unwrap();
        let yaml = bootstrap_yaml_with_rds("local_route", rds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::RdsRouteConfigNotFound { ref name, .. } if name == "local_route"),
            "got {err:?}"
        );
    }

    // (e) §5.7 RDS+CDS composition: an RDS route to a CDS-supplied cluster
    //     resolves because CDS merges before the post-merge re-validation.
    #[test]
    fn rds_route_to_cds_cluster_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        // MINIMAL_CDS defines cluster `dynamic_backend`.
        std::fs::write(&cds_path, MINIMAL_CDS).unwrap();
        let rds_path = dir.path().join("rds.yaml");
        std::fs::write(
            &rds_path,
            rds_file("local_route", "/dynamic", "dynamic_backend", false),
        )
        .unwrap();
        let yaml = bootstrap_yaml_with_rds_and_cds(
            "local_route",
            rds_path.to_str().unwrap(),
            cds_path.to_str().unwrap(),
        );
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        crate::load_dynamic_resources(&mut b)
            .expect("RDS route to a CDS cluster must resolve (§5.7)");
        let rc = first_hcm(&b).route_config.as_ref().unwrap();
        assert_eq!(rc.name, "local_route");
        assert!(b.all_clusters().any(|c| c.name == "dynamic_backend"));
    }

    // (f) an RDS route to a cluster in NEITHER list → UnknownCluster (NOT a
    //     panic) via the post-merge re-validation.
    #[test]
    fn rds_unresolved_route_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let rds_path = dir.path().join("rds.yaml");
        std::fs::write(&rds_path, rds_file("local_route", "/x", "nope", false)).unwrap();
        let yaml = bootstrap_yaml_with_rds("local_route", rds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnknownCluster(ref c) if c == "nope"),
            "got {err:?}"
        );
    }

    // (g) the post-merge re-validation walks the rds-populated route_config: an
    //     RDS RouteConfiguration with empty virtual_hosts → EmptyVirtualHosts.
    #[test]
    fn rds_empty_virtual_hosts_is_fatal_post_merge() {
        let dir = tempfile::tempdir().unwrap();
        let rds_path = dir.path().join("rds.yaml");
        std::fs::write(
            &rds_path,
            rds_file("local_route", "/", "static_backend", true),
        )
        .unwrap();
        let yaml = bootstrap_yaml_with_rds("local_route", rds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::EmptyVirtualHosts { .. }),
            "got {err:?}"
        );
    }

    // (h) closes M20-T1-c: check_route_sources is re-run over the MERGED set, so
    //     an LDS-supplied HCM with NEITHER route source → MissingRouteSource.
    #[test]
    fn lds_hcm_with_no_route_source_is_missing_route_source() {
        let dir = tempfile::tempdir().unwrap();
        let lds_path = dir.path().join("lds.yaml");
        // An LDS listener whose HCM has neither route_config nor rds.
        let lds = r#"
resources:
- "@type": type.googleapis.com/envoy.config.listener.v3.Listener
  name: lds_listener
  address:
    socket_address: { address: 0.0.0.0, port_value: 10000 }
  filter_chains:
  - filters:
    - name: envoy.filters.network.http_connection_manager
      typed_config:
        "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
        stat_prefix: ingress_http
        codec_type: HTTP1
        http_filters:
        - name: envoy.filters.http.router
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#;
        std::fs::write(&lds_path, lds).unwrap();
        let yaml = bootstrap_yaml_with_lds(lds_path.to_str().unwrap());
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::MissingRouteSource { ref stat_prefix } if stat_prefix == "ingress_http"),
            "got {err:?}"
        );
    }

    // ── 21 D3: EDS load pass (load_dynamic_resources populates load_assignment) ──

    /// A minimal EDS file body: the bare `resources:` envelope with one
    /// @type-tagged ClusterLoadAssignment named `cla_name` carrying one numeric
    /// endpoint (`127.0.0.1:port`). `empty_endpoints = true` emits a CLA with
    /// zero endpoints (parses, but the post-merge validate rejects it).
    fn eds_file(cla_name: &str, port: u16, empty_endpoints: bool) -> String {
        let endpoints = if empty_endpoints {
            "  endpoints: []".to_string()
        } else {
            format!(
                r#"  endpoints:
  - lb_endpoints:
    - endpoint:
        address:
          socket_address: {{ address: 127.0.0.1, port_value: {port} }}"#
            )
        };
        format!(
            r#"
resources:
- "@type": type.googleapis.com/envoy.config.endpoint.v3.ClusterLoadAssignment
  cluster_name: {cla_name}
{endpoints}
"#
        )
    }

    /// Build a bootstrap with a STATIC HCM listener routing `/` → `eds_backend`,
    /// plus a `type: EDS` cluster `eds_backend` pointing at `eds_path` (with an
    /// optional `service_name`). NO `dynamic_resources` block — a purely-static
    /// EDS cluster (fixture-0029 shape, C16).
    fn bootstrap_yaml_with_eds(eds_path: &str, service_name: Option<&str>) -> String {
        let svc = match service_name {
            Some(s) => format!("\n        service_name: {s}"),
            None => String::new(),
        };
        format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          path_config_source: {{ path: {eds_path} }}{svc}
  listeners:
    - name: hcm_listener
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: rc
                  validate_clusters: false
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          route: {{ cluster: eds_backend }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
"#
        )
    }

    /// (a) the EDS pass loads + populates: a static EDS cluster whose file CLA
    /// (named `eds_backend`) has one endpoint → succeeds; load_assignment is
    /// populated (Some) with the file's endpoint; eds_cluster_config stays Some.
    #[test]
    fn load_dynamic_eds_pass_loads_and_populates() {
        let dir = tempfile::tempdir().unwrap();
        let eds_path = dir.path().join("eds.yaml");
        std::fs::write(&eds_path, eds_file("eds_backend", 9001, false)).unwrap();
        let yaml = bootstrap_yaml_with_eds(eds_path.to_str().unwrap(), None);
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        // Pre-merge: no inline load_assignment on the EDS cluster.
        assert!(b.static_resources.clusters[0].load_assignment.is_none());
        crate::load_dynamic_resources(&mut b).unwrap();
        let c = &b.static_resources.clusters[0];
        let la = c.load_assignment.as_ref().expect("populated post-merge");
        assert_eq!(la.cluster_name, "eds_backend");
        assert_eq!(la.endpoints.len(), 1);
        assert_eq!(single_endpoint_port(c), 9001);
        // eds_cluster_config is retained (not consumed).
        assert!(c.eds_cluster_config.is_some());
    }

    /// (b) service_name selection (L8): `service_name: my_svc` selects the
    /// `my_svc` CLA from the file (NOT the cluster-name `eds_backend`).
    #[test]
    fn load_dynamic_eds_service_name_selection() {
        let dir = tempfile::tempdir().unwrap();
        let eds_path = dir.path().join("eds.yaml");
        // The file defines my_svc (port 9002), not eds_backend.
        std::fs::write(&eds_path, eds_file("my_svc", 9002, false)).unwrap();
        let yaml = bootstrap_yaml_with_eds(eds_path.to_str().unwrap(), Some("my_svc"));
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        crate::load_dynamic_resources(&mut b).unwrap();
        let c = &b.static_resources.clusters[0];
        let la = c.load_assignment.as_ref().expect("populated post-merge");
        assert_eq!(la.cluster_name, "my_svc");
        assert_eq!(single_endpoint_port(c), 9002);
    }

    /// (c) missing EDS file is fatal (L4): `EdsFileError`.
    #[test]
    fn load_dynamic_eds_missing_file_is_fatal() {
        let yaml = bootstrap_yaml_with_eds("/nonexistent/eds.yaml", None);
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::EdsFileError { ref path, .. } if path == "/nonexistent/eds.yaml"),
            "got {err:?}"
        );
    }

    /// (d) malformed EDS file is fatal (L4): `EdsParseError`.
    #[test]
    fn load_dynamic_eds_malformed_file_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let eds_path = dir.path().join("eds.yaml");
        std::fs::write(&eds_path, "resources: [unclosed").unwrap();
        let yaml = bootstrap_yaml_with_eds(eds_path.to_str().unwrap(), None);
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::EdsParseError { .. }),
            "got {err:?}"
        );
    }

    /// (e) missing/mismatched CLA is fatal (L4/L8): the file defines `other`,
    /// the cluster wants `eds_backend` → `EdsClusterLoadAssignmentNotFound`.
    #[test]
    fn load_dynamic_eds_cla_mismatch_is_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let eds_path = dir.path().join("eds.yaml");
        std::fs::write(&eds_path, eds_file("other", 9003, false)).unwrap();
        let yaml = bootstrap_yaml_with_eds(eds_path.to_str().unwrap(), None);
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::EdsClusterLoadAssignmentNotFound { ref name, .. } if name == "eds_backend"),
            "got {err:?}"
        );
    }

    /// (f) empty CLA endpoints is fatal (L4 (d)): the matched CLA has zero
    /// endpoints → the post-merge validate rejects with `EmptyClusterEndpoints`.
    #[test]
    fn load_dynamic_eds_empty_endpoints_is_fatal_post_merge() {
        let dir = tempfile::tempdir().unwrap();
        let eds_path = dir.path().join("eds.yaml");
        std::fs::write(&eds_path, eds_file("eds_backend", 0, true)).unwrap();
        let yaml = bootstrap_yaml_with_eds(eds_path.to_str().unwrap(), None);
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::EmptyClusterEndpoints(ref c) if c == "eds_backend"),
            "got {err:?}"
        );
    }

    /// (g) the §5.7 ordering — a CDS-supplied cluster is composition-ready: a
    /// bootstrap with BOTH a `cds_config` (a STRICT_DNS `dynamic_backend`) and a
    /// static EDS cluster → the EDS pass walks the full static+CDS set;
    /// `dynamic_backend` is untouched, `eds_backend` is populated.
    #[test]
    fn load_dynamic_eds_walks_static_and_cds_clusters() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        std::fs::write(&cds_path, MINIMAL_CDS).unwrap();
        let eds_path = dir.path().join("eds.yaml");
        std::fs::write(&eds_path, eds_file("eds_backend", 9004, false)).unwrap();
        let yaml = format!(
            r#"
node: {{ id: t, cluster: c }}
static_resources:
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          path_config_source: {{ path: {eds} }}
  listeners: []
admin:
  address: {{ socket_address: {{ address: 127.0.0.1, port_value: 9901 }} }}
dynamic_resources:
  cds_config:
    path_config_source: {{ path: {cds} }}
"#,
            eds = eds_path.to_str().unwrap(),
            cds = cds_path.to_str().unwrap(),
        );
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        crate::load_dynamic_resources(&mut b).unwrap();
        // The CDS-supplied dynamic_backend (STRICT_DNS, inline LA) is untouched.
        let dyn_c = b
            .all_clusters()
            .find(|c| c.name == "dynamic_backend")
            .expect("CDS cluster present");
        assert_eq!(single_endpoint_port(dyn_c), 8124);
        // The static EDS cluster is populated from its file.
        let eds_c = b
            .all_clusters()
            .find(|c| c.name == "eds_backend")
            .expect("EDS cluster present");
        assert_eq!(single_endpoint_port(eds_c), 9004);
    }

    /// (h) the validate-gate fires for a static-only EDS bootstrap (C16): NO
    /// `dynamic_resources`/`rds`, but a static EDS cluster whose file CLA has a
    /// bad shape (empty endpoints) → the post-merge validate STILL runs and
    /// rejects (proving `had_eds_cluster` extends the gate). This is distinct
    /// from (f) only in that it asserts the GATE — without the extension, the
    /// post-merge validate would be skipped and the load would WRONGLY succeed.
    #[test]
    fn load_dynamic_eds_validate_gate_fires_static_only() {
        let dir = tempfile::tempdir().unwrap();
        let eds_path = dir.path().join("eds.yaml");
        std::fs::write(&eds_path, eds_file("eds_backend", 0, true)).unwrap();
        let yaml = bootstrap_yaml_with_eds(eds_path.to_str().unwrap(), None);
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        // No dynamic_resources / rds — only the EDS cluster drives the gate.
        assert!(b.dynamic_clusters.is_none());
        assert!(b.dynamic_listeners.is_none());
        assert!(
            crate::load_dynamic_resources(&mut b).is_err(),
            "the validate gate must fire for a static-only EDS bootstrap"
        );
    }

    /// Step 4: a CDS file whose cluster is `type: EDS` carrying an inline
    /// `load_assignment` → `LoadAssignmentOnEdsCluster`. The CDS file bypasses
    /// `parse_bootstrap`'s endpoint-source check; `load_dynamic_resources` must
    /// re-run `check_endpoint_sources` over the merged static+CDS set BEFORE the
    /// EDS pass populates anything, so a malformed CDS-supplied EDS cluster is
    /// rejected.
    #[test]
    fn load_dynamic_eds_cds_eds_with_inline_load_assignment_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let cds_path = dir.path().join("cds.yaml");
        // A CDS cluster that is type: EDS but carries an inline load_assignment.
        let cds = r#"
resources:
- "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
  name: bad_eds
  type: EDS
  lb_policy: ROUND_ROBIN
  eds_cluster_config:
    eds_config:
      path_config_source: { path: /x }
  load_assignment:
    cluster_name: bad_eds
    endpoints:
    - lb_endpoints:
      - endpoint:
          address:
            socket_address: { address: 127.0.0.1, port_value: 8124 }
"#;
        std::fs::write(&cds_path, cds).unwrap();
        let yaml = bootstrap_yaml_with_cds_route_to(cds_path.to_str().unwrap(), "bad_eds");
        let mut b = crate::parse_bootstrap(&yaml).unwrap();
        let err = crate::load_dynamic_resources(&mut b).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::LoadAssignmentOnEdsCluster { ref cluster } if cluster == "bad_eds"),
            "got {err:?}"
        );
    }

    #[test]
    fn parses_cluster_with_type_strict_dns() {
        // 05.1 NEW: ClusterType gains StrictDns variant. The serde tag STRICT_DNS
        // maps mechanically via the existing #[serde(rename_all = "SCREAMING_SNAKE_CASE")].
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(
            matches!(c.cluster_type, ClusterType::StrictDns),
            "expected ClusterType::StrictDns, got {:?}",
            c.cluster_type,
        );
        assert_eq!(c.name, "backend");
        assert_eq!(
            c.load_assignment.as_ref().unwrap().endpoints[0].lb_endpoints[0]
                .endpoint
                .address
                .socket_address
                .address,
            "localhost",
        );
    }

    #[test]
    fn parses_cluster_with_type_static_unchanged() {
        // 05.1 NEW: regression guard — the existing STATIC parse path stays
        // unchanged after StrictDns lands. (Phase-02.1 REVIEW I3 originally
        // requested this discriminator test; the positive Static runtime test
        // lands separately in envoy-cluster as static_cluster_constructs_with_literal_ip.)
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(
            matches!(c.cluster_type, ClusterType::Static),
            "expected ClusterType::Static, got {:?}",
            c.cluster_type,
        );
    }

    #[test]
    fn rejects_cluster_with_type_logical_dns() {
        // 05.1 NEW: documents the ADR-0023 LOGICAL_DNS deferral at the parser surface.
        // serde rejects with an "unknown variant" error naming LOGICAL_DNS. If a
        // future phase lifts the deferral, this test gets renamed to
        // parses_cluster_with_type_logical_dns and the assertion flips.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: LOGICAL_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: example.com
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject LOGICAL_DNS");
        let s = err.to_string();
        assert!(
            s.contains("LOGICAL_DNS") || s.contains("unknown variant"),
            "expected LOGICAL_DNS unknown-variant error, got: {s}",
        );
    }

    #[test]
    fn rejects_cluster_with_unknown_type_value() {
        // 05.1 NEW: covers the deny_unknown_fields-equivalent posture on the
        // variant tag — any tag that isn't STATIC or STRICT_DNS rejects.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: WEIRD_TYPE
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject WEIRD_TYPE");
        let s = err.to_string();
        assert!(
            s.contains("WEIRD_TYPE") || s.contains("unknown variant"),
            "expected WEIRD_TYPE unknown-variant error, got: {s}",
        );
    }

    #[test]
    fn parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment() {
        // 05.1 NEW: verifies that DNS-name endpoints are stored as raw strings at
        // config-parse time (resolution lands at runtime in envoy-cluster's
        // from_bootstrap, NOT at parse time). Two endpoints with the same DNS name
        // but different ports parse cleanly into the Vec<LbEndpoint>.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7001
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(matches!(c.cluster_type, ClusterType::StrictDns));
        let lbe = &c.load_assignment.as_ref().unwrap().endpoints[0].lb_endpoints;
        assert_eq!(lbe.len(), 2);
        assert_eq!(lbe[0].endpoint.address.socket_address.address, "localhost");
        assert_eq!(lbe[0].endpoint.address.socket_address.port_value, 7000);
        assert_eq!(lbe[1].endpoint.address.socket_address.address, "localhost");
        assert_eq!(lbe[1].endpoint.address.socket_address.port_value, 7001);
    }

    #[test]
    fn validates_strict_dns_cluster_does_not_require_literal_ip_endpoints() {
        // 05.1 NEW: explicit assertion that envoy-config's validator passes the
        // parse stage for STRICT_DNS clusters even though the endpoint address is
        // a DNS name (not a literal IP). The runtime-side endpoint parse via
        // SocketAddr::from_str (which would fail on "host.docker.internal") lives
        // in envoy-cluster's from_bootstrap, NOT in envoy-config's validator —
        // and envoy-cluster's STRICT_DNS arm uses tokio::net::lookup_host instead
        // of SocketAddr::from_str on the StrictDns path, so the DNS-name endpoint
        // is fine end-to-end.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: host.docker.internal
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        // The validator passes the parse stage cleanly; runtime resolution is
        // out of scope for this test (envoy-cluster's from_bootstrap is not
        // invoked here).
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
        let c = &bootstrap.static_resources.clusters[0];
        assert!(matches!(c.cluster_type, ClusterType::StrictDns));
        assert_eq!(
            c.load_assignment.as_ref().unwrap().endpoints[0].lb_endpoints[0]
                .endpoint
                .address
                .socket_address
                .address,
            "host.docker.internal",
        );
    }

    /// 05.4 NEW (D1, ADR-0024): Cluster gains `dns_lookup_family: Option<DnsLookupFamily>`.
    /// The field is parsed-and-stored on envoy-rust's typed Cluster struct; runtime
    /// non-consumption is deliberate per ADR-0024 (only the upstream Envoy side
    /// observes the V4_ONLY knob via the D2 envoy.yaml edit).
    #[test]
    fn parses_cluster_with_dns_lookup_family_v4_only() {
        let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: "host.docker.internal", port_value: 9001 } }
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses");
        assert_eq!(bootstrap.static_resources.clusters.len(), 1);
        let c = &bootstrap.static_resources.clusters[0];
        assert!(matches!(c.cluster_type, ClusterType::StrictDns));
        assert_eq!(c.dns_lookup_family, Some(DnsLookupFamily::V4Only));
    }

    /// 05.4 NEW (D3, ADR-0026): Listener gains `listener_filters: Vec<serde_yaml::Value>`
    /// parse-and-ignore field. envoy-rust never executes listener filters by design
    /// (phase 03.2 chose to put SNI dispatch at the rustls layer); the field is
    /// purely for upstream-Envoy `envoy.yaml` parseability. New pattern in
    /// envoy-config — see ADR-0026.
    #[test]
    fn parses_listener_with_tls_inspector_listener_filter() {
        let yaml = r#"
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: "0.0.0.0", port_value: 0 } }
      listener_filters:
        - name: envoy.filters.listener.tls_inspector
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.listener.tls_inspector.v3.TlsInspector
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: "/tmp/leaf.pem" }
                    private_key:       { filename: "/tmp/leaf.key" }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: "127.0.0.1", port_value: 9001 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parses");
        assert_eq!(bootstrap.static_resources.listeners.len(), 1);
        let listener = &bootstrap.static_resources.listeners[0];
        assert_eq!(listener.listener_filters.len(), 1);
        // Smoke-check the opaque value contains the tls_inspector filter name.
        let filter_yaml =
            serde_yaml::to_string(&listener.listener_filters[0]).expect("filter serialises back");
        assert!(
            filter_yaml.contains("envoy.filters.listener.tls_inspector"),
            "filter yaml should contain tls_inspector name: {filter_yaml:?}"
        );
    }

    // --- phase 05.2 Task 2: codec_type: HTTP2 accept-flip + TLS-rejection ---

    #[test]
    fn parses_hcm_with_codec_type_http2() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) = listener.filter_chains[0].filters[0]
            .typed_config
            .as_ref()
            .unwrap()
        else {
            panic!("expected HCM");
        };
        assert!(matches!(hcm.codec_type, CodecType::HTTP2));
    }

    #[test]
    fn rejects_hcm_with_codec_type_http2_on_tls_listener() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: tls_h2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/cert.pem }
                    private_key: { filename: /tmp/key.pem }
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject TLS+HTTP2");
        assert!(
            matches!(err, crate::ConfigError::Http2OverTlsNotSupported),
            "expected Http2OverTlsNotSupported, got {err:?}"
        );
    }

    // --- phase 05.2 Task 3: Http2ProtocolOptions struct + RFC 7540 ranges ---

    #[test]
    fn parses_hcm_http2_protocol_options_default() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else {
            panic!();
        };
        assert!(hcm.http2_protocol_options.is_none());
    }

    #[test]
    fn parses_hcm_http2_protocol_options_all_fields() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                http2_protocol_options:
                  max_concurrent_streams: 50
                  initial_stream_window_size: 131072
                  initial_connection_window_size: 262144
                  max_frame_size: 32768
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else {
            panic!();
        };
        let opts = hcm.http2_protocol_options.as_ref().expect("present");
        assert_eq!(opts.max_concurrent_streams, Some(50));
        assert_eq!(opts.initial_stream_window_size, Some(131072));
        assert_eq!(opts.initial_connection_window_size, Some(262144));
        assert_eq!(opts.max_frame_size, Some(32768));
    }

    #[test]
    fn rejects_http2_protocol_options_max_frame_size_too_small() {
        let yaml = http2_options_yaml(/* max_frame_size = */ Some(1024), None, None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        match err {
            crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                field,
                value,
                range,
            } => {
                assert_eq!(field, "max_frame_size");
                assert_eq!(value, 1024);
                assert_eq!(range, (16384, 16777215));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_http2_protocol_options_max_frame_size_too_large() {
        let yaml = http2_options_yaml(Some(17_000_000), None, None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "max_frame_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_initial_stream_window_size_too_large() {
        // 2^31 = 2147483648 is one above the max.
        let yaml = http2_options_yaml(None, Some(2_147_483_648), None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "initial_stream_window_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_initial_connection_window_size_too_large() {
        let yaml = http2_options_yaml(None, None, Some(2_147_483_648), None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "initial_connection_window_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_unknown_field() {
        // hpack_table_size is a real Envoy field; envoy-rust 05.2 doesn't ship
        // it. The struct's deny_unknown_fields rejects.
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                http2_protocol_options:
                  hpack_table_size: 4096
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert_unknown_field(err);
    }

    #[test]
    fn fuzz_corpus_hcm_codec_http2_seed_parses() {
        // Sanity-check that the new fuzz seed parses cleanly through the
        // serde + validator pipeline. First per-seed content-asserting
        // corpus-walk test in this file; the cohort-level pattern lives
        // in `fuzz_corpus_seeds_parse_or_reject_cleanly` (~line 2274).
        let yaml = include_str!("../fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml");
        let bs = crate::parse_bootstrap(yaml).expect("seed must parse");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else {
            panic!();
        };
        assert!(matches!(hcm.codec_type, CodecType::HTTP2));
        let opts = hcm
            .http2_protocol_options
            .as_ref()
            .expect("seed has options");
        assert_eq!(opts.max_concurrent_streams, Some(100));
    }

    #[test]
    fn fuzz_corpus_cluster_http2_protocol_options_seed_parses() {
        // Mirrors the existing fuzz_corpus_hcm_codec_http2_seed_parses
        // pattern. The seed exercises the cluster-side
        // typed_extension_protocol_options accept-path; the fuzzer never runs
        // the H2 codec or the runtime cluster construction
        // (`parse_bootstrap` only exercises serde + the validator).
        let yaml =
            include_str!("../fuzz/corpus/parse_bootstrap/cluster_http2_protocol_options.yaml");
        crate::parse_bootstrap(yaml).expect("seed parses cleanly");
    }

    /// Builds a minimal HCM `codec_type: HTTP2` bootstrap with the given
    /// http2_protocol_options field values. Helper for the 4 range-rejection
    /// tests above. Each Option<u32> argument controls one field.
    fn http2_options_yaml(
        max_frame_size: Option<u32>,
        initial_stream_window_size: Option<u32>,
        initial_connection_window_size: Option<u32>,
        max_concurrent_streams: Option<u32>,
    ) -> String {
        let mut opts_block = String::from("                http2_protocol_options:\n");
        if let Some(v) = max_frame_size {
            opts_block.push_str(&format!("                  max_frame_size: {v}\n"));
        }
        if let Some(v) = initial_stream_window_size {
            opts_block.push_str(&format!(
                "                  initial_stream_window_size: {v}\n"
            ));
        }
        if let Some(v) = initial_connection_window_size {
            opts_block.push_str(&format!(
                "                  initial_connection_window_size: {v}\n"
            ));
        }
        if let Some(v) = max_concurrent_streams {
            opts_block.push_str(&format!("                  max_concurrent_streams: {v}\n"));
        }
        format!(
            r#"
node: {{ id: x, cluster: y }}
static_resources:
  listeners:
    - name: h2
      address: {{ socket_address: {{ address: 0.0.0.0, port_value: 9000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
{opts_block}                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
        )
    }

    #[test]
    fn parses_cluster_with_typed_extension_protocol_options_http2() {
        // 06.3 D14.3: changed from codec_type HTTP1 → HTTP2 so the H2 cluster
        // target remains valid under the new H1×H2 reachability gate. The
        // purpose of this test is to verify typed_extension_protocol_options
        // parsing, not codec negotiation; HTTP2 listener + H2 cluster is the
        // correct canonical shape.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_concurrent_streams: 100
              initial_stream_window_size: 65535
              initial_connection_window_size: 65535
              max_frame_size: 16384
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        let teo = cluster
            .typed_extension_protocol_options
            .as_ref()
            .expect("typed_extension_protocol_options present");
        let h2 = teo
            .http_protocol_options
            .explicit_http_config
            .http2_protocol_options
            .as_ref()
            .expect("http2 arm present");
        assert_eq!(h2.max_concurrent_streams, Some(100));
        assert_eq!(h2.max_frame_size, Some(16384));
    }

    #[test]
    fn parses_cluster_with_typed_extension_protocol_options_http1() {
        // The H1 arm of explicit_http_config is the empty Http1ProtocolOptions.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        let teo = cluster
            .typed_extension_protocol_options
            .as_ref()
            .expect("teo present");
        assert!(
            teo.http_protocol_options
                .explicit_http_config
                .http_protocol_options
                .is_some()
        );
        assert!(
            teo.http_protocol_options
                .explicit_http_config
                .http2_protocol_options
                .is_none()
        );
    }

    #[test]
    fn rejects_cluster_with_both_http1_and_http2_in_explicit_http_config() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http_protocol_options: {}
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects mutual");
        assert!(
            matches!(
                err,
                crate::ConfigError::MutuallyExclusiveExplicitHttpConfig { ref cluster }
                    if cluster == "backend"
            ),
            "expected MutuallyExclusiveExplicitHttpConfig {{cluster: backend}}, got {err:?}"
        );
    }

    #[test]
    fn rejects_cluster_with_wrong_typed_config_url() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.config.core.v3.Http2ProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects wrong URL");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedTypedConfigUrl { .. }),
            "expected UnsupportedTypedConfigUrl, got {err:?}"
        );
    }

    #[test]
    fn rejects_cluster_http2_protocol_options_max_frame_size_too_small() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options:
              max_frame_size: 1024
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects out-of-range");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                    field: "max_frame_size",
                    value: 1024,
                    ..
                }
            ),
            "expected Http2ProtocolOptionsOutOfRange, got {err:?}"
        );
    }

    #[test]
    fn parses_cluster_without_typed_extension_protocol_options_defaults_to_http1() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let cluster = &bs.static_resources.clusters[0];
        assert!(cluster.typed_extension_protocol_options.is_none());
    }

    #[test]
    fn rejects_cluster_with_unknown_typed_extension_key() {
        // Key other than HttpProtocolOptions; serde deny_unknown_fields rejects.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.UnknownExtension":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.UnknownExtension
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("rejects unknown key");
        // serde "unknown field" on TypedExtensionProtocolOptions surfaces as
        // ConfigError::Yaml (the deny_unknown_fields path on the
        // TypedExtensionProtocolOptions wrapper struct).
        assert!(
            matches!(err, crate::ConfigError::Yaml(_)),
            "expected serde Yaml error for unknown typed-extension key, got {err:?}"
        );
    }

    #[test]
    fn parses_cluster_with_strict_dns_and_http2_protocol_options_combined() {
        // Load-bearing for fixture 0010 (Task 10) which combines exactly these
        // two surfaces.
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: l
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses STRICT_DNS + H2 combined");
        let cluster = &bs.static_resources.clusters[0];
        assert!(matches!(cluster.cluster_type, ClusterType::StrictDns));
        assert!(cluster.typed_extension_protocol_options.is_some());
    }

    // --- 06.1 Task 9: Admin.access_log_path parse-and-ignore (ADR-0026) ---

    #[test]
    fn parses_admin_with_access_log_path() {
        let yaml = r#"
node: { id: t, cluster: t }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
  access_log_path: /var/log/envoy_admin.log
static_resources: { listeners: [], clusters: [] }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse OK");
        let admin = bootstrap.admin.expect("admin present");
        assert_eq!(
            admin.access_log_path,
            Some("/var/log/envoy_admin.log".to_string())
        );
    }

    #[test]
    fn parses_admin_without_access_log_path() {
        let yaml = r#"
node: { id: t, cluster: t }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
static_resources: { listeners: [], clusters: [] }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse OK");
        let admin = bootstrap.admin.expect("admin present");
        assert_eq!(admin.access_log_path, None);
    }

    #[test]
    fn rejects_admin_with_unknown_field() {
        let yaml = r#"
node: { id: t, cluster: t }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
  profile_path: /tmp
static_resources: { listeners: [], clusters: [] }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("unknown field rejected");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("profile_path") || msg.contains("unknown field"),
            "diagnostic should mention the unknown field; got: {msg}"
        );
    }

    // ----- 06.2 Task 5: access_log schema tests -----

    fn hcm_with_access_log_yaml(access_log_block: &str) -> String {
        format!(
            r#"
node: {{ id: t, cluster: t }}
static_resources:
  listeners:
    - name: l1
      address: {{ socket_address: {{ address: 127.0.0.1, port_value: 10000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
{access_log_block}
                route_config:
                  name: r
                  virtual_hosts:
                    - name: v
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
            access_log_block = access_log_block
        )
    }

    #[test]
    fn parses_hcm_with_file_access_log() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let listener = &bootstrap.static_resources.listeners[0];
        let filter = &listener.filter_chains[0].filters[0];
        let hcm = match &filter.typed_config {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        assert_eq!(hcm.access_log.len(), 1);
        assert_eq!(hcm.access_log[0].name, "envoy.access_loggers.file");
        match &hcm.access_log[0].typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                assert_eq!(cfg.path, "/tmp/access.log");
            }
        }
    }

    #[test]
    fn parses_hcm_with_no_access_log_block() {
        let yaml = hcm_with_access_log_yaml("");
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0]
            .typed_config
        {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        assert!(hcm.access_log.is_empty());
    }

    #[test]
    fn parses_hcm_with_empty_access_log_array() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log: []
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0]
            .typed_config
        {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        assert!(hcm.access_log.is_empty());
    }

    #[test]
    fn rejects_hcm_with_unsupported_access_log_name() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.stdout
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        match err {
            crate::ConfigError::UnsupportedAccessLogType { actual } => {
                assert_eq!(actual, "envoy.access_loggers.stdout");
            }
            other => panic!("expected UnsupportedAccessLogType; got {:?}", other),
        }
    }

    #[test]
    fn rejects_hcm_with_unsupported_access_log_type_url() {
        // The serde-tagged `@type` enum rejects unknown URLs at
        // deserialization time (wrapped as ConfigError::Yaml).
        // The test accepts either error path.
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.unknown.v3.UnknownAccessLog
                      path: /tmp/access.log
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        match err {
            crate::ConfigError::Yaml(_) => {}
            crate::ConfigError::UnsupportedAccessLogType { .. } => {}
            other => panic!("expected Yaml or UnsupportedAccessLogType; got {:?}", other),
        }
    }

    #[test]
    fn rejects_hcm_with_empty_access_log_path() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: ""
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(err, crate::ConfigError::InvalidAccessLogPath));
    }

    // --- phase 70 t1: AccessLog.filter (AccessLogFilter oneof) schema ---

    #[test]
    fn parses_status_code_filter_ge_500() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      status_code_filter:
                        comparison:
                          op: GE
                          value:
                            default_value: 500
                            runtime_key: unused
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("should parse");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0]
            .typed_config
        {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        let filter = hcm.access_log[0].filter.as_ref().expect("filter present");
        let scf = filter
            .status_code_filter
            .as_ref()
            .expect("status_code_filter present");
        assert_eq!(scf.comparison.op, crate::bootstrap::ComparisonOp::Ge);
        assert_eq!(scf.comparison.value.default_value, 500);
        assert_eq!(scf.comparison.value.runtime_key, "unused");
    }

    #[test]
    fn rejects_status_code_filter_unknown_op() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      status_code_filter:
                        comparison:
                          op: NE
                          value: { default_value: 500, runtime_key: unused }
"#,
        );
        // Unknown enum token → serde deserialize error → ConfigError::Yaml.
        let err = crate::parse_bootstrap(&yaml).expect_err("NE must be rejected");
        assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");
        // M70-R3: `ConfigError::Yaml(_)` alone is satisfied by ANY YAML-level
        // error, so an unrelated typo in the fixture above would keep this test
        // green for the wrong reason. Pin that the rejection is about the
        // offending token specifically. Anchored to the full serde phrase
        // (M70-R6): a bare `contains("NE")` is a 2-char substring that a future
        // `NONE`-like enum token would silently satisfy.
        let msg = err.to_string();
        assert!(
            msg.contains("unknown variant `NE`"),
            "rejection must name the offending op token, got {msg:?}",
        );
    }

    #[test]
    fn rejects_access_log_filter_with_no_variant() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter: {}
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("empty filter must be rejected");
        assert!(
            matches!(err, crate::ConfigError::AmbiguousAccessLogFilter { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_status_code_filter_empty_runtime_key() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/al.log
                    filter:
                      status_code_filter:
                        comparison:
                          op: GE
                          value: { default_value: 500, runtime_key: "" }
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("empty runtime_key must be rejected");
        assert!(
            matches!(err, crate::ConfigError::EmptyStatusCodeFilterRuntimeKey),
            "got {err:?}"
        );
    }

    // --- phase 32 t4: FileAccessLog.log_format boot-fatal format validation ---

    #[test]
    fn accepts_hcm_with_valid_access_log_format() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        text_format_source:
                          inline_string: "%RESPONSE_CODE%"
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0]
            .typed_config
        {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        match &hcm.access_log[0].typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                let fmt = cfg.log_format.as_ref().expect("log_format present");
                assert_eq!(
                    fmt.text_format_source.as_ref().unwrap().inline_string,
                    "%RESPONSE_CODE%"
                );
            }
        }
    }

    #[test]
    fn rejects_hcm_with_unknown_access_log_format_operator() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        text_format_source:
                          inline_string: "%NOPE%"
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(
            err,
            crate::ConfigError::InvalidAccessLogFormat { .. }
        ));
    }

    #[test]
    fn rejects_hcm_with_malformed_access_log_format() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        text_format_source:
                          inline_string: "%REQ("
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(
            err,
            crate::ConfigError::InvalidAccessLogFormat { .. }
        ));
    }

    #[test]
    fn accepts_hcm_with_absent_access_log_format() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
"#,
        );
        let bootstrap = crate::parse_bootstrap(&yaml).expect("parse + validate");
        let hcm = match &bootstrap.static_resources.listeners[0].filter_chains[0].filters[0]
            .typed_config
        {
            Some(TypedConfig::HttpConnectionManager(h)) => h,
            _ => panic!("expected HCM"),
        };
        match &hcm.access_log[0].typed_config {
            AccessLogTypedConfig::FileAccessLog(cfg) => {
                assert!(cfg.log_format.is_none());
            }
        }
    }

    #[test]
    fn rejects_hcm_with_unknown_nested_log_format_key() {
        // A misspelled nested key (`inline_strings` instead of `inline_string`)
        // must be caught by `#[serde(deny_unknown_fields)]` at YAML
        // deserialization — surfacing as `ConfigError::Yaml(_)`, NOT the later
        // `InvalidAccessLogFormat` validation error.
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        text_format_source:
                          inline_strings: "%RESPONSE_CODE%"
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(err, crate::ConfigError::Yaml(_)));
    }

    // --- phase 38 t1: SubstitutionFormatString {text_format_source|json_format} oneof ---

    #[test]
    fn json_format_parses_into_sorted_btreemap() {
        let yaml = r#"
text_format_source: null
json_format:
  zebra: "%PROTOCOL%"
  alpha: "%RESPONSE_CODE%"
"#;
        let sfs: SubstitutionFormatString = serde_yaml::from_str(yaml).unwrap();
        let jf = sfs.json_format.expect("json_format set");
        // BTreeMap iteration is sorted by key (ADR-0092 §A): alpha before zebra.
        let keys: Vec<&str> = jf.keys().map(String::as_str).collect();
        assert_eq!(keys, vec!["alpha", "zebra"]);
        assert!(sfs.text_format_source.is_none());
    }

    #[test]
    fn text_format_source_arm_still_parses() {
        let yaml = r#"text_format_source: { inline_string: "%RESPONSE_CODE%" }"#;
        let sfs: SubstitutionFormatString = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            sfs.text_format_source.unwrap().inline_string,
            "%RESPONSE_CODE%"
        );
        assert!(sfs.json_format.is_none());
    }

    // --- phase 40 t1 (ADR-0096 §E): omit_empty_values bool field ---

    #[test]
    fn omit_empty_values_round_trips_and_defaults_false() {
        let s: SubstitutionFormatString =
            serde_yaml::from_str("omit_empty_values: true\njson_format:\n  a: \"%PROTOCOL%\"\n")
                .unwrap();
        assert!(s.omit_empty_values);
        let d: SubstitutionFormatString =
            serde_yaml::from_str("json_format:\n  a: \"x\"\n").unwrap();
        assert!(!d.omit_empty_values); // default false
    }

    // --- phase 38 t2: exactly-one-of log_format cardinality + per-value parse ---

    #[test]
    fn both_arms_set_is_ambiguous() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        text_format_source:
                          inline_string: "%RESPONSE_CODE%"
                        json_format:
                          code: "%RESPONSE_CODE%"
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(err, crate::ConfigError::AmbiguousLogFormat { .. }));
    }

    #[test]
    fn neither_arm_set_is_ambiguous() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format: {}
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(err, crate::ConfigError::AmbiguousLogFormat { .. }));
    }

    #[test]
    fn empty_json_format_map_is_valid() {
        // ADR-0092 §E: empty json_format: {} → Envoy boots, emits `{}\n`.
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        json_format: {}
"#,
        );
        crate::parse_bootstrap(&yaml).expect("empty json_format is valid");
    }

    #[test]
    fn malformed_json_format_value_is_invalid_format() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        json_format:
                          a: "%NOPE%"
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(
            err,
            crate::ConfigError::InvalidAccessLogFormat { .. }
        ));
    }

    #[test]
    fn valid_json_format_passes() {
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        json_format:
                          code: "%RESPONSE_CODE%"
"#,
        );
        crate::parse_bootstrap(&yaml).expect("valid json_format passes");
    }

    // --- Phase 39 T2 (ADR-0094 §G): recursive per-leaf json_format validator ---

    #[test]
    fn malformed_operator_in_nested_object_leaf_is_invalid_format() {
        // ADR-0094 §G: a malformed operator in a NESTED object leaf is boot-fatal
        // via the EXISTING InvalidAccessLogFormat (recursive walk).
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        json_format:
                          a:
                            b: "%NOPE%"
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(
            err,
            crate::ConfigError::InvalidAccessLogFormat { .. }
        ));
    }

    #[test]
    fn malformed_operator_in_nested_list_leaf_is_invalid_format() {
        // ADR-0094 §G: a malformed operator inside a LIST leaf is boot-fatal too.
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        json_format:
                          a: [ "%PROTOCOL%", "%NOPE%" ]
"#,
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("expected reject");
        assert!(matches!(
            err,
            crate::ConfigError::InvalidAccessLogFormat { .. }
        ));
    }

    #[test]
    fn well_formed_nested_json_format_validates_ok() {
        // ADR-0094 §A–§F: a well-formed recursive config (nested object + list +
        // bool/null literal leaves) validates Ok.
        let yaml = hcm_with_access_log_yaml(
            r#"                access_log:
                  - name: envoy.access_loggers.file
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog
                      path: /tmp/access.log
                      log_format:
                        json_format:
                          top: "%PROTOCOL%"
                          obj: { method: "%REQ(:METHOD)%", code: "%RESPONSE_CODE%" }
                          list: [ "%REQ(:METHOD)%", "%RESPONSE_CODE%", "%UPSTREAM_HOST%" ]
                          flag: true
                          missing: ~
"#,
        );
        crate::parse_bootstrap(&yaml).expect("well-formed nested json_format validates");
    }

    // --- 06.3 Task 2: D14.3 H1-listener × H2-cluster parse-time validator gate ---

    /// Positive: codec_type HTTP1 + cluster with NO typed_extension_protocol_options →
    /// validator accepts. The default-H1 cluster is always reachable from an H1 listener.
    #[test]
    fn validates_h1_listener_with_h1_cluster_passes() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b).expect("H1 listener + H1 cluster (no teo) must be accepted");
    }

    /// Positive: codec_type HTTP2 + cluster with typed_extension_protocol_options carrying
    /// http2_protocol_options → validator accepts. H2×H2 is the canonical H2 path.
    #[test]
    fn validates_h2_listener_with_h2_cluster_passes() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b).expect("H2 listener + H2 cluster must be accepted");
    }

    /// Positive: codec_type HTTP2 + cluster with NO typed_extension_protocol_options →
    /// validator accepts. Per 05.3 D4, an H2 listener proxying to an H1 cluster MUST
    /// keep working — the gate is H1/AUTO × H2-cluster only, not H2 × H1-cluster.
    #[test]
    fn validates_h2_listener_with_h1_cluster_passes() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b).expect("H2 listener + H1 cluster (no teo) must be accepted");
    }

    /// Negative: codec_type HTTP1 + cluster carries http2_protocol_options →
    /// validator returns ConfigError::Http2ClusterFromHttp1Listener.
    /// Closes 05.3 REVIEW I1 per ADR-0028 option-(B) deferral gate.
    #[test]
    fn rejects_h1_listener_with_h2_cluster() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&mut b).unwrap_err();
        match err {
            crate::ConfigError::Http2ClusterFromHttp1Listener {
                ref listener,
                ref cluster,
            } => {
                assert_eq!(listener, "ingress_http");
                assert_eq!(cluster, "backend");
            }
            other => panic!(
                "expected Http2ClusterFromHttp1Listener {{listener: ingress_http, cluster: backend}}, got {:?}",
                other
            ),
        }
    }

    /// Negative: codec_type AUTO + cluster carries http2_protocol_options →
    /// same rejection as HTTP1. AUTO is treated as H1-only per parent §4 of the spec.
    #[test]
    fn rejects_auto_listener_with_h2_cluster() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: AUTO
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          route: { cluster: backend }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        let err = validate(&mut b).unwrap_err();
        match err {
            crate::ConfigError::Http2ClusterFromHttp1Listener {
                ref listener,
                ref cluster,
            } => {
                assert_eq!(listener, "ingress_http");
                assert_eq!(cluster, "backend");
            }
            other => panic!(
                "expected Http2ClusterFromHttp1Listener {{listener: ingress_http, cluster: backend}}, got {:?}",
                other
            ),
        }
    }

    /// Carve-out: TCP-proxy listener (no codec_type, no HCM) + cluster carrying
    /// http2_protocol_options → validator accepts. The H1×H2 gate is HCM-scoped only;
    /// TCP-proxy routes don't undergo codec negotiation on the listener side.
    #[test]
    fn tcp_proxy_listener_with_h2_cluster_unaffected() {
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
static_resources:
  listeners:
    - name: ingress_tcp
      address: { socket_address: { address: 0.0.0.0, port_value: 8080 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
      typed_extension_protocol_options:
        "envoy.extensions.upstreams.http.v3.HttpProtocolOptions":
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {}
"#;
        let mut b: Bootstrap = serde_yaml::from_str(yaml).unwrap();
        validate(&mut b)
            .expect("TCP-proxy listener + H2 cluster must be accepted (gate is HCM-only)");
    }

    #[test]
    fn validate_http_filters_accepts_single_router() {
        let filters = vec![crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }];
        let result = super::validate_http_filters(&filters, "ingress_http");
        assert!(result.is_ok(), "single Router passes; got {result:?}");
    }

    #[test]
    fn validate_http_filters_rejects_empty_list() {
        let filters: Vec<crate::HttpFilter> = Vec::new();
        let err =
            super::validate_http_filters(&filters, "ingress_http").expect_err("empty list rejects");
        match err {
            crate::ConfigError::EmptyHttpFilters { listener } => {
                assert_eq!(listener, "ingress_http");
            }
            other => panic!("expected EmptyHttpFilters, got {other:?}"),
        }
    }

    #[test]
    fn validate_http_filters_rejects_duplicate_router() {
        let router = || crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        };
        let filters = vec![router(), router()];
        let err = super::validate_http_filters(&filters, "ingress_http")
            .expect_err("duplicate Router rejects");
        match err {
            crate::ConfigError::DuplicateRouterFilter { listener } => {
                assert_eq!(listener, "ingress_http");
            }
            other => panic!("expected DuplicateRouterFilter, got {other:?}"),
        }
    }

    #[test]
    fn validate_http_filters_rejects_name_typed_config_mismatch() {
        let filters = vec![crate::HttpFilter {
            name: "envoy.filters.http.fault".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }];
        let err = super::validate_http_filters(&filters, "ingress_http")
            .expect_err("name/typed_config mismatch rejects");
        match err {
            crate::ConfigError::UnsupportedHttpFilter { name } => {
                assert_eq!(name, "envoy.filters.http.fault");
            }
            other => panic!("expected UnsupportedHttpFilter, got {other:?}"),
        }
    }

    #[test]
    fn validate_http_filters_listener_name_propagates() {
        let filters: Vec<crate::HttpFilter> = Vec::new();
        let err = super::validate_http_filters(&filters, "custom_listener_42")
            .expect_err("empty list rejects");
        assert!(format!("{err:?}").contains("custom_listener_42"));
    }

    #[test]
    fn validate_http_filters_duplicate_router_takes_precedence_over_router_not_terminal() {
        let router = || crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        };
        let filters = vec![router(), router(), router()];
        let err =
            super::validate_http_filters(&filters, "ingress_http").expect_err("3 Routers rejects");
        assert!(matches!(
            err,
            crate::ConfigError::DuplicateRouterFilter { .. }
        ));
    }

    #[test]
    fn validate_http_filters_accepts_existing_fixture_shape() {
        let filters = vec![crate::HttpFilter {
            name: "envoy.filters.http.router".to_string(),
            typed_config: crate::HttpFilterTypedConfig::Router(crate::RouterConfig {}),
        }];
        super::validate_http_filters(&filters, "ingress_http")
            .expect("pre-07.1 fixture filter-chain shape stays valid");
    }

    mod header_mutation_schema_tests {
        use crate::{AppendAction, HeaderMutationConfig};

        fn parse(yaml: &str) -> Result<HeaderMutationConfig, serde_yaml::Error> {
            serde_yaml::from_str(yaml)
        }

        #[test]
        fn minimal_request_only_mutations_parse() {
            let cfg = parse(
                "mutations:\n  request_mutations:\n    - append:\n        header:\n          key: x-foo\n          value: bar\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect("request-only parses");
            assert_eq!(cfg.mutations.request_mutations.len(), 1);
            assert_eq!(cfg.mutations.response_mutations.len(), 0);
            let e = &cfg.mutations.request_mutations[0];
            assert_eq!(e.append.header.key, "x-foo");
            assert_eq!(e.append.header.value, "bar");
            assert_eq!(e.append.append_action, AppendAction::AppendIfExistsOrAdd);
        }

        #[test]
        fn minimal_response_only_mutations_parse() {
            let cfg = parse(
                "mutations:\n  response_mutations:\n    - append:\n        header:\n          key: x-resp\n          value: stamp\n        append_action: OVERWRITE_IF_EXISTS_OR_ADD\n",
            )
            .expect("response-only parses");
            assert_eq!(cfg.mutations.request_mutations.len(), 0);
            assert_eq!(cfg.mutations.response_mutations.len(), 1);
            assert_eq!(
                cfg.mutations.response_mutations[0].append.append_action,
                AppendAction::OverwriteIfExistsOrAdd
            );
        }

        #[test]
        fn both_request_and_response_mutations_parse() {
            let cfg = parse(
                "mutations:\n  request_mutations:\n    - append:\n        header:\n          key: x-req\n          value: a\n        append_action: APPEND_IF_EXISTS_OR_ADD\n  response_mutations:\n    - append:\n        header:\n          key: x-resp\n          value: b\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect("both parse");
            assert_eq!(cfg.mutations.request_mutations.len(), 1);
            assert_eq!(cfg.mutations.response_mutations.len(), 1);
            assert_eq!(
                cfg.mutations.request_mutations[0].append.header.key,
                "x-req"
            );
            assert_eq!(
                cfg.mutations.response_mutations[0].append.header.key,
                "x-resp"
            );
        }

        #[test]
        fn empty_mutations_parse_via_serde_default() {
            let cfg = parse("mutations: {}\n").expect("empty mutations parse");
            assert_eq!(cfg.mutations.request_mutations, Vec::new());
            assert_eq!(cfg.mutations.response_mutations, Vec::new());
        }

        #[test]
        fn multiple_entries_parse() {
            let cfg = parse(
                "mutations:\n  request_mutations:\n    - append:\n        header: { key: x-a, value: '1' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n    - append:\n        header: { key: x-b, value: '2' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n    - append:\n        header: { key: x-c, value: '3' }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect("3 entries parse");
            assert_eq!(cfg.mutations.request_mutations.len(), 3);
        }

        #[test]
        fn both_supported_append_actions_parse() {
            for (yaml_val, expect) in [
                ("APPEND_IF_EXISTS_OR_ADD", AppendAction::AppendIfExistsOrAdd),
                (
                    "OVERWRITE_IF_EXISTS_OR_ADD",
                    AppendAction::OverwriteIfExistsOrAdd,
                ),
            ] {
                let cfg = parse(&format!(
                    "mutations:\n  request_mutations:\n    - append:\n        header: {{ key: k, value: v }}\n        append_action: {yaml_val}\n"
                ))
                .expect("supported action parses");
                assert_eq!(
                    cfg.mutations.request_mutations[0].append.append_action,
                    expect
                );
            }
        }

        #[test]
        fn unsupported_append_actions_parse_at_schema_level() {
            // ADD_IF_ABSENT / OVERWRITE_IF_EXISTS parse at the schema layer; the
            // Task 2 validator rejects them. Present in the enum so serde does not
            // emit a generic "unknown variant" error.
            for (yaml_val, expect) in [
                ("ADD_IF_ABSENT", AppendAction::AddIfAbsent),
                ("OVERWRITE_IF_EXISTS", AppendAction::OverwriteIfExists),
            ] {
                let cfg = parse(&format!(
                    "mutations:\n  request_mutations:\n    - append:\n        header: {{ key: k, value: v }}\n        append_action: {yaml_val}\n"
                ))
                .expect("unsupported action still parses at schema level");
                assert_eq!(
                    cfg.mutations.request_mutations[0].append.append_action,
                    expect
                );
            }
        }

        #[test]
        fn unknown_field_rejects() {
            let err = parse("mutations:\n  request_mutations: []\n  bogus_key: 1\n")
                .expect_err("unknown field rejects");
            assert!(format!("{err}").contains("bogus_key") || format!("{err}").contains("unknown"));
        }

        #[test]
        fn missing_mutations_field_rejects() {
            parse("not_mutations: {}\n").expect_err("missing `mutations` rejects");
        }

        #[test]
        fn missing_key_field_rejects() {
            parse(
                "mutations:\n  request_mutations:\n    - append:\n        header: { value: v }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect_err("missing header.key rejects");
        }

        #[test]
        fn missing_value_field_rejects() {
            parse(
                "mutations:\n  request_mutations:\n    - append:\n        header: { key: k }\n        append_action: APPEND_IF_EXISTS_OR_ADD\n",
            )
            .expect_err("missing header.value rejects");
        }

        #[test]
        fn unknown_at_type_url_rejects_on_http_filter() {
            // The tagged-enum on an unknown @type tag rejects.
            let err: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(
                "\"@type\": type.googleapis.com/envoy.extensions.filters.http.header_mutation.v3.HeaderMutation.unknown\n",
            );
            err.expect_err("unknown @type rejects");
        }
    }

    mod header_mutation_validator_tests {
        use super::router_filter;
        use crate::{
            AppendAction, HeaderMutationConfig, HeaderMutationEntry, HeaderValue,
            HeaderValueOption, HttpFilter, HttpFilterTypedConfig, Mutations,
        };

        fn entry(key: &str, value: &str, action: AppendAction) -> HeaderMutationEntry {
            HeaderMutationEntry {
                append: HeaderValueOption {
                    header: HeaderValue {
                        key: key.to_string(),
                        value: value.to_string(),
                    },
                    append_action: action,
                },
            }
        }

        fn header_mutation_filter(
            request_mutations: Vec<HeaderMutationEntry>,
            response_mutations: Vec<HeaderMutationEntry>,
        ) -> HttpFilter {
            HttpFilter {
                name: "envoy.filters.http.header_mutation".to_string(),
                typed_config: HttpFilterTypedConfig::HeaderMutation(HeaderMutationConfig {
                    mutations: Mutations {
                        request_mutations,
                        response_mutations,
                    },
                }),
            }
        }

        #[test]
        fn header_mutation_with_all_supported_entries_passes() {
            let filters = vec![
                header_mutation_filter(
                    vec![
                        entry("x-a", "1", AppendAction::AppendIfExistsOrAdd),
                        entry("x-b", "2", AppendAction::OverwriteIfExistsOrAdd),
                    ],
                    vec![
                        entry("x-c", "3", AppendAction::AppendIfExistsOrAdd),
                        entry("x-d", "4", AppendAction::OverwriteIfExistsOrAdd),
                    ],
                ),
                router_filter(),
            ];
            super::validate_http_filters(&filters, "ingress_http").expect("supported entries pass");
        }

        #[test]
        fn empty_key_rejects() {
            let filters = vec![
                header_mutation_filter(
                    vec![entry("", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("empty key rejects")
            {
                crate::ConfigError::EmptyHeaderMutationKey { listener, position } => {
                    assert_eq!(listener, "ingress_http");
                    assert_eq!(position, 0);
                }
                other => panic!("expected EmptyHeaderMutationKey, got {other:?}"),
            }
        }

        #[test]
        fn invalid_token_in_key_rejects() {
            let filters = vec![
                header_mutation_filter(
                    vec![entry("x bad", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("invalid token rejects")
            {
                crate::ConfigError::InvalidHeaderMutationKey {
                    listener,
                    position,
                    key,
                } => {
                    assert_eq!(listener, "ingress_http");
                    assert_eq!(position, 0);
                    assert_eq!(key, "x bad");
                }
                other => panic!("expected InvalidHeaderMutationKey, got {other:?}"),
            }
        }

        #[test]
        fn add_if_absent_rejects() {
            let filters = vec![
                header_mutation_filter(vec![entry("x-a", "v", AppendAction::AddIfAbsent)], vec![]),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("ADD_IF_ABSENT rejects")
            {
                crate::ConfigError::UnsupportedHeaderMutationAppendAction {
                    listener,
                    position,
                    action,
                } => {
                    assert_eq!(listener, "ingress_http");
                    assert_eq!(position, 0);
                    assert_eq!(action, "ADD_IF_ABSENT");
                }
                other => panic!("expected UnsupportedHeaderMutationAppendAction, got {other:?}"),
            }
        }

        #[test]
        fn overwrite_if_exists_rejects() {
            let filters = vec![
                header_mutation_filter(
                    vec![],
                    vec![entry("x-a", "v", AppendAction::OverwriteIfExists)],
                ),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("OVERWRITE_IF_EXISTS rejects")
            {
                crate::ConfigError::UnsupportedHeaderMutationAppendAction { action, .. } => {
                    assert_eq!(action, "OVERWRITE_IF_EXISTS");
                }
                other => panic!("expected UnsupportedHeaderMutationAppendAction, got {other:?}"),
            }
        }

        #[test]
        fn router_not_terminal_still_rejects_under_header_mutation_chain() {
            // [Router, HeaderMutation] — Router first; the 07.1 Task 4 validator
            // still fires RouterNotTerminal.
            let filters = vec![
                router_filter(),
                header_mutation_filter(
                    vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("Router-not-terminal rejects")
            {
                crate::ConfigError::RouterNotTerminal { position, .. } => {
                    assert_eq!(position, 0)
                }
                other => panic!("expected RouterNotTerminal, got {other:?}"),
            }
        }

        #[test]
        fn duplicate_router_rejects_under_header_mutation_chain() {
            let filters = vec![
                header_mutation_filter(
                    vec![entry("x-a", "v", AppendAction::AppendIfExistsOrAdd)],
                    vec![],
                ),
                router_filter(),
                router_filter(),
            ];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("duplicate Router rejects")
            {
                crate::ConfigError::DuplicateRouterFilter { .. } => {}
                other => panic!("expected DuplicateRouterFilter, got {other:?}"),
            }
        }

        #[test]
        fn name_typed_config_mismatch_rejects() {
            let mut f = header_mutation_filter(vec![], vec![]);
            f.name = "envoy.filters.http.fault".to_string();
            let filters = vec![f, router_filter()];
            match super::validate_http_filters(&filters, "ingress_http")
                .expect_err("name/typed_config mismatch rejects")
            {
                crate::ConfigError::UnsupportedHttpFilter { name } => {
                    assert_eq!(name, "envoy.filters.http.fault");
                }
                other => panic!("expected UnsupportedHttpFilter, got {other:?}"),
            }
        }
    }

    // -------------------------------------------------------------------
    // Phase 09 — LocalRateLimit envoy-config schema + validator tests
    // (Task 1; see docs/envoy-rust/phases/09-http-filter-local-rate-limit/PLAN.md)
    // -------------------------------------------------------------------
    mod local_rate_limit_tests {
        use super::super::{parse_duration, validate_http_filters};
        use super::router_filter;
        use crate::{
            AppendAction, ConfigError, HttpFilter, HttpFilterTypedConfig, HttpStatus,
            LocalRateLimitConfig, TokenBucket,
        };

        fn parse(yaml: &str) -> Result<LocalRateLimitConfig, serde_yaml::Error> {
            serde_yaml::from_str(yaml)
        }

        #[test]
        fn deserialize_local_rate_limit_minimal_succeeds() {
            let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
"#;
            let cfg = parse(yaml).expect("minimal LocalRateLimit parses");
            assert_eq!(cfg.stat_prefix, "phase_09");
            assert_eq!(cfg.token_bucket.max_tokens, 3);
            assert_eq!(cfg.token_bucket.tokens_per_fill, 0);
            assert_eq!(cfg.status.code, 429);
            assert!(cfg.response_headers_to_add.is_empty());
        }

        #[test]
        fn deserialize_local_rate_limit_with_status_succeeds() {
            let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
status:
  code: 429
"#;
            let cfg = parse(yaml).expect("with status parses");
            assert_eq!(cfg.status.code, 429);
        }

        #[test]
        fn deserialize_local_rate_limit_with_response_headers_succeeds() {
            let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
response_headers_to_add:
  - header:
      key: x-rate-limit-policy
      value: phase-09
    append_action: APPEND_IF_EXISTS_OR_ADD
"#;
            let cfg = parse(yaml).expect("with response_headers_to_add parses");
            assert_eq!(cfg.response_headers_to_add.len(), 1);
            assert_eq!(
                cfg.response_headers_to_add[0].header.key,
                "x-rate-limit-policy"
            );
            assert_eq!(cfg.response_headers_to_add[0].header.value, "phase-09");
            assert_eq!(
                cfg.response_headers_to_add[0].append_action,
                AppendAction::AppendIfExistsOrAdd
            );
        }

        #[test]
        fn deserialize_local_rate_limit_rejects_unknown_field() {
            let yaml = r#"
stat_prefix: phase_09
token_bucket:
  max_tokens: 3
  tokens_per_fill: 0
  fill_interval: 60s
descriptors: []
"#;
            let err = parse(yaml).expect_err("unknown field rejected by deny_unknown_fields");
            assert!(format!("{err}").contains("descriptors"), "err: {err}");
        }

        fn make_filter(cfg: LocalRateLimitConfig) -> HttpFilter {
            HttpFilter {
                name: "envoy.filters.http.local_ratelimit".to_string(),
                typed_config: HttpFilterTypedConfig::LocalRateLimit(cfg),
            }
        }

        fn ok_cfg() -> LocalRateLimitConfig {
            LocalRateLimitConfig {
                stat_prefix: "phase_09".to_string(),
                token_bucket: TokenBucket {
                    max_tokens: 3,
                    tokens_per_fill: 0,
                    fill_interval: serde_yaml::Value::String("60s".to_string()),
                },
                response_headers_to_add: Vec::new(),
                status: HttpStatus { code: 429 },
            }
        }

        #[test]
        fn validate_accepts_local_rate_limit_followed_by_router() {
            let filters = vec![make_filter(ok_cfg()), router_filter()];
            validate_http_filters(&filters, "ingress_http").expect("valid chain");
        }

        #[test]
        fn validate_rejects_empty_stat_prefix() {
            let mut cfg = ok_cfg();
            cfg.stat_prefix = String::new();
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::EmptyLocalRateLimitStatPrefix { ref listener } if listener == "ingress_http"
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_zero_max_tokens() {
            let mut cfg = ok_cfg();
            cfg.token_bucket.max_tokens = 0;
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::TokenBucketMaxTokensMustBePositive { ref listener } if listener == "ingress_http"
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_zero_fill_interval() {
            let mut cfg = ok_cfg();
            cfg.token_bucket.fill_interval = serde_yaml::Value::String("0s".to_string());
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(err, ConfigError::InvalidTokenBucketFillInterval { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_unparseable_fill_interval() {
            let mut cfg = ok_cfg();
            cfg.token_bucket.fill_interval = serde_yaml::Value::String("forever".to_string());
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(err, ConfigError::InvalidTokenBucketFillInterval { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_non_429_status_code() {
            let mut cfg = ok_cfg();
            cfg.status = HttpStatus { code: 503 };
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::UnsupportedLocalRateLimitStatusCode { code, .. } if code == 503
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_local_rate_limit_with_wrong_name() {
            let mut filter = make_filter(ok_cfg());
            filter.name = "envoy.filters.http.something_else".to_string();
            let filters = vec![filter, router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(err, ConfigError::UnsupportedHttpFilter { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn parse_duration_accepts_seconds() {
            let d = parse_duration("60s").expect("60s parses");
            assert_eq!(d, std::time::Duration::from_secs(60));
        }

        #[test]
        fn parse_duration_accepts_milliseconds() {
            let d = parse_duration("250ms").expect("250ms parses");
            assert_eq!(d, std::time::Duration::from_millis(250));
        }

        #[test]
        fn parse_duration_accepts_microseconds() {
            let d = parse_duration("500us").expect("500us parses");
            assert_eq!(d, std::time::Duration::from_micros(500));
        }

        #[test]
        fn parse_duration_rejects_unknown_unit() {
            let err = parse_duration("60m")
                .expect_err("60m has no documented Duration shape at phase 09");
            assert!(err.contains("unit"), "err: {err}");
        }

        #[test]
        fn parse_duration_rejects_empty() {
            let err = parse_duration("").expect_err("empty rejected");
            assert!(!err.is_empty());
        }
    }

    // -------------------------------------------------------------------
    // Phase 10 — RBAC envoy-config schema + validator tests
    // (Task 1; see docs/envoy-rust/phases/10-http-filter-rbac/PLAN.md)
    // -------------------------------------------------------------------
    mod rbac_tests {
        use super::*;
        use crate::{
            Action, ConfigError, HttpFilter, HttpFilterTypedConfig, Permission, PermissionSet,
            Policy, Principal, PrincipalSet, RbacConfig, Rules,
        };
        use std::collections::BTreeMap;

        fn parse(yaml: &str) -> Result<RbacConfig, serde_yaml::Error> {
            serde_yaml::from_str(yaml)
        }

        #[test]
        fn deserialize_rbac_minimal_allow_succeeds() {
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    "pass_with_header":
      permissions:
        - any: true
      principals:
        - header:
            name: x-rbac-pass
            string_match: { exact: "yes" }
"#;
            let cfg = parse(yaml).expect("minimal Rbac parses");
            assert_eq!(cfg.rules.action, Action::Allow);
            assert_eq!(cfg.rules.policies.len(), 1);
            let p = cfg.rules.policies.get("pass_with_header").unwrap();
            assert_eq!(p.permissions.len(), 1);
            assert_eq!(p.principals.len(), 1);
        }

        #[test]
        fn deserialize_rbac_default_action_is_allow() {
            let yaml = r#"
rules:
  policies:
    "p":
      permissions: [{ any: true }]
      principals: [{ any: true }]
"#;
            let cfg = parse(yaml).expect("default action parses");
            assert_eq!(cfg.rules.action, Action::Allow);
        }

        #[test]
        fn deserialize_rbac_deny_action_succeeds() {
            let yaml = r#"
rules:
  action: DENY
  policies:
    "p":
      permissions: [{ any: true }]
      principals: [{ any: true }]
"#;
            let cfg = parse(yaml).expect("DENY action parses");
            assert_eq!(cfg.rules.action, Action::Deny);
        }

        #[test]
        fn deserialize_rbac_rejects_unknown_field() {
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    "p":
      permissions: [{ any: true }]
      principals: [{ any: true }]
shadow_rules: {}
"#;
            let err = parse(yaml).expect_err("unknown top-level field rejected");
            assert!(format!("{err}").contains("shadow_rules"), "err: {err}");
        }

        #[test]
        fn deserialize_rbac_permission_and_or_not_combinators_succeed() {
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    "complex":
      permissions:
        - and_rules:
            rules:
              - or_rules:
                  rules:
                    - any: true
                    - header: { name: x-a, string_match: { exact: "1" } }
              - not_rule:
                  header: { name: x-b, present_match: true }
      principals:
        - any: true
"#;
            let cfg = parse(yaml).expect("nested combinators parse");
            let p = cfg.rules.policies.get("complex").unwrap();
            assert_eq!(p.permissions.len(), 1);
            match &p.permissions[0] {
                Permission::AndRules(set) => assert_eq!(set.rules.len(), 2),
                other => panic!("expected AndRules, got {other:?}"),
            }
        }

        #[test]
        fn deserialize_rbac_principal_and_or_not_combinators_succeed() {
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    "complex_principals":
      permissions: [{ any: true }]
      principals:
        - and_ids:
            ids:
              - or_ids:
                  ids:
                    - any: true
                    - header: { name: x-c, string_match: { exact: "2" } }
              - not_id:
                  header: { name: x-d, present_match: true }
"#;
            let cfg = parse(yaml).expect("nested principal combinators parse");
            let p = cfg.rules.policies.get("complex_principals").unwrap();
            match &p.principals[0] {
                Principal::AndIds(set) => assert_eq!(set.ids.len(), 2),
                other => panic!("expected AndIds, got {other:?}"),
            }
        }

        // -------------------------------------------------------------------
        // phase 35 T1: RBAC `metadata` Permission/Principal matcher.
        // -------------------------------------------------------------------
        #[test]
        fn parses_rbac_metadata_permission() {
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    tier_prod:
      permissions:
        - metadata:
            filter: envoy.filters.http.header_to_metadata
            path:
              - key: tier
            value:
              string_match: { exact: "prod" }
      principals:
        - any: true
"#;
            let cfg: RbacConfig = serde_yaml::from_str(yaml).expect("parses");
            let policy = &cfg.rules.policies["tier_prod"];
            match &policy.permissions[0] {
                Permission::Metadata(m) => {
                    assert_eq!(m.filter, "envoy.filters.http.header_to_metadata");
                    assert_eq!(m.path.len(), 1);
                    assert_eq!(m.path[0].key, "tier");
                    assert!(m.value.matches("prod"));
                    assert!(!m.value.matches("dev"));
                }
                other => panic!("expected Metadata, got {other:?}"),
            }
        }

        #[test]
        fn rbac_metadata_permission_json_round_trips() {
            // BEHAVIOR_CONTRACT A1: the metadata matcher field names round-trip verbatim
            // (snake_case) through /config_dump. /config_dump is a JSON surface (mirrors the
            // 08.1 JSON roundtrip test at bootstrap.rs:14771), so JSON is the correct
            // serializer here. (YAML serialize would emit `!Tag` syntax for these
            // externally-tagged enums — the exact serde_yaml 0.9 limitation the hand-rolled
            // Permission/ValueMatcher Deserialize impls exist to work around — so the JSON
            // round-trip, not a YAML one, is what backs the A1 config_dump claim.)
            let yaml = r#"
metadata:
  filter: envoy.filters.http.header_to_metadata
  path:
    - key: tier
  value:
    string_match: { exact: "prod" }
"#;
            let p1: Permission = serde_yaml::from_str(yaml).expect("parses");
            let json = serde_json::to_string(&p1).expect("serializes to JSON");
            let p2: Permission = serde_json::from_str(&json).expect("JSON re-parses");
            assert_eq!(p1, p2);
            // and assert the JSON uses the verbatim snake_case keys, not Rust variant names
            assert!(json.contains("\"metadata\""));
            assert!(json.contains("\"filter\""));
            assert!(json.contains("\"string_match\""));
        }

        // ---- Phase 37: PathMatcher (RBAC url_path) config struct (Task 1) ----

        #[test]
        fn path_matcher_parses_exact_and_round_trips() {
            let pm: crate::PathMatcher =
                serde_yaml::from_str("path: { exact: \"/allowed\" }").unwrap();
            assert_eq!(
                pm.path,
                StringMatcher {
                    mode: StringMatcherMode::Exact("/allowed".into()),
                    ignore_case: false
                }
            );
            // round-trips through serde_yaml
            let s = serde_yaml::to_string(&pm).unwrap();
            let pm2: crate::PathMatcher = serde_yaml::from_str(&s).unwrap();
            assert_eq!(pm, pm2);
        }

        #[test]
        fn path_matcher_empty_is_missing_path_error() {
            // §D case 1: `url_path: {}` → empty PathMatcher → missing `path`.
            let err = serde_yaml::from_str::<crate::PathMatcher>("{}")
                .unwrap_err()
                .to_string();
            assert!(err.contains("path"), "want missing-path error, got: {err}");
        }

        #[test]
        fn path_matcher_empty_string_matcher_is_missing_mode_error() {
            // §D case 2: `url_path: { path: {} }` → StringMatcher with no mode key.
            let err = serde_yaml::from_str::<crate::PathMatcher>("path: {}")
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("mode key"),
                "want missing-mode error, got: {err}"
            );
        }

        #[test]
        fn path_matcher_unknown_subkey_is_denied() {
            // §D case 3: `url_path: { foo: bar }` → deny_unknown_fields.
            let err = serde_yaml::from_str::<crate::PathMatcher>("foo: bar")
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("foo") || err.contains("unknown"),
                "want unknown-field error, got: {err}"
            );
        }

        // ---- Phase 37: Permission::UrlPath (Task 2) ----

        #[test]
        fn permission_parses_url_path_and_json_round_trips() {
            // Parse from YAML (the config-load surface).
            let p: crate::Permission =
                serde_yaml::from_str("url_path: { path: { exact: \"/allowed\" } }").unwrap();
            assert!(matches!(&p, crate::Permission::UrlPath(pm)
                if pm.path == StringMatcher { mode: StringMatcherMode::Exact("/allowed".into()), ignore_case: false }));
            // Round-trip through JSON — NOT YAML. `serde_yaml` 0.9 serializes these
            // externally-tagged hand-rolled enums as `!url_path` TAG syntax, which the
            // hand-rolled Deserialize visitor does NOT accept on reparse (it expects a
            // map). /config_dump is a JSON surface anyway. Precedent + rationale:
            // `rbac_metadata_permission_json_round_trips` above.
            let s = serde_json::to_string(&p).unwrap();
            assert!(s.contains("url_path"));
            let p2: crate::Permission = serde_json::from_str(&s).unwrap();
            assert_eq!(p, p2);
        }

        // ---- Phase 37: Principal::UrlPath (Task 3) ----

        #[test]
        fn principal_parses_url_path() {
            let p: crate::Principal =
                serde_yaml::from_str("url_path: { path: { prefix: \"/api\" } }").unwrap();
            assert!(matches!(p, crate::Principal::UrlPath(_)));
        }

        // ---- Phase 37: url_path config-validity boot-fatal §D 1-3 (Task 5) ----
        // The SAME rejections as the bare-PathMatcher tests, but THROUGH a full
        // RBAC `Permission` (the real boot path).
        #[test]
        fn rbac_url_path_empty_and_unknown_are_boot_fatal() {
            assert!(serde_yaml::from_str::<crate::Permission>("url_path: {}").is_err()); // §D1
            assert!(serde_yaml::from_str::<crate::Permission>("url_path: { path: {} }").is_err()); // §D2
            assert!(serde_yaml::from_str::<crate::Permission>("url_path: { foo: bar }").is_err()); // §D3
        }

        #[test]
        fn parses_rbac_metadata_principal() {
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    tier_prod:
      permissions:
        - any: true
      principals:
        - metadata:
            filter: envoy.filters.http.header_to_metadata
            path:
              - key: tier
            value:
              string_match: { exact: "prod" }
"#;
            let cfg: RbacConfig = serde_yaml::from_str(yaml).expect("parses");
            let policy = &cfg.rules.policies["tier_prod"];
            match &policy.principals[0] {
                Principal::Metadata(m) => {
                    assert_eq!(m.filter, "envoy.filters.http.header_to_metadata");
                    assert_eq!(m.path.len(), 1);
                    assert_eq!(m.path[0].key, "tier");
                    assert!(m.value.matches("prod"));
                    assert!(!m.value.matches("dev"));
                }
                other => panic!("expected Metadata, got {other:?}"),
            }
        }

        #[test]
        fn rbac_metadata_accepts_present_match_value() {
            // F1 §A5: present_match is now an ACCEPTED ValueMatcher variant.
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    p:
      permissions:
        - metadata:
            filter: envoy.filters.http.header_to_metadata
            path: [ { key: tier } ]
            value: { present_match: true }
      principals:
        - any: true
"#;
            let cfg: RbacConfig = serde_yaml::from_str(yaml).expect("present_match accepted");
            match &cfg.rules.policies["p"].permissions[0] {
                Permission::Metadata(m) => assert_eq!(m.value, ValueMatcher::PresentMatch(true)),
                other => panic!("expected Metadata, got {other:?}"),
            }
        }

        #[test]
        fn rbac_metadata_present_match_false_parses() {
            let yaml = r#"
rules: { action: ALLOW, policies: { p: { permissions: [ { metadata: { filter: f, path: [ { key: tier } ], value: { present_match: false } } } ], principals: [ { any: true } ] } } }
"#;
            let cfg: RbacConfig = serde_yaml::from_str(yaml).expect("present_match:false accepted");
            assert_eq!(
                cfg.rules.policies["p"].permissions[0],
                Permission::Metadata(MetadataMatcher {
                    filter: "f".into(),
                    path: vec![MetadataPathSegment { key: "tier".into() }],
                    value: ValueMatcher::PresentMatch(false)
                })
            );
        }

        #[test]
        fn rbac_metadata_rejects_other_value_matcher_keys() {
            // §A5: null_match/bool_match/etc. stay boot-fatal (stricter than Envoy).
            for key in [
                "null_match: {}",
                "bool_match: true",
                "double_match: { exact: 1.0 }",
            ] {
                let yaml = format!(
                    r#"
rules: {{ action: ALLOW, policies: {{ p: {{ permissions: [ {{ metadata: {{ filter: f, path: [ {{ key: tier }} ], value: {{ {key} }} }} }} ], principals: [ {{ any: true }} ] }} }} }}
"#
                );
                serde_yaml::from_str::<RbacConfig>(&yaml)
                    .expect_err("non-string/present value rejected");
            }
        }

        #[test]
        fn rbac_metadata_rejects_unknown_field() {
            let yaml = r#"
rules:
  action: ALLOW
  policies:
    tier_prod:
      permissions:
        - metadata:
            filter: envoy.filters.http.header_to_metadata
            path:
              - key: tier
            value:
              string_match: { exact: "prod" }
            invert: true
      principals:
        - any: true
"#;
            serde_yaml::from_str::<RbacConfig>(yaml)
                .expect_err("unknown `invert` field rejected (deny_unknown_fields)");
        }

        fn ok_cfg() -> RbacConfig {
            let mut policies = BTreeMap::new();
            policies.insert(
                "p".to_string(),
                Policy {
                    permissions: vec![Permission::Any(true)],
                    principals: vec![Principal::Any(true)],
                },
            );
            RbacConfig {
                rules: Rules {
                    action: Action::Allow,
                    policies,
                },
            }
        }

        fn make_filter(cfg: RbacConfig) -> HttpFilter {
            HttpFilter {
                name: "envoy.filters.http.rbac".to_string(),
                typed_config: HttpFilterTypedConfig::Rbac(cfg),
            }
        }

        #[test]
        fn validate_accepts_rbac_followed_by_router() {
            let filters = vec![make_filter(ok_cfg()), router_filter()];
            validate_http_filters(&filters, "ingress_http").expect("valid chain");
        }

        #[test]
        fn validate_rejects_empty_policies() {
            let mut cfg = ok_cfg();
            cfg.rules.policies.clear();
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::EmptyRbacPolicies { ref listener } if listener == "ingress_http"
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_empty_policy_permissions() {
            let mut cfg = ok_cfg();
            cfg.rules.policies.get_mut("p").unwrap().permissions.clear();
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::EmptyRbacPolicyPermissions { ref policy_name, .. }
                        if policy_name == "p"
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_empty_policy_principals() {
            let mut cfg = ok_cfg();
            cfg.rules.policies.get_mut("p").unwrap().principals.clear();
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::EmptyRbacPolicyPrincipals { ref policy_name, .. }
                        if policy_name == "p"
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_empty_permission_set() {
            let mut cfg = ok_cfg();
            cfg.rules.policies.get_mut("p").unwrap().permissions =
                vec![Permission::AndRules(PermissionSet { rules: vec![] })];
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(err, ConfigError::EmptyRbacPermissionSet { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_empty_principal_set() {
            let mut cfg = ok_cfg();
            cfg.rules.policies.get_mut("p").unwrap().principals =
                vec![Principal::OrIds(PrincipalSet { ids: vec![] })];
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(err, ConfigError::EmptyRbacPrincipalSet { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_tree_too_deep() {
            // Build a Permission::NotRule chain of depth RBAC_TREE_MAX_DEPTH + 1.
            let mut perm = Permission::Any(true);
            for _ in 0..=RBAC_TREE_MAX_DEPTH {
                perm = Permission::NotRule(Box::new(perm));
            }
            let mut cfg = ok_cfg();
            cfg.rules.policies.get_mut("p").unwrap().permissions = vec![perm];
            let filters = vec![make_filter(cfg), router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(err, ConfigError::RbacTreeTooDeep { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_rbac_with_wrong_name() {
            let mut filter = make_filter(ok_cfg());
            filter.name = "envoy.filters.http.something_else".to_string();
            let filters = vec![filter, router_filter()];
            let err = validate_http_filters(&filters, "ingress_http").unwrap_err();
            assert!(
                matches!(err, ConfigError::UnsupportedHttpFilter { .. }),
                "err: {err:?}"
            );
        }

        // ---------------------------------------------------------------------
        // phase 35 T2: RBAC `metadata` matcher validation. These drive the full
        // `parse_bootstrap` entry-point (broader than the sibling
        // EmptyRbacPermissionSet / RbacTreeTooDeep tests above, which call
        // `validate_http_filters` on a hand-built `cfg` directly). `{rule}` is
        // the per-test rule block (a `permissions:`/`principals:` pair) spliced
        // into a minimal HCM + rbac + router bootstrap.
        // ---------------------------------------------------------------------
        fn rbac_metadata_bootstrap(rule: &str) -> String {
            format!(
                r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.rbac
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.rbac.v3.RBAC
                      rules:
                        action: ALLOW
                        policies:
                          p:
{rule}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 0 }}
"#
            )
        }

        #[test]
        fn rbac_metadata_empty_filter_is_fatal() {
            let rule = r#"                            permissions:
                              - metadata:
                                  filter: ""
                                  path:
                                    - key: tier
                                  value:
                                    string_match: { exact: "prod" }
                            principals:
                              - any: true"#;
            let err = crate::parse_bootstrap(&rbac_metadata_bootstrap(rule))
                .expect_err("empty filter must reject");
            assert!(
                matches!(err, ConfigError::RbacMetadataMatcherInvalid { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn rbac_metadata_multi_segment_path_is_fatal() {
            let rule = r#"                            permissions:
                              - metadata:
                                  filter: envoy.filters.http.header_to_metadata
                                  path:
                                    - key: tier
                                    - key: sub
                                  value:
                                    string_match: { exact: "prod" }
                            principals:
                              - any: true"#;
            let err = crate::parse_bootstrap(&rbac_metadata_bootstrap(rule))
                .expect_err("multi-segment path must reject");
            assert!(
                matches!(err, ConfigError::RbacMetadataMatcherInvalid { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn rbac_metadata_empty_path_is_fatal() {
            let rule = r#"                            permissions:
                              - metadata:
                                  filter: envoy.filters.http.header_to_metadata
                                  path: []
                                  value:
                                    string_match: { exact: "prod" }
                            principals:
                              - any: true"#;
            let err = crate::parse_bootstrap(&rbac_metadata_bootstrap(rule))
                .expect_err("empty path must reject");
            assert!(
                matches!(err, ConfigError::RbacMetadataMatcherInvalid { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn rbac_metadata_principal_empty_filter_is_fatal() {
            let rule = r#"                            permissions:
                              - any: true
                            principals:
                              - metadata:
                                  filter: ""
                                  path:
                                    - key: tier
                                  value:
                                    string_match: { exact: "prod" }"#;
            let err = crate::parse_bootstrap(&rbac_metadata_bootstrap(rule))
                .expect_err("empty filter under principals must reject");
            assert!(
                matches!(err, ConfigError::RbacMetadataMatcherInvalid { .. }),
                "err: {err:?}"
            );
        }

        #[test]
        fn rbac_metadata_valid_single_segment_ok() {
            let rule = r#"                            permissions:
                              - metadata:
                                  filter: envoy.filters.http.header_to_metadata
                                  path:
                                    - key: tier
                                  value:
                                    string_match: { exact: "prod" }
                            principals:
                              - any: true"#;
            crate::parse_bootstrap(&rbac_metadata_bootstrap(rule))
                .expect("well-formed single-segment metadata matcher is valid");
        }

        #[test]
        fn rbac_metadata_value_safe_regex_parse_accepted_and_compilable() {
            // F2 (phase 36, §A4): a SafeRegex value on an RBAC `metadata` matcher is
            // parse-accepted AND now compilable via the public `ValueMatcher::compile_safe_regexes`
            // helper (the RBAC lowering path, Task 4, calls this at startup so a runtime match
            // no longer panics — the M35-1 limitation is closed). Anchored pattern per §A3b.
            let rule = r#"                            permissions:
                              - metadata:
                                  filter: envoy.filters.http.header_to_metadata
                                  path:
                                    - key: tier
                                  value:
                                    string_match:
                                      safe_regex: { regex: "^(prod|staging)$" }
                            principals:
                              - any: true"#;
            crate::parse_bootstrap(&rbac_metadata_bootstrap(rule))
                .expect("single-segment SafeRegex-value metadata matcher is parse-accepted");
            // Also verify: parsing a ValueMatcher directly and compiling it via the new helper.
            let value_yaml = r#"string_match:
  safe_regex: { regex: "^(prod|staging)$" }"#;
            let mut value: ValueMatcher =
                serde_yaml::from_str(value_yaml).expect("ValueMatcher parses");
            value.compile_safe_regexes().expect("compiles");
            assert!(
                matches!(&value, ValueMatcher::StringMatch(sm)
                    if matches!(&sm.mode, StringMatcherMode::SafeRegex(sr) if sr.compiled.is_some())),
                "compiled SafeRegex should be Some after compile_safe_regexes"
            );
        }
    }

    mod fault_tests {
        use super::*;
        use crate::{
            ConfigError, DenominatorType, FaultAbort, FaultConfig, FractionalPercent, HttpFilter,
            HttpFilterTypedConfig,
        };

        // ── schema deserialization ─────────────────────────────────────────

        #[test]
        fn fault_config_parses_full_abort_with_header_gate() {
            let yaml = r#"
abort:
  http_status: 503
  percentage: { numerator: 100, denominator: HUNDRED }
headers:
- name: x-fault
  string_match: { exact: abort }
"#;
            let cfg: FaultConfig = serde_yaml::from_str(yaml).expect("parses");
            assert_eq!(cfg.abort.http_status, 503);
            assert_eq!(cfg.abort.percentage.numerator, 100);
            assert_eq!(cfg.abort.percentage.denominator, DenominatorType::Hundred);
            assert_eq!(cfg.headers.len(), 1);
        }

        #[test]
        fn fault_config_denominator_defaults_to_hundred() {
            let yaml = r#"
abort:
  http_status: 503
  percentage: { numerator: 0 }
"#;
            let cfg: FaultConfig = serde_yaml::from_str(yaml).expect("parses");
            assert_eq!(cfg.abort.percentage.denominator, DenominatorType::Hundred);
            assert!(cfg.headers.is_empty());
        }

        #[test]
        fn fault_config_rejects_unknown_field() {
            let yaml = r#"
abort:
  http_status: 503
  percentage: { numerator: 100 }
delay: { fixed_delay: 5s }
"#;
            let err = serde_yaml::from_str::<FaultConfig>(yaml).unwrap_err();
            assert!(format!("{err}").contains("delay"), "err: {err}");
        }

        #[test]
        fn denominator_type_value_maps_correctly() {
            assert_eq!(DenominatorType::Hundred.value(), 100);
            assert_eq!(DenominatorType::TenThousand.value(), 10_000);
            assert_eq!(DenominatorType::Million.value(), 1_000_000);
        }

        #[test]
        fn fractional_percent_selects_deterministic() {
            let p100 = FractionalPercent {
                numerator: 100,
                denominator: DenominatorType::Hundred,
            };
            let p0 = FractionalPercent {
                numerator: 0,
                denominator: DenominatorType::Hundred,
            };
            let p_full_million = FractionalPercent {
                numerator: 1_000_000,
                denominator: DenominatorType::Million,
            };
            assert!(p100.selects_deterministic());
            assert!(!p0.selects_deterministic());
            assert!(p_full_million.selects_deterministic());
        }

        // ── validator: positive ────────────────────────────────────────────

        #[test]
        fn validate_accepts_fault_abort_100_percent() {
            let fault = HttpFilter {
                name: "envoy.filters.http.fault".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 503,
                        percentage: FractionalPercent {
                            numerator: 100,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            };
            assert!(validate_http_filters(&[fault, router_filter()], "ingress").is_ok());
        }

        #[test]
        fn validate_accepts_fault_abort_0_percent() {
            let fault = HttpFilter {
                name: "envoy.filters.http.fault".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 503,
                        percentage: FractionalPercent {
                            numerator: 0,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            };
            assert!(validate_http_filters(&[fault, router_filter()], "ingress").is_ok());
        }

        // ── validator: negative ────────────────────────────────────────────

        #[test]
        fn validate_rejects_invalid_abort_status() {
            let fault = HttpFilter {
                name: "envoy.filters.http.fault".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 999,
                        percentage: FractionalPercent {
                            numerator: 100,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            };
            let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::InvalidFaultAbortStatus { status: 999, .. }
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_percentage_out_of_range() {
            let fault = HttpFilter {
                name: "envoy.filters.http.fault".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 503,
                        percentage: FractionalPercent {
                            numerator: 200,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            };
            let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::FaultPercentageOutOfRange {
                        numerator: 200,
                        denominator: 100,
                        ..
                    }
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_fractional_percentage() {
            let fault = HttpFilter {
                name: "envoy.filters.http.fault".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 503,
                        percentage: FractionalPercent {
                            numerator: 50,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            };
            let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
            assert!(
                matches!(
                    err,
                    ConfigError::UnsupportedFractionalFaultPercentage {
                        numerator: 50,
                        denominator: 100,
                        ..
                    }
                ),
                "err: {err:?}"
            );
        }

        #[test]
        fn validate_rejects_name_typed_config_mismatch() {
            let fault = HttpFilter {
                name: "envoy.filters.http.WRONG".to_string(),
                typed_config: HttpFilterTypedConfig::Fault(FaultConfig {
                    abort: FaultAbort {
                        http_status: 503,
                        percentage: FractionalPercent {
                            numerator: 100,
                            denominator: DenominatorType::Hundred,
                        },
                    },
                    headers: vec![],
                }),
            };
            let err = validate_http_filters(&[fault, router_filter()], "ingress").unwrap_err();
            assert!(
                matches!(err, ConfigError::UnsupportedHttpFilter { .. }),
                "err: {err:?}"
            );
        }
    }

    // ── 12.1 Task 1: D1 health-check schema tests ────────────────────────────

    #[test]
    fn parses_cluster_with_http_health_check_and_panic_threshold() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      common_lb_config:
        healthy_panic_threshold: { value: 0 }
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 2
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 200, end: 201 }
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: localhost, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("valid");
        let cluster = &bootstrap.static_resources.clusters[0];
        assert_eq!(cluster.health_checks.len(), 1);
        let hc = &cluster.health_checks[0];
        assert_eq!(hc.timeout, "1s");
        assert_eq!(hc.interval, "1s");
        assert_eq!(hc.healthy_threshold, 1);
        assert_eq!(hc.unhealthy_threshold, 2);
        let http = hc.http_health_check.as_ref().expect("http checker present");
        assert_eq!(http.path, "/healthz");
        assert_eq!(
            http.expected_statuses,
            vec![crate::Int64Range {
                start: 200,
                end: 201
            }]
        );
        assert!(http.host.is_none());
        let clb = cluster
            .common_lb_config
            .as_ref()
            .expect("common_lb_config present");
        assert_eq!(clb.healthy_panic_threshold.as_ref().unwrap().value, 0.0);
    }

    #[test]
    fn cluster_without_health_checks_defaults_to_empty_vec_and_none() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("valid");
        let cluster = &bootstrap.static_resources.clusters[0];
        assert!(cluster.health_checks.is_empty());
        assert!(cluster.common_lb_config.is_none());
    }

    // -----------------------------------------------------------------------
    // 68: HealthCheckPayload hex/base64 decode
    // -----------------------------------------------------------------------

    #[test]
    fn payload_decode_hex_text_ok() {
        let p = HealthCheckPayload {
            text: Some("50494e47".to_string()),
            binary: None,
        };
        assert_eq!(p.decode().unwrap(), b"PING");
    }

    #[test]
    fn payload_decode_odd_length_hex_is_err() {
        let p = HealthCheckPayload {
            text: Some("0".to_string()),
            binary: None,
        };
        assert!(matches!(p.decode(), Err(PayloadDecodeError::InvalidHex(ref s)) if s == "0"));
    }

    #[test]
    fn payload_decode_non_hex_is_err() {
        let p = HealthCheckPayload {
            text: Some("zzzz".to_string()),
            binary: None,
        };
        assert!(matches!(p.decode(), Err(PayloadDecodeError::InvalidHex(ref s)) if s == "zzzz"));
    }

    #[test]
    fn payload_decode_base64_binary_ok() {
        let p = HealthCheckPayload {
            text: None,
            binary: Some("AAECAw==".to_string()),
        };
        assert_eq!(p.decode().unwrap(), vec![0u8, 1, 2, 3]);
    }

    #[test]
    fn payload_decode_bad_base64_is_err() {
        let p = HealthCheckPayload {
            text: None,
            binary: Some("!!!!".to_string()),
        };
        assert!(matches!(
            p.decode(),
            Err(PayloadDecodeError::InvalidBase64(_))
        ));
    }

    #[test]
    fn payload_decode_empty_is_err() {
        let p = HealthCheckPayload {
            text: None,
            binary: None,
        };
        assert!(matches!(p.decode(), Err(PayloadDecodeError::Empty)));
        let both = HealthCheckPayload {
            text: Some("00".to_string()),
            binary: Some("AA==".to_string()),
        };
        assert!(matches!(both.decode(), Err(PayloadDecodeError::Empty)));
    }

    // -----------------------------------------------------------------------
    // 68: TcpHealthCheck parse
    // -----------------------------------------------------------------------

    #[test]
    fn parses_empty_tcp_health_check_connection_only() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: {}",
            "",
        );
        let bs = crate::parse_bootstrap(&yaml).expect("empty tcp_health_check parses");
        let hc = &bs.static_resources.clusters[0].health_checks[0];
        let tcp = hc.tcp_health_check.as_ref().expect("tcp checker present");
        assert!(tcp.send.is_none());
        assert!(tcp.receive.is_empty());
        assert!(hc.http_health_check.is_none());
    }

    #[test]
    fn parses_tcp_health_check_send_receive() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check:\n            send: { text: \"000102\" }\n            receive:\n              - { text: \"0304\" }",
            "",
        );
        let bs = crate::parse_bootstrap(&yaml).expect("send/receive tcp_health_check parses");
        let tcp = bs.static_resources.clusters[0].health_checks[0]
            .tcp_health_check
            .as_ref()
            .unwrap();
        assert_eq!(tcp.send.as_ref().unwrap().text.as_deref(), Some("000102"));
        assert_eq!(tcp.receive.len(), 1);
        assert_eq!(tcp.receive[0].text.as_deref(), Some("0304"));
    }

    #[test]
    fn tcp_health_check_rejects_unknown_field() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { bogus: 1 }",
            "",
        );
        assert!(
            crate::parse_bootstrap(&yaml).is_err(),
            "deny_unknown_fields rejects unknown tcp_health_check key"
        );
    }

    // -----------------------------------------------------------------------
    // 69: GrpcHealthCheck parse
    // -----------------------------------------------------------------------

    #[test]
    fn parses_grpc_health_check_with_fields() {
        // gRPC checker on an H2-upstream cluster; all three fields set.
        // NOTE: HeaderValueOption.append_action has no schema default
        // (verified in-tree: every other call site sets it explicitly), so
        // unlike the brief's literal snippet this YAML must set it too.
        let yaml = cluster_yaml_with_h2_and_health_check(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check:\n            service_name: my.svc\n            authority: hc.example.com\n            initial_metadata:\n              - header: { key: x-hc, value: \"1\" }\n                append_action: APPEND_IF_EXISTS_OR_ADD",
        );
        let bs = crate::parse_bootstrap(&yaml).expect("grpc_health_check parses");
        let hc = bs.static_resources.clusters[0]
            .health_checks
            .first()
            .unwrap();
        let grpc = hc.grpc_health_check.as_ref().expect("grpc checker present");
        assert_eq!(grpc.service_name, "my.svc");
        assert_eq!(grpc.authority, "hc.example.com");
        assert_eq!(grpc.initial_metadata.len(), 1);
        assert!(hc.http_health_check.is_none());
        assert!(hc.tcp_health_check.is_none());
    }

    #[test]
    fn parses_empty_grpc_health_check() {
        let yaml = cluster_yaml_with_h2_and_health_check(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: {}",
        );
        let bs = crate::parse_bootstrap(&yaml).expect("empty grpc_health_check parses");
        let grpc = bs.static_resources.clusters[0].health_checks[0]
            .grpc_health_check
            .as_ref()
            .unwrap();
        assert_eq!(grpc.service_name, ""); // empty ⇒ overall server
    }

    #[test]
    fn grpc_health_check_rejects_unknown_field() {
        let yaml = cluster_yaml_with_h2_and_health_check(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: { bogus: 1 }",
        );
        assert!(
            crate::parse_bootstrap(&yaml).is_err(),
            "deny_unknown_fields rejects unknown grpc key"
        );
    }

    #[test]
    fn cluster_rejects_unknown_health_check_field() {
        // deny_unknown_fields rejects custom checkers + deferred upstream knobs
        // (TCP is supported since phase 68; gRPC is supported since phase 69,
        // gated on H2 — see cluster_yaml_with_h2_and_health_check tests).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          custom_health_check: {}
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        assert!(crate::parse_bootstrap(yaml).is_err());
    }

    // -----------------------------------------------------------------------
    // 12.1 Task 2: validate_health_checks tests
    // -----------------------------------------------------------------------

    /// Helper: build a single-cluster bootstrap YAML wrapping a `health_checks:` block.
    fn hc_yaml(health_checks_block: &str, common_lb_config_block: &str) -> String {
        format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
{common_lb_config_block}
{health_checks_block}
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: localhost, port_value: 7000 }} }}
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: 9901 }}
"#
        )
    }

    /// 69: like `hc_yaml`, but the cluster is STATIC and carries H2
    /// `typed_extension_protocol_options` (gRPC health checking requires the
    /// upstream to support HTTP/2 — ADR-0139).
    fn cluster_yaml_with_h2_and_health_check(health_checks_block: &str) -> String {
        format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
{health_checks_block}
      typed_extension_protocol_options:
        envoy.extensions.upstreams.http.v3.HttpProtocolOptions:
          "@type": type.googleapis.com/envoy.extensions.upstreams.http.v3.HttpProtocolOptions
          explicit_http_config:
            http2_protocol_options: {{}}
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: 7000 }} }}
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: 9901 }}
"#
        )
    }

    /// 69: like `hc_yaml`, but the cluster carries NO
    /// `typed_extension_protocol_options` (non-H2), for negative
    /// `grpc_health_check` validator tests.
    fn cluster_yaml_non_h2_with_health_check(health_checks_block: &str) -> String {
        format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: hc_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
{health_checks_block}
      load_assignment:
        cluster_name: hc_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: 7000 }} }}
admin:
  address:
    socket_address: {{ address: 127.0.0.1, port_value: 9901 }}
"#
        )
    }

    const VALID_HC: &str = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz"#;

    #[test]
    fn validate_accepts_well_formed_health_check() {
        assert!(crate::parse_bootstrap(&hc_yaml(VALID_HC, "")).is_ok());
    }

    #[test]
    fn validate_rejects_multiple_health_checks() {
        let two = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz2 }"#;
        let err = crate::parse_bootstrap(&hc_yaml(two, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnsupportedMultipleHealthChecks { ref cluster } if cluster == "hc_backend"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_missing_http_checker() {
        let no_http = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1"#;
        let err = crate::parse_bootstrap(&hc_yaml(no_http, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnsupportedHealthCheckType { ref cluster } if cluster == "hc_backend"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_zero_threshold() {
        let zero = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 0
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }"#;
        let err = crate::parse_bootstrap(&hc_yaml(zero, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::InvalidHealthCheckThreshold { ref cluster, field } if cluster == "hc_backend" && field == "healthy_threshold"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_zero_unhealthy_threshold() {
        let zero = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 0
          http_health_check: { path: /healthz }"#;
        let err = crate::parse_bootstrap(&hc_yaml(zero, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::InvalidHealthCheckThreshold { ref cluster, field } if cluster == "hc_backend" && field == "unhealthy_threshold"),
            "got {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // 68: validate_health_checks TCP arm
    // -----------------------------------------------------------------------

    #[test]
    fn validate_accepts_tcp_only_checker() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { receive: [ { text: \"50494e47\" } ] }",
            "",
        );
        assert!(
            crate::parse_bootstrap(&yaml).is_ok(),
            "tcp-only checker validates"
        );
    }

    #[test]
    fn validate_rejects_both_http_and_tcp() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          http_health_check: { path: /z }\n          tcp_health_check: {}",
            "",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("both checkers rejected");
        assert!(
            matches!(err, crate::ConfigError::MultipleHealthCheckers { ref cluster } if cluster == "hc_backend"),
            "got {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // 69: validate_health_checks gRPC arm
    // -----------------------------------------------------------------------

    #[test]
    fn validate_rejects_grpc_on_non_h2_cluster() {
        // STATIC (non-H2) cluster + grpc_health_check ⇒ GrpcHealthCheckRequiresHttp2.
        let yaml = cluster_yaml_non_h2_with_health_check(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: {}",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("grpc on non-H2 is fatal");
        assert!(
            matches!(err, crate::ConfigError::GrpcHealthCheckRequiresHttp2 { ref cluster } if cluster == "hc_backend"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_grpc_on_h2_cluster() {
        let yaml = cluster_yaml_with_h2_and_health_check(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          grpc_health_check: {}",
        );
        assert!(
            crate::parse_bootstrap(&yaml).is_ok(),
            "grpc on H2 cluster validates OK"
        );
    }

    #[test]
    fn validate_rejects_multiple_health_checkers() {
        let yaml = cluster_yaml_with_h2_and_health_check(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          http_health_check: { path: /z }\n          grpc_health_check: {}",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("two checkers is fatal");
        assert!(
            matches!(err, crate::ConfigError::MultipleHealthCheckers { ref cluster } if cluster == "hc_backend"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_tcp_payload_bad_hex() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { send: { text: \"zzzz\" } }",
            "",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("bad hex rejected");
        assert!(
            matches!(err, crate::ConfigError::InvalidHealthCheckPayloadHex { ref cluster, ref value } if cluster == "hc_backend" && value == "zzzz"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_tcp_payload_empty() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 1\n          unhealthy_threshold: 2\n          tcp_health_check: { receive: [ {} ] }",
            "",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("empty payload rejected");
        assert!(
            matches!(err, crate::ConfigError::EmptyHealthCheckPayload { ref cluster } if cluster == "hc_backend"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_tcp_bad_threshold() {
        let yaml = hc_yaml(
            "      health_checks:\n        - timeout: 1s\n          interval: 1s\n          healthy_threshold: 0\n          unhealthy_threshold: 2\n          tcp_health_check: {}",
            "",
        );
        let err = crate::parse_bootstrap(&yaml).expect_err("threshold validated for tcp too");
        assert!(
            matches!(err, crate::ConfigError::InvalidHealthCheckThreshold { ref cluster, field } if cluster == "hc_backend" && field == "healthy_threshold"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_subsecond_decimal_duration() {
        // §6.2 item-6: parse_duration rejects "0.5s" → surfaces as InvalidHealthCheckTiming.
        let half = r#"      health_checks:
        - timeout: 0.5s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }"#;
        let err = crate::parse_bootstrap(&hc_yaml(half, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::InvalidHealthCheckTiming { ref cluster, field } if cluster == "hc_backend" && field == "timeout"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_zero_duration() {
        let zero = r#"      health_checks:
        - timeout: 0s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }"#;
        let err = crate::parse_bootstrap(&hc_yaml(zero, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::InvalidHealthCheckTiming { ref cluster, field } if cluster == "hc_backend" && field == "timeout"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_zero_interval_duration() {
        let zero = r#"      health_checks:
        - timeout: 1s
          interval: 0s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: /healthz }"#;
        let err = crate::parse_bootstrap(&hc_yaml(zero, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::InvalidHealthCheckTiming { ref cluster, field } if cluster == "hc_backend" && field == "interval"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_empty_path() {
        let empty = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check: { path: "" }"#;
        let err = crate::parse_bootstrap(&hc_yaml(empty, "")).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::EmptyHealthCheckPath { ref cluster } if cluster == "hc_backend"),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_inverted_expected_status_range() {
        let bad = r#"      health_checks:
        - timeout: 1s
          interval: 1s
          healthy_threshold: 1
          unhealthy_threshold: 1
          http_health_check:
            path: /healthz
            expected_statuses:
              - { start: 300, end: 200 }"#;
        let err = crate::parse_bootstrap(&hc_yaml(bad, "")).unwrap_err();
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidInt64Range {
                    start: 300,
                    end: 200
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_rejects_out_of_range_panic_threshold() {
        let clb = "      common_lb_config:\n        healthy_panic_threshold: { value: 150 }";
        let err = crate::parse_bootstrap(&hc_yaml(VALID_HC, clb)).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::InvalidPanicThreshold { ref cluster, value } if cluster == "hc_backend" && value == 150.0),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_accepts_zero_panic_threshold() {
        let clb = "      common_lb_config:\n        healthy_panic_threshold: { value: 0 }";
        assert!(crate::parse_bootstrap(&hc_yaml(VALID_HC, clb)).is_ok());
    }

    // --- 13.1 D1: Cluster.circuit_breakers schema ---

    #[test]
    fn cluster_circuit_breakers_parses_minimal_shape() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: pooled
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
      load_assignment:
        cluster_name: pooled
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse");
        let cluster = &bootstrap.static_resources.clusters[0];
        let cb = cluster
            .circuit_breakers
            .as_ref()
            .expect("circuit_breakers present");
        assert_eq!(cb.thresholds.len(), 1);
        assert_eq!(
            cb.thresholds[0].priority,
            Some(crate::RoutingPriority::Default)
        );
        assert_eq!(cb.thresholds[0].max_connections, Some(4));
    }

    #[test]
    fn cluster_circuit_breakers_omitted_yields_none() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse");
        assert!(
            bootstrap.static_resources.clusters[0]
                .circuit_breakers
                .is_none()
        );
    }

    // ---- 62 D1 (ADR-0119): common_http_protocol_options.idle_timeout ----

    #[test]
    fn cluster_idle_timeout_parses_and_round_trips() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: pooled
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_http_protocol_options:
        idle_timeout: 30s
      load_assignment:
        cluster_name: pooled
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse");
        let opts = bootstrap.static_resources.clusters[0]
            .common_http_protocol_options
            .as_ref()
            .expect("common_http_protocol_options present");
        assert_eq!(opts.idle_timeout.as_deref(), Some("30s"));
        // The stored scalar parses to the intended Duration at pool-build time.
        assert_eq!(
            parse_duration(opts.idle_timeout.as_deref().unwrap()),
            Ok(std::time::Duration::from_secs(30))
        );
    }

    #[test]
    fn cluster_idle_timeout_omitted_yields_none() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("parse");
        assert!(
            bootstrap.static_resources.clusters[0]
                .common_http_protocol_options
                .is_none()
        );
    }

    #[test]
    fn cluster_idle_timeout_rejects_malformed() {
        // Fail-closed: a non-parse_duration scalar is rejected at parse time
        // rather than silently defaulting to 60s (ADR-0119).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      common_http_protocol_options:
        idle_timeout: banana
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        match crate::parse_bootstrap(yaml) {
            Err(crate::ConfigError::InvalidClusterIdleTimeout { cluster }) => {
                assert_eq!(cluster, "c");
            }
            other => panic!("expected InvalidClusterIdleTimeout, got {other:?}"),
        }
    }

    #[test]
    fn cluster_circuit_breakers_rejects_still_deferred_threshold_fields() {
        // deny_unknown_fields rejects still-deferred fields (max_connection_pools, retry_budget).
        // NOTE: max_requests/max_retries/track_remaining were promoted in phase-17 D1 and
        // are now accepted — this test uses max_connection_pools which remains deferred.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_connection_pools: 5
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("max_connection_pools") || msg.contains("unknown field"),
            "expected unknown-field error mentioning max_connection_pools; got: {msg}"
        );
    }

    #[test]
    fn cluster_max_pending_requests_zero_accepted() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_pending_requests: 0
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap =
            crate::parse_bootstrap(yaml).expect("max_pending_requests:0 must parse+validate");
        assert_eq!(
            bootstrap.static_resources.clusters[0]
                .circuit_breakers
                .as_ref()
                .unwrap()
                .thresholds[0]
                .max_pending_requests,
            Some(0)
        );
    }

    #[test]
    fn cluster_max_pending_requests_positive_rejected_by_validator() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_pending_requests: 5
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 8080 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err =
            crate::parse_bootstrap(yaml).expect_err("max_pending_requests>0 must be rejected");
        let msg = format!("{err:#}");
        assert!(msg.contains("max_pending_requests"), "got: {msg}");
    }

    // --- 13.1 D2: validate_circuit_breakers ---

    #[test]
    fn validate_circuit_breakers_accepts_minimal() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 4
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        crate::parse_bootstrap(yaml).expect("parses and validates");
    }

    #[test]
    fn validate_circuit_breakers_rejects_multiple_thresholds() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
          - max_connections: 2
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedMultipleCircuitBreakerThresholds { ref cluster }
                    if cluster == "c"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_circuit_breakers_rejects_high_priority() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: HIGH
            max_connections: 1
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedCircuitBreakerPriority { ref cluster, priority: crate::RoutingPriority::High }
                    if cluster == "c"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn validate_circuit_breakers_rejects_zero_max_connections() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 0
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidMaxConnections { ref cluster, value: 0 }
                    if cluster == "c"
            ),
            "got {err:?}"
        );
    }

    // --- 17 D1: Thresholds budget fields (max_requests / max_retries / track_remaining) ---

    #[test]
    fn thresholds_parse_budget_fields() {
        // (a) all three new fields present and populated
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_requests: 0
            max_retries: 0
            track_remaining: true
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("must parse+validate");
        let t = &bootstrap.static_resources.clusters[0]
            .circuit_breakers
            .as_ref()
            .unwrap()
            .thresholds[0];
        assert_eq!(t.max_requests, Some(0));
        assert_eq!(t.max_retries, Some(0));
        assert_eq!(t.track_remaining, Some(true));
    }

    #[test]
    fn thresholds_parse_budget_fields_nonzero_and_false() {
        // (a2) non-zero caps + track_remaining: false
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_requests: 5
            max_retries: 3
            track_remaining: false
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("must parse+validate");
        let t = &bootstrap.static_resources.clusters[0]
            .circuit_breakers
            .as_ref()
            .unwrap()
            .thresholds[0];
        assert_eq!(t.max_requests, Some(5));
        assert_eq!(t.max_retries, Some(3));
        assert_eq!(t.track_remaining, Some(false));
    }

    #[test]
    fn thresholds_budget_fields_absent_yield_none() {
        // (b) new fields absent → all three None
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("must parse+validate");
        let t = &bootstrap.static_resources.clusters[0]
            .circuit_breakers
            .as_ref()
            .unwrap()
            .thresholds[0];
        assert_eq!(t.max_requests, None);
        assert_eq!(t.max_retries, None);
        assert_eq!(t.track_remaining, None);
    }

    #[test]
    fn thresholds_reject_deferred_retry_budget() {
        // (c) retry_budget stays rejected by deny_unknown_fields
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            retry_budget: { budget_percent: 20 }
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("retry_budget must be rejected");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("retry_budget") || msg.contains("unknown field"),
            "expected unknown-field error mentioning retry_budget; got: {msg}"
        );
    }

    #[test]
    fn thresholds_reject_deferred_max_connection_pools() {
        // (d) max_connection_pools stays rejected by deny_unknown_fields
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connection_pools: 1
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("max_connection_pools must be rejected");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("max_connection_pools") || msg.contains("unknown field"),
            "expected unknown-field error mentioning max_connection_pools; got: {msg}"
        );
    }

    #[test]
    fn validate_circuit_breakers_accepts_zero_budget_caps() {
        // (e) max_requests: 0 and max_retries: 0 are ACCEPTED (always-open-breaker configs;
        // contrast with max_connections: 0 which is REJECTED by InvalidMaxConnections).
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: DEFAULT
            max_connections: 1
            max_requests: 0
            max_retries: 0
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        crate::parse_bootstrap(yaml)
            .expect("max_requests:0 and max_retries:0 must be accepted by the validator");
    }

    #[test]
    fn validate_circuit_breakers_existing_rejections_still_fire_with_budget_fields() {
        // (f) existing rejections remain: multiple thresholds, HIGH priority,
        // max_pending_requests > 0 — verified here with budget fields present to
        // ensure the new fields don't mask pre-existing validator checks.

        // multiple thresholds
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_requests: 5
          - max_connections: 2
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("multiple thresholds must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedMultipleCircuitBreakerThresholds { ref cluster }
                    if cluster == "c"
            ),
            "got {err:?}"
        );

        // HIGH priority
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - priority: HIGH
            max_connections: 1
            max_requests: 5
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("HIGH priority must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedCircuitBreakerPriority {
                    ref cluster,
                    priority: crate::RoutingPriority::High
                } if cluster == "c"
            ),
            "got {err:?}"
        );

        // max_pending_requests > 0
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: c
      type: STATIC
      lb_policy: ROUND_ROBIN
      circuit_breakers:
        thresholds:
          - max_connections: 1
            max_pending_requests: 5
            max_requests: 10
      load_assignment:
        cluster_name: c
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address: { address: 127.0.0.1, port_value: 1 }
admin:
  address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("max_pending_requests > 0 must reject");
        let msg = format!("{err:#}");
        assert!(msg.contains("max_pending_requests"), "got: {msg}");
    }

    #[test]
    fn parses_cluster_with_outlier_detection_minimum_viable() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: od_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection:
        consecutive_5xx: 5
        consecutive_gateway_failure: 5
        interval: 10s
        base_ejection_time: 30s
        max_ejection_percent: 100
      load_assignment:
        cluster_name: od_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("valid");
        let cluster = &bootstrap.static_resources.clusters[0];
        let od = cluster
            .outlier_detection
            .as_ref()
            .expect("outlier_detection present");
        assert_eq!(od.consecutive_5xx, Some(5));
        assert_eq!(od.consecutive_gateway_failure, Some(5));
        assert_eq!(od.interval.as_deref(), Some("10s"));
        assert_eq!(od.base_ejection_time.as_deref(), Some("30s"));
        assert_eq!(od.max_ejection_percent, Some(100));
    }

    #[test]
    fn cluster_without_outlier_detection_defaults_to_none() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: plain_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: plain_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let bootstrap = crate::parse_bootstrap(yaml).expect("valid");
        assert!(
            bootstrap.static_resources.clusters[0]
                .outlier_detection
                .is_none()
        );
    }

    #[test]
    fn outlier_detection_rejects_unknown_fields() {
        // success_rate_minimum_hosts is one of the parent §4 deferred fields rejected
        // by deny_unknown_fields per ADR-0041 §6.2 item-1.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: od_backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection:
        consecutive_5xx: 5
        success_rate_minimum_hosts: 5
      load_assignment:
        cluster_name: od_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin:
  address:
    socket_address: { address: 127.0.0.1, port_value: 9901 }
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(matches!(err, crate::ConfigError::Yaml(_)), "got {err:?}");
    }

    // --- 16.1 D1: RetryPolicy schema + retry_policy field + include_attempt_count_in_response ---

    /// (a) A route YAML with retry_policy round-trips into the expected struct.
    #[test]
    fn route_retry_policy_parses_minimal_shape() {
        let yaml = r#"
cluster: my-cluster
retry_policy:
  retry_on: "5xx"
  num_retries: 1
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(ar.cluster, "my-cluster");
        let rp = ar.retry_policy.as_ref().expect("retry_policy present");
        assert_eq!(rp.retry_on, "5xx");
        assert_eq!(rp.num_retries, Some(1));
        assert_eq!(rp.retriable_status_codes, Vec::<u32>::new());
    }

    /// (b) A route with NO retry_policy → retry_policy: None.
    #[test]
    fn route_retry_policy_absent_yields_none() {
        let yaml = r#"
cluster: my-cluster
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses");
        assert!(ar.retry_policy.is_none());
    }

    /// (c) retry_policy with a deferred field per_try_timeout → parse ERROR (deny_unknown_fields).
    #[test]
    fn route_retry_policy_rejects_deferred_field_per_try_timeout() {
        let yaml = r#"
cluster: my-cluster
retry_policy:
  retry_on: "5xx"
  per_try_timeout: 1s
"#;
        let err = serde_yaml::from_str::<RouteAction_Route>(yaml)
            .expect_err("must reject deferred field");
        let msg = format!("{err}");
        assert!(
            msg.contains("per_try_timeout") || msg.contains("unknown field"),
            "expected unknown-field error mentioning per_try_timeout; got: {msg}"
        );
    }

    // --- 28 Task 4: route-level hash_policy config (parse + validate) ---

    /// (a) A route with a single header hash_policy parses into exactly one
    /// HashPolicy carrying the header source and the expected header_name.
    #[test]
    fn route_hash_policy_parses_header_source() {
        let yaml = r#"
cluster: my-cluster
hash_policy:
  - header:
      header_name: x-hash-key
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(ar.cluster, "my-cluster");
        assert_eq!(ar.hash_policy.len(), 1);
        let hp = &ar.hash_policy[0];
        let header = hp.header.as_ref().expect("header specifier present");
        assert_eq!(header.header_name, "x-hash-key");
        // The validator accepts a header-only policy.
        validate_hash_policy(hp).expect("header policy is valid");
    }

    /// (b) A route with NO hash_policy yields an empty Vec (regression-equivalence
    /// default — every pre-existing route parses unchanged).
    #[test]
    fn route_hash_policy_absent_yields_empty_vec() {
        let yaml = r#"
cluster: my-cluster
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses");
        assert!(ar.hash_policy.is_empty());
    }

    /// (a') Multiple header hash policies collect into a Vec of len 2 — confirms
    /// the repeated field.
    #[test]
    fn route_hash_policy_collects_multiple_headers() {
        let yaml = r#"
cluster: my-cluster
hash_policy:
  - header:
      header_name: x-a
  - header:
      header_name: x-b
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(ar.hash_policy.len(), 2);
        assert_eq!(
            ar.hash_policy[0].header.as_ref().unwrap().header_name,
            "x-a"
        );
        assert_eq!(
            ar.hash_policy[1].header.as_ref().unwrap().header_name,
            "x-b"
        );
    }

    /// (c) An unsupported hash-policy source (cookie) is recognized at parse but
    /// REJECTED by the validator with the precise UnsupportedHashPolicy fatal.
    #[test]
    fn route_hash_policy_rejects_unsupported_cookie_source() {
        let yaml = r#"
cluster: my-cluster
hash_policy:
  - cookie:
      name: foo
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses (recognized key)");
        assert_eq!(ar.hash_policy.len(), 1);
        let err =
            validate_hash_policy(&ar.hash_policy[0]).expect_err("cookie source must be rejected");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedHashPolicy { ref specifier } if specifier == "cookie"
            ),
            "expected UnsupportedHashPolicy(cookie); got {err:?}"
        );
    }

    /// (c') connection_properties source is likewise rejected with the precise variant.
    #[test]
    fn route_hash_policy_rejects_unsupported_connection_properties_source() {
        let yaml = r#"
cluster: my-cluster
hash_policy:
  - connection_properties:
      source_ip: true
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses (recognized key)");
        let err = validate_hash_policy(&ar.hash_policy[0])
            .expect_err("connection_properties source must be rejected");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedHashPolicy { ref specifier }
                    if specifier == "connection_properties"
            ),
            "expected UnsupportedHashPolicy(connection_properties); got {err:?}"
        );
    }

    /// (c'') End-to-end wiring: an unsupported cookie hash_policy on a route in a
    /// COMPLETE bootstrap (HCM + inline route_config) is rejected by the real
    /// `parse_bootstrap` route loop — proving `validate_hcm` actually invokes
    /// `validate_hash_policy` (the new wiring this task added), not merely that
    /// the validator's logic is correct in isolation.
    #[test]
    fn parse_bootstrap_rejects_route_hash_policy_cookie_source() {
        let routes = r#"- match: { prefix: "/" }
                          route:
                            cluster: backend
                            hash_policy:
                              - cookie:
                                  name: foo"#;
        let yaml = route_action_yaml(routes, BACKEND_CLUSTER);
        let err = crate::parse_bootstrap(&yaml).expect_err("cookie hash_policy must be rejected");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedHashPolicy { ref specifier } if specifier == "cookie"
            ),
            "expected UnsupportedHashPolicy(cookie) through full parse+validate; got {err:?}"
        );
    }

    /// (c''') An empty-oneof hash_policy (no specifier set at all) is rejected
    /// with `specifier == "<none>"` — covers the distinct empty-oneof branch of
    /// the validator.
    #[test]
    fn route_hash_policy_rejects_empty_oneof() {
        let hp = HashPolicy::default();
        let err = validate_hash_policy(&hp).expect_err("empty oneof must be rejected");
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedHashPolicy { ref specifier } if specifier == "<none>"
            ),
            "expected UnsupportedHashPolicy(<none>); got {err:?}"
        );
    }

    /// 30 Task 3: a route `metadata_match` with the Envoy `core.v3.Metadata`
    /// nested `filter_metadata."envoy.lb"` wire shape parses into `Some(LbMetadata)`
    /// carrying the namespace map; an absent `metadata_match` yields `None`.
    #[test]
    fn route_metadata_match_parses_envoy_lb_namespace() {
        let yaml = r#"
cluster: backend
metadata_match:
  filter_metadata:
    envoy.lb:
      stage: prod
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(ar.cluster, "backend");
        let md = ar.metadata_match.as_ref().expect("metadata_match present");
        assert_eq!(md.envoy_lb.get("stage").map(String::as_str), Some("prod"));
    }

    /// 30 Task 3: a route with NO `metadata_match` yields `None` (regression-
    /// equivalence default — every pre-existing route parses unchanged).
    #[test]
    fn route_metadata_match_absent_yields_none() {
        let yaml = r#"
cluster: backend
"#;
        let ar: RouteAction_Route = serde_yaml::from_str(yaml).expect("parses");
        assert!(ar.metadata_match.is_none());
    }

    /// (d) A VirtualHost YAML with include_attempt_count_in_response: true → field true;
    ///     absent → false (default).
    #[test]
    fn virtual_host_include_attempt_count_in_response_parses() {
        let yaml_with_flag = r#"
name: vh
domains: ["*"]
routes: []
include_attempt_count_in_response: true
"#;
        let vh: VirtualHost = serde_yaml::from_str(yaml_with_flag).expect("parses");
        assert!(vh.include_attempt_count_in_response);

        let yaml_absent = r#"
name: vh
domains: ["*"]
routes: []
"#;
        let vh2: VirtualHost = serde_yaml::from_str(yaml_absent).expect("parses");
        assert!(!vh2.include_attempt_count_in_response);
    }

    // --- 16.1 D2: RetryConfig + retry_on tokenization (accept-and-ignore) + is_retriable ---

    #[test]
    fn retry_on_parses_known_tokens_and_ignores_unknown() {
        // L2: accept-and-ignore unknown tokens (Envoy-faithful, empirically verified)
        let p = RetryPolicy {
            retry_on: "5xx,bogus-token-xyz".into(),
            num_retries: None,
            retriable_status_codes: vec![],
        };
        let rc = RetryConfig::from(&p);
        assert_eq!(rc.num_retries, 1); // L3: default 1
        assert!(rc.is_retriable(503, AttemptOutcome::Response)); // 5xx matches
        assert!(rc.is_retriable(500, AttemptOutcome::Response)); // 5xx = 500-599 (L1)
        assert!(!rc.is_retriable(404, AttemptOutcome::Response)); // not retriable
    }

    #[test]
    fn gateway_error_is_502_503_504_only() {
        let p = RetryPolicy {
            retry_on: "gateway-error".into(),
            num_retries: Some(2),
            retriable_status_codes: vec![],
        };
        let rc = RetryConfig::from(&p);
        assert_eq!(rc.num_retries, 2);
        assert!(rc.is_retriable(503, AttemptOutcome::Response));
        assert!(!rc.is_retriable(500, AttemptOutcome::Response)); // L1: 500 NOT in gateway-error
    }

    #[test]
    fn connect_failure_and_reset_and_retriable_status_codes() {
        let p = RetryPolicy {
            retry_on: "connect-failure,reset,retriable-status-codes".into(),
            num_retries: Some(1),
            retriable_status_codes: vec![409],
        };
        let rc = RetryConfig::from(&p);
        assert!(rc.is_retriable(0, AttemptOutcome::ConnectFailure));
        assert!(rc.is_retriable(0, AttemptOutcome::Reset));
        assert!(rc.is_retriable(409, AttemptOutcome::Response)); // retriable_status_codes
        assert!(!rc.is_retriable(503, AttemptOutcome::Response)); // 5xx token NOT present
    }

    #[test]
    fn backoff_exponential_base_25ms_cap_250ms() {
        use std::time::Duration;
        assert_eq!(RetryConfig::backoff(0), None);
        assert_eq!(RetryConfig::backoff(1), Some(Duration::from_millis(25)));
        assert_eq!(RetryConfig::backoff(2), Some(Duration::from_millis(50)));
        assert_eq!(RetryConfig::backoff(3), Some(Duration::from_millis(100)));
        assert_eq!(RetryConfig::backoff(4), Some(Duration::from_millis(200)));
        // attempt 5 would be 400ms → capped to 250ms.
        assert_eq!(RetryConfig::backoff(5), Some(Duration::from_millis(250)));
        assert_eq!(RetryConfig::backoff(100), Some(Duration::from_millis(250)));
    }

    // --- 19 D1 (ADR-0050): file-based LDS schema + validator-gate migration ---

    /// One minimal static echo listener (satisfies NoRuntime; one allowed by the
    /// single-listener limitation).
    const ONE_STATIC_LISTENER: &str = r#"
static_resources:
  listeners:
    - name: a
      address: { socket_address: { address: 0.0.0.0, port_value: 1 } }
      filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
"#;

    /// Parse a standalone Listener YAML (test convenience — no listener helper exists).
    fn parse_listener(yaml: &str) -> Listener {
        serde_yaml::from_str(yaml).expect("listener parses")
    }

    #[test]
    fn listener_enable_reuse_port_defaults_true_when_absent() {
        // A bootstrap that omits enable_reuse_port keeps Envoy's default (true)
        // without tripping deny_unknown_fields.
        let l = parse_listener(
            r#"
name: a
address: { socket_address: { address: 0.0.0.0, port_value: 1 } }
filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
"#,
        );
        assert!(l.enable_reuse_port, "defaults to true when omitted");
    }

    #[test]
    fn listener_enable_reuse_port_round_trips_true_and_false() {
        for (yaml_val, expected) in [("true", true), ("false", false)] {
            let l = parse_listener(&format!(
                r#"
name: a
address: {{ socket_address: {{ address: 0.0.0.0, port_value: 1 }} }}
enable_reuse_port: {yaml_val}
filter_chains: [{{ filters: [{{ name: envoy.filters.network.echo }}] }}]
"#
            ));
            assert_eq!(
                l.enable_reuse_port, expected,
                "enable_reuse_port: {yaml_val}"
            );
        }
    }

    #[test]
    fn bootstrap_parses_dynamic_resources_lds_path_config_source() {
        // (a) lds_config parses: resource_api_version + path_config_source.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  lds_config:
    resource_api_version: V3
    path_config_source:
      path: /tmp/lds.yaml
"#;
        let b = crate::parse_bootstrap(yaml).unwrap();
        let dr = b.dynamic_resources.as_ref().unwrap();
        assert!(dr.lds_config.is_some());
        let cs = dr.lds_config.as_ref().unwrap();
        assert_eq!(cs.path_config_source.path, "/tmp/lds.yaml");
        assert_eq!(cs.resource_api_version.as_deref(), Some("V3"));
    }

    #[test]
    fn bootstrap_parses_cds_and_lds_side_by_side() {
        // (b) the fixture-0027 shape: cds_config + lds_config together.
        let yaml = r#"
node: { id: test, cluster: test }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  cds_config:
    path_config_source: { path: /tmp/cds.yaml }
  lds_config:
    path_config_source: { path: /tmp/lds.yaml }
"#;
        let b = crate::parse_bootstrap(yaml).unwrap();
        let dr = b.dynamic_resources.as_ref().unwrap();
        assert!(dr.cds_config.is_some());
        assert!(dr.lds_config.is_some());
    }

    #[test]
    fn dynamic_resources_still_rejects_deferred_fields_with_lds_present() {
        // (c) deny_unknown_fields regression gate: ads_config / api_config_source /
        // watched_directory remain rejected even now that lds_config is a field.
        for field in [
            "ads_config: { api_type: GRPC }",
            "api_config_source: { api_type: GRPC }",
        ] {
            let yaml = format!(
                "node: {{ id: t, cluster: t }}\nadmin: {{ address: {{ socket_address: {{ address: 0.0.0.0, port_value: 0 }} }} }}\ndynamic_resources:\n  {field}"
            );
            assert!(
                crate::parse_bootstrap(&yaml).is_err(),
                "{field} should reject"
            );
        }
        // watched_directory inside lds_config.path_config_source still rejects.
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  lds_config:
    path_config_source:
      path: /tmp/lds.yaml
      watched_directory: { path: /tmp }
"#;
        assert!(crate::parse_bootstrap(yaml).is_err());
    }

    #[test]
    fn lds_configured_but_unloaded_transitions() {
        // (d) true when configured + dynamic_listeners is None; false unconfigured;
        // false once dynamic_listeners is Some (even empty).
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  lds_config:
    path_config_source: { path: /tmp/lds.yaml }
"#;
        let mut b = crate::parse_bootstrap(yaml).unwrap();
        assert!(b.lds_configured_but_unloaded());
        b.dynamic_listeners = Some(vec![]);
        assert!(!b.lds_configured_but_unloaded());

        // Unconfigured: no lds_config at all → false.
        let mut b2 = crate::parse_bootstrap(MINIMAL).unwrap();
        assert!(!b2.lds_configured_but_unloaded());
        b2.dynamic_listeners = Some(vec![]);
        assert!(!b2.lds_configured_but_unloaded());
    }

    #[test]
    fn all_listeners_chains_static_and_dynamic() {
        // (e) 1 static + 1 dynamic → all_listeners().count() == 2.
        let mut b = crate::parse_bootstrap(ONE_STATIC_LISTENER).unwrap();
        assert_eq!(b.all_listeners().count(), 1);
        let listener2 = parse_listener(
            r#"
name: b
address: { socket_address: { address: 0.0.0.0, port_value: 2 } }
filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
"#,
        );
        b.dynamic_listeners = Some(vec![listener2]);
        assert_eq!(b.all_listeners().count(), 2);
    }

    #[test]
    fn too_many_listeners_gate_fires_on_merged_count() {
        // (f) 1 static + 1 dynamic (distinct names) → validate() errs TooManyListeners(2).
        let mut b = crate::parse_bootstrap(ONE_STATIC_LISTENER).unwrap();
        let listener2 = parse_listener(
            r#"
name: b
address: { socket_address: { address: 0.0.0.0, port_value: 2 } }
filter_chains: [{ filters: [{ name: envoy.filters.network.echo }] }]
"#,
        );
        b.dynamic_listeners = Some(vec![listener2]);
        let err = validate(&mut b).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::TooManyListeners(2)),
            "got {err:?}"
        );
    }

    #[test]
    fn no_runtime_gate_defers_while_lds_configured_but_unloaded() {
        // (g) no admin + zero static listeners + lds_config configured (unloaded)
        // → parse_bootstrap SUCCEEDS (deferred).
        let yaml = r#"
node: { id: t, cluster: t }
dynamic_resources:
  lds_config:
    path_config_source: { path: /tmp/lds.yaml }
"#;
        assert!(
            crate::parse_bootstrap(yaml).is_ok(),
            "lds-configured-but-unloaded must defer NoRuntime"
        );

        // Same bootstrap WITHOUT lds_config → NoRuntime (pre-existing behavior).
        let yaml_no_lds = "node: { id: t, cluster: t }\n";
        let err = crate::parse_bootstrap(yaml_no_lds).expect_err("must reject");
        assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");

        // Same bootstrap with dynamic_listeners = Some(vec![]) (loaded-but-empty)
        // → NoRuntime (post-merge enforcement; the deferral no longer applies).
        let mut b = crate::parse_bootstrap(yaml).unwrap();
        b.dynamic_listeners = Some(vec![]);
        let err = validate(&mut b).expect_err("must reject post-merge");
        assert!(matches!(err, crate::ConfigError::NoRuntime), "got {err:?}");
    }

    #[test]
    fn lds_resource_api_version_v3_or_absent_accepted_others_rejected() {
        // (h) V3/absent accepted; V2 rejected with UnsupportedResourceApiVersion.
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  lds_config:
    resource_api_version: V2
    path_config_source: { path: /tmp/lds.yaml }
"#;
        let err = crate::parse_bootstrap(yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnsupportedResourceApiVersion(ref v) if v == "V2"),
            "got {err:?}"
        );
        // Absent resource_api_version on lds_config is accepted.
        let yaml = r#"
node: { id: t, cluster: t }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
dynamic_resources:
  lds_config:
    path_config_source: { path: /tmp/lds.yaml }
"#;
        assert!(crate::parse_bootstrap(yaml).is_ok());
    }

    // ----------------------------------------------------------------------
    // 21 D1 (ADR-0053/0054): EDS schema + exactly-one-of-and-consistent
    // endpoint-source validation. Groups (a)-(i) per PLAN Task 1 Step 1.
    // Each fixture carries an `admin` block so validate() reaches the cluster
    // checks (the NoRuntime gate fires for a no-admin / no-listener bootstrap).
    // ----------------------------------------------------------------------

    /// (a) An EDS cluster (`type: EDS` + `eds_cluster_config` with `service_name`,
    /// NO inline `load_assignment`) parses; the schema round-trips faithfully.
    #[test]
    fn eds_schema_eds_cluster_parses() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          path_config_source:
            path: /x
        service_name: svc
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("EDS cluster parses");
        let c = &b.static_resources.clusters[0];
        assert_eq!(c.cluster_type, ClusterType::Eds);
        assert_eq!(
            c.eds_cluster_config,
            Some(EdsClusterConfig {
                eds_config: ConfigSource {
                    path_config_source: PathConfigSource {
                        path: "/x".to_string(),
                    },
                    resource_api_version: None,
                },
                service_name: Some("svc".to_string()),
            })
        );
        assert!(c.load_assignment.is_none());
    }

    /// (b) `service_name` + `resource_api_version` are both optional inside
    /// `eds_cluster_config` / `eds_config`.
    #[test]
    fn eds_schema_service_name_optional() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          path_config_source:
            path: /x
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("EDS cluster parses");
        let c = &b.static_resources.clusters[0];
        let ecc = c.eds_cluster_config.as_ref().expect("eds_cluster_config");
        assert_eq!(ecc.service_name, None);
        assert_eq!(ecc.eds_config.resource_api_version, None);
    }

    /// (c) Regression-equivalence: an inline STATIC cluster still parses, with
    /// `load_assignment: Some` and `eds_cluster_config: None`.
    #[test]
    fn eds_schema_inline_static_still_parses() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let b = crate::parse_bootstrap(yaml).expect("inline STATIC parses");
        let c = &b.static_resources.clusters[0];
        assert!(c.load_assignment.is_some());
        assert!(c.eds_cluster_config.is_none());
        assert_eq!(c.load_assignment.as_ref().unwrap().cluster_name, "backend");
    }

    /// (d) EDS cluster with neither `eds_cluster_config` nor `load_assignment`
    /// → `MissingEdsClusterConfig` (L6 6c).
    #[test]
    fn eds_schema_eds_missing_config_rejected() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingEdsClusterConfig { ref cluster } if cluster == "eds_backend"),
            "expected MissingEdsClusterConfig; got {err:?}",
        );
    }

    /// (e) EDS cluster carrying an inline `load_assignment` →
    /// `LoadAssignmentOnEdsCluster` (L6 6a — stricter than Envoy; PARSE-time,
    /// driven through `parse_bootstrap`'s `check_endpoint_sources` pass).
    #[test]
    fn eds_schema_eds_inline_load_assignment_rejected() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          path_config_source:
            path: /x
      load_assignment:
        cluster_name: eds_backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::LoadAssignmentOnEdsCluster { ref cluster } if cluster == "eds_backend"),
            "expected LoadAssignmentOnEdsCluster; got {err:?}",
        );
    }

    /// (f) STATIC cluster carrying `eds_cluster_config` →
    /// `EdsConfigOnNonEdsCluster` (L6 6b).
    #[test]
    fn eds_schema_static_with_eds_config_rejected() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
      eds_cluster_config:
        eds_config:
          path_config_source:
            path: /x
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::EdsConfigOnNonEdsCluster { ref cluster } if cluster == "backend"),
            "expected EdsConfigOnNonEdsCluster; got {err:?}",
        );
    }

    /// (g) Non-EDS cluster with no `load_assignment` → `MissingLoadAssignment`.
    #[test]
    fn eds_schema_static_missing_load_assignment_rejected() {
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::MissingLoadAssignment { ref cluster } if cluster == "backend"),
            "expected MissingLoadAssignment; got {err:?}",
        );
    }

    /// (h) An unknown field inside `eds_cluster_config` is rejected
    /// (`deny_unknown_fields`).
    #[test]
    fn eds_schema_unknown_field_in_eds_cluster_config_rejected() {
        let yaml = r#"
static_resources:
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          path_config_source:
            path: /x
        ads: {}
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown field") || msg.contains("ads"),
            "expected deny_unknown_fields rejection; got {msg}",
        );
    }

    /// (i) `ConfigSource`'s `deny_unknown_fields` still rejects
    /// `api_config_source`/`ads`/`watched_directory` inside `eds_config`.
    #[test]
    fn eds_schema_unknown_field_in_eds_config_rejected() {
        let yaml = r#"
static_resources:
  clusters:
    - name: eds_backend
      type: EDS
      lb_policy: ROUND_ROBIN
      eds_cluster_config:
        eds_config:
          path_config_source:
            path: /x
          api_config_source: {}
"#;
        let err = serde_yaml::from_str::<Bootstrap>(yaml).expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("unknown field") || msg.contains("api_config_source"),
            "expected deny_unknown_fields rejection; got {msg}",
        );
    }

    // ---- 30 Task 1 (ADR-0073/0074): LbMetadata + endpoint metadata ----

    #[test]
    fn lb_metadata_parses_envoy_lb_namespace() {
        let yaml = r#"
endpoint:
  address:
    socket_address: { address: 127.0.0.1, port_value: 8001 }
metadata:
  filter_metadata:
    envoy.lb:
      stage: prod
      version: v2
"#;
        let ep: LbEndpoint = serde_yaml::from_str(yaml).expect("valid LbEndpoint");
        let md = ep.metadata.expect("metadata present");
        assert_eq!(md.envoy_lb.get("stage"), Some(&"prod".to_string()));
        assert_eq!(md.envoy_lb.get("version"), Some(&"v2".to_string()));
        assert_eq!(md.envoy_lb.len(), 2);
    }

    #[test]
    fn lb_metadata_absent_is_none() {
        let yaml = r#"
endpoint:
  address:
    socket_address: { address: 127.0.0.1, port_value: 8001 }
"#;
        let ep: LbEndpoint = serde_yaml::from_str(yaml).expect("valid LbEndpoint");
        assert!(ep.metadata.is_none(), "no metadata → None");
    }

    #[test]
    fn lb_metadata_non_envoy_lb_namespace_ignored() {
        // A non-`envoy.lb` namespace must parse without error and yield an empty
        // `envoy.lb` map (parsed-and-ignored, not an error).
        let yaml = r#"
endpoint:
  address:
    socket_address: { address: 127.0.0.1, port_value: 8001 }
metadata:
  filter_metadata:
    envoy.transport_socket_match:
      foo: bar
"#;
        let ep: LbEndpoint = serde_yaml::from_str(yaml).expect("valid LbEndpoint");
        let md = ep.metadata.expect("metadata struct present");
        assert!(md.envoy_lb.is_empty(), "non-envoy.lb namespace ignored");
    }

    #[test]
    fn lb_metadata_coerces_non_string_scalars() {
        // §6.2 COERCE: non-string scalars (number / bool) stringify permissively
        // so the `envoy_lb` map is uniformly string-valued (Task 4 subset keys).
        let yaml = r#"
endpoint:
  address:
    socket_address: { address: 127.0.0.1, port_value: 8001 }
metadata:
  filter_metadata:
    envoy.lb:
      version: 2
      enabled: true
"#;
        let ep: LbEndpoint = serde_yaml::from_str(yaml).expect("valid LbEndpoint");
        let md = ep.metadata.expect("metadata present");
        assert_eq!(md.envoy_lb.get("version"), Some(&"2".to_string()));
        assert_eq!(md.envoy_lb.get("enabled"), Some(&"true".to_string()));
        assert_eq!(md.envoy_lb.len(), 2);
    }

    // ---- 30 Task 2 (ADR-0073/0074): Cluster.lb_subset_config (accept-all) ----

    fn cluster_yaml(lb_subset_block: &str) -> String {
        format!(
            r#"
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  clusters:
    - name: backend
      type: STATIC
      lb_policy: MAGLEV
{lb_subset_block}      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 10001
"#
        )
    }

    #[test]
    fn lb_subset_config_absent_is_none() {
        let b = crate::parse_bootstrap(&cluster_yaml("")).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        assert!(c.lb_subset_config.is_none(), "no lb_subset_config → None");
    }

    #[test]
    fn lb_subset_config_default_fallback_policy_is_no_fallback() {
        // fallback_policy omitted ⇒ NO_FALLBACK (serde default).
        let block =
            "      lb_subset_config:\n        subset_selectors:\n          - keys: [stage]\n";
        let b = crate::parse_bootstrap(&cluster_yaml(block)).expect("valid YAML");
        let c = &b.static_resources.clusters[0];
        let cfg = c.lb_subset_config.as_ref().expect("present");
        assert_eq!(cfg.fallback_policy, LbSubsetFallbackPolicy::NoFallback);
        assert_eq!(cfg.subset_selectors.len(), 1);
        assert_eq!(cfg.subset_selectors[0].keys, vec!["stage".to_string()]);
        assert!(cfg.default_subset.is_none());
    }

    #[test]
    fn lb_subset_config_any_endpoint_fallback() {
        let block = concat!(
            "      lb_subset_config:\n",
            "        fallback_policy: ANY_ENDPOINT\n",
            "        subset_selectors:\n",
            "          - keys: [stage]\n",
        );
        let b = crate::parse_bootstrap(&cluster_yaml(block)).expect("valid YAML");
        let cfg = b.static_resources.clusters[0]
            .lb_subset_config
            .as_ref()
            .expect("present");
        assert_eq!(cfg.fallback_policy, LbSubsetFallbackPolicy::AnyEndpoint);
    }

    #[test]
    fn lb_subset_config_default_subset_fallback_with_default_subset() {
        let block = concat!(
            "      lb_subset_config:\n",
            "        fallback_policy: DEFAULT_SUBSET\n",
            "        default_subset:\n",
            "          stage: prod\n",
            "        subset_selectors:\n",
            "          - keys: [stage]\n",
        );
        let b = crate::parse_bootstrap(&cluster_yaml(block)).expect("valid YAML");
        let cfg = b.static_resources.clusters[0]
            .lb_subset_config
            .as_ref()
            .expect("present");
        assert_eq!(cfg.fallback_policy, LbSubsetFallbackPolicy::DefaultSubset);
        let ds = cfg.default_subset.as_ref().expect("default_subset present");
        assert_eq!(ds.get("stage").map(String::as_str), Some("prod"));
        assert_eq!(ds.len(), 1);
    }

    #[test]
    fn lb_subset_config_empty_selectors_parse_ok() {
        // §A divergence #1 (ADR-0074): empty subset_selectors is NOT fatal.
        let block = "      lb_subset_config:\n        subset_selectors: []\n";
        let b = crate::parse_bootstrap(&cluster_yaml(block))
            .expect("valid YAML — empty selectors accepted (not fatal)");
        let c = &b.static_resources.clusters[0];
        let cfg = c.lb_subset_config.as_ref().expect("present");
        assert!(cfg.subset_selectors.is_empty());
        // The validator must return Ok for empty selectors.
        assert!(super::validate_cluster(c).is_ok(), "empty selectors → Ok");
    }

    #[test]
    fn lb_subset_config_empty_keys_parse_ok() {
        // §A divergence #1 (ADR-0074): a selector with keys: [] is NOT fatal.
        let block = concat!(
            "      lb_subset_config:\n",
            "        subset_selectors:\n",
            "          - keys: []\n",
        );
        let b = crate::parse_bootstrap(&cluster_yaml(block))
            .expect("valid YAML — empty keys accepted (not fatal)");
        let c = &b.static_resources.clusters[0];
        let cfg = c.lb_subset_config.as_ref().expect("present");
        assert_eq!(cfg.subset_selectors.len(), 1);
        assert!(cfg.subset_selectors[0].keys.is_empty());
        assert!(super::validate_cluster(c).is_ok(), "empty keys → Ok");
    }
}

// ---------------------------------------------------------------------------
// Serialize roundtrip tests (Task 4 — sibling module per Tasks 1/2/3 cadence)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod serialize_roundtrip_tests {
    use crate::bootstrap::Bootstrap;

    /// Pre-D6 sanity check per 08.1 SPEC §6.4.
    ///
    /// Takes fixture 0008's `envoy-rust.yaml` — the most varied bootstrap shape
    /// in-tree at 08.1 time (HCM, STRICT_DNS cluster, 1 listener, http_filters,
    /// multi-route) — parses via serde_yaml, serializes via serde_json, deserializes
    /// via serde_json, and asserts structural equality.
    #[test]
    fn fixture_0008_bootstrap_roundtrips_yaml_to_json() {
        let raw = std::fs::read_to_string(
            "../../tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml",
        )
        .expect("fixture 0008 envoy-rust.yaml readable from envoy-config crate dir");
        // Fixture uses {{PORT}}, {{BACKEND_HOST}}, {{HTTP1_BACKEND_PORT}} as
        // template variables; substitute static values so serde_yaml can parse
        // port_value as u16 and address as a string.
        // template values are arbitrary — the test asserts struct-level roundtrip
        // equality, not the chosen substitution values.
        let yaml = raw
            .replace("{{PORT}}", "10000")
            .replace("{{BACKEND_HOST}}", "127.0.0.1")
            .replace("{{HTTP1_BACKEND_PORT}}", "10001");
        // YAML -> struct
        let parsed: Bootstrap = serde_yaml::from_str(&yaml).expect("YAML parses");
        // struct -> JSON
        let json = serde_json::to_string_pretty(&parsed).expect("Bootstrap serializes to JSON");
        // JSON -> struct
        let reparsed: Bootstrap =
            serde_json::from_str(&json).expect("JSON round-trips back to Bootstrap");
        // Coarse-grained idempotency check: re-serialize and compare strings.
        let json2 = serde_json::to_string_pretty(&reparsed).expect("re-serializes");
        assert_eq!(
            json, json2,
            "JSON serialization is idempotent after roundtrip"
        );
    }

    #[test]
    fn minimal_bootstrap_serializes_to_json() {
        let yaml =
            "node:\n  id: t\n  cluster: t\nstatic_resources:\n  listeners: []\n  clusters: []\n";
        let parsed: Bootstrap = serde_yaml::from_str(yaml).expect("minimal parses");
        let json = serde_json::to_string(&parsed).expect("minimal serializes");
        assert!(json.contains("\"node\""));
        assert!(json.contains("\"static_resources\""));
    }

    #[test]
    fn validate_outlier_detection_accepts_empty_block() {
        // §6.2 item-1: outlier_detection: {} (all fields absent) is accepted per Envoy v1.33.0.
        let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: od
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection: {}
      load_assignment:
        cluster_name: od
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: 127.0.0.1, port_value: 7000 } }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } } }
"#;
        crate::parse_bootstrap(yaml).expect("empty outlier_detection block accepted");
    }

    #[test]
    fn validate_outlier_detection_rejects_zero_consecutive_5xx() {
        let yaml = build_od_yaml(r#"consecutive_5xx: 0"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidOutlierDetectionThreshold {
                    ref cluster, field
                } if cluster == "od" && field == "consecutive_5xx"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_outlier_detection_rejects_zero_consecutive_gateway_failure() {
        let yaml = build_od_yaml(r#"consecutive_gateway_failure: 0"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidOutlierDetectionThreshold {
                    ref cluster, field
                } if cluster == "od" && field == "consecutive_gateway_failure"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_outlier_detection_rejects_zero_interval() {
        let yaml = build_od_yaml(r#"interval: 0s"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidOutlierDetectionTiming {
                    ref cluster, field
                } if cluster == "od" && field == "interval"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_outlier_detection_rejects_subsecond_decimal_interval() {
        // §6.2 item-6: parse_duration rejects sub-second decimals; surfaces as
        // InvalidOutlierDetectionTiming.
        let yaml = build_od_yaml(r#"interval: 0.5s"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidOutlierDetectionTiming {
                    ref cluster, field
                } if cluster == "od" && field == "interval"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_outlier_detection_rejects_zero_base_ejection_time() {
        let yaml = build_od_yaml(r#"base_ejection_time: 0s"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidOutlierDetectionTiming {
                    ref cluster, field
                } if cluster == "od" && field == "base_ejection_time"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_outlier_detection_rejects_max_ejection_percent_above_100() {
        let yaml = build_od_yaml(r#"max_ejection_percent: 101"#);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::InvalidMaxEjectionPercent {
                    ref cluster, value: 101
                } if cluster == "od"
            ),
            "got {err:?}",
        );
    }

    #[test]
    fn validate_outlier_detection_accepts_max_ejection_percent_zero() {
        // Boundary: 0 is in [0,100]; the validator accepts it. (At runtime, cap_count = 0
        // means every threshold-crossing increments ejections_overflow; that's a Task-4
        // concern, not Task-2.)
        let yaml = build_od_yaml(r#"max_ejection_percent: 0"#);
        crate::parse_bootstrap(&yaml).expect("0 is in [0,100]");
    }

    #[test]
    fn validate_outlier_detection_accepts_max_ejection_percent_100() {
        let yaml = build_od_yaml(r#"max_ejection_percent: 100"#);
        crate::parse_bootstrap(&yaml).expect("100 is in [0,100]");
    }

    #[test]
    fn validate_outlier_detection_accepts_minimum_viable_full_block() {
        let yaml = build_od_yaml(
            "consecutive_5xx: 5\n        consecutive_gateway_failure: 5\n        interval: 10s\n        base_ejection_time: 30s\n        max_ejection_percent: 10",
        );
        crate::parse_bootstrap(&yaml).expect("Envoy-default block validates");
    }

    // Helper: build a single-cluster bootstrap YAML with the named outlier_detection body.
    // Caller-supplied `od_body` is the indented content of the `outlier_detection:` block
    // (one or more lines, each indented to match the YAML structure).
    fn build_od_yaml(od_body: &str) -> String {
        format!(
            r#"
static_resources:
  listeners: []
  clusters:
    - name: od
      type: STATIC
      lb_policy: ROUND_ROBIN
      outlier_detection:
        {od_body}
      load_assignment:
        cluster_name: od
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: {{ socket_address: {{ address: 127.0.0.1, port_value: 7000 }} }}
admin: {{ address: {{ socket_address: {{ address: 127.0.0.1, port_value: 9901 }} }} }}
"#
        )
    }

    mod jwt_authn_validator_tests {
        use super::super::validate_jwt_authn_config;

        const VALID_JWKS: &str = r#"{"keys":[{"kty":"RSA","kid":"k1","n":"sXche4iX","e":"AQAB"}]}"#;

        #[test]
        fn jwt_authn_validator_rejects_empty_providers() {
            let cfg = crate::JwtAuthnConfig {
                providers: std::collections::BTreeMap::new(),
                rules: vec![],
            };
            let err = validate_jwt_authn_config(&cfg, "l0").unwrap_err();
            assert!(matches!(
                err,
                crate::ConfigError::JwtAuthnNoProviders { .. }
            ));
        }

        #[test]
        fn jwt_authn_validator_rejects_dangling_provider_ref() {
            let mut providers = std::collections::BTreeMap::new();
            providers.insert(
                "p1".to_string(),
                crate::JwtProvider {
                    issuer: "i".to_string(),
                    audiences: vec![],
                    local_jwks: crate::DataSource {
                        filename: None,
                        inline_string: Some(VALID_JWKS.to_string()),
                    },
                    forward: false,
                },
            );
            let cfg = crate::JwtAuthnConfig {
                providers,
                rules: vec![crate::RequirementRule {
                    r#match: crate::RouteMatch {
                        prefix: Some("/".to_string()),
                        path: None,
                        headers: vec![],
                    },
                    requires: crate::JwtRequirement {
                        provider_name: "nope".to_string(),
                    },
                }],
            };
            assert!(matches!(
                validate_jwt_authn_config(&cfg, "l0").unwrap_err(),
                crate::ConfigError::JwtAuthnUnknownProvider { .. }
            ));
        }

        #[test]
        fn jwt_authn_validator_rejects_bad_jwks() {
            let mut providers = std::collections::BTreeMap::new();
            providers.insert(
                "p1".to_string(),
                crate::JwtProvider {
                    issuer: "i".to_string(),
                    audiences: vec![],
                    local_jwks: crate::DataSource {
                        filename: None,
                        inline_string: Some("not json".to_string()),
                    },
                    forward: false,
                },
            );
            let cfg = crate::JwtAuthnConfig {
                providers,
                rules: vec![],
            };
            assert!(matches!(
                validate_jwt_authn_config(&cfg, "l0").unwrap_err(),
                crate::ConfigError::JwtAuthnInvalidJwks { .. }
            ));
        }

        #[test]
        fn jwt_authn_validator_accepts_valid() {
            let mut providers = std::collections::BTreeMap::new();
            providers.insert(
                "p1".to_string(),
                crate::JwtProvider {
                    issuer: "i".to_string(),
                    audiences: vec![],
                    local_jwks: crate::DataSource {
                        filename: None,
                        inline_string: Some(VALID_JWKS.to_string()),
                    },
                    forward: false,
                },
            );
            let cfg = crate::JwtAuthnConfig {
                providers,
                rules: vec![crate::RequirementRule {
                    r#match: crate::RouteMatch {
                        prefix: Some("/".to_string()),
                        path: None,
                        headers: vec![],
                    },
                    requires: crate::JwtRequirement {
                        provider_name: "p1".to_string(),
                    },
                }],
            };
            assert!(validate_jwt_authn_config(&cfg, "l0").is_ok());
        }
    }

    mod jwt_authn_tests {
        #[test]
        fn parses_jwt_authn_filter_config() {
            let yaml = r#"
name: envoy.filters.http.jwt_authn
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.jwt_authn.v3.JwtAuthentication
  providers:
    provider1:
      issuer: "testing@secure.istio.io"
      audiences: ["aud1"]
      local_jwks:
        inline_string: '{"keys":[]}'
      forward: true
  rules:
    - match: { prefix: "/" }
      requires: { provider_name: "provider1" }
"#;
            let hf: crate::HttpFilter = serde_yaml::from_str(yaml).expect("parses");
            match hf.typed_config {
                crate::HttpFilterTypedConfig::JwtAuthn(cfg) => {
                    assert_eq!(cfg.providers.len(), 1);
                    let p = &cfg.providers["provider1"];
                    assert_eq!(p.issuer, "testing@secure.istio.io");
                    assert_eq!(p.audiences, vec!["aud1".to_string()]);
                    assert!(p.forward);
                    assert_eq!(cfg.rules.len(), 1);
                    assert_eq!(cfg.rules[0].requires.provider_name, "provider1");
                }
                other => panic!("expected JwtAuthn, got {other:?}"),
            }
        }

        #[test]
        fn jwt_provider_forward_defaults_false() {
            let yaml = r#"
issuer: "i"
local_jwks: { inline_string: "{}" }
"#;
            let p: crate::JwtProvider = serde_yaml::from_str(yaml).expect("parses");
            assert!(
                !p.forward,
                "forward defaults false (§6.2 L6 — strip Authorization)"
            );
            assert!(p.audiences.is_empty());
        }
    }
}

// ---------------------------------------------------------------------------
// 23 D1: per-route typed_per_filter_config schema tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod typed_per_filter_config_tests {
    use super::*;

    #[test]
    fn route_parses_typed_per_filter_config_cors() {
        let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
typed_per_filter_config:
  envoy.filters.http.cors:
    "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
    allow_origin_string_match:
      - exact: "http://allowed.example.com"
    allow_methods: "GET, POST, OPTIONS"
    allow_headers: "x-custom-header, content-type"
    expose_headers: "x-exposed-header"
    max_age: "3600"
    allow_credentials: true
"#;
        let route: Route = serde_yaml::from_str(yaml).expect("parses");
        let pfc = route
            .typed_per_filter_config
            .get("envoy.filters.http.cors")
            .expect("cors per-filter config present");
        let PerFilterConfig::Cors(p) = pfc else {
            panic!("expected Cors variant")
        };
        assert_eq!(p.allow_origin_string_match.len(), 1);
        assert!(p.allow_origin_string_match[0].matches("http://allowed.example.com"));
        assert_eq!(p.allow_methods.as_deref(), Some("GET, POST, OPTIONS"));
        assert_eq!(
            p.allow_headers.as_deref(),
            Some("x-custom-header, content-type")
        );
        assert_eq!(p.expose_headers.as_deref(), Some("x-exposed-header"));
        assert_eq!(p.max_age.as_deref(), Some("3600"));
        assert_eq!(p.allow_credentials, Some(true));
    }

    #[test]
    fn route_parses_name_when_present_and_defaults_empty_when_absent() {
        // phase 41: a NAMED route round-trips its config `name`.
        let named = r#"
name: myroute
match: { prefix: "/" }
route: { cluster: backend }
"#;
        let route: Route = serde_yaml::from_str(named).expect("named route parses");
        assert_eq!(route.name, "myroute");

        // An UNNAMED (name-less) route still parses; `name` defaults empty.
        let unnamed = r#"
match: { prefix: "/" }
route: { cluster: backend }
"#;
        let route: Route = serde_yaml::from_str(unnamed).expect("name-less route parses");
        assert_eq!(route.name, "");
    }

    #[test]
    fn csrf_policy_parses_filter_enabled_and_additional_origins() {
        let yaml = r#"
filter_enabled:
  default_value: { numerator: 100, denominator: HUNDRED }
additional_origins:
- exact: "additional.example.com"
- suffix: ".trusted.example.com"
"#;
        let p: CsrfPolicy = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(p.filter_enabled.default_value.numerator, 100);
        assert!(p.filter_enabled.default_value.selects_deterministic());
        assert_eq!(p.filter_enabled.runtime_key, None);
        assert_eq!(p.additional_origins.len(), 2);
        assert!(p.additional_origins[0].matches("additional.example.com"));
    }

    #[test]
    fn csrf_policy_parses_runtime_key() {
        let yaml = r#"
filter_enabled:
  default_value: { numerator: 100, denominator: HUNDRED }
  runtime_key: "csrf.enabled"
"#;
        let p: CsrfPolicy = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(
            p.filter_enabled.runtime_key.as_deref(),
            Some("csrf.enabled")
        );
    }

    #[test]
    fn csrf_policy_requires_filter_enabled() {
        // filter_enabled has no #[serde(default)] → absence is a parse error.
        let yaml = r#"
additional_origins:
- exact: "additional.example.com"
"#;
        assert!(serde_yaml::from_str::<CsrfPolicy>(yaml).is_err());
    }

    #[test]
    fn csrf_policy_rejects_shadow_enabled() {
        let yaml = r#"
filter_enabled:
  default_value: { numerator: 100, denominator: HUNDRED }
shadow_enabled:
  default_value: { numerator: 100, denominator: HUNDRED }
"#;
        assert!(
            serde_yaml::from_str::<CsrfPolicy>(yaml).is_err(),
            "deny_unknown_fields must reject shadow_enabled"
        );
    }

    #[test]
    fn route_parses_typed_per_filter_config_csrf() {
        let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
typed_per_filter_config:
  envoy.filters.http.csrf:
    "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
    filter_enabled:
      default_value: { numerator: 100, denominator: HUNDRED }
    additional_origins:
    - exact: "additional.example.com"
"#;
        let route: Route = serde_yaml::from_str(yaml).expect("parses");
        let pfc = route
            .typed_per_filter_config
            .get("envoy.filters.http.csrf")
            .expect("csrf pfc present");
        match pfc {
            PerFilterConfig::Csrf(p) => {
                assert!(p.filter_enabled.default_value.selects_deterministic());
                assert_eq!(p.additional_origins.len(), 1);
            }
            other => panic!("expected Csrf, got {other:?}"),
        }
    }

    // 25.2 Task 1: buffer filter config schema parse tests (ADR-0063/0064)
    #[test]
    fn buffer_chain_config_parses_plain_integer() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: 10
"#;
        let cfg: crate::HttpFilterTypedConfig = serde_yaml::from_str(yaml).unwrap();
        match cfg {
            crate::HttpFilterTypedConfig::Buffer(b) => assert_eq!(b.max_request_bytes, 10),
            _ => panic!("expected Buffer"),
        }
    }

    #[test]
    fn buffer_chain_config_zero_is_accepted() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: 0
"#;
        let cfg: crate::HttpFilterTypedConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(cfg, crate::HttpFilterTypedConfig::Buffer(b) if b.max_request_bytes == 0));
    }

    #[test]
    fn buffer_chain_config_absent_max_request_bytes_is_fatal() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
"#;
        let r: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            r.is_err(),
            "absent max_request_bytes must be a fatal parse error"
        );
    }

    #[test]
    fn buffer_chain_config_negative_max_request_bytes_is_fatal() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: -1
"#;
        let r: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(yaml);
        assert!(
            r.is_err(),
            "negative max_request_bytes must be a fatal parse error"
        );
    }

    #[test]
    fn buffer_per_route_disabled_parses() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
disabled: true
"#;
        let pfc: crate::PerFilterConfig = serde_yaml::from_str(yaml).unwrap();
        match pfc {
            crate::PerFilterConfig::Buffer(b) => {
                assert!(b.disabled);
                assert!(b.buffer.is_none());
            }
            _ => panic!("expected Buffer per-route"),
        }
    }

    #[test]
    fn buffer_per_route_lowered_limit_parses() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.BufferPerRoute
buffer:
  max_request_bytes: 4
"#;
        let pfc: crate::PerFilterConfig = serde_yaml::from_str(yaml).unwrap();
        match pfc {
            crate::PerFilterConfig::Buffer(b) => {
                assert!(!b.disabled);
                assert_eq!(b.buffer.unwrap().max_request_bytes, 4);
            }
            _ => panic!("expected Buffer per-route"),
        }
    }

    #[test]
    fn buffer_chain_config_rejects_unknown_field() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.buffer.v3.Buffer
max_request_bytes: 10
bogus: 1
"#;
        let r: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err(), "deny_unknown_fields must reject unknown field");
    }

    #[test]
    fn route_without_typed_per_filter_config_defaults_empty() {
        let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
"#;
        let route: Route = serde_yaml::from_str(yaml).expect("parses");
        assert!(route.typed_per_filter_config.is_empty());
    }

    #[test]
    fn cors_policy_rejects_unknown_field() {
        let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
typed_per_filter_config:
  envoy.filters.http.cors:
    "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
    allow_origin_string_match: [ { exact: "x" } ]
    allow_private_network_access: true
"#;
        assert!(serde_yaml::from_str::<Route>(yaml).is_err());
    }

    #[test]
    fn route_rejects_unknown_top_level_key() {
        let yaml = r#"
match: { prefix: "/" }
route: { cluster: backend }
bogus_key: 1
"#;
        assert!(serde_yaml::from_str::<Route>(yaml).is_err());
    }

    /// Verify `CorsConfig` can be constructed as a default (near-empty filter-chain entry).
    /// NOTE: `CorsConfig` is used INSIDE `HttpFilterTypedConfig` (Task 4 adds the enum
    /// variant), where the `@type` field is consumed by the enum's `#[serde(tag = "@type")]`
    /// before the inner struct sees it. Deserializing a bare `CorsConfig` directly from
    /// `"@type": ...` YAML would reject the `@type` key via `deny_unknown_fields`. The
    /// meaningful test is that the struct exists, carries `Default`, and can be used —
    /// which the filter-chain machinery does after the enum dispatches.
    #[test]
    fn cors_config_filter_chain_entry_is_near_empty() {
        let _c = CorsConfig::default();
    }
}

// ---------------------------------------------------------------------------
// 23 D3: PerRouteConfigForAbsentFilter validator tests (ADR-0058 / L7)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod per_route_absent_filter_tests {
    // ---- YAML helper constants ----

    /// A minimal full-bootstrap YAML with a CorsPolicy on a route, but the
    /// `cors` filter is NOT in the HCM's http_filters chain (only `router` is).
    /// This must trigger ConfigError::PerRouteConfigForAbsentFilter.
    ///
    /// The route uses `direct_response` (no cluster reference needed) to keep
    /// this fully self-contained.
    const MINIMAL_HCM_BOOTSTRAP_WITH_CORS_POLICY_BUT_NO_CORS_FILTER: &str = r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                          typed_per_filter_config:
                            envoy.filters.http.cors:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
                              allow_origin_string_match:
                                - exact: "http://allowed.example.com"
                              allow_methods: "GET, OPTIONS"
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 9901 }
"#;

    /// A minimal full-bootstrap YAML with an EMPTY `typed_per_filter_config`
    /// on a route (the common case for all existing fixtures). The validator
    /// must NOT fire for this case — this is the negative-validator regression
    /// guard.
    const MINIMAL_HCM_BOOTSTRAP_EMPTY_TYPED_PER_FILTER_CONFIG: &str = r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 9901 }
"#;

    /// NEGATIVE (primary deliverable): a route with `typed_per_filter_config`
    /// keyed under `envoy.filters.http.cors` while only `envoy.filters.http.router`
    /// is in the HCM's http_filters chain → must reject with
    /// ConfigError::PerRouteConfigForAbsentFilter { filter: "envoy.filters.http.cors" }.
    ///
    /// ADR-0058 / L7 stricter-reject (recorded divergence vs upstream Envoy which
    /// accepts-and-ignores). BEHAVIOR_CONTRACT.
    #[test]
    fn cors_per_route_config_without_cors_filter_is_fatal() {
        let yaml = MINIMAL_HCM_BOOTSTRAP_WITH_CORS_POLICY_BUT_NO_CORS_FILTER;
        let err = crate::parse_bootstrap(yaml).unwrap_err();
        assert!(
            matches!(
                err,
                crate::ConfigError::PerRouteConfigForAbsentFilter { ref filter }
                    if filter == "envoy.filters.http.cors"
            ),
            "expected PerRouteConfigForAbsentFilter(cors), got {err:?}"
        );
    }

    /// REGRESSION GUARD: a route with no `typed_per_filter_config` (empty
    /// BTreeMap — the common case for every existing fixture) must NOT trigger
    /// the validator. This ensures the existing fixture suite stays valid.
    #[test]
    fn empty_typed_per_filter_config_does_not_trigger_validator() {
        let yaml = MINIMAL_HCM_BOOTSTRAP_EMPTY_TYPED_PER_FILTER_CONFIG;
        assert!(
            crate::parse_bootstrap(yaml).is_ok(),
            "empty typed_per_filter_config must not trigger PerRouteConfigForAbsentFilter"
        );
    }

    /// POSITIVE CASE (ignored pending Task 4):
    ///
    /// Once Task 4 adds `HttpFilterTypedConfig::Cors` variant, this test should
    /// be un-ignored. It verifies that a route with a CorsPolicy where the HCM's
    /// http_filters chain INCLUDES `envoy.filters.http.cors` parses successfully
    /// (the validator's present-filter path). Until that variant exists, a YAML
    /// entry with `@type: .../Cors` fails to parse at the `HttpFilterTypedConfig`
    /// deserialization stage.
    #[test]
    fn cors_per_route_config_with_cors_filter_present_parses() {
        let yaml = r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                          typed_per_filter_config:
                            envoy.filters.http.cors:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.CorsPolicy
                              allow_origin_string_match:
                                - exact: "http://allowed.example.com"
                              allow_methods: "GET, OPTIONS"
                http_filters:
                  - name: envoy.filters.http.cors
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 9901 }
"#;
        assert!(
            crate::parse_bootstrap(yaml).is_ok(),
            "CorsPolicy on route + cors filter in chain must parse successfully"
        );
    }

    // ---- Task 4: HttpFilterTypedConfig::Cors variant ----

    #[test]
    fn cors_filter_chain_entry_parses_to_cors_variant() {
        let yaml = r#"
name: envoy.filters.http.cors
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.cors.v3.Cors
"#;
        let hf: crate::HttpFilter = serde_yaml::from_str(yaml).expect("parses");
        assert!(matches!(
            hf.typed_config,
            crate::HttpFilterTypedConfig::Cors(_)
        ));
    }
}

// ---------------------------------------------------------------------------
// 24 D3: csrf filter-chain entry + validate_csrf_config tests (ADR-0061 L6/L7)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod csrf_validator_tests {
    /// Build a minimal full-bootstrap YAML with a `csrf` filter in the HCM's
    /// http_filters chain (`filter_enabled` numerator = `numerator` of HUNDRED,
    /// optional `runtime_key`) followed by a `router` terminus. Self-contained
    /// (direct_response route — no cluster reference needed).
    fn bootstrap_with_csrf_chain(numerator: u32, runtime_key: Option<&str>) -> String {
        let runtime_key_line = match runtime_key {
            Some(k) => format!("\n                        runtime_key: \"{k}\""),
            None => String::new(),
        };
        format!(
            r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.csrf
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
                      filter_enabled:
                        default_value:
                          numerator: {numerator}
                          denominator: HUNDRED{runtime_key_line}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 9901 }}
"#
        )
    }

    /// A route carrying a csrf `typed_per_filter_config` but NO csrf filter in
    /// the chain (only `router`). Must hit `PerRouteConfigForAbsentFilter` (L7).
    fn bootstrap_csrf_route_without_chain_filter() -> String {
        r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: { address: 0.0.0.0, port_value: 8080 }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                          typed_per_filter_config:
                            envoy.filters.http.csrf:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
                              filter_enabled:
                                default_value:
                                  numerator: 100
                                  denominator: HUNDRED
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: { address: 0.0.0.0, port_value: 9901 }
"#
        .to_string()
    }

    /// A route carrying a csrf override (`filter_enabled` numerator = `numerator`
    /// of HUNDRED, optional `runtime_key`) AND a csrf filter IN the chain (so the
    /// absent-filter check passes and the route-level `validate_csrf_config` runs).
    fn bootstrap_csrf_route_override(numerator: u32, runtime_key: Option<&str>) -> String {
        let runtime_key_line = match runtime_key {
            Some(k) => format!("\n                                runtime_key: \"{k}\""),
            None => String::new(),
        };
        format!(
            r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                          typed_per_filter_config:
                            envoy.filters.http.csrf:
                              "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
                              filter_enabled:
                                default_value:
                                  numerator: {numerator}
                                  denominator: HUNDRED{runtime_key_line}
                http_filters:
                  - name: envoy.filters.http.csrf
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.csrf.v3.CsrfPolicy
                      filter_enabled:
                        default_value:
                          numerator: 100
                          denominator: HUNDRED
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 9901 }}
"#
        )
    }

    #[test]
    fn hcm_accepts_csrf_filter_chain_entry() {
        let yaml = bootstrap_with_csrf_chain(100, None);
        assert!(
            crate::parse_bootstrap(&yaml).is_ok(),
            "csrf filter chain entry (filter_enabled 100%) + router terminus must parse + validate clean"
        );
    }

    #[test]
    fn rejects_non_deterministic_csrf_filter_enabled() {
        let yaml = bootstrap_with_csrf_chain(50, None); // numerator 50 of HUNDRED
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedNonDeterministicCsrfFilterEnabled { .. }
            ),
            "expected UnsupportedNonDeterministicCsrfFilterEnabled, got {err:?}"
        );
    }

    #[test]
    fn rejects_runtime_keyed_csrf_filter_enabled() {
        let yaml = bootstrap_with_csrf_chain(100, Some("csrf.enabled"));
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled { .. }
            ),
            "expected UnsupportedRuntimeKeyedCsrfFilterEnabled, got {err:?}"
        );
    }

    #[test]
    fn rejects_csrf_per_route_policy_for_absent_filter() {
        let yaml = bootstrap_csrf_route_without_chain_filter();
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(
                err,
                crate::ConfigError::PerRouteConfigForAbsentFilter { ref filter }
                    if filter == "envoy.filters.http.csrf"
            ),
            "expected PerRouteConfigForAbsentFilter(csrf), got {err:?}"
        );
    }

    #[test]
    fn rejects_non_deterministic_csrf_route_override() {
        let yaml = bootstrap_csrf_route_override(50, None);
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedNonDeterministicCsrfFilterEnabled { .. }
            ),
            "expected UnsupportedNonDeterministicCsrfFilterEnabled (route override), got {err:?}"
        );
    }

    #[test]
    fn rejects_runtime_keyed_csrf_route_override() {
        let yaml = bootstrap_csrf_route_override(100, Some("csrf.enabled"));
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(
                err,
                crate::ConfigError::UnsupportedRuntimeKeyedCsrfFilterEnabled { .. }
            ),
            "expected UnsupportedRuntimeKeyedCsrfFilterEnabled (route override), got {err:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// phase 31 Task 2: cdn_loop filter config schema + cdn_id validation tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod cdn_loop_config_tests {
    /// A standalone `cdn_loop` filter typed_config (chain-level), parsed in
    /// isolation as `HttpFilterTypedConfig`. `cdn_id` is required; the optional
    /// `max_allowed_occurrences` defaults to 0.
    fn cdn_loop_typed_config(cdn_id: &str, max_line: &str) -> String {
        format!(
            r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig
cdn_id: "{cdn_id}"{max_line}
"#
        )
    }

    #[test]
    fn cdn_loop_config_parses_and_selects_variant() {
        let yaml = cdn_loop_typed_config("mycdn.example", "");
        let cfg: crate::HttpFilterTypedConfig = serde_yaml::from_str(&yaml).unwrap();
        match cfg {
            crate::HttpFilterTypedConfig::CdnLoop(c) => {
                assert_eq!(c.cdn_id, "mycdn.example");
                assert_eq!(c.max_allowed_occurrences, 0);
            }
            other => panic!("expected CdnLoop variant, got {other:?}"),
        }
    }

    #[test]
    fn cdn_loop_config_max_allowed_occurrences_parses() {
        let yaml = cdn_loop_typed_config("mycdn.example", "\nmax_allowed_occurrences: 3");
        let cfg: crate::HttpFilterTypedConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(matches!(
            cfg,
            crate::HttpFilterTypedConfig::CdnLoop(c)
                if c.cdn_id == "mycdn.example" && c.max_allowed_occurrences == 3
        ));
    }

    #[test]
    fn cdn_loop_config_absent_max_defaults_zero() {
        let yaml = cdn_loop_typed_config("mycdn.example", "");
        let cfg: crate::HttpFilterTypedConfig = serde_yaml::from_str(&yaml).unwrap();
        assert!(matches!(
            cfg,
            crate::HttpFilterTypedConfig::CdnLoop(c) if c.max_allowed_occurrences == 0
        ));
    }

    #[test]
    fn cdn_loop_config_rejects_unknown_field() {
        let yaml = r#"
"@type": type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig
cdn_id: "mycdn.example"
bogus: 1
"#;
        let r: Result<crate::HttpFilterTypedConfig, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err(), "deny_unknown_fields must reject unknown field");
    }

    /// A full bootstrap with a `cdn_loop` filter chain entry (+ router terminus),
    /// parameterized on `cdn_id`. Exercises `parse_bootstrap`'s validation path.
    fn bootstrap_with_cdn_loop(cdn_id: &str) -> String {
        format!(
            r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.cdn_loop
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.cdn_loop.v3.CdnLoopConfig
                      cdn_id: "{cdn_id}"
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 9901 }}
"#
        )
    }

    #[test]
    fn hcm_accepts_cdn_loop_filter_chain_entry() {
        let yaml = bootstrap_with_cdn_loop("mycdn.example");
        assert!(
            crate::parse_bootstrap(&yaml).is_ok(),
            "valid cdn_id token + router terminus must parse + validate clean"
        );
    }

    #[test]
    fn rejects_empty_cdn_id() {
        let yaml = bootstrap_with_cdn_loop("");
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::CdnLoopEmptyCdnId { .. }),
            "expected CdnLoopEmptyCdnId, got {err:?}"
        );
    }

    #[test]
    fn rejects_comma_in_cdn_id() {
        let yaml = bootstrap_with_cdn_loop("a,b");
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::CdnLoopInvalidCdnId { .. }),
            "expected CdnLoopInvalidCdnId for comma token, got {err:?}"
        );
    }

    #[test]
    fn rejects_space_in_cdn_id() {
        let yaml = bootstrap_with_cdn_loop("a b");
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::CdnLoopInvalidCdnId { .. }),
            "expected CdnLoopInvalidCdnId for space token, got {err:?}"
        );
    }

    #[test]
    fn rejects_at_sign_in_cdn_id() {
        let yaml = bootstrap_with_cdn_loop("a@b");
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::CdnLoopInvalidCdnId { .. }),
            "expected CdnLoopInvalidCdnId for '@' token, got {err:?}"
        );
    }
}

#[cfg(test)]
mod set_metadata_config_tests {
    use super::*;

    #[test]
    fn parses_set_metadata_filter_modern_form() {
        let yaml = r#"
name: envoy.filters.http.set_metadata
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config
  metadata:
  - metadata_namespace: envoy.test
    value:
      tier: prod
"#;
        let hf: HttpFilter = serde_yaml::from_str(yaml).expect("parses");
        match hf.typed_config {
            HttpFilterTypedConfig::SetMetadata(cfg) => {
                assert_eq!(cfg.metadata.len(), 1);
                assert_eq!(cfg.metadata[0].metadata_namespace, "envoy.test");
                assert_eq!(cfg.metadata[0].value["tier"], "prod");
                assert!(!cfg.metadata[0].allow_overwrite); // serde default false
            }
            other => panic!("expected SetMetadata, got {other:?}"),
        }
    }

    #[test]
    fn set_metadata_non_string_value_is_rejected() {
        // A non-string YAML scalar value (a number) must FAIL serde
        // deserialization — the string-only MVP boundary (§A5).
        let yaml = r#"
name: envoy.filters.http.set_metadata
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config
  metadata:
  - metadata_namespace: envoy.test
    value:
      tier: 7
"#;
        let r: Result<HttpFilter, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err(), "non-string value must be rejected by serde");
    }

    /// A full bootstrap with a `set_metadata` filter chain entry (+ router
    /// terminus), parameterized on the filter `name` and the `metadata_namespace`.
    /// Exercises `parse_bootstrap`'s validation path (the same entry-point the
    /// cdn_loop validator tests use).
    fn bootstrap_with_set_metadata(filter_name: &str, namespace: &str) -> String {
        format!(
            r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: {filter_name}
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.set_metadata.v3.Config
                      metadata:
                      - metadata_namespace: "{namespace}"
                        value: {{ tier: prod }}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 9901 }}
"#
        )
    }

    #[test]
    fn set_metadata_empty_namespace_is_fatal() {
        let yaml = bootstrap_with_set_metadata("envoy.filters.http.set_metadata", "");
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::SetMetadataEmptyNamespace { .. }),
            "expected SetMetadataEmptyNamespace, got {err:?}"
        );
    }

    #[test]
    fn set_metadata_name_mismatch_is_unsupported() {
        let yaml = bootstrap_with_set_metadata("envoy.filters.http.wrong_name", "envoy.test");
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnsupportedHttpFilter { .. }),
            "expected UnsupportedHttpFilter for name mismatch, got {err:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Phase 34 Task 1: HeaderToMetadataConfig schema tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod header_to_metadata_config_tests {
    use super::*;

    #[test]
    fn parses_header_to_metadata_filter() {
        let yaml = r#"
name: envoy.filters.http.header_to_metadata
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
  request_rules:
  - header: x-tier
    on_header_present:
      metadata_namespace: envoy.lb
      key: tier
    on_header_missing:
      metadata_namespace: envoy.lb
      key: tier
      value: default-tier
"#;
        let hf: HttpFilter = serde_yaml::from_str(yaml).expect("parses");
        match hf.typed_config {
            HttpFilterTypedConfig::HeaderToMetadata(cfg) => {
                assert_eq!(cfg.request_rules.len(), 1);
                let r = &cfg.request_rules[0];
                assert_eq!(r.header, "x-tier");
                let p = r.on_header_present.as_ref().unwrap();
                assert_eq!(p.metadata_namespace, "envoy.lb");
                assert_eq!(p.key, "tier");
                assert!(p.value.is_none());
                assert_eq!(
                    r.on_header_missing.as_ref().unwrap().value.as_deref(),
                    Some("default-tier")
                );
            }
            other => panic!("expected HeaderToMetadata, got {other:?}"),
        }
    }

    #[test]
    fn header_to_metadata_default_namespace_is_filter_name() {
        // When metadata_namespace is omitted, the default must be the filter's
        // canonical name "envoy.filters.http.header_to_metadata" (A2-LOCKED).
        let yaml = r#"
name: envoy.filters.http.header_to_metadata
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
  request_rules:
  - header: x-env
    on_header_present:
      key: env
"#;
        let hf: HttpFilter = serde_yaml::from_str(yaml).expect("parses");
        match hf.typed_config {
            HttpFilterTypedConfig::HeaderToMetadata(cfg) => {
                let p = cfg.request_rules[0].on_header_present.as_ref().unwrap();
                assert_eq!(
                    p.metadata_namespace,
                    "envoy.filters.http.header_to_metadata"
                );
            }
            other => panic!("expected HeaderToMetadata, got {other:?}"),
        }
    }

    #[test]
    fn header_to_metadata_rejects_unknown_rule_field() {
        // A rule with `remove: true` (a deferred field) must FAIL serde
        // deserialization — rejected by deny_unknown_fields (boot-fatal, §A1).
        let yaml = r#"
name: envoy.filters.http.header_to_metadata
typed_config:
  "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
  request_rules:
  - header: x-tier
    remove: true
    on_header_present:
      key: tier
"#;
        let r: Result<HttpFilter, _> = serde_yaml::from_str(yaml);
        assert!(
            r.is_err(),
            "deferred `remove` field must be rejected by deny_unknown_fields"
        );
    }
}

// ---------------------------------------------------------------------------
// Phase 34 Task 2: header_to_metadata validator tests (via parse_bootstrap)
// ---------------------------------------------------------------------------
#[cfg(test)]
mod header_to_metadata_validator_tests {
    /// Full bootstrap YAML with one HCM containing a `header_to_metadata` filter,
    /// parameterized on filter `name` and raw YAML for the `request_rules` block.
    fn bootstrap_with_header_to_metadata(filter_name: &str, request_rules_yaml: &str) -> String {
        format!(
            r#"
static_resources:
  listeners:
    - name: ingress_http
      address:
        socket_address: {{ address: 0.0.0.0, port_value: 8080 }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: local
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: {filter_name}
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.header_to_metadata.v3.Config
                      request_rules:
{request_rules_yaml}
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
admin:
  address:
    socket_address: {{ address: 0.0.0.0, port_value: 9901 }}
"#
        )
    }

    #[test]
    fn header_to_metadata_empty_header_is_fatal() {
        let rules = r#"                      - header: ""
                        on_header_present:
                          key: tier"#;
        let yaml =
            bootstrap_with_header_to_metadata("envoy.filters.http.header_to_metadata", rules);
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::HeaderToMetadataInvalidRule { .. }),
            "expected HeaderToMetadataInvalidRule, got {err:?}"
        );
    }

    #[test]
    fn header_to_metadata_no_action_is_fatal() {
        let rules = r#"                      - header: x-tier"#;
        let yaml =
            bootstrap_with_header_to_metadata("envoy.filters.http.header_to_metadata", rules);
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::HeaderToMetadataInvalidRule { .. }),
            "expected HeaderToMetadataInvalidRule, got {err:?}"
        );
    }

    #[test]
    fn header_to_metadata_empty_key_is_fatal() {
        let rules = r#"                      - header: x-tier
                        on_header_present:
                          key: """#;
        let yaml =
            bootstrap_with_header_to_metadata("envoy.filters.http.header_to_metadata", rules);
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::HeaderToMetadataInvalidRule { .. }),
            "expected HeaderToMetadataInvalidRule, got {err:?}"
        );
    }

    #[test]
    fn header_to_metadata_empty_key_on_missing_is_fatal() {
        let rules = r#"                      - header: x-tier
                        on_header_missing:
                          key: ""
                          value: default"#;
        let yaml =
            bootstrap_with_header_to_metadata("envoy.filters.http.header_to_metadata", rules);
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::HeaderToMetadataInvalidRule { .. }),
            "expected HeaderToMetadataInvalidRule, got {err:?}"
        );
    }

    #[test]
    fn header_to_metadata_missing_without_value_is_fatal() {
        let rules = r#"                      - header: x-tier
                        on_header_missing:
                          key: tier"#;
        let yaml =
            bootstrap_with_header_to_metadata("envoy.filters.http.header_to_metadata", rules);
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::HeaderToMetadataInvalidRule { .. }),
            "expected HeaderToMetadataInvalidRule, got {err:?}"
        );
    }

    #[test]
    fn header_to_metadata_name_mismatch_is_unsupported() {
        let rules = r#"                      - header: x-tier
                        on_header_present:
                          key: tier"#;
        let yaml = bootstrap_with_header_to_metadata("envoy.filters.http.wrong_name", rules);
        let err = crate::parse_bootstrap(&yaml).unwrap_err();
        assert!(
            matches!(err, crate::ConfigError::UnsupportedHttpFilter { .. }),
            "expected UnsupportedHttpFilter for name mismatch, got {err:?}"
        );
    }
}

#[cfg(test)]
mod json_format_value_tests {
    //! Phase 39 T1 (ADR-0094 §A–§D) — recursive `JsonFormatValue` deserialization.
    use super::{JsonFormatValue, SubstitutionFormatString};

    #[test]
    fn json_format_recursive_deserialization() {
        let s: SubstitutionFormatString = serde_yaml::from_str(
            "json_format:\n  a: \"%PROTOCOL%\"\n  b: true\n  c: ~\n  d: { z: \"%REQ(:METHOD)%\", a: \"x\" }\n  e: [ \"%PROTOCOL%\", false ]\n",
        )
        .unwrap();
        let map = s.json_format.expect("json_format present");

        // string scalar → Format
        assert_eq!(map["a"], JsonFormatValue::Format("%PROTOCOL%".into()));
        // true/false → Bool (ADR-0094 §D)
        assert_eq!(map["b"], JsonFormatValue::Bool(true));
        // null/~ → Null (ADR-0094 §D)
        assert_eq!(map["c"], JsonFormatValue::Null);

        // nested map → Object; BTreeMap iterates keys SORTED (a before z) — §A
        match &map["d"] {
            JsonFormatValue::Object(inner) => {
                let keys: Vec<&String> = inner.keys().collect();
                assert_eq!(keys, vec!["a", "z"], "nested object keys sorted (a,z)");
                assert_eq!(inner["a"], JsonFormatValue::Format("x".into()));
                assert_eq!(inner["z"], JsonFormatValue::Format("%REQ(:METHOD)%".into()));
            }
            other => panic!("d should be Object, got {other:?}"),
        }

        // sequence → Array; ORDER PRESERVED (NOT sorted) — §B
        match &map["e"] {
            JsonFormatValue::Array(items) => {
                assert_eq!(
                    items,
                    &vec![
                        JsonFormatValue::Format("%PROTOCOL%".into()),
                        JsonFormatValue::Bool(false),
                    ]
                );
            }
            other => panic!("e should be Array, got {other:?}"),
        }
    }

    #[test]
    fn numeric_literal_leaf_is_rejected() {
        // CF-39-1 (ADR-0094 §D): a numeric scalar matches NO untagged arm → ERR.
        assert!(
            serde_yaml::from_str::<SubstitutionFormatString>("json_format:\n  n: 42\n").is_err(),
            "integer literal leaf must be boot-rejected (CF-39-1)"
        );
        assert!(
            serde_yaml::from_str::<SubstitutionFormatString>("json_format:\n  f: 1.5\n").is_err(),
            "float literal leaf must be boot-rejected (CF-39-1)"
        );
    }

    #[test]
    fn empty_top_map_still_valid() {
        // ADR-0094 §G: empty top-level map UNCHANGED.
        let s: SubstitutionFormatString =
            serde_yaml::from_str("json_format: {}\n").expect("empty map parses");
        assert!(s.json_format.unwrap().is_empty());
    }
}
