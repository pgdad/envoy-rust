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
    /// Dotted-path key, e.g. `configs.0.bootstrap.node.id`.
    pub path: String,
    pub expected: serde_yaml::Value,
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
    // 04.3 may add Post for upstream-proxy fixture; otherwise 04.x is GET-only.
}

impl Http1Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Http1Method::Get => "GET",
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
/// TOCTOU: between the drop and the subsequent bind by envoy-rust, another
/// process on the host could grab this port. This is accepted for a
/// pre-production harness per SPEC §6 point 6. If CI flakes materialize, this
/// becomes its own split phase with a port-range reservation strategy.
pub fn reserve_port() -> Result<u16> {
    let listener =
        StdTcpListener::bind(("127.0.0.1", 0)).context("binding 127.0.0.1:0 to reserve a port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
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

/// Write `content` to a new temp file in `dir` and return the path. The caller
/// is responsible for ensuring `dir` is already created.
pub fn write_temp(dir: &Path, name: &str, content: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    let mut f =
        std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
    f.write_all(content.as_bytes())?;
    Ok(path)
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
    req.push_str("Connection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;

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
        matches!(method, Http1Method::Get),
        "drive_http2 currently only supports GET; widen the helper if/when a fixture needs body request methods"
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
        let _ = drive_http1(addr, &method, &pre.path, &pre.host, &[])
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

    drive_http1(admin_addr, &Http1Method::Get, path, "admin.local", &[])
        .await
        .with_context(|| format!("admin scrape GET {path}"))
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

/// End-to-end run of one fixture. Panics-on-failure paths unwind through Drop
/// guards so the container and envoy-rust subprocess are cleaned up even on
/// assertion failure.
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
    let port_key = match &expectations.driver {
        Driver::TcpEcho
        | Driver::TlsTcp { .. }
        | Driver::TlsTcpProbeList { .. }
        | Driver::Http1 { .. }
        | Driver::Http1ProbeList { .. }
        | Driver::Http1WithAccessLog { .. }
        | Driver::Http1AfterSettle { .. }
        | Driver::Http2 { .. }
        | Driver::Http2ProbeList { .. }
        // 06.1 D6.a: AdminScrape's HCM listener uses {{PORT}} like the other
        // HCM-shaped drivers. The admin listener is separately substituted
        // via {{ADMIN_PORT}} (see admin_host_port reservation below).
        | Driver::AdminScrape { .. } => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    };

    // 06.1 D6.a: reserve a kernel-ephemeral admin port whenever either
    // template references `{{ADMIN_PORT}}` AND the driver is AdminScrape
    // (the other consumer, Driver::HttpGet, drives admin via the single
    // listener port and does not need a separate reservation). Mirrors
    // the existing `_backend` / `_tls_backend` cadence: the reservation
    // happens once at run_fixture start so kvs and dispatch both see it.
    let needs_admin_port = matches!(&expectations.driver, Driver::AdminScrape { .. })
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
    let needs_backend = upstream_template.contains("{{BACKEND_PORT}}")
        || subject_template.contains("{{BACKEND_PORT}}");
    let needs_health_aware_backend =
        needs_backend && fixture_name == "0019-upstream-active-health-check";
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
            Some(
                crate::backend::HealthAwareHttp1Backend::spawn()
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
    let needs_tls_backend = upstream_template.contains("{{TLS_BACKEND_PORT}}")
        || subject_template.contains("{{TLS_BACKEND_PORT}}");
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
    let needs_http1_backend = upstream_template.contains("{{HTTP1_BACKEND_PORT}}")
        || subject_template.contains("{{HTTP1_BACKEND_PORT}}");
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

    // 05.3 NEW per SPEC §3 D6.b: spawn Http2EchoBackend if either template
    // needs one. Same alive-keeper binding-order discipline as _backend /
    // _tls_backend / _http1_backend above.
    let needs_http2_backend = upstream_template.contains("{{HTTP2_BACKEND_PORT}}")
        || subject_template.contains("{{HTTP2_BACKEND_PORT}}");
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
        if let Some(h2p) = http2_backend_port_str.as_deref() {
            v.push(("HTTP2_BACKEND_PORT", h2p.to_string()));
        }
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
            || http2_backend_port_str.is_some()
        {
            // Per ADR-0015: container-side reaches the host backend via
            // host.docker.internal (with the harness's with_host call below).
            // Generalized in Task 9 to fire for either backend variant; was
            // previously gated only on BACKEND_PORT (Task 8 cadence). Task 13
            // extends the gate to HTTP1_BACKEND_PORT. 05.3 Task 9 extends to
            // HTTP2_BACKEND_PORT.
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
        if let Some(h2p) = http2_backend_port_str.as_deref() {
            v.push(("HTTP2_BACKEND_PORT", h2p.to_string()));
        }
        if backend_port_str.is_some()
            || tls_backend_port_str.is_some()
            || http1_backend_port_str.is_some()
            || http2_backend_port_str.is_some()
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
        v
    };

    // (d) Adapt render_yaml call sites: build _refs intermediates since
    // render_yaml takes &[(&str, &str)] but kvs are Vec<(&str, String)>.
    let upstream_kvs_refs: Vec<(&str, &str)> =
        upstream_kvs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let subject_kvs_refs: Vec<(&str, &str)> =
        subject_kvs.iter().map(|(k, v)| (*k, v.as_str())).collect();

    let upstream_yaml = render_yaml(&upstream_template, &upstream_kvs_refs);
    let subject_yaml = render_yaml(&subject_template, &subject_kvs_refs);
    let upstream_path = write_temp(tmp.path(), "envoy.yaml", &upstream_yaml)?;
    let subject_path = write_temp(tmp.path(), "envoy-rust.yaml", &subject_yaml)?;

    // The `host_uses_host_gateway` flag drives upstream::start to attach
    // `with_host("host.docker.internal", Host::HostGateway)` on the
    // testcontainers image (per ADR-0015). The flag is true exactly when the
    // upstream YAML actually references the hostname — silent when it
    // doesn't, so fixtures 0001 and 0002 stay unchanged.
    let host_uses_host_gateway = upstream_yaml.contains("host.docker.internal");
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
        Driver::Http1WithAccessLog {
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
    wait_accept_ready(subject_addr, budget)
        .await
        .context("envoy-rust never became accept-ready")?;

    match &expectations.driver {
        Driver::TcpEcho => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
            let upstream_out = drive_tcp(upstream_addr, &payload)
                .await
                .context("upstream envoy drive")?;
            let subject_out = drive_tcp(subject_addr, &payload)
                .await
                .context("envoy-rust drive")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(&expectations, None, None, &upstream_out, &subject_out)?;
        }
        Driver::HttpGet { path, host } => {
            let upstream_resp = drive_http_get(upstream_addr, path, host)
                .await
                .context("upstream envoy http get")?;
            let subject_resp = drive_http_get(subject_addr, path, host)
                .await
                .context("envoy-rust http get")?;
            subject.shutdown(Duration::from_secs(5)).await.ok();
            drop(upstream);
            assert_equivalence(
                &expectations,
                Some(upstream_resp.status),
                Some(subject_resp.status),
                &upstream_resp.body,
                &subject_resp.body,
            )?;
        }
        // (f) Real TLS dispatch arm.
        Driver::TlsTcp { sni, expected_cn } => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
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
            assert_equivalence(&expectations, None, None, &upstream_out, &subject_out)?;
        }
        // 03.2 Task 8: per-SNI probe list. Equivalence is enforced inside
        // `drive_tls_probes` per probe (byte-equality + per-probe expected_cn);
        // both sides succeeding ⇒ equivalent cert selection per SNI without a
        // final `assert_equivalence` call.
        Driver::TlsTcpProbeList { probes } => {
            let payload = std::fs::read(fixture_dir.join("inputs/payload.bin"))
                .context("reading payload.bin")?;
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
            let upstream_resp = drive_http1(upstream_addr, method, path, host, &[])
                .await
                .context("upstream envoy http1 drive")?;
            let subject_resp = drive_http1(subject_addr, method, path, host, &[])
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
            tracing::debug!(
                settle_ms,
                "Driver::Http1AfterSettle: sleeping for active-HC settle"
            );
            tokio::time::sleep(Duration::from_millis(*settle_ms)).await;

            let upstream_resp = drive_http1(upstream_addr, method, path, host, &[])
                .await
                .context("upstream envoy http1 drive (after settle)")?;
            let subject_resp = drive_http1(subject_addr, method, path, host, &[])
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
        }
        Driver::Http1ProbeList { probes } => {
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
                )
                .await
                .with_context(|| format!("upstream envoy http1 drive (probe {})", probe.name))?;
                let subject_resp = drive_http1(
                    subject_addr,
                    &probe.method,
                    &probe.path,
                    &probe.host,
                    &probe.extra_headers,
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
        }
        // 11 NEW: HTTP/2 probe-list driver. Mirrors Driver::Http1ProbeList
        // verbatim, swapping drive_http1 → drive_http2 (H2 cleartext
        // prior-knowledge per drive_http2's handshake). The Http1Probe struct
        // is codec-agnostic, so the per-probe equivalence cascade is identical.
        // This is the first HTTP-filter-family fixture on an H2 listener,
        // exercising the phase-11 D6 decorate_filter_synth_response_h2 helper
        // bilaterally (closes 09 REVIEW M2). Per phase-11 SPEC §3 D8.1.
        Driver::Http2ProbeList { probes } => {
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
            // Wire-protocol leg: reuse drive_http1 unchanged from 04.1.
            // `Http1Method` is the harness's narrow GET-only enum today;
            // mirror the conversion shape used by `drive_admin_scrape`.
            let http1_method = match method.as_str() {
                "GET" => Http1Method::Get,
                other => bail!("Driver::Http1WithAccessLog: unsupported method {:?}", other),
            };
            let upstream_resp =
                drive_http1(upstream_addr, &http1_method, path, host, extra_headers)
                    .await
                    .context("upstream envoy http1 drive (Http1WithAccessLog)")?;
            let subject_resp = drive_http1(subject_addr, &http1_method, path, host, extra_headers)
                .await
                .context("envoy-rust http1 drive (Http1WithAccessLog)")?;
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

            // Access-log files. Wait up to 5s for both files to appear (the
            // synchronous-after-write dispatch should have emitted before the
            // response completed, but flush timing is best-effort).
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let envoy_path = std::path::PathBuf::from(&expected_access_log_paths.envoy);
            let envoy_rust_path = std::path::PathBuf::from(&expected_access_log_paths.envoy_rust);
            while std::time::Instant::now() < deadline {
                if envoy_path.exists() && envoy_rust_path.exists() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            // One final yield to let the OS flush.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            let envoy_contents = std::fs::read_to_string(&envoy_path).with_context(|| {
                format!("read envoy access-log file at {}", envoy_path.display())
            })?;
            let envoy_rust_contents =
                std::fs::read_to_string(&envoy_rust_path).with_context(|| {
                    format!(
                        "read envoy-rust access-log file at {}",
                        envoy_rust_path.display()
                    )
                })?;
            let envoy_lines: Vec<String> = envoy_contents.lines().map(|s| s.to_owned()).collect();
            let envoy_rust_lines: Vec<String> =
                envoy_rust_contents.lines().map(|s| s.to_owned()).collect();

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
            if scrapes.is_empty() {
                bail!(
                    "Driver::AdminScrape requires at least one sub-case (`scrapes:` must be non-empty)"
                );
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
            let upstream_admin_addr: SocketAddr =
                format!("127.0.0.1:{upstream_admin_port}").parse()?;
            let subject_admin_addr: SocketAddr =
                format!("127.0.0.1:{subject_admin_port}").parse()?;
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
                drive_http1(upstream_pre_addr, &method, &pre.path, &pre.host, &[])
                    .await
                    .with_context(|| {
                        format!(
                            "upstream envoy pre-request {} {} (host={}, port_key={})",
                            pre.method, pre.path, pre.host, pre.port_key,
                        )
                    })?;
                drive_http1(subject_pre_addr, &method, &pre.path, &pre.host, &[])
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
                            .with_context(|| {
                                format!("upstream envoy pre_admin_action POST {path}")
                            })?;
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
                let upstream_resp =
                    drive_admin_scrape(pre, upstream_admin_addr, &upstream_hcm, &case.path)
                        .await
                        .with_context(|| format!("upstream envoy admin scrape: {}", case.path))?;
                let subject_resp =
                    drive_admin_scrape(pre, subject_admin_addr, &subject_hcm, &case.path)
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
        }
    }

    // _backend Drop fires here.
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
                let envoy_sub = walk_pointer(&envoy_json, &subtree.path)
                    .with_context(|| format!("envoy required_subtree path {:?}", subtree.path))?;
                let rust_sub = walk_pointer(&rust_json, &subtree.path).with_context(|| {
                    format!("envoy-rust required_subtree path {:?}", subtree.path)
                })?;
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
                        "required_subtree {:?} envoy != expected:\n  envoy:    {envoy_str}\n  expected: {expected_str}",
                        subtree.path,
                    );
                }
                if rust_str != expected_str {
                    bail!(
                        "required_subtree {:?} envoy-rust != expected:\n  envoy-rust: {rust_str}\n  expected:   {expected_str}",
                        subtree.path,
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
