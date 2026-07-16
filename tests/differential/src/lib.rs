#![forbid(unsafe_code)]

//! Differential test harness for envoy-rust. Phase 01 surface: TCP echo + HTTP
//! admin GET.
//!
//! Contract: `run_fixture(fixture_dir)` starts upstream Envoy (via
//! testcontainers) and envoy-rust (via subprocess) against the fixture's paired
//! configs, then dispatches on `expectations.yaml`'s tagged `driver:` —
//! `tcp_echo` drives `inputs/payload.bin` via `drive_tcp`; `http_get` issues
//! a minimal `GET` via `drive_http_get`. Equivalence rules from `expectations`
//! are enforced by `assert_equivalence` (status-exact and/or byte-exact body).

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub mod access_log;
pub mod backend;
pub mod subject;
pub mod tls;
pub mod upstream;

/// Contents of `<fixture>/expectations.yaml`. See SPEC §D5.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    pub driver: Driver,
    #[serde(default)]
    pub equivalence: Equivalence,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Driver {
    TcpEcho,
    /// 66 NEW (ADR-0123): raw-TCP connect -> send NOTHING -> read to EOF.
    ///
    /// The harness's first read-to-EOF raw-TCP driver. `TcpEcho`/`drive_tcp`
    /// cannot express `direct_response`: it writes a payload and reads exactly
    /// `payload.len()` bytes back (ADR-0006/ADR-0007), whereas
    /// `direct_response` ignores client input and writes a payload of its own
    /// length before closing.
    TcpDirectResponse,
    /// 67.1 D7 (phase-67 SPEC R-8): a raw-TCP probe WITH a post-settle bilateral
    /// admin-stat scrape — the first `expected_stats` on any non-HTTP driver.
    ///
    /// **This variant exists because `ByteExact` cannot witness a DENY.**
    /// `assert_body_rule`'s `ByteExact` is a bare `envoy_body != rust_body`
    /// check, so a fixture asserting "both proxies returned zero bytes" passes
    /// vacuously even if envoy-rust never implemented the filter and simply
    /// failed to write. The stats assertion is what makes fixture `0072` a
    /// witness rather than a vacuous pass.
    ///
    /// `probe` selects the wire shape; both reuse the existing raw-TCP drivers:
    /// - `echo` -> `drive_tcp` (write `inputs/payload.bin`, read-exact, then the
    ///   ADR-0007 trailing-byte poll). Fixture `0073`.
    /// - `read_to_eof` -> `drive_tcp_direct_response` (send nothing, read to EOF).
    /// - `write_then_read_to_eof` -> `drive_tcp_write_then_read_to_eof` (send the
    ///   payload, read to EOF). Fixture `0072` — the DENY shape (ADR-0131).
    ///
    /// `expected_stats` are the DELTAS the probe must move each named counter by,
    /// not absolute values (ADR-0131) — `run_fixture`'s readiness connect
    /// perturbs per-connection counters on the data listener asymmetrically
    /// between the local subject and the containerized upstream.
    ///
    /// Requires `{{ADMIN_PORT}}` on BOTH sides (see `driver_needs_admin_port`).
    TcpWithStats {
        probe: TcpProbeKind,
        #[serde(default)]
        settle_ms: u64,
        #[serde(default)]
        expected_stats: Vec<KeepAliveExpectedStat>,
    },
    HttpGet {
        path: String,
        host: String,
    },
    /// 03.1 NEW: TLS round-trip with explicit SNI + optional CN/SAN check.
    TlsTcp {
        sni: String,
        #[serde(default)]
        expected_cn: Option<String>,
    },
    /// 03.2 NEW: drive a sequence of per-SNI TLS probes against a single
    /// listener address. Each probe runs a fresh TLS handshake (varying SNI),
    /// optionally asserts the presented leaf cert's CN/SAN matches
    /// `expected_cn` (DER-substring scan via `check_cn_or_san`), then writes
    /// `payload.bin` and reads-exact + ADR-0007 trailing-byte poll.
    /// Equivalence is enforced *inside* `drive_tls_probes` per probe (each
    /// side asserts byte-equality against the input payload + per-probe
    /// `expected_cn`); both sides succeeding ⇒ equivalent cert selection
    /// per SNI without a final `assert_equivalence` call.
    TlsTcpProbeList {
        probes: Vec<TlsTcpProbe>,
    },
    /// 04.1 NEW: drive an HTTP/1.1 request and assert the response shape.
    /// Async I/O lives in Task 14's `drive_http1`; this variant only carries
    /// grammar. Per SPEC §3 D5.
    Http1 {
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
    /// 04.2 NEW: drive a sequence of HTTP/1.1 probes against a single listener
    /// address. Each probe runs an independent request/response cycle and
    /// applies the per-probe equivalence cascade. Mirrors the established
    /// `TlsTcpProbeList` shape (03.2). Per SPEC §3 D3.
    Http1ProbeList {
        probes: Vec<Http1Probe>,
    },
    /// 06.2 NEW: HTTP/1.1 driver with post-request access-log line
    /// assertion. Drives one GET/POST via `drive_http1` (reused from
    /// 04.1), then reads the configured access-log files from each
    /// proxy and asserts per-token equivalence via
    /// `access_log::assert_access_log_lines_equivalent`.
    Http1WithAccessLog {
        method: String,
        path: String,
        host: String,
        expected_status: u16,
        // 08.1 Task 11: `BodyRule` grew at Task 10 (added two new struct-form
        // variants with 5 + 4 `Vec<String>` allow-list fields), and Task 11
        // shrunk the sibling `Driver::AdminScrape` variant (the inline
        // `path` / `expected_status` / `expected_content_type` / `expected_body_rule`
        // tuple moved into `AdminScrapeCase` carried via `Vec<AdminScrapeCase>`).
        // Together these tip `Driver` past clippy's `large_enum_variant`
        // threshold (Http1WithAccessLog is now ~285 bytes larger than the
        // second-largest variant). Boxing the body rule here is the
        // minimal-surface fix the clippy hint itself suggests.
        expected_body: Box<BodyRule>,
        expected_headers: HeaderRule,
        #[serde(default)]
        extra_headers: Vec<(String, String)>,
        expected_access_log_paths: AccessLogPaths,
        expected_access_log_lines: Vec<Vec<crate::access_log::AccessLogLineRule>>,
    },
    /// Phase 32 Task 6 (ADR-0079): whole-line byte-exact access-log
    /// differential. Drives a SEQUENCE of H1 probes (via the same
    /// `drive_http1` machinery `Http1WithAccessLog` uses) against a
    /// `direct_response` listener whose file access-logger carries a
    /// CUSTOM `log_format` of DETERMINISTIC command operators. After all
    /// probes complete, scrapes BOTH proxies' access-log files and asserts
    /// every emitted line is byte-identical via
    /// `access_log::assert_access_log_lines_byte_identical` (NOT the
    /// per-token default-format comparison). The fixture uses a
    /// `direct_response` route so `%UPSTREAM_HOST%` renders `-` on both
    /// sides — byte-identical with zero `{{BACKEND_IP}}` complexity.
    Http1AccessLogByteExact {
        // No Box needed: the `probes` `Vec` is already heap-indirected, so
        // this variant stays under clippy's `large_enum_variant` threshold
        // (unlike `Http1WithAccessLog`, which boxes its inline body rule).
        probes: Vec<AccessLogByteExactProbe>,
        expected_access_log_paths: AccessLogPaths,
    },
    /// Phase 56 (ADR-0113): the H2 sibling of `Http1AccessLogByteExact`.
    /// Drives a SEQUENCE of H2-prior-knowledge probes via `drive_http2`
    /// against an H2C listener whose file access-logger carries a CUSTOM
    /// `log_format`. After all probes complete, scrapes BOTH proxies'
    /// access-log files and asserts every emitted line is byte-identical via
    /// `access_log::assert_access_log_lines_byte_identical` — identical
    /// assertion machinery to the H1 sibling, only the wire driver differs.
    /// `drive_http2` currently supports GET/OPTIONS with no request body
    /// (see its `debug_assert!`); every probe's `body` field is therefore
    /// ignored on this arm (H2 fixtures needing a body must extend
    /// `drive_http2` first — none do as of this phase).
    Http2AccessLogByteExact {
        probes: Vec<AccessLogByteExactProbe>,
        expected_access_log_paths: AccessLogPaths,
    },
    /// 05.2 NEW: drive an HTTP/2 cleartext (H2C prior-knowledge) request and
    /// assert the response shape. Mirrors `Http1`'s shape; the `host` field
    /// becomes `:authority` on the H2 wire. Per SPEC §3 D5.
    Http2 {
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
    /// 11 NEW: drive a sequence of HTTP/2 probes against a single listener
    /// address. Each probe runs an independent H2 request/response cycle and
    /// applies the per-probe equivalence cascade. Mirrors `Http1ProbeList`
    /// (04.2) but drives over H2 via `drive_http2`. The `Http1Probe` struct is
    /// codec-agnostic (request shape + per-probe expectations) and is reused
    /// directly. Per phase-11 SPEC §3 D8.1.
    Http2ProbeList {
        probes: Vec<Http1Probe>,
    },
    /// 12.2 NEW: settle-then-drive H1 variant. Sleeps `settle_ms`
    /// past active-HC convergence (≥ `interval × unhealthy_threshold +
    /// timeout + margin`), then drives ONE Http1 request and applies the
    /// existing 5-axis equivalence cascade. The fixture asserts the
    /// post-convergence STEADY STATE, not a transient. Phase 12 does NOT
    /// opt into Timing tolerances (the settle_ms is a harness mechanic,
    /// not a compared latency bound — BEHAVIOR_CONTRACT.md §Timing).
    Http1AfterSettle {
        settle_ms: u64,
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
    /// Phase 69 (grpc_health_check, ADR-0138): the H2 sibling of
    /// `Http1AfterSettle`. Sleeps `settle_ms` past active-HC convergence,
    /// then drives ONE H2C prior-knowledge request via `drive_http2` and
    /// applies the same equivalence cascade. Fixture 0075 asserts the
    /// post-convergence 503 "no healthy upstream" steady state and omits
    /// `expected_headers` entirely — with `#[serde(default)]` that field
    /// deserializes to `None` and the header-axis check is skipped.
    Http2AfterSettle {
        settle_ms: u64,
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
    /// 13.1 D10: drive N sequential HTTP/1.1 requests over a SINGLE downstream
    /// keep-alive conn. The discriminating-observable shape per parent-13 §2
    /// item-iv: with separate per-request conns, upstream_cx_total: N. With
    /// this driver, upstream_cx_total: 1 (full pool reuse via the H1 pool
    /// landed at 13.1 Tasks 3-4). After all requests + a `settle_ms` sleep,
    /// scrapes named admin stats and asserts byte-equal bilaterally.
    ///
    /// The harness runs both proxies sequentially (upstream first, then
    /// subject), using a single TCP keep-alive conn per side. Each request
    /// drains its response (status line + headers + Content-Length body)
    /// so the next request on the same conn starts at a clean boundary.
    /// After both sides have driven all requests, the harness sleeps
    /// `settle_ms` (covers stat-write visibility per SPEC §6 signpost 11),
    /// then scrapes each named stat from BOTH admin listeners and asserts
    /// the value matches bilaterally.
    Http1KeepAlive {
        requests: Vec<Http1KeepAliveRequest>,
        #[serde(default)]
        settle_ms: u64,
        #[serde(default)]
        expected_stats: Vec<KeepAliveExpectedStat>,
        /// 18 Task 6 (ADR-0049): after the post-settle `expected_stats`
        /// scrape, run zero or more admin-listener scrape sub-cases through
        /// the SAME per-case assertion logic the `Driver::AdminScrape` arm
        /// uses (status + content-type + body-rule, each side independently;
        /// the shared `assert_admin_scrape_case` fn). Fixture 0026 uses this
        /// to assert `/config_dump`'s `ClustersConfigDump` reflects the
        /// file-based CDS load without adding a new `Driver` variant.
        /// `#[serde(default)]` so the existing fixtures (0020-0025) that
        /// omit the field deserialize unchanged to an empty vec.
        #[serde(default)]
        admin_scrapes: Vec<AdminScrapeCase>,
    },
    /// 13.2 Task 5 (ADR-0039 topology pivot): drive N sequential single-stream
    /// HTTP/2 requests over a SINGLE downstream H2 connection. The
    /// architectural sibling of `Http1KeepAlive` (13.1 D10), exercising the
    /// H2-pool surface end-to-end after the H2-listener × H1-cluster
    /// configuration was rejected at parse time by the 06.3 D14.3 gate per
    /// ADR-0028 (the H1-listener × H2-cluster path is deferred). With this
    /// driver and an H2 upstream cluster, `upstream_cx_total: 1` (one
    /// upstream H2 conn multiplexing N streams) — the discriminating
    /// observable matches the H1 sibling under the value-exact disposition.
    /// `upstream_cx_http2_total: 1` tracks the same site under the
    /// per-codec stat split (13.2 D7.2).
    ///
    /// The harness opens ONE TCP conn to each proxy's downstream H2 listener,
    /// runs `h2::client::handshake` to obtain a `SendRequest<Bytes>`, drives
    /// the H2 `Connection` future on a background tokio task, and for each
    /// request: clones `SendRequest`, builds an `http::Request<()>`, calls
    /// `send_request(req, /*end_of_stream=*/ true)` (GET-only — no body),
    /// awaits the `ResponseFuture`, drains the response body. Sequential
    /// (await each fully before the next) means N multiplexed streams share
    /// ONE downstream H2 conn — which exercises the upstream H2 pool's
    /// stream-multiplex path on the cluster side.
    ///
    /// `Http1KeepAliveRequest` + `KeepAliveExpectedStat` are reused directly:
    /// both substructs are codec-agnostic (method + path + host +
    /// expected_status; stat-name + value) — mirrors the 11 D8.1 precedent
    /// where `Driver::Http2ProbeList` reuses `Http1Probe` verbatim under the
    /// same codec-agnostic argument.
    Http2KeepAlive {
        requests: Vec<Http1KeepAliveRequest>,
        #[serde(default)]
        settle_ms: u64,
        #[serde(default)]
        expected_stats: Vec<KeepAliveExpectedStat>,
    },
    /// 06.1 D6.a: drive a sequence of HCM-side `PreRequest`s (so the registry
    /// has counters incremented), sleep ~50ms (per SPEC §6 signpost 11 to let
    /// Relaxed-ordered counter writes become visible), then perform one or
    /// more admin-listener scrapes and assert on each response. The
    /// per-sub-case `expected_body_rule` reuses the harness-level `BodyRule`
    /// enum so `BodyRule::PrometheusExposition` can drive metric-name-set
    /// equality modulo per-fixture allow-lists, while
    /// `BodyRule::JsonShape` + `BodyRule::TextLines` cover the 08.1
    /// `/config_dump` + `/server_info` + `/clusters` + `/listeners` family.
    ///
    /// 08.1 Task 11: widened from a SINGLE per-invocation `path` /
    /// `expected_*` tuple to a `Vec<AdminScrapeCase>` so fixture 0014 can
    /// scrape 4 admin endpoints in one fixture without adding a new
    /// `Driver` variant (architecture-decision lock-in #13). Fixture 0011
    /// migrated in lockstep to a single-element `scrapes:` list with no
    /// semantic change.
    ///
    /// 08.2 Task 7 (D16): widened again with `pre_admin_actions` (POST
    /// hooks issued against the admin listener BEFORE the scrape loop —
    /// fixture 0015's `/drain_listeners` trigger) and
    /// `post_admin_assertions` (wire-level invariants verified AFTER the
    /// scrape loop — fixture 0015's "data-plane refuses connections").
    /// Both default to empty `Vec` via `#[serde(default)]` so fixtures
    /// 0011 + 0014 (which declare neither field) carry forward
    /// unchanged.
    ///
    /// YAML field order: `pre_admin_actions` is declared BEFORE
    /// `pre_requests` (architecture-decision lock-in #18) so a reader of
    /// the YAML sees the drain trigger at the top of the block. The
    /// TEMPORAL dispatch order is independent — the dispatch fn body
    /// fires `pre_requests` FIRST (HCM-side traffic so the registry has
    /// counters incremented for the pre-drain baseline), then
    /// `pre_admin_actions` (the drain POSTs), then the `scrapes` loop
    /// (post-drain state assertions), then `post_admin_assertions`
    /// (wire-level "drained" assertion). This matches fixture 0015's
    /// natural shape: "verify pre-drain baseline → drain → verify
    /// post-drain state → wire-level assertion."
    AdminScrape {
        /// 08.2 Task 7 (D16): POST hooks issued against the admin
        /// listener BEFORE the scrape loop. Used by fixture 0015 to
        /// trigger `/drain_listeners` so the subsequent scrape +
        /// wire-level assertion observe the post-drain state.
        #[serde(default)]
        pre_admin_actions: Vec<AdminAction>,
        #[serde(default)]
        pre_requests: Vec<PreRequest>,
        scrapes: Vec<AdminScrapeCase>,
        /// 08.2 Task 7 (D16): wire-level invariants verified AFTER the
        /// scrape loop. Today's only variant
        /// (`DataPlaneConnectionRefused`) probes a data-plane listener
        /// address with a poll loop and accepts either ECONNREFUSED or
        /// an immediate-EOF connect as evidence the listener is
        /// drained.
        #[serde(default)]
        post_admin_assertions: Vec<AdminAssertion>,
    },
    /// 26 Task 7: an RDS hot-reload differential step. Runs `pre_probes` (bilateral
    /// equivalence), then atomic-renames the post-reload RDS content over the watched
    /// path on BOTH sides (subject host file + upstream container file), waits — bounded —
    /// for both proxies to converge on the new table (polling `reload.discriminator`,
    /// NOT a fixed sleep), then runs `post_probes` (bilateral equivalence). The
    /// post-reload differential is NATIVE-Linux-CI-authoritative (the upstream reload is
    /// unobservable under Docker Desktop virtiofs); the harness mechanics are unit-tested locally.
    Http1RdsReload {
        pre_probes: Vec<Http1Probe>,
        reload: RdsReloadStep,
        post_probes: Vec<Http1Probe>,
    },
    /// 27 Task 6 (D6 / §6.2-LOCKED V2 / ADR-0068): an EDS hot-reload differential
    /// step — the EDS sibling of `Http1RdsReload`. Runs `pre_probes` (bilateral
    /// equivalence on backend_1), atomic-renames the post-reload EDS content
    /// (the endpoint swapped `[backend_1]` → `[backend_2]`) over the watched path
    /// on BOTH sides, waits — bounded — for both proxies to converge on the new
    /// endpoint set (polling `reload.discriminator`, NOT a fixed sleep), then
    /// runs `post_probes` (bilateral equivalence on backend_2). The
    /// discriminating observable is the per-backend body marker (`backend: backend_2`)
    /// OR `cluster.eds_backend.update_success` advancing. The post-reload
    /// differential is NATIVE-Linux-CI-authoritative (the upstream container
    /// reload is unobservable under Docker Desktop virtiofs); the harness
    /// mechanics are unit-tested locally. Folds the phase-26 M26-2 fix: the
    /// dispatch arm `bail!`s if the discriminator has neither `expected_status`
    /// nor `expected_body` (a both-None discriminator would report spurious
    /// instant convergence — see `eds_reload_discriminator_is_load_bearing`).
    Http1EdsReload {
        pre_probes: Vec<Http1Probe>,
        reload: EdsReloadStep,
        post_probes: Vec<Http1Probe>,
    },
    /// 28 Task 7 (ADR-0070): RING_HASH consistent-hashing LB cross-proxy
    /// differential. Sweeps a list of `x-hash-key` header values against BOTH
    /// proxies through a STATIC `lb_policy: RING_HASH` cluster with two
    /// distinguishable echo backends (`--body-marker backend_1`/`backend_2`),
    /// extracting each response body's leading `backend: <marker>\n` line as
    /// the selected-backend discriminator. Asserts THREE properties:
    ///
    ///   STRONG (the core differential): for each key the marker chosen by
    ///   envoy-rust is IDENTICAL to the one chosen by upstream Envoy
    ///   (cross-proxy identical RING_HASH selection — the locked xxHash64 ring
    ///   reproduced end-to-end against the real Envoy).
    ///
    ///   SPREAD: over the full sweep BOTH markers appear on EACH side (the ring
    ///   actually distributes; a sweep that collapses to one backend fails —
    ///   it would not prove ring selection).
    ///
    ///   STABILITY: a repeated key yields the SAME marker on each proxy
    ///   (same-key → same-backend; each key is probed twice).
    ///
    /// This differential is LOCALLY observable (a plain request/response with
    /// NO file-watch/reload trigger), so the Docker test runs and is
    /// authoritative on any host with a Docker daemon.
    Http1HashSweep {
        /// The `x-hash-key` header values to sweep. Each is sent (twice, for
        /// the stability check) to both proxies; the response body marker is
        /// compared cross-proxy per key.
        keys: Vec<String>,
        /// Request path (e.g. `/`). Routed to the RING_HASH cluster.
        path: String,
        /// Request `Host` header value.
        host: String,
        /// Expected status for every probe on both sides (e.g. 200).
        expected_status: u16,
    },
    /// 30 Task 7 (ADR-0074): route-selection differential for subset LB. Drives
    /// a list of distinct PATHS (each route carries a `metadata_match`) against
    /// BOTH proxies; asserts cross-proxy identical backend selection by the
    /// `backend: <marker>` body line (STRONG), plus the NO_FALLBACK 503 probe.
    ///
    /// Unlike `Http1HashSweep` the discriminator is the ROUTE (path), not an
    /// `x-hash-key` header: each path's route carries a `metadata_match` that
    /// narrows the STATIC `subset_cluster`'s eligible endpoint set to the subset
    /// whose endpoint `metadata` matches. Per 200 probe: STRONG — the marker
    /// chosen by envoy-rust is IDENTICAL to upstream Envoy's AND equals the §A
    /// oracle marker. The 503 probe (a `metadata_match` resolving to NO subset
    /// under `NO_FALLBACK`) asserts each side returns 503 with the fixed
    /// 19-byte `no healthy upstream` local-reply body (byte-equal cross-proxy).
    ///
    /// LOCALLY observable (a plain request/response with NO file-watch/reload
    /// trigger), so the Docker test runs and is authoritative on any host with a
    /// Docker daemon.
    Http1RouteSelect {
        /// The distinct paths to drive. Each is sent (GET, Host: localhost) to
        /// both proxies; the response status + body marker are compared
        /// cross-proxy per probe.
        probes: Vec<RouteSelectProbe>,
    },
}

/// 30 Task 7 (ADR-0074): one probe of `Driver::Http1RouteSelect`. A path whose
/// route carries a `metadata_match`, the status both proxies must return, and —
/// for a 200 probe — the `backend: <marker>` body line the §A oracle expects.
/// A `None` `expected_marker` marks the NO_FALLBACK 503 probe (whose body is
/// asserted to be the fixed `no healthy upstream` local reply instead).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RouteSelectProbe {
    /// Human-readable probe name (for assertion messages), e.g. `prod-route`.
    pub name: String,
    /// Request path, e.g. `/prod`. Routed (by prefix) to `subset_cluster` with
    /// a route-carried `metadata_match`.
    pub path: String,
    /// Status both proxies must return for this path (200 for a resolved
    /// subset; 503 for the NO_FALLBACK no-subset probe).
    pub expected_status: u16,
    /// The §A oracle `backend: <marker>` body line for a 200 probe; `None` for
    /// the 503 probe (whose body is the fixed `no healthy upstream` local reply).
    #[serde(default)]
    pub expected_marker: Option<String>,
}

/// 26 Task 7: the reload directive inside `Driver::Http1RdsReload`. Carries the
/// post-reload RDS template path (rendered per-side like `rds.yaml`), the
/// convergence-wait bound, and the discriminating probe whose NEW-table response
/// signals each side has converged.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RdsReloadStep {
    /// Fixture-relative file holding the POST-reload RDS content, rendered per-side
    /// exactly like `rds.yaml`. Default `rds-reload.yaml`.
    #[serde(default = "default_reload_file")]
    pub reload_file: String,
    /// Bound (ms) for the wait-for-convergence poll. Generous slack over the ~50 ms
    /// Task-1 settle latency (e.g. fixtures set 5000).
    pub settle_budget_ms: u64,
    /// The discriminating probe polled (each side) until its response reflects the NEW
    /// table — its `expected_status` / `expected_body` define "converged". Bounded by
    /// `settle_budget_ms`.
    pub discriminator: Http1Probe,
}

fn default_reload_file() -> String {
    "rds-reload.yaml".to_string()
}

/// 27 Task 6 (D6 / §6.2-LOCKED V2 / ADR-0068): the reload directive inside
/// `Driver::Http1EdsReload`. The EDS sibling of `RdsReloadStep`. Carries the
/// post-reload EDS template path (rendered per-side like `eds.yaml`), the
/// convergence-wait bound (~8 ms settle with generous slack, e.g. fixtures set
/// 2000), and the discriminating probe whose POST-swap response (the
/// `backend: backend_2` body marker, or an advanced `update_success`) signals
/// each side has converged.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EdsReloadStep {
    /// Fixture-relative file holding the POST-reload EDS content (the
    /// `ClusterLoadAssignment` with the swapped endpoint), rendered per-side
    /// exactly like `eds.yaml`. Default `eds-reload.yaml`.
    #[serde(default = "default_eds_reload_file")]
    pub reload_file: String,
    /// Bound (ms) for the wait-for-convergence poll. Generous slack over the
    /// ~8 ms settle latency (e.g. fixtures set 2000).
    pub settle_budget_ms: u64,
    /// The discriminating probe polled (each side) until its response reflects
    /// the SWAPPED endpoint — its `expected_status` / `expected_body` define
    /// "converged". Bounded by `settle_budget_ms`. MUST be load-bearing (carry
    /// at least one of `expected_status` / `expected_body`) — the dispatch arm
    /// `bail!`s otherwise (M26-2 guard, see `eds_reload_discriminator_is_load_bearing`).
    pub discriminator: Http1Probe,
}

fn default_eds_reload_file() -> String {
    "eds-reload.yaml".to_string()
}

/// 27 Task 6 (D6): the M26-2 spurious-convergence guard. A reload discriminator
/// with NEITHER `expected_status` NOR `expected_body` makes
/// `wait_for_reload_convergence` return Ok on the FIRST poll (`status_ok &&
/// body_ok == true` when both expectations are absent), reporting instant
/// "convergence" before the reload took effect (the phase-26 M26-2 trap — the
/// RDS arm did not guard this). The `Http1EdsReload` dispatch arm calls this at
/// its START and `bail!`s when it returns `false`. A discriminator is
/// load-bearing iff it carries at least one expectation.
pub fn eds_reload_discriminator_is_load_bearing(discriminator: &Http1Probe) -> bool {
    discriminator.expected_status.is_some() || discriminator.expected_body.is_some()
}

/// 08.1 Task 11: one admin-listener sub-case inside `Driver::AdminScrape`.
/// The `path` + `expected_*` tuple was previously inline on the variant
/// (06.1 single-scrape shape); fixture 0014 needs 4 sub-cases against the
/// same proxy invocation, so the per-sub-case tuple moves into a dedicated
/// struct that `Driver::AdminScrape` carries as `Vec<AdminScrapeCase>`.
///
/// `expected_content_type` is matched case-insensitively by
/// `check_content_type` (e.g. envoy ↔ envoy-rust may format media-type
/// parameters differently; the harness elides the difference).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AdminScrapeCase {
    pub path: String,
    pub expected_status: u16,
    pub expected_content_type: String,
    pub expected_body_rule: BodyRule,
}

/// 06.1 D6.a: minimal HTTP/1.1 request shape used by `Driver::AdminScrape` to
/// drive the HCM listener before scraping the admin listener. `port_key`
/// names the template marker (e.g. `"PORT"`) whose substituted address the
/// pre-request is sent to — fixtures may eventually drive multiple listeners
/// from one scrape, but 06.1 only uses the single HCM listener under
/// `{{PORT}}`.
///
/// `method` is held as `String` (not `Http1Method`) per the PLAN's grammar
/// projection; the dispatch arm converts it to `Http1Method` at drive time
/// (only `GET` is supported in 06.1; future methods widen the conversion).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreRequest {
    pub method: String,
    pub path: String,
    pub host: String,
    pub port_key: String,
}

/// 13.1 D10: one HTTP/1.1 request in a `Driver::Http1KeepAlive` sequence.
/// Carries the request shape (`method` + `path` + `host`) and the
/// per-request `expected_status` the harness asserts before reading the
/// next request from the same keep-alive conn. `expected_status` is
/// REQUIRED (not `Option<u16>` like the single-shot `Driver::Http1`) —
/// the per-class counter discriminator fixture 0020 asserts the response
/// status mapping (2xx/3xx/4xx/5xx) so a hung response or class mismatch
/// surfaces at the request boundary rather than only at the post-settle
/// stat scrape.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Http1KeepAliveRequest {
    pub method: String,
    pub path: String,
    pub host: String,
    pub expected_status: u16,
    /// 14.2 D8.1a (SPEC correction B-3): optional per-request body-byte
    /// assertion. Each side's response body independently must equal these
    /// bytes. Reuses the existing `Http1BodyRule::ByteExact { body }`. Omit
    /// (the shape fixtures 0020/0021 use) to assert nothing about the body.
    #[serde(default)]
    pub expected_body: Option<Http1BodyRule>,
    /// 14.2 D8.1a: assert this (lower-cased) header NAME is PRESENT on each
    /// side's response. Only PRESENCE is asserted, not the value — e.g.
    /// `x-envoy-upstream-service-time` differs per proxy but must exist on
    /// both, matching the BEHAVIOR_CONTRACT allow-list disposition.
    #[serde(default)]
    pub require_header_present: Option<String>,
    /// 14.2 D8.1a: assert this (lower-cased) header NAME is ABSENT on each
    /// side's response. The bilateral counterpart of `require_header_present`
    /// for responses (e.g. local-reply 503s) that must NOT carry the header.
    #[serde(default)]
    pub require_header_absent: Option<String>,
    /// 16 Task 7 (fixture 0024): assert this (lower-cased) header NAME is
    /// present on each side's response AND its value equals the given string.
    /// Value-exact counterpart of `require_header_present` — used for
    /// `x-envoy-attempt-count: 2`, where presence-only is insufficient (the
    /// retry behaviour must yield exactly 2 attempts, not merely the header).
    /// Serializes as `{ name: x-envoy-attempt-count, value: "2" }`.
    #[serde(default)]
    pub require_header_value: Option<Http1HeaderValueRule>,
}

/// 16 Task 7: value-exact header assertion for `Http1KeepAliveRequest`. The
/// header `name` (compared case-insensitively) must be present and its value
/// must equal `value` byte-for-byte on each side's response.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Http1HeaderValueRule {
    pub name: String,
    pub value: String,
}

/// 13.1 D10: bilateral stat assertion for
/// `Driver::Http1KeepAlive::expected_stats`. Each entry names an admin-
/// listener stat (e.g. `cluster.backend_cluster.upstream_cx_total`) and
/// the exact integer value BOTH proxies must emit at scrape time. The
/// harness scrapes both `/stats` endpoints after the post-request
/// `settle_ms` and asserts each side's value equals `value` independently
/// — cross-side consistency follows from transitivity.
/// 67.1 D7: which raw-TCP wire shape `Driver::TcpWithStats` drives. Both arms
/// delegate to a pre-existing driver function; no new wire driver is written.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TcpProbeKind {
    /// Write `inputs/payload.bin`, read exactly that many bytes back, then poll
    /// for trailing bytes (ADR-0006 / ADR-0007). The ALLOW shape.
    Echo,
    /// Send nothing; read to EOF. Used where the peer speaks first.
    ReadToEof,
    /// Write `inputs/payload.bin`, then read to EOF. The DENY shape (ADR-0131):
    /// upstream Envoy evaluates network RBAC on the FIRST DOWNSTREAM BYTE, so
    /// the probe must send one; on DENY the peer answers with zero bytes and a
    /// clean EOF, discarding what was sent. Fixture `0072`.
    WriteThenReadToEof,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeepAliveExpectedStat {
    pub name: String,
    pub value: u64,
}

/// 08.2 Task 7 (D16): a single admin-side action issued BEFORE the
/// `Driver::AdminScrape` scrape loop. Today only `Post` is supported;
/// the variant carries the admin-listener path to POST against and the
/// expected response status. Internally-tagged on `kind` (e.g.
/// `{ kind: post, path: /drain_listeners, expected_status: 200 }`) so
/// future variants slot in without re-shaping the YAML.
///
/// `path` is GET-formatted (`/foo`, NOT `http://host/foo`); the helper
/// `drive_admin_post` issues `POST <path> HTTP/1.1\r\nHost: admin.local\r\n…`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdminAction {
    /// `POST <path>` against the admin listener; the response status
    /// MUST equal `expected_status` or the dispatch arm bails with a
    /// descriptive error.
    Post { path: String, expected_status: u16 },
}

/// 08.2 Task 7 (D16): a wire-level invariant verified AFTER the
/// `Driver::AdminScrape` scrape loop. Today only
/// `DataPlaneConnectionRefused` is supported; the variant carries the
/// data-plane listener address to probe and a `within_ms` budget for
/// the poll loop. Internally-tagged on `kind` so future variants slot
/// in without re-shaping the YAML.
///
/// `within_ms` is `u64` (raw milliseconds) per architecture-decision
/// lock-in #19 — adding `humantime-serde` would be a new top-level
/// Cargo dep and is rejected at this phase.
///
/// `DataPlaneConnectionRefused` succeeds on EITHER ECONNREFUSED (the
/// kernel-level "no listener" signal that the listener fd has been
/// dropped) OR an immediate-EOF connect (the in-flight-drain shape:
/// kernel still accepts because the listening fd is alive on this
/// side but server-side immediately FINs the accepted socket). Both
/// dispositions are accepted as evidence the listener is drained.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdminAssertion {
    /// Probe `listener_address` (e.g. `127.0.0.1:8080`) in 100ms
    /// intervals until `within_ms` elapses; succeed on the first
    /// observation of ECONNREFUSED OR immediate EOF.
    DataPlaneConnectionRefused {
        listener_address: String,
        within_ms: u64,
    },
}

/// One TLS-SNI probe entry inside `Driver::TlsTcpProbeList`. SPEC §D6.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TlsTcpProbe {
    pub sni: String,
    #[serde(default)]
    pub expected_cn: Option<String>,
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Equivalence {
    #[serde(default)]
    pub response_status: Option<StatusRule>,
    #[serde(default)]
    pub response_body: Option<BodyRule>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StatusRule {
    Exact,
}

/// Body equivalence rule.
///
/// 06.1 Task 12 extended this enum with the struct-form
/// `BodyRule::PrometheusExposition` variant. To accommodate struct-form
/// variants alongside the original unit-form `ByteExact`, the serde
/// representation switched from externally-tagged-with-rename_all to
/// internally-tagged via `#[serde(tag = "kind", rename_all = "snake_case")]`,
/// matching the existing `Driver` enum's shape.
///
/// Wire-shape consequence: existing fixtures (0001-0010) had
/// `response_body: byte_exact`; under the new shape they read
/// `response_body: { kind: byte_exact }`. All 10 fixtures were updated in
/// the same commit; no other fixture grammar change ships with 06.1.
///
/// 08.1 Task 10 (D15) extended this enum with `JsonShape` + `TextLines`
/// struct-form variants for the `/config_dump` + `/clusters` + `/listeners`
/// admin-endpoint diff territory. `JsonShape::required_subtree` carries a
/// `serde_yaml::Value` (via `JsonSubtreeRule`); `serde_yaml::Value` does
/// implement `Eq` (per `serde_yaml` 0.9.34+deprecated source), so the
/// existing `#[derive(... PartialEq, Eq)]` on `BodyRule` is retained
/// unchanged — no cascade drops on `Driver` / `Equivalence` / `Expectations`
/// are needed.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BodyRule {
    ByteExact,
    /// 06.1 D6.b: parse the body as Prometheus text-exposition format and
    /// assert the metric-name set is equal between envoy and envoy-rust
    /// modulo the per-fixture allow-lists. 06.3 extends with the three fields
    /// below (D18.3 + signpost 9 option (1)). The allow-lists are populated
    /// empirically per SPEC §6 signpost 12 (Task 13's territory); both default
    /// to empty so a fixture can declare the rule without pre-seeding.
    PrometheusExposition {
        /// Metric names emitted by upstream Envoy that envoy-rust does not.
        #[serde(default)]
        allowlist_envoy_only: Vec<String>,
        /// Metric names emitted by envoy-rust that upstream Envoy does not.
        #[serde(default)]
        allowlist_envoy_rust_only: Vec<String>,
        /// 06.3 NEW: each pair `(stat_name, expected_value)` must match
        /// exactly on BOTH proxies' scrapes. Pairs are `Vec` (not HashMap)
        /// for deterministic ordering in error messages.
        #[serde(default)]
        value_exact: Vec<(String, u64)>,
        /// 06.3 NEW: each stat name must equal 0 on BOTH proxies' scrapes
        /// (terminal-zero gauges; e.g., listener.<name>.downstream_cx_active
        /// after the test's connections have closed).
        #[serde(default)]
        value_must_be_zero: Vec<String>,
        /// 06.3 NEW: each stat name must be present on BOTH proxies'
        /// scrapes; value may differ (for stats with disposition
        /// "name-required, value-may-differ" per BEHAVIOR_CONTRACT.md).
        #[serde(default)]
        value_present_only: Vec<String>,
    },
    /// 08.1 Task 10 (D15) + Task 11 strictness wiring: parse BOTH bodies
    /// as JSON objects and assert schema-level invariants without
    /// requiring byte-for-byte equality. Used by `/config_dump` +
    /// `/server_info` and similar JSON-emitting admin endpoints where
    /// Envoy's emission carries fields envoy-rust does not (and vice
    /// versa).
    ///
    /// Fail strictness (Task 11):
    /// - `required_keys`: every key MUST appear on the top-level JSON
    ///   object on BOTH sides.
    /// - `required_subtree`: walk `path` (dotted-segment selector) on
    ///   both bodies AND assert `envoy_sub == expected` AND
    ///   `rust_sub == expected` (after JSON-string normalization). The
    ///   `envoy_sub == rust_sub` cross-side consistency check follows
    ///   from transitivity (both equal the same expected).
    /// - Top-level keys present on only the envoy side that are NOT in
    ///   `allowlist_envoy_only_keys` AND NOT in `value_may_differ_keys`
    ///   MUST also appear on the envoy-rust side; symmetrically for
    ///   rust-only keys vs `allowlist_envoy_rust_only_keys`.
    /// - Top-level keys present on BOTH sides and NOT in
    ///   `value_may_differ_keys` MUST serialize equal (the addressed
    ///   sub-values rendered via `serde_json::to_string`).
    ///
    /// All allow-lists are top-level-keys-only — nested keys are
    /// expressed via `required_subtree.path`. This is sufficient for
    /// 08.1's admin-endpoint diff territory; if a future endpoint needs
    /// per-nested-key control, extend the strictness model at that point.
    JsonShape {
        #[serde(default)]
        required_keys: Vec<String>,
        #[serde(default)]
        required_subtree: Option<JsonSubtreeRule>,
        #[serde(default)]
        allowlist_envoy_only_keys: Vec<String>,
        #[serde(default)]
        allowlist_envoy_rust_only_keys: Vec<String>,
        /// Shared keys whose values may differ bilaterally; presence is required, value equality is not. (08.1 REVIEW M2 closure landed at 08.2 D16.)
        #[serde(default)]
        value_may_differ_keys: Vec<String>,
    },
    /// 08.1 Task 10 (D15) + Task 11 strictness wiring: treat both bodies
    /// as UTF-8 text and assert per-line invariants. Used by `/clusters`
    /// and `/listeners` and other line-oriented admin endpoints.
    ///
    /// Fail strictness (Task 11):
    /// - `required_lines`: every entry MUST appear verbatim on BOTH
    ///   sides.
    /// - `required_line_prefixes`: every entry MUST be the prefix of at
    ///   least one line on BOTH sides (covers varying-suffix lines
    ///   like `listener_0::counter_<dynamic>`).
    /// - Lines present on only the envoy side that are NOT in
    ///   `allowlist_envoy_only_lines` AND that do NOT start with any
    ///   prefix in `allowlist_envoy_only_line_prefixes` MUST also
    ///   appear on the envoy-rust side; symmetrically for rust-only
    ///   lines.
    ///
    /// The `*_line_prefixes` allow-list family (Task 11 NEW) absorbs
    /// address-bearing per-side lines whose suffix varies per fixture
    /// run (e.g. fixture 0014's `/clusters` per-endpoint counter lines
    /// like `backend::192.168.65.254:<ephemeral>::cx_active::0`, or
    /// `/listeners` `ingress_http::<addr>:<port>` per-side line shape).
    /// A line is allow-listed if it appears verbatim in the exact-line
    /// allow-list OR starts with any entry in the prefix allow-list.
    TextLines {
        #[serde(default)]
        required_lines: Vec<String>,
        #[serde(default)]
        required_line_prefixes: Vec<String>,
        #[serde(default)]
        allowlist_envoy_only_lines: Vec<String>,
        #[serde(default)]
        allowlist_envoy_rust_only_lines: Vec<String>,
        /// 08.1 Task 11 NEW: per-side line-prefix allow-list. Absorbs
        /// address-bearing varying-suffix lines that cannot be enumerated
        /// verbatim because the port/IP/timestamp segment shifts per
        /// fixture run.
        #[serde(default)]
        allowlist_envoy_only_line_prefixes: Vec<String>,
        #[serde(default)]
        allowlist_envoy_rust_only_line_prefixes: Vec<String>,
    },
}

/// 08.1 Task 10 (D15) + Task 11 strictness wiring: helper for
/// `BodyRule::JsonShape::required_subtree`.
///
/// `path` is a dotted-segment selector walked via `walk_pointer`; each
/// segment is interpreted as an `usize` array index if it parses as one,
/// else as an object key. Example: `configs.0.bootstrap.node.id` selects
/// the `id` field of the `node` object on the first element of the
/// `configs` array of a `/config_dump`-shaped payload.
///
/// `expected` is a `serde_yaml::Value` (so fixture YAML can declare the
/// expected sub-value in YAML-native form) and is converted to a
/// `serde_json::Value` at assertion time (`serde_json::to_value` on the
/// `serde_yaml::Value`) so it can be JSON-string-compared against the
/// addressed sub-value on each side.
///
/// Task 11 strictness wiring: BOTH `envoy_sub` and `rust_sub` are asserted
/// equal to `expected` (Task 10 only checked `envoy_sub == rust_sub`,
/// which would silently accept a bilateral drift away from the documented
/// shape). `PartialEq` + `Eq` are derived — `serde_yaml::Value` does
/// implement `Eq` (per `serde_yaml` 0.9.34+deprecated source), so the
/// `BodyRule::JsonShape` enclosing `#[derive(PartialEq, Eq)]` propagates
/// cleanly through `Option<JsonSubtreeRule>`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct JsonSubtreeRule {
    /// Dotted-path key, e.g. `configs.0.bootstrap.node.id`. Shared default;
    /// overridden per-side when `path_envoy` / `path_envoy_rust` is present.
    #[serde(default)]
    pub path: String,
    /// 20 Task 6 (ADR-0052): per-side path override. When present, overrides
    /// `path` for that proxy only — the `RoutesConfigDump` entry lands at
    /// `configs[4]` on Envoy (which interposes ScopedRoutes/Secrets sections +
    /// an always-on `RoutesConfigDump` that envoy-rust does not emit) vs
    /// `configs[2/3]` on envoy-rust, so a fixed shared index cannot match both.
    /// Mirrors the per-side allow-list mechanism used elsewhere.
    #[serde(default)]
    pub path_envoy: Option<String>,
    #[serde(default)]
    pub path_envoy_rust: Option<String>,
    pub expected: serde_yaml::Value,
}

impl JsonSubtreeRule {
    /// 20 Task 6 (ADR-0052): the dotted path to walk on the Envoy (upstream)
    /// side — the per-side override if present, else the shared `path`.
    pub fn envoy_path(&self) -> &str {
        self.path_envoy.as_deref().unwrap_or(&self.path)
    }
    /// 20 Task 6 (ADR-0052): the dotted path to walk on the envoy-rust
    /// (subject) side — the per-side override if present, else the shared `path`.
    pub fn rust_path(&self) -> &str {
        self.path_envoy_rust.as_deref().unwrap_or(&self.path)
    }
}

/// 08.1 Task 10 (D15) helper: walk a dotted-path selector through a
/// `serde_json::Value`. Each segment becomes an `usize` array index if it
/// parses as one, else an object key. Returns a borrow of the addressed
/// sub-value or an `anyhow::Error` naming the offending segment if the
/// path does not resolve.
fn walk_pointer<'a>(
    value: &'a serde_json::Value,
    dotted_path: &str,
) -> Result<&'a serde_json::Value> {
    // 08.1 REVIEW M4 closure landed at 08.2 D16: reject dotted paths
    // containing empty segments (e.g. `a..b`, `a.b.`, `.foo`) with a
    // structured error naming the offending path; the existing
    // "key not found: " message is opaque under this shape because
    // serde_json::Value::get("") silently returns None.
    if dotted_path.split('.').any(str::is_empty) {
        bail!("walk_pointer: dotted path contains empty segment: {dotted_path:?}");
    }
    let mut cur = value;
    for seg in dotted_path.split('.') {
        cur = if let Ok(idx) = seg.parse::<usize>() {
            cur.get(idx)
                .with_context(|| format!("array index out of range: {seg}"))?
        } else {
            cur.get(seg)
                .with_context(|| format!("key not found: {seg}"))?
        };
    }
    Ok(cur)
}

/// 06.1 D6.b: parse a Prometheus text-exposition body into the set of metric
/// names. Skips `#`-prefixed lines (HELP / TYPE comments) and blank lines;
/// for sample lines, extracts the leading whitespace-or-`{`-delimited token.
/// Returns a `BTreeSet` for deterministic ordering when failure messages are
/// constructed.
pub fn parse_prometheus_metric_names(body: &[u8]) -> std::collections::BTreeSet<String> {
    let s = std::str::from_utf8(body).unwrap_or("");
    let mut out = std::collections::BTreeSet::new();
    for line in s.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        // Sample line shape: `<name>{<labels>} <value>` or `<name> <value>`.
        let name_end = t
            .find(|c: char| c.is_whitespace() || c == '{')
            .unwrap_or(t.len());
        let name = &t[..name_end];
        if !name.is_empty() {
            out.insert(name.to_string());
        }
    }
    out
}

/// 06.3 D18.3: parse Prometheus text-exposition body into name → value
/// pairs. Skips `#`-prefixed lines + blanks. For sample lines, extracts
/// the leading name (up to whitespace or `{`) and the trailing value
/// (parses as `u64`; non-parseable values silently skipped). Returns
/// `BTreeMap` for deterministic ordering when failure messages are
/// constructed. Labels (e.g., `metric{key="value"} 42`) are dropped —
/// the value-side of value_exact / value_must_be_zero / value_present_only
/// asserts only on the bare-name → value projection.
pub fn parse_prometheus_samples(body: &[u8]) -> std::collections::BTreeMap<String, u64> {
    let s = std::str::from_utf8(body).unwrap_or("");
    let mut out = std::collections::BTreeMap::new();
    for line in s.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let name_end = t
            .find(|c: char| c.is_whitespace() || c == '{')
            .unwrap_or(t.len());
        let name = &t[..name_end];
        if name.is_empty() {
            continue;
        }
        let after_name = &t[name_end..];
        let after_labels = if let Some(rest) = after_name.strip_prefix('{') {
            match rest.find('}') {
                Some(close_idx) => &rest[close_idx + 1..],
                None => continue,
            }
        } else {
            after_name
        };
        let value_str = after_labels.trim();
        let value_field = value_str.split_whitespace().next().unwrap_or("");
        if let Ok(v) = value_field.parse::<u64>() {
            out.insert(name.to_string(), v);
        }
    }
    out
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Http1Method {
    Get,
    /// Phase-23 NEW: OPTIONS is required by the CORS preflight probe (fixture
    /// 0031). The harness builds the request line from `method.as_str()` so
    /// `OPTIONS / HTTP/1.1` is emitted on the wire; no other behaviour changes.
    Options,
    /// Phase-24 NEW: POST is required by the CSRF modify-method probes (fixture
    /// 0032). The H1 driver builds the request line from `method.as_str()`; POST
    /// probes carry no request body (the CSRF guard is header-only). POST is
    /// never driven over H2 this phase — fixture 0032 is H1-only, so
    /// `drive_http2`'s `matches!(GET | OPTIONS)` debug_assert stays unwidened.
    Post,
}

impl Http1Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Http1Method::Get => "GET",
            Http1Method::Options => "OPTIONS",
            Http1Method::Post => "POST",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Http1BodyRule {
    /// Body must equal this string's UTF-8 bytes exactly (the expected value
    /// comes from the fixture's expectations.yaml). Distinct from the
    /// harness-level `BodyRule::ByteExact` which compares envoy ↔ envoy-rust
    /// outputs.
    ///
    /// Field is `String` rather than `Vec<u8>` because serde's default YAML
    /// deserialization of `Vec<u8>` rejects YAML scalar strings (it expects a
    /// sequence of integer bytes). 04.x bodies are always text — the string
    /// form keeps fixture YAML readable (e.g. `body: "ok\n"`). If a future
    /// fixture needs to assert on non-UTF-8 bytes, switch to `serde_bytes` or
    /// add a string-to-bytes deserializer at that point.
    ByteExact { body: String },
    // 04.3 may add ByteExactWithRequestEcho — for the http1-echo-server's
    // deterministic echo response shape.
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Http1HeaderRule {
    SetEqualModuloAllowList,
}

/// 06.2 NEW: header equivalence rule for `Driver::Http1WithAccessLog`.
/// Internally-tagged under `tag = "rule"` so the fixture YAML reads
/// `expected_headers: { rule: set_equal_modulo_allow_list }`, paralleling
/// `BodyRule`'s `tag = "kind"` shape. Distinct from `Http1HeaderRule`
/// (externally-tagged unit variant, used by the 04.x drivers).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "rule", rename_all = "snake_case", deny_unknown_fields)]
pub enum HeaderRule {
    SetEqualModuloAllowList,
}

/// 06.2 NEW: per-proxy file paths for access-log diff. The harness
/// reads both files after the wire-protocol leg completes.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccessLogPaths {
    pub envoy: String,
    pub envoy_rust: String,
}

/// Phase 32 Task 6 (ADR-0079): one probe inside
/// `Driver::Http1AccessLogByteExact`. Each probe drives one H1 request
/// through both proxies (via `drive_http1`); the emitted access-log line
/// is later compared whole-line byte-exact. `extra_headers` lets a probe
/// exercise request-header command operators (e.g. `%REQ(USER-AGENT)%`,
/// `%REQ(X-FORWARDED-FOR)%`) deterministically. `expected_status` defaults
/// to 200 (the `direct_response` status used by fixture 0040).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccessLogByteExactProbe {
    pub method: Http1Method,
    pub path: String,
    pub host: String,
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default = "default_byte_exact_status")]
    pub expected_status: u16,
    /// Phase 70 (ADR-0141): `false` marks a probe whose response is expected to
    /// be SUPPRESSED by an access-log filter (contributes no log line on EITHER
    /// proxy). Defaults to `true` so every pre-existing byte-exact fixture
    /// deserializes unchanged.
    #[serde(default = "default_expect_logged")]
    pub expect_logged: bool,
}

fn default_byte_exact_status() -> u16 {
    200
}

fn default_expect_logged() -> bool {
    true
}

/// Phase 70 (ADR-0141): how many access-log lines a byte-exact probe sequence is
/// expected to produce — filter-suppressed probes (`expect_logged: false`) emit
/// none. Feeds both the `wait_file_lines` flush poll and the line-count
/// assertions of the H1/H2 byte-exact arms.
fn expected_logged_count(probes: &[AccessLogByteExactProbe]) -> usize {
    probes.iter().filter(|p| p.expect_logged).count()
}

/// 04.2 NEW: one probe entry inside `Driver::Http1ProbeList`. Each probe drives
/// one HTTP/1.1 request through both upstream Envoy and envoy-rust, applying
/// the same 5-axis equivalence cascade the single-probe `Driver::Http1` does.
/// Extra request headers (e.g. `X-Foo: bar`) inject through `extra_headers`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Http1Probe {
    /// Human-readable label for this probe (appears in failure messages).
    pub name: String,
    pub method: Http1Method,
    pub path: String,
    pub host: String,
    /// Extra request headers beyond the harness-emitted defaults
    /// (`Host`, `Connection: close`). Empty Vec means no extras.
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
    /// Optional request body. When present, the driver automatically adds a
    /// `Content-Length` header; do NOT also list `content-length` in
    /// `extra_headers`.
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub expected_status: Option<u16>,
    #[serde(default)]
    pub expected_body: Option<Http1BodyRule>,
    #[serde(default)]
    pub expected_headers: Option<Http1HeaderRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowMode {
    NameRequired,
    // future: NameOptional, ValueRegex, ValueOneOf, ...
}

/// Header allow-list per BEHAVIOR_CONTRACT.md `Header allow-list` table.
/// Sourced from the contract; updates to the contract update this constant
/// in lockstep. 04.1 added `server` and `date`; 04.3 added
/// `x-envoy-upstream-service-time`.
pub const HEADER_ALLOW_LIST: &[(&str, AllowMode)] = &[
    ("server", AllowMode::NameRequired),
    ("date", AllowMode::NameRequired),
    ("x-envoy-upstream-service-time", AllowMode::NameRequired), // 04.3 NEW
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveHttp1Result {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Set-equal modulo allow-list: case-insensitive name set equality, plus
/// value-exact match for any name not on the allow-list.
pub fn diff_headers(
    envoy: &[(String, String)],
    envoy_rust: &[(String, String)],
    allow_list: &[(&str, AllowMode)],
) -> anyhow::Result<()> {
    use std::collections::BTreeSet;

    fn names_lc(headers: &[(String, String)]) -> BTreeSet<String> {
        headers
            .iter()
            .map(|(n, _)| n.to_ascii_lowercase())
            .collect()
    }

    let envoy_names = names_lc(envoy);
    let envoy_rust_names = names_lc(envoy_rust);

    if envoy_names != envoy_rust_names {
        let only_envoy: Vec<_> = envoy_names.difference(&envoy_rust_names).collect();
        let only_rust: Vec<_> = envoy_rust_names.difference(&envoy_names).collect();
        anyhow::bail!(
            "header name sets differ: only-in-envoy={only_envoy:?}, only-in-envoy-rust={only_rust:?}"
        );
    }

    for name in envoy_names.iter() {
        let allow_entry = allow_list
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name));
        let envoy_value = envoy
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        let rust_value = envoy_rust
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .unwrap_or("");

        match allow_entry {
            Some((_, AllowMode::NameRequired)) => {
                // Skip value comparison.
            }
            None => {
                if envoy_value != rust_value {
                    anyhow::bail!(
                        "header `{name}`: envoy=`{envoy_value}` envoy-rust=`{rust_value}`"
                    );
                }
            }
        }
    }

    Ok(())
}

pub fn load_expectations(path: &Path) -> Result<Expectations> {
    let yaml =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed: Expectations =
        serde_yaml::from_str(&yaml).with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed)
}

/// Reserve a free TCP port on 127.0.0.1. Binds `:0`, reads the assigned port,
/// drops the listener, and returns the number.
///
/// Intra-process dedup: the kernel may hand back the same ephemeral port for
/// successive binds within a test process (CI run 26861955222 returned 40875 for
/// both data + admin listeners, causing envoy-rust to fail `Address already in use`).
/// This function tracks all ports handed out in the current process and retries the
/// ephemeral bind if a duplicate is encountered.
///
/// TOCTOU: between the drop and the subsequent bind by envoy-rust, another
/// process on the host could grab this port. This is accepted for a
/// pre-production harness per SPEC §6 point 6. If CI flakes materialize, this
/// becomes its own split phase with a port-range reservation strategy.
pub fn reserve_port() -> Result<u16> {
    reserve_port_with(|| {
        let listener = StdTcpListener::bind(("127.0.0.1", 0))
            .context("binding 127.0.0.1:0 to reserve a port")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    })
}

/// Core of `reserve_port` with an injectable ephemeral-port allocator so the
/// dedup logic is unit-testable. Ports are never returned to the set: a test
/// process reserves a few dozen ports at most.
fn reserve_port_with(mut bind_ephemeral: impl FnMut() -> Result<u16>) -> Result<u16> {
    static RESERVED_PORTS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<u16>>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

    for _ in 0..64 {
        let port = bind_ephemeral()?;
        if RESERVED_PORTS.lock().unwrap().insert(port) {
            return Ok(port);
        }
    }
    bail!("64 consecutive ephemeral-port reservations were duplicates of already-handed-out ports")
}

/// Template-render a fixture YAML by substituting literal `{{KEY}}` tokens.
/// The `kvs` list is the set of tokens to replace; any `{{…}}` token not in
/// `kvs` is left untouched so a typo surfaces as a parser error rather than
/// silently rendering to the empty string.
pub fn render_yaml(template: &str, kvs: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in kvs {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

/// 18 Task 6 (ADR-0049): does ANY of `sources` reference the literal
/// `{{marker}}` token? Used by `run_fixture`'s backend-launch detection so a
/// marker that lives ONLY in the CDS template (`cds.yaml` — the main configs
/// having zero static clusters and routing to a CDS-defined cluster, as in
/// fixture 0026) still triggers the corresponding host backend to spawn. The
/// `marker` argument is the bare key (e.g. `HTTP1_BACKEND_PORT`); the `{{…}}`
/// wrapping is added here so callers read as a plain key check.
pub fn scan_needs_marker(sources: &[&str], marker: &str) -> bool {
    let token = format!("{{{{{marker}}}}}");
    sources.iter().any(|s| s.contains(&token))
}

/// 18 Task 11 (ADR-0015, ADR-0049); generalized to a slice in 19 Task 6
/// (ADR-0050): decide whether the upstream Envoy container needs the
/// `with_host("host.docker.internal", Host::HostGateway)` mapping. The flag is
/// true exactly when ANY rendered upstream source references the
/// `host.docker.internal` hostname — silent otherwise so fixtures 0001/0002 stay
/// unchanged. The reference can live in the main config (most fixtures), the
/// rendered upstream CDS file (fixture 0026, where the main config has zero
/// static clusters and the backend endpoint is defined in the CDS file), OR the
/// rendered upstream LDS file (fixture 0027, where the HCM/route carrying the
/// endpoint lives in the dynamically-loaded listener). The mapping is required
/// on Linux CI, where the host-gateway is wired via `--add-host`; macOS Docker
/// Desktop resolves `host.docker.internal` natively, which is why missing a
/// rendered-source scan site only ever surfaced as a CI-only 503 (the phase-18
/// escaped-to-CI Critical). Pass all rendered upstream sources (main + CDS +
/// LDS, empty strings for absent ones) so no site is missed.
pub fn uses_host_gateway(sources: &[&str]) -> bool {
    sources.iter().any(|s| s.contains("host.docker.internal"))
}

/// 21 Task 6 (ADR-0054; §6.2 L9): discover the NUMERIC IPv4 host-gateway IP the
/// upstream Envoy container uses to reach the host backend. File-based EDS
/// rejects hostnames (L1 — `malformed IP address` → `update_rejected`), so the
/// EDS file's endpoint `socket_address.address` must be a numeric IP — and that
/// IP varies by platform (`192.168.65.254` on macOS Docker Desktop; the bridge
/// gateway e.g. `172.17.0.1` on Linux CI). Resolve it portably by running
/// `getent` inside a throwaway container with the host-gateway mapping (the
/// pinned Envoy image is Ubuntu-based and ships `getent`; NO new image
/// dependency). The bridge-network-inspect shortcut is WRONG on macOS, so
/// getent-in-container is the only cross-platform method.
///
/// Uses the `ahostsv4` database (NOT the PLAN-sketched `hosts`): on macOS Docker
/// Desktop `getent hosts host.docker.internal` returns ONLY the IPv6 mapping
/// (`fdc4:f303:9324::254`), which is unusable for an IPv4 `socket_address`;
/// `ahostsv4` forces IPv4-only resolution and yields `192.168.65.254`. Each
/// `ahostsv4` output line is `<ip>  <SOCKTYPE>  [canonname]`, so the parse scans
/// EVERY whitespace token across ALL lines and takes the first that parses as an
/// `Ipv4Addr` (robust to the leading non-IP tokens and the repeated
/// STREAM/DGRAM/RAW rows).
///
/// The image is pinned to the `ENVOY_TARGET.md` value via the
/// `upstream::IMAGE_NAME`/`upstream::IMAGE_TAG` constants the rest of the
/// harness uses (NOT a fresh literal). Gated to EDS fixtures — `run_fixture`
/// only calls this when `needs_eds`.
pub fn discover_host_gateway_ip() -> anyhow::Result<String> {
    let image = format!("{}:{}", upstream::IMAGE_NAME, upstream::IMAGE_TAG);
    let out = std::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "--add-host=host.docker.internal:host-gateway",
            "--entrypoint",
            "getent",
            &image,
            "ahostsv4",
            "host.docker.internal",
        ])
        .output()
        .context("running getent to discover the host-gateway IP")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let ip = text
        .split_whitespace()
        .find(|s| s.parse::<std::net::Ipv4Addr>().is_ok())
        .ok_or_else(|| {
            anyhow::anyhow!("getent did not return a numeric IPv4 host-gateway IP: {text:?}")
        })?
        .to_string();
    Ok(ip)
}

/// 28 Task 7 (ADR-0070): discover the host's primary non-loopback IPv4 — the
/// ONE address string the RING_HASH differential renders into BOTH proxies'
/// endpoints (`{{BACKEND_IP}}`), so both build their ring from identical
/// `ip:port` keys (the cross-proxy STRONG-selection precondition). Both the
/// subject (a host process) and the upstream container reach the host's
/// `0.0.0.0`-bound echo backends via this IP (loopback `127.0.0.1` is NOT
/// usable — it is not reachable from inside the container, and the container's
/// own loopback is a different namespace).
///
/// Discovery is route-based and sends NO packets: a UDP socket is "connected"
/// to a public address so the kernel selects the egress interface, and its
/// local address is read back. The target IP need not be reachable.
///
/// CAVEAT: the returned IP is the **egress-to-internet** interface's address,
/// which is ASSUMED to also be docker-bridge-reachable from the container; on a
/// runner where the egress interface is not bridge-reachable this still returns
/// `Ok` but defers the failure to the probe phase (the all-keys-non-200
/// signature documented in the fixture README's "CI portability" note).
pub fn discover_host_lan_ip() -> anyhow::Result<String> {
    use std::net::UdpSocket;
    let sock =
        UdpSocket::bind("0.0.0.0:0").context("binding UDP socket for host-LAN-IP discovery")?;
    sock.connect("8.8.8.8:53")
        .context("connecting UDP socket to select egress interface")?;
    let ip = sock
        .local_addr()
        .context("reading local_addr for host-LAN-IP discovery")?
        .ip();
    match ip {
        std::net::IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_unspecified() => Ok(v4.to_string()),
        other => bail!("host-LAN-IP discovery returned an unusable address: {other}"),
    }
}

/// 18 Task 6 (ADR-0049): scan a CDS rendition for any residual `{{MARKER}}`
/// token left behind by `render_yaml` (which intentionally leaves unmatched
/// tokens in place). Returns the first offending marker name (the text between
/// the first `{{` and its `}}`), or `None` if the content is fully resolved.
/// Used to fail fast with a named marker instead of letting an unsubstituted
/// token surface as a confusing downstream Envoy parse error.
fn residual_marker(content: &str) -> Option<&str> {
    let start = content.find("{{")?;
    let after = &content[start + 2..];
    let end = after.find("}}")?;
    Some(&after[..end])
}

/// Write `content` to a new temp file in `dir` and return the path. The caller
/// is responsible for ensuring `dir` is already created.
pub fn write_temp(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut f =
        std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(path)
}

/// 26 Task 7: atomic-rename `new_content` over `target` — write to a sibling temp file
/// in the SAME directory (so `rename` is a same-filesystem atomic swap, never a
/// cross-device copy), then `std::fs::rename`. The ONLY rewrite operation that triggers
/// Envoy's default file-watch (in-place truncate-rewrite does NOT — §6.2/ADR-0066).
fn atomic_rename_over(target: &Path, new_content: &str) -> std::io::Result<()> {
    // Deterministic sibling temp name in the SAME dir as `target` (appending a
    // suffix to the file name keeps it on the same filesystem/mount, so the
    // subsequent `rename` is a same-fs atomic swap rather than a cross-device
    // copy). On success the temp is consumed by the rename; on the write path
    // failing before the rename we remove it so no `.reload-tmp` leftover remains.
    // The fixed suffix is collision-safe under the current contract: each fixture
    // runs in its own tempdir and reloads at most once per run (one `Http1RdsReload`
    // step). A future multi-reload-per-fixture driver would need a per-reload nonce.
    let mut tmp = target.as_os_str().to_owned();
    tmp.push(".reload-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, new_content.as_bytes())?;
    match std::fs::rename(&tmp, target) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Best-effort cleanup; propagate the original rename error.
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// 26 Task 7: drive a single `Http1Probe` through BOTH proxies and apply the
/// per-probe equivalence cascade (response_status / response_body / probe
/// expected_status / expected_body / expected_headers) — the SAME cascade the
/// `Driver::Http1ProbeList` arm runs inline. Factored out so the
/// `Http1RdsReload` arm can reuse it for `pre_probes` AND `post_probes` without
/// duplicating the cascade. `label` (e.g. "pre" / "post") prefixes the probe
/// name in failure messages so a pre/post-reload mismatch is unambiguous.
async fn run_http1_probe_bilateral(
    upstream_addr: SocketAddr,
    subject_addr: SocketAddr,
    equivalence: &Equivalence,
    probe: &Http1Probe,
    label: &str,
) -> Result<()> {
    let upstream_resp = drive_http1(
        upstream_addr,
        &probe.method,
        &probe.path,
        &probe.host,
        &probe.extra_headers,
        probe.body.as_deref().map(str::as_bytes),
    )
    .await
    .with_context(|| format!("upstream envoy http1 drive ({label} probe {})", probe.name))?;
    let subject_resp = drive_http1(
        subject_addr,
        &probe.method,
        &probe.path,
        &probe.host,
        &probe.extra_headers,
        probe.body.as_deref().map(str::as_bytes),
    )
    .await
    .with_context(|| format!("envoy-rust http1 drive ({label} probe {})", probe.name))?;

    // Status: envoy ↔ envoy-rust under `response_status: exact`.
    if matches!(equivalence.response_status, Some(StatusRule::Exact))
        && upstream_resp.status != subject_resp.status
    {
        bail!(
            "{label} probe {}: response status mismatch under `response_status: exact`\n  \
             upstream: {}\n  subject:  {}",
            probe.name,
            upstream_resp.status,
            subject_resp.status,
        );
    }
    if let Some(es) = probe.expected_status {
        if upstream_resp.status != es {
            bail!(
                "{label} probe {}: upstream status {} != expected {}",
                probe.name,
                upstream_resp.status,
                es,
            );
        }
        if subject_resp.status != es {
            bail!(
                "{label} probe {}: subject status {} != expected {}",
                probe.name,
                subject_resp.status,
                es,
            );
        }
    }

    // Body.
    if let Some(rule) = &equivalence.response_body {
        assert_body_rule(rule, &upstream_resp.body, &subject_resp.body)
            .with_context(|| format!("{label} probe {}", probe.name))?;
    }
    if let Some(Http1BodyRule::ByteExact { body }) = &probe.expected_body {
        let expected = body.as_bytes();
        if upstream_resp.body != expected {
            bail!(
                "{label} probe {}: upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                probe.name,
                upstream_resp.body,
                expected,
            );
        }
        if subject_resp.body != expected {
            bail!(
                "{label} probe {}: subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                probe.name,
                subject_resp.body,
                expected,
            );
        }
    }

    // Headers.
    if matches!(
        probe.expected_headers,
        Some(Http1HeaderRule::SetEqualModuloAllowList)
    ) {
        diff_headers(
            &upstream_resp.headers,
            &subject_resp.headers,
            HEADER_ALLOW_LIST,
        )
        .with_context(|| format!("{label} probe {}: diff_headers", probe.name))?;
    }
    Ok(())
}

/// 26 Task 7: drive `probe` against `addr` repeatedly (bounded by `budget`) until the
/// response matches the probe's `expected_status` (and `expected_body`, if set) — the
/// signal the proxy has converged on the reloaded table. Returns Ok on convergence,
/// Err on budget exhaustion. NOT a fixed sleep — this is the 12.2 wait-for-convergence
/// pattern on a discriminating observable (the routed-to behavior). Polls every 25ms.
async fn wait_for_reload_convergence(
    addr: SocketAddr,
    probe: &Http1Probe,
    budget: Duration,
) -> Result<()> {
    let deadline = std::time::Instant::now() + budget;
    let poll = Duration::from_millis(25);
    // Retain the most recent drive error so a budget-exhaustion failure can name
    // the underlying cause (an opaque "did not converge" timeout on the CI-only
    // path is far harder to diagnose than one carrying the last attempt's error).
    let mut last_err: Option<String> = None;
    loop {
        // A drive error (connection reset mid-reload, etc.) is non-fatal while
        // the budget remains — treat it as "not converged yet" and retry.
        let matched = match drive_http1(
            addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
            probe.body.as_deref().map(str::as_bytes),
        )
        .await
        {
            Ok(resp) => {
                let status_ok = probe.expected_status.is_none_or(|es| resp.status == es);
                let body_ok = match &probe.expected_body {
                    Some(Http1BodyRule::ByteExact { body }) => resp.body == body.as_bytes(),
                    None => true,
                };
                status_ok && body_ok
            }
            Err(e) => {
                last_err = Some(e.to_string());
                false
            }
        };
        if matched {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let last = last_err
                .as_deref()
                .map(|e| format!("; last drive error: {e}"))
                .unwrap_or_default();
            bail!(
                "RDS reload did not converge on {addr} within {budget:?} \
                 (discriminator probe {}){last}",
                probe.name,
            );
        }
        tokio::time::sleep(poll).await;
    }
}

/// Poll `addr` with exponential backoff (starting at 50ms, doubling, capped at
/// 500ms) until a TCP connect succeeds or `budget` elapses. Returns `Err` on
/// timeout.
pub async fn wait_accept_ready(addr: std::net::SocketAddr, budget: Duration) -> Result<()> {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(err) => bail!("{addr} not accept-ready within {budget:?}: {err}"),
        }
    }
}

/// Poll `path` until it exists with len > 0, or `budget` expires.
/// Returns whether the file became non-empty. Non-fatal by design: callers
/// fall through to the byte-level assertion, which reports the real diff.
async fn wait_file_nonempty(path: &std::path::Path, budget: Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;
    loop {
        if path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Budget for the byte-exact access-log scrape to see all N lines before the
/// container is SIGKILLed. Sized to outlast Envoy's ~10s FileAccessLog flush timer.
const ACCESS_LOG_FLUSH_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

/// Poll `path` until it contains at least `want` lines or `budget` elapses.
/// Returns true if the line count was reached. Mirrors `wait_file_nonempty`'s
/// deadline/100ms-sleep skeleton, generalized from non-empty to a line-count
/// predicate (used by the byte-exact access-log driver, which must scrape N
/// lines from a still-alive container before SIGKILL drops Envoy's buffered lines).
async fn wait_file_lines(path: &std::path::Path, want: usize, budget: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + budget;
    loop {
        let have = std::fs::read_to_string(path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        if have >= want {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Parse Envoy admin `/stats` plain text: warm iff at least one
/// `cluster.<name>.membership_healthy` gauge exists and ALL such gauges
/// are >= 1.
fn clusters_warm_from_stats_text(stats: &str) -> bool {
    let mut saw_any = false;
    for line in stats.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.starts_with("cluster.") && name.trim_end().ends_with(".membership_healthy") {
            saw_any = true;
            if value.trim().parse::<u64>().map(|v| v >= 1) != Ok(true) {
                return false;
            }
        }
    }
    saw_any
}

/// File-based xDS warm-up gate (CI runs 26862683687 / 26862493718: upstream
/// answered 503 because the CDS-supplied STRICT_DNS cluster had not resolved
/// when the measured request fired). Polls admin `/stats` until clusters
/// report healthy membership. Budget expiry is deliberately NON-FATAL: the
/// measured drive then fails with exactly the diff it would have produced
/// without the gate, so the gate cannot mask a real differential bug.
/// Admin-only on purpose: these fixtures assert exact data-plane counters,
/// so throwaway data-plane probes would break them deterministically.
async fn wait_clusters_warm(admin_addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if let Ok(Ok(resp)) =
            tokio::time::timeout(remaining, drive_http_get(admin_addr, "/stats", "localhost")).await
            && clusters_warm_from_stats_text(&String::from_utf8_lossy(&resp.body))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    tracing::warn!(%admin_addr, ?budget, "wait_clusters_warm: budget expired without all clusters warm; proceeding ungated");
}

/// Drive `payload` at `addr`: open TCP, write payload, read exactly
/// `payload.len()` bytes of echoed response, then confirm the peer writes
/// no further bytes before shutting down the write side and dropping the
/// stream. Returns the echoed bytes.
///
/// Why `read_exact(payload.len())` instead of half-close + `read_to_end`: see
/// `docs/envoy-rust/DECISIONS.md` ADR-0006. Upstream Envoy v1.33.0's default
/// `ConnectionImpl` (enable_half_close_=false) translates a client FIN into
/// `PostIoAction::Close` and calls `closeSocket(RemoteClose)` before the echo
/// filter's queued write is flushed, so a pre-read half-close causes the
/// response bytes to be dropped. Phase 00's only fixture (echo filter) has a
/// deterministic 1:1 byte-count contract, so `read_exact(payload.len())` is
/// both sufficient and matches upstream Envoy's own echo integration test
/// pattern. Graceful write-side shutdown still fires after the read so the
/// envoy-rust subject's echo loop exits on FIN rather than a peer reset.
///
/// Why the trailing-byte poll: see ADR-0007. A bare `read_exact(payload.len())`
/// silently ignores any bytes the peer writes after the echo, which would
/// narrow BEHAVIOR_CONTRACT row 2's "byte-exact" assertion to "first N bytes
/// match." After `read_exact`, we poll the socket with a short deadline
/// (100ms) and bail if the peer delivers more data before EOF or the
/// deadline — a peer that follows the echo-filter contract closes its
/// write side cleanly and we observe `Ok(0)` or a timeout.
pub async fn drive_tcp(addr: SocketAddr, payload: &[u8]) -> Result<Vec<u8>> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    stream.write_all(payload).await?;
    let mut out = vec![0u8; payload.len()];
    stream.read_exact(&mut out).await?;

    // ADR-0007: detect trailing bytes past the echoed payload. A compliant
    // peer either closes (Ok(0)) or stays silent until the deadline (timeout
    // Err). Any non-zero read is a contract violation.
    let mut tail = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(100), stream.read(&mut tail)).await {
        Ok(Ok(0)) | Err(_) => {}
        Ok(Ok(n)) => bail!("{addr} sent {n} trailing bytes after echo"),
        Ok(Err(e)) => bail!("{addr} read error after echo: {e}"),
    }

    stream.shutdown().await.ok();
    drop(stream);
    Ok(out)
}

/// Connect to `addr`, send NOTHING, and read until the peer closes.
///
/// `envoy.filters.network.direct_response` writes its configured payload the
/// moment a connection is accepted and then half-closes, so the whole response
/// is "everything until EOF". A missing EOF within the deadline is a contract
/// violation (the peer must close, not linger). Phase 66, SPEC §0 R-0.5.
pub async fn drive_tcp_direct_response(addr: SocketAddr) -> Result<Vec<u8>> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let mut out = Vec::new();
    match tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut out)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => bail!("{addr} read error before EOF (reset?): {e}"),
        Err(_) => bail!("{addr} did not close within 5s; direct_response must half-close"),
    }
    drop(stream);
    Ok(out)
}

/// 67.1 D7 (ADR-0131): connect to `addr`, WRITE `payload`, then read until the
/// peer closes.
///
/// This is the DENY shape for `envoy.filters.network.rbac`. Upstream Envoy
/// evaluates network RBAC on the FIRST DOWNSTREAM BYTE (`ONE_TIME_ON_FIRST_BYTE`),
/// so a probe that sends nothing is never evaluated and the connection simply
/// stays open — measured. The probe must therefore send at least one byte, and
/// on DENY the peer answers with ZERO bytes and a clean EOF, discarding what was
/// sent. `drive_tcp` cannot express this: it `read_exact`s `payload.len()` bytes
/// back, which never arrive.
pub async fn drive_tcp_write_then_read_to_eof(addr: SocketAddr, payload: &[u8]) -> Result<Vec<u8>> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    stream
        .write_all(payload)
        .await
        .with_context(|| format!("writing probe payload to {addr}"))?;
    let mut out = Vec::new();
    match tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut out)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => bail!("{addr} read error before EOF (reset?): {e}"),
        Err(_) => bail!("{addr} did not close within 5s after the first byte"),
    }
    drop(stream);
    Ok(out)
}

/// Drive a payload through `addr` over a TLS connection terminated by the
/// peer (downstream-TLS scenario). The peer's leaf cert is verified against
/// `root_store`; the SNI is `sni`; if `expected_cn` is `Some`, the
/// post-handshake cert chain's leaf is walked for SAN-DNS entries and
/// CommonName, and the test fails if no case-insensitive exact match is
/// found (no wildcard support in 03.1 — SPEC §6 signpost 11).
///
/// Mirrors `drive_tcp`'s ADR-0006/0007 discipline: writes payload, reads
/// exactly `payload.len()` bytes, then runs the 100ms trailing-byte poll.
/// Graceful TLS shutdown on the write side completes before drop.
pub async fn drive_tls(
    addr: SocketAddr,
    payload: &[u8],
    sni: &str,
    root_store: rustls::RootCertStore,
    expected_cn: Option<&str>,
) -> Result<Vec<u8>> {
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_cfg));
    let server_name = ServerName::try_from(sni)
        .map_err(|e| anyhow::anyhow!("parsing sni {sni:?}: {e}"))?
        .to_owned();

    let tcp = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let mut tls = connector
        .connect(server_name, tcp)
        .await
        .with_context(|| format!("TLS handshake against {addr}"))?;

    if let Some(cn) = expected_cn {
        let peer_certs = tls
            .get_ref()
            .1
            .peer_certificates()
            .ok_or_else(|| anyhow::anyhow!("no peer certificate after handshake"))?;
        let leaf = peer_certs
            .first()
            .ok_or_else(|| anyhow::anyhow!("peer cert chain is empty"))?;
        check_cn_or_san(leaf, cn).context("expected_cn match")?;
    }

    tls.write_all(payload).await?;
    let mut out = vec![0u8; payload.len()];
    tls.read_exact(&mut out).await?;

    let mut tail = [0u8; 64];
    match tokio::time::timeout(Duration::from_millis(100), tls.read(&mut tail)).await {
        Ok(Ok(0)) | Err(_) => {}
        Ok(Ok(n)) => bail!("{addr} sent {n} trailing bytes after echo"),
        Ok(Err(e)) => bail!("{addr} read error after echo: {e}"),
    }

    tls.shutdown().await.ok();
    drop(tls);
    Ok(out)
}

/// Drive a sequence of per-SNI TLS probes against a single listener address.
/// Each probe gets a fresh TCP connection + TLS handshake; the SNI varies per
/// probe; if the probe declares `expected_cn`, the post-handshake leaf cert is
/// matched (DER-substring scan via `check_cn_or_san`) before any payload write.
/// Each probe runs the same ADR-0006 read-exact + ADR-0007 trailing-byte poll
/// discipline as `drive_tls`.
///
/// Returns `Ok(probe_outputs)` where `probe_outputs[i]` is the bytes echoed
/// back for `probes[i]` (typically equal to `payload`). On any per-probe
/// failure (handshake, expected_cn mismatch, byte mismatch, trailing-byte
/// detection) returns `Err` naming the probe's SNI for diagnostics.
///
/// Equivalence note: byte-equality is enforced *inside* this helper (each
/// probe writes `payload`, reads-exact `payload.len()` bytes, and the read
/// would not have succeeded as a different byte sequence under
/// `read_exact`-then-bail-on-trailing semantics). Per-probe `expected_cn`
/// matches enforce the cert-selection invariant on each side independently;
/// the conjunction across upstream + subject is the "both proxies select the
/// same cert for the same SNI" property — implicit, no final
/// `assert_equivalence` needed.
pub async fn drive_tls_probes(
    addr: SocketAddr,
    payload: &[u8],
    probes: &[TlsTcpProbe],
    root_store: rustls::RootCertStore,
) -> Result<Vec<Vec<u8>>> {
    use rustls::pki_types::ServerName;
    use std::convert::TryFrom;
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let client_cfg = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(client_cfg));

    let mut outputs = Vec::with_capacity(probes.len());
    for probe in probes {
        let server_name = ServerName::try_from(probe.sni.as_str())
            .map_err(|e| anyhow::anyhow!("parsing sni {:?}: {e}", probe.sni))?
            .to_owned();

        let tcp = tokio::net::TcpStream::connect(addr)
            .await
            .with_context(|| format!("connecting to {addr} for probe sni={:?}", probe.sni))?;
        let mut tls = connector.connect(server_name, tcp).await.with_context(|| {
            format!("TLS handshake against {addr} for probe sni={:?}", probe.sni)
        })?;

        if let Some(cn) = &probe.expected_cn {
            let peer_certs = tls
                .get_ref()
                .1
                .peer_certificates()
                .ok_or_else(|| anyhow::anyhow!("no peer cert for probe sni={:?}", probe.sni))?;
            let leaf = peer_certs.first().ok_or_else(|| {
                anyhow::anyhow!("peer cert chain empty for probe sni={:?}", probe.sni)
            })?;
            check_cn_or_san(leaf, cn)
                .with_context(|| format!("expected_cn match for probe sni={:?}", probe.sni))?;
        }

        tls.write_all(payload)
            .await
            .with_context(|| format!("write for probe sni={:?}", probe.sni))?;
        let mut out = vec![0u8; payload.len()];
        tls.read_exact(&mut out)
            .await
            .with_context(|| format!("read_exact for probe sni={:?}", probe.sni))?;

        // ADR-0007 trailing-byte poll, mirroring drive_tls.
        let mut tail = [0u8; 64];
        match tokio::time::timeout(Duration::from_millis(100), tls.read(&mut tail)).await {
            Ok(Ok(0)) | Err(_) => {}
            Ok(Ok(n)) => bail!(
                "{addr} sent {n} trailing bytes after echo for probe sni={:?}",
                probe.sni
            ),
            Ok(Err(e)) => bail!(
                "{addr} read error after echo for probe sni={:?}: {e}",
                probe.sni
            ),
        }

        tls.shutdown().await.ok();
        drop(tls);

        outputs.push(out);
    }
    Ok(outputs)
}

/// Walk a leaf cert's SAN DNS entries + CommonName for a case-insensitive
/// exact match against `wanted`. No wildcard support in 03.1 (SPEC §6
/// signpost 11). The cert is parsed via the rcgen-roundtrip path —
/// rustls-pemfile yields `CertificateDer`, which we re-parse to extract the
/// SAN/CN strings. We use an inline minimal X.509 walk via rustls-pemfile +
/// `rustls::pki_types` machinery; full TLS validation already happened during
/// the handshake.
fn check_cn_or_san(cert: &rustls::pki_types::CertificateDer<'_>, wanted: &str) -> Result<()> {
    // The simplest path: re-encode the DER to PEM, then use rcgen's parser
    // (we already pull rcgen for cert generation, so its parser is in scope
    // for free). If that proves fragile, swap to `x509-parser` under a new
    // ADR. For 03.1 the cert chain we're matching against is rcgen-built
    // ourselves, so an exact match on the SAN DNS string is reliable.
    //
    // rcgen 0.13 doesn't ship a public PEM/DER parser exposing SAN strings
    // directly; rather than fight that, fall back to walking the DER for
    // the SAN extension's GeneralNames manually. Phase 03.2 may pull
    // `x509-parser` under a follow-up ADR if more sophisticated cert
    // introspection is needed; for 03.1, the harness's `expected_cn` is
    // optional and used only for sanity — the differential body equivalence
    // is the primary signal.
    //
    // Simplest viable check: the rcgen-built leaf's DER includes the SAN
    // value as a literal UTF-8 substring. Search for it.
    let der_bytes: &[u8] = cert.as_ref();
    let needle = wanted.to_ascii_lowercase();
    let hay: Vec<u8> = der_bytes.iter().map(|b| b.to_ascii_lowercase()).collect();
    if hay.windows(needle.len()).any(|w| w == needle.as_bytes()) {
        return Ok(());
    }
    bail!("expected_cn / SAN match for {wanted:?} not found in peer cert (DER-substring scan)",);
}

/// Decoded HTTP/1.1 response. Headers are captured for debug tracing but play
/// no part in the phase-01 equivalence diff (ADR-0011).
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    #[allow(dead_code)]
    pub headers: Vec<(String, Vec<u8>)>,
}

/// Open a TCP connection to `addr`, issue a minimal `GET` for `path` with
/// `Host: host`, and parse the response. Supports `content-length`-framed,
/// `transfer-encoding: chunked`-framed, and `connection: close`-framed
/// responses. Chunked support was added in phase 01 when upstream Envoy v1.33.0
/// was observed returning chunked responses for `/ready` (SPEC §6 signpost 9).
pub async fn drive_http_get(addr: SocketAddr, path: &str, host: &str) -> Result<HttpResponse> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await?;
    stream.flush().await.ok();

    let mut buf = Vec::with_capacity(2048);
    let mut scratch = [0u8; 2048];
    let head_end;
    loop {
        let n = stream.read(&mut scratch).await?;
        if n == 0 {
            bail!("{addr} closed before a response head was received");
        }
        buf.extend_from_slice(&scratch[..n]);

        let mut headers = [httparse::EMPTY_HEADER; 32];
        let mut resp = httparse::Response::new(&mut headers);
        match resp.parse(&buf) {
            Ok(httparse::Status::Complete(n)) => {
                head_end = n;
                let status = resp
                    .code
                    .ok_or_else(|| anyhow::anyhow!("missing response status code"))?;
                let mut captured_headers: Vec<(String, Vec<u8>)> = Vec::new();
                let mut content_length: Option<usize> = None;
                let mut connection_close = false;
                let mut chunked = false;
                for h in resp.headers.iter() {
                    captured_headers.push((h.name.to_ascii_lowercase(), h.value.to_vec()));
                    if h.name.eq_ignore_ascii_case("content-length") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        content_length = Some(s.parse()?);
                    } else if h.name.eq_ignore_ascii_case("connection") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        if s.eq_ignore_ascii_case("close") {
                            connection_close = true;
                        }
                    } else if h.name.eq_ignore_ascii_case("transfer-encoding") {
                        let s = std::str::from_utf8(h.value)?.trim();
                        if s.eq_ignore_ascii_case("chunked") {
                            chunked = true;
                        }
                    }
                }

                // Drain the body.
                let body = if chunked {
                    // Decode HTTP/1.1 chunked transfer encoding. Read all
                    // wire bytes (already-buffered tail + remaining from
                    // stream), then parse chunk frames and concatenate the
                    // chunk data. This handles upstream Envoy v1.33.0's
                    // habit of sending `/ready` bodies as chunked.
                    let mut wire = buf[head_end..].to_vec();
                    stream.read_to_end(&mut wire).await?;
                    decode_chunked(&wire)
                        .with_context(|| format!("{addr} chunked decoding failed"))?
                } else {
                    match content_length {
                        Some(cl) => {
                            let mut body = Vec::with_capacity(cl);
                            let already = &buf[head_end..];
                            let take = already.len().min(cl);
                            body.extend_from_slice(&already[..take]);
                            if body.len() < cl {
                                let remaining = cl - body.len();
                                let mut rest = vec![0u8; remaining];
                                stream.read_exact(&mut rest).await?;
                                body.extend(rest);
                            }
                            body
                        }
                        None if connection_close => {
                            let mut body = Vec::new();
                            body.extend_from_slice(&buf[head_end..]);
                            stream.read_to_end(&mut body).await?;
                            body
                        }
                        None => bail!(
                            "{addr} response has neither `content-length` nor \
                             `connection: close` nor `transfer-encoding: chunked`; \
                             drive_http_get does not support keep-alive in phase 01",
                        ),
                    }
                };

                return Ok(HttpResponse {
                    status,
                    body,
                    headers: captured_headers,
                });
            }
            Ok(httparse::Status::Partial) => continue,
            Err(e) => bail!("{addr} response parse error: {e}"),
        }
    }
}

/// Drive an HTTP/1.1 request (no body) at `addr` for the 04.1 differential
/// suite: open TCP, write a request line + `Host:` + `Connection: close`,
/// then read until httparse signals headers `Complete`, capture every header
/// in order, parse `Content-Length`, and read the declared body bytes (zero if
/// no `Content-Length`). Returns status + headers + body.
///
/// Framing scope: this helper only handles `Content-Length`-framed responses
/// (the only shape produced by 04.1's `direct_response` filter). `chunked` /
/// `connection: close` framing is the existing `drive_http_get` helper's
/// responsibility — when 04.x grows fixtures that need those, the dispatch
/// arm will pick the right helper rather than overloading this one.
///
/// Reads run under a 5s per-poll timeout; a peer EOF before headers complete
/// or before the declared body is consumed surfaces as `Err`. The connection
/// is dropped after the body bytes have been read (peer closes via
/// `Connection: close`).
pub async fn drive_http1(
    addr: SocketAddr,
    method: &Http1Method,
    path: &str,
    host: &str,
    extra_headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<DriveHttp1Result> {
    use tokio::net::TcpStream;
    let mut stream = TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let mut req = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\n",
        method.as_str(),
        path,
        host,
    );
    for (n, v) in extra_headers {
        req.push_str(&format!("{n}: {v}\r\n"));
    }
    if let Some(b) = body {
        req.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    req.push_str("Connection: close\r\n\r\n");
    let mut wire: Vec<u8> = req.into_bytes();
    if let Some(b) = body {
        wire.extend_from_slice(b);
    }
    stream.write_all(&wire).await?;

    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let read_timeout = Duration::from_secs(5);

    // Read headers until httparse signals Complete; then read Content-Length body.
    let (status, headers, headers_end, content_length, chunked) = loop {
        let mut chunk = [0u8; 4096];
        let n = tokio::time::timeout(read_timeout, stream.read(&mut chunk)).await??;
        if n == 0 {
            anyhow::bail!("unexpected EOF before headers complete");
        }
        buf.extend_from_slice(&chunk[..n]);

        let mut hp_headers = [httparse::EMPTY_HEADER; 64];
        let mut resp = httparse::Response::new(&mut hp_headers);
        match resp.parse(&buf)? {
            httparse::Status::Complete(headers_end) => {
                let status = resp.code.ok_or_else(|| anyhow::anyhow!("no status code"))?;
                let mut headers: Vec<(String, String)> = Vec::with_capacity(resp.headers.len());
                for h in resp.headers.iter() {
                    if h.name.is_empty() {
                        continue;
                    }
                    let value = std::str::from_utf8(h.value)
                        .map_err(|e| anyhow::anyhow!("invalid utf8 header value: {e}"))?
                        .to_string();
                    headers.push((h.name.to_string(), value));
                }
                let content_length = headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse::<usize>().ok());
                let chunked = headers.iter().any(|(n, v)| {
                    n.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
                });
                break (status, headers, headers_end, content_length, chunked);
            }
            httparse::Status::Partial => continue,
        }
    };

    // Body framing precedence per RFC 7230 §3.3.3 (with the simpler subset
    // sufficient for the harness): `transfer-encoding: chunked` overrides
    // `content-length`. `content-length`-framed: read exactly N. Otherwise
    // (no length, no chunked): read-until-EOF (`Connection: close`-framed,
    // which is what the harness always asks for via the close header).
    //
    // 06.1 Task 13 fix: this arm previously hard-defaulted content_length
    // to 0 and only handled the `Some(content_length)` shape, so an admin
    // scrape against upstream Envoy v1.33.0's `/stats/prometheus` (which
    // ships chunked) was decoded as a 0-byte body. Mirrors `drive_http_get`'s
    // chunked handling (added in phase 01 for `/ready`).
    let body = if chunked {
        // Drain the rest of the wire to EOF, then chunk-decode.
        let mut wire = buf[headers_end..].to_vec();
        loop {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(read_timeout, stream.read(&mut chunk)).await??;
            if n == 0 {
                break;
            }
            wire.extend_from_slice(&chunk[..n]);
        }
        decode_chunked(&wire).context("chunked decoding failed")?
    } else if let Some(cl) = content_length {
        while buf.len() < headers_end + cl {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(read_timeout, stream.read(&mut chunk)).await??;
            if n == 0 {
                if buf.len() < headers_end + cl {
                    anyhow::bail!(
                        "unexpected EOF before body complete: have {}, expected {}",
                        buf.len() - headers_end,
                        cl,
                    );
                }
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        buf[headers_end..headers_end + cl].to_vec()
    } else {
        // Connection-close framing: read until the peer EOFs.
        let mut body = buf[headers_end..].to_vec();
        loop {
            let mut chunk = [0u8; 4096];
            let n = tokio::time::timeout(read_timeout, stream.read(&mut chunk)).await??;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        body
    };

    Ok(DriveHttp1Result {
        status,
        headers,
        body,
    })
}

/// Drive an HTTP/2 cleartext (H2C prior-knowledge) request against the given
/// listener address. Mirrors `drive_http1`'s shape so `assert_equivalence`'s
/// `diff_headers` works without modification. Per parent-05 SPEC §6 signpost 8
/// this helper consumes `h2 = "0.4"` directly — the documented carve-out from
/// cross-sub-phase architectural rule 1, parallel to phase-04.1 REVIEW
/// M-architectural-claim's `httparse` posture for `drive_http1`.
pub async fn drive_http2(
    addr: SocketAddr,
    method: &Http1Method,
    path: &str,
    host: &str,
    extra_headers: &[(String, String)],
) -> Result<DriveHttp1Result> {
    use tokio::net::TcpStream;

    debug_assert!(
        matches!(method, Http1Method::Get | Http1Method::Options),
        "drive_http2 currently only supports GET/OPTIONS; widen the helper if/when a fixture needs body request methods"
    );

    let tcp = TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let (mut send_request, conn) = h2::client::handshake(tcp).await.context("H2 handshake")?;

    // Drive the connection in the background.
    let conn_handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    // Build the request. Use absolute-form URI so :authority is populated.
    let uri: http::Uri = format!("http://{host}{path}")
        .parse()
        .context("URI parse")?;
    let mut builder = http::Request::builder().method(method.as_str()).uri(uri);
    for (n, v) in extra_headers {
        builder = builder.header(n.as_str(), v.as_str());
    }
    let req = builder.body(()).context("request build")?;

    // Send the request with end_of_stream=true. Currently GET-only — see the
    // debug_assert! above; a future widening would compute end_of_stream from
    // a body parameter.
    let (response_fut, _send_stream) = send_request
        .send_request(req, true)
        .context("H2 send_request")?;

    let resp = response_fut.await.context("H2 response")?;
    let status = resp.status().as_u16();
    let header_map = resp.headers().clone();
    let mut body_stream = resp.into_body();

    let mut body = Vec::new();
    while let Some(chunk) = body_stream.data().await {
        let chunk = chunk.context("H2 body data")?;
        body.extend_from_slice(&chunk);
        // Best-effort window release — any error here will also surface on the
        // next body_stream.data() call, so swallowing keeps the helper readable.
        // (Asymmetric to HCM at crates/envoy-http2/src/hcm.rs:104 which
        // propagates with `?`; HCM owns the connection lifetime, this helper
        // does not.)
        body_stream
            .flow_control()
            .release_capacity(chunk.len())
            .ok();
    }

    let mut headers: Vec<(String, String)> = Vec::with_capacity(header_map.len());
    for (n, v) in header_map.iter() {
        let value_str = v
            .to_str()
            .with_context(|| format!("non-UTF-8 H2 response header value for `{}`", n.as_str()))?;
        headers.push((n.as_str().to_string(), value_str.to_string()));
    }

    // Abort the connection task — we have the full response, and the server
    // will not necessarily close the TCP socket on its own (h2's `Connection`
    // future runs until peer EOF). Awaiting unconditionally would tie test
    // wall-time to whichever side closes first; aborting makes the helper
    // return as soon as the response is drained.
    drop(send_request);
    conn_handle.abort();
    let _ = conn_handle.await;

    Ok(DriveHttp1Result {
        status,
        headers,
        body,
    })
}

/// 13.2 Task 5 (ADR-0039): drive N sequential single-stream HTTP/2 requests
/// over ONE downstream H2 conn opened to `proxy_addr`. Mirrors `drive_http2`
/// (single-shot) but holds the `SendRequest<Bytes>` across N requests so all
/// streams share the same downstream H2 conn — the discriminating-observable
/// shape per parent-13 SPEC §6.2 item-iv.
///
/// Per-request flow (mirrors `drive_http2`'s body verbatim modulo the loop):
///   1. Clone `SendRequest` (it is `Clone` to support per-stream multiplex).
///   2. Build `http::Request<()>` with absolute-form URI so `:authority` is
///      populated (matches `drive_http2`'s absolute-URI shape).
///   3. `send_request(req, /*end_of_stream=*/ true)` — GET-only, no body.
///   4. Await `ResponseFuture`, assert status equals `expected_status`, drain
///      the response body with best-effort flow-control window release.
///
/// The connection-driving `tokio::spawn` (the H2 `Connection` future) is
/// retained across all requests and aborted once the response loop completes,
/// matching `drive_http2`'s teardown shape — the server (echo backend OR
/// proxy) will not necessarily close the socket on its own, so an explicit
/// abort avoids tying test wall-time to peer-close.
///
/// `side_name` ("upstream" / "subject") is for error context — request errors
/// surface the failing side at the request boundary so a hung response on
/// only one side is named immediately.
pub async fn drive_http2_keep_alive(
    proxy_addr: SocketAddr,
    requests: &[Http1KeepAliveRequest],
    side_name: &str,
) -> Result<()> {
    use tokio::net::TcpStream;

    let tcp = TcpStream::connect(proxy_addr)
        .await
        .with_context(|| format!("{side_name}: connecting to proxy {proxy_addr}"))?;
    let (send_request, conn) = h2::client::handshake(tcp)
        .await
        .with_context(|| format!("{side_name}: H2 handshake against proxy {proxy_addr}"))?;

    // Drive the H2 `Connection` future in the background across all N
    // requests. Mirrors `drive_http2`'s shape — abort + await at the end.
    let conn_handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    let drive_result: Result<()> = async {
        for req in requests {
            // Per-stream clone of `SendRequest` — h2 derives `Clone` precisely
            // to support multiplexed stream issuance from one connection.
            // Sequential await means we never have multiple in-flight streams,
            // but cloning is the documented multiplex idiom and the binding
            // ADR-0039 scope item #4 names this shape explicitly.
            let mut sr = send_request.clone();

            // Absolute-form URI so the h2 codec populates :authority. Mirrors
            // `drive_http2` line 1389-1391 verbatim.
            let uri: http::Uri = format!("http://{}{}", req.host, req.path)
                .parse()
                .with_context(|| {
                    format!(
                        "{side_name}: URI parse for {} {} (host={})",
                        req.method, req.path, req.host
                    )
                })?;
            let request = http::Request::builder()
                .method(req.method.as_str())
                .uri(uri)
                .body(())
                .with_context(|| {
                    format!(
                        "{side_name}: building H2 request {} {}",
                        req.method, req.path
                    )
                })?;

            let (response_fut, _send_stream) = sr
                .send_request(request, /*end_of_stream=*/ true)
                .with_context(|| {
                    format!(
                        "{side_name}: H2 send_request for {} {}",
                        req.method, req.path
                    )
                })?;
            let resp = response_fut.await.with_context(|| {
                format!(
                    "{side_name}: awaiting H2 response for {} {}",
                    req.method, req.path
                )
            })?;
            let status = resp.status().as_u16();
            anyhow::ensure!(
                status == req.expected_status,
                "{side_name}: expected status {} for {} {}, got {}",
                req.expected_status,
                req.method,
                req.path,
                status,
            );

            // Drain the response body so the stream completes cleanly before
            // we issue the next request. Mirrors `drive_http2`'s flow-control
            // release cadence verbatim — best-effort release because errors
            // here will also surface on the next `data().await`.
            let mut body_stream = resp.into_body();
            while let Some(chunk) = body_stream.data().await {
                let chunk = chunk.with_context(|| {
                    format!("{side_name}: H2 body data for {} {}", req.method, req.path)
                })?;
                body_stream
                    .flow_control()
                    .release_capacity(chunk.len())
                    .ok();
            }
        }
        Ok(())
    }
    .await;

    // Teardown mirrors `drive_http2`'s shape verbatim. `drop(send_request)`
    // releases the last SendRequest handle, so the h2 `Connection` future's
    // inbound channel closes — drop-before-abort is hygienic ordering. The
    // load-bearing step is `conn_handle.abort()`: it synchronously preempts
    // the Connection future so this helper returns as soon as the response
    // is drained, without tying test wall-time to peer-side GOAWAY hygiene
    // (the future is never polled again post-abort, so no clean GOAWAY
    // round-trip fires).
    drop(send_request);
    conn_handle.abort();
    let _ = conn_handle.await;

    drive_result
}

/// 06.1 D6.c: drive a sequence of HCM-side `PreRequest`s (so the registry has
/// counters incremented), sleep ~50ms (per SPEC §6 signpost 11 to let
/// Relaxed-ordered counter writes become visible to the scrape), then scrape
/// the admin listener at `path`. Returns the admin response shape so the
/// caller can dispatch on `expected_status` / `expected_content_type` /
/// `expected_body_rule`.
///
/// `hcm_addrs` maps `PreRequest.port_key` (e.g. `"PORT"`) to the per-side
/// listener address. Each pre-request resolves its port via this map; missing
/// keys surface as `Err`. 06.1 only uses `"PORT"` (the single HCM listener),
/// but the map shape matches the existing template-marker discipline so
/// future fixtures with multiple listeners (e.g. ingress + egress) slot in
/// without harness churn.
///
/// `method` strings on each `PreRequest` are converted to `Http1Method` here;
/// 06.1 only supports `GET` (case-insensitive). Other methods bail with a
/// descriptive error so a fixture-time typo surfaces immediately rather than
/// silently driving a `GET`.
pub async fn drive_admin_scrape(
    pre_requests: &[PreRequest],
    admin_addr: SocketAddr,
    hcm_addrs: &std::collections::BTreeMap<String, SocketAddr>,
    path: &str,
) -> Result<DriveHttp1Result> {
    for pre in pre_requests {
        let addr = hcm_addrs
            .get(&pre.port_key)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown PreRequest.port_key: {}", pre.port_key))?;
        let method = match pre.method.to_ascii_uppercase().as_str() {
            "GET" => Http1Method::Get,
            other => bail!(
                "PreRequest.method {other:?} not supported in 06.1 (only GET); widen drive_admin_scrape to add more"
            ),
        };
        let _ = drive_http1(addr, &method, &pre.path, &pre.host, &[], None)
            .await
            .with_context(|| {
                format!(
                    "pre-request {} {} (host={}, port_key={})",
                    pre.method, pre.path, pre.host, pre.port_key,
                )
            })?;
    }

    // SPEC §6 signpost 11: sleep ~50ms so the registry's Relaxed-ordered
    // counter writes are visible to the scrape's read. Do NOT shorten — the
    // exact figure is the documented worst-case Relaxed-ordering visibility
    // window for Counter::inc on x86_64+ARM under the std::sync::atomic
    // happens-before relations the registry relies on. Guarded on
    // `pre_requests.is_empty()` so subsequent sub-cases (which pass `&[]`)
    // hit the admin listener directly without re-paying the 50ms budget.
    if !pre_requests.is_empty() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    drive_http1(
        admin_addr,
        &Http1Method::Get,
        path,
        "admin.local",
        &[],
        None,
    )
    .await
    .with_context(|| format!("admin scrape GET {path}"))
}

/// 13.1 D10 Task 7: read one HTTP/1.1 response from `stream` (status
/// line, then headers, then Content-Length-framed body), return the
/// status code, and leave the stream positioned at the next response's
/// first byte so the caller's `Driver::Http1KeepAlive` loop can issue
/// the next request on the same keep-alive conn without staling on the
/// previous response's unread bytes.
///
/// The helper assumes Content-Length framing exclusively (no
/// `Transfer-Encoding: chunked`); the configurable-status backend
/// always emits Content-Length explicitly per
/// `tests/helpers/health-aware-http1-backend/src/main.rs` and the H1
/// router-arm pool path writes Content-Length-framed responses too. If
/// a future driver needs chunked handling, mirror `drive_http1`'s
/// chunked decoding cascade.
pub async fn read_h1_response_status<R>(stream: &mut R) -> Result<u16>
where
    R: tokio::io::AsyncRead + Unpin,
{
    // 14.2 D8.1a: delegate to `read_h1_response_full` so there is a SINGLE
    // Content-Length framing/drain implementation. The full reader consumes
    // exactly the status line + headers + Content-Length body, leaving the
    // keep-alive conn positioned at the next response's first byte — the
    // same on-wire behavior this function had before the refactor.
    let (status, _headers, _body) = read_h1_response_full(stream).await?;
    Ok(status)
}

/// 14.2 D8.1a (SPEC correction B-3): like `read_h1_response_status` but also
/// returns the response headers (names lower-cased) and the
/// Content-Length-delimited body, so the `Driver::Http1KeepAlive` driver can
/// assert per-request body bytes + header presence/absence in addition to the
/// status. Consumes EXACTLY one full response (status line + headers +
/// Content-Length body) so the keep-alive conn stays correctly positioned for
/// the next pipelined/sequential request.
///
/// Framing scope matches the prior `read_h1_response_status`: Content-Length
/// exclusively (no `Transfer-Encoding: chunked`), defaulting to a zero-length
/// body when no `Content-Length` header is present. The configurable-status
/// backend + the H1 router-arm pool path both emit Content-Length explicitly;
/// if a future driver needs chunked handling, mirror `drive_http1`'s chunked
/// decoding cascade.
#[allow(clippy::type_complexity)]
pub async fn read_h1_response_full<R>(
    stream: &mut R,
) -> Result<(u16, Vec<(String, String)>, Vec<u8>)>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    // Read up to and including the status line terminator ("\r\n").
    let mut status_line: Vec<u8> = Vec::with_capacity(64);
    let mut prev = 0u8;
    loop {
        let mut byte = [0u8; 1];
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            bail!("EOF before status line complete");
        }
        status_line.push(byte[0]);
        if prev == b'\r' && byte[0] == b'\n' {
            break;
        }
        prev = byte[0];
    }
    // Read headers until the blank line ("\r\n\r\n") that terminates them.
    let mut header_buf: Vec<u8> = Vec::with_capacity(512);
    loop {
        let mut byte = [0u8; 1];
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            bail!("EOF before headers complete");
        }
        header_buf.push(byte[0]);
        if header_buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    // Parse headers into (lower-cased name, trimmed value) pairs and pick up
    // Content-Length (case-insensitive) — default to a 0-length body if absent.
    let headers_str = std::str::from_utf8(&header_buf).unwrap_or("");
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut cl: usize = 0;
    for line in headers_str.split("\r\n") {
        let (name, value) = match line.split_once(':') {
            Some(p) => p,
            None => continue,
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            continue;
        }
        if name.eq_ignore_ascii_case("content-length") {
            cl = value.parse().unwrap_or(0);
        }
        headers.push((name.to_ascii_lowercase(), value.to_string()));
    }
    // Read EXACTLY the Content-Length body so the next request on this
    // keep-alive conn starts at a clean response boundary.
    let mut body = vec![0u8; cl];
    if cl > 0 {
        stream.read_exact(&mut body).await?;
    }
    // Parse the status from the captured status line
    // (`HTTP/1.1 <status> <reason>\r\n`).
    let status_line_str = std::str::from_utf8(&status_line)
        .with_context(|| format!("status line is not UTF-8: {status_line:?}"))?;
    let parts: Vec<&str> = status_line_str.split_whitespace().collect();
    if parts.len() < 2 {
        bail!("malformed status line: {status_line_str:?}");
    }
    let status: u16 = parts[1]
        .parse()
        .with_context(|| format!("parsing status code {:?}", parts[1]))?;
    Ok((status, headers, body))
}

/// 13.1 D10 Task 7: GET `/stats` from the admin listener at `admin_addr`
/// and return the value of the named stat as `u64`. Returns 0 if the
/// stat is absent (Envoy's text-format `/stats` endpoint omits names
/// that have not been registered; envoy-rust matches per
/// `crates/envoy-admin/src/endpoint.rs::render_stats`).
///
/// Both proxies emit the same `<name>: <value>\n` per-line shape so the
/// per-line parse here is shared across the bilateral assertion.
pub async fn scrape_admin_stat(admin_addr: SocketAddr, stat_name: &str) -> Result<u64> {
    let resp = drive_http1(
        admin_addr,
        &Http1Method::Get,
        "/stats",
        "admin.local",
        &[],
        None,
    )
    .await
    .with_context(|| format!("GET /stats from {admin_addr}"))?;
    let body = std::str::from_utf8(&resp.body).context("/stats body is not UTF-8")?;
    for line in body.lines() {
        if let Some((name, value)) = line.split_once(": ")
            && name.trim() == stat_name
        {
            return value
                .trim()
                .parse::<u64>()
                .with_context(|| format!("parsing stat value {value:?} for {stat_name}"));
        }
    }
    Ok(0)
}

/// 08.2 Task 7 (D16): issue a POST against an admin listener at `path`
/// and assert the response status equals `expected_status`. Used by
/// the `Driver::AdminScrape::pre_admin_actions` dispatch arm to drive
/// `/drain_listeners` (and future admin POSTs) before the scrape loop.
///
/// Mirrors `drive_admin_scrape`'s wire-shape conventions: connects via
/// raw TCP, writes a minimal HTTP/1.1 request (zero-length body,
/// `Host: admin.local`, `Connection: close`), parses the response head
/// via `httparse`, and discards the body. The 5s per-poll read timeout
/// matches `drive_http1`'s budget.
pub async fn drive_admin_post(
    admin_addr: SocketAddr,
    path: &str,
    expected_status: u16,
) -> Result<()> {
    let mut stream = tokio::net::TcpStream::connect(admin_addr)
        .await
        .with_context(|| format!("connecting to {admin_addr} for admin POST {path}"))?;
    let req = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: admin.local\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .await
        .with_context(|| format!("write admin POST {path}"))?;
    stream.flush().await.ok();

    let read_timeout = Duration::from_secs(5);
    let mut buf: Vec<u8> = Vec::with_capacity(2048);
    let status = loop {
        let mut chunk = [0u8; 2048];
        let n = tokio::time::timeout(read_timeout, stream.read(&mut chunk))
            .await
            .with_context(|| format!("admin POST {path}: read timeout"))?
            .with_context(|| format!("admin POST {path}: read error"))?;
        if n == 0 {
            bail!("admin POST {path}: unexpected EOF before headers complete");
        }
        buf.extend_from_slice(&chunk[..n]);

        let mut hp_headers = [httparse::EMPTY_HEADER; 32];
        let mut resp = httparse::Response::new(&mut hp_headers);
        match resp
            .parse(&buf)
            .with_context(|| format!("admin POST {path}: response parse"))?
        {
            httparse::Status::Complete(_) => {
                break resp
                    .code
                    .ok_or_else(|| anyhow::anyhow!("admin POST {path}: no status code"))?;
            }
            httparse::Status::Partial => continue,
        }
    };

    if status != expected_status {
        bail!("admin POST {path}: response status {status} != expected {expected_status}",);
    }
    Ok(())
}

/// 08.2 Task 7 (D16): poll `addr` in 100ms intervals until `within`
/// elapses; succeed on the first observation of ANY of THREE
/// dispositions, all treated as evidence the listener is drained:
///
/// 1. **ECONNREFUSED** (or any connect error) — the kernel-level
///    "no listener" signal.
/// 2. **Immediate-EOF after connect** — kernel still accepts because
///    the listening fd is alive on this side, but server-side
///    immediately FINs the accepted socket without writing any bytes
///    (read returns `Ok(0)`).
/// 3. **Ungraceful close (RST) after connect** — server-side RSTs the
///    accepted socket mid-handshake; the read returns `Err`
///    (ECONNRESET on Unix). Some drain configurations RST in-flight
///    connections rather than FINing cleanly; this is the third
///    disposition per PLAN.md worked example (lines 2282-2286).
///
/// Returns `Err` if NONE of the three dispositions is observed before
/// `within` elapses; the error names the address and the
/// last-observed live-listener disposition (read bytes or read
/// timeout).
pub async fn assert_data_plane_connection_refused(
    addr: SocketAddr,
    within: Duration,
) -> Result<()> {
    let deadline = std::time::Instant::now() + within;
    let poll_interval = Duration::from_millis(100);
    // Diagnostic surfaced on deadline expiry. Every live-listener
    // loop arm (connect-timeout, read-bytes, read-timeout) overwrites
    // this binding before the deadline-reached branch can fire — the
    // initial sentinel here is the surface-of-record IFF the deadline
    // expires before any live-listener arm has run (which would
    // require every prior loop pass to have hit a success arm and
    // returned early; logically unreachable). Carries a real
    // diagnostic string so the M3 closure (remove
    // `#[allow(unused_assignments)]`) is now sound: the binding's
    // initial value is itself a valid output of the deadline branch.
    let mut last_disposition = format!("no live-listener disposition observed before {within:?}");
    loop {
        // Wrap connect in a 200ms timeout per PLAN.md worked example
        // (lines 2257-2261) so a slow accept does not erode the
        // deadline budget beyond the poll interval.
        match tokio::time::timeout(
            Duration::from_millis(200),
            tokio::net::TcpStream::connect(addr),
        )
        .await
        {
            Ok(Err(e)) => {
                // Connect error — ECONNREFUSED disposition. Drain success.
                let _ = e;
                return Ok(());
            }
            Err(_timeout) => {
                // Connect timed out — listener accepted slowly enough
                // that we exceeded the per-attempt budget. Treat as
                // failure-and-continue (slow accept ⇒ listener is
                // still live in some form); record + re-poll.
                last_disposition = format!("connect timed out (>200ms) to {addr}");
            }
            Ok(Ok(mut s)) => {
                // Connect succeeded. Read with a short timeout (50ms
                // per PLAN.md worked example line 2271): Ok(0) ⇒
                // immediate-EOF drain success; Err ⇒ ungraceful-close
                // drain success; Ok(n) or timeout ⇒ live listener,
                // re-poll.
                let mut tail = [0u8; 64];
                match tokio::time::timeout(Duration::from_millis(50), s.read(&mut tail)).await {
                    Ok(Ok(0)) => return Ok(()),
                    Ok(Err(_err)) => {
                        // Read Err (ECONNRESET / etc.) — ungraceful
                        // close. Drain success per PLAN.md worked
                        // example lines 2282-2286.
                        return Ok(());
                    }
                    Ok(Ok(n)) => {
                        last_disposition = format!(
                            "live listener responded with {n} bytes: {:?}",
                            String::from_utf8_lossy(&tail[..n]),
                        );
                    }
                    Err(_) => {
                        last_disposition =
                            "live listener kept connection open without writing (read timeout)"
                                .into();
                    }
                }
                drop(s);
            }
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "assert_data_plane_connection_refused({addr}, within={within:?}): \
                 listener did not refuse within deadline; last disposition: {last_disposition}",
            );
        }
        tokio::time::sleep(poll_interval).await;
    }
}

/// Decode HTTP/1.1 chunked transfer-encoded body bytes into plain body bytes.
/// Each chunk has the form `<hex-size>\r\n<data>\r\n`; the last chunk is
/// `0\r\n\r\n`. Trailer headers (if any) are ignored. Returns an error if the
/// wire bytes do not conform to the chunked framing grammar.
fn decode_chunked(wire: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        // Find the CRLF that terminates the chunk-size line.
        let crlf = wire[pos..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| anyhow::anyhow!("missing CRLF after chunk size at offset {pos}"))?;
        let size_line = std::str::from_utf8(&wire[pos..pos + crlf])
            .context("chunk size line is not UTF-8")?
            .trim();
        // Strip optional chunk extensions (`;ext=val`).
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(size_hex, 16)
            .with_context(|| format!("invalid chunk size hex: {size_hex:?}"))?;
        pos += crlf + 2; // advance past size line + CRLF
        if chunk_size == 0 {
            // Last chunk — ignore optional trailers.
            break;
        }
        if pos + chunk_size + 2 > wire.len() {
            bail!(
                "chunk data truncated: need {} bytes at offset {pos}, have {}",
                chunk_size + 2,
                wire.len() - pos,
            );
        }
        out.extend_from_slice(&wire[pos..pos + chunk_size]);
        pos += chunk_size + 2; // advance past data + trailing CRLF
    }
    Ok(out)
}

/// Shared per-fixture state threaded from `run_fixture`'s setup phase into
/// the per-`Driver` dispatch-arm fns (`run_*_arm` below). Carries only
/// borrows and `Copy` scalars; the `upstream`/`subject` proxy handles are
/// passed to each arm by value instead — every arm consumes them
/// (`subject.shutdown(..)` + `drop(upstream)` at its tail).
#[derive(Clone, Copy)]
struct FixtureCtx<'a> {
    fixture_dir: &'a Path,
    expectations: &'a Expectations,
    upstream_addr: SocketAddr,
    subject_addr: SocketAddr,
    /// 06.1 D6.a: reserved host admin port for the subject (only `Some` for
    /// the AdminScrape / *KeepAlive fixtures whose templates reference
    /// `{{ADMIN_PORT}}`).
    admin_host_port: Option<u16>,
    /// Accept-ready wait budget (shared with the admin-listener waits).
    budget: Duration,
    tls_pki: &'a Option<crate::tls::TlsTestPki>,
    upstream_kvs_refs: &'a [(&'a str, &'a str)],
    subject_kvs_refs: &'a [(&'a str, &'a str)],
    upstream_rds_path: &'a Option<PathBuf>,
    subject_rds_path: &'a Path,
    upstream_eds_path: &'a Option<PathBuf>,
    subject_eds_path: &'a Path,
}

/// End-to-end run of one fixture. Panics-on-failure paths unwind through Drop
/// guards so the container and envoy-rust subprocess are cleaned up even on
/// assertion failure.
/// Which `{{…}}` token the fixture's data listener port substitutes into.
/// Extracted from `run_fixture` at 67.1 D7 so it is unit-testable.
fn port_key_for(driver: &Driver) -> &'static str {
    match driver {
        Driver::TcpEcho
        // Phase 66 (ADR-0123): the direct_response listener uses the same
        // {{PORT}} convention as the other raw-TCP drivers.
        | Driver::TcpDirectResponse
        // 67.1 D7: TcpWithStats drives a `{{PORT}}` data listener like the
        // other raw-TCP drivers; its admin listener is separately wired via
        // `{{ADMIN_PORT}}` (see `driver_needs_admin_port`).
        | Driver::TcpWithStats { .. }
        | Driver::TlsTcp { .. }
        | Driver::TlsTcpProbeList { .. }
        | Driver::Http1 { .. }
        | Driver::Http1ProbeList { .. }
        | Driver::Http1WithAccessLog { .. }
        // Phase 32 Task 6 (ADR-0079): the byte-exact access-log driver runs
        // over the same {{PORT}} H1 listener convention as the other
        // HCM-shaped drivers.
        | Driver::Http1AccessLogByteExact { .. }
        | Driver::Http1AfterSettle { .. }
        // Phase 69 (ADR-0138): Http2AfterSettle's HCM listener uses {{PORT}}
        // like its H1 sibling above.
        | Driver::Http2AfterSettle { .. }
        // 13.1 D10: Http1KeepAlive's HCM listener uses {{PORT}} like
        // the other HCM-shaped drivers; the admin listener is wired via
        // {{ADMIN_PORT}} (see needs_admin_port below).
        | Driver::Http1KeepAlive { .. }
        // 13.2 Task 5 (ADR-0039): Http2KeepAlive's downstream H2 listener
        // also uses {{PORT}}; the admin port plumbing mirrors the H1
        // sibling (see needs_admin_port below).
        | Driver::Http2KeepAlive { .. }
        | Driver::Http2 { .. }
        | Driver::Http2ProbeList { .. }
        // Phase 56 (ADR-0113): the H2 access-log byte-exact driver runs over
        // the same {{PORT}} H2C listener convention as its H1 sibling and
        // the other HCM-shaped drivers.
        | Driver::Http2AccessLogByteExact { .. }
        // 06.1 D6.a: AdminScrape's HCM listener uses {{PORT}} like the other
        // HCM-shaped drivers. The admin listener is separately substituted
        // via {{ADMIN_PORT}} (see admin_host_port reservation below).
        | Driver::AdminScrape { .. }
        // 26 Task 7: the RDS-hot-reload driver runs over an HCM `{{PORT}}`
        // listener like the other HTTP drivers; the reload swaps the
        // file-based RouteConfiguration out from under that listener.
        // 28 Task 7 (ADR-0070): RING_HASH sweep uses the same {{PORT}}
        // data-listener convention as the other HCM-shaped drivers.
        | Driver::Http1HashSweep { .. }
        // 30 Task 7 (ADR-0074): the subset route-selection driver runs over the
        // same {{PORT}} data-listener convention as the other HCM-shaped drivers.
        | Driver::Http1RouteSelect { .. }
        | Driver::Http1RdsReload { .. }
        // 27 Task 6 (D6): the EDS-hot-reload driver runs over an HCM `{{PORT}}`
        // listener like the other HTTP drivers; the reload swaps the file-based
        // EDS endpoint set out from under that listener's cluster.
        | Driver::Http1EdsReload { .. } => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    }
}

/// Does this driver need an admin listener exposed on BOTH proxies?
///
/// Gates three things at once: the subject's host admin-port reservation, the
/// upstream container's `expose_admin_port`, and the `ADMIN_PORT` kv injected
/// into the upstream template. `run_fixture` ALSO requires `{{ADMIN_PORT}}` to
/// appear in one of the templates.
///
/// 67.1 D7: `TcpWithStats` joins the three HTTP keep-alive/scrape drivers here —
/// its post-settle bilateral stat scrape is the whole reason it exists.
fn driver_needs_admin_port(driver: &Driver) -> bool {
    matches!(
        driver,
        Driver::AdminScrape { .. }
            | Driver::Http1KeepAlive { .. }
            | Driver::Http2KeepAlive { .. }
            | Driver::TcpWithStats { .. }
    )
}

pub async fn run_fixture(fixture_dir: &Path) -> Result<()> {
    let expectations = load_expectations(&fixture_dir.join("expectations.yaml"))?;

    let host_port = reserve_port()?;

    let tmp = tempfile::tempdir().context("creating fixture temp dir")?;
    let upstream_template = std::fs::read_to_string(fixture_dir.join("envoy.yaml"))
        .context("reading upstream envoy.yaml")?;
    let subject_template = std::fs::read_to_string(fixture_dir.join("envoy-rust.yaml"))
        .context("reading envoy-rust.yaml")?;

    let upstream_port_str = upstream::CONTAINER_PORT.to_string();
    let subject_port_str = host_port.to_string();
    let port_key = port_key_for(&expectations.driver);

    // 06.1 D6.a: reserve a kernel-ephemeral admin port whenever either
    // template references `{{ADMIN_PORT}}` AND the driver is AdminScrape
    // (the other consumer, Driver::HttpGet, drives admin via the single
    // listener port and does not need a separate reservation). Mirrors
    // the existing `_backend` / `_tls_backend` cadence: the reservation
    // happens once at run_fixture start so kvs and dispatch both see it.
    // 13.1 D10: `Driver::Http1KeepAlive` also needs the admin listener
    // exposed for the post-settle bilateral stat scrape (fixture 0020's
    // per-class counter assertion territory). Same template-marker discipline
    // — only reserve the host admin port when one of the YAMLs references
    // `{{ADMIN_PORT}}`.
    let needs_admin_port = driver_needs_admin_port(&expectations.driver)
        && (upstream_template.contains("{{ADMIN_PORT}}")
            || subject_template.contains("{{ADMIN_PORT}}"));
    let admin_host_port: Option<u16> = if needs_admin_port {
        Some(reserve_port().context("reserving admin host port for Driver::AdminScrape")?)
    } else {
        None
    };

    // (a) Detect TLS templates — if any TLS substitution token appears in
    // either template, generate a fresh TlsTestPki for this fixture run.
    let needs_tls_pki = upstream_template.contains("{{LEAF_A_CERT_PATH}}")
        || upstream_template.contains("{{LEAF_A_KEY_PATH}}")
        || upstream_template.contains("{{CA_PATH}}")
        || upstream_template.contains("{{LEAF_B_CERT_PATH}}")
        || upstream_template.contains("{{SERVER_CERT_PATH}}")
        || subject_template.contains("{{LEAF_A_CERT_PATH}}")
        || subject_template.contains("{{CA_PATH}}");
    let tls_pki = if needs_tls_pki {
        Some(crate::tls::TlsTestPki::generate().context("generating TLS test PKI")?)
    } else {
        None
    };

    // 18 Task 6 (ADR-0049): detect file-based CDS. When either main template
    // references `{{CDS_PATH}}`, the fixture carries a `cds.yaml` template that
    // is rendered TWICE (once per side, through the same per-side kv map so
    // `{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}` etc. get per-side values),
    // written to the temp dir, and threaded into each side's config:
    //   - upstream side: `{{CDS_PATH}}` → `CDS_CONTAINER_PATH` (a `.yaml`-
    //     suffixed container constant per L1); the rendered upstream file is
    //     copied into the container via `upstream::start(.., cds_file, ..)`.
    //   - subject side: `{{CDS_PATH}}` → the host temp path of the rendered
    //     subject CDS file (the subject runs as a host subprocess and reads
    //     the file directly).
    // The CDS render/write happens AFTER the per-side kv maps are built (so
    // the kv maps drive the CDS render) but the CDS_PATH marker value is a
    // per-side CONSTANT known up-front, so it is added to each kv map before
    // the maps are used — same dependency shape as the TLS-PKI paths.
    let needs_cds =
        upstream_template.contains("{{CDS_PATH}}") || subject_template.contains("{{CDS_PATH}}");
    let cds_template = if needs_cds {
        Some(
            std::fs::read_to_string(fixture_dir.join("cds.yaml"))
                .context("reading cds.yaml (fixture references {{CDS_PATH}})")?,
        )
    } else {
        None
    };
    // Subject-side host path is known before rendering (the temp dir exists);
    // the upstream-side value is the container constant.
    let subject_cds_path = tmp.path().join("cds-subject.yaml");
    let subject_cds_path_str = subject_cds_path.to_string_lossy().into_owned();

    // 19 Task 6 (ADR-0050): detect file-based LDS. When either main template
    // references `{{LDS_PATH}}`, the fixture carries PER-SIDE LDS templates —
    // `lds-envoy.yaml` (upstream) and `lds-envoy-rust.yaml` (subject). Unlike
    // CDS (a single SHARED `cds.yaml` rendered twice), the LDS payload carries
    // the HCM, whose Envoy-only fields (`generate_request_id`,
    // `request_headers_to_remove`) the envoy-rust parser rejects — so the two
    // sides need DIFFERENT LDS files, mirroring the `envoy.yaml`/
    // `envoy-rust.yaml` main-config split. Each per-side file is rendered
    // through that side's kv map (so `{{BACKEND_HOST}}`/`{{HTTP1_BACKEND_PORT}}`
    // etc. get per-side values). A missing per-side file is a hard error
    // naming the expected filename.
    //   - upstream side: `{{LDS_PATH}}` → `LDS_CONTAINER_PATH` (a `.yaml`-
    //     suffixed container constant per L1); the rendered upstream file is
    //     copied into the container via `upstream::start(.., lds_file, ..)`.
    //   - subject side: `{{LDS_PATH}}` → the host temp path of the rendered
    //     subject LDS file (the subject runs as a host subprocess and reads
    //     the file directly).
    let needs_lds =
        upstream_template.contains("{{LDS_PATH}}") || subject_template.contains("{{LDS_PATH}}");
    let (upstream_lds_template, subject_lds_template): (Option<String>, Option<String>) =
        if needs_lds {
            let up = std::fs::read_to_string(fixture_dir.join("lds-envoy.yaml")).context(
                "reading lds-envoy.yaml (fixture references {{LDS_PATH}}; \
                 the upstream per-side LDS template)",
            )?;
            let subj = std::fs::read_to_string(fixture_dir.join("lds-envoy-rust.yaml")).context(
                "reading lds-envoy-rust.yaml (fixture references {{LDS_PATH}}; \
                 the subject per-side LDS template)",
            )?;
            (Some(up), Some(subj))
        } else {
            (None, None)
        };
    let subject_lds_path = tmp.path().join("lds-subject.yaml");
    let subject_lds_path_str = subject_lds_path.to_string_lossy().into_owned();

    // 20 Task 6 (ADR-0052): detect file-based RDS. When either main template
    // references `{{RDS_PATH}}`, the fixture carries a SINGLE SHARED `rds.yaml`
    // template — UNLIKE LDS (per-side `lds-envoy.yaml`/`lds-envoy-rust.yaml`
    // because the HCM payload carries Envoy-only fields) and LIKE CDS: an RDS
    // file carries only a bare `RouteConfiguration` (name + virtual_hosts),
    // which envoy-rust accepts as-is, so ONE shared template is rendered twice
    // (once per side, through the same per-side kv map so backend host/port
    // markers resolve per-side).
    //   - upstream side: `{{RDS_PATH}}` → `RDS_CONTAINER_PATH` (a `.yaml`-
    //     suffixed container constant per L1); the rendered upstream file is
    //     copied into the container via `upstream::start(.., rds_file, ..)`.
    //   - subject side: `{{RDS_PATH}}` → the host temp path of the rendered
    //     subject RDS file (the subject runs as a host subprocess and reads
    //     the file directly).
    let needs_rds =
        upstream_template.contains("{{RDS_PATH}}") || subject_template.contains("{{RDS_PATH}}");
    let rds_template = if needs_rds {
        Some(
            std::fs::read_to_string(fixture_dir.join("rds.yaml"))
                .context("reading rds.yaml (fixture references {{RDS_PATH}})")?,
        )
    } else {
        None
    };
    let subject_rds_path = tmp.path().join("rds-subject.yaml");
    let subject_rds_path_str = subject_rds_path.to_string_lossy().into_owned();

    // 21 Task 6 (ADR-0054): detect file-based EDS. When either main template
    // references `{{EDS_PATH}}`, the fixture carries a SINGLE SHARED `eds.yaml`
    // template — LIKE CDS (one shared `cds.yaml` rendered twice through each
    // side's own kv map) and UNLIKE LDS (per-side `lds-envoy.yaml`/
    // `lds-envoy-rust.yaml`): an EDS file carries only a bare
    // `ClusterLoadAssignment` (cluster_name + endpoints), which both proxies
    // accept, so ONE shared template is rendered twice. BUT the endpoint
    // `socket_address.address` must be a NUMERIC IP that differs per side (L1 —
    // EDS rejects hostnames), so the shared template carries a NEW
    // `{{EDS_BACKEND_IP}}` marker resolved per-side (upstream → the
    // runtime-discovered numeric host-gateway IP; subject → `127.0.0.1`).
    //   - upstream side: `{{EDS_PATH}}` → `EDS_CONTAINER_PATH` (a `.yaml`-
    //     suffixed container constant per L1); the rendered upstream file is
    //     copied into the container via `upstream::start(.., eds_file, ..)`.
    //   - subject side: `{{EDS_PATH}}` → the host temp path of the rendered
    //     subject EDS file (the subject runs as a host subprocess and reads
    //     the file directly).
    let needs_eds =
        upstream_template.contains("{{EDS_PATH}}") || subject_template.contains("{{EDS_PATH}}");
    let eds_template = if needs_eds {
        Some(
            std::fs::read_to_string(fixture_dir.join("eds.yaml"))
                .context("reading eds.yaml (fixture references {{EDS_PATH}})")?,
        )
    } else {
        None
    };
    let subject_eds_path = tmp.path().join("eds-subject.yaml");
    let subject_eds_path_str = subject_eds_path.to_string_lossy().into_owned();
    // 21 Task 6 (ADR-0054; §6.2 L9): discover the NUMERIC host-gateway IP the
    // upstream Envoy container uses to reach the host backend — gated to EDS
    // fixtures, run ONCE when `needs_eds` (the only consumer today is fixture
    // 0029). EDS rejects hostnames, so the upstream EDS file's endpoint address
    // must be this numeric IP; the subject side uses `127.0.0.1`.
    // 28 Task 7 (ADR-0070): the RING_HASH differential needs BOTH proxies to
    // build their ring from the *identical* endpoint address strings — the ring
    // key is `xxh64("{ip:port}_{i}")`, so divergent address strings give
    // divergent rings and the cross-proxy STRONG target cannot hold. The EDS
    // per-side numeric-IP split (`{{EDS_BACKEND_IP}}` → host-gateway IP upstream
    // / `127.0.0.1` subject) would defeat this. Instead the `{{BACKEND_IP}}`
    // marker renders to ONE shared address on BOTH sides: the host's primary
    // non-loopback LAN IPv4, which the subject (a host process) reaches directly
    // and the upstream container reaches via the Docker bridge/VM NAT (verified
    // reachable from both). A STATIC cluster also rejects hostnames, so this
    // must be a numeric IP — the same numeric-IP requirement EDS has.
    let needs_backend_ip =
        upstream_template.contains("{{BACKEND_IP}}") || subject_template.contains("{{BACKEND_IP}}");
    let shared_backend_ip = if needs_backend_ip {
        Some(discover_host_lan_ip()?)
    } else {
        None
    };
    let host_gateway_ip = if needs_eds {
        Some(discover_host_gateway_ip()?)
    } else {
        None
    };

    // Spawn a host-local backend if either template needs one. Holding the
    // backend in a binding outside the proxies' lifetime ensures the child
    // process outlives the fixture run; Drop fires after `run_fixture`'s
    // returns paths.
    //
    // 12.2 Task 5 (D7.2): fixture `0019-upstream-active-health-check` is the
    // FIRST consumer of `{{BACKEND_PORT}}` that needs an HTTP/1.1 health-aware
    // backend (`HealthAwareHttp1Backend` — 200 on `/`, 503 on `/healthz`)
    // instead of the default TCP-echo backend (`TcpProxyBackend`). Per PLAN
    // Task 5 Step 8: follow `0008-http1-router-upstream`'s `Http1EchoBackend`
    // arm verbatim, substituting `HealthAwareHttp1Backend` — the principle is
    // backend-struct substitution at the same dispatch site. Since the
    // existing harness is template-marker-driven (single
    // `{{BACKEND_PORT}}` token → single backend), the per-fixture dispatch
    // here is keyed on the fixture directory name to keep the existing
    // `{{BACKEND_PORT}}` consumers (fixtures 0003/0004/0005/0006/0013/0014)
    // unchanged on `TcpProxyBackend`.
    let fixture_name = fixture_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    // 18 Task 6 (ADR-0049): backend-launch detection scans the main templates
    // AND the CDS template. Phase-18 fixture 0026 places `{{BACKEND_HOST}}` and
    // the backend-port markers ONLY in `cds.yaml` (the main configs route to a
    // CDS-defined cluster and carry no static clusters), so a scan over only
    // `upstream_template`/`subject_template` would never spawn the backend and
    // the markers would render unsubstituted. The combined source threads the
    // CDS template (empty string when no `cds.yaml`) into every `needs_*`
    // check below via `scan_needs_marker`.
    let cds_scan = cds_template.as_deref().unwrap_or("");
    // 19 Task 6 (ADR-0050): the combined backend-launch scan must ALSO cover the
    // rendered LDS source (fixture 0027 places its backend markers ONLY in the
    // dynamically-loaded listener's HCM/route). The upstream LDS TEMPLATE (pre-
    // render) is scanned for the `{{MARKER}}` tokens — `scan_needs_marker` looks
    // for the literal `{{...}}` form, which is present in the unrendered
    // template. This is the same carryforward-disposition-2 bug-class lesson as
    // the phase-18 CDS scan extension: scan ALL sources or a CI-only 503 hides
    // until Linux. Empty string when the fixture carries no LDS template.
    let lds_scan = upstream_lds_template.as_deref().unwrap_or("");
    // 20 Task 6 (ADR-0052): the combined backend-launch scan must ALSO cover the
    // shared RDS template. For fixture 0028 the backend markers live in
    // `cds.yaml` (the `/dynamic` cluster) and the static cluster in the main
    // config; the RDS file references CLUSTER NAMES, not host/port markers — but
    // scan it anyway for symmetry/safety, per the phase-18/19 carryforward-
    // disposition-2 bug-class lesson (a marker living only in `rds.yaml` must
    // still spawn the backend, else a CI-only 503 hides until Linux). Empty
    // string when the fixture carries no RDS template.
    let rds_scan = rds_template.as_deref().unwrap_or("");
    // 21 Task 6 (ADR-0054): the combined backend-launch scan must ALSO cover the
    // shared EDS template. For fixture 0029 the backend endpoint lives ONLY in
    // `eds.yaml` (the cluster is `type: EDS` and carries no inline
    // `load_assignment`), so the `{{HTTP1_BACKEND_PORT}}` marker that drives the
    // echo-backend spawn is present ONLY in the unrendered EDS template — the
    // phase-18 carryforward-disposition-2 bug-class lesson (a marker living only
    // in a dynamic file must still spawn the backend, else a CI-only 503 hides
    // until Linux). Empty string when the fixture carries no EDS template.
    let eds_scan = eds_template.as_deref().unwrap_or("");
    let backend_scan_sources: [&str; 6] = [
        &upstream_template,
        &subject_template,
        cds_scan,
        lds_scan,
        rds_scan,
        eds_scan,
    ];
    let needs_backend = scan_needs_marker(&backend_scan_sources, "BACKEND_PORT");
    // 13.1 D9.1 / Task 7: fixture 0020 reuses `HealthAwareHttp1Backend` but
    // needs the helper's per-path status mapping
    // (`/301=301,/404=404,/500=500`) to span 2xx/3xx/4xx/5xx classes so the
    // post-settle bilateral counter scrape covers every class. The helper's
    // `--per-path` flag landed at Task 6 (D8); the gate keys on
    // fixture-directory name (mirrors the 0019 dispatch shape).
    let needs_health_aware_backend = needs_backend
        && (fixture_name == "0019-upstream-active-health-check"
            || fixture_name == "0020-upstream-connection-pooling-and-per-class-counters"
            || fixture_name == "0022-upstream-outlier-detection-consecutive-5xx"
            || fixture_name == "0024-upstream-retry-on-5xx"
            || fixture_name == "0025-upstream-circuit-breaker-retry-budget"
            || fixture_name == "0059-accesslog-rf-retry-exhausted"
            || fixture_name == "0067-accesslog-h2-urx-retry-exhausted");
    let _backend = if needs_backend && !needs_health_aware_backend {
        Some(
            backend::TcpProxyBackend::spawn()
                .await
                .context("spawning backend")?,
        )
    } else {
        None
    };
    let _health_aware_backend: Option<crate::backend::HealthAwareHttp1Backend> =
        if needs_health_aware_backend {
            // 13.1 D10: fixture 0020 needs per-path status mapping so each
            // GET drives a deterministic 2xx/3xx/4xx/5xx response class.
            // Fixture 0019 keeps the default-arms semantics (200 on `/`,
            // 503 on `/healthz`).
            // 16 Task 7 (fixture 0024): the retry-on-5xx fixture needs a
            // STATEFUL `/retry-success` path (503 "fail\n" on attempt 1, then
            // 200 "ok\n" on the retry — a single global per-path cyclic window,
            // fail:1 → 503,200,…, so each sequentially-driven proxy's
            // consecutive retry pair lands in its own window; NAT-immune on
            // macOS where Docker collapses source IPs to 127.0.0.1) alongside a
            // STATELESS `/retry-exhausted` path (always 503 "service unavailable\n").
            // Spawned via the dedicated `spawn_with_retry_script` arm below;
            // its `per_path` here stays None so this `spawn_with_per_path`
            // branch is not taken for 0024.
            let per_path =
                if fixture_name == "0020-upstream-connection-pooling-and-per-class-counters" {
                    Some("/301=301,/404=404,/500=500".to_string())
                } else if fixture_name == "0022-upstream-outlier-detection-consecutive-5xx" {
                    // 14.2 D8.1: fixture 0022 needs `/fail` to return a backend
                    // 500 ("server error\n", 13 bytes) so the consecutive_5xx
                    // detector ticks across requests 1-3 and ejects the sole
                    // endpoint; `/` keeps the default 200 (for the un-eject
                    // direction, exercised by the in-process backstop). Without
                    // this per-path arm the backend serves 200 on `/fail` and
                    // the ejection never fires.
                    Some("/fail=500".to_string())
                } else {
                    None
                };
            // 16 Task 7: fixture 0024 forwards a retry-script + per-path pair.
            // 17 Task 8: fixture 0025 reuses the same backend with a budget-keyed
            // pair — `/budget-blocked` always-503 (stateless, the max_retries:0
            // budget-blocked retry path) + `/budget-allowed=fail:1` (stateful
            // cyclic window, the within-cap retry-success control). `/rq-blocked`
            // needs no backend mapping — its request-budget gate rejects before
            // any upstream connect, so the backend is never contacted there.
            let retry_script = if fixture_name == "0024-upstream-retry-on-5xx" {
                Some("/retry-success=fail:1".to_string())
            } else if fixture_name == "0025-upstream-circuit-breaker-retry-budget" {
                Some("/budget-allowed=fail:1".to_string())
            } else {
                None
            };
            let per_path = if fixture_name == "0024-upstream-retry-on-5xx" {
                Some("/retry-exhausted=503".to_string())
            } else if fixture_name == "0025-upstream-circuit-breaker-retry-budget" {
                Some("/budget-blocked=503".to_string())
            } else if fixture_name == "0059-accesslog-rf-retry-exhausted"
                || fixture_name == "0067-accesslog-h2-urx-retry-exhausted"
            {
                // phase 51 (ADR-0108) fixture 0059 / phase 61 (ADR-0118)
                // fixture 0067: the H1/H2 retry-limit-exceeded (L9) access-log
                // %RESPONSE_FLAGS%=URX witnesses. STATELESS always-503
                // `/retry-exhausted` (retry_script stays None — both attempts
                // 503, the budget of 1 consumed, the last 503 surfaced
                // verbatim). Identical per-path mapping reused for both
                // fixtures — the retry loop is upstream-protocol-agnostic.
                Some("/retry-exhausted=503".to_string())
            } else {
                per_path
            };
            Some(
                crate::backend::HealthAwareHttp1Backend::spawn_with_retry_script(
                    retry_script,
                    per_path,
                )
                .await
                .context("spawning HealthAwareHttp1Backend")?,
            )
        } else {
            None
        };
    let backend_port_str = match (&_backend, &_health_aware_backend) {
        (Some(b), _) => Some(b.port().to_string()),
        (_, Some(b)) => Some(b.port().to_string()),
        _ => None,
    };

    // 03.2 Task 9: spawn a TlsEchoBackend if either template needs one.
    // Same alive-keeper binding-order discipline as `_backend` above — the
    // `Option<TlsEchoBackend>` outlives both proxies, and Drop fires after
    // `run_fixture` returns. Requires `tls_pki` to also be present (the
    // backend reads cert + key from the same PKI the upstream consults).
    let needs_tls_backend = scan_needs_marker(&backend_scan_sources, "TLS_BACKEND_PORT");
    let _tls_backend: Option<crate::backend::TlsEchoBackend> = if needs_tls_backend {
        let pki = tls_pki
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("TLS backend implies TLS pki shape"))?;
        Some(
            crate::backend::TlsEchoBackend::spawn(&pki.server_cert, &pki.server_key)
                .await
                .context("spawning TlsEchoBackend")?,
        )
    } else {
        None
    };
    let tls_backend_port_str = _tls_backend.as_ref().map(|b| b.port().to_string());

    // 04.3 Task 13: spawn an Http1EchoBackend if either template needs one.
    // Same alive-keeper binding-order discipline as `_backend` and
    // `_tls_backend` above — Drop fires after `run_fixture` returns.
    let needs_http1_backend = scan_needs_marker(&backend_scan_sources, "HTTP1_BACKEND_PORT");
    let _http1_backend: Option<crate::backend::Http1EchoBackend> = if needs_http1_backend {
        Some(
            crate::backend::Http1EchoBackend::spawn()
                .await
                .context("spawning Http1EchoBackend")?,
        )
    } else {
        None
    };
    let http1_backend_port_str = _http1_backend.as_ref().map(|b| b.port().to_string());

    // 27 Task 6 (D6 / §6.2-LOCKED V2 / ADR-0068): spawn a SECOND distinguishable
    // echo backend when the fixture references the EDS-reload markers. The
    // EDS-reload fixture (Task 7's 0035) needs TWO single-endpoint echo backends
    // distinguishable by a per-backend body marker so the `[backend_1]` →
    // `[backend_2]` endpoint swap is a REAL swap (a `GET /probe` response's
    // leading `backend: <marker>\n` line identifies which one served it).
    //
    // `backend_1` and `backend_2` are spawned independently, each gated on its
    // own `{{HTTP1_BACKEND_1_PORT}}` / `{{HTTP1_BACKEND_2_PORT}}` marker
    // (mirroring the single-backend `{{HTTP1_BACKEND_PORT}}` convention). The
    // markers' bare-token scan does NOT alias the existing `HTTP1_BACKEND_PORT`
    // token (exact-token match in `scan_needs_marker`), so all pre-27 fixtures
    // stay inert here. The `--body-marker` value is the fixed `backend_1` /
    // `backend_2` string the discriminator's `expected_body` matches.
    let needs_http1_backend_1 = scan_needs_marker(&backend_scan_sources, "HTTP1_BACKEND_1_PORT");
    let _http1_backend_1: Option<crate::backend::Http1EchoBackend> = if needs_http1_backend_1 {
        Some(
            crate::backend::Http1EchoBackend::spawn_with_marker("backend_1")
                .await
                .context("spawning Http1EchoBackend backend_1")?,
        )
    } else {
        None
    };
    let http1_backend_1_port_str = _http1_backend_1.as_ref().map(|b| b.port().to_string());

    let needs_http1_backend_2 = scan_needs_marker(&backend_scan_sources, "HTTP1_BACKEND_2_PORT");
    let _http1_backend_2: Option<crate::backend::Http1EchoBackend> = if needs_http1_backend_2 {
        Some(
            crate::backend::Http1EchoBackend::spawn_with_marker("backend_2")
                .await
                .context("spawning Http1EchoBackend backend_2")?,
        )
    } else {
        None
    };
    let http1_backend_2_port_str = _http1_backend_2.as_ref().map(|b| b.port().to_string());

    // 05.3 NEW per SPEC §3 D6.b: spawn Http2EchoBackend if either template
    // needs one. Same alive-keeper binding-order discipline as _backend /
    // _tls_backend / _http1_backend above.
    let needs_http2_backend = scan_needs_marker(&backend_scan_sources, "HTTP2_BACKEND_PORT");
    let _http2_backend: Option<crate::backend::Http2EchoBackend> = if needs_http2_backend {
        Some(
            crate::backend::Http2EchoBackend::spawn()
                .await
                .context("spawning Http2EchoBackend")?,
        )
    } else {
        None
    };
    let http2_backend_port_str = _http2_backend.as_ref().map(|b| b.port().to_string());

    // Phase 53 (ADR-0110): the accept-then-close upstream for the fixture-0061
    // reset/UC witness. Distinct from {{BACKEND_PORT}} (which routes to the
    // echoing TcpProxyBackend); this marker spawns the close-on-accept backend.
    let needs_close_backend = scan_needs_marker(&backend_scan_sources, "CLOSE_BACKEND_PORT");
    let _close_backend: Option<crate::backend::TcpCloseBackend> = if needs_close_backend {
        Some(
            crate::backend::TcpCloseBackend::spawn()
                .await
                .context("spawning TcpCloseBackend")?,
        )
    } else {
        None
    };
    let close_backend_port_str = _close_backend.as_ref().map(|b| b.port().to_string());

    // Phase 64 (ADR-0121): the H2-aware handshake-then-reset upstream for
    // the fixture-0069 reset/UC witness. Distinct from CLOSE_BACKEND_PORT
    // (a raw TCP accept-then-close backend, which envoy-rust's H2 client
    // would misclassify as a connect failure) — this marker spawns the
    // Http2CloseBackend (a genuine H2 handshake, then a stream-level reset).
    let needs_h2_close_backend = scan_needs_marker(&backend_scan_sources, "H2_CLOSE_BACKEND_PORT");
    let _h2_close_backend: Option<crate::backend::Http2CloseBackend> = if needs_h2_close_backend {
        Some(
            crate::backend::Http2CloseBackend::spawn()
                .await
                .context("spawning Http2CloseBackend")?,
        )
    } else {
        None
    };
    let h2_close_backend_port_str = _h2_close_backend.as_ref().map(|b| b.port().to_string());

    // 68 (ADR-0137 PV-2): a hermetic REFUSED port — reserve an ephemeral port
    // and spawn NO listener, so both proxies get ECONNREFUSED on the TCP HC
    // probe. `reserve_port` skips ports already handed out to the proxies, so
    // nothing binds it for the test's duration.
    let needs_dead_backend = scan_needs_marker(&backend_scan_sources, "DEAD_BACKEND_PORT");
    let dead_backend_port_str: Option<String> = if needs_dead_backend {
        Some(
            reserve_port()
                .context("reserving DEAD_BACKEND_PORT")?
                .to_string(),
        )
    } else {
        None
    };

    // (c) Build per-side substitution maps with TLS path keys.
    // Type is Vec<(&str, String)> to accommodate owned strings from TLS paths.
    let upstream_tls_paths = tls_pki.as_ref().map(|p| p.envoy_side_paths());
    let subject_tls_paths = tls_pki.as_ref().map(|p| p.subject_side_paths());

    let upstream_kvs: Vec<(&str, String)> = {
        let mut v: Vec<(&str, String)> = vec![(port_key, upstream_port_str.clone())];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp.to_string()));
        }
        if let Some(tp) = tls_backend_port_str.as_deref() {
            v.push(("TLS_BACKEND_PORT", tp.to_string()));
        }
        if let Some(hp) = http1_backend_port_str.as_deref() {
            v.push(("HTTP1_BACKEND_PORT", hp.to_string()));
        }
        // 27 Task 6 (D6): the two distinguishable EDS-reload backends.
        if let Some(hp) = http1_backend_1_port_str.as_deref() {
            v.push(("HTTP1_BACKEND_1_PORT", hp.to_string()));
        }
        if let Some(hp) = http1_backend_2_port_str.as_deref() {
            v.push(("HTTP1_BACKEND_2_PORT", hp.to_string()));
        }
        if let Some(h2p) = http2_backend_port_str.as_deref() {
            v.push(("HTTP2_BACKEND_PORT", h2p.to_string()));
        }
        // Phase 53 (ADR-0110): the accept-then-close backend port.
        if let Some(cp) = close_backend_port_str.as_deref() {
            v.push(("CLOSE_BACKEND_PORT", cp.to_string()));
        }
        // Phase 64 (ADR-0121): the H2-aware handshake-then-reset backend port.
        if let Some(h2cp) = h2_close_backend_port_str.as_deref() {
            v.push(("H2_CLOSE_BACKEND_PORT", h2cp.to_string()));
        }
        // 68 (ADR-0137 PV-2): the hermetic refused port (no listener).
        if let Some(dp) = dead_backend_port_str.as_deref() {
            v.push(("DEAD_BACKEND_PORT", dp.to_string()));
        }
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
            || http1_backend_1_port_str.is_some()
            || http1_backend_2_port_str.is_some()
            || http2_backend_port_str.is_some()
            || close_backend_port_str.is_some()
            || h2_close_backend_port_str.is_some()
            || dead_backend_port_str.is_some()
        {
            // Per ADR-0015: container-side reaches the host backend via
            // host.docker.internal (with the harness's with_host call below).
            // Generalized in Task 9 to fire for either backend variant; was
            // previously gated only on BACKEND_PORT (Task 8 cadence). Task 13
            // extends the gate to HTTP1_BACKEND_PORT. 05.3 Task 9 extends to
            // HTTP2_BACKEND_PORT. Phase 53 extends to CLOSE_BACKEND_PORT.
            // Phase 64 extends to H2_CLOSE_BACKEND_PORT.
            v.push(("BACKEND_HOST", "host.docker.internal".to_string()));
        }
        if needs_admin_port {
            // 06.1 D6.a: container-internal admin port. The host-mapped
            // admin port is read from `upstream.host_admin_port()` after
            // `upstream::start` returns and is plumbed into the dispatch
            // arm separately.
            v.push(("ADMIN_PORT", upstream::ADMIN_CONTAINER_PORT.to_string()));
        }
        if let Some(map) = upstream_tls_paths.as_ref() {
            for (k, val) in map {
                v.push((*k, val.clone()));
            }
        }
        if needs_cds {
            // 18 Task 6 (ADR-0049 L1): the container-internal `.yaml` path the
            // rendered upstream CDS file is copied to.
            v.push(("CDS_PATH", upstream::CDS_CONTAINER_PATH.to_string()));
        }
        if needs_lds {
            // 19 Task 6 (ADR-0050 L1): the container-internal `.yaml` path the
            // rendered upstream LDS file is copied to.
            v.push(("LDS_PATH", upstream::LDS_CONTAINER_PATH.to_string()));
        }
        if needs_rds {
            // 20 Task 6 (ADR-0052 L1): the container-internal `.yaml` path the
            // rendered SHARED upstream RDS file is copied to.
            v.push(("RDS_PATH", upstream::RDS_CONTAINER_PATH.to_string()));
        }
        if needs_eds {
            // 21 Task 6 (ADR-0054 L1): the container-internal `.yaml` path the
            // rendered SHARED upstream EDS file is copied to.
            v.push(("EDS_PATH", upstream::EDS_CONTAINER_PATH.to_string()));
            // 21 Task 6 (ADR-0054 L9): the NUMERIC host-gateway IP the upstream
            // EDS endpoint must use (EDS rejects hostnames). Discovered above
            // via `discover_host_gateway_ip` when `needs_eds`.
            if let Some(ip) = host_gateway_ip.as_deref() {
                v.push(("EDS_BACKEND_IP", ip.to_string()));
            }
        }
        if needs_backend_ip {
            // 28 Task 7 (ADR-0070): the SHARED host LAN IP — IDENTICAL on both
            // sides so both proxies build the same RING_HASH ring. The upstream
            // container reaches the host's 0.0.0.0-bound backends via this IP
            // (Docker bridge / Desktop-VM NAT).
            if let Some(ip) = shared_backend_ip.as_deref() {
                v.push(("BACKEND_IP", ip.to_string()));
            }
        }
        v
    };
    let subject_kvs: Vec<(&str, String)> = {
        let mut v: Vec<(&str, String)> = vec![(port_key, subject_port_str.clone())];
        if let Some(bp) = backend_port_str.as_deref() {
            v.push(("BACKEND_PORT", bp.to_string()));
        }
        if let Some(tp) = tls_backend_port_str.as_deref() {
            v.push(("TLS_BACKEND_PORT", tp.to_string()));
        }
        if let Some(hp) = http1_backend_port_str.as_deref() {
            v.push(("HTTP1_BACKEND_PORT", hp.to_string()));
        }
        // 27 Task 6 (D6): the two distinguishable EDS-reload backends.
        if let Some(hp) = http1_backend_1_port_str.as_deref() {
            v.push(("HTTP1_BACKEND_1_PORT", hp.to_string()));
        }
        if let Some(hp) = http1_backend_2_port_str.as_deref() {
            v.push(("HTTP1_BACKEND_2_PORT", hp.to_string()));
        }
        if let Some(h2p) = http2_backend_port_str.as_deref() {
            v.push(("HTTP2_BACKEND_PORT", h2p.to_string()));
        }
        // Phase 53 (ADR-0110): the accept-then-close backend port.
        if let Some(cp) = close_backend_port_str.as_deref() {
            v.push(("CLOSE_BACKEND_PORT", cp.to_string()));
        }
        // Phase 64 (ADR-0121): the H2-aware handshake-then-reset backend port.
        if let Some(h2cp) = h2_close_backend_port_str.as_deref() {
            v.push(("H2_CLOSE_BACKEND_PORT", h2cp.to_string()));
        }
        // 68 (ADR-0137 PV-2): the hermetic refused port (no listener).
        if let Some(dp) = dead_backend_port_str.as_deref() {
            v.push(("DEAD_BACKEND_PORT", dp.to_string()));
        }
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
            || http1_backend_1_port_str.is_some()
            || http1_backend_2_port_str.is_some()
            || http2_backend_port_str.is_some()
            || close_backend_port_str.is_some()
            || h2_close_backend_port_str.is_some()
            || dead_backend_port_str.is_some()
        {
            v.push(("BACKEND_HOST", "127.0.0.1".to_string()));
        }
        if let Some(admin) = admin_host_port {
            // 06.1 D6.a: reserved host admin port for the subject.
            v.push(("ADMIN_PORT", admin.to_string()));
        }
        if let Some(map) = subject_tls_paths.as_ref() {
            for (k, val) in map {
                v.push((*k, val.clone()));
            }
        }
        if needs_cds {
            // 18 Task 6 (ADR-0049): host temp path of the rendered subject CDS
            // file; the subject subprocess reads it directly from the host FS.
            v.push(("CDS_PATH", subject_cds_path_str.clone()));
        }
        if needs_lds {
            // 19 Task 6 (ADR-0050): host temp path of the rendered subject LDS
            // file; the subject subprocess reads it directly from the host FS.
            v.push(("LDS_PATH", subject_lds_path_str.clone()));
        }
        if needs_rds {
            // 20 Task 6 (ADR-0052): host temp path of the rendered SHARED subject
            // RDS file; the subject subprocess reads it directly from the host FS.
            v.push(("RDS_PATH", subject_rds_path_str.clone()));
        }
        if needs_eds {
            // 21 Task 6 (ADR-0054): host temp path of the rendered SHARED subject
            // EDS file; the subject subprocess reads it directly from the host FS.
            v.push(("EDS_PATH", subject_eds_path_str.clone()));
            // 21 Task 6 (ADR-0054 L9): the subject reaches the host backend over
            // loopback, so its EDS endpoint address is the numeric `127.0.0.1`
            // (NOT the host-gateway IP the container uses).
            v.push(("EDS_BACKEND_IP", "127.0.0.1".to_string()));
        }
        if needs_backend_ip {
            // 28 Task 7 (ADR-0070): the SAME shared host LAN IP as the upstream
            // side (NOT `127.0.0.1`) so BOTH proxies build their RING_HASH ring
            // from the IDENTICAL endpoint address strings — the precondition for
            // the cross-proxy STRONG selection target. The subject (a host
            // process) reaches the 0.0.0.0-bound backends via this IP directly.
            if let Some(ip) = shared_backend_ip.as_deref() {
                v.push(("BACKEND_IP", ip.to_string()));
            }
        }
        v
    };

    // (d) Adapt render_yaml call sites: build _refs intermediates since
    // render_yaml takes &[(&str, &str)] but kvs are Vec<(&str, String)>.
    let upstream_kvs_refs: Vec<(&str, &str)> =
        upstream_kvs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let subject_kvs_refs: Vec<(&str, &str)> =
        subject_kvs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    // 18 Task 6 (ADR-0049): render + write the per-side CDS files BEFORE the
    // main configs. Each rendition uses the side's own kv map (so backend
    // host/port markers inside cds.yaml resolve per-side). The upstream file is
    // later copied into the container at `CDS_CONTAINER_PATH`; the subject file
    // stays at its host temp path (`subject_cds_path`, already injected into
    // the subject kv map as `{{CDS_PATH}}`).
    // 18 Task 11 (ADR-0049): retain the rendered upstream CDS string so the
    // `host_uses_host_gateway` decision below can scan it too (fixture 0026: the
    // `host.docker.internal` reference lives ONLY in the CDS file).
    let mut upstream_cds_yaml: Option<String> = None;
    let upstream_cds_path: Option<PathBuf> = if let Some(tpl) = cds_template.as_ref() {
        let up_cds = render_yaml(tpl, &upstream_kvs_refs);
        let subject_cds_yaml = render_yaml(tpl, &subject_kvs_refs);
        // 18 Task 6 (ADR-0049): `render_yaml` intentionally leaves any unmatched
        // `{{MARKER}}` token in place, so a marker present in `cds.yaml` but
        // absent from the kv map would otherwise slip through and surface as a
        // confusing Envoy parse error. Fail fast here, CDS-scoped, naming the
        // offending marker. (Deliberately NOT applied to main-config rendering.)
        if let Some(marker) = residual_marker(&up_cds) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered upstream cds.yaml");
        }
        if let Some(marker) = residual_marker(&subject_cds_yaml) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered subject cds.yaml");
        }
        let up_path = write_temp(tmp.path(), "cds-upstream.yaml", &up_cds)?;
        // Write the subject rendition at the exact path injected into the
        // subject kv map so `{{CDS_PATH}}` resolves to a file that exists.
        write_temp(tmp.path(), "cds-subject.yaml", &subject_cds_yaml)?;
        upstream_cds_yaml = Some(up_cds);
        Some(up_path)
    } else {
        None
    };

    // 19 Task 6 (ADR-0050): render + write the PER-SIDE LDS files. Unlike CDS
    // (one shared template rendered twice), the two sides have DIFFERENT source
    // templates (`lds-envoy.yaml` / `lds-envoy-rust.yaml`) — each rendered
    // through its own side's kv map. The upstream rendition is copied into the
    // container at `LDS_CONTAINER_PATH`; the subject rendition stays at its host
    // temp path (`subject_lds_path`, already injected into the subject kv map as
    // `{{LDS_PATH}}`). The rendered upstream LDS string is retained so the
    // `host_uses_host_gateway` scan below can cover it too (fixture 0027: the
    // `host.docker.internal` reference may live ONLY in the LDS-carried route).
    let mut upstream_lds_yaml: Option<String> = None;
    let upstream_lds_path: Option<PathBuf> = if let (Some(up_tpl), Some(subj_tpl)) = (
        upstream_lds_template.as_ref(),
        subject_lds_template.as_ref(),
    ) {
        let up_lds = render_yaml(up_tpl, &upstream_kvs_refs);
        let subject_lds = render_yaml(subj_tpl, &subject_kvs_refs);
        // Fail fast, LDS-scoped, naming any unsubstituted marker (a token
        // present in an LDS template but absent from the kv map). Mirrors the
        // CDS residual-marker guard above.
        if let Some(marker) = residual_marker(&up_lds) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered upstream lds-envoy.yaml");
        }
        if let Some(marker) = residual_marker(&subject_lds) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered subject lds-envoy-rust.yaml");
        }
        let up_path = write_temp(tmp.path(), "lds-upstream.yaml", &up_lds)?;
        // Write the subject rendition at the exact path injected into the
        // subject kv map so `{{LDS_PATH}}` resolves to a file that exists.
        write_temp(tmp.path(), "lds-subject.yaml", &subject_lds)?;
        upstream_lds_yaml = Some(up_lds);
        Some(up_path)
    } else {
        None
    };

    // 20 Task 6 (ADR-0052): render + write the SHARED RDS file per-side. LIKE
    // CDS (one shared `rds.yaml` template rendered twice through each side's own
    // kv map) and UNLIKE LDS (per-side templates). The upstream rendition is
    // copied into the container at `RDS_CONTAINER_PATH`; the subject rendition
    // stays at its host temp path (`subject_rds_path`, already injected into the
    // subject kv map as `{{RDS_PATH}}`). The rendered upstream RDS string is
    // retained so the `host_uses_host_gateway` scan below can cover it too.
    let mut upstream_rds_yaml: Option<String> = None;
    let upstream_rds_path: Option<PathBuf> = if let Some(tpl) = rds_template.as_ref() {
        let up_rds = render_yaml(tpl, &upstream_kvs_refs);
        let subject_rds = render_yaml(tpl, &subject_kvs_refs);
        // Fail fast, RDS-scoped, naming any unsubstituted marker (a token
        // present in `rds.yaml` but absent from the kv map). Mirrors the CDS
        // residual-marker guard above.
        if let Some(marker) = residual_marker(&up_rds) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered upstream rds.yaml");
        }
        if let Some(marker) = residual_marker(&subject_rds) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered subject rds.yaml");
        }
        let up_path = write_temp(tmp.path(), "rds-upstream.yaml", &up_rds)?;
        // Write the subject rendition at the exact path injected into the
        // subject kv map so `{{RDS_PATH}}` resolves to a file that exists.
        write_temp(tmp.path(), "rds-subject.yaml", &subject_rds)?;
        upstream_rds_yaml = Some(up_rds);
        Some(up_path)
    } else {
        None
    };

    // 21 Task 6 (ADR-0054): render + write the SHARED EDS file per-side. LIKE
    // CDS/RDS (one shared `eds.yaml` template rendered twice through each side's
    // own kv map) and UNLIKE LDS (per-side templates). The upstream rendition is
    // copied into the container at `EDS_CONTAINER_PATH`; the subject rendition
    // stays at its host temp path (`subject_eds_path`, already injected into the
    // subject kv map as `{{EDS_PATH}}`). The per-side `{{EDS_BACKEND_IP}}` marker
    // resolves to the numeric host-gateway IP (upstream) vs `127.0.0.1`
    // (subject) — EDS rejects hostnames (L1), so unlike CDS/RDS the two
    // renditions carry DIFFERENT numeric endpoint addresses, not the shared
    // `host.docker.internal` string. The rendered upstream EDS string is
    // retained so the `host_uses_host_gateway` scan below can cover it too.
    let mut upstream_eds_yaml: Option<String> = None;
    let upstream_eds_path: Option<PathBuf> = if let Some(tpl) = eds_template.as_ref() {
        let up_eds = render_yaml(tpl, &upstream_kvs_refs);
        let subject_eds = render_yaml(tpl, &subject_kvs_refs);
        // Fail fast, EDS-scoped, naming any unsubstituted marker (a token
        // present in `eds.yaml` but absent from the kv map). Mirrors the CDS
        // residual-marker guard above.
        if let Some(marker) = residual_marker(&up_eds) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered upstream eds.yaml");
        }
        if let Some(marker) = residual_marker(&subject_eds) {
            bail!("unsubstituted marker {{{{{marker}}}}} in rendered subject eds.yaml");
        }
        let up_path = write_temp(tmp.path(), "eds-upstream.yaml", &up_eds)?;
        // Write the subject rendition at the exact path injected into the
        // subject kv map so `{{EDS_PATH}}` resolves to a file that exists.
        write_temp(tmp.path(), "eds-subject.yaml", &subject_eds)?;
        upstream_eds_yaml = Some(up_eds);
        Some(up_path)
    } else {
        None
    };

    let upstream_yaml = render_yaml(&upstream_template, &upstream_kvs_refs);
    let subject_yaml = render_yaml(&subject_template, &subject_kvs_refs);
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    // The `host_uses_host_gateway` flag drives upstream::start to attach
    // `with_host("host.docker.internal", Host::HostGateway)` on the
    // testcontainers image (per ADR-0015). The flag is true exactly when the
    // upstream YAML actually references the hostname — silent when it
    // doesn't, so fixtures 0001 and 0002 stay unchanged.
    //
    // 18 Task 11 (ADR-0015, ADR-0049): the scan also covers the rendered
    // upstream CDS file. Fixture 0026's main config has zero static clusters —
    // the backend endpoint (and thus the only `host.docker.internal` reference)
    // lives in cds.yaml. On Linux CI the host-gateway is wired via `--add-host`,
    // so without this the container's STRICT_DNS resolution fails and the route
    // 503s; macOS Docker Desktop resolves the hostname natively, which is why
    // the gap surfaced only in CI.
    // 19 Task 6 (ADR-0050): the scan now covers ALL rendered upstream sources —
    // the main config, the rendered upstream CDS file, AND the rendered upstream
    // LDS file (fixture 0027). Empty strings stand in for absent sources.
    // 20 Task 6 (ADR-0052): the scan also covers the rendered upstream RDS file
    // (a `host.docker.internal` reference could in principle live only in an
    // RDS-carried route). Empty string when the fixture carries no RDS file.
    // 21 Task 6 (ADR-0054): the scan also covers the rendered upstream EDS file.
    // For fixture 0029 the EDS endpoint address is the NUMERIC host-gateway IP
    // (NOT the `host.docker.internal` string — EDS rejects hostnames, L1), so
    // the EDS rendition alone does NOT trigger this flag; the host-gateway
    // `--add-host` mapping is still applied because the MAIN config references
    // `host.docker.internal` (via `{{BACKEND_HOST}}` for the echo backend the
    // fixture also reserves an `HTTP1_BACKEND_PORT` for). The EDS source is
    // scanned anyway for symmetry/safety per the phase-18/19/20 carryforward-
    // disposition-2 bug-class lesson. Empty string when the fixture carries no
    // EDS file.
    let host_uses_host_gateway = uses_host_gateway(&[
        &upstream_yaml,
        upstream_cds_yaml.as_deref().unwrap_or(""),
        upstream_lds_yaml.as_deref().unwrap_or(""),
        upstream_rds_yaml.as_deref().unwrap_or(""),
        upstream_eds_yaml.as_deref().unwrap_or(""),
    ]);
    // (e) Thread tls_pki through to upstream::start. 06.1: also thread
    // `needs_admin_port` so the container exposes ADMIN_CONTAINER_PORT for
    // Driver::AdminScrape fixtures.
    //
    // 06.2 D4.2.c (revised CI fix #2): bind-mount the PARENT DIRECTORY of
    // each access-log path rather than the file itself. Linux Docker
    // bind-mount semantics for individual files don't reliably propagate
    // write permission to the in-container UID even at 0o666; bind-mounting
    // a 0o777 directory and letting Envoy create the file fresh sidesteps
    // this entirely. The host-side file is created by Envoy under its own
    // UID, but the host's 0o777 dir lets the harness read it back regardless
    // of ownership. The envoy-rust side runs as a subprocess and writes
    // directly to the host path; only the upstream Envoy needs the mount.
    let upstream_access_log_mounts: Vec<(String, String)> = match &expectations.driver {
        // Phase 32 Task 6 (ADR-0079): the byte-exact access-log driver mounts
        // both log dirs identically to `Http1WithAccessLog`. Phase 56 Task 1
        // extends this to HTTP/2 via `Http2AccessLogByteExact`.
        Driver::Http1WithAccessLog {
            expected_access_log_paths,
            ..
        }
        | Driver::Http1AccessLogByteExact {
            expected_access_log_paths,
            ..
        }
        | Driver::Http2AccessLogByteExact {
            expected_access_log_paths,
            ..
        } => {
            let to_parent = |p: &str| -> Result<(std::path::PathBuf, String)> {
                let path = std::path::PathBuf::from(p);
                let parent = path
                    .parent()
                    .ok_or_else(|| anyhow::anyhow!("access-log path {} has no parent", p))?;
                Ok((parent.to_path_buf(), parent.to_string_lossy().into_owned()))
            };
            let (envoy_parent, envoy_parent_s) = to_parent(&expected_access_log_paths.envoy)?;
            let (envoy_rust_parent, _) = to_parent(&expected_access_log_paths.envoy_rust)?;

            // Create both host-side parent dirs; chmod 0o777 so the in-
            // container envoy UID can create the log file inside on Linux
            // Docker without UID-translation. Remove any leftover file from
            // a previous run so the harness's per-token diff doesn't see
            // stale lines.
            for parent in [&envoy_parent, &envoy_rust_parent] {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("create access-log parent dir {}", parent.display())
                })?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o777))
                        .with_context(|| {
                            format!("chmod access-log parent dir {}", parent.display())
                        })?;
                }
            }
            for p in [
                &expected_access_log_paths.envoy,
                &expected_access_log_paths.envoy_rust,
            ] {
                // Best-effort remove of any leftover file; ignore NotFound.
                if let Err(e) = std::fs::remove_file(p)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    return Err(e).with_context(|| format!("remove leftover access-log file {p}"));
                }
            }

            // Bind-mount the envoy-side parent directory into the container
            // at the same path. Envoy-rust runs as a host subprocess so no
            // mount is needed for its side.
            vec![(envoy_parent_s.clone(), envoy_parent_s)]
        }
        _ => Vec::new(),
    };
    let upstream = upstream::start(
        &upstream_path,
        host_uses_host_gateway,
        tls_pki.as_ref(),
        needs_admin_port,
        upstream_cds_path.as_deref(),
        upstream_lds_path.as_deref(),
        upstream_rds_path.as_deref(),
        upstream_eds_path.as_deref(),
        &upstream_access_log_mounts,
    )
    .await?;
    let mut subject = subject::start(&subject_path, host_port).await?;

    let upstream_addr: SocketAddr = format!("127.0.0.1:{}", upstream.host_port()).parse()?;
    let subject_addr: SocketAddr = format!("127.0.0.1:{}", subject.port()).parse()?;

    let budget = Duration::from_secs(10);
    wait_accept_ready(upstream_addr, budget)
        .await
        .context("upstream Envoy never became accept-ready")?;
    // Like wait_accept_ready, but bail immediately if the subject process has
    // already exited (e.g. a listener bind failure) — the connect loop would
    // otherwise burn the whole budget and mask the real error (CI run
    // 26861955222: data + admin listener port collision → instant exit →
    // misleading 10s "never became accept-ready" timeout).
    let subject_deadline = std::time::Instant::now() + budget;
    loop {
        if let Some(status) = subject.try_exit_status() {
            bail!("envoy-rust exited before accept-ready: {status}");
        }
        match tokio::net::TcpStream::connect(subject_addr).await {
            Ok(_) => break,
            Err(_) if std::time::Instant::now() < subject_deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => {
                bail!("envoy-rust {subject_addr} not accept-ready within {budget:?}: {err}")
            }
        }
    }

    // Gate only fixtures whose clusters arrive via file-based xDS; static-cluster
    // fixtures warm synchronously and some intentionally drive 503s. Upstream
    // (real Envoy) side only — envoy-rust does not emit membership_healthy
    // without active health checks.
    if (upstream_cds_path.is_some() || upstream_eds_path.is_some())
        && needs_admin_port
        && let Some(p) = upstream.host_admin_port()
    {
        wait_clusters_warm(format!("127.0.0.1:{p}").parse()?, Duration::from_secs(10)).await;
    }

    // Shared borrows + `Copy` scalars threaded into the per-driver arm fns;
    // `upstream`/`subject` are moved into the selected arm by value.
    let ctx = FixtureCtx {
        fixture_dir,
        expectations: &expectations,
        upstream_addr,
        subject_addr,
        admin_host_port,
        budget,
        tls_pki: &tls_pki,
        upstream_kvs_refs: &upstream_kvs_refs,
        subject_kvs_refs: &subject_kvs_refs,
        upstream_rds_path: &upstream_rds_path,
        subject_rds_path: &subject_rds_path,
        upstream_eds_path: &upstream_eds_path,
        subject_eds_path: &subject_eds_path,
    };

    match &expectations.driver {
        Driver::TcpEcho => {
            run_tcp_echo_arm(&ctx, upstream, subject).await?;
        }
        Driver::TcpDirectResponse => {
            run_tcp_direct_response_arm(&ctx, upstream, subject).await?;
        }
        Driver::TcpWithStats {
            probe,
            settle_ms,
            expected_stats,
        } => {
            run_tcp_with_stats_arm(&ctx, upstream, subject, probe, settle_ms, expected_stats)
                .await?;
        }
        Driver::HttpGet { path, host } => {
            run_http_get_arm(&ctx, upstream, subject, path, host).await?;
        }
        // (f) Real TLS dispatch arm.
        Driver::TlsTcp { sni, expected_cn } => {
            run_tls_tcp_arm(&ctx, upstream, subject, sni, expected_cn).await?;
        }
        // 03.2 Task 8: per-SNI probe list. Equivalence is enforced inside
        // `drive_tls_probes` per probe (byte-equality + per-probe expected_cn);
        // both sides succeeding ⇒ equivalent cert selection per SNI without a
        // final `assert_equivalence` call.
        Driver::TlsTcpProbeList { probes } => {
            run_tls_tcp_probe_list_arm(&ctx, upstream, subject, probes).await?;
        }
        // 04.1 Task 14: real dispatch. Drive both proxies, then apply
        // equivalence rules (envoy ↔ envoy-rust) plus per-driver `expected_*`
        // anchors (each side independently). The header allow-list path is
        // a per-driver `Http1HeaderRule::SetEqualModuloAllowList` (the
        // `Equivalence` struct has no `response_headers` field today —
        // documenting the asymmetry against status/body for the eventual
        // 04.x cleanup).
        Driver::Http1 {
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
        } => {
            run_http1_arm(
                &ctx,
                upstream,
                subject,
                method,
                path,
                host,
                expected_status,
                expected_body,
                expected_headers,
            )
            .await?;
        }
        // 12.2 NEW: Driver::Http1AfterSettle — sleep `settle_ms` past
        // active-HC convergence, then drive ONE Http1 request via the same
        // helper + equivalence cascade the single-probe `Driver::Http1` arm
        // uses. The settle_ms is a harness mechanic (not a compared latency
        // bound — phase 12 does NOT opt into Timing tolerances per
        // BEHAVIOR_CONTRACT.md §Timing). The fixture asserts the
        // post-convergence STEADY STATE, not a transient. See PLAN
        // lock-in #20.
        Driver::Http1AfterSettle {
            settle_ms,
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
        } => {
            run_http1_after_settle_arm(
                &ctx,
                upstream,
                subject,
                settle_ms,
                method,
                path,
                host,
                expected_status,
                expected_body,
                expected_headers,
            )
            .await?;
        }
        // Phase 69 (grpc_health_check, ADR-0138): Driver::Http2AfterSettle —
        // the H2 sibling of `Driver::Http1AfterSettle` immediately above.
        // Sleep `settle_ms` past active-HC convergence, then drive ONE H2C
        // prior-knowledge request via `drive_http2` and apply the same
        // equivalence cascade. Fixture 0075 asserts the post-convergence
        // 503 "no healthy upstream" steady state.
        Driver::Http2AfterSettle {
            settle_ms,
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
        } => {
            run_http2_after_settle_arm(
                &ctx,
                upstream,
                subject,
                settle_ms,
                method,
                path,
                host,
                expected_status,
                expected_body,
                expected_headers,
            )
            .await?;
        }
        // 13.1 D10 / Task 7: drive N sequential HTTP/1.1 requests over a
        // SINGLE downstream keep-alive conn against BOTH proxies in turn,
        // then sleep `settle_ms` past the last request + scrape named
        // admin stats and assert each side's value matches bilaterally.
        //
        // The discriminating-observable shape per parent-13 §2 item-iv:
        // with separate per-request conns, `upstream_cx_total: N`. Under
        // the H1 pool landed at 13.1 Tasks 3-4, `upstream_cx_total: 1` —
        // all N requests reuse the single first-acquire conn. Fixture
        // 0020 also asserts the 8 per-class downstream/upstream counters
        // (downstream_rq_{2,3,4,5}xx + upstream_rq_{2,3,4,5}xx +
        // downstream_rq_total + upstream_rq_total) to close the 06.3
        // REVIEW I2 (a) wire-level per-class counter property at the
        // bilateral seam.
        //
        // Per-side dispatch shape mirrors `Driver::Http1AfterSettle`
        // (single-shot) for the request shape, and `Driver::AdminScrape`
        // for the admin-port plumbing (`upstream.host_admin_port()` +
        // `admin_host_port`). The settle sleep fires ONCE after both
        // proxies have driven all requests; both sides converge under
        // the same Relaxed-ordering visibility budget.
        Driver::Http1KeepAlive {
            requests,
            settle_ms,
            expected_stats,
            admin_scrapes,
        } => {
            run_http1_keep_alive_arm(
                &ctx,
                upstream,
                subject,
                requests,
                settle_ms,
                expected_stats,
                admin_scrapes,
            )
            .await?;
        }
        // 13.2 Task 5 (ADR-0039): the H2 sibling of `Http1KeepAlive`. Each
        // proxy gets ONE downstream H2 conn (TCP + h2::client::handshake),
        // N sequential single-stream requests via cloned `SendRequest`,
        // then a settle sleep + bilateral admin stat scrape.
        //
        // The discriminating observable under fixture 0021's H2 upstream
        // cluster: `cluster.<name>.upstream_cx_total: 1` +
        // `cluster.<name>.upstream_cx_http2_total: 1` because the H2 pool
        // (Task 2's integration) reuses the single first-acquire upstream
        // H2 conn across all N downstream-stream → upstream-stream
        // dispatches. With a per-call upstream-`Client::connect` regression
        // this counter would be N and the fixture would fail RED.
        //
        // Architectural shape per ADR-0039 + parent-13 SPEC §6.2 item-iv:
        // the discriminating-observable bilateral validation is preserved
        // under the H2-downstream + H2-upstream topology (instead of the
        // PLAN's H1-downstream + H2-upstream topology, which is rejected
        // at parse time by the 06.3 D14.3 gate per ADR-0028).
        Driver::Http2KeepAlive {
            requests,
            settle_ms,
            expected_stats,
        } => {
            run_http2_keep_alive_arm(&ctx, upstream, subject, requests, settle_ms, expected_stats)
                .await?;
        }
        Driver::Http1ProbeList { probes } => {
            run_http1_probe_list_arm(&ctx, upstream, subject, probes).await?;
        }
        // 28 Task 7 (ADR-0070): RING_HASH consistent-hashing cross-proxy
        // differential. For each `x-hash-key` value: send GET <path> with that
        // header to BOTH proxies (twice, for stability), extract each response
        // body's leading `backend: <marker>\n` line, and assert per-key
        // cross-proxy marker agreement (STRONG), full-sweep spread (BOTH markers
        // appear on EACH side), and same-key stability. Locally observable — a
        // plain request/response with no reload trigger.
        Driver::Http1HashSweep {
            keys,
            path,
            host,
            expected_status,
        } => {
            run_http1_hash_sweep_arm(&ctx, upstream, subject, keys, path, host, expected_status)
                .await?;
        }
        // 30 Task 7 (ADR-0074): subset LB route-selection cross-proxy
        // differential. For each probe: GET <path> against BOTH proxies, assert
        // both return `expected_status`. 200 probes — extract each body's leading
        // `backend: <marker>` line; assert cross-proxy marker agreement (STRONG)
        // AND that it equals the §A oracle marker. The 503 probe — assert each
        // side's body is the fixed `no healthy upstream` NO_FALLBACK local reply.
        // Locally observable — a plain request/response with no reload trigger.
        Driver::Http1RouteSelect { probes } => {
            run_http1_route_select_arm(&ctx, upstream, subject, probes).await?;
        }
        // 26 Task 7: file-based RDS hot-reload differential step. Runs
        // `pre_probes` bilaterally, atomic-renames the post-reload RDS content
        // over the watched path on BOTH sides (subject host file + upstream
        // container file), waits — bounded, polling the discriminator, NOT a
        // fixed sleep — for both proxies to converge on the new table, then runs
        // `post_probes` bilaterally. The post-reload differential is
        // NATIVE-Linux-CI-authoritative: the upstream container reload is
        // unobservable under Docker Desktop virtiofs (bind-mount inotify does
        // not propagate), so the in-container atomic-rename + convergence is
        // only meaningful on native Linux CI.
        Driver::Http1RdsReload {
            pre_probes,
            reload,
            post_probes,
        } => {
            run_http1_rds_reload_arm(&ctx, upstream, subject, pre_probes, reload, post_probes)
                .await?;
        }
        // 27 Task 6 (D6 / §6.2-LOCKED V2 / ADR-0068): file-based EDS hot-reload
        // differential step — the EDS sibling of `Http1RdsReload`. Runs
        // `pre_probes` bilaterally (backend_1 serving), atomic-renames the
        // post-reload EDS content (endpoint swapped `[backend_1]` → `[backend_2]`)
        // over the watched path on BOTH sides (subject host file + upstream
        // container file), waits — bounded, polling the discriminator, NOT a
        // fixed sleep — for both proxies to converge on the new endpoint set,
        // then runs `post_probes` bilaterally (backend_2 serving). The
        // post-reload differential is NATIVE-Linux-CI-authoritative: the upstream
        // container reload is unobservable under Docker Desktop virtiofs
        // (bind-mount inotify does not propagate), so the in-container
        // atomic-rename + convergence is only meaningful on native Linux CI.
        Driver::Http1EdsReload {
            pre_probes,
            reload,
            post_probes,
        } => {
            run_http1_eds_reload_arm(&ctx, upstream, subject, pre_probes, reload, post_probes)
                .await?;
        }
        // 11 NEW: HTTP/2 probe-list driver. Mirrors Driver::Http1ProbeList
        // verbatim, swapping drive_http1 → drive_http2 (H2 cleartext
        // prior-knowledge per drive_http2's handshake). The Http1Probe struct
        // is codec-agnostic, so the per-probe equivalence cascade is identical.
        // This is the first HTTP-filter-family fixture on an H2 listener,
        // exercising the phase-11 D6 decorate_filter_synth_response_h2 helper
        // bilaterally (closes 09 REVIEW M2). Per phase-11 SPEC §3 D8.1.
        Driver::Http2ProbeList { probes } => {
            run_http2_probe_list_arm(&ctx, upstream, subject, probes).await?;
        }
        // 06.2 NEW: HTTP/1.1 driver with post-request access-log line
        // assertion. Wire-protocol leg reuses `drive_http1` (04.1); after the
        // response equivalence cascade, file-reads both proxies' configured
        // access-log files and dispatches per-token equivalence through
        // `access_log::assert_access_log_lines_equivalent`. Per SPEC §3 D4.2.
        Driver::Http1WithAccessLog {
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
            extra_headers,
            expected_access_log_paths,
            expected_access_log_lines,
        } => {
            run_http1_with_access_log_arm(
                &ctx,
                upstream,
                subject,
                method,
                path,
                host,
                expected_status,
                expected_body,
                expected_headers,
                extra_headers,
                expected_access_log_paths,
                expected_access_log_lines,
            )
            .await?;
        }
        // Phase 32 Task 6 (ADR-0079): whole-line byte-exact access-log
        // differential. Drives a SEQUENCE of H1 probes (reusing `drive_http1`
        // exactly as the `Http1WithAccessLog` arm does), then scrapes BOTH
        // proxies' access-log files and asserts every emitted line is
        // byte-identical (NOT the per-token default-format comparison). The
        // fixture's custom `log_format` is deterministic-operators-only, so a
        // whole-line `==` is the strongest possible assertion.
        Driver::Http1AccessLogByteExact {
            probes,
            expected_access_log_paths,
        } => {
            run_http1_access_log_byte_exact_arm(
                &ctx,
                upstream,
                subject,
                probes,
                expected_access_log_paths,
            )
            .await?;
        }
        // Phase 56 (ADR-0113): the H2 sibling of the byte-exact access-log
        // driver above. Drives a SEQUENCE of H2 probes (via `drive_http2`),
        // then scrapes BOTH proxies' access-log files and asserts every
        // emitted line is byte-identical.
        Driver::Http2AccessLogByteExact {
            probes,
            expected_access_log_paths,
        } => {
            run_http2_access_log_byte_exact_arm(
                &ctx,
                upstream,
                subject,
                probes,
                expected_access_log_paths,
            )
            .await?;
        }
        // 05.2 Task 11: real H2 dispatch. Mirrors Driver::Http1 in shape; the
        // only delta is `drive_http2` instead of `drive_http1`. Per SPEC §3 D5.
        Driver::Http2 {
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
        } => {
            run_http2_arm(
                &ctx,
                upstream,
                subject,
                method,
                path,
                host,
                expected_status,
                expected_body,
                expected_headers,
            )
            .await?;
        }
        // 06.1 D6.a / 08.1 Task 11: HCM pre-requests then 1..N admin
        // scrapes. Drives both proxies, applies per-sub-case
        // expected_status / expected_content_type / expected_body_rule
        // assertions on each side. Body-rule equivalence is enforced by
        // `assert_body_rule` per sub-case.
        Driver::AdminScrape {
            pre_admin_actions,
            pre_requests,
            scrapes,
            post_admin_assertions,
        } => {
            run_admin_scrape_arm(
                &ctx,
                upstream,
                subject,
                pre_admin_actions,
                pre_requests,
                scrapes,
                post_admin_assertions,
            )
            .await?;
        }
    }

    // _backend Drop fires here.
    Ok(())
}

/// `Driver::TcpEcho` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_tcp_echo_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
) -> Result<()> {
    let FixtureCtx {
        fixture_dir,
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    let payload =
        std::fs::read(fixture_dir.join("inputs/payload.bin")).context("reading payload.bin")?;
    let upstream_out = drive_tcp(upstream_addr, &payload)
        .await
        .context("upstream envoy drive")?;
    let subject_out = drive_tcp(subject_addr, &payload)
        .await
        .context("envoy-rust drive")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    assert_equivalence(expectations, None, None, &upstream_out, &subject_out)?;
    Ok(())
}

/// 13.1 D10, hoisted at 67.1 D7: scrape each named stat from BOTH admin
/// listeners and assert each side's value equals `stat.value` independently.
/// Cross-side consistency follows by transitivity.
///
/// `scrape_admin_stat` returns `Ok(0)` for a stat name the proxy never
/// registered. A `value: 0` assertion therefore passes vacuously when the name
/// is ABSENT; only a non-zero assertion is a real witness. Fixture READMEs must
/// say which of their assertions is the witness.
async fn assert_expected_stats_bilaterally(
    upstream_admin_addr: SocketAddr,
    subject_admin_addr: SocketAddr,
    expected_stats: &[KeepAliveExpectedStat],
) -> Result<()> {
    for stat in expected_stats {
        let upstream_value = scrape_admin_stat(upstream_admin_addr, &stat.name)
            .await
            .with_context(|| format!("upstream scraping stat {}", stat.name))?;
        let subject_value = scrape_admin_stat(subject_admin_addr, &stat.name)
            .await
            .with_context(|| format!("subject scraping stat {}", stat.name))?;
        anyhow::ensure!(
            upstream_value == stat.value,
            "upstream stat {} expected {} got {}",
            stat.name,
            stat.value,
            upstream_value,
        );
        anyhow::ensure!(
            subject_value == stat.value,
            "subject stat {} expected {} got {}",
            stat.name,
            stat.value,
            subject_value,
        );
    }
    Ok(())
}

/// `Driver::TcpWithStats` arm of `run_fixture` (67.1 D7). Drives ONE raw-TCP
/// probe against each proxy via a pre-existing driver, then settles and scrapes
/// both admin listeners, and finally asserts body equivalence.
///
/// The scrape MUST happen BEFORE `subject.shutdown()`, which kills the
/// envoy-rust process and its admin listener with it. `run_tcp_direct_response_arm`
/// shuts down before `assert_equivalence` because it needs no admin access; this
/// arm must not copy that order.
async fn run_tcp_with_stats_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probe: &TcpProbeKind,
    settle_ms: &u64,
    expected_stats: &[KeepAliveExpectedStat],
) -> Result<()> {
    let FixtureCtx {
        fixture_dir,
        expectations,
        upstream_addr,
        subject_addr,
        admin_host_port,
        budget,
        ..
    } = *ctx;

    let upstream_admin_port = upstream.host_admin_port().ok_or_else(|| {
        anyhow::anyhow!(
            "Driver::TcpWithStats requires the upstream container to expose its admin port; \
             either the fixture's envoy.yaml does not reference {{ADMIN_PORT}} or the harness \
             failed to wire `expose_admin_port = true`",
        )
    })?;
    let subject_admin_port = admin_host_port.ok_or_else(|| {
        anyhow::anyhow!(
            "Driver::TcpWithStats requires the subject's envoy-rust.yaml to reference \
             {{ADMIN_PORT}}; the harness only reserves a host admin port when one of the \
             templates contains the marker",
        )
    })?;
    let upstream_admin_addr: SocketAddr = format!("127.0.0.1:{upstream_admin_port}").parse()?;
    let subject_admin_addr: SocketAddr = format!("127.0.0.1:{subject_admin_port}").parse()?;
    wait_accept_ready(upstream_admin_addr, budget)
        .await
        .context("upstream admin listener never became accept-ready")?;
    wait_accept_ready(subject_admin_addr, budget)
        .await
        .context("envoy-rust admin listener never became accept-ready")?;

    // ADR-0131: settle, THEN baseline. `run_fixture` already opened a readiness
    // `TcpStream::connect` to each proxy's data port. The settle lets any such
    // pre-probe connection land before the baseline is taken, so the delta below
    // isolates the probe's own effect on each counter.
    tokio::time::sleep(Duration::from_millis(*settle_ms)).await;
    let upstream_baseline =
        scrape_stat_snapshot(upstream_admin_addr, expected_stats, "upstream").await?;
    let subject_baseline =
        scrape_stat_snapshot(subject_admin_addr, expected_stats, "subject").await?;

    let (upstream_out, subject_out) = match probe {
        TcpProbeKind::Echo => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            (
                drive_tcp(upstream_addr, &payload)
                    .await
                    .context("upstream envoy drive")?,
                drive_tcp(subject_addr, &payload)
                    .await
                    .context("envoy-rust drive")?,
            )
        }
        TcpProbeKind::ReadToEof => (
            drive_tcp_direct_response(upstream_addr)
                .await
                .context("upstream envoy drive")?,
            drive_tcp_direct_response(subject_addr)
                .await
                .context("envoy-rust drive")?,
        ),
        TcpProbeKind::WriteThenReadToEof => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            (
                drive_tcp_write_then_read_to_eof(upstream_addr, &payload)
                    .await
                    .context("upstream envoy drive")?,
                drive_tcp_write_then_read_to_eof(subject_addr, &payload)
                    .await
                    .context("envoy-rust drive")?,
            )
        }
    };

    // Single post-probe settle, covering stat-write visibility on BOTH sides
    // under the same Relaxed-ordering budget.
    tokio::time::sleep(Duration::from_millis(*settle_ms)).await;
    assert_expected_stat_deltas_bilaterally(
        upstream_admin_addr,
        subject_admin_addr,
        &upstream_baseline,
        &subject_baseline,
        expected_stats,
    )
    .await?;

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    assert_equivalence(expectations, None, None, &upstream_out, &subject_out)?;
    Ok(())
}

/// 67.1 D7 (ADR-0131): snapshot the current value of each named stat from ONE
/// admin listener, in `expected`'s order. `scrape_admin_stat` returns `Ok(0)`
/// for a name the proxy never registered, so an absent counter snapshots as 0.
async fn scrape_stat_snapshot(
    admin_addr: SocketAddr,
    expected: &[KeepAliveExpectedStat],
    side: &str,
) -> Result<Vec<u64>> {
    let mut out = Vec::with_capacity(expected.len());
    for stat in expected {
        out.push(
            scrape_admin_stat(admin_addr, &stat.name)
                .await
                .with_context(|| format!("{side} baseline-scraping stat {}", stat.name))?,
        );
    }
    Ok(out)
}

/// 67.1 D7 (ADR-0131): assert the DELTA each named stat moved by, on BOTH sides.
///
/// `run_fixture` opens a readiness `TcpStream::connect` to each proxy's data port
/// before the probe runs, and a fixture cannot know what else may touch the
/// listener. Asserting absolute values on a per-connection counter therefore
/// couples the fixture to harness incidentals. Baselining after a settle and
/// asserting the delta isolates the probe's own effect.
///
/// (Under ADR-0131's first-byte semantics neither proxy counts a data-less
/// readiness connect, so absolute values would happen to work today. Deltas keep
/// the fixture correct regardless — this was found the hard way: the absolute
/// form failed with `subject allowed expected 1 got 2` before the first-byte
/// divergence was diagnosed.)
///
/// The witness property is PRESERVED: `scrape_admin_stat` yields 0 for a stat
/// name the proxy never registered, so an unimplemented filter snapshots 0 and
/// finishes at 0 — a delta of 0, which fails any non-zero expectation.
async fn assert_expected_stat_deltas_bilaterally(
    upstream_admin_addr: SocketAddr,
    subject_admin_addr: SocketAddr,
    upstream_baseline: &[u64],
    subject_baseline: &[u64],
    expected_stats: &[KeepAliveExpectedStat],
) -> Result<()> {
    for (idx, stat) in expected_stats.iter().enumerate() {
        let upstream_final = scrape_admin_stat(upstream_admin_addr, &stat.name)
            .await
            .with_context(|| format!("upstream scraping stat {}", stat.name))?;
        let subject_final = scrape_admin_stat(subject_admin_addr, &stat.name)
            .await
            .with_context(|| format!("subject scraping stat {}", stat.name))?;
        let upstream_delta = upstream_final
            .checked_sub(upstream_baseline[idx])
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "upstream stat {} went backwards: baseline {} final {}",
                    stat.name,
                    upstream_baseline[idx],
                    upstream_final,
                )
            })?;
        let subject_delta = subject_final
            .checked_sub(subject_baseline[idx])
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "subject stat {} went backwards: baseline {} final {}",
                    stat.name,
                    subject_baseline[idx],
                    subject_final,
                )
            })?;
        anyhow::ensure!(
            upstream_delta == stat.value,
            "upstream stat {} delta expected {} got {} (baseline {}, final {})",
            stat.name,
            stat.value,
            upstream_delta,
            upstream_baseline[idx],
            upstream_final,
        );
        anyhow::ensure!(
            subject_delta == stat.value,
            "subject stat {} delta expected {} got {} (baseline {}, final {})",
            stat.name,
            stat.value,
            subject_delta,
            subject_baseline[idx],
            subject_final,
        );
    }
    Ok(())
}

/// `Driver::TcpDirectResponse` arm of `run_fixture`. No `inputs/` payload: the
/// probe sends nothing and reads to EOF on both proxies, then asserts the two
/// response bodies are byte-equal.
async fn run_tcp_direct_response_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    let upstream_out = drive_tcp_direct_response(upstream_addr)
        .await
        .context("upstream envoy drive")?;
    let subject_out = drive_tcp_direct_response(subject_addr)
        .await
        .context("envoy-rust drive")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    assert_equivalence(expectations, None, None, &upstream_out, &subject_out)?;
    Ok(())
}

/// `Driver::HttpGet` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http_get_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    path: &str,
    host: &str,
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    let upstream_resp = drive_http_get(upstream_addr, path, host)
        .await
        .context("upstream envoy http get")?;
    let subject_resp = drive_http_get(subject_addr, path, host)
        .await
        .context("envoy-rust http get")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    assert_equivalence(
        expectations,
        Some(upstream_resp.status),
        Some(subject_resp.status),
        &upstream_resp.body,
        &subject_resp.body,
    )?;
    Ok(())
}

/// `Driver::TlsTcp` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_tls_tcp_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    sni: &str,
    expected_cn: &Option<String>,
) -> Result<()> {
    let FixtureCtx {
        fixture_dir,
        expectations,
        upstream_addr,
        subject_addr,
        tls_pki,
        ..
    } = *ctx;
    let payload =
        std::fs::read(fixture_dir.join("inputs/payload.bin")).context("reading payload.bin")?;
    // Build a RootCertStore from the test CA. Both sides trust the
    // same CA — both proxies present a leaf signed by it.
    let pki = tls_pki
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(
                    "Driver::TlsTcp requires a TLS-shaped fixture (template did not reference any *_PATH key)"
                ))?;
    let ca_bytes = std::fs::read(&pki.ca_pem_path).context("read ca.pem")?;
    let mut ca_slice = ca_bytes.as_slice();
    let ca_certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut ca_slice)
            .collect::<Result<Vec<_>, _>>()
            .context("parse ca.pem certs")?;
    let mut roots = rustls::RootCertStore::empty();
    for c in ca_certs {
        roots.add(c).context("RootCertStore::add")?;
    }

    let upstream_out = drive_tls(
        upstream_addr,
        &payload,
        sni,
        roots.clone(),
        expected_cn.as_deref(),
    )
    .await
    .context("upstream envoy tls drive")?;
    let subject_out = drive_tls(subject_addr, &payload, sni, roots, expected_cn.as_deref())
        .await
        .context("envoy-rust tls drive")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    assert_equivalence(expectations, None, None, &upstream_out, &subject_out)?;
    Ok(())
}

/// `Driver::TlsTcpProbeList` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_tls_tcp_probe_list_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probes: &[TlsTcpProbe],
) -> Result<()> {
    let FixtureCtx {
        fixture_dir,
        upstream_addr,
        subject_addr,
        tls_pki,
        ..
    } = *ctx;
    let payload =
        std::fs::read(fixture_dir.join("inputs/payload.bin")).context("reading payload.bin")?;
    let pki = tls_pki
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!(
                    "Driver::TlsTcpProbeList requires a TLS-shaped fixture (template did not reference any *_PATH key)"
                ))?;
    let ca_bytes = std::fs::read(&pki.ca_pem_path).context("read ca.pem")?;
    let mut ca_slice = ca_bytes.as_slice();
    let ca_certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut ca_slice)
            .collect::<Result<Vec<_>, _>>()
            .context("parse ca.pem certs")?;
    let mut roots = rustls::RootCertStore::empty();
    for c in ca_certs {
        roots.add(c).context("RootCertStore::add")?;
    }

    drive_tls_probes(upstream_addr, &payload, probes, roots.clone())
        .await
        .context("upstream envoy tls probes")?;
    drive_tls_probes(subject_addr, &payload, probes, roots)
        .await
        .context("envoy-rust tls probes")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http1` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
// The parameter list mirrors the `Driver` variant's fields, threaded straight
// from the `run_fixture` dispatcher; bundling them into a struct would add
// indirection without clarifying the dispatch (same disposition as
// `upstream::start`).
#[allow(clippy::too_many_arguments)]
async fn run_http1_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    method: &Http1Method,
    path: &str,
    host: &str,
    expected_status: &Option<u16>,
    expected_body: &Option<Http1BodyRule>,
    expected_headers: &Option<Http1HeaderRule>,
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    let upstream_resp = drive_http1(upstream_addr, method, path, host, &[], None)
        .await
        .context("upstream envoy http1 drive")?;
    let subject_resp = drive_http1(subject_addr, method, path, host, &[], None)
        .await
        .context("envoy-rust http1 drive")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    // Status: envoy ↔ envoy-rust under `response_status: exact`.
    if matches!(
        expectations.equivalence.response_status,
        Some(StatusRule::Exact)
    ) && upstream_resp.status != subject_resp.status
    {
        bail!(
            "response status mismatch under `response_status: exact`\n  \
                     upstream: {}\n  subject:  {}",
            upstream_resp.status,
            subject_resp.status,
        );
    }
    // Per-driver `expected_status`: each side independently equals it.
    if let Some(es) = expected_status {
        if upstream_resp.status != *es {
            bail!(
                "upstream status {} != expected {}",
                upstream_resp.status,
                es,
            );
        }
        if subject_resp.status != *es {
            bail!("subject status {} != expected {}", subject_resp.status, es,);
        }
    }

    // Body: envoy ↔ envoy-rust per `equivalence.response_body` rule.
    // 06.1 Task 13 carryforward: route through `assert_body_rule` so
    // BodyRule variants (ByteExact / PrometheusExposition) dispatch
    // through the single centralized helper instead of inline
    // `matches!`. Behaviorally equivalent for ByteExact; admits
    // PrometheusExposition without re-touching the arm.
    if let Some(rule) = &expectations.equivalence.response_body {
        assert_body_rule(rule, &upstream_resp.body, &subject_resp.body)?;
    }
    // Per-driver `expected_body`: each side independently equals bytes.
    if let Some(Http1BodyRule::ByteExact { body }) = expected_body {
        let expected = body.as_bytes();
        if upstream_resp.body != expected {
            bail!(
                "upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                upstream_resp.body,
                expected,
            );
        }
        if subject_resp.body != expected {
            bail!(
                "subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                subject_resp.body,
                expected,
            );
        }
    }

    // Headers: per-driver allow-list diff between envoy ↔ envoy-rust.
    if matches!(
        expected_headers,
        Some(Http1HeaderRule::SetEqualModuloAllowList)
    ) {
        diff_headers(
            &upstream_resp.headers,
            &subject_resp.headers,
            HEADER_ALLOW_LIST,
        )
        .context("diff_headers (set_equal_modulo_allow_list)")?;
    }
    Ok(())
}

/// `Driver::Http1AfterSettle` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
// The parameter list mirrors the `Driver` variant's fields, threaded straight
// from the `run_fixture` dispatcher; bundling them into a struct would add
// indirection without clarifying the dispatch (same disposition as
// `upstream::start`).
#[allow(clippy::too_many_arguments)]
async fn run_http1_after_settle_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    settle_ms: &u64,
    method: &Http1Method,
    path: &str,
    host: &str,
    expected_status: &Option<u16>,
    expected_body: &Option<Http1BodyRule>,
    expected_headers: &Option<Http1HeaderRule>,
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    tracing::debug!(
        settle_ms,
        "Driver::Http1AfterSettle: sleeping for active-HC settle"
    );
    tokio::time::sleep(Duration::from_millis(*settle_ms)).await;

    let upstream_resp = drive_http1(upstream_addr, method, path, host, &[], None)
        .await
        .context("upstream envoy http1 drive (after settle)")?;
    let subject_resp = drive_http1(subject_addr, method, path, host, &[], None)
        .await
        .context("envoy-rust http1 drive (after settle)")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    // Status: envoy ↔ envoy-rust under `response_status: exact`.
    if matches!(
        expectations.equivalence.response_status,
        Some(StatusRule::Exact)
    ) && upstream_resp.status != subject_resp.status
    {
        bail!(
            "response status mismatch under `response_status: exact`\n  \
                     upstream: {}\n  subject:  {}",
            upstream_resp.status,
            subject_resp.status,
        );
    }
    if let Some(es) = expected_status {
        if upstream_resp.status != *es {
            bail!(
                "upstream status {} != expected {}",
                upstream_resp.status,
                es,
            );
        }
        if subject_resp.status != *es {
            bail!("subject status {} != expected {}", subject_resp.status, es,);
        }
    }

    if let Some(rule) = &expectations.equivalence.response_body {
        assert_body_rule(rule, &upstream_resp.body, &subject_resp.body)?;
    }
    if let Some(Http1BodyRule::ByteExact { body }) = expected_body {
        let expected = body.as_bytes();
        if upstream_resp.body != expected {
            bail!(
                "upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                upstream_resp.body,
                expected,
            );
        }
        if subject_resp.body != expected {
            bail!(
                "subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                subject_resp.body,
                expected,
            );
        }
    }

    if matches!(
        expected_headers,
        Some(Http1HeaderRule::SetEqualModuloAllowList)
    ) {
        diff_headers(
            &upstream_resp.headers,
            &subject_resp.headers,
            HEADER_ALLOW_LIST,
        )
        .context("diff_headers (set_equal_modulo_allow_list)")?;
    }
    Ok(())
}

/// `Driver::Http2AfterSettle` arm of `run_fixture` — the H2 sibling of
/// `run_http1_after_settle_arm` immediately above (Phase 69, ADR-0138):
/// verbatim clone with the two `drive_http1(..., &[], None)` calls replaced
/// by `drive_http2(..., &[])` (H2C prior-knowledge, no request body).
// The parameter list mirrors the `Driver` variant's fields, threaded straight
// from the `run_fixture` dispatcher; bundling them into a struct would add
// indirection without clarifying the dispatch (same disposition as
// `upstream::start`).
#[allow(clippy::too_many_arguments)]
async fn run_http2_after_settle_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    settle_ms: &u64,
    method: &Http1Method,
    path: &str,
    host: &str,
    expected_status: &Option<u16>,
    expected_body: &Option<Http1BodyRule>,
    expected_headers: &Option<Http1HeaderRule>,
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    tracing::debug!(
        settle_ms,
        "Driver::Http2AfterSettle: sleeping for active-HC settle"
    );
    tokio::time::sleep(Duration::from_millis(*settle_ms)).await;

    let upstream_resp = drive_http2(upstream_addr, method, path, host, &[])
        .await
        .context("upstream envoy http2 drive (after settle)")?;
    let subject_resp = drive_http2(subject_addr, method, path, host, &[])
        .await
        .context("envoy-rust http2 drive (after settle)")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    // Status: envoy ↔ envoy-rust under `response_status: exact`.
    if matches!(
        expectations.equivalence.response_status,
        Some(StatusRule::Exact)
    ) && upstream_resp.status != subject_resp.status
    {
        bail!(
            "response status mismatch under `response_status: exact`\n  \
                     upstream: {}\n  subject:  {}",
            upstream_resp.status,
            subject_resp.status,
        );
    }
    if let Some(es) = expected_status {
        if upstream_resp.status != *es {
            bail!(
                "upstream status {} != expected {}",
                upstream_resp.status,
                es,
            );
        }
        if subject_resp.status != *es {
            bail!("subject status {} != expected {}", subject_resp.status, es,);
        }
    }

    if let Some(rule) = &expectations.equivalence.response_body {
        assert_body_rule(rule, &upstream_resp.body, &subject_resp.body)?;
    }
    if let Some(Http1BodyRule::ByteExact { body }) = expected_body {
        let expected = body.as_bytes();
        if upstream_resp.body != expected {
            bail!(
                "upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                upstream_resp.body,
                expected,
            );
        }
        if subject_resp.body != expected {
            bail!(
                "subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                subject_resp.body,
                expected,
            );
        }
    }

    if matches!(
        expected_headers,
        Some(Http1HeaderRule::SetEqualModuloAllowList)
    ) {
        diff_headers(
            &upstream_resp.headers,
            &subject_resp.headers,
            HEADER_ALLOW_LIST,
        )
        .context("diff_headers (set_equal_modulo_allow_list)")?;
    }
    Ok(())
}

/// `Driver::Http1KeepAlive` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http1_keep_alive_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    requests: &[Http1KeepAliveRequest],
    settle_ms: &u64,
    expected_stats: &[KeepAliveExpectedStat],
    admin_scrapes: &[AdminScrapeCase],
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        admin_host_port,
        budget,
        ..
    } = *ctx;
    use tokio::io::AsyncWriteExt;
    let upstream_admin_port = upstream.host_admin_port().ok_or_else(|| {
                anyhow::anyhow!(
                    "Driver::Http1KeepAlive requires the upstream container to expose its admin port; either the fixture's envoy.yaml does not reference {{ADMIN_PORT}} or the harness failed to wire `expose_admin_port = true`",
                )
            })?;
    let subject_admin_port = admin_host_port.ok_or_else(|| {
                anyhow::anyhow!(
                    "Driver::Http1KeepAlive requires the subject's envoy-rust.yaml to reference {{ADMIN_PORT}}; the harness only reserves a host admin port when one of the templates contains the marker",
                )
            })?;
    let upstream_admin_addr: SocketAddr = format!("127.0.0.1:{upstream_admin_port}").parse()?;
    let subject_admin_addr: SocketAddr = format!("127.0.0.1:{subject_admin_port}").parse()?;
    wait_accept_ready(upstream_admin_addr, budget)
        .await
        .context("upstream admin listener never became accept-ready")?;
    wait_accept_ready(subject_admin_addr, budget)
        .await
        .context("envoy-rust admin listener never became accept-ready")?;

    // For each proxy, open ONE TCP keep-alive conn and issue the
    // full `requests` sequence on it. Drain each response's
    // Content-Length body before issuing the next so the stream
    // starts at a clean boundary; assert the per-request status
    // matches `expected_status` so a hung response or class
    // mismatch surfaces at the request boundary (not only at the
    // post-settle scrape).
    for (side_name, proxy_addr) in &[("upstream", upstream_addr), ("subject", subject_addr)] {
        let mut stream = tokio::net::TcpStream::connect(*proxy_addr)
            .await
            .with_context(|| format!("{side_name}: connecting to proxy {proxy_addr}"))?;
        for req in requests {
            let wire = format!(
                "{} {} HTTP/1.1\r\nhost: {}\r\nconnection: keep-alive\r\n\r\n",
                req.method, req.path, req.host,
            );
            stream.write_all(wire.as_bytes()).await.with_context(|| {
                format!("{side_name}: writing request {} {}", req.method, req.path)
            })?;
            stream.flush().await.with_context(|| {
                format!("{side_name}: flushing request {} {}", req.method, req.path)
            })?;
            let (resp_status, resp_headers, resp_body) =
                read_h1_response_full(&mut stream).await.with_context(|| {
                    format!(
                        "{side_name}: reading response for {} {}",
                        req.method, req.path
                    )
                })?;
            anyhow::ensure!(
                resp_status == req.expected_status,
                "{side_name}: expected status {} for {} {}, got {}",
                req.expected_status,
                req.method,
                req.path,
                resp_status,
            );
            // 14.2 D8.1a (SPEC correction B-3): optional per-request
            // body + header assertions. Each side independently must
            // satisfy these (NOT a cross-proxy diff of the values).
            if let Some(Http1BodyRule::ByteExact { body }) = &req.expected_body {
                anyhow::ensure!(
                    resp_body == body.as_bytes(),
                    "{side_name}: body mismatch for {} {} — expected {:?}, got {:?}",
                    req.method,
                    req.path,
                    body.as_bytes(),
                    resp_body,
                );
            }
            if let Some(h) = &req.require_header_present {
                anyhow::ensure!(
                    resp_headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(h)),
                    "{side_name}: expected header {h} present for {} {}",
                    req.method,
                    req.path,
                );
            }
            if let Some(h) = &req.require_header_absent {
                anyhow::ensure!(
                    !resp_headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(h)),
                    "{side_name}: expected header {h} absent for {} {}",
                    req.method,
                    req.path,
                );
            }
            if let Some(rule) = &req.require_header_value {
                let got = resp_headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(&rule.name))
                    .map(|(_, v)| v.as_str());
                anyhow::ensure!(
                    got == Some(rule.value.as_str()),
                    "{side_name}: header {} expected value {:?} for {} {}, got {:?}",
                    rule.name,
                    rule.value,
                    req.method,
                    req.path,
                    got,
                );
            }
        }
        drop(stream);
    }

    // Single post-request settle: covers stat-write visibility on
    // BOTH sides under the same Relaxed-ordering budget (SPEC §6
    // signpost 11). A fixture-time bump may be needed if a future
    // counter site lands behind a longer happens-before chain.
    tokio::time::sleep(Duration::from_millis(*settle_ms)).await;

    assert_expected_stats_bilaterally(upstream_admin_addr, subject_admin_addr, expected_stats)
        .await?;

    // 18 Task 6 (ADR-0049): after the bilateral stat scrape, run any
    // `admin_scrapes` sub-cases through the SAME per-case assertion
    // logic the `Driver::AdminScrape` arm uses (the shared
    // `assert_admin_scrape_case` fn). Fixture 0026 uses this to assert
    // `/config_dump`'s `ClustersConfigDump` reflects the file-based CDS
    // load. Empty for fixtures 0020-0025 (`#[serde(default)]`), so the
    // loop is skipped and the dispatch shape is unchanged for them.
    // No HCM pre-requests here — keep-alive already drove the
    // data-plane requests above, so pass an empty `hcm_addrs` map and
    // `&[]` pre-requests (drive_admin_scrape then hits the admin
    // listener directly).
    let empty_hcm: std::collections::BTreeMap<String, SocketAddr> =
        std::collections::BTreeMap::new();
    let no_pre: &[PreRequest] = &[];
    for case in admin_scrapes {
        let upstream_resp = drive_admin_scrape(no_pre, upstream_admin_addr, &empty_hcm, &case.path)
            .await
            .with_context(|| format!("upstream envoy admin scrape: {}", case.path))?;
        let subject_resp = drive_admin_scrape(no_pre, subject_admin_addr, &empty_hcm, &case.path)
            .await
            .with_context(|| format!("envoy-rust admin scrape: {}", case.path))?;
        assert_admin_scrape_case(case, &upstream_resp, &subject_resp)?;
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http2KeepAlive` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http2_keep_alive_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    requests: &[Http1KeepAliveRequest],
    settle_ms: &u64,
    expected_stats: &[KeepAliveExpectedStat],
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        admin_host_port,
        budget,
        ..
    } = *ctx;
    let upstream_admin_port = upstream.host_admin_port().ok_or_else(|| {
                anyhow::anyhow!(
                    "Driver::Http2KeepAlive requires the upstream container to expose its admin port; either the fixture's envoy.yaml does not reference {{ADMIN_PORT}} or the harness failed to wire `expose_admin_port = true`",
                )
            })?;
    let subject_admin_port = admin_host_port.ok_or_else(|| {
                anyhow::anyhow!(
                    "Driver::Http2KeepAlive requires the subject's envoy-rust.yaml to reference {{ADMIN_PORT}}; the harness only reserves a host admin port when one of the templates contains the marker",
                )
            })?;
    let upstream_admin_addr: SocketAddr = format!("127.0.0.1:{upstream_admin_port}").parse()?;
    let subject_admin_addr: SocketAddr = format!("127.0.0.1:{subject_admin_port}").parse()?;
    wait_accept_ready(upstream_admin_addr, budget)
        .await
        .context("upstream admin listener never became accept-ready")?;
    wait_accept_ready(subject_admin_addr, budget)
        .await
        .context("envoy-rust admin listener never became accept-ready")?;

    for (side_name, proxy_addr) in &[("upstream", upstream_addr), ("subject", subject_addr)] {
        drive_http2_keep_alive(*proxy_addr, requests, side_name).await?;
    }

    // Single post-request settle: covers stat-write visibility on
    // BOTH sides under the same Relaxed-ordering budget (SPEC §6
    // signpost 11). Mirrors the H1 sibling at `Driver::Http1KeepAlive`.
    tokio::time::sleep(Duration::from_millis(*settle_ms)).await;

    assert_expected_stats_bilaterally(upstream_admin_addr, subject_admin_addr, expected_stats)
        .await?;

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http1ProbeList` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http1_probe_list_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probes: &[Http1Probe],
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    // Iterate probes; per-probe equivalence cascade mirrors the
    // single-probe Driver::Http1 arm. Subject + upstream tear down
    // AFTER all probes have run.
    for probe in probes {
        let upstream_resp = drive_http1(
            upstream_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
            probe.body.as_deref().map(str::as_bytes),
        )
        .await
        .with_context(|| format!("upstream envoy http1 drive (probe {})", probe.name))?;
        let subject_resp = drive_http1(
            subject_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
            probe.body.as_deref().map(str::as_bytes),
        )
        .await
        .with_context(|| format!("envoy-rust http1 drive (probe {})", probe.name))?;

        // Status: envoy ↔ envoy-rust under `response_status: exact`.
        if matches!(
            expectations.equivalence.response_status,
            Some(StatusRule::Exact)
        ) && upstream_resp.status != subject_resp.status
        {
            bail!(
                "probe {}: response status mismatch under `response_status: exact`\n  \
                         upstream: {}\n  subject:  {}",
                probe.name,
                upstream_resp.status,
                subject_resp.status,
            );
        }
        if let Some(es) = probe.expected_status {
            if upstream_resp.status != es {
                bail!(
                    "probe {}: upstream status {} != expected {}",
                    probe.name,
                    upstream_resp.status,
                    es,
                );
            }
            if subject_resp.status != es {
                bail!(
                    "probe {}: subject status {} != expected {}",
                    probe.name,
                    subject_resp.status,
                    es,
                );
            }
        }

        // Body. 06.1 Task 13 carryforward: route through
        // `assert_body_rule` so BodyRule variants dispatch through the
        // single centralized helper instead of inline `matches!`.
        if let Some(rule) = &expectations.equivalence.response_body {
            assert_body_rule(rule, &upstream_resp.body, &subject_resp.body)
                .with_context(|| format!("probe {}", probe.name))?;
        }
        if let Some(Http1BodyRule::ByteExact { body }) = &probe.expected_body {
            let expected = body.as_bytes();
            if upstream_resp.body != expected {
                bail!(
                    "probe {}: upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                    probe.name,
                    upstream_resp.body,
                    expected,
                );
            }
            if subject_resp.body != expected {
                bail!(
                    "probe {}: subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                    probe.name,
                    subject_resp.body,
                    expected,
                );
            }
        }

        // Headers.
        if matches!(
            probe.expected_headers,
            Some(Http1HeaderRule::SetEqualModuloAllowList)
        ) {
            diff_headers(
                &upstream_resp.headers,
                &subject_resp.headers,
                HEADER_ALLOW_LIST,
            )
            .with_context(|| format!("probe {}: diff_headers", probe.name))?;
        }
    }
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http1HashSweep` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http1_hash_sweep_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    keys: &[String],
    path: &str,
    host: &str,
    expected_status: &u16,
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    use std::collections::BTreeSet;

    if keys.is_empty() {
        bail!("Driver::Http1HashSweep requires a non-empty `keys:` sweep");
    }

    // Extract the selected-backend marker from an echo response body's
    // leading `backend: <marker>\n` line. The two backends are spawned
    // `--body-marker backend_1`/`backend_2`, so this line names WHICH
    // backend the RING_HASH ring selected for the request hash.
    fn extract_marker(body: &[u8], side: &str, key: &str) -> Result<String> {
        let text = std::str::from_utf8(body)
            .with_context(|| format!("{side} response body for key `{key}` is not utf8"))?;
        let first = text.lines().next().unwrap_or("");
        let marker = first.strip_prefix("backend: ").ok_or_else(|| {
            anyhow::anyhow!(
                "{side} response body for key `{key}` does not begin with \
                         `backend: <marker>`; got first line `{first}`"
            )
        })?;
        Ok(marker.trim().to_string())
    }

    // Probe one key once on one proxy; return the selected marker.
    async fn probe_marker(
        addr: SocketAddr,
        path: &str,
        host: &str,
        key: &str,
        expected_status: u16,
        side: &str,
    ) -> Result<String> {
        let extra = vec![("x-hash-key".to_string(), key.to_string())];
        let resp = drive_http1(addr, &Http1Method::Get, path, host, &extra, None)
            .await
            .with_context(|| format!("{side} http1 drive (key `{key}`)"))?;
        if resp.status != expected_status {
            bail!(
                "{side} status {} != expected {} (key `{key}`)",
                resp.status,
                expected_status,
            );
        }
        extract_marker(&resp.body, side, key)
    }

    let mut upstream_markers: BTreeSet<String> = BTreeSet::new();
    let mut subject_markers: BTreeSet<String> = BTreeSet::new();

    for key in keys {
        // First selection on each side.
        let up1 =
            probe_marker(upstream_addr, path, host, key, *expected_status, "upstream").await?;
        let su1 = probe_marker(subject_addr, path, host, key, *expected_status, "subject").await?;

        // STRONG (the core differential): cross-proxy identical
        // RING_HASH selection for this key.
        if up1 != su1 {
            bail!(
                "RING_HASH cross-proxy selection mismatch for x-hash-key `{key}`:\n  \
                         upstream Envoy -> `{up1}`\n  envoy-rust      -> `{su1}`\n\
                         (the locked xxHash64 ring — ADR-0070 — must select the SAME backend)"
            );
        }

        // STABILITY: the SAME key must select the SAME backend on a
        // repeat request, on each proxy independently.
        let up2 =
            probe_marker(upstream_addr, path, host, key, *expected_status, "upstream").await?;
        let su2 = probe_marker(subject_addr, path, host, key, *expected_status, "subject").await?;
        if up2 != up1 {
            bail!(
                "RING_HASH instability on upstream Envoy for key `{key}`: \
                         first=`{up1}` repeat=`{up2}` (same key must hit same backend)"
            );
        }
        if su2 != su1 {
            bail!(
                "RING_HASH instability on envoy-rust for key `{key}`: \
                         first=`{su1}` repeat=`{su2}` (same key must hit same backend)"
            );
        }

        upstream_markers.insert(up1);
        subject_markers.insert(su1);
    }

    // SPREAD: over the full sweep BOTH backends must be selected on EACH
    // side. A sweep that collapses to a single backend does not exercise
    // ring distribution and is treated as a failure.
    let expected_spread: BTreeSet<String> = ["backend_1".to_string(), "backend_2".to_string()]
        .into_iter()
        .collect();
    if upstream_markers != expected_spread {
        bail!(
            "RING_HASH spread failure on upstream Envoy: the {}-key sweep selected only \
                     {:?} (expected BOTH backend_1 and backend_2)",
            keys.len(),
            upstream_markers,
        );
    }
    if subject_markers != expected_spread {
        bail!(
            "RING_HASH spread failure on envoy-rust: the {}-key sweep selected only \
                     {:?} (expected BOTH backend_1 and backend_2)",
            keys.len(),
            subject_markers,
        );
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http1RouteSelect` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http1_route_select_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probes: &[RouteSelectProbe],
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    if probes.is_empty() {
        bail!("Driver::Http1RouteSelect requires a non-empty `probes:` list");
    }

    // Extract the selected-backend marker from an echo response body's
    // leading `backend: <marker>\n` line. The two backends are spawned
    // `--body-marker backend_1`/`backend_2`, so this line names WHICH
    // backend the subset selected.
    fn extract_marker(body: &[u8], side: &str, name: &str) -> Result<String> {
        let text = std::str::from_utf8(body)
            .with_context(|| format!("{side} response body for probe `{name}` is not utf8"))?;
        let first = text.lines().next().unwrap_or("");
        let marker = first.strip_prefix("backend: ").ok_or_else(|| {
            anyhow::anyhow!(
                "{side} response body for probe `{name}` does not begin with \
                         `backend: <marker>`; got first line `{first}`"
            )
        })?;
        Ok(marker.trim().to_string())
    }

    // Drive one probe on one proxy; return the full response (status +
    // body) after asserting the status matches.
    async fn drive_probe(
        addr: SocketAddr,
        path: &str,
        expected_status: u16,
        side: &str,
        name: &str,
    ) -> Result<DriveHttp1Result> {
        let resp = drive_http1(addr, &Http1Method::Get, path, "localhost", &[], None)
            .await
            .with_context(|| format!("{side} http1 drive (probe `{name}` path `{path}`)"))?;
        if resp.status != expected_status {
            bail!(
                "{side} status {} != expected {} (probe `{name}` path `{path}`)",
                resp.status,
                expected_status,
            );
        }
        Ok(resp)
    }

    for probe in probes {
        let up = drive_probe(
            upstream_addr,
            &probe.path,
            probe.expected_status,
            "upstream",
            &probe.name,
        )
        .await?;
        let su = drive_probe(
            subject_addr,
            &probe.path,
            probe.expected_status,
            "subject",
            &probe.name,
        )
        .await?;

        match &probe.expected_marker {
            // 200 probe: STRONG cross-proxy identical subset selection,
            // AND agreement with the §A oracle marker.
            Some(expected) => {
                let up_marker = extract_marker(&up.body, "upstream", &probe.name)?;
                let su_marker = extract_marker(&su.body, "subject", &probe.name)?;
                if up_marker != su_marker {
                    bail!(
                        "subset LB cross-proxy selection mismatch for probe `{}` (path `{}`):\n  \
                                 upstream Envoy -> `{up_marker}`\n  envoy-rust      -> `{su_marker}`\n\
                                 (the §6.2-LOCKED subset resolution — ADR-0074 — must select the SAME backend)",
                        probe.name,
                        probe.path,
                    );
                }
                if &up_marker != expected {
                    bail!(
                        "subset LB §A oracle mismatch for probe `{}` (path `{}`): \
                                 selected `{up_marker}` but oracle expects `{expected}`",
                        probe.name,
                        probe.path,
                    );
                }
            }
            // 503 (NO_FALLBACK) probe: each side's body is the fixed
            // 19-byte `no healthy upstream` local reply (byte-equal
            // cross-proxy).
            None => {
                const NO_HEALTHY: &str = "no healthy upstream";
                let up_body = std::str::from_utf8(&up.body).with_context(|| {
                    format!("upstream body for probe `{}` is not utf8", probe.name)
                })?;
                let su_body = std::str::from_utf8(&su.body).with_context(|| {
                    format!("subject body for probe `{}` is not utf8", probe.name)
                })?;
                if up_body != NO_HEALTHY {
                    bail!(
                        "upstream Envoy NO_FALLBACK body mismatch for probe `{}` (path `{}`): \
                                 expected `{NO_HEALTHY}`, got `{up_body}`",
                        probe.name,
                        probe.path,
                    );
                }
                if su_body != NO_HEALTHY {
                    bail!(
                        "envoy-rust NO_FALLBACK body mismatch for probe `{}` (path `{}`): \
                                 expected `{NO_HEALTHY}`, got `{su_body}`",
                        probe.name,
                        probe.path,
                    );
                }
            }
        }
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http1RdsReload` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http1_rds_reload_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    pre_probes: &[Http1Probe],
    reload: &RdsReloadStep,
    post_probes: &[Http1Probe],
) -> Result<()> {
    let FixtureCtx {
        fixture_dir,
        expectations,
        upstream_addr,
        subject_addr,
        upstream_kvs_refs,
        subject_kvs_refs,
        upstream_rds_path,
        subject_rds_path,
        ..
    } = *ctx;
    // An RDS-reload fixture MUST carry an `rds.yaml` (the reload swaps
    // the file-based RouteConfiguration). `subject_rds_path` is always
    // bound, but its file only exists when the fixture is RDS-based.
    if upstream_rds_path.is_none() {
        bail!(
            "Driver::Http1RdsReload requires a file-based RDS fixture \
                     (no {{{{RDS_PATH}}}} marker / rds.yaml found)"
        );
    }

    // 1. pre_probes — bilateral equivalence on the ORIGINAL table.
    for probe in pre_probes {
        run_http1_probe_bilateral(
            upstream_addr,
            subject_addr,
            &expectations.equivalence,
            probe,
            "pre",
        )
        .await?;
    }

    // 2. Read + render the POST-reload RDS template per-side, exactly
    //    like rds.yaml (same kv ref slices, same residual-marker guard).
    let reload_tpl = std::fs::read_to_string(fixture_dir.join(&reload.reload_file))
        .with_context(|| format!("reading RDS reload file {}", reload.reload_file))?;
    let upstream_reload = render_yaml(&reload_tpl, upstream_kvs_refs);
    let subject_reload = render_yaml(&reload_tpl, subject_kvs_refs);
    if let Some(marker) = residual_marker(&upstream_reload) {
        bail!("unsubstituted marker {{{{{marker}}}}} in rendered upstream reload RDS");
    }
    if let Some(marker) = residual_marker(&subject_reload) {
        bail!("unsubstituted marker {{{{{marker}}}}} in rendered subject reload RDS");
    }

    // 3. Reload BOTH sides via atomic-rename (the ONLY rewrite Envoy's
    //    default file-watch observes — §6.2/ADR-0066). Subject = host
    //    file; upstream = in-container file via docker exec.
    atomic_rename_over(subject_rds_path, &subject_reload)
        .context("atomic-rename of reloaded subject RDS file")?;
    upstream
        .reload_rds_atomic(&upstream_reload)
        .await
        .context("atomic-rename of reloaded upstream container RDS file")?;

    // 4. Wait — bounded — for BOTH sides to converge on the new table,
    //    polling the discriminator (its expected_status/body define
    //    "converged"). NOT a fixed sleep.
    let budget = Duration::from_millis(reload.settle_budget_ms);
    wait_for_reload_convergence(upstream_addr, &reload.discriminator, budget)
        .await
        .context("upstream Envoy never converged on reloaded RDS table")?;
    wait_for_reload_convergence(subject_addr, &reload.discriminator, budget)
        .await
        .context("envoy-rust never converged on reloaded RDS table")?;

    // 5. post_probes — bilateral equivalence on the RELOADED table.
    for probe in post_probes {
        run_http1_probe_bilateral(
            upstream_addr,
            subject_addr,
            &expectations.equivalence,
            probe,
            "post",
        )
        .await?;
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http1EdsReload` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http1_eds_reload_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    pre_probes: &[Http1Probe],
    reload: &EdsReloadStep,
    post_probes: &[Http1Probe],
) -> Result<()> {
    let FixtureCtx {
        fixture_dir,
        expectations,
        upstream_addr,
        subject_addr,
        upstream_kvs_refs,
        subject_kvs_refs,
        upstream_eds_path,
        subject_eds_path,
        ..
    } = *ctx;
    // M26-2 GUARD (folds the phase-26 fix the RDS arm omitted): a
    // discriminator with NEITHER expected_status NOR expected_body would
    // make `wait_for_reload_convergence` return Ok on the FIRST poll
    // (`status_ok && body_ok == true` with both absent), reporting
    // spurious instant "convergence" before the reload took effect. Bail
    // BEFORE driving anything.
    if !eds_reload_discriminator_is_load_bearing(&reload.discriminator) {
        bail!(
            "Driver::Http1EdsReload discriminator {} carries neither \
                     expected_status nor expected_body — it would report spurious \
                     instant convergence (M26-2); the EDS-reload discriminator MUST \
                     be load-bearing",
            reload.discriminator.name,
        );
    }

    // An EDS-reload fixture MUST carry an `eds.yaml` (the reload swaps the
    // file-based ClusterLoadAssignment endpoint). `subject_eds_path` is
    // always bound, but its file only exists when the fixture is EDS-based.
    if upstream_eds_path.is_none() {
        bail!(
            "Driver::Http1EdsReload requires a file-based EDS fixture \
                     (no {{{{EDS_PATH}}}} marker / eds.yaml found)"
        );
    }

    // 1. pre_probes — bilateral equivalence on the ORIGINAL endpoint (backend_1).
    for probe in pre_probes {
        run_http1_probe_bilateral(
            upstream_addr,
            subject_addr,
            &expectations.equivalence,
            probe,
            "pre",
        )
        .await?;
    }

    // 2. Read + render the POST-reload EDS template per-side, exactly
    //    like eds.yaml (same kv ref slices, same residual-marker guard).
    //    The per-side `{{EDS_BACKEND_IP}}` resolves to the numeric
    //    host-gateway IP (upstream) vs `127.0.0.1` (subject) — EDS rejects
    //    hostnames (L1), so the two renditions carry DIFFERENT numeric
    //    endpoint addresses.
    let reload_tpl = std::fs::read_to_string(fixture_dir.join(&reload.reload_file))
        .with_context(|| format!("reading EDS reload file {}", reload.reload_file))?;
    let upstream_reload = render_yaml(&reload_tpl, upstream_kvs_refs);
    let subject_reload = render_yaml(&reload_tpl, subject_kvs_refs);
    if let Some(marker) = residual_marker(&upstream_reload) {
        bail!("unsubstituted marker {{{{{marker}}}}} in rendered upstream reload EDS");
    }
    if let Some(marker) = residual_marker(&subject_reload) {
        bail!("unsubstituted marker {{{{{marker}}}}} in rendered subject reload EDS");
    }

    // 3. Reload BOTH sides via atomic-rename (the ONLY rewrite Envoy's
    //    default file-watch observes — §6.2/ADR-0066). Subject = host
    //    file; upstream = in-container file via docker exec.
    atomic_rename_over(subject_eds_path, &subject_reload)
        .context("atomic-rename of reloaded subject EDS file")?;
    upstream
        .reload_eds_atomic(&upstream_reload)
        .await
        .context("atomic-rename of reloaded upstream container EDS file")?;

    // 4. Wait — bounded — for BOTH sides to converge on the new endpoint
    //    set, polling the discriminator (its expected_status/body define
    //    "converged"). NOT a fixed sleep.
    let budget = Duration::from_millis(reload.settle_budget_ms);
    wait_for_reload_convergence(upstream_addr, &reload.discriminator, budget)
        .await
        .context("upstream Envoy never converged on reloaded EDS endpoint set")?;
    wait_for_reload_convergence(subject_addr, &reload.discriminator, budget)
        .await
        .context("envoy-rust never converged on reloaded EDS endpoint set")?;

    // 5. post_probes — bilateral equivalence on the SWAPPED endpoint (backend_2).
    for probe in post_probes {
        run_http1_probe_bilateral(
            upstream_addr,
            subject_addr,
            &expectations.equivalence,
            probe,
            "post",
        )
        .await?;
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http2ProbeList` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http2_probe_list_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probes: &[Http1Probe],
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    for probe in probes {
        let upstream_resp = drive_http2(
            upstream_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
        )
        .await
        .with_context(|| format!("upstream envoy http2 drive (probe {})", probe.name))?;
        let subject_resp = drive_http2(
            subject_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
        )
        .await
        .with_context(|| format!("envoy-rust http2 drive (probe {})", probe.name))?;

        // Status: envoy ↔ envoy-rust under `response_status: exact`.
        if matches!(
            expectations.equivalence.response_status,
            Some(StatusRule::Exact)
        ) && upstream_resp.status != subject_resp.status
        {
            bail!(
                "probe {}: response status mismatch under `response_status: exact`\n  \
                         upstream: {}\n  subject:  {}",
                probe.name,
                upstream_resp.status,
                subject_resp.status,
            );
        }
        if let Some(es) = probe.expected_status {
            if upstream_resp.status != es {
                bail!(
                    "probe {}: upstream status {} != expected {}",
                    probe.name,
                    upstream_resp.status,
                    es,
                );
            }
            if subject_resp.status != es {
                bail!(
                    "probe {}: subject status {} != expected {}",
                    probe.name,
                    subject_resp.status,
                    es,
                );
            }
        }

        // Body. Route through `assert_body_rule` (mirrors the
        // Http1ProbeList arm).
        if let Some(rule) = &expectations.equivalence.response_body {
            assert_body_rule(rule, &upstream_resp.body, &subject_resp.body)
                .with_context(|| format!("probe {}", probe.name))?;
        }
        if let Some(Http1BodyRule::ByteExact { body }) = &probe.expected_body {
            let expected = body.as_bytes();
            if upstream_resp.body != expected {
                bail!(
                    "probe {}: upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                    probe.name,
                    upstream_resp.body,
                    expected,
                );
            }
            if subject_resp.body != expected {
                bail!(
                    "probe {}: subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                    probe.name,
                    subject_resp.body,
                    expected,
                );
            }
        }

        // Headers.
        if matches!(
            probe.expected_headers,
            Some(Http1HeaderRule::SetEqualModuloAllowList)
        ) {
            diff_headers(
                &upstream_resp.headers,
                &subject_resp.headers,
                HEADER_ALLOW_LIST,
            )
            .with_context(|| format!("probe {}: diff_headers", probe.name))?;
        }
    }
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// `Driver::Http1WithAccessLog` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
// The parameter list mirrors the `Driver` variant's fields, threaded straight
// from the `run_fixture` dispatcher; bundling them into a struct would add
// indirection without clarifying the dispatch (same disposition as
// `upstream::start`).
#[allow(clippy::too_many_arguments)]
async fn run_http1_with_access_log_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    method: &str,
    path: &str,
    host: &str,
    expected_status: &u16,
    expected_body: &BodyRule,
    expected_headers: &HeaderRule,
    extra_headers: &[(String, String)],
    expected_access_log_paths: &AccessLogPaths,
    expected_access_log_lines: &[Vec<crate::access_log::AccessLogLineRule>],
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    // Wire-protocol leg: reuse drive_http1 unchanged from 04.1.
    // `Http1Method` is the harness's narrow GET-only enum today;
    // mirror the conversion shape used by `drive_admin_scrape`.
    let http1_method = match method {
        "GET" => Http1Method::Get,
        other => bail!("Driver::Http1WithAccessLog: unsupported method {:?}", other),
    };
    let upstream_resp = drive_http1(
        upstream_addr,
        &http1_method,
        path,
        host,
        extra_headers,
        None,
    )
    .await
    .context("upstream envoy http1 drive (Http1WithAccessLog)")?;
    let subject_resp = drive_http1(subject_addr, &http1_method, path, host, extra_headers, None)
        .await
        .context("envoy-rust http1 drive (Http1WithAccessLog)")?;

    // envoy-rust's access-log emit is a fire-and-forget task that runs after the
    // response completes; subject.shutdown() is SIGKILL (subject.rs TODO on
    // graceful drain). Wait for the line to land BEFORE killing the process —
    // CI run 27059869720 lost the race (`envoy=1 envoy-rust=0`) because the old
    // post-shutdown poll could never observe a write from a dead process.
    let envoy_rust_path = std::path::PathBuf::from(&expected_access_log_paths.envoy_rust);
    if !wait_file_nonempty(&envoy_rust_path, std::time::Duration::from_secs(5)).await {
        tracing::warn!(
            "differential: envoy-rust access-log file {} still empty after 5s (pre-shutdown wait)",
            envoy_rust_path.display()
        );
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    // Status: per-driver `expected_status` — each side independently
    // equals the value (the Http1WithAccessLog variant requires it,
    // unlike Http1's Option<u16>).
    if upstream_resp.status != *expected_status {
        bail!(
            "upstream status {} != expected {}",
            upstream_resp.status,
            expected_status,
        );
    }
    if subject_resp.status != *expected_status {
        bail!(
            "subject status {} != expected {}",
            subject_resp.status,
            expected_status,
        );
    }

    // Body: envoy ↔ envoy-rust via `assert_body_rule` (mirrors the
    // Http1 arm). The `expected_body: BodyRule` is required (not
    // Option), so dispatch unconditionally.
    assert_body_rule(expected_body, &upstream_resp.body, &subject_resp.body)?;

    // Headers: per-driver allow-list diff between envoy ↔ envoy-rust.
    // `expected_headers: HeaderRule` is required; the only variant
    // today is `SetEqualModuloAllowList` (matches Http1HeaderRule's
    // sole variant).
    match expected_headers {
        HeaderRule::SetEqualModuloAllowList => {
            diff_headers(
                &upstream_resp.headers,
                &subject_resp.headers,
                HEADER_ALLOW_LIST,
            )
            .context("diff_headers (set_equal_modulo_allow_list)")?;
        }
    }

    // Envoy-side flush is driven by container stop (SIGTERM) above. The
    // non-empty poll (rather than an exists check) preserves the fix for CI run
    // 26375100437: the FileSink creates the file at open time, so existence
    // alone does not imply the line landed.
    let envoy_path = std::path::PathBuf::from(&expected_access_log_paths.envoy);
    if !wait_file_nonempty(&envoy_path, std::time::Duration::from_secs(5)).await {
        tracing::warn!(
            "differential: envoy access-log file {} still empty after 5s (post container-stop wait)",
            envoy_path.display()
        );
    }
    // One final yield to let the OS flush any in-flight bytes that crossed the
    // metadata-len threshold but haven't fully landed.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let envoy_contents = std::fs::read_to_string(&envoy_path)
        .with_context(|| format!("read envoy access-log file at {}", envoy_path.display()))?;
    let envoy_rust_contents = std::fs::read_to_string(&envoy_rust_path).with_context(|| {
        format!(
            "read envoy-rust access-log file at {}",
            envoy_rust_path.display()
        )
    })?;
    let envoy_lines: Vec<String> = envoy_contents.lines().map(|s| s.to_owned()).collect();
    let envoy_rust_lines: Vec<String> = envoy_rust_contents.lines().map(|s| s.to_owned()).collect();

    crate::access_log::assert_access_log_lines_equivalent(
        &envoy_lines,
        &envoy_rust_lines,
        expected_access_log_lines,
    )
    .map_err(|e| {
        anyhow::anyhow!(
            "access log mismatch: {}\nenvoy lines: {:?}\nenvoy-rust lines: {:?}",
            e,
            envoy_lines,
            envoy_rust_lines,
        )
    })?;
    Ok(())
}

/// `Driver::Http1AccessLogByteExact` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http1_access_log_byte_exact_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probes: &[AccessLogByteExactProbe],
    expected_access_log_paths: &AccessLogPaths,
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    // Phase 70 (ADR-0141): filter-suppressed probes emit NO line, so the
    // target is the LOGGED count — not `probes.len()`, which would starve
    // every `wait_file_lines` poll below for the full flush timeout.
    let expected_lines = expected_logged_count(probes);

    // Drive each probe in order against BOTH proxies. Reuse the exact
    // request build (`drive_http1`) the `Http1WithAccessLog` arm uses;
    // assert each side's status matches the probe's `expected_status`.
    for (idx, probe) in probes.iter().enumerate() {
        let body: Option<&[u8]> = probe.body.as_deref().map(|s| s.as_bytes());
        let upstream_resp = drive_http1(
            upstream_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
            body,
        )
        .await
        .with_context(|| {
            format!("upstream envoy http1 drive (Http1AccessLogByteExact probe {idx})")
        })?;
        let subject_resp = drive_http1(
            subject_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
            body,
        )
        .await
        .with_context(|| format!("envoy-rust http1 drive (Http1AccessLogByteExact probe {idx})"))?;
        if upstream_resp.status != probe.expected_status {
            bail!(
                "probe {idx}: upstream status {} != expected {}",
                upstream_resp.status,
                probe.expected_status,
            );
        }
        if subject_resp.status != probe.expected_status {
            bail!(
                "probe {idx}: subject status {} != expected {}",
                subject_resp.status,
                probe.expected_status,
            );
        }
    }

    // envoy-rust's access-log emit is fire-and-forget; wait for all N
    // lines to land BEFORE shutdown (SIGKILL) — mirrors the
    // `Http1WithAccessLog` pre-shutdown wait, generalised to N lines.
    let envoy_rust_path = std::path::PathBuf::from(&expected_access_log_paths.envoy_rust);
    // Budget generously: Envoy's FileAccessLog flushes on a periodic
    // timer (~10s default) rather than per-record, so a multi-probe
    // scrape must outlast one flush cycle (a 5s budget saw only the
    // first, already-flushed line — CI/local observed).
    if !wait_file_lines(&envoy_rust_path, expected_lines, ACCESS_LOG_FLUSH_WAIT).await {
        tracing::warn!(
            "differential: envoy-rust access-log file {} still has < {} lines after {:?} (pre-shutdown wait)",
            envoy_rust_path.display(),
            expected_lines,
            ACCESS_LOG_FLUSH_WAIT,
        );
    }

    // Wait for the upstream-Envoy file to reach all N lines BEFORE
    // stopping the container. Envoy's FileAccessLog buffers and flushes
    // on a periodic timer; testcontainers tears the container down with
    // `docker rm -f` (SIGKILL, no graceful drain), so any line still
    // buffered at stop is LOST. Polling while the container is alive
    // lets the flush timer fire and land all N lines (CI-observed: a
    // post-stop wait saw only the first, already-flushed line).
    let envoy_path = std::path::PathBuf::from(&expected_access_log_paths.envoy);
    if !wait_file_lines(&envoy_path, expected_lines, ACCESS_LOG_FLUSH_WAIT).await {
        tracing::warn!(
            "differential: envoy access-log file {} still has < {} lines after {:?} (pre-stop wait)",
            envoy_path.display(),
            expected_lines,
            ACCESS_LOG_FLUSH_WAIT,
        );
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    // One final yield to let the OS flush any in-flight bytes.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let envoy_contents = std::fs::read_to_string(&envoy_path)
        .with_context(|| format!("read envoy access-log file at {}", envoy_path.display()))?;
    let envoy_rust_contents = std::fs::read_to_string(&envoy_rust_path).with_context(|| {
        format!(
            "read envoy-rust access-log file at {}",
            envoy_rust_path.display()
        )
    })?;
    let envoy_lines: Vec<String> = envoy_contents.lines().map(|s| s.to_owned()).collect();
    let envoy_rust_lines: Vec<String> = envoy_rust_contents.lines().map(|s| s.to_owned()).collect();

    // Each NON-suppressed probe emits exactly one access-log line.
    if envoy_lines.len() != expected_lines {
        bail!(
            "envoy emitted {} access-log lines but {} were expected to be logged; lines: {:?}",
            envoy_lines.len(),
            expected_lines,
            envoy_lines,
        );
    }
    if envoy_rust_lines.len() != expected_lines {
        bail!(
            "envoy-rust emitted {} access-log lines but {} were expected to be logged; lines: {:?}",
            envoy_rust_lines.len(),
            expected_lines,
            envoy_rust_lines,
        );
    }

    crate::access_log::assert_access_log_lines_byte_identical(&envoy_lines, &envoy_rust_lines)
        .map_err(|e| {
            anyhow::anyhow!(
                "access log byte-exact mismatch: {}\nenvoy lines: {:?}\nenvoy-rust lines: {:?}",
                e,
                envoy_lines,
                envoy_rust_lines,
            )
        })?;
    Ok(())
}

/// `Driver::Http2AccessLogByteExact` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_http2_access_log_byte_exact_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    probes: &[AccessLogByteExactProbe],
    expected_access_log_paths: &AccessLogPaths,
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    // Phase 70 (ADR-0141): filter-suppressed probes emit NO line, so the
    // target is the LOGGED count — not `probes.len()`, which would starve
    // every `wait_file_lines` poll below for the full flush timeout.
    let expected_lines = expected_logged_count(probes);

    for (idx, probe) in probes.iter().enumerate() {
        let upstream_resp = drive_http2(
            upstream_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
        )
        .await
        .with_context(|| {
            format!("upstream envoy http2 drive (Http2AccessLogByteExact probe {idx})")
        })?;
        let subject_resp = drive_http2(
            subject_addr,
            &probe.method,
            &probe.path,
            &probe.host,
            &probe.extra_headers,
        )
        .await
        .with_context(|| format!("envoy-rust http2 drive (Http2AccessLogByteExact probe {idx})"))?;
        if upstream_resp.status != probe.expected_status {
            bail!(
                "probe {idx}: upstream status {} != expected {}",
                upstream_resp.status,
                probe.expected_status,
            );
        }
        if subject_resp.status != probe.expected_status {
            bail!(
                "probe {idx}: subject status {} != expected {}",
                subject_resp.status,
                probe.expected_status,
            );
        }
    }

    let envoy_rust_path = std::path::PathBuf::from(&expected_access_log_paths.envoy_rust);
    if !wait_file_lines(&envoy_rust_path, expected_lines, ACCESS_LOG_FLUSH_WAIT).await {
        tracing::warn!(
            "differential: envoy-rust access-log file {} still has < {} lines after {:?} (pre-shutdown wait)",
            envoy_rust_path.display(),
            expected_lines,
            ACCESS_LOG_FLUSH_WAIT,
        );
    }

    let envoy_path = std::path::PathBuf::from(&expected_access_log_paths.envoy);
    if !wait_file_lines(&envoy_path, expected_lines, ACCESS_LOG_FLUSH_WAIT).await {
        tracing::warn!(
            "differential: envoy access-log file {} still has < {} lines after {:?} (pre-stop wait)",
            envoy_path.display(),
            expected_lines,
            ACCESS_LOG_FLUSH_WAIT,
        );
    }

    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let envoy_contents = std::fs::read_to_string(&envoy_path)
        .with_context(|| format!("read envoy access-log file at {}", envoy_path.display()))?;
    let envoy_rust_contents = std::fs::read_to_string(&envoy_rust_path).with_context(|| {
        format!(
            "read envoy-rust access-log file at {}",
            envoy_rust_path.display()
        )
    })?;
    let envoy_lines: Vec<String> = envoy_contents.lines().map(|s| s.to_owned()).collect();
    let envoy_rust_lines: Vec<String> = envoy_rust_contents.lines().map(|s| s.to_owned()).collect();

    if envoy_lines.len() != expected_lines {
        bail!(
            "envoy emitted {} access-log lines but {} were expected to be logged; lines: {:?}",
            envoy_lines.len(),
            expected_lines,
            envoy_lines,
        );
    }
    if envoy_rust_lines.len() != expected_lines {
        bail!(
            "envoy-rust emitted {} access-log lines but {} were expected to be logged; lines: {:?}",
            envoy_rust_lines.len(),
            expected_lines,
            envoy_rust_lines,
        );
    }

    crate::access_log::assert_access_log_lines_byte_identical(&envoy_lines, &envoy_rust_lines)
        .map_err(|e| {
            anyhow::anyhow!(
                "access log byte-exact mismatch: {}\nenvoy lines: {:?}\nenvoy-rust lines: {:?}",
                e,
                envoy_lines,
                envoy_rust_lines,
            )
        })?;
    Ok(())
}

/// `Driver::Http2` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
// The parameter list mirrors the `Driver` variant's fields, threaded straight
// from the `run_fixture` dispatcher; bundling them into a struct would add
// indirection without clarifying the dispatch (same disposition as
// `upstream::start`).
#[allow(clippy::too_many_arguments)]
async fn run_http2_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    method: &Http1Method,
    path: &str,
    host: &str,
    expected_status: &Option<u16>,
    expected_body: &Option<Http1BodyRule>,
    expected_headers: &Option<Http1HeaderRule>,
) -> Result<()> {
    let FixtureCtx {
        expectations,
        upstream_addr,
        subject_addr,
        ..
    } = *ctx;
    let upstream_resp = drive_http2(upstream_addr, method, path, host, &[])
        .await
        .context("upstream envoy http2 drive")?;
    let subject_resp = drive_http2(subject_addr, method, path, host, &[])
        .await
        .context("envoy-rust http2 drive")?;
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);

    // Status: envoy ↔ envoy-rust under `response_status: exact`.
    if matches!(
        expectations.equivalence.response_status,
        Some(StatusRule::Exact)
    ) && upstream_resp.status != subject_resp.status
    {
        bail!(
            "response status mismatch under `response_status: exact`\n  \
                     upstream: {}\n  subject:  {}",
            upstream_resp.status,
            subject_resp.status,
        );
    }
    // Per-driver `expected_status`: each side independently equals it.
    if let Some(es) = expected_status {
        if upstream_resp.status != *es {
            bail!(
                "upstream status {} != expected {}",
                upstream_resp.status,
                es,
            );
        }
        if subject_resp.status != *es {
            bail!("subject status {} != expected {}", subject_resp.status, es,);
        }
    }

    // Body: envoy ↔ envoy-rust per `equivalence.response_body` rule.
    // 06.1 Task 13 carryforward: route through `assert_body_rule` (see
    // Driver::Http1 arm above for rationale).
    if let Some(rule) = &expectations.equivalence.response_body {
        assert_body_rule(rule, &upstream_resp.body, &subject_resp.body)?;
    }
    // Per-driver `expected_body`: each side independently equals bytes.
    if let Some(Http1BodyRule::ByteExact { body }) = expected_body {
        let expected = body.as_bytes();
        if upstream_resp.body != expected {
            bail!(
                "upstream body != expected\n  upstream: {:?}\n  expected: {:?}",
                upstream_resp.body,
                expected,
            );
        }
        if subject_resp.body != expected {
            bail!(
                "subject body != expected\n  subject:  {:?}\n  expected: {:?}",
                subject_resp.body,
                expected,
            );
        }
    }

    // Headers: per-driver allow-list diff between envoy ↔ envoy-rust.
    if matches!(
        expected_headers,
        Some(Http1HeaderRule::SetEqualModuloAllowList)
    ) {
        diff_headers(
            &upstream_resp.headers,
            &subject_resp.headers,
            HEADER_ALLOW_LIST,
        )
        .context("diff_headers (set_equal_modulo_allow_list)")?;
    }
    Ok(())
}

/// `Driver::AdminScrape` arm of `run_fixture` — extracted verbatim (pure code
/// motion; the arm-level rationale comments remain at the dispatch site).
async fn run_admin_scrape_arm(
    ctx: &FixtureCtx<'_>,
    upstream: upstream::UpstreamProxy,
    mut subject: subject::Subject,
    pre_admin_actions: &[AdminAction],
    pre_requests: &[PreRequest],
    scrapes: &[AdminScrapeCase],
    post_admin_assertions: &[AdminAssertion],
) -> Result<()> {
    let FixtureCtx {
        upstream_addr,
        subject_addr,
        admin_host_port,
        budget,
        ..
    } = *ctx;
    if scrapes.is_empty() {
        bail!("Driver::AdminScrape requires at least one sub-case (`scrapes:` must be non-empty)");
    }
    let upstream_admin_port = upstream.host_admin_port().ok_or_else(|| {
                anyhow::anyhow!(
                    "Driver::AdminScrape requires the upstream container to expose its admin port; either the fixture's envoy.yaml does not reference {{ADMIN_PORT}} or the harness failed to wire `expose_admin_port = true`",
                )
            })?;
    let subject_admin_port = admin_host_port.ok_or_else(|| {
                anyhow::anyhow!(
                    "Driver::AdminScrape requires the subject's envoy-rust.yaml to reference {{ADMIN_PORT}}; the harness only reserves a host admin port when one of the templates contains the marker",
                )
            })?;
    let upstream_admin_addr: SocketAddr = format!("127.0.0.1:{upstream_admin_port}").parse()?;
    let subject_admin_addr: SocketAddr = format!("127.0.0.1:{subject_admin_port}").parse()?;
    wait_accept_ready(upstream_admin_addr, budget)
        .await
        .context("upstream admin listener never became accept-ready")?;
    wait_accept_ready(subject_admin_addr, budget)
        .await
        .context("envoy-rust admin listener never became accept-ready")?;

    // Per-side HCM port maps. Today only `"PORT"` is populated; the
    // map shape matches the existing template-marker discipline so
    // future fixtures with multiple HCM listeners slot in without
    // harness churn.
    let mut upstream_hcm = std::collections::BTreeMap::new();
    upstream_hcm.insert("PORT".to_string(), upstream_addr);
    let mut subject_hcm = std::collections::BTreeMap::new();
    subject_hcm.insert("PORT".to_string(), subject_addr);

    // 08.2 Task 7 (D16) temporal dispatch sequence per PLAN
    // architecture-decision lock-in #18:
    //
    //   1. pre_requests          — HCM-side traffic so the
    //                              registry has counters
    //                              incremented (pre-drain
    //                              baseline).
    //   2. pre_admin_actions     — POSTs against the admin
    //                              listener (e.g. fixture
    //                              0015's `/drain_listeners`
    //                              trigger).
    //   3. scrapes               — GETs against the admin
    //                              listener (post-drain state
    //                              assertions).
    //   4. post_admin_assertions — wire-level invariants (e.g.
    //                              fixture 0015's
    //                              `data_plane_connection_refused`).
    //
    // The YAML field order (`pre_admin_actions` declared
    // BEFORE `pre_requests` in the struct definition) is
    // independent of this temporal order. The YAML shape is
    // for reader ergonomics (drain trigger at the top of the
    // block); the temporal order is for fixture-semantic
    // correctness ("verify baseline → drain → verify
    // post-drain state → wire-level assertion").
    //
    // STEP 1: pre_requests. Drive each HCM-side pre-request
    // against BOTH proxies, then sleep ~50ms (SPEC §6
    // signpost 11) to let the registry's Relaxed-ordered
    // counter writes become visible to subsequent scrapes.
    // Extracted out of the scrape loop so it precedes
    // pre_admin_actions; the per-side dispatch shape mirrors
    // `drive_admin_scrape`'s internal pre-request handling
    // verbatim. (When `pre_requests.is_empty()`, both the
    // dispatch loop and the visibility sleep are skipped.)
    for pre in pre_requests {
        let method = match pre.method.to_ascii_uppercase().as_str() {
            "GET" => Http1Method::Get,
            other => bail!(
                "PreRequest.method {other:?} not supported (only GET); widen drive_admin_scrape to add more"
            ),
        };
        let upstream_pre_addr = *upstream_hcm.get(&pre.port_key).ok_or_else(|| {
            anyhow::anyhow!("unknown PreRequest.port_key on upstream: {}", pre.port_key)
        })?;
        let subject_pre_addr = *subject_hcm.get(&pre.port_key).ok_or_else(|| {
            anyhow::anyhow!("unknown PreRequest.port_key on subject: {}", pre.port_key)
        })?;
        drive_http1(upstream_pre_addr, &method, &pre.path, &pre.host, &[], None)
            .await
            .with_context(|| {
                format!(
                    "upstream envoy pre-request {} {} (host={}, port_key={})",
                    pre.method, pre.path, pre.host, pre.port_key,
                )
            })?;
        drive_http1(subject_pre_addr, &method, &pre.path, &pre.host, &[], None)
            .await
            .with_context(|| {
                format!(
                    "envoy-rust pre-request {} {} (host={}, port_key={})",
                    pre.method, pre.path, pre.host, pre.port_key,
                )
            })?;
    }
    if !pre_requests.is_empty() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // STEP 2: pre_admin_actions — POSTs against BOTH proxies'
    // admin listeners. Each action is dispatched serially per
    // proxy with anyhow context tags naming the side and the
    // action path.
    for action in pre_admin_actions {
        match action {
            AdminAction::Post {
                path,
                expected_status,
            } => {
                drive_admin_post(upstream_admin_addr, path, *expected_status)
                    .await
                    .with_context(|| format!("upstream envoy pre_admin_action POST {path}"))?;
                drive_admin_post(subject_admin_addr, path, *expected_status)
                    .await
                    .with_context(|| format!("envoy-rust pre_admin_action POST {path}"))?;
            }
        }
    }

    // STEP 3: the scrape loop. pre_requests already fired in
    // STEP 1; pass `&[]` so drive_admin_scrape skips its
    // bundled pre-request + visibility-sleep path.
    let pre: &[PreRequest] = &[];
    let mut results = Vec::with_capacity(scrapes.len());
    for case in scrapes {
        let upstream_resp = drive_admin_scrape(pre, upstream_admin_addr, &upstream_hcm, &case.path)
            .await
            .with_context(|| format!("upstream envoy admin scrape: {}", case.path))?;
        let subject_resp = drive_admin_scrape(pre, subject_admin_addr, &subject_hcm, &case.path)
            .await
            .with_context(|| format!("envoy-rust admin scrape: {}", case.path))?;
        results.push((case, upstream_resp, subject_resp));
    }

    // 08.1 Task 11 diagnostic: set `DIFFERENTIAL_DUMP_ADMIN=1`
    // to dump ALL sub-cases' bodies (both sides + content-type)
    // BEFORE any assertion fires. Used during empirical allow-list
    // seeding (SPEC §6 signpost 12) to capture both proxies'
    // outputs in a single failing run rather than iterating
    // assertion-by-assertion. Leave-on disposition matches the
    // dispatch-level diagnostics established by 04.x's
    // RUST_LOG-controlled tracing layer.
    if std::env::var("DIFFERENTIAL_DUMP_ADMIN").is_ok() {
        for (case, upstream_resp, subject_resp) in &results {
            eprintln!(
                "=== {} ===\n--- ENVOY ({}, ct={:?}) ---\n{}\n--- ENVOY-RUST ({}, ct={:?}) ---\n{}\n=== /{} ===",
                case.path,
                upstream_resp.status,
                upstream_resp
                    .headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or(""),
                String::from_utf8_lossy(&upstream_resp.body),
                subject_resp.status,
                subject_resp
                    .headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or(""),
                String::from_utf8_lossy(&subject_resp.body),
                case.path,
            );
        }
    }
    for (case, upstream_resp, subject_resp) in &results {
        assert_admin_scrape_case(case, upstream_resp, subject_resp)?;
    }

    // STEP 4: post_admin_assertions — wire-level invariants
    // verified AFTER the scrape loop. Today's only variant
    // (`DataPlaneConnectionRefused`) probes a data-plane
    // listener address with a poll loop and accepts either
    // ECONNREFUSED or an immediate-EOF connect as evidence the
    // listener is drained. Per architecture-decision lock-in
    // #18, this is the final step in the temporal sequence —
    // fired BEFORE the subject/upstream teardown so the
    // data-plane addresses are still live (drained-but-live;
    // post-drain "kernel-refused" is the success signal).
    //
    // Per-side dispatch (08.2 Task 8 extension): the YAML
    // `listener_address` is template-rendered per-side via the
    // `{{PORT}}` + `{{ADMIN_PORT}}` markers — `{{PORT}}` resolves
    // to the side's HCM data-plane port (from
    // `upstream_hcm` / `subject_hcm`), `{{ADMIN_PORT}}` resolves
    // to the side's admin port (`upstream_admin_port` /
    // `subject_admin_port`). Both sides are probed; the
    // assertion succeeds only if BOTH proxies refuse the
    // connection within `within_ms`. A YAML address with no
    // markers (a fully-formed `host:port` literal) is probed
    // verbatim on BOTH sides — useful for fixtures where
    // upstream + subject share an address shape, and the
    // backward-compatible shape for Task 7's existing literal-
    // address tests at the parsing layer. The template-render
    // mirrors the existing `render_yaml` mechanism used to
    // substitute `{{PORT}}` / `{{ADMIN_PORT}}` in the fixture
    // YAMLs at config-load time; we re-use the same marker
    // grammar here so a fixture author writes one address
    // template and gets per-side resolution for free.
    for assertion in post_admin_assertions {
        match assertion {
            AdminAssertion::DataPlaneConnectionRefused {
                listener_address,
                within_ms,
            } => {
                let upstream_addr_s = listener_address
                    .replace("{{PORT}}", &upstream_addr.port().to_string())
                    .replace("{{ADMIN_PORT}}", &upstream_admin_port.to_string());
                let subject_addr_s = listener_address
                    .replace("{{PORT}}", &subject_addr.port().to_string())
                    .replace("{{ADMIN_PORT}}", &subject_admin_port.to_string());
                let upstream_parsed: SocketAddr = upstream_addr_s
                            .parse()
                            .with_context(|| {
                                format!(
                                    "parsing post_admin_assertion upstream listener_address {upstream_addr_s:?} (template {listener_address:?})",
                                )
                            })?;
                let subject_parsed: SocketAddr = subject_addr_s
                            .parse()
                            .with_context(|| {
                                format!(
                                    "parsing post_admin_assertion subject listener_address {subject_addr_s:?} (template {listener_address:?})",
                                )
                            })?;
                let within = Duration::from_millis(*within_ms);
                assert_data_plane_connection_refused(upstream_parsed, within)
                            .await
                            .with_context(|| {
                                format!(
                                    "post_admin_assertion: upstream data_plane_connection_refused {upstream_addr_s} (template {listener_address})",
                                )
                            })?;
                assert_data_plane_connection_refused(subject_parsed, within)
                            .await
                            .with_context(|| {
                                format!(
                                    "post_admin_assertion: subject data_plane_connection_refused {subject_addr_s} (template {listener_address})",
                                )
                            })?;
            }
        }
    }

    // Teardown LAST so post_admin_assertions observe the
    // post-drain state against a still-running subject /
    // upstream.
    subject.shutdown(Duration::from_secs(5)).await.ok();
    drop(upstream);
    Ok(())
}

/// 18 Task 6 (ADR-0049): per-`AdminScrapeCase` assertion logic, shared by the
/// `Driver::AdminScrape` scrape loop and the `Driver::Http1KeepAlive`
/// `admin_scrapes` loop. Asserts, for one sub-case against both proxies'
/// already-fetched responses, that (1) `expected_status` is matched by each
/// side independently, (2) each side independently carries a matching
/// (case-insensitive) `content-type` header equal to `expected_content_type`,
/// and (3) the `expected_body_rule` `BodyRule`-dispatched envoy ↔ envoy-rust
/// body comparison passes (e.g. `JsonShape`, `PrometheusExposition`,
/// `TextLines`, `ByteExact`).
///
/// Extracted verbatim from the former inline `Driver::AdminScrape` body so the
/// json_shape / prometheus diff code is not duplicated across the two drivers.
fn assert_admin_scrape_case(
    case: &AdminScrapeCase,
    upstream_resp: &DriveHttp1Result,
    subject_resp: &DriveHttp1Result,
) -> Result<()> {
    // expected_status: each side independently equals it.
    if upstream_resp.status != case.expected_status {
        bail!(
            "upstream admin status {} != expected {} (path={})",
            upstream_resp.status,
            case.expected_status,
            case.path,
        );
    }
    if subject_resp.status != case.expected_status {
        bail!(
            "subject admin status {} != expected {} (path={})",
            subject_resp.status,
            case.expected_status,
            case.path,
        );
    }

    // expected_content_type: each side independently has a
    // `content-type:` header whose (case-insensitive) value matches.
    check_content_type(&upstream_resp.headers, &case.expected_content_type)
        .with_context(|| format!("upstream admin content-type: {}", case.path))?;
    check_content_type(&subject_resp.headers, &case.expected_content_type)
        .with_context(|| format!("envoy-rust admin content-type: {}", case.path))?;

    // Body rule: dispatch on BodyRule variant.
    assert_body_rule(
        &case.expected_body_rule,
        &upstream_resp.body,
        &subject_resp.body,
    )
    .with_context(|| format!("admin body rule: {}", case.path))?;
    Ok(())
}

/// 06.1 D6.a: assert that the headers carry a `content-type:` whose
/// (case-insensitive) value equals `expected`. Bails with a descriptive error
/// if the header is missing or mismatched.
///
/// 08.1 Task 11: matches BOTH the bare-media-type-only form (`text/plain`)
/// AND the parameter-bearing form (`text/plain; charset=UTF-8`) when the
/// expected value is the bare form. This accommodates fixture 0014's
/// `/clusters` + `/listeners` envoy ↔ envoy-rust divergence: upstream
/// Envoy v1.33 emits `text/plain; charset=UTF-8`; envoy-rust emits the
/// bare `text/plain` (per the renderers in `crates/envoy-admin/src/endpoint.rs`,
/// Tasks 8 + 9 — content-type pin is intentional, BEHAVIOR_CONTRACT.md
/// will absorb the charset-parameter variance at the row-level in a
/// follow-on phase). When `expected` carries explicit parameters (e.g.
/// fixture 0011's `text/plain; charset=UTF-8` for `/stats/prometheus`),
/// the actual value MUST carry the same parameter shape — strict match.
fn check_content_type(headers: &[(String, String)], expected: &str) -> Result<()> {
    let actual = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!("response is missing the `content-type` header (expected {expected:?})")
        })?;
    // If `expected` has no parameter, allow the actual to optionally carry
    // any parameter suffix (e.g. `; charset=UTF-8`). Otherwise strict match.
    let expected_has_param = expected.contains(';');
    if expected_has_param {
        if !actual.eq_ignore_ascii_case(expected) {
            bail!("content-type {actual:?} != expected {expected:?}");
        }
    } else {
        let actual_media = actual.split(';').next().unwrap_or(actual).trim();
        if !actual_media.eq_ignore_ascii_case(expected) {
            bail!("content-type {actual:?} (bare {actual_media:?}) != expected {expected:?}");
        }
    }
    Ok(())
}

/// 06.1 D6.b: dispatch on `BodyRule` variant. `ByteExact` enforces exact byte
/// equality between envoy ↔ envoy-rust. `PrometheusExposition` parses both
/// bodies via `parse_prometheus_metric_names` and asserts the metric-name
/// symmetric difference is empty after subtracting the per-fixture
/// allow-lists (does NOT compare numeric values; 06.3 extends).
fn assert_body_rule(rule: &BodyRule, envoy_body: &[u8], rust_body: &[u8]) -> Result<()> {
    match rule {
        BodyRule::ByteExact => {
            if envoy_body != rust_body {
                bail!(
                    "byte-exact body mismatch\n  upstream: {envoy_body:?}\n  subject:  {rust_body:?}",
                );
            }
            Ok(())
        }
        BodyRule::PrometheusExposition {
            allowlist_envoy_only,
            allowlist_envoy_rust_only,
            value_exact,
            value_must_be_zero,
            value_present_only,
        } => {
            let envoy_names = parse_prometheus_metric_names(envoy_body);
            let rust_names = parse_prometheus_metric_names(rust_body);
            let allow_envoy: std::collections::BTreeSet<String> =
                allowlist_envoy_only.iter().cloned().collect();
            let allow_rust: std::collections::BTreeSet<String> =
                allowlist_envoy_rust_only.iter().cloned().collect();
            let envoy_only: Vec<String> = envoy_names
                .difference(&rust_names)
                .filter(|n| !allow_envoy.contains(*n))
                .cloned()
                .collect();
            let rust_only: Vec<String> = rust_names
                .difference(&envoy_names)
                .filter(|n| !allow_rust.contains(*n))
                .cloned()
                .collect();
            if !envoy_only.is_empty() || !rust_only.is_empty() {
                bail!(
                    "prometheus exposition metric-name sets diverged after allow-lists:\n  envoy-only:      {envoy_only:?}\n  envoy-rust-only: {rust_only:?}",
                );
            }
            // 06.3 D18.3: value_exact — both proxies must have the expected value.
            if !value_exact.is_empty() {
                let envoy_samples = parse_prometheus_samples(envoy_body);
                let rust_samples = parse_prometheus_samples(rust_body);
                for (name, expected) in value_exact {
                    let envoy_val = envoy_samples.get(name.as_str()).copied();
                    let rust_val = rust_samples.get(name.as_str()).copied();
                    if envoy_val != Some(*expected) || rust_val != Some(*expected) {
                        bail!(
                            "value_exact mismatch for {name:?}: expected {expected}, \
                             envoy={envoy_val:?}, envoy-rust={rust_val:?}",
                        );
                    }
                }
            }
            // 06.3 D18.3: value_must_be_zero — both proxies must report 0.
            if !value_must_be_zero.is_empty() {
                let envoy_samples = parse_prometheus_samples(envoy_body);
                let rust_samples = parse_prometheus_samples(rust_body);
                for name in value_must_be_zero {
                    let envoy_val = envoy_samples.get(name.as_str()).copied();
                    let rust_val = rust_samples.get(name.as_str()).copied();
                    if envoy_val != Some(0) || rust_val != Some(0) {
                        bail!(
                            "value_must_be_zero violated for {name:?}: \
                             envoy={envoy_val:?}, envoy-rust={rust_val:?}",
                        );
                    }
                }
            }
            // 06.3 D18.3: value_present_only — both proxies must have the name; value may differ.
            if !value_present_only.is_empty() {
                let envoy_samples = parse_prometheus_samples(envoy_body);
                let rust_samples = parse_prometheus_samples(rust_body);
                for name in value_present_only {
                    let envoy_present = envoy_samples.contains_key(name.as_str());
                    let rust_present = rust_samples.contains_key(name.as_str());
                    if !envoy_present || !rust_present {
                        bail!(
                            "value_present_only: {name:?} missing from one or both proxies: \
                             envoy={envoy_present}, envoy-rust={rust_present}",
                        );
                    }
                }
            }
            Ok(())
        }
        // 08.1 Task 10 (D15) + Task 11 strictness wiring: JSON-shape
        // assertions. Parse both bodies as JSON objects; assert:
        //   - required_keys present on BOTH sides (top-level)
        //   - required_subtree.expected == envoy_sub AND == rust_sub
        //     (Task 11: `expected` was a schema-level no-op at Task 10)
        //   - envoy-only top-level keys NOT in `allowlist_envoy_only_keys`
        //     AND NOT in `value_may_differ_keys` MUST appear on the
        //     envoy-rust side (and symmetrically)
        //   - top-level keys present on BOTH sides and NOT in
        //     `value_may_differ_keys` MUST serialize equal.
        BodyRule::JsonShape {
            required_keys,
            required_subtree,
            allowlist_envoy_only_keys,
            allowlist_envoy_rust_only_keys,
            value_may_differ_keys,
        } => {
            let envoy_json: serde_json::Value = serde_json::from_slice(envoy_body)
                .context("envoy body is not valid JSON for BodyRule::JsonShape")?;
            let rust_json: serde_json::Value = serde_json::from_slice(rust_body)
                .context("envoy-rust body is not valid JSON for BodyRule::JsonShape")?;
            let envoy_obj = envoy_json
                .as_object()
                .context("envoy body is not a JSON object for BodyRule::JsonShape")?;
            let rust_obj = rust_json
                .as_object()
                .context("envoy-rust body is not a JSON object for BodyRule::JsonShape")?;
            for key in required_keys {
                if !envoy_obj.contains_key(key) {
                    bail!("required_keys: {key:?} missing on envoy side");
                }
                if !rust_obj.contains_key(key) {
                    bail!("required_keys: {key:?} missing on envoy-rust side");
                }
            }
            if let Some(subtree) = required_subtree {
                // 20 Task 6 (ADR-0052): resolve the dotted path per-side. The
                // shared `path` is used unless a per-side override
                // (`path_envoy` / `path_envoy_rust`) is present — needed because
                // the RoutesConfigDump entry lands at a different `configs[*]`
                // index on Envoy vs envoy-rust.
                let envoy_path = subtree.envoy_path();
                let rust_path = subtree.rust_path();
                let envoy_sub = walk_pointer(&envoy_json, envoy_path)
                    .with_context(|| format!("envoy required_subtree path {envoy_path:?}"))?;
                let rust_sub = walk_pointer(&rust_json, rust_path)
                    .with_context(|| format!("envoy-rust required_subtree path {rust_path:?}"))?;
                // 08.1 Task 11: also assert against `expected` (Task 10
                // accepted the field but never consulted it). Compare
                // via the canonical serde_json string form so both YAML-
                // native scalars and JSON-native values normalize the
                // same way.
                let expected_json: serde_json::Value =
                    serde_json::to_value(&subtree.expected).context(
                        "converting required_subtree.expected (serde_yaml::Value) to serde_json::Value",
                    )?;
                let expected_str = serde_json::to_string(&expected_json)
                    .context("rendering required_subtree.expected as JSON")?;
                let envoy_str = serde_json::to_string(envoy_sub)
                    .context("rendering envoy required_subtree sub-value as JSON")?;
                let rust_str = serde_json::to_string(rust_sub)
                    .context("rendering envoy-rust required_subtree sub-value as JSON")?;
                if envoy_str != expected_str {
                    bail!(
                        "required_subtree {envoy_path:?} envoy != expected:\n  envoy:    {envoy_str}\n  expected: {expected_str}",
                    );
                }
                if rust_str != expected_str {
                    bail!(
                        "required_subtree {rust_path:?} envoy-rust != expected:\n  envoy-rust: {rust_str}\n  expected:   {expected_str}",
                    );
                }
            }

            // 08.1 Task 11: top-level key-set diff between envoy and
            // envoy-rust, modulo the per-side allow-lists + the bilateral
            // `value_may_differ_keys` (which acts as a key-presence-and-
            // value-drift allowance on both sides simultaneously).
            let allow_envoy: std::collections::BTreeSet<&str> = allowlist_envoy_only_keys
                .iter()
                .map(String::as_str)
                .collect();
            let allow_rust: std::collections::BTreeSet<&str> = allowlist_envoy_rust_only_keys
                .iter()
                .map(String::as_str)
                .collect();
            let may_differ: std::collections::BTreeSet<&str> =
                value_may_differ_keys.iter().map(String::as_str).collect();
            let envoy_keys: std::collections::BTreeSet<&str> =
                envoy_obj.keys().map(String::as_str).collect();
            let rust_keys: std::collections::BTreeSet<&str> =
                rust_obj.keys().map(String::as_str).collect();
            let envoy_only: Vec<&str> = envoy_keys
                .difference(&rust_keys)
                .copied()
                .filter(|k| !allow_envoy.contains(*k) && !may_differ.contains(*k))
                .collect();
            let rust_only: Vec<&str> = rust_keys
                .difference(&envoy_keys)
                .copied()
                .filter(|k| !allow_rust.contains(*k) && !may_differ.contains(*k))
                .collect();
            if !envoy_only.is_empty() || !rust_only.is_empty() {
                bail!(
                    "json_shape top-level keys diverged after allow-lists:\n  envoy-only:      {envoy_only:?}\n  envoy-rust-only: {rust_only:?}",
                );
            }

            // 08.1 Task 11: for keys present on BOTH sides AND not on
            // `value_may_differ_keys`, serialize-equal check.
            let shared: Vec<&str> = envoy_keys.intersection(&rust_keys).copied().collect();
            for key in shared {
                if may_differ.contains(key) {
                    continue;
                }
                let envoy_val = &envoy_obj[key];
                let rust_val = &rust_obj[key];
                let envoy_s = serde_json::to_string(envoy_val)
                    .with_context(|| format!("rendering envoy[{key:?}] as JSON"))?;
                let rust_s = serde_json::to_string(rust_val)
                    .with_context(|| format!("rendering envoy-rust[{key:?}] as JSON"))?;
                if envoy_s != rust_s {
                    bail!(
                        "json_shape shared key {key:?} value differs (not in value_may_differ_keys):\n  envoy:      {envoy_s}\n  envoy-rust: {rust_s}",
                    );
                }
            }
            Ok(())
        }
        // 08.1 Task 10 (D15) + Task 11 strictness wiring: line-oriented
        // text assertions. Treat both bodies as UTF-8 text, split on \n
        // via `str::lines`, and assert:
        //   - required_lines on BOTH sides (exact line match)
        //   - required_line_prefixes on BOTH sides (at least one line
        //     starts with each prefix)
        //   - envoy-only lines NOT in `allowlist_envoy_only_lines` MUST
        //     appear on the envoy-rust side; symmetrically for rust-only.
        BodyRule::TextLines {
            required_lines,
            required_line_prefixes,
            allowlist_envoy_only_lines,
            allowlist_envoy_rust_only_lines,
            allowlist_envoy_only_line_prefixes,
            allowlist_envoy_rust_only_line_prefixes,
        } => {
            let envoy_text = std::str::from_utf8(envoy_body)
                .context("envoy body is not valid UTF-8 for BodyRule::TextLines")?;
            let rust_text = std::str::from_utf8(rust_body)
                .context("envoy-rust body is not valid UTF-8 for BodyRule::TextLines")?;
            let envoy_lines: std::collections::BTreeSet<&str> = envoy_text.lines().collect();
            let rust_lines: std::collections::BTreeSet<&str> = rust_text.lines().collect();
            for line in required_lines {
                if !envoy_lines.contains(line.as_str()) {
                    bail!("required_lines: {line:?} missing on envoy side");
                }
                if !rust_lines.contains(line.as_str()) {
                    bail!("required_lines: {line:?} missing on envoy-rust side");
                }
            }
            for prefix in required_line_prefixes {
                if !envoy_lines.iter().any(|l| l.starts_with(prefix.as_str())) {
                    bail!("required_line_prefixes: no line starts with {prefix:?} on envoy side");
                }
                if !rust_lines.iter().any(|l| l.starts_with(prefix.as_str())) {
                    bail!(
                        "required_line_prefixes: no line starts with {prefix:?} on envoy-rust side"
                    );
                }
            }
            // 08.1 Task 11: line-set diff between envoy and envoy-rust,
            // modulo the per-side allow-lists (exact-line + prefix-line).
            let allow_envoy: std::collections::BTreeSet<&str> = allowlist_envoy_only_lines
                .iter()
                .map(String::as_str)
                .collect();
            let allow_rust: std::collections::BTreeSet<&str> = allowlist_envoy_rust_only_lines
                .iter()
                .map(String::as_str)
                .collect();
            // 08.1 Task 11 NEW: per-side line-prefix allow-lists.
            let allow_envoy_prefix: Vec<&str> = allowlist_envoy_only_line_prefixes
                .iter()
                .map(String::as_str)
                .collect();
            let allow_rust_prefix: Vec<&str> = allowlist_envoy_rust_only_line_prefixes
                .iter()
                .map(String::as_str)
                .collect();
            let envoy_only: Vec<&str> = envoy_lines
                .difference(&rust_lines)
                .copied()
                .filter(|l| {
                    !allow_envoy.contains(*l)
                        && !allow_envoy_prefix.iter().any(|p| l.starts_with(p))
                })
                .collect();
            let rust_only: Vec<&str> = rust_lines
                .difference(&envoy_lines)
                .copied()
                .filter(|l| {
                    !allow_rust.contains(*l) && !allow_rust_prefix.iter().any(|p| l.starts_with(p))
                })
                .collect();
            if !envoy_only.is_empty() || !rust_only.is_empty() {
                bail!(
                    "text_lines diverged after allow-lists:\n  envoy-only:      {envoy_only:?}\n  envoy-rust-only: {rust_only:?}",
                );
            }
            Ok(())
        }
    }
}

fn assert_equivalence(
    expectations: &Expectations,
    upstream_status: Option<u16>,
    subject_status: Option<u16>,
    upstream_body: &[u8],
    subject_body: &[u8],
) -> Result<()> {
    if matches!(
        expectations.equivalence.response_status,
        Some(StatusRule::Exact)
    ) {
        match (upstream_status, subject_status) {
            (Some(u), Some(s)) if u == s => {}
            (u, s) => bail!(
                "response status mismatch under `response_status: exact`\n  \
                 upstream: {u:?}\n  subject:  {s:?}"
            ),
        }
    }
    if let Some(rule) = &expectations.equivalence.response_body {
        assert_body_rule(rule, upstream_body, subject_body)?;
    }
    // Neither rule configured → silently pass + log a warning (SPEC §D5).
    if expectations.equivalence.response_status.is_none()
        && expectations.equivalence.response_body.is_none()
    {
        tracing::warn!(
            "fixture has neither response_status nor response_body equivalence rule — running as a smoke test"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 70 (ADR-0141): the byte-exact line-count target must EXCLUDE
    /// probes an access-log filter is expected to suppress. Pins the helper
    /// that feeds every `wait_file_lines` poll and line-count assertion in
    /// both the H1 and H2 byte-exact arms.
    #[test]
    fn expected_logged_count_excludes_suppressed() {
        let p = |expect_logged: bool| AccessLogByteExactProbe {
            method: Http1Method::Get,
            path: "/x".into(),
            host: "h".into(),
            extra_headers: vec![],
            body: None,
            expected_status: 200,
            expect_logged,
        };
        assert_eq!(expected_logged_count(&[p(true), p(false), p(true)]), 2);
        assert_eq!(expected_logged_count(&[p(true), p(true)]), 2);
    }

    #[test]
    fn expectations_parse_byte_exact() {
        let yaml =
            "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: { kind: byte_exact }\n";
        let e: Expectations = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert!(matches!(e.driver, Driver::TcpEcho));
    }

    #[test]
    fn expectations_reject_unknown_rule() {
        let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: sorta_equal\n";
        let r = serde_yaml::from_str::<Expectations>(yaml);
        assert!(r.is_err());
    }

    /// 66 (ADR-0123): the fixture-`0071` driver tag.
    #[test]
    fn parses_tcp_direct_response_driver() {
        let y = "driver:\n  kind: tcp_direct_response\nequivalence:\n  response_body:\n    kind: byte_exact\n";
        let e: Expectations = serde_yaml::from_str(y).expect("parses");
        assert!(matches!(e.driver, Driver::TcpDirectResponse));
    }

    // --- 67.1 D7: `expected_stats` on the raw-TCP driver family (SPEC R-8) ---

    /// 67.1 D7: the raw-TCP driver family gains `expected_stats`. Echo probe.
    #[test]
    fn parses_tcp_with_stats_echo_driver() {
        let yaml = r#"
driver:
  kind: tcp_with_stats
  probe: echo
  settle_ms: 500
  expected_stats:
    - { name: rbac_allow.rbac.allowed, value: 1 }
    - { name: rbac_allow.rbac.denied, value: 0 }
equivalence:
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::TcpWithStats {
            probe,
            settle_ms,
            expected_stats,
        } = &e.driver
        else {
            panic!("expected TcpWithStats, got {:?}", e.driver);
        };
        assert_eq!(*probe, TcpProbeKind::Echo);
        assert_eq!(*settle_ms, 500);
        assert_eq!(expected_stats.len(), 2);
        assert_eq!(expected_stats[0].name, "rbac_allow.rbac.allowed");
        assert_eq!(expected_stats[0].value, 1);
    }

    /// 67.1 D7: read-to-EOF probe — the DENY shape (send nothing, read to EOF).
    #[test]
    fn parses_tcp_with_stats_read_to_eof_driver() {
        let yaml = r#"
driver:
  kind: tcp_with_stats
  probe: read_to_eof
  settle_ms: 500
  expected_stats:
    - { name: rbac_deny.rbac.denied, value: 1 }
equivalence:
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::TcpWithStats { probe, .. } = &e.driver else {
            panic!("expected TcpWithStats")
        };
        assert_eq!(*probe, TcpProbeKind::ReadToEof);
    }

    /// 67.1 D7: `TcpWithStats` needs an admin port on BOTH sides — the whole
    /// point of the variant. It uses the `{{PORT}}` data-listener convention.
    #[test]
    fn tcp_with_stats_needs_admin_port_and_uses_port_key() {
        let driver = Driver::TcpWithStats {
            probe: TcpProbeKind::ReadToEof,
            settle_ms: 0,
            expected_stats: vec![],
        };
        assert_eq!(port_key_for(&driver), "PORT");
        assert!(driver_needs_admin_port(&driver));
        // The pre-existing raw-TCP drivers still do NOT.
        assert!(!driver_needs_admin_port(&Driver::TcpEcho));
        assert!(!driver_needs_admin_port(&Driver::TcpDirectResponse));
    }

    /// 67.1 D7: the pre-existing UNIT variants still deserialize from a bare
    /// `kind:` with no fields. Adding fields to them would have broken every
    /// landed expectations.yaml.
    #[test]
    fn unit_raw_tcp_drivers_still_parse_without_fields() {
        let e: Expectations = serde_yaml::from_str("driver:\n  kind: tcp_echo\n").expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        let e: Expectations =
            serde_yaml::from_str("driver:\n  kind: tcp_direct_response\n").expect("parses");
        assert!(matches!(e.driver, Driver::TcpDirectResponse));
    }

    // Regression for REVIEW.md M3: `#[serde(deny_unknown_fields)]` must reject
    // a typo'd or unexpected top-level key rather than silently dropping it.
    #[test]
    fn expectations_reject_unknown_field() {
        let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: { kind: byte_exact }\nfoo: bar\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown top-level field");
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "unexpected: {msg}");
    }

    // Regression for REVIEW.md M3 at the nested `Equivalence` level.
    #[test]
    fn equivalence_reject_unknown_field() {
        let yaml = "driver:\n  kind: tcp_echo\nequivalence:\n  response_body: { kind: byte_exact }\n  extra: true\n";
        let err = serde_yaml::from_str::<Expectations>(yaml)
            .expect_err("must reject unknown nested field");
        let msg = err.to_string();
        assert!(msg.contains("unknown field"), "unexpected: {msg}");
    }

    /// 13.1 D10 Task 7: `Driver::Http1KeepAlive` round-trips through the
    /// snake_case-tagged serde representation. Asserts the new variant
    /// alongside the `Http1KeepAliveRequest` and `KeepAliveExpectedStat`
    /// substructs parse from the on-disk YAML shape fixture 0020 uses
    /// (snake_case `kind:` discriminator per the `Driver` enum's
    /// `#[serde(tag = "kind", rename_all = "snake_case")]` attribute).
    #[test]
    fn driver_http1_keep_alive_round_trips_through_serde() {
        let yaml = r#"
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /
      host: backend_cluster
      expected_status: 200
  settle_ms: 100
  expected_stats:
    - name: cluster.backend_cluster.upstream_cx_total
      value: 1
"#;
        let exp: crate::Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
        let Driver::Http1KeepAlive {
            requests,
            settle_ms,
            expected_stats,
            admin_scrapes,
        } = exp.driver
        else {
            panic!("expected Driver::Http1KeepAlive");
        };
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].path, "/");
        assert_eq!(requests[0].host, "backend_cluster");
        assert_eq!(requests[0].expected_status, 200);
        assert_eq!(settle_ms, 100);
        assert_eq!(expected_stats.len(), 1);
        assert_eq!(
            expected_stats[0].name,
            "cluster.backend_cluster.upstream_cx_total"
        );
        assert_eq!(expected_stats[0].value, 1);
        // 18 Task 6: admin_scrapes defaults to empty when the key is absent.
        assert!(admin_scrapes.is_empty());
    }

    /// 14.2 D8.1a (SPEC correction B-3): the three optional per-request
    /// body/header assertion fields on `Http1KeepAliveRequest` round-trip
    /// through serde. Fixture 0022 needs to assert per-request body bytes +
    /// the presence/absence of `x-envoy-upstream-service-time` bilaterally,
    /// so the keep-alive request gained `expected_body` (reusing
    /// `Http1BodyRule::ByteExact`), `require_header_present`, and
    /// `require_header_absent`. All three are `#[serde(default)]` so the
    /// existing fixtures (0020/0021) that omit them still deserialize.
    #[test]
    fn http1_keep_alive_request_round_trips_body_and_header_assertions() {
        let yaml = r#"
method: GET
path: /fail
host: c1
expected_status: 500
expected_body: { kind: byte_exact, body: "server error\n" }
require_header_present: x-envoy-upstream-service-time
"#;
        let req: Http1KeepAliveRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.expected_status, 500);
        assert!(matches!(
            req.expected_body,
            Some(Http1BodyRule::ByteExact { .. })
        ));
        assert_eq!(
            req.require_header_present.as_deref(),
            Some("x-envoy-upstream-service-time")
        );
        assert!(req.require_header_absent.is_none());
    }

    /// 14.2 D8.1a: a `Http1KeepAliveRequest` that omits all three new fields
    /// (the shape existing fixtures 0020/0021 use) still deserializes, with
    /// the new fields defaulting to `None`. Guards the backward-compat
    /// contract for `#[serde(default)]`.
    #[test]
    fn http1_keep_alive_request_without_new_fields_still_parses() {
        let yaml = r#"
method: GET
path: /
host: backend_cluster
expected_status: 200
"#;
        let req: Http1KeepAliveRequest = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(req.expected_status, 200);
        assert!(req.expected_body.is_none());
        assert!(req.require_header_present.is_none());
        assert!(req.require_header_absent.is_none());
    }

    /// 18 Task 6 (ADR-0049): the per-side `{{CDS_PATH}}` + backend-marker
    /// rendering of a fixture `cds.yaml` template produces per-side
    /// substitutions — the upstream (container-perspective) side resolves
    /// `{{BACKEND_HOST}}` to `host.docker.internal`, while the subject
    /// (host-perspective) side resolves it to `127.0.0.1`. Mirrors the
    /// existing `render_yaml_substitutes_backend_keys_for_{envoy,envoy_rust}_side`
    /// tests, exercising the same `render_yaml` mechanic that `run_fixture`'s
    /// CDS pre-flight uses (the Docker-gated end-to-end proof is fixture
    /// 0026's job, Task 7).
    #[test]
    fn render_cds_template_substitutes_backend_host_per_side() {
        let cds_template = r#"
resources:
  - "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
    name: dynamic_backend
    load_assignment:
      endpoints:
        - lb_endpoints:
            - endpoint:
                address:
                  socket_address:
                    address: {{BACKEND_HOST}}
                    port_value: {{HTTP1_BACKEND_PORT}}
"#;
        let upstream_cds = render_yaml(
            cds_template,
            &[
                ("BACKEND_HOST", "host.docker.internal"),
                ("HTTP1_BACKEND_PORT", "31415"),
            ],
        );
        let subject_cds = render_yaml(
            cds_template,
            &[
                ("BACKEND_HOST", "127.0.0.1"),
                ("HTTP1_BACKEND_PORT", "31415"),
            ],
        );
        assert!(
            upstream_cds.contains("address: host.docker.internal"),
            "upstream cds should use container-perspective backend host: {upstream_cds}",
        );
        assert!(
            subject_cds.contains("address: 127.0.0.1"),
            "subject cds should use host-perspective backend host: {subject_cds}",
        );
        // Both renditions resolve the shared backend port marker.
        assert!(upstream_cds.contains("port_value: 31415"));
        assert!(subject_cds.contains("port_value: 31415"));
    }

    /// 18 Task 6 (ADR-0049): backend-launch detection must scan the CDS
    /// template too. Fixture 0026 places `{{HTTP1_BACKEND_PORT}}` and
    /// `{{BACKEND_HOST}}` ONLY in `cds.yaml`; the main templates carry just
    /// `{{CDS_PATH}}` + `{{PORT}}` + `{{ADMIN_PORT}}` (no backend markers, the
    /// configs routing to a CDS-defined cluster). A scan over only the main
    /// templates would report `needs_http1_backend == false` and the backend
    /// would never spawn. `scan_needs_marker` over the combined source (main +
    /// CDS) is what `run_fixture` uses; this locks that it sees the CDS-only
    /// marker.
    #[test]
    fn backend_scan_detects_marker_in_cds_template_only() {
        let upstream_main = "  path: {{CDS_PATH}}\n  port_value: {{PORT}}\n  admin: {{ADMIN_PORT}}";
        let subject_main = "  path: {{CDS_PATH}}\n  port_value: {{PORT}}\n  admin: {{ADMIN_PORT}}";
        let cds = r#"
resources:
  - "@type": type.googleapis.com/envoy.config.cluster.v3.Cluster
    load_assignment:
      endpoints:
        - lb_endpoints:
            - endpoint:
                address:
                  socket_address:
                    address: {{BACKEND_HOST}}
                    port_value: {{HTTP1_BACKEND_PORT}}
"#;
        // Mirrors run_fixture's combined `backend_scan_sources`.
        let sources: [&str; 3] = [upstream_main, subject_main, cds];

        // No backend marker in the main configs alone.
        assert!(
            !scan_needs_marker(&[upstream_main, subject_main], "HTTP1_BACKEND_PORT"),
            "main templates alone must not report an http1 backend need",
        );
        // The combined scan (incl. CDS) must report the need.
        assert!(
            scan_needs_marker(&sources, "HTTP1_BACKEND_PORT"),
            "combined scan (main + cds.yaml) must detect the CDS-only HTTP1_BACKEND_PORT marker",
        );
        // The CDS-only BACKEND_HOST presence is likewise visible (so the
        // host.docker.internal-vs-127.0.0.1 gate is reachable once the backend
        // port lands in the kv map).
        assert!(
            scan_needs_marker(&sources, "BACKEND_HOST"),
            "combined scan must detect the CDS-only BACKEND_HOST marker",
        );
        // Empty CDS source (the no-cds.yaml case) collapses to main-only.
        assert!(
            !scan_needs_marker(&[upstream_main, subject_main, ""], "HTTP1_BACKEND_PORT"),
            "empty cds source must not fabricate a backend need",
        );
    }

    /// 18 Task 11 (ADR-0015, ADR-0049): the host-gateway decision must scan the
    /// rendered upstream CDS file too, not just the main config. Fixture 0026's
    /// main config has zero static clusters — the backend endpoint (and the only
    /// `host.docker.internal` reference) lives in cds.yaml. Scanning only the
    /// main config (the pre-fix behaviour) returned `false`, so on Linux CI the
    /// container never got the `--add-host` host-gateway mapping and the route
    /// 503'd; macOS Docker Desktop resolves the hostname natively, hiding the
    /// gap locally. `uses_host_gateway` is the testable extraction of the
    /// `host_uses_host_gateway` decision in `run_fixture`.
    #[test]
    fn host_gateway_detected_in_cds_only() {
        // The CDS-only case: marker lives ONLY in the rendered CDS string, not
        // the main config. This is the fixture-0026 regression the fix targets.
        assert!(
            uses_host_gateway(&["no marker here", "address: host.docker.internal"]),
            "host.docker.internal in the CDS file alone must drive the host-gateway mapping",
        );
        // No CDS file and no marker in main → no mapping (fixtures 0001/0002).
        assert!(
            !uses_host_gateway(&["no marker"]),
            "absent hostname must leave the mapping off",
        );
        // The original main-config path stays true (no regression).
        assert!(
            uses_host_gateway(&["host.docker.internal"]),
            "host.docker.internal in the main config must still drive the mapping",
        );
        // A CDS file present but without the marker must not fabricate a need.
        assert!(
            !uses_host_gateway(&["no marker", "address: 127.0.0.1"]),
            "a CDS file without the hostname must not turn the mapping on",
        );
    }

    /// 18 Task 6 (ADR-0049): `render_yaml` leaves an unmatched `{{MARKER}}`
    /// token in place, so a CDS rendition with a marker absent from the kv map
    /// would otherwise slip into Envoy and fail with an opaque parse error. The
    /// `residual_marker` guard (applied to each CDS rendition in `run_fixture`)
    /// detects the leftover token and names it. A fully-resolved rendition
    /// returns `None`.
    #[test]
    fn residual_marker_names_unsubstituted_cds_token() {
        // A backend port marker with no kv entry → render_yaml leaves it; the
        // guard reports the offending marker name.
        let cds_template = "    address: {{BACKEND_HOST}}\n    port_value: {{HTTP1_BACKEND_PORT}}";
        // Only BACKEND_HOST resolved; HTTP1_BACKEND_PORT has no kv entry.
        let rendered = render_yaml(cds_template, &[("BACKEND_HOST", "127.0.0.1")]);
        assert_eq!(
            residual_marker(&rendered),
            Some("HTTP1_BACKEND_PORT"),
            "guard must name the unsubstituted marker, got: {rendered}",
        );

        // A fully-resolved rendition has no residual marker.
        let resolved = render_yaml(
            cds_template,
            &[
                ("BACKEND_HOST", "127.0.0.1"),
                ("HTTP1_BACKEND_PORT", "31415"),
            ],
        );
        assert_eq!(
            residual_marker(&resolved),
            None,
            "fully-resolved rendition must have no residual marker, got: {resolved}",
        );
    }

    /// 18 Task 6 (ADR-0049 L1): the upstream-side `{{CDS_PATH}}` substitution
    /// value is the container constant `upstream::CDS_CONTAINER_PATH`, which
    /// MUST end in `.yaml` (Envoy selects its config parser by file extension;
    /// a non-`.yaml` path would make it parse the YAML content as JSON-only and
    /// fail). The subject-side value is a host temp path to the subject's
    /// rendered cds file. This locks the L1 extension constraint structurally:
    /// the container path is a compile-time constant.
    #[test]
    fn cds_path_substitution_is_per_side_and_container_path_is_yaml() {
        // L1: the container-perspective path is a constant ending in `.yaml`.
        assert!(
            upstream::CDS_CONTAINER_PATH.ends_with(".yaml"),
            "L1: upstream container CDS path must end in .yaml, got {}",
            upstream::CDS_CONTAINER_PATH,
        );

        let main_template = "  path: {{CDS_PATH}}";
        // Upstream side: {{CDS_PATH}} → the container constant.
        let upstream_main =
            render_yaml(main_template, &[("CDS_PATH", upstream::CDS_CONTAINER_PATH)]);
        assert_eq!(
            upstream_main,
            format!("  path: {}", upstream::CDS_CONTAINER_PATH),
        );
        assert!(
            upstream_main.trim_end().ends_with(".yaml"),
            "upstream rendered CDS path must end in .yaml: {upstream_main}",
        );

        // Subject side: {{CDS_PATH}} → a host temp path to cds-subject.yaml.
        let tmp = tempfile::tempdir().unwrap();
        let subject_cds_path = tmp.path().join("cds-subject.yaml");
        let subject_cds_path_str = subject_cds_path.to_string_lossy().into_owned();
        let subject_main = render_yaml(main_template, &[("CDS_PATH", &subject_cds_path_str)]);
        assert_eq!(subject_main, format!("  path: {subject_cds_path_str}"));
        assert_ne!(
            subject_cds_path_str,
            upstream::CDS_CONTAINER_PATH,
            "subject CDS path must be a host temp path, not the container constant",
        );
    }

    /// 20 Task 6 (ADR-0052 L1): the upstream-side `{{RDS_PATH}}` substitution
    /// value is the container constant `upstream::RDS_CONTAINER_PATH`, which
    /// MUST end in `.yaml` (Envoy selects its config parser by file extension).
    /// The subject-side value is a host temp path to the subject's rendered RDS
    /// file. Mirrors the CDS render-path test above — RDS, like CDS, uses ONE
    /// SHARED `rds.yaml` rendered per-side.
    #[test]
    fn rds_path_substitution_is_per_side_and_container_path_is_yaml() {
        // L1: the container-perspective path is a constant ending in `.yaml`.
        assert!(
            upstream::RDS_CONTAINER_PATH.ends_with(".yaml"),
            "L1: upstream container RDS path must end in .yaml, got {}",
            upstream::RDS_CONTAINER_PATH,
        );

        let main_template = "  path: {{RDS_PATH}}";
        // Upstream side: {{RDS_PATH}} → the container constant.
        let upstream_main =
            render_yaml(main_template, &[("RDS_PATH", upstream::RDS_CONTAINER_PATH)]);
        assert_eq!(
            upstream_main,
            format!("  path: {}", upstream::RDS_CONTAINER_PATH),
        );
        assert!(
            upstream_main.trim_end().ends_with(".yaml"),
            "upstream rendered RDS path must end in .yaml: {upstream_main}",
        );

        // Subject side: {{RDS_PATH}} → a host temp path to rds-subject.yaml.
        let tmp = tempfile::tempdir().unwrap();
        let subject_rds_path = tmp.path().join("rds-subject.yaml");
        let subject_rds_path_str = subject_rds_path.to_string_lossy().into_owned();
        let subject_main = render_yaml(main_template, &[("RDS_PATH", &subject_rds_path_str)]);
        assert_eq!(subject_main, format!("  path: {subject_rds_path_str}"));
        assert_ne!(
            subject_rds_path_str,
            upstream::RDS_CONTAINER_PATH,
            "subject RDS path must be a host temp path, not the container constant",
        );
    }

    // ---- 26 Task 7: file-based RDS hot-reload step (local unit tests) ----
    // The Docker-gated end-to-end reload differential is native-Linux-CI
    // authoritative (Task 8's fixture); here we lock in the schema + the
    // locally-testable helpers (`atomic_rename_over`, per-side reload render).

    /// 26 Task 7: a `Driver::Http1RdsReload` expectations YAML round-trips
    /// through the snake_case-tagged serde representation. The `reload.reload_file`
    /// key is OMITTED on purpose to exercise the `default_reload_file` default
    /// (`rds-reload.yaml`); `pre_probes`, `post_probes`, and the discriminator
    /// probe parse into the expected `RdsReloadStep`. Mirrors
    /// `driver_http1_keep_alive_round_trips_through_serde`.
    #[test]
    fn driver_http1_rds_reload_round_trips_through_serde() {
        let yaml = r#"
driver:
  kind: http1_rds_reload
  pre_probes:
    - name: pre-route
      method: get
      path: /v1
      host: backend_cluster
      expected_status: 200
  reload:
    settle_budget_ms: 5000
    discriminator:
      name: discriminator
      method: get
      path: /v2
      host: backend_cluster
      expected_status: 200
  post_probes:
    - name: post-route
      method: get
      path: /v2
      host: backend_cluster
      expected_status: 200
"#;
        let exp: crate::Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
        let Driver::Http1RdsReload {
            pre_probes,
            reload,
            post_probes,
        } = exp.driver
        else {
            panic!("expected Driver::Http1RdsReload");
        };
        // reload_file omitted ⇒ default applied.
        assert_eq!(reload.reload_file, "rds-reload.yaml");
        assert_eq!(reload.settle_budget_ms, 5000);
        assert_eq!(reload.discriminator.name, "discriminator");
        assert_eq!(reload.discriminator.path, "/v2");
        assert_eq!(reload.discriminator.expected_status, Some(200));
        assert_eq!(pre_probes.len(), 1);
        assert_eq!(pre_probes[0].name, "pre-route");
        assert_eq!(pre_probes[0].path, "/v1");
        assert_eq!(post_probes.len(), 1);
        assert_eq!(post_probes[0].name, "post-route");
        assert_eq!(post_probes[0].path, "/v2");
    }

    /// 28 Task 7 (ADR-0070): a `Driver::Http1HashSweep` expectations YAML
    /// round-trips through the serde grammar — lock in the schema (mirroring the
    /// RDS/EDS round-trip tests). The fixture-0036 wire shape: a `keys:` list,
    /// `path`, `host`, and `expected_status`.
    #[test]
    fn driver_http1_hash_sweep_round_trips_through_serde() {
        let yaml = r#"
driver:
  kind: http1_hash_sweep
  path: /
  host: ring_cluster
  expected_status: 200
  keys:
    - key-0
    - key-2
    - user-alice
    - 1.2.3.4
"#;
        let exp: crate::Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
        let Driver::Http1HashSweep {
            keys,
            path,
            host,
            expected_status,
        } = exp.driver
        else {
            panic!("expected Driver::Http1HashSweep");
        };
        assert_eq!(path, "/");
        assert_eq!(host, "ring_cluster");
        assert_eq!(expected_status, 200);
        assert_eq!(keys, vec!["key-0", "key-2", "user-alice", "1.2.3.4"]);
    }

    /// 26 Task 7: `atomic_rename_over` swaps new content over the target via a
    /// same-dir temp sibling + `rename` — the ONLY rewrite that triggers Envoy's
    /// default file-watch (§6.2/ADR-0066). Asserts the post-swap content AND that
    /// no leftover sibling temp file remains (exactly one dir entry afterward —
    /// the same-dir invariant that keeps the rename a same-filesystem atomic swap).
    #[test]
    fn atomic_rename_over_swaps_content_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rds-subject.yaml");
        std::fs::write(&target, "A").unwrap();

        atomic_rename_over(&target, "B").expect("atomic rename succeeds");

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "B");
        // No leftover sibling temp file — exactly the target remains.
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "expected only the target file to remain, found {entries:?}",
        );
        assert_eq!(entries[0], target.file_name().unwrap());
    }

    /// 26 Task 7: the POST-reload RDS template renders per-side exactly like
    /// `rds.yaml` — the upstream (container-perspective) kv map resolves
    /// `{{BACKEND_HOST}}` to `host.docker.internal`, the subject
    /// (host-perspective) kv map to `127.0.0.1`, yielding per-side-distinct
    /// output. Mirrors `rds_path_substitution_is_per_side_and_container_path_is_yaml`.
    #[test]
    fn rds_reload_template_renders_per_side() {
        let reload_template = "  cluster: {{BACKEND_HOST}}";
        let upstream_reload =
            render_yaml(reload_template, &[("BACKEND_HOST", "host.docker.internal")]);
        let subject_reload = render_yaml(reload_template, &[("BACKEND_HOST", "127.0.0.1")]);
        assert_eq!(upstream_reload, "  cluster: host.docker.internal");
        assert_eq!(subject_reload, "  cluster: 127.0.0.1");
        assert_ne!(
            upstream_reload, subject_reload,
            "reload template must render per-side-distinct output",
        );
    }

    // ---- 19 Task 6 (ADR-0050): per-side LDS-template render-path tests ----
    // These mirror the phase-18 CDS render-path tests above. The LDS file is
    // PER-SIDE (`lds-envoy.yaml` upstream + `lds-envoy-rust.yaml` subject)
    // because the LDS payload carries the HCM whose Envoy-only fields
    // (`generate_request_id`, `request_headers_to_remove`) the envoy-rust
    // parser rejects — the two sides need different LDS files (mirroring the
    // `envoy.yaml`/`envoy-rust.yaml` main-config split). Each side's LDS file
    // is rendered through that side's kv map (container-perspective vs
    // host-perspective backend host/port). The Docker-gated end-to-end proof
    // is fixture 0027's job (Task 7).

    /// 19 Task 6 (a): the per-side LDS templates render through their side's kv
    /// map. The upstream (container-perspective) side resolves
    /// `{{BACKEND_HOST}}` to `host.docker.internal`; the subject
    /// (host-perspective) side resolves it to `127.0.0.1`. Because the LDS
    /// files are per-side, the two templates may also differ in body — here
    /// the upstream one carries the Envoy-only `generate_request_id` field that
    /// the envoy-rust template omits, proving the split is honoured.
    #[test]
    fn render_lds_template_substitutes_backend_host_per_side() {
        let upstream_lds = r#"
resources:
  - "@type": type.googleapis.com/envoy.config.listener.v3.Listener
    name: dynamic_listener
    filter_chains:
      - filters:
          - name: envoy.filters.network.http_connection_manager
            typed_config:
              generate_request_id: true
              route_config:
                virtual_hosts:
                  - routes:
                      - route: { host_rewrite_literal: {{BACKEND_HOST}}, port: {{HTTP1_BACKEND_PORT}} }
"#;
        let subject_lds = r#"
resources:
  - "@type": type.googleapis.com/envoy.config.listener.v3.Listener
    name: dynamic_listener
    filter_chains:
      - filters:
          - name: envoy.filters.network.http_connection_manager
            typed_config:
              route_config:
                virtual_hosts:
                  - routes:
                      - route: { host_rewrite_literal: {{BACKEND_HOST}}, port: {{HTTP1_BACKEND_PORT}} }
"#;
        let rendered_up = render_yaml(
            upstream_lds,
            &[
                ("BACKEND_HOST", "host.docker.internal"),
                ("HTTP1_BACKEND_PORT", "31415"),
            ],
        );
        let rendered_subject = render_yaml(
            subject_lds,
            &[
                ("BACKEND_HOST", "127.0.0.1"),
                ("HTTP1_BACKEND_PORT", "31415"),
            ],
        );
        assert!(
            rendered_up.contains("host_rewrite_literal: host.docker.internal"),
            "upstream lds should use container-perspective backend host: {rendered_up}",
        );
        assert!(
            rendered_subject.contains("host_rewrite_literal: 127.0.0.1"),
            "subject lds should use host-perspective backend host: {rendered_subject}",
        );
        // Both resolve the shared backend port marker.
        assert!(rendered_up.contains("port: 31415"));
        assert!(rendered_subject.contains("port: 31415"));
        // Per-side split: the Envoy-only field lives only in the upstream
        // rendition.
        assert!(
            rendered_up.contains("generate_request_id"),
            "upstream lds carries the Envoy-only HCM field",
        );
        assert!(
            !rendered_subject.contains("generate_request_id"),
            "subject lds omits the Envoy-only HCM field",
        );
    }

    /// 19 Task 6 (b): the upstream-side `{{LDS_PATH}}` substitution value is the
    /// container constant `upstream::LDS_CONTAINER_PATH`, which MUST end in
    /// `.yaml` (the L1 extension constraint — Envoy selects its config parser by
    /// file extension). The subject-side value is a host temp path to the
    /// subject's rendered LDS file. Locks the L1 constraint structurally: the
    /// container path is a compile-time constant.
    #[test]
    fn lds_path_substitution_is_per_side_and_container_path_is_yaml() {
        assert!(
            upstream::LDS_CONTAINER_PATH.ends_with(".yaml"),
            "L1: upstream container LDS path must end in .yaml, got {}",
            upstream::LDS_CONTAINER_PATH,
        );

        let main_template = "  path: {{LDS_PATH}}";
        // Upstream side: {{LDS_PATH}} → the container constant.
        let upstream_main =
            render_yaml(main_template, &[("LDS_PATH", upstream::LDS_CONTAINER_PATH)]);
        assert_eq!(
            upstream_main,
            format!("  path: {}", upstream::LDS_CONTAINER_PATH),
        );
        assert!(
            upstream_main.trim_end().ends_with(".yaml"),
            "upstream rendered LDS path must end in .yaml: {upstream_main}",
        );

        // Subject side: {{LDS_PATH}} → a host temp path to lds-subject.yaml.
        let tmp = tempfile::tempdir().unwrap();
        let subject_lds_path = tmp.path().join("lds-subject.yaml");
        let subject_lds_path_str = subject_lds_path.to_string_lossy().into_owned();
        let subject_main = render_yaml(main_template, &[("LDS_PATH", &subject_lds_path_str)]);
        assert_eq!(subject_main, format!("  path: {subject_lds_path_str}"));
        assert_ne!(
            subject_lds_path_str,
            upstream::LDS_CONTAINER_PATH,
            "subject LDS path must be a host temp path, not the container constant",
        );
    }

    /// 19 Task 6 (c): the `residual_marker` guard names an unsubstituted
    /// `{{MARKER}}` left inside a rendered LDS file. `render_yaml` leaves an
    /// unmatched token in place, so an LDS rendition with a marker absent from
    /// the kv map would otherwise slip into Envoy and fail with an opaque parse
    /// error. A fully-resolved rendition returns `None`.
    #[test]
    fn residual_marker_names_unsubstituted_lds_token() {
        let lds_template =
            "    host_rewrite_literal: {{BACKEND_HOST}}\n    port: {{HTTP1_BACKEND_PORT}}";
        // Only BACKEND_HOST resolved; HTTP1_BACKEND_PORT has no kv entry.
        let rendered = render_yaml(lds_template, &[("BACKEND_HOST", "127.0.0.1")]);
        assert_eq!(
            residual_marker(&rendered),
            Some("HTTP1_BACKEND_PORT"),
            "guard must name the unsubstituted LDS marker, got: {rendered}",
        );

        let resolved = render_yaml(
            lds_template,
            &[
                ("BACKEND_HOST", "127.0.0.1"),
                ("HTTP1_BACKEND_PORT", "31415"),
            ],
        );
        assert_eq!(
            residual_marker(&resolved),
            None,
            "fully-resolved LDS rendition must have no residual marker, got: {resolved}",
        );
    }

    /// 19 Task 6 (d) — THE regression guard for the phase-18 escaped-to-CI
    /// Critical (carryforward-disposition-2): the combined-source scans must
    /// cover the rendered LDS file too. Fixture 0027 places its
    /// `{{HTTP1_BACKEND_PORT}}` and `host.docker.internal` references ONLY in
    /// the LDS file (the main configs route to a CDS cluster and carry no
    /// backend markers). A scan over only main + CDS would report
    /// `needs_http1_backend == false` and `uses_host_gateway == false` — the
    /// backend never spawns and on Linux CI the container never gets the
    /// `--add-host` mapping, 503-ing the route. The fix adds the LDS rendition
    /// as a fourth scan source AND generalizes `uses_host_gateway` to a slice.
    #[test]
    fn backend_and_host_gateway_scans_detect_lds_only_markers() {
        let upstream_main = "  cds_path: {{CDS_PATH}}\n  lds_path: {{LDS_PATH}}\n  port: {{PORT}}";
        let subject_main = "  cds_path: {{CDS_PATH}}\n  lds_path: {{LDS_PATH}}\n  port: {{PORT}}";
        // CDS file: no backend marker, no host.docker.internal (the endpoint
        // lives in the LDS-carried route in this fixture shape).
        let cds = "resources:\n  - name: dynamic_backend\n";
        // LDS file: the ONLY site of the backend marker and the gateway host.
        let lds_up = r#"
resources:
  - "@type": type.googleapis.com/envoy.config.listener.v3.Listener
    route:
      host: host.docker.internal
      port: {{HTTP1_BACKEND_PORT}}
"#;

        // The combined backend-scan source as `run_fixture` builds it: main x2
        // + CDS + LDS (4 sources).
        let backend_scan_sources: [&str; 4] = [upstream_main, subject_main, cds, lds_up];

        // Main + CDS alone (the pre-LDS-extension scan) must NOT see the
        // backend need — proving the test genuinely fails without the LDS
        // source.
        assert!(
            !scan_needs_marker(&[upstream_main, subject_main, cds], "HTTP1_BACKEND_PORT"),
            "main + CDS alone must not report the backend need (the bug-class baseline)",
        );
        // The 4-source scan (incl. LDS) must report the need.
        assert!(
            scan_needs_marker(&backend_scan_sources, "HTTP1_BACKEND_PORT"),
            "combined scan (main + CDS + LDS) must detect the LDS-only HTTP1_BACKEND_PORT marker",
        );

        // host-gateway: the slice-based signature must see the LDS-only
        // hostname. Main + CDS alone return false (the regression baseline).
        assert!(
            !uses_host_gateway(&[upstream_main, cds]),
            "main + CDS alone must not drive the host-gateway mapping (the bug-class baseline)",
        );
        assert!(
            uses_host_gateway(&[upstream_main, cds, lds_up]),
            "host.docker.internal in the LDS file alone must drive the host-gateway mapping",
        );
    }

    /// 18 Task 6 (ADR-0049): `Driver::Http1KeepAlive` with an `admin_scrapes:`
    /// list deserializes from YAML (the fixture-0026 shape — a `/config_dump`
    /// json_shape sub-case after the keep-alive request + stat scrape).
    #[test]
    fn http1_keep_alive_with_admin_scrapes_round_trips() {
        let yaml = r#"
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /
      host: dynamic_backend
      expected_status: 200
  settle_ms: 100
  admin_scrapes:
    - path: /config_dump
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
"#;
        let exp: crate::Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
        let Driver::Http1KeepAlive {
            requests,
            settle_ms,
            expected_stats,
            admin_scrapes,
        } = exp.driver
        else {
            panic!("expected Driver::Http1KeepAlive");
        };
        assert_eq!(requests.len(), 1);
        assert_eq!(settle_ms, 100);
        assert!(expected_stats.is_empty());
        assert_eq!(admin_scrapes.len(), 1);
        assert_eq!(admin_scrapes[0].path, "/config_dump");
        assert_eq!(admin_scrapes[0].expected_status, 200);
        assert_eq!(admin_scrapes[0].expected_content_type, "application/json");
        assert!(matches!(
            admin_scrapes[0].expected_body_rule,
            BodyRule::JsonShape { .. }
        ));
    }

    /// 18 Task 6 (ADR-0049): an existing-style keep-alive `Driver` block that
    /// omits the `admin_scrapes` key (the shape fixtures 0020-0025 use) still
    /// deserializes, with `admin_scrapes` defaulting to an empty vec. Guards
    /// the `#[serde(default)]` backward-compat contract.
    #[test]
    fn http1_keep_alive_without_admin_scrapes_defaults_empty() {
        let yaml = r#"
driver:
  kind: http1_keep_alive
  requests:
    - method: GET
      path: /
      host: backend_cluster
      expected_status: 200
  settle_ms: 100
  expected_stats:
    - name: cluster.backend_cluster.upstream_cx_total
      value: 1
"#;
        let exp: crate::Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
        let Driver::Http1KeepAlive { admin_scrapes, .. } = exp.driver else {
            panic!("expected Driver::Http1KeepAlive");
        };
        assert!(
            admin_scrapes.is_empty(),
            "admin_scrapes must default to empty when the key is absent",
        );
    }

    /// 13.2 Task 5 (ADR-0039): `Driver::Http2KeepAlive` round-trips through
    /// the snake_case-tagged serde representation. Asserts the new variant
    /// reuses `Http1KeepAliveRequest` + `KeepAliveExpectedStat` verbatim
    /// (the codec-agnostic substructs — same precedent as `Http2ProbeList`
    /// reusing `Http1Probe` at 11 D8.1). The kind tag is
    /// `http2_keep_alive` per the `Driver` enum's
    /// `#[serde(tag = "kind", rename_all = "snake_case")]` attribute.
    #[test]
    fn driver_http2_keep_alive_round_trips_through_serde() {
        let yaml = r#"
driver:
  kind: http2_keep_alive
  requests:
    - method: GET
      path: /
      host: backend_cluster
      expected_status: 200
  settle_ms: 500
  expected_stats:
    - name: cluster.backend_cluster.upstream_cx_total
      value: 1
    - name: cluster.backend_cluster.upstream_cx_http2_total
      value: 1
"#;
        let exp: crate::Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
        let Driver::Http2KeepAlive {
            requests,
            settle_ms,
            expected_stats,
        } = exp.driver
        else {
            panic!("expected Driver::Http2KeepAlive");
        };
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].path, "/");
        assert_eq!(requests[0].host, "backend_cluster");
        assert_eq!(requests[0].expected_status, 200);
        assert_eq!(settle_ms, 500);
        assert_eq!(expected_stats.len(), 2);
        assert_eq!(
            expected_stats[0].name,
            "cluster.backend_cluster.upstream_cx_total"
        );
        assert_eq!(expected_stats[0].value, 1);
        assert_eq!(
            expected_stats[1].name,
            "cluster.backend_cluster.upstream_cx_http2_total"
        );
        assert_eq!(expected_stats[1].value, 1);
    }

    #[test]
    fn render_yaml_substitutes_all_port_tokens() {
        let t = "a: {{PORT}}\nb: {{PORT}}\n";
        assert_eq!(render_yaml(t, &[("PORT", "9000")]), "a: 9000\nb: 9000\n");
    }

    #[test]
    fn render_yaml_substitutes_admin_port_key() {
        let t = "address: 127.0.0.1\nport: {{ADMIN_PORT}}\n";
        assert_eq!(
            render_yaml(t, &[("ADMIN_PORT", "9901")]),
            "address: 127.0.0.1\nport: 9901\n"
        );
    }

    // Awareness-only: the full Docker-gated dispatch lands in Task 15's
    // fixture 0008. This test exercises only the harness's spawn-and-render
    // wiring per SPEC §6 signpost 11 (M11 carryforward shape surfaces).
    #[tokio::test(flavor = "multi_thread")]
    async fn run_fixture_dispatches_http1_backend_on_template_marker() {
        if crate::backend::locate_http1_echo_server().is_err() {
            eprintln!(
                "skipping run_fixture_dispatches_http1_backend_on_template_marker — http1-echo-server not built"
            );
            return;
        }
        let backend = crate::backend::Http1EchoBackend::spawn()
            .await
            .expect("spawn http1 backend");
        let port = backend.port();
        let port_str = port.to_string();
        let template = "endpoint: {{BACKEND_HOST}}:{{HTTP1_BACKEND_PORT}}";
        let kvs = &[
            ("BACKEND_HOST", "host.docker.internal"),
            ("HTTP1_BACKEND_PORT", port_str.as_str()),
        ];
        let rendered = render_yaml(template, kvs);
        assert_eq!(
            rendered,
            format!("endpoint: host.docker.internal:{port_str}"),
            "rendered: {rendered}"
        );
        drop(backend);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_fixture_dispatches_http2_backend_on_template_marker() {
        // Per SPEC §3 D6.b: run_fixture spawns Http2EchoBackend when either
        // upstream_template or subject_template contains {{HTTP2_BACKEND_PORT}}.
        // Test by passing a synthetic template through render_yaml directly
        // and asserting the substitution occurred (the spawn-side is exercised
        // by the dedicated http2_router_upstream Docker-gated test at Task 10).
        let template = "endpoint: {{BACKEND_HOST}}:{{HTTP2_BACKEND_PORT}}";
        let port_str = "7000";
        let kvs: Vec<(&str, &str)> = vec![
            ("HTTP2_BACKEND_PORT", port_str),
            ("BACKEND_HOST", "host.docker.internal"),
        ];
        let rendered = render_yaml(template, &kvs);
        assert!(
            rendered.contains("endpoint: host.docker.internal:7000"),
            "expected substitution; got: {rendered}"
        );
    }

    #[test]
    fn reserve_port_returns_nonzero() {
        let p = reserve_port().unwrap();
        assert!(p > 0);
    }

    #[test]
    fn reserve_port_with_skips_port_already_handed_out() {
        // Simulate the kernel returning the same ephemeral port twice in a row
        // (CI run 26861955222: data + admin listener both got 40875).
        let mut calls = 0u32;
        let first = reserve_port_with(|| {
            calls += 1;
            Ok(61001)
        })
        .unwrap();
        assert_eq!(first, 61001);
        let second = reserve_port_with(|| {
            calls += 1;
            // Kernel hands back 61001 again; helper must reject it and retry.
            Ok(if calls <= 2 { 61001 } else { 61002 })
        })
        .unwrap();
        assert_eq!(second, 61002);
    }

    #[tokio::test]
    async fn wait_accept_ready_succeeds_for_listening_socket() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        wait_accept_ready(addr, Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wait_accept_ready_times_out_for_closed_socket() {
        let listener = StdTcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);
        let result = wait_accept_ready(addr, Duration::from_millis(200)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn wait_file_nonempty_true_for_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("log");
        std::fs::write(&p, "line\n").unwrap();
        assert!(wait_file_nonempty(&p, Duration::from_millis(200)).await);
    }

    #[tokio::test]
    async fn wait_file_nonempty_false_when_budget_expires() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("log");
        std::fs::write(&p, "").unwrap();
        assert!(!wait_file_nonempty(&p, Duration::from_millis(200)).await);
    }

    #[tokio::test]
    async fn wait_file_lines_true_when_count_reached() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("log");
        std::fs::write(&p, "a\nb\nc\n").unwrap();
        assert!(wait_file_lines(&p, 3, Duration::from_millis(300)).await);
    }

    #[tokio::test]
    async fn wait_file_lines_false_when_count_unreached() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("log");
        std::fs::write(&p, "a\nb\nc\n").unwrap();
        assert!(!wait_file_lines(&p, 4, Duration::from_millis(200)).await);
    }

    #[tokio::test]
    async fn wait_file_nonempty_true_when_content_arrives_mid_poll() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("log");
        std::fs::write(&p, "").unwrap();
        let p2 = p.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            std::fs::write(&p2, "line\n").unwrap();
        });
        assert!(wait_file_nonempty(&p, Duration::from_secs(10)).await);
    }

    // Mirrors upstream Envoy v1.33.0's echo filter semantics per ADR-0006: the
    // server accepts one connection, reads `payload.len()` bytes, echoes them
    // back, and closes WITHOUT ever honoring a client half-close. A harness
    // that half-closed before reading (the pre-ADR-0006 `drive_tcp`) would
    // race against this close and see an empty response.
    #[tokio::test]
    async fn drive_tcp_round_trips_without_half_close() {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let payload: &'static [u8] = b"hello, envoy-rust\n";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; payload.len()];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            // Drop without waiting for a client FIN — this is what upstream
            // Envoy's echo path does once it has written the response.
            drop(stream);
        });

        let echoed = drive_tcp(addr, payload).await.unwrap();
        assert_eq!(echoed, payload);
        server.await.unwrap();
    }

    #[test]
    fn expectations_parse_tcp_echo_driver() {
        let yaml = r#"
driver:
  kind: tcp_echo
equivalence:
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }

    #[test]
    fn expectations_parse_http_get_driver() {
        let yaml = r#"
driver:
  kind: http_get
  path: /ready
  host: envoy-rust-phase-01
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::HttpGet { path, host } => {
                assert_eq!(path, "/ready");
                assert_eq!(host, "envoy-rust-phase-01");
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
        assert_eq!(e.equivalence.response_status, Some(StatusRule::Exact));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    }

    // RED for Task 8: parses `kind: tls_tcp_probe_list` with a `probes:`
    // sequence whose entries are `{sni, expected_cn?}` maps.
    #[test]
    fn expectations_parse_tls_tcp_probe_list_driver() {
        let yaml = r#"
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
      expected_cn: a.example.com
    - sni: b.example.com
      expected_cn: b.example.com
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcpProbeList { ref probes } => {
                assert_eq!(probes.len(), 2);
                assert_eq!(probes[0].sni, "a.example.com");
                assert_eq!(probes[0].expected_cn.as_deref(), Some("a.example.com"));
                assert_eq!(probes[1].sni, "b.example.com");
                assert_eq!(probes[1].expected_cn.as_deref(), Some("b.example.com"));
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    // RED for Task 8: `expected_cn` is `#[serde(default)]` so it may be absent.
    #[test]
    fn expectations_parse_tls_tcp_probe_list_without_expected_cn() {
        let yaml = r#"
driver:
  kind: tls_tcp_probe_list
  probes:
    - sni: a.example.com
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        match e.driver {
            Driver::TlsTcpProbeList { ref probes } => {
                assert_eq!(probes.len(), 1);
                assert_eq!(probes[0].sni, "a.example.com");
                assert!(probes[0].expected_cn.is_none());
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
    }

    #[test]
    fn expectations_reject_unknown_driver_kind() {
        let yaml = r#"
driver:
  kind: quantum_bogon
equivalence:
  response_body:
    kind: byte_exact
"#;
        let r: Result<Expectations, _> = serde_yaml::from_str(yaml);
        assert!(r.is_err(), "quantum_bogon must not parse: {r:?}");
    }

    // Regression for REVIEW.md I1 (ADR-0007): a server that writes
    // `payload.len()` bytes and then additional trailing bytes must cause
    // `drive_tcp` to fail the fixture. Before ADR-0007's trailing-byte check,
    // `drive_tcp` silently consumed only the first `payload.len()` bytes and
    // returned Ok, narrowing BEHAVIOR_CONTRACT row 2's "byte-exact" contract
    // to "first N bytes match."
    #[tokio::test]
    async fn drive_tcp_rejects_trailing_bytes_after_echo() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let payload: &'static [u8] = b"hello, envoy-rust\n";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; payload.len()];
            stream.read_exact(&mut buf).await.unwrap();
            stream.write_all(&buf).await.unwrap();
            // Write extra trailing bytes beyond the echoed payload. A pre-
            // ADR-0007 `drive_tcp` would not notice these.
            stream.write_all(b"EXTRA").await.unwrap();
            // Hold the stream open long enough that the harness's trailing-
            // byte poll deadline sees the bytes rather than an early EOF.
            tokio::time::sleep(Duration::from_millis(250)).await;
            drop(stream);
        });

        let err = drive_tcp(addr, payload)
            .await
            .expect_err("drive_tcp must fail when the peer writes trailing bytes");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("trailing bytes"),
            "unexpected error message: {msg}",
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn drive_http_get_round_trips() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read the request (we don't parse — just drain until CRLFCRLF).
            let mut buf = [0u8; 512];
            let mut read = Vec::new();
            loop {
                let n = stream.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                read.extend_from_slice(&buf[..n]);
                if read.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nLIVE\n",
                )
                .await
                .unwrap();
            drop(stream);
        });

        let resp = drive_http_get(addr, "/ready", "x").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"LIVE\n");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn drive_http_get_handles_explicit_content_length() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let _ = tokio::io::copy(
                &mut tokio::io::empty(),
                &mut tokio::io::BufWriter::new(&mut s),
            )
            .await;
            s.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 4\r\n\r\nNOPE")
                .await
                .unwrap();
            // Hold open long enough for the client to read_exact the 4 bytes.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            drop(s);
        });

        let resp = drive_http_get(addr, "/x", "h").await.unwrap();
        assert_eq!(resp.status, 404);
        assert_eq!(resp.body, b"NOPE");
    }

    #[tokio::test]
    async fn drive_http_get_handles_connection_close_without_length() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            // Drain the incoming request so the receive buffer is empty before
            // we write and close. Without this, macOS sends RST instead of FIN
            // when dropping a TcpStream with unread data.
            let mut drain = [0u8; 512];
            loop {
                let n = s.read(&mut drain).await.unwrap();
                if n == 0 {
                    break;
                }
                if drain[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\nhello-close")
                .await
                .unwrap();
            s.shutdown().await.ok();
            drop(s);
        });

        let resp = drive_http_get(addr, "/x", "h").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"hello-close");
    }

    #[tokio::test]
    async fn drive_http_get_rejects_malformed_response() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            s.write_all(b"this is not a valid http response\r\n\r\n")
                .await
                .unwrap();
            drop(s);
        });

        let err = drive_http_get(addr, "/x", "h")
            .await
            .expect_err("malformed must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("parse") || msg.contains("invalid"),
            "got: {msg}"
        );
    }

    #[test]
    fn decode_chunked_empty_stream() {
        let decoded = super::decode_chunked(b"0\r\n\r\n").expect("empty stream decodes");
        assert!(decoded.is_empty(), "got {decoded:?}");
    }

    #[test]
    fn decode_chunked_with_chunk_extension() {
        let wire = b"5;name=value\r\nhello\r\n0\r\n\r\n";
        let decoded = super::decode_chunked(wire).expect("chunk extensions tolerated");
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn decode_chunked_truncated_size_line() {
        // No CRLF anywhere — the first `windows(2).position(== \r\n)` miss
        // must surface as Err, not silent Ok(partial).
        let err = super::decode_chunked(b"5hello").expect_err("must reject");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("missing CRLF"),
            "expected CRLF-missing error; got {msg}",
        );
    }

    #[test]
    fn decode_chunked_ignores_trailer_bytes() {
        let wire = b"3\r\nabc\r\n0\r\nTrailer-Name: value\r\n\r\n";
        let decoded = super::decode_chunked(wire).expect("trailer tolerated");
        assert_eq!(decoded, b"abc");
    }

    #[test]
    fn fixture_0001_expectations_parses_as_tcp_echo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0001-tcp-echo/expectations.yaml");
        let e = load_expectations(&path).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }

    #[test]
    fn fixture_0002_expectations_parses_as_http_get() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0002-static-admin-ready/expectations.yaml");
        let e = load_expectations(&path).expect("parses");
        match e.driver {
            Driver::HttpGet { ref path, ref host } => {
                assert_eq!(path, "/ready");
                assert_eq!(host, "envoy-rust-phase-01");
            }
            _ => panic!("unexpected driver: {:?}", e.driver),
        }
        assert_eq!(e.equivalence.response_status, Some(StatusRule::Exact));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
    }

    #[test]
    fn render_yaml_substitutes_backend_keys_for_envoy_side() {
        // Upstream-Envoy rendering: {{BACKEND_HOST}} → host.docker.internal,
        // {{BACKEND_PORT}} → harness-reserved port. {{PORT}} → the listener port.
        let template = r#"
listeners: [{{PORT}}]
endpoint: {{BACKEND_HOST}}:{{BACKEND_PORT}}
"#;
        let got = render_yaml(
            template,
            &[
                ("PORT", "10000"),
                ("BACKEND_HOST", "host.docker.internal"),
                ("BACKEND_PORT", "31415"),
            ],
        );
        assert!(
            got.contains("listeners: [10000]"),
            "PORT not substituted: {got}"
        );
        assert!(
            got.contains("endpoint: host.docker.internal:31415"),
            "BACKEND_{{HOST,PORT}} not substituted: {got}",
        );
    }

    #[test]
    fn render_yaml_substitutes_backend_keys_for_envoy_rust_side() {
        // envoy-rust-side rendering: {{BACKEND_HOST}} → 127.0.0.1.
        let template = r#"
listeners: [{{PORT}}]
endpoint: {{BACKEND_HOST}}:{{BACKEND_PORT}}
"#;
        let got = render_yaml(
            template,
            &[
                ("PORT", "20000"),
                ("BACKEND_HOST", "127.0.0.1"),
                ("BACKEND_PORT", "31415"),
            ],
        );
        assert!(
            got.contains("listeners: [20000]"),
            "PORT not substituted: {got}"
        );
        assert!(
            got.contains("endpoint: 127.0.0.1:31415"),
            "BACKEND_HOST not substituted to 127.0.0.1: {got}",
        );
    }

    #[test]
    fn render_yaml_substitutes_tls_paths_for_envoy_side() {
        let template = r#"
trusted_ca:
  filename: {{CA_PATH}}
leaf_cert:
  filename: {{LEAF_A_CERT_PATH}}
leaf_key:
  filename: {{LEAF_A_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/etc/envoy-rust-tls/ca.pem"),
                ("LEAF_A_CERT_PATH", "/etc/envoy-rust-tls/leaf-a-cert.pem"),
                ("LEAF_A_KEY_PATH", "/etc/envoy-rust-tls/leaf-a-key.pem"),
            ],
        );
        assert!(got.contains("filename: /etc/envoy-rust-tls/ca.pem"));
        assert!(got.contains("filename: /etc/envoy-rust-tls/leaf-a-cert.pem"));
        assert!(got.contains("filename: /etc/envoy-rust-tls/leaf-a-key.pem"));
    }

    #[test]
    fn render_yaml_substitutes_tls_paths_for_subject_side() {
        let template = r#"
trusted_ca:
  filename: {{CA_PATH}}
leaf_cert:
  filename: {{LEAF_A_CERT_PATH}}
leaf_key:
  filename: {{LEAF_A_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/tmp/abc/ca.pem"),
                ("LEAF_A_CERT_PATH", "/tmp/abc/leaf-a-cert.pem"),
                ("LEAF_A_KEY_PATH", "/tmp/abc/leaf-a-key.pem"),
            ],
        );
        assert!(got.contains("filename: /tmp/abc/ca.pem"));
        assert!(got.contains("filename: /tmp/abc/leaf-a-cert.pem"));
        assert!(got.contains("filename: /tmp/abc/leaf-a-key.pem"));
    }

    // 03.2 Task 8: render_yaml must substitute LEAF_B_* keys (used by
    // fixture 0006-tls-sni's second filter chain).
    #[test]
    fn render_yaml_substitutes_leaf_b_paths() {
        let template = r#"
chain_b_cert: {{LEAF_B_CERT_PATH}}
chain_b_key: {{LEAF_B_KEY_PATH}}
ca: {{CA_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("CA_PATH", "/etc/envoy-rust-tls/ca.pem"),
                ("LEAF_B_CERT_PATH", "/etc/envoy-rust-tls/leaf-b-cert.pem"),
                ("LEAF_B_KEY_PATH", "/etc/envoy-rust-tls/leaf-b-key.pem"),
            ],
        );
        assert!(got.contains("chain_b_cert: /etc/envoy-rust-tls/leaf-b-cert.pem"));
        assert!(got.contains("chain_b_key: /etc/envoy-rust-tls/leaf-b-key.pem"));
        assert!(got.contains("ca: /etc/envoy-rust-tls/ca.pem"));
        assert!(!got.contains("{{"));
    }

    // 03.2 Task 8: render_yaml must substitute SERVER_* keys (used by
    // fixture 0005-tls-upstream's TlsEchoBackend on the upstream cluster).
    #[test]
    fn render_yaml_substitutes_server_paths() {
        let template = r#"
server_cert: {{SERVER_CERT_PATH}}
server_key: {{SERVER_KEY_PATH}}
"#;
        let got = render_yaml(
            template,
            &[
                ("SERVER_CERT_PATH", "/etc/envoy-rust-tls/server-cert.pem"),
                ("SERVER_KEY_PATH", "/etc/envoy-rust-tls/server-key.pem"),
            ],
        );
        assert!(got.contains("server_cert: /etc/envoy-rust-tls/server-cert.pem"));
        assert!(got.contains("server_key: /etc/envoy-rust-tls/server-key.pem"));
        assert!(!got.contains("{{"));
    }

    #[test]
    fn fixture_0003_expectations_parses_as_tcp_echo() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests/fixtures/0003-tcp-proxy/expectations.yaml");
        if !path.exists() {
            eprintln!(
                "skipping: fixture 0003-tcp-proxy/expectations.yaml not yet landed (Task 12)"
            );
            return;
        }
        let e = load_expectations(&path).expect("parses");
        assert!(matches!(e.driver, Driver::TcpEcho));
        assert_eq!(e.equivalence.response_body, Some(BodyRule::ByteExact));
        assert_eq!(e.equivalence.response_status, None);
    }

    #[test]
    fn diff_headers_passes_set_equal_modulo_allow_list() {
        let envoy = vec![
            ("server".to_string(), "envoy".to_string()),
            (
                "date".to_string(),
                "Sun, 06 Nov 1994 08:49:37 GMT".to_string(),
            ),
            ("content-length".to_string(), "3".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
            ("connection".to_string(), "keep-alive".to_string()),
        ];
        let envoy_rust = vec![
            ("server".to_string(), "envoy-rust".to_string()),
            (
                "date".to_string(),
                "Mon, 07 Nov 1994 12:00:00 GMT".to_string(),
            ),
            ("content-length".to_string(), "3".to_string()),
            ("content-type".to_string(), "text/plain".to_string()),
            ("connection".to_string(), "keep-alive".to_string()),
        ];
        diff_headers(&envoy, &envoy_rust, HEADER_ALLOW_LIST).expect("server+date allow-listed");
    }

    #[test]
    fn diff_headers_fails_on_value_diff_outside_allow_list() {
        let envoy = vec![("content-length".to_string(), "3".to_string())];
        let envoy_rust = vec![("content-length".to_string(), "4".to_string())];
        let err = diff_headers(&envoy, &envoy_rust, HEADER_ALLOW_LIST)
            .expect_err("content-length value mismatch");
        assert!(err.to_string().contains("content-length"), "msg: {err}");
    }

    #[test]
    fn diff_headers_fails_on_name_set_diff() {
        let envoy = vec![
            ("x-foo".to_string(), "1".to_string()),
            ("date".to_string(), "...".to_string()),
        ];
        let envoy_rust = vec![("date".to_string(), "...".to_string())];
        let err = diff_headers(&envoy, &envoy_rust, HEADER_ALLOW_LIST)
            .expect_err("envoy emits x-foo, envoy-rust does not");
        assert!(err.to_string().contains("x-foo"), "msg: {err}");
    }

    #[test]
    fn parses_expectations_with_http1_probe_list() {
        // 04.2 NEW: Driver::Http1ProbeList shape parses round-trip from YAML.
        let yaml = r#"
driver:
  kind: http1_probe_list
  probes:
    - name: default-route
      method: get
      path: "/healthz"
      host: "envoy-rust.test"
      expected_status: 200
      expected_body:
        kind: byte_exact
        body: "ok\n"
      expected_headers: set_equal_modulo_allow_list
    - name: matcher-route
      method: get
      path: "/api/widgets"
      host: "envoy-rust.test"
      extra_headers:
        - ["X-Foo", "bar"]
      expected_status: 418
      expected_body:
        kind: byte_exact
        body: "teapot\n"
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::Http1ProbeList { probes } = e.driver else {
            panic!("expected Http1ProbeList");
        };
        assert_eq!(probes.len(), 2);
        assert_eq!(probes[0].name, "default-route");
        assert_eq!(probes[0].extra_headers.len(), 0);
        assert_eq!(probes[1].name, "matcher-route");
        assert_eq!(probes[1].extra_headers.len(), 1);
        assert_eq!(probes[1].extra_headers[0].0, "X-Foo");
        assert_eq!(probes[1].extra_headers[0].1, "bar");
    }

    #[test]
    fn http2_probe_list_round_trips_from_yaml() {
        // 11 NEW: Driver::Http2ProbeList shape parses round-trip from YAML.
        // Reuses the codec-agnostic Http1Probe struct directly.
        let yaml = r#"
driver:
  kind: http2_probe_list
  probes:
    - name: abort
      method: get
      path: /
      host: envoy-rust.test
      extra_headers:
        - [x-fault, abort]
      expected_status: 503
      expected_body: { kind: byte_exact, body: "fault filter abort" }
      expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body:
    kind: byte_exact
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::Http2ProbeList { probes } = e.driver else {
            panic!("expected Http2ProbeList");
        };
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0].name, "abort");
        assert_eq!(probes[0].expected_status, Some(503));
        assert_eq!(probes[0].extra_headers.len(), 1);
        assert_eq!(probes[0].extra_headers[0].0, "x-fault");
        assert_eq!(probes[0].extra_headers[0].1, "abort");
    }

    #[test]
    fn http1_probe_extra_headers_default_empty() {
        let yaml = r#"
name: simple
method: get
path: "/"
host: "x.test"
expected_status: 200
expected_body:
  kind: byte_exact
  body: ""
expected_headers: set_equal_modulo_allow_list
"#;
        let p: Http1Probe = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(p.extra_headers.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drive_http2_round_trip_against_in_process_listener() {
        // Spawn envoy-bin as a subprocess against an HCM HTTP2 direct_response
        // config; drive a GET via drive_http2; assert the returned tuple
        // matches expectations.
        //
        // Deviation from PLAN: the PLAN's `env!("CARGO_BIN_EXE_envoy-bin")`
        // does not work here — that env var is only set for integration tests
        // *of the package owning the binary*. The differential crate is
        // separate, so we use the existing `subject::locate_envoy_bin()`
        // helper (matching the convention in `subject::start`).
        if crate::subject::locate_envoy_bin().is_err() {
            eprintln!("skipping: envoy-bin not built");
            return;
        }
        let bin = crate::subject::locate_envoy_bin().unwrap();
        let port = reserve_port().unwrap();
        let yaml = format!(
            r#"
node: {{ id: x, cluster: y }}
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {port} }}
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
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
        );
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("envoy-rust.yaml");
        std::fs::write(&cfg, yaml).unwrap();

        let child = tokio::process::Command::new(&bin)
            .arg("-c")
            .arg(&cfg)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn envoy-bin");

        // Wait for listener readiness.
        let listener_addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if tokio::net::TcpStream::connect(listener_addr).await.is_ok() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                drop(child);
                panic!("listener never became ready");
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let result = drive_http2(listener_addr, &Http1Method::Get, "/", "test.example", &[]).await;
        drop(child);
        let result = result.expect("drive_http2 returns Ok");
        assert_eq!(result.status, 200);
        assert_eq!(&result.body[..], b"ok\n");
    }

    // 06.1 D6.c: round-trips `drive_admin_scrape` against a spawned
    // envoy-bin subprocess running an admin-only bootstrap. Validates the
    // helper's wire-shape end to end (no mocks): the scrape lands a real
    // GET on `/stats/prometheus`, parses the response, and we assert
    // status=200 + body parses as a Prometheus exposition (the metric set
    // may be empty when no HCM listeners feed the registry — the registry
    // emits no samples, which `parse_prometheus_metric_names` returns as
    // an empty BTreeSet without erroring). Pre-requests are empty here
    // since the subprocess has no HCM listener; the HCM-pre-request path
    // is exercised by fixture 0011 (Task 13).
    #[tokio::test(flavor = "multi_thread")]
    async fn drive_admin_scrape_round_trips_against_envoy_bin_admin() {
        if crate::subject::locate_envoy_bin().is_err() {
            eprintln!("skipping: envoy-bin not built");
            return;
        }
        let bin = crate::subject::locate_envoy_bin().unwrap();
        let admin_port = reserve_port().expect("reserve_port");
        let yaml = format!(
            r#"
node:
  id: backstop
  cluster: backstop
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: {admin_port}
static_resources:
  listeners: []
  clusters: []
"#
        );
        let dir = tempfile::tempdir().expect("tempdir");
        let cfg = dir.path().join("admin_only.yaml");
        std::fs::write(&cfg, yaml).expect("write yaml");

        let mut child = tokio::process::Command::new(&bin)
            .arg("-c")
            .arg(&cfg)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn envoy-bin");

        let admin_addr: SocketAddr = format!("127.0.0.1:{admin_port}").parse().unwrap();
        if let Err(err) = wait_accept_ready(admin_addr, Duration::from_secs(5)).await {
            let _ = child.kill().await;
            panic!("admin listener never accept-ready: {err}");
        }

        let hcm_addrs: std::collections::BTreeMap<String, SocketAddr> =
            std::collections::BTreeMap::new();
        let result = drive_admin_scrape(&[], admin_addr, &hcm_addrs, "/stats/prometheus").await;
        let _ = child.kill().await;
        let resp = result.expect("drive_admin_scrape returns Ok");
        assert_eq!(resp.status, 200, "headers: {:?}", resp.headers);

        // Body parses as Prometheus exposition. Empty registry → empty
        // metric-name set; the parser must return Ok(empty BTreeSet)
        // rather than panic.
        let names = parse_prometheus_metric_names(&resp.body);
        assert!(
            names.is_empty() || names.iter().all(|n| !n.contains(' ')),
            "metric names contain whitespace: {names:?}",
        );

        // Content-type is text/plain (our admin emits text-exposition).
        let ct = resp
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str())
            .expect("content-type header present");
        assert!(
            ct.starts_with("text/plain"),
            "unexpected content-type: {ct:?}",
        );
    }

    #[test]
    fn parse_prometheus_metric_names_handles_labels_and_comments() {
        // Mixed input: HELP/TYPE comments, blank lines, samples with and
        // without labels. Output must be the set of leading-token metric
        // names in BTreeSet (sorted) order.
        let body = b"# HELP foo total\n\
                     # TYPE foo counter\n\
                     foo 1\n\
                     bar{le=\"0.5\"} 2\n\
                     \n\
                     baz_total{name=\"x\"} 3\n";
        let names = parse_prometheus_metric_names(body);
        let expected: std::collections::BTreeSet<String> = ["bar", "baz_total", "foo"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn parse_prometheus_metric_names_handles_empty_input() {
        assert!(parse_prometheus_metric_names(b"").is_empty());
        assert!(parse_prometheus_metric_names(b"# only a comment\n").is_empty());
        assert!(parse_prometheus_metric_names(b"\n\n\n").is_empty());
    }

    #[test]
    fn body_rule_prometheus_exposition_parses_with_default_allowlists() {
        // Harness consumers can declare the rule without seeding either
        // allow-list (empirical seeding lands at Task 13 per signpost 12).
        let yaml = "kind: prometheus_exposition";
        let r: BodyRule = serde_yaml::from_str(yaml).expect("parses");
        match r {
            BodyRule::PrometheusExposition {
                allowlist_envoy_only,
                allowlist_envoy_rust_only,
                ..
            } => {
                assert!(allowlist_envoy_only.is_empty());
                assert!(allowlist_envoy_rust_only.is_empty());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn body_rule_prometheus_exposition_parses_with_seeded_allowlists() {
        let yaml = r#"
kind: prometheus_exposition
allowlist_envoy_only:
  - server.uptime
  - cluster.x.upstream_cx_total
allowlist_envoy_rust_only:
  - foo_total
"#;
        let r: BodyRule = serde_yaml::from_str(yaml).expect("parses");
        match r {
            BodyRule::PrometheusExposition {
                allowlist_envoy_only,
                allowlist_envoy_rust_only,
                ..
            } => {
                assert_eq!(allowlist_envoy_only.len(), 2);
                assert_eq!(allowlist_envoy_rust_only, vec!["foo_total".to_string()]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn body_rule_byte_exact_parses_under_internally_tagged_form() {
        // 06.1 Task 12 switched BodyRule from externally-tagged (YAML scalar
        // `byte_exact`) to internally-tagged (`{ kind: byte_exact }`) so
        // struct-form variants like PrometheusExposition could land
        // alongside ByteExact. Existing fixtures (0001-0010) updated in
        // the same commit.
        let yaml = "kind: byte_exact";
        let r: BodyRule = serde_yaml::from_str(yaml).expect("parses");
        assert_eq!(r, BodyRule::ByteExact);
    }

    #[test]
    fn driver_admin_scrape_parses_with_default_pre_requests() {
        // 08.1 Task 11: Driver::AdminScrape widened to take a list of
        // sub-cases under `scrapes: [...]` (architecture-decision lock-in
        // #13 forbids a new Driver variant; fixture 0014 needs to scrape
        // 4 admin endpoints in one fixture).
        let yaml = r#"
kind: admin_scrape
scrapes:
  - path: /stats/prometheus
    expected_status: 200
    expected_content_type: text/plain
    expected_body_rule:
      kind: prometheus_exposition
"#;
        let d: Driver = serde_yaml::from_str(yaml).expect("parses");
        match d {
            Driver::AdminScrape {
                pre_requests,
                scrapes,
                ..
            } => {
                assert!(pre_requests.is_empty());
                assert_eq!(scrapes.len(), 1);
                assert_eq!(scrapes[0].path, "/stats/prometheus");
                assert_eq!(scrapes[0].expected_status, 200);
                assert_eq!(scrapes[0].expected_content_type, "text/plain");
                assert!(matches!(
                    scrapes[0].expected_body_rule,
                    BodyRule::PrometheusExposition { .. }
                ));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn driver_admin_scrape_parses_with_pre_requests() {
        let yaml = r#"
kind: admin_scrape
pre_requests:
  - method: GET
    path: /
    host: x.test
    port_key: PORT
scrapes:
  - path: /stats/prometheus
    expected_status: 200
    expected_content_type: text/plain; version=0.0.4
    expected_body_rule:
      kind: prometheus_exposition
      allowlist_envoy_only: []
      allowlist_envoy_rust_only: []
"#;
        let d: Driver = serde_yaml::from_str(yaml).expect("parses");
        match d {
            Driver::AdminScrape {
                pre_requests,
                scrapes,
                ..
            } => {
                assert_eq!(pre_requests.len(), 1);
                assert_eq!(pre_requests[0].method, "GET");
                assert_eq!(pre_requests[0].port_key, "PORT");
                assert_eq!(scrapes.len(), 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn driver_admin_scrape_parses_with_multiple_scrapes() {
        // 08.1 Task 11: fixture 0014 drives 4 admin scrapes in one
        // expectation. Validate Vec<AdminScrapeCase> deserializes with
        // distinct body-rule kinds per sub-case (the realistic shape:
        // `/config_dump` is json_shape; `/clusters` is text_lines; etc).
        let yaml = r#"
kind: admin_scrape
pre_requests: []
scrapes:
  - path: /config_dump
    expected_status: 200
    expected_content_type: application/json
    expected_body_rule:
      kind: json_shape
      required_keys: ["configs"]
  - path: /clusters
    expected_status: 200
    expected_content_type: text/plain
    expected_body_rule:
      kind: text_lines
      required_lines: ["backend::observability_name::backend"]
"#;
        let d: Driver = serde_yaml::from_str(yaml).expect("parses");
        match d {
            Driver::AdminScrape {
                pre_requests,
                scrapes,
                ..
            } => {
                assert!(pre_requests.is_empty());
                assert_eq!(scrapes.len(), 2);
                assert_eq!(scrapes[0].path, "/config_dump");
                assert!(matches!(
                    scrapes[0].expected_body_rule,
                    BodyRule::JsonShape { .. }
                ));
                assert_eq!(scrapes[1].path, "/clusters");
                assert!(matches!(
                    scrapes[1].expected_body_rule,
                    BodyRule::TextLines { .. }
                ));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn driver_http2_after_settle_parses_expected_fields() {
        // Phase 69 (grpc_health_check, ADR-0138): the H2 sibling of
        // `Driver::Http1AfterSettle` (12.2). Fixture 0075 drives an H2
        // request past active-HC settle and asserts a byte-exact
        // "no healthy upstream" 503 body, with no `expected_headers`
        // (the header axis is skipped when the fixture omits the field).
        let yaml = r#"
kind: http2_after_settle
settle_ms: 100
method: get
path: "/"
host: h
expected_status: 503
expected_body:
  kind: byte_exact
  body: "no healthy upstream"
"#;
        let d: Driver = serde_yaml::from_str(yaml).expect("parses");
        match d {
            Driver::Http2AfterSettle {
                settle_ms,
                method,
                path,
                host,
                expected_status,
                expected_body,
                expected_headers,
            } => {
                assert_eq!(settle_ms, 100);
                assert_eq!(method, Http1Method::Get);
                assert_eq!(path, "/");
                assert_eq!(host, "h");
                assert_eq!(expected_status, Some(503));
                assert_eq!(
                    expected_body,
                    Some(Http1BodyRule::ByteExact {
                        body: "no healthy upstream".to_string()
                    })
                );
                assert_eq!(expected_headers, None);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn render_yaml_substitutes_admin_port_in_admin_scrape_template() {
        // 06.1 D6.a: Driver::AdminScrape fixtures use {{ADMIN_PORT}}
        // substitution for the admin listener (joins {{PORT}} for the
        // HCM listener). This test exercises only the render path; the
        // run_fixture-side reservation+expose is exercised by the
        // Docker-gated fixture 0011 test (Task 13).
        let template = "admin: 127.0.0.1:{{ADMIN_PORT}}\nlistener: 127.0.0.1:{{PORT}}";
        let rendered = render_yaml(template, &[("ADMIN_PORT", "29999"), ("PORT", "10000")]);
        assert_eq!(
            rendered,
            "admin: 127.0.0.1:29999\nlistener: 127.0.0.1:10000"
        );
    }

    #[test]
    fn assert_body_rule_byte_exact_passes_on_equal_bytes() {
        assert_body_rule(&BodyRule::ByteExact, b"hello", b"hello").unwrap();
    }

    #[test]
    fn assert_body_rule_byte_exact_fails_on_unequal_bytes() {
        let err = assert_body_rule(&BodyRule::ByteExact, b"hello", b"world").unwrap_err();
        assert!(err.to_string().contains("byte-exact"), "msg: {err}");
    }

    #[test]
    fn assert_body_rule_prometheus_exposition_passes_on_equal_metric_sets() {
        let rule = BodyRule::PrometheusExposition {
            allowlist_envoy_only: vec![],
            allowlist_envoy_rust_only: vec![],
            value_exact: vec![],
            value_must_be_zero: vec![],
            value_present_only: vec![],
        };
        let envoy = b"foo 1\nbar 2\n";
        let rust = b"bar 5\nfoo 9\n"; // values differ; names equal — must pass
        assert_body_rule(&rule, envoy, rust).unwrap();
    }

    #[test]
    fn assert_body_rule_prometheus_exposition_fails_on_envoy_only_outside_allowlist() {
        let rule = BodyRule::PrometheusExposition {
            allowlist_envoy_only: vec![],
            allowlist_envoy_rust_only: vec![],
            value_exact: vec![],
            value_must_be_zero: vec![],
            value_present_only: vec![],
        };
        let envoy = b"foo 1\nbar 2\n";
        let rust = b"foo 1\n"; // bar only on envoy side — must fail
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("envoy-only"), "msg: {msg}");
        assert!(msg.contains("bar"), "msg: {msg}");
    }

    #[test]
    fn assert_body_rule_prometheus_exposition_passes_when_diff_inside_allowlist() {
        let rule = BodyRule::PrometheusExposition {
            allowlist_envoy_only: vec!["bar".to_string()],
            allowlist_envoy_rust_only: vec!["baz".to_string()],
            value_exact: vec![],
            value_must_be_zero: vec![],
            value_present_only: vec![],
        };
        let envoy = b"foo 1\nbar 2\n";
        let rust = b"foo 1\nbaz 3\n";
        assert_body_rule(&rule, envoy, rust).unwrap();
    }

    #[test]
    fn assert_body_rule_prometheus_exposition_passes_on_value_exact_match() {
        let envoy_body = b"# TYPE foo counter\nfoo 5\n# TYPE bar counter\nbar 0\n";
        let rust_body = b"# TYPE foo counter\nfoo 5\n# TYPE bar counter\nbar 0\n";
        let rule = BodyRule::PrometheusExposition {
            allowlist_envoy_only: vec![],
            allowlist_envoy_rust_only: vec![],
            value_exact: vec![("foo".to_string(), 5)],
            value_must_be_zero: vec!["bar".to_string()],
            value_present_only: vec![],
        };
        assert_body_rule(&rule, envoy_body, rust_body).expect("value-exact + must-be-zero match");
    }

    #[test]
    fn assert_body_rule_prometheus_exposition_fails_on_value_mismatch() {
        let envoy_body = b"# TYPE foo counter\nfoo 5\n";
        let rust_body = b"# TYPE foo counter\nfoo 6\n";
        let rule = BodyRule::PrometheusExposition {
            allowlist_envoy_only: vec![],
            allowlist_envoy_rust_only: vec![],
            value_exact: vec![("foo".to_string(), 5)],
            value_must_be_zero: vec![],
            value_present_only: vec![],
        };
        let err = assert_body_rule(&rule, envoy_body, rust_body).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("value_exact mismatch"),
            "expected value_exact mismatch, got: {msg}"
        );
    }
}

/// 08.1 Task 10 (D15): tests for the two new `BodyRule` variants
/// `JsonShape` + `TextLines`, plus the `JsonSubtreeRule` helper struct
/// and the `walk_pointer` dotted-path helper. Sibling `#[cfg(test)]` block
/// placed AFTER the existing `mod tests` per the per-task placement
/// convention (Tasks 6-9 each appended new test modules at the file's end).
#[cfg(test)]
mod body_rule_extension_tests {
    use super::{BodyRule, JsonSubtreeRule, assert_body_rule};

    #[test]
    fn json_shape_required_keys_pass_when_all_present() {
        let rule = BodyRule::JsonShape {
            required_keys: vec!["a".into(), "b".into(), "c".into()],
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec!["a".into(), "b".into(), "c".into()],
        };
        let envoy = br#"{"a":1,"b":"two","c":[3]}"#;
        let rust = br#"{"a":9,"b":"different","c":[42,42]}"#;
        assert_body_rule(&rule, envoy, rust).expect("required keys present on both sides");
    }

    #[test]
    fn json_shape_required_keys_fail_when_missing() {
        let rule = BodyRule::JsonShape {
            required_keys: vec!["a".into(), "b".into(), "c".into()],
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        // Both bodies are missing `b`.
        let envoy = br#"{"a":1,"c":3}"#;
        let rust = br#"{"a":1,"c":3}"#;
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains('b'), "expected `b` mentioned, got: {msg}");
    }

    #[test]
    fn json_shape_envoy_only_key_allowed() {
        let rule = BodyRule::JsonShape {
            required_keys: vec!["a".into()],
            required_subtree: None,
            allowlist_envoy_only_keys: vec!["envoy_only".into()],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec!["a".into()],
        };
        let envoy = br#"{"a":1,"envoy_only":"value"}"#;
        let rust = br#"{"a":1}"#;
        assert_body_rule(&rule, envoy, rust).expect("envoy-only key on allowlist passes");
    }

    #[test]
    fn json_shape_required_subtree_value_exact() {
        // Task 11 strictness wiring: required_subtree asserts the
        // dotted-path sub-value equals `expected` on BOTH sides. Use
        // identical top-level objects so the Task-11 shared-key
        // value-equality check passes too (the asymmetric "other":1 vs
        // "other":99 shape that Task 10 tolerated is now diff-strict
        // unless the top-level key sits in value_may_differ_keys).
        let rule = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: Some(JsonSubtreeRule {
                path: "node.id".into(),
                path_envoy: None,
                path_envoy_rust: None,
                expected: serde_yaml::Value::String("x".into()),
            }),
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        let envoy = br#"{"node":{"id":"x"}}"#;
        let rust = br#"{"node":{"id":"x"}}"#;
        assert_body_rule(&rule, envoy, rust).expect("required_subtree matches on both sides");
    }

    /// 20 Task 6 (ADR-0052) regression: a shared `path` with NO per-side
    /// overrides resolves to the same dotted path on BOTH sides — the
    /// pre-existing fixtures (0014/0026/0027) rely on this. `envoy_path()`
    /// and `rust_path()` both return the shared `path`.
    #[test]
    fn json_subtree_rule_shared_path_resolves_both_sides() {
        let rule = JsonSubtreeRule {
            path: "configs.1.x".into(),
            path_envoy: None,
            path_envoy_rust: None,
            expected: serde_yaml::Value::Null,
        };
        assert_eq!(rule.envoy_path(), "configs.1.x");
        assert_eq!(rule.rust_path(), "configs.1.x");

        // End-to-end: the same dotted path walks to the same node on both
        // bodies (the RoutesConfigDump-at-fixed-index legacy shape).
        let e2e = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: Some(JsonSubtreeRule {
                path: "configs.1.x".into(),
                path_envoy: None,
                path_envoy_rust: None,
                expected: serde_yaml::Value::String("shared".into()),
            }),
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        let envoy = br#"{"configs":[{},{"x":"shared"}]}"#;
        let rust = br#"{"configs":[{},{"x":"shared"}]}"#;
        assert_body_rule(&e2e, envoy, rust).expect("shared-path subtree matches on both sides");
    }

    /// 20 Task 6 (ADR-0052): per-side path override. When `path_envoy` /
    /// `path_envoy_rust` are present they override the shared `path` for that
    /// side only — the Envoy body addresses `configs.4.y` while the envoy-rust
    /// body addresses `configs.2.y`, both compared to the same `expected`.
    #[test]
    fn json_subtree_rule_per_side_path_override() {
        let rule = JsonSubtreeRule {
            path: String::new(),
            path_envoy: Some("configs.4.y".into()),
            path_envoy_rust: Some("configs.2.y".into()),
            expected: serde_yaml::Value::Null,
        };
        // The accessors pick the per-side override, not the (empty) shared path.
        assert_eq!(rule.envoy_path(), "configs.4.y");
        assert_eq!(rule.rust_path(), "configs.2.y");

        // End-to-end: the RoutesConfigDump entry lands at configs[4] on Envoy
        // but configs[2] on envoy-rust. Distinct array shapes per side; the
        // per-side path resolution must pick the right node on each.
        let e2e = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: Some(JsonSubtreeRule {
                path: String::new(),
                path_envoy: Some("configs.4.y".into()),
                path_envoy_rust: Some("configs.2.y".into()),
                expected: serde_yaml::Value::String("routes".into()),
            }),
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            // The whole point of the per-side override is that the two bodies'
            // top-level `configs` array differs (different length/ordering); the
            // subtree rule pins the SEMANTIC node per-side. Tolerate the
            // top-level value drift so only the per-side subtree check runs.
            value_may_differ_keys: vec!["configs".into()],
        };
        // Envoy: the addressed node sits at configs[4]; configs[2] holds a
        // DIFFERENT value, proving the envoy side does NOT walk the rust path.
        let envoy = br#"{"configs":[{},{},{"y":"WRONG"},{},{"y":"routes"}]}"#;
        // envoy-rust: the addressed node sits at configs[2].
        let rust = br#"{"configs":[{},{},{"y":"routes"}]}"#;
        assert_body_rule(&e2e, envoy, rust)
            .expect("per-side-override subtree matches the right node on each side");
    }

    #[test]
    fn text_lines_required_lines_pass_when_present() {
        let rule = BodyRule::TextLines {
            required_lines: vec!["foo".into(), "bar".into()],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec![],
            allowlist_envoy_rust_only_lines: vec![],
            allowlist_envoy_only_line_prefixes: vec![],
            allowlist_envoy_rust_only_line_prefixes: vec![],
        };
        let envoy = b"foo\nbar\n";
        let rust = b"bar\nfoo\n";
        assert_body_rule(&rule, envoy, rust).expect("required lines present on both sides");
    }

    #[test]
    fn text_lines_envoy_only_lines_allowed() {
        let rule = BodyRule::TextLines {
            required_lines: vec!["foo".into()],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec!["envoy_only_extra".into()],
            allowlist_envoy_rust_only_lines: vec![],
            allowlist_envoy_only_line_prefixes: vec![],
            allowlist_envoy_rust_only_line_prefixes: vec![],
        };
        let envoy = b"foo\nenvoy_only_extra\n";
        let rust = b"foo\n";
        assert_body_rule(&rule, envoy, rust).expect("envoy-only line on allowlist passes");
    }

    #[test]
    fn text_lines_required_prefix_matches() {
        // Task 11 strictness wiring: lines that diverge between envoy
        // and envoy-rust (outside the per-side allow-lists) are now
        // diff-strict. The varying-suffix counter shape this test
        // demonstrates needs the per-side allow-lists to cover the
        // varying-suffix lines explicitly (each side's suffix lands in
        // the other side's allow-list as an envoy-only / rust-only line).
        let rule = BodyRule::TextLines {
            required_lines: vec![],
            required_line_prefixes: vec![
                "listener_0::counter_".into(),
                "listener_1::counter_".into(),
            ],
            allowlist_envoy_only_lines: vec![
                "listener_0::counter_X".into(),
                "listener_1::counter_Y".into(),
            ],
            allowlist_envoy_rust_only_lines: vec![
                "listener_0::counter_A".into(),
                "listener_1::counter_B".into(),
            ],
            allowlist_envoy_only_line_prefixes: vec![],
            allowlist_envoy_rust_only_line_prefixes: vec![],
        };
        let envoy = b"listener_0::counter_X\nlistener_1::counter_Y\n";
        let rust = b"listener_0::counter_A\nlistener_1::counter_B\n";
        assert_body_rule(&rule, envoy, rust).expect("required line prefixes match on both sides");
    }

    // -------------------------------------------------------------------
    // 08.1 Task 11 strictness-wiring tests (Task 10 minor-findings #1, #2,
    // #3 close). The fields below were accepted at the schema level in
    // Task 10 but did NOT participate in fail logic; Task 11 wires them.
    // -------------------------------------------------------------------

    /// Task 10 left a coverage gap: `JsonSubtreeRule.expected` was accepted
    /// but never consulted. Task 11 wires it: the dispatch arm asserts
    /// `envoy_sub == expected` AND `rust_sub == expected` (in addition to
    /// the existing `envoy_sub == rust_sub`). This test flips the expected
    /// value to confirm the assertion now fails.
    #[test]
    fn json_shape_required_subtree_fails_when_expected_value_mismatches() {
        let rule = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: Some(JsonSubtreeRule {
                path: "node.id".into(),
                path_envoy: None,
                path_envoy_rust: None,
                // Both bodies report id="x"; expected says "y" — must fail.
                expected: serde_yaml::Value::String("y".into()),
            }),
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        let envoy = br#"{"node":{"id":"x"}}"#;
        let rust = br#"{"node":{"id":"x"}}"#;
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("required_subtree") && msg.contains("expected"),
            "expected required_subtree-expected mismatch, got: {msg}"
        );
    }

    /// Task 11 strictness wiring: an envoy-side key NOT on
    /// `allowlist_envoy_only_keys` AND NOT on `value_may_differ_keys`
    /// MUST cause failure when absent on the envoy-rust side.
    #[test]
    fn json_shape_fails_on_envoy_only_key_outside_allowlist() {
        let rule = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        let envoy = br#"{"a":1,"unexpected_envoy_only":42}"#;
        let rust = br#"{"a":1}"#;
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unexpected_envoy_only"),
            "expected `unexpected_envoy_only` mentioned, got: {msg}"
        );
    }

    /// Task 11 strictness wiring: an envoy-rust-side key NOT on
    /// `allowlist_envoy_rust_only_keys` MUST cause failure when absent
    /// on the envoy side.
    #[test]
    fn json_shape_fails_on_rust_only_key_outside_allowlist() {
        let rule = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        let envoy = br#"{"a":1}"#;
        let rust = br#"{"a":1,"unexpected_rust_only":42}"#;
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unexpected_rust_only"),
            "expected `unexpected_rust_only` mentioned, got: {msg}"
        );
    }

    /// Task 11 strictness wiring: keys appearing on BOTH sides and NOT
    /// listed in `value_may_differ_keys` MUST serialize equal.
    #[test]
    fn json_shape_fails_when_shared_key_values_differ_outside_may_differ() {
        let rule = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec![],
        };
        let envoy = br#"{"shared":"envoy-value"}"#;
        let rust = br#"{"shared":"rust-value"}"#;
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("shared") && msg.contains("value"),
            "expected shared-key value diff, got: {msg}"
        );
    }

    /// Task 11 strictness wiring: when a shared key IS listed in
    /// `value_may_differ_keys`, value drift is silently accepted.
    #[test]
    fn json_shape_passes_when_value_diff_inside_may_differ() {
        let rule = BodyRule::JsonShape {
            required_keys: vec![],
            required_subtree: None,
            allowlist_envoy_only_keys: vec![],
            allowlist_envoy_rust_only_keys: vec![],
            value_may_differ_keys: vec!["last_updated".into()],
        };
        let envoy = br#"{"last_updated":"2026-05-16T00:00:00Z"}"#;
        let rust = br#"{"last_updated":"2026-05-16T00:00:01Z"}"#;
        assert_body_rule(&rule, envoy, rust).expect("value_may_differ_keys covers drift");
    }

    /// Task 11 strictness wiring: an envoy-side line NOT on
    /// `allowlist_envoy_only_lines` MUST cause failure when absent on
    /// the envoy-rust side.
    #[test]
    fn text_lines_fails_on_envoy_only_line_outside_allowlist() {
        let rule = BodyRule::TextLines {
            required_lines: vec![],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec![],
            allowlist_envoy_rust_only_lines: vec![],
            allowlist_envoy_only_line_prefixes: vec![],
            allowlist_envoy_rust_only_line_prefixes: vec![],
        };
        let envoy = b"foo\nunexpected_envoy_only_line\n";
        let rust = b"foo\n";
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unexpected_envoy_only_line"),
            "expected envoy-only line in diff, got: {msg}"
        );
    }

    /// Task 11 strictness wiring: an envoy-rust-side line NOT on
    /// `allowlist_envoy_rust_only_lines` MUST cause failure when absent
    /// on the envoy side.
    #[test]
    fn text_lines_fails_on_rust_only_line_outside_allowlist() {
        let rule = BodyRule::TextLines {
            required_lines: vec![],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec![],
            allowlist_envoy_rust_only_lines: vec![],
            allowlist_envoy_only_line_prefixes: vec![],
            allowlist_envoy_rust_only_line_prefixes: vec![],
        };
        let envoy = b"foo\n";
        let rust = b"foo\nunexpected_rust_only_line\n";
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unexpected_rust_only_line"),
            "expected rust-only line in diff, got: {msg}"
        );
    }

    /// Task 11 NEW: `allowlist_envoy_only_line_prefixes` absorbs varying-
    /// suffix per-side lines whose port/IP/timestamp segment shifts per
    /// fixture run (e.g. fixture 0014's `/clusters` per-endpoint counter
    /// lines like `backend::<ip>:<ephemeral>::cx_active::0`).
    #[test]
    fn text_lines_envoy_only_line_prefix_absorbs_varying_suffix() {
        let rule = BodyRule::TextLines {
            required_lines: vec![],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec![],
            allowlist_envoy_rust_only_lines: vec![],
            allowlist_envoy_only_line_prefixes: vec!["backend::192.168.65.254:".into()],
            allowlist_envoy_rust_only_line_prefixes: vec![],
        };
        let envoy = b"foo\nbackend::192.168.65.254:63570::cx_active::0\nbackend::192.168.65.254:63570::rq_total::0\n";
        let rust = b"foo\n";
        assert_body_rule(&rule, envoy, rust)
            .expect("envoy-only lines starting with prefix are allow-listed");
    }

    /// Task 11 NEW: the prefix-allow-list does NOT shadow other lines —
    /// lines NOT starting with any prefix AND NOT in the exact list still
    /// fail.
    #[test]
    fn text_lines_envoy_only_line_prefix_does_not_shadow_other_lines() {
        let rule = BodyRule::TextLines {
            required_lines: vec![],
            required_line_prefixes: vec![],
            allowlist_envoy_only_lines: vec![],
            allowlist_envoy_rust_only_lines: vec![],
            allowlist_envoy_only_line_prefixes: vec!["backend::192.168.65.254:".into()],
            allowlist_envoy_rust_only_line_prefixes: vec![],
        };
        let envoy = b"backend::192.168.65.254:63570::cx_active::0\nbackend::added_via_api::false\n";
        let rust = b"";
        let err = assert_body_rule(&rule, envoy, rust).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("added_via_api"),
            "expected non-prefix line in diff, got: {msg}"
        );
        assert!(
            !msg.contains("cx_active"),
            "prefix-allow-listed line should NOT appear in diff, got: {msg}"
        );
    }
}

/// 08.2 Task 7 (D16): tests for the `Driver::AdminScrape`
/// `pre_admin_actions` + `post_admin_assertions` extensions, the new
/// `AdminAction` + `AdminAssertion` enums, the supporting helpers
/// (`drive_admin_post` + `assert_data_plane_connection_refused`), and
/// the 08.1 REVIEW M4 closure (`walk_pointer` empty-segment guard).
/// Sibling `#[cfg(test)]` block placed AFTER `body_rule_extension_tests`
/// per the per-task placement convention.
#[cfg(test)]
mod admin_action_extension_tests {
    use super::{
        AdminAction, AdminAssertion, Driver, Expectations, assert_data_plane_connection_refused,
        drive_admin_post, walk_pointer,
    };
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    // ----- Deserialization tests ---------------------------------------

    /// Driver::AdminScrape YAML carries an explicit `pre_admin_actions:`
    /// list with a `kind: post` action; the action deserializes to
    /// `AdminAction::Post { path, expected_status }`.
    #[test]
    fn admin_scrape_deserializes_pre_admin_actions_with_post() {
        let yaml = r#"
driver:
  kind: admin_scrape
  pre_admin_actions:
    - kind: post
      path: /drain_listeners
      expected_status: 200
  scrapes:
    - path: /server_info
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
        required_keys: ["state"]
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::AdminScrape {
            pre_admin_actions, ..
        } = e.driver
        else {
            panic!("expected AdminScrape");
        };
        assert_eq!(pre_admin_actions.len(), 1);
        match &pre_admin_actions[0] {
            AdminAction::Post {
                path,
                expected_status,
            } => {
                assert_eq!(path, "/drain_listeners");
                assert_eq!(*expected_status, 200u16);
            }
        }
    }

    /// Driver::AdminScrape YAML carries an explicit `post_admin_assertions:`
    /// list with `kind: data_plane_connection_refused`; the assertion
    /// deserializes to `AdminAssertion::DataPlaneConnectionRefused` with
    /// `within_ms` as a raw `u64` (per architecture-decision lock-in #19,
    /// no humantime dep).
    #[test]
    fn admin_scrape_deserializes_post_admin_assertions_with_data_plane_connection_refused() {
        let yaml = r#"
driver:
  kind: admin_scrape
  scrapes:
    - path: /server_info
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
        required_keys: ["state"]
  post_admin_assertions:
    - kind: data_plane_connection_refused
      listener_address: 127.0.0.1:8080
      within_ms: 5000
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::AdminScrape {
            post_admin_assertions,
            ..
        } = e.driver
        else {
            panic!("expected AdminScrape");
        };
        assert_eq!(post_admin_assertions.len(), 1);
        match &post_admin_assertions[0] {
            AdminAssertion::DataPlaneConnectionRefused {
                listener_address,
                within_ms,
            } => {
                assert_eq!(listener_address, "127.0.0.1:8080");
                assert_eq!(*within_ms, 5000u64);
            }
        }
    }

    /// Driver::AdminScrape YAML that omits BOTH new fields keeps fixtures
    /// 0011 + 0014 backward-compatible: each new field defaults to an
    /// empty `Vec` via `#[serde(default)]`.
    #[test]
    fn admin_scrape_pre_admin_actions_defaults_to_empty_vec() {
        let yaml = r#"
driver:
  kind: admin_scrape
  scrapes:
    - path: /stats/prometheus
      expected_status: 200
      expected_content_type: text/plain
      expected_body_rule:
        kind: prometheus_exposition
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::AdminScrape {
            pre_admin_actions,
            post_admin_assertions,
            ..
        } = e.driver
        else {
            panic!("expected AdminScrape");
        };
        assert!(pre_admin_actions.is_empty());
        assert!(post_admin_assertions.is_empty());
    }

    /// AdminScrape YAML may declare multiple pre_admin_actions and
    /// multiple post_admin_assertions; both deserialize in order.
    #[test]
    fn admin_scrape_deserializes_multiple_pre_admin_actions_and_assertions() {
        let yaml = r#"
driver:
  kind: admin_scrape
  pre_admin_actions:
    - kind: post
      path: /healthcheck/fail
      expected_status: 200
    - kind: post
      path: /drain_listeners
      expected_status: 200
  scrapes:
    - path: /server_info
      expected_status: 200
      expected_content_type: application/json
      expected_body_rule:
        kind: json_shape
        required_keys: ["state"]
  post_admin_assertions:
    - kind: data_plane_connection_refused
      listener_address: 127.0.0.1:1
      within_ms: 100
    - kind: data_plane_connection_refused
      listener_address: 127.0.0.1:2
      within_ms: 100
"#;
        let e: Expectations = serde_yaml::from_str(yaml).expect("parses");
        let Driver::AdminScrape {
            pre_admin_actions,
            post_admin_assertions,
            ..
        } = e.driver
        else {
            panic!("expected AdminScrape");
        };
        assert_eq!(pre_admin_actions.len(), 2);
        assert_eq!(post_admin_assertions.len(), 2);
        match &pre_admin_actions[0] {
            AdminAction::Post { path, .. } => assert_eq!(path, "/healthcheck/fail"),
        }
        match &pre_admin_actions[1] {
            AdminAction::Post { path, .. } => assert_eq!(path, "/drain_listeners"),
        }
    }

    // ----- Helper: drive_admin_post ------------------------------------

    /// `drive_admin_post` issues a real POST against a mock admin
    /// listener; when the mock returns the expected status, the helper
    /// returns `Ok(())`.
    #[tokio::test(flavor = "multi_thread")]
    async fn drive_admin_post_succeeds_on_expected_status() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            // Drain headers until CRLFCRLF.
            let mut buf = [0u8; 1024];
            let mut acc = Vec::new();
            loop {
                let n = s.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                acc.extend_from_slice(&buf[..n]);
                if acc.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            use tokio::io::AsyncWriteExt as _;
            s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await
                .unwrap();
            s.shutdown().await.ok();
            drop(s);
        });

        drive_admin_post(addr, "/drain_listeners", 200)
            .await
            .expect("expected status matched");
        server.await.unwrap();
    }

    /// `drive_admin_post` fails when the mock returns a status that
    /// does not match the expected one; the error mentions both values.
    #[tokio::test(flavor = "multi_thread")]
    async fn drive_admin_post_fails_on_status_mismatch() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let mut acc = Vec::new();
            loop {
                let n = s.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                acc.extend_from_slice(&buf[..n]);
                if acc.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            use tokio::io::AsyncWriteExt as _;
            s.write_all(
                b"HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
            )
            .await
            .unwrap();
            s.shutdown().await.ok();
            drop(s);
        });

        let err = drive_admin_post(addr, "/drain_listeners", 200)
            .await
            .expect_err("status 503 != expected 200");
        let msg = format!("{err:#}");
        assert!(msg.contains("503"), "msg: {msg}");
        assert!(msg.contains("200"), "msg: {msg}");
        server.await.unwrap();
    }

    // ----- Helper: assert_data_plane_connection_refused ----------------

    /// `assert_data_plane_connection_refused` succeeds when the target
    /// address has no listener (kernel returns ECONNREFUSED).
    #[tokio::test(flavor = "multi_thread")]
    async fn assert_data_plane_connection_refused_succeeds_when_econnrefused() {
        // Bind+drop reserves a port the kernel will reject when
        // re-connected against (TOCTOU window minimal; harness-side
        // pattern matches reserve_port).
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);
        assert_data_plane_connection_refused(addr, Duration::from_millis(500))
            .await
            .expect("ECONNREFUSED accepted as drained");
    }

    /// `assert_data_plane_connection_refused` succeeds when the target
    /// address accepts the connection but immediately closes with EOF
    /// (the post-drain "draining listener" disposition: kernel still
    /// accepts because the listening fd hasn't been torn down on this
    /// side, but the server-side immediately FINs).
    #[tokio::test(flavor = "multi_thread")]
    async fn assert_data_plane_connection_refused_succeeds_on_immediate_eof() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        // Spawn a server that accepts and immediately drops every connection
        // for the duration of the assertion window.
        let server = tokio::spawn(async move {
            while let Ok((s, _)) = listener.accept().await {
                drop(s);
            }
        });
        assert_data_plane_connection_refused(addr, Duration::from_millis(500))
            .await
            .expect("immediate EOF accepted as drained");
        server.abort();
    }

    /// `assert_data_plane_connection_refused` FAILS when the target
    /// address accepts the connection and writes data before EOF (the
    /// listener is still "live" — drain did not take effect).
    #[tokio::test(flavor = "multi_thread")]
    async fn assert_data_plane_connection_refused_fails_when_listener_responds() {
        use tokio::io::AsyncWriteExt as _;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            // Keep accepting + responding for the duration of the assertion
            // window so every poll within `within` observes "live".
            while let Ok((mut s, _)) = listener.accept().await {
                let _ = s.write_all(b"LIVE\n").await;
                // Hold open briefly so the harness's read does not
                // hit immediate EOF.
                tokio::time::sleep(Duration::from_millis(50)).await;
                drop(s);
            }
        });
        let err = assert_data_plane_connection_refused(addr, Duration::from_millis(400))
            .await
            .expect_err("live listener must fail the assertion");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("listener") || msg.contains("data plane") || msg.contains("LIVE"),
            "unexpected error: {msg}",
        );
        server.abort();
    }

    /// `assert_data_plane_connection_refused` succeeds when the target
    /// address accepts the connection but the listener tears it down
    /// ungracefully (RST), so the harness's post-connect read returns
    /// `Err` rather than `Ok(0)`. This is the third drain-success
    /// disposition per PLAN.md worked example (lines 2282-2286): some
    /// drain configurations RST in-flight connections rather than
    /// FINing cleanly, and either shape counts as evidence the
    /// listener is drained.
    #[tokio::test(flavor = "multi_thread")]
    async fn assert_data_plane_connection_refused_treats_ungraceful_close_as_drain_success() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        // Spawn a server that accepts then RSTs each connection. The
        // RST shape is forced by SO_LINGER=0 before drop — the kernel
        // emits RST instead of FIN. The client's read on a RST'd
        // socket returns Err (ECONNRESET), exercising the helper's
        // ungraceful-close arm (NOT the Ok(0) immediate-EOF arm
        // exercised by the sibling _succeeds_on_immediate_eof test).
        //
        // Note: tokio::net::TcpStream::set_linger is deprecated
        // upstream (the deprecation flags that SO_LINGER causes the
        // socket to block the EXECUTOR thread on drop in production
        // code paths). For this synthetic mock listener that exists
        // solely to RST the per-attempt accepted socket and then
        // returns to the accept loop, the executor-blocking concern
        // does not apply: the linger duration is 0, the close issues
        // RST immediately without buffering, and the spawned task is
        // the only user of this executor scaffold. std::net's
        // set_linger is still unstable (rust-lang/rust#88494) and we
        // cannot add socket2 or libc as new top-level Cargo deps
        // per the 08.2 no-new-deps doctrine — so the documented +
        // locally-allowed deprecated tokio path is the right call
        // here. The allow is narrowly scoped to this single
        // statement.
        let server = tokio::spawn(async move {
            while let Ok((s, _)) = listener.accept().await {
                #[allow(deprecated)]
                let _ = s.set_linger(Some(Duration::from_secs(0)));
                drop(s);
            }
        });
        assert_data_plane_connection_refused(addr, Duration::from_millis(500))
            .await
            .expect("ungraceful close (RST) accepted as drained");
        server.abort();
    }

    // ----- 08.1 REVIEW M4 closure --------------------------------------

    /// `walk_pointer` MUST reject dotted paths containing empty segments
    /// with a structured error that names the offending path; this is
    /// the 08.1 REVIEW M4 closure.
    #[test]
    fn walk_pointer_rejects_empty_segment_with_structured_error() {
        let v = serde_json::json!({"a": {"b": 1}});
        let err = walk_pointer(&v, "a..b").expect_err("dotted path with empty segment must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("empty segment"),
            "error must name the empty-segment defect, got: {msg}",
        );
        assert!(
            msg.contains("a..b"),
            "error must include the offending path, got: {msg}",
        );
    }

    /// `walk_pointer` MUST also reject paths with a trailing dot
    /// (empty segment at the tail). Same guard, second exemplar.
    #[test]
    fn walk_pointer_rejects_trailing_empty_segment() {
        let v = serde_json::json!({"a": {"b": 1}});
        let err = walk_pointer(&v, "a.b.").expect_err("trailing-dot path must fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("empty segment"),
            "error must name the empty-segment defect, got: {msg}",
        );
        assert!(
            msg.contains("a.b."),
            "error must include the offending path, got: {msg}",
        );
    }
}

/// 21 Task 6 (ADR-0054; §6.2 L9): the file-based-EDS harness primitives — the
/// `{{EDS_PATH}}`-marker scan (the `needs_eds` inertness gate) and the
/// host-gateway-IP discovery. Sibling `#[cfg(test)]` block placed AFTER
/// `admin_action_extension_tests` per the per-task placement convention.
#[cfg(test)]
mod eds_harness_tests {
    use super::Driver;
    use super::discover_host_gateway_ip;

    /// The EDS-marker scan. A fixture template that references `{{EDS_PATH}}`
    /// (the file-based EDS path marker) drives `needs_eds == true`; one without
    /// it stays `false`. The gate is what keeps the new `{{EDS_PATH}}`
    /// shared-template + `{{EDS_BACKEND_IP}}` + host-gateway-IP-discovery
    /// machinery entirely inert for fixtures 0001-0028, mirroring the existing
    /// `needs_cds`/`needs_lds`/`needs_rds` detection (the
    /// `contains("{{EDS_PATH}}")` check `run_fixture` uses).
    #[test]
    fn eds_marker_scan_detects_eds_path_token() {
        // A main template (either side) that references `{{EDS_PATH}}` sets the
        // need. Mirrors `run_fixture`'s `needs_eds` disjunction over the two
        // main templates.
        let with_eds = "  eds_config:\n    path_config_source:\n      path: {{EDS_PATH}}";
        let without_eds = "  load_assignment:\n    cluster_name: c1";

        let needs_eds_upstream =
            with_eds.contains("{{EDS_PATH}}") || without_eds.contains("{{EDS_PATH}}");
        assert!(
            needs_eds_upstream,
            "a template carrying {{{{EDS_PATH}}}} must set needs_eds == true",
        );

        let needs_eds_neither =
            without_eds.contains("{{EDS_PATH}}") || without_eds.contains("{{EDS_PATH}}");
        assert!(
            !needs_eds_neither,
            "templates with no {{{{EDS_PATH}}}} must leave needs_eds == false (fixtures 0001-0028 inert)",
        );
    }

    /// The host-gateway-IP discovery returns a NUMERIC IPv4 string. EDS rejects
    /// hostnames (L1), so the EDS file's endpoint address must be a numeric IP —
    /// discovered at runtime by running `getent hosts host.docker.internal`
    /// inside a throwaway pinned-Envoy container with the host-gateway mapping.
    /// Docker-gated per the existing `#[ignore = "requires Docker; ..."]`
    /// discipline (e.g. `upstream::tests::starts_upstream_envoy_and_exposes_host_port`);
    /// under `cargo test --workspace` in CI / locally where Docker is available
    /// it runs and asserts the result parses as `std::net::Ipv4Addr`.
    #[test]
    #[ignore = "requires Docker; runs under `cargo test --workspace` in CI"]
    fn discover_host_gateway_ip_returns_numeric_ipv4() {
        let ip = discover_host_gateway_ip().expect("discovery must succeed when Docker is present");
        ip.parse::<std::net::Ipv4Addr>().unwrap_or_else(|e| {
            panic!("discovered host-gateway IP {ip:?} must be numeric IPv4: {e}")
        });
    }

    // ---- 27 Task 6 (D6 / §6.2-LOCKED V2): EDS hot-reload step (local unit
    // tests). The Docker-gated end-to-end reload differential is
    // native-Linux-CI-authoritative (Task 7's fixture 0035 — under Docker
    // Desktop virtiofs the Envoy-side reload is NOT locally observable); here we
    // lock in the schema (mirroring `Driver::Http1RdsReload`'s round-trip), the
    // `default_reload_file` default (`eds-reload.yaml`), the per-side render of
    // an EDS-reload template, and the M26-2 discriminator-guard `bail!`. ----

    /// A `Driver::Http1EdsReload` expectations YAML round-trips through the
    /// snake_case-tagged serde representation. The `reload.reload_file` key is
    /// OMITTED to exercise the `default_eds_reload_file` default
    /// (`eds-reload.yaml`); the discriminator probe (a body marker discriminator
    /// here — `backend_2` after the swap) parses into the expected
    /// `EdsReloadStep`. Mirrors `driver_http1_rds_reload_round_trips_through_serde`.
    #[test]
    fn driver_http1_eds_reload_round_trips_through_serde() {
        let yaml = r#"
driver:
  kind: http1_eds_reload
  pre_probes:
    - name: pre-backend-1
      method: get
      path: /probe
      host: eds_backend
      expected_status: 200
      expected_body:
        kind: byte_exact
        body: "backend: backend_1\nmethod: GET\npath: /probe\nheaders:\n  ...\nbody: \n"
  reload:
    settle_budget_ms: 2000
    discriminator:
      name: discriminator
      method: get
      path: /probe
      host: eds_backend
      expected_body:
        kind: byte_exact
        body: "backend: backend_2\nmethod: GET\npath: /probe\nheaders:\n  ...\nbody: \n"
  post_probes:
    - name: post-backend-2
      method: get
      path: /probe
      host: eds_backend
      expected_status: 200
"#;
        let exp: crate::Expectations = serde_yaml::from_str(yaml).expect("yaml parses");
        let Driver::Http1EdsReload {
            pre_probes,
            reload,
            post_probes,
        } = exp.driver
        else {
            panic!("expected Driver::Http1EdsReload");
        };
        // reload_file omitted ⇒ default applied.
        assert_eq!(reload.reload_file, "eds-reload.yaml");
        assert_eq!(reload.settle_budget_ms, 2000);
        assert_eq!(reload.discriminator.name, "discriminator");
        // The discriminator carries an expected_body (the post-swap marker) —
        // so the M26-2 both-None guard does NOT trip for it.
        assert!(reload.discriminator.expected_body.is_some());
        assert_eq!(pre_probes.len(), 1);
        assert_eq!(pre_probes[0].name, "pre-backend-1");
        assert_eq!(post_probes.len(), 1);
        assert_eq!(post_probes[0].name, "post-backend-2");
    }

    /// `#[serde(deny_unknown_fields)]` on `EdsReloadStep` rejects a stray key —
    /// the schema is locked (mirrors the RDS step's strictness).
    #[test]
    fn eds_reload_step_rejects_unknown_field() {
        let yaml = r#"
driver:
  kind: http1_eds_reload
  pre_probes: []
  reload:
    settle_budget_ms: 2000
    bogus_key: 1
    discriminator:
      name: d
      method: get
      path: /probe
      host: eds_backend
      expected_status: 200
  post_probes: []
"#;
        let res: Result<crate::Expectations, _> = serde_yaml::from_str(yaml);
        assert!(
            res.is_err(),
            "deny_unknown_fields must reject the stray `bogus_key`",
        );
    }

    /// The POST-reload EDS template renders per-side exactly like `eds.yaml` —
    /// the upstream (container-perspective) kv map resolves `{{EDS_BACKEND_IP}}`
    /// to the numeric host-gateway IP, the subject (host-perspective) kv map to
    /// `127.0.0.1`, and `{{HTTP1_BACKEND_2_PORT}}` swaps the endpoint to
    /// backend_2. Mirrors `rds_reload_template_renders_per_side`.
    #[test]
    fn eds_reload_template_renders_per_side_and_swaps_backend() {
        let reload_template =
            "endpoint:\n  address: {{EDS_BACKEND_IP}}\n  port: {{HTTP1_BACKEND_2_PORT}}";
        let upstream_reload = crate::render_yaml(
            reload_template,
            &[
                ("EDS_BACKEND_IP", "172.17.0.1"),
                ("HTTP1_BACKEND_2_PORT", "54322"),
            ],
        );
        let subject_reload = crate::render_yaml(
            reload_template,
            &[
                ("EDS_BACKEND_IP", "127.0.0.1"),
                ("HTTP1_BACKEND_2_PORT", "54322"),
            ],
        );
        assert!(upstream_reload.contains("address: 172.17.0.1"));
        assert!(subject_reload.contains("address: 127.0.0.1"));
        // Both renditions point at backend_2's port (the swapped endpoint).
        assert!(upstream_reload.contains("port: 54322"));
        assert!(subject_reload.contains("port: 54322"));
        assert_ne!(upstream_reload, subject_reload);
    }

    /// The M26-2 spurious-convergence guard: a reload discriminator with NEITHER
    /// `expected_status` NOR `expected_body` would make `wait_for_reload_convergence`
    /// return Ok on the FIRST poll (`status_ok && body_ok == true` with both
    /// expectations absent), reporting instant "convergence" before any reload
    /// took effect — the phase-26 M26-2 trap. `eds_reload_discriminator_is_load_bearing`
    /// is the guard the `Http1EdsReload` dispatch arm calls at its START to
    /// `bail!` on such a discriminator (folding the fix the RDS arm omitted).
    #[test]
    fn eds_reload_discriminator_both_none_is_rejected() {
        // both None ⇒ NOT load-bearing ⇒ the dispatch arm must bail!.
        let both_none = crate::Http1Probe {
            name: "d".to_string(),
            method: crate::Http1Method::Get,
            path: "/probe".to_string(),
            host: "eds_backend".to_string(),
            extra_headers: vec![],
            body: None,
            expected_status: None,
            expected_body: None,
            expected_headers: None,
        };
        assert!(
            !crate::eds_reload_discriminator_is_load_bearing(&both_none),
            "a both-None discriminator must be rejected (M26-2 spurious convergence)",
        );

        // An expected_status alone is load-bearing.
        let status_only = crate::Http1Probe {
            expected_status: Some(200),
            ..both_none.clone()
        };
        assert!(crate::eds_reload_discriminator_is_load_bearing(
            &status_only
        ));

        // An expected_body alone is load-bearing (the body-marker swap path).
        let body_only = crate::Http1Probe {
            expected_body: Some(crate::Http1BodyRule::ByteExact {
                body: "backend: backend_2\n".to_string(),
            }),
            ..both_none
        };
        assert!(crate::eds_reload_discriminator_is_load_bearing(&body_only));
    }
}

/// Task 4 (CI flake-fix): parser for the admin `/stats` cluster warm-up gate.
/// Placed after `eds_harness_tests` per the per-task placement convention.
#[cfg(test)]
mod xds_warmup_tests {
    use super::clusters_warm_from_stats_text;

    #[test]
    fn clusters_warm_requires_at_least_one_cluster() {
        assert!(!clusters_warm_from_stats_text("server.live: 1\n"));
    }

    #[test]
    fn clusters_warm_false_when_any_membership_unhealthy() {
        let s = "cluster.a.membership_healthy: 1\ncluster.b.membership_healthy: 0\n";
        assert!(!clusters_warm_from_stats_text(s));
    }

    #[test]
    fn clusters_warm_true_when_all_memberships_healthy() {
        let s =
            "cluster.a.membership_healthy: 1\ncluster.b.membership_healthy: 2\nserver.live: 1\n";
        assert!(clusters_warm_from_stats_text(s));
    }
}

#[cfg(test)]
mod drive_http1_body_tests {
    use super::{Http1Method, drive_http1};

    #[tokio::test]
    async fn drive_http1_sends_request_body() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let recorded = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let rec = recorded.clone();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            loop {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(200),
                    sock.read(&mut buf),
                )
                .await
                {
                    Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                    Ok(Ok(n)) => rec.lock().unwrap().extend_from_slice(&buf[..n]),
                }
            }
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await;
            let _ = sock.shutdown().await;
        });
        drive_http1(addr, &Http1Method::Post, "/", "x.test", &[], Some(b"hello"))
            .await
            .expect("drive_http1 must succeed");
        let got = String::from_utf8_lossy(&recorded.lock().unwrap()).into_owned();
        assert!(
            got.contains("Content-Length: 5"),
            "driver set content-length: {got}"
        );
        assert!(got.ends_with("hello"), "driver appended the body: {got}");
    }
}
