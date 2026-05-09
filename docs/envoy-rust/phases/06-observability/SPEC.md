# Phase 06 — Observability foundations: access log (file sink, Envoy default format) + stats (counters + gauges) + admin endpoint with Prometheus exposition

- **Phase id:** `06`
- **Slug:** `06-observability`
- **Title:** Observability foundations — first end-to-end emission of access logs (file sink, Envoy default format), of stats (counter + gauge primitives, hierarchical name tree, no histograms), and of a Prometheus-format admin scrape endpoint, plus the migration of phase-01's bare-bones admin handler to a real HCM-backed admin listener that can serve multiple endpoints
- **Depends on:** `05` (HTTP/2 cleartext data plane). Phase 05 ROADMAP row is `done` as of commit `82c26b8` (the parent-05 state-6 close-out, which also flipped sub-phase row `05.3` from `in-progress` to `done` per the ROADMAP-schema invariant in §4.1 of `BOOTSTRAP_PROMPT.md`). Phase 06 enters `in-progress` at this state-1 close-out commit.
- **Seeded by:** `BOOTSTRAP_PROMPT.md` §8 row 06 — *"Access log (file sink, Envoy default format) + stats + Prometheus admin endpoint"* with the differential surface gate *"access log + Prometheus fixtures green"*. Doctrine `D-3.2` lists *Access log formatters and sinks*, *Stats subsystem*, and *Admin API* on its **Must be written from scratch** list — none of `prometheus`, `prometheus-client`, `metrics`, or `metrics-exporter-prometheus` are permitted as direct deps. Counter / gauge primitives, the Prometheus exposition format emitter, and the admin HTTP handler are hand-rolled atop the existing `tokio` + `bytes` + `tracing` + `thiserror` + `serde_yaml` foundations.
- **Differential surface when done:**
  - **Pre-existing fixtures:** `0001-tcp-echo`, `0002-static-admin-ready`, `0003-tcp-proxy`, `0004-tls-downstream`, `0005-tls-upstream`, `0006-tls-sni`, `0007-http1-direct-response`, `0008-http1-router-upstream`, `0009-http2-direct-response`, `0010-http2-router-upstream` — all 10 stay green at the Docker-gated CI level. Fixture `0002-static-admin-ready` continues to exercise the admin `/ready` endpoint; phase 06's admin migration preserves its semantics byte-equivalent (the `/ready` 200-on-server-ready response is reachable via the new HCM-backed admin listener; PROGRESS.md at the migration commit quotes the test result inline).
  - **New fixtures:** `tests/fixtures/0011-admin-stats-prometheus/` (admin `/stats/prometheus` scrape returns Prometheus exposition with the names emitted by 06.1 + 06.3, semantically equivalent across the two proxies modulo allow-list — see §2 below) and `tests/fixtures/0012-access-log-file-sink/` (HCM emits one access-log line per request to a file sink; both proxies' files are diffed semantically per the access-log field-mapping rule populated in BEHAVIOR_CONTRACT.md at 06.2).
  - **Conformance suites unchanged:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D10. Phase 06 does not edit the runner or the gate semantics by default; if any phase-06 task surfaces new H2-framing impact, the planner re-runs h2spec at the relevant state-4 verification.
- **Sub-phases:** **`06.1`, `06.2`, `06.3`** projected (codified at parent-06 state-2 via **ADR-0029** — see §7).

This SPEC is the design contract for the parent phase 06. It projects the split into three sub-phases by surface boundary (stats foundation + admin migration + Prometheus exposition → access-log foundation + HCM wiring + file-sink fixture → comprehensive stats wiring + parent-06 close). The 3-way split mirrors phase-04's precedent under ADR-0020 and phase-05's precedent under ADR-0022, and was selected over a 2-way split for the same drift-headroom reason phase-05 cited: a 2-way split's larger sub-phase (~2000 LoC) would exceed the §6.1 split-gate's ~1500 LoC threshold once execution-time drift is factored in (phase-04.3's experience showed ~+20% drift between brainstorm-time estimates and landed LoC).

This SPEC is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-05 surface (via `git log` and the in-tree `envoy-{bin,cluster,config,http1,http2,listener,tcp,tls}` shape at HEAD `82c26b8`) must be able to operate as the parent-06 state-2 session — landing **ADR-0029** (split decision), the three sub-phase SPECs, and the ROADMAP rows for `06.1`, `06.2`, `06.3`. Each sub-phase then enters its own state-1 brainstorm cadence with its own SPEC.

---

## 1. Goal and acceptance signal

**Goal.** Land observability foundations on three surfaces in three coordinated sub-phases. Across all three, the architectural rule is **`envoy-stats`, `envoy-accesslog`, and `envoy-admin` each own a single subsystem cleanly**, mirroring the established pattern: `envoy-http1` is the sole owner of `httparse` (parent-04 SPEC §3 cross-sub-phase rule 1; established in 04.1), `envoy-http2` is the sole owner of `h2` (parent-05 SPEC §3 cross-sub-phase rule 1; established in 05.2), `envoy-tls` is the sole owner of `rustls` (phase-03 precedent). Phase 06 introduces no new permitted-foundation deps: counters/gauges hand-rolled on `std::sync::atomic::{AtomicU64, AtomicI64}`; Prometheus exposition emitter hand-rolled (~30 LoC of formatter); access-log default-format emitter hand-rolled with hand-rolled ISO-8601 timestamp emission from `std::time::SystemTime` (~50 LoC including a Gregorian calendar arithmetic helper with golden tests).

1. **Stats foundation + admin listener migration + Prometheus exposition** (sub-phase **06.1**). New workspace member `crates/envoy-stats/` (sole-dep-owner of any stats deps; phase 06 introduces none). Public surface: a `Counter` (`AtomicU64` increment-only), a `Gauge` (`AtomicI64` set/inc/dec), a `StatsRegistry` (a single `Arc<StatsRegistry>` per `envoy-bin` process, threaded through HCM/router/listener/cluster constructors via DI), and a `prometheus::write_exposition` function that emits the registry contents as Prometheus exposition format (text-based; `# HELP`, `# TYPE`, name-value triples). New workspace member `crates/envoy-admin/` (sole-dep-owner of admin-listener wiring; depends on `envoy-http1` for HCM dispatch + `envoy-stats` for the registry read; provides the admin HTTP handler that maps URL paths to handlers). The migration of phase-01's bare-bones `/ready` handler at `crates/envoy-bin/src/main.rs` to an HCM-backed admin listener serving an admin route table (`/ready` → 200; `/stats` → text/plain stats dump; `/stats/prometheus` → Prometheus exposition) is in 06.1 — the bare-bones approach can't grow to support multiple endpoints cleanly, and the HCM-backed approach reuses 04.1's HCM machinery + 04.2's matchers. **Stats wiring depth is "representative" in 06.1**: one counter per layer to demonstrate the mechanism end-to-end (`listener.<name>.downstream_cx_total`, `cluster.<name>.upstream_cx_total`, `http.<stat_prefix>.downstream_rq_total`). Fixture `0011-admin-stats-prometheus` proves the admin `/stats/prometheus` scrape works end-to-end. The admin migration preserves fixture `0002-static-admin-ready`'s `/ready` semantics byte-equivalent against upstream Envoy (regression-guarded by D3.1's in-process backstop test plus the existing Docker-gated `0002` fixture).

2. **Access-log foundation + HCM wiring + file sink** (sub-phase **06.2**). New workspace member `crates/envoy-accesslog/` (sole-dep-owner of any access-log deps; phase 06 introduces none). Public surface: an `AccessLogRecord` struct (request + response + timing + upstream-host fields populated by the HCM at on-response-complete time), a `Sink` trait, a `FileSink` impl (tokio `fs::OpenOptions::append(true)` + `AsyncWriteExt::write_all`; one line per record terminated by `\n`), and a `default_format::format(record) -> String` emitter that renders Envoy's documented default format. **Format-string customization is OUT of scope in 06.2** — only the fixed default format ships; format-string parsing (`%REQ(:METHOD)%`, `%START_TIME(%Y-%m-%dT%T.%3fZ)%`, etc.) defers to a later observability-family phase. Schema additions in `envoy-config`: `access_log:` block on `HttpConnectionManagerConfig` (an array of `AccessLog` entries, each with `name: "envoy.access_loggers.file"`, `typed_config: { @type: ".../v3.FileAccessLog", path: "/path/to/log" }`); the validator accepts only the file-sink `@type` URL in 06.2 and rejects others with a typed `ConfigError::UnsupportedAccessLogType`. HCM grows an on-response-complete hook that builds an `AccessLogRecord` and dispatches it to the configured sinks. Fixture `0012-access-log-file-sink` proves an HCM round-trip emits a file-sink line semantically equivalent across both proxies (modulo wall-clock divergence in `%START_TIME%` and `%DURATION%` — see §2 below for the BEHAVIOR_CONTRACT.md `Access log field mapping` section that populates here for the first time).

3. **Comprehensive stats wiring + parent-06 close** (sub-phase **06.3**). Wires the standard Envoy stat tree at HCM/router/listener/cluster sites — beyond 06.1's representative subset. Includes per-response-class counters (`http.<prefix>.downstream_rq_2xx`, `..._3xx`, `..._4xx`, `..._5xx`), connection lifetime gauges (`listener.<name>.downstream_cx_active`, `cluster.<name>.upstream_cx_active`), upstream-side counters (`cluster.<name>.upstream_rq_total`, `..._upstream_rq_5xx`), and access-log line counters (`http.<prefix>.access_logs_total`). Fixture 0011's `expectations.yaml` is extended in 06.3 to assert the comprehensive set. Opportunistically closes the **05.3 REVIEW I1 carryforward** (Http2ClusterFromHttp1Listener parse-time validator gate) as a Task-1 preamble, mirroring phase-05.1's posture toward phase-02.1 REVIEW I3. Parent ROADMAP row `06` flips `done` at 06.3's state-6 phase-done commit per the `e626862`-shape close-out (the last sub-phase commit also closes the parent in the same commit).

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to phase 06's full feature surface across all three sub-phases:

- **(a)** the new differential fixtures `tests/fixtures/0011-admin-stats-prometheus/` and `tests/fixtures/0012-access-log-file-sink/` are green at the Docker-gated CI level;
- **(b)** the pre-existing differential fixtures `0001` through `0010` are all green at the Docker-gated CI level (no regression on any earlier surface; the admin migration in 06.1 preserves fixture `0002-static-admin-ready`'s `/ready` semantics byte-equivalent against upstream Envoy);
- **(c)** the conformance suite `tests/conformance/h2spec/` continues to pass at **≥95%** with `known-failures.txt` unchanged (phase 06 does not engage H2-framing surfaces; if any task surfaces new framing impact, the planner re-runs h2spec at state-4 and updates the gate evidence accordingly);
- **(d)** the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 06.1 (≥1 new admin-listener-with-stats-route seed) + 06.2 (≥1 new HCM `access_log` block seed). No new fuzz target ships in phase 06;
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job;
- **(f)** all three sub-phase `REVIEW.md` verdicts are approved.

The parent-phase-done commit lands at the **last sub-phase's state-6 commit** (i.e., 06.3's phase-done commit also flips parent row `06` from `in-progress` to `done` — mirrors phase 04's `e626862` close-out and phase 05's `82c26b8` close-out).

---

## 2. Behavior-contract scope for phase 06

Phase 06 is the first phase to populate two long-empty subsections of `docs/envoy-rust/BEHAVIOR_CONTRACT.md`:

1. **`Stat-name mapping` section.** The section's standing comment (*"populated starting phase 06"*) is fulfilled in 06.1 and extended in 06.3. The default assumption per the section preamble is that stat names match Envoy's documented tree one-to-one; the table records mapping entries only when envoy-rust must produce a stat under a different internal label that needs to be projected back to the Envoy-canonical name at the stats sink. **Initial entries projected at 06.1**:
   - `listener.<name>.downstream_cx_total` — counter; one increment per accepted TCP connection on the listener; envoy-rust internal label matches one-to-one. Both proxies emit on every accept.
   - `cluster.<name>.upstream_cx_total` — counter; one increment per established upstream TCP connection at cluster-build / connection-pool acquire time. envoy-rust per-connection-per-call (no pooling per phase-04.3 / phase-05.3 posture) so the counter increments once per upstream call; Envoy with default pool may increment less frequently. **Disposition: divergence accepted** — Envoy's stat semantics are *"per-established-connection"* and envoy-rust's are *"per-call"* under the no-pooling regime; both are correct under their respective contracts. Fixture `0011`'s `expectations.yaml` asserts `name-required, value-may-differ` for this counter, with rationale recorded in BEHAVIOR_CONTRACT.md Section `Stat-name mapping`. When connection pooling lands (upstream-robustness family), the counter's semantics tighten to value-exact.
   - `http.<stat_prefix>.downstream_rq_total` — counter; one increment per HCM-handled request (any response code; any method). Both proxies emit on every request. **Disposition: value-exact** under deterministic load (the test fixture sends a fixed number of requests, both proxies see the same number, both counters land at the same value).
   
   **Extended entries at 06.3** (per-response-class counters, connection-lifetime gauges, upstream-rq counters): each new stat lands an entry with disposition `value-exact` if both proxies are deterministic on the metric, or `name-required, value-may-differ` if envoy-rust's emission-point semantics diverge from Envoy's (with rationale).

2. **`Access log field mapping` section.** The section's standing comment (*"populated in phase 06 when access logs first ship"*) is fulfilled in 06.2. The section maps each Envoy default-format token to its envoy-rust internal data source. **Initial entries projected at 06.2**, one row per token in the default format (the Envoy default format is a 14-token sequence — `%START_TIME%`, `%REQ(:METHOD)% %REQ(X-ENVOY-ORIGINAL-PATH?:PATH)% %PROTOCOL%`, `%RESPONSE_CODE%`, `%RESPONSE_FLAGS%`, `%BYTES_RECEIVED%`, `%BYTES_SENT%`, `%DURATION%`, `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%`, `%REQ(X-FORWARDED-FOR)%`, `%REQ(USER-AGENT)%`, `%REQ(X-REQUEST-ID)%`, `%REQ(:AUTHORITY)%`, `%UPSTREAM_HOST%`):
   - `%START_TIME%` — equivalence: `name-required, value-may-differ`; rationale: wall-clock non-determinism (both proxies stamp at slightly different instants); ISO-8601 emission format `YYYY-MM-DDTHH:MM:SS.sssZ`. Fixture 0012 asserts the field is present and parses-as-ISO-8601, but does not assert exact value.
   - `%DURATION%` — equivalence: `name-required, value-may-differ`; rationale: per-request wall-clock latency; values diverge by measurement.
   - `%REQ(:METHOD)%`, `%REQ(:AUTHORITY)%`, `%REQ(USER-AGENT)%`, `%REQ(X-FORWARDED-FOR)%`, `%REQ(X-REQUEST-ID)%`, `%PROTOCOL%`, `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` — equivalence: `value-exact` under deterministic harness input (the harness sends a fixed request; both proxies see identical request bytes; the access-log emission of each is value-exact).
   - `%RESPONSE_CODE%`, `%BYTES_RECEIVED%`, `%BYTES_SENT%` — equivalence: `value-exact` under deterministic load (small fixed request → small fixed response).
   - `%RESPONSE_FLAGS%` — equivalence: `value-exact` for the `-` (no-flags) case in 06.2's happy-path fixture; non-`-` flag combinations defer to whichever phase first surfaces them (e.g., a timeout or fault filter).
   - `%RESP(X-ENVOY-UPSTREAM-SERVICE-TIME)%` — equivalence: `name-required, value-may-differ`; rationale matches the existing `Header allow-list` entry for the same header (per phase-04.3 BEHAVIOR_CONTRACT.md row).
   - `%UPSTREAM_HOST%` — equivalence: `value-exact` for fixture 0012 (a fixed STRICT_DNS upstream resolves to a deterministic IP:port for both proxies).

3. **`Header allow-list` — minimal extensions in phase 06.** The admin-listener responses in 06.1 (Prometheus exposition responses; `/ready` responses) emit a `content-type` header; both proxies emit `text/plain; version=0.0.4; charset=utf-8` (Prometheus exposition standard) for `/stats/prometheus` and `text/plain` for `/ready`. No new allow-list entries are anticipated — the response headers come from envoy-rust's admin handler and Envoy's admin handler respectively, and both should match exactly under the Prometheus exposition standard. If empirical testing surfaces a divergence, BEHAVIOR_CONTRACT.md grows a row in lockstep with the in-code `HEADER_ALLOW_LIST` constant per the established phase-04.3 / phase-05.x posture.

4. **`xDS wire state machine` and `Timing tolerances` subsections — untouched.** Phase 06 does not engage xDS or timing-sensitive features.

---

## 3. Deliverables (organized by sub-phase)

This section enumerates the projected deliverables across the three sub-phases. Each sub-phase's own SPEC (written at parent-06 state-2 via the split commit per ADR-0029) will expand its own deliverables into the per-task PLAN cadence the project follows. Total LoC and task counts are first-order estimates; per phase-04.3's drift experience, the planner should expect ~+20% drift at execution time.

### Phase 06.1 — `envoy-stats` foundation + `envoy-admin` HCM-backed listener migration + Prometheus exposition + fixture 0011 (~1300 LoC, ~12 tasks)

**D1.1 — New library crate `crates/envoy-stats/`.** Added to root `Cargo.toml` `[workspace] members`. Cargo deps: `tokio = { version = "1", features = ["sync"] }` (for `Arc` re-export though `std::sync::Arc` likely suffices — the planner picks at task time), `bytes = "1"` (for `Bytes` in the Prometheus exposition emitter buffer), `tracing = "0.1"`, `thiserror = "2"`. **No new permitted-foundations grants** — counters use `std::sync::atomic::AtomicU64`, gauges use `std::sync::atomic::AtomicI64`, registry uses `std::sync::RwLock<HashMap<String, StatHandle>>` for the name-tree map. Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

  **Module decomposition** (final shape decided at 06.1 SPEC writeup time; this is the projection):
  ```
  crates/envoy-stats/src/
    lib.rs        // crate root: #![forbid(unsafe_code)]; public re-exports
    counter.rs    // Counter (AtomicU64 increment-only)
    gauge.rs      // Gauge (AtomicI64 set/inc/dec)
    registry.rs   // StatsRegistry (Arc-shared global registry)
    prometheus.rs // write_exposition emitter
    error.rs      // StatsError typed-error enum (registration failures, name validation)
  ```

  Public surface re-exported at `lib.rs`:
  ```rust
  pub mod counter;
  pub mod gauge;
  pub mod registry;
  pub mod prometheus;
  pub use counter::Counter;
  pub use gauge::Gauge;
  pub use registry::StatsRegistry;
  pub use error::StatsError;
  ```

  ~400 LoC impl + ~250 LoC unit tests (counter increment + multi-thread torture; gauge set/inc/dec + multi-thread torture; registry register-and-lookup + duplicate-name behavior; Prometheus emitter golden output for empty registry, single counter, single gauge, mixed counter+gauge; name validation per Prometheus naming conventions).

**D2.1 — New library crate `crates/envoy-admin/`.** Added to root `Cargo.toml` `[workspace] members`. Cargo deps: `tokio = { version = "1", features = ["net", "io-util", "macros", "sync"] }`, `bytes = "1"`, `thiserror = "2"`, `tracing = "0.1"`, `envoy-config = { path = "../envoy-config" }`, `envoy-http1 = { path = "../envoy-http1" }` (for HCM dispatch — admin endpoints are HTTP/1.1 only in 06.1; H2 admin defers), `envoy-stats = { path = "../envoy-stats" }` (to read the registry for `/stats` and `/stats/prometheus`), `envoy-listener = { path = "../envoy-listener" }`. Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

  **Public surface:**
  ```rust
  pub struct AdminConfig {
      pub address: SocketAddr,         // listener bind address
      pub access_log_path: Option<PathBuf>, // admin-side access log; OUT OF SCOPE in 06.1, parsed-and-ignored
  }

  pub struct AdminHandler {
      // wraps envoy_http1::HCMConfig with a route table mapping admin paths
      // to AdminEndpoint handlers; constructed from AdminConfig + the shared
      // Arc<StatsRegistry>; usable as a ConnectionHandler at the listener.
  }

  pub enum AdminEndpoint {
      Ready,                            // GET /ready -> 200 "LIVE\n" or 503 "PRE_INITIALIZING\n"
      Stats,                            // GET /stats -> text/plain dump
      StatsPrometheus,                  // GET /stats/prometheus -> Prometheus exposition
  }
  ```

  The `AdminHandler` is constructed once per `envoy-bin` startup; it shares the `Arc<StatsRegistry>` with the data-path subsystems (listener, cluster, HCM). The handler's `handle_request(req: envoy_http1::Request) -> envoy_http1::Response` dispatches by exact path match (admin endpoints are exact-match only in 06.1; prefix-match defers). Each endpoint's response is a synthesized `envoy_http1::Response` value that the existing `envoy-http1` HCM machinery serializes to the wire.

  ~350 LoC impl + ~200 LoC unit tests (per-endpoint handler tests; admin listener accept-and-route smoke test; missing-path 404; non-GET method 405).

**D3.1 — Phase-01 admin migration in `crates/envoy-bin/`.** The bare-bones `/ready` handler at the existing admin-bind site in `crates/envoy-bin/src/main.rs` is replaced by an HCM-backed admin listener constructed from `envoy_admin::AdminHandler`. The migration:
  1. Adds an `Admin` arm to the listener-walk in `crates/envoy-bin/src/main.rs` (sibling of the `TcpProxy` and `HttpConnectionManager` arms; specific to the bootstrap-config `admin:` block).
  2. The admin listener binds at `bootstrap.admin.address.socket_address.address:port` (existing parse from phase 01).
  3. The admin listener's `ConnectionHandler` is `envoy_admin::AdminHandler`, constructed once at `envoy-bin` startup with the global `Arc<StatsRegistry>` injected.
  4. **Regression-guard test:** an in-process integration test at `crates/envoy-bin/tests/admin_ready.rs` (sibling of phase-04 / 05's in-process backstops) spawns envoy-bin against the existing fixture-0002-style admin config, drives a `GET /ready` HTTP/1.1 request, and asserts the 200 / "LIVE\n" response — proving the migration preserves phase-01's `/ready` semantics.
  5. `tests/fixtures/0002-static-admin-ready/` is unchanged at the YAML level — both proxies' admin emission shapes were already aligned in phase 01; the migration is internal to envoy-bin.

  ~150 LoC + the in-process backstop.

**D4.1 — Stats wiring "representative" subset.**
  - **Listener-side:** `envoy-listener` gains a `Listener.cx_total: Arc<Counter>` field set at construct time from the registry; the accept loop increments on every accepted TCP connection. ~30 LoC + 1 unit test.
  - **Cluster-side:** `envoy-cluster` gains a `Cluster.cx_total: Arc<Counter>` field set at construct time; the cluster's `pick_endpoint` site (or wherever per-call connection establishment lands per phase-04.3 / 05.3 posture) increments on every upstream connect. ~30 LoC + 1 unit test.
  - **HCM-side:** `envoy-http1::HCMConfig` and `envoy-http2::HCMConfig` (the latter is a type alias to the former per phase 05.2) gain a `stats: Arc<HCMStats>` field; the HCM increments `downstream_rq_total` on every request. The `stat_prefix` config field on `HttpConnectionManagerConfig` (Envoy-canonical name `stat_prefix`) is parsed at config-load time and used to namespace the emitted stats. ~50 LoC + 2 unit tests.
  - **Admin-side:** the admin handler reads the registry on `/stats` and `/stats/prometheus` requests; no new counters land in 06.1 for the admin handler itself.

  ~200 LoC across `envoy-listener` / `envoy-cluster` / `envoy-http1` / `envoy-http2` (the HCMConfig alias means the H2 path inherits the stats wiring without a separate edit).

**D5.1 — `envoy-config` schema additions for phase 06 admin.** `crates/envoy-config/src/bootstrap.rs::Admin` (currently parsed-and-stored in phase 01 with `address: SocketAddress` only) gains an optional `access_log_path: Option<String>` field (parsed-and-stored only — the admin-side access log is OUT OF SCOPE in 06.1; the field is parsed for fixture compatibility with upstream Envoy admin configs that include it, mirroring ADR-0026's parse-and-ignore pattern). `HttpConnectionManagerConfig` gains an optional `stat_prefix: String` field (parsed-and-consumed; threaded into the `HCMStats` registration namespace). ~30 LoC + 3 unit tests.

**D6.1 — Differential harness extensions for fixture 0011.**
  - `Driver::AdminScrape { path, expected_status, expected_content_type, expected_body_rule }` — a new driver variant on the existing `Driver` enum (sibling of `Driver::Http1` from 04.1 and `Driver::Http2` from 05.2). Drives a `GET <path>` HTTP/1.1 request to the admin endpoint; asserts status, `content-type` header, and a body rule (described below). The driver reuses 04.x's `drive_http1` shape internally.
  - `BodyRule::PrometheusExposition` — a new body-rule variant that parses the body as Prometheus exposition format and asserts on the **set of metric names** present, not on values. Per BEHAVIOR_CONTRACT.md Section `Stat-name mapping`'s value-vs-name disposition (some stats are `name-required, value-may-differ`), the body rule asserts equivalence on the symmetric difference of metric names between the two proxies' scrapes (must be empty modulo a per-fixture allow-list of envoy-only or envoy-rust-only stats).
  - Fixture `tests/fixtures/0011-admin-stats-prometheus/` — 5 files (`envoy.yaml` with admin block + a minimal HCM listener that drives one request through HCM/cluster/listener; `envoy-rust.yaml` per-side divergences; `inputs/payload.bin` describing the request sequence; `expectations.yaml` driver kind `admin_scrape` with `path: "/stats/prometheus"`, `expected_status: 200`, `expected_content_type: "text/plain; version=0.0.4; charset=utf-8"`, `expected_body_rule: { kind: prometheus_exposition, allowlist_envoy_only: [...], allowlist_envoy_rust_only: [...] }`; `README.md`).
  - Docker-gated `tests/differential/tests/admin_stats_prometheus.rs` (sibling of `http1_direct_response.rs` / `http2_direct_response.rs`).

  ~400 LoC harness + 5 fixture files + the Docker-gated test.

**D7.1 (verification deliverable, no code).** State-4 phase-done verification per the `BOOTSTRAP_PROMPT.md` §7.5 gate, scoped to 06.1's surfaces. PROGRESS.md quotes the CI run URL + the 0001-0011 + h2spec results inline.

### Phase 06.2 — `envoy-accesslog` foundation + Envoy default format + HCM wiring + fixture 0012 (~1300 LoC, ~11 tasks)

**D8.2 — New library crate `crates/envoy-accesslog/`.** Added to root `Cargo.toml` `[workspace] members`. Cargo deps: `tokio = { version = "1", features = ["fs", "io-util", "sync"] }`, `bytes = "1"`, `tracing = "0.1"`, `thiserror = "2"`, `envoy-http1 = { path = "../envoy-http1" }` (for `Request`/`Response` value-types — the access-log record borrows references to these for the format pass). **No new permitted-foundations grants** — wall-clock time formatting hand-rolled. Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

  **Module decomposition:**
  ```
  crates/envoy-accesslog/src/
    lib.rs           // crate root: #![forbid(unsafe_code)]; public re-exports
    record.rs        // AccessLogRecord struct + builder
    sink.rs          // Sink trait + dispatch helper
    file_sink.rs     // FileSink impl (tokio fs append)
    default_format.rs // Envoy default-format emitter + ISO-8601 helper
    error.rs         // AccessLogError typed-error enum
  ```

  Public surface:
  ```rust
  pub struct AccessLogRecord {
      pub start_time: SystemTime,
      pub method: String,
      pub path: String,
      pub protocol: String,           // "HTTP/1.1" or "HTTP/2"
      pub response_code: u16,
      pub response_flags: String,     // "-" by default
      pub bytes_received: u64,
      pub bytes_sent: u64,
      pub duration: Duration,
      pub upstream_service_time: Option<Duration>,
      pub forwarded_for: Option<String>,
      pub user_agent: Option<String>,
      pub request_id: Option<String>,
      pub authority: Option<String>,
      pub upstream_host: Option<String>,
  }

  #[async_trait::async_trait]   // OR a hand-rolled trait if async_trait isn't on permitted-foundations
  pub trait Sink: Send + Sync {
      async fn emit(&self, record: &AccessLogRecord) -> Result<(), AccessLogError>;
  }

  pub struct FileSink {
      path: PathBuf,
      // tokio::sync::Mutex<File> for append-serialized writes
  }

  pub mod default_format {
      pub fn format(record: &AccessLogRecord) -> String;
  }
  ```

  **Note:** `async_trait = "0.1"` is **NOT** on D-3.2's permitted-foundations list. The planner at 06.2 SPEC writeup time picks between (a) hand-rolling the trait without `async_trait` (e.g., return a `Pin<Box<dyn Future<...>>>` from the trait method, accepting the boxing cost), (b) adding `async_trait` under a small permitted-foundations-extension ADR (likely ADR-0030), or (c) skipping the trait abstraction in 06.2 and only shipping `FileSink` concretely (the trait can land when N≥2 sink types exist). **Recommended: option (c)** — phase 06.2 ships `FileSink` concretely; `Sink` trait + multi-sink dispatch defer to whichever phase first ships a second sink (likely a gRPC ALS sink or stdout sink in the Observability family). This avoids the foundations-extension ADR.

  ~400 LoC impl + ~250 LoC unit tests (record builder; FileSink append-and-flush; multi-line append serialization under torture; default-format emitter golden output for happy-path / 4xx / 5xx records; ISO-8601 emitter golden output for known epoch seconds + a leap-day boundary test; UTF-8-edge-case in user-agent / authority).

**D9.2 — `envoy-config` schema additions for `access_log:`.** `HttpConnectionManagerConfig` gains an optional `access_log: Vec<AccessLogConfig>` field. The `AccessLogConfig` struct mirrors Envoy's `envoy.config.accesslog.v3.AccessLog` proto — `name: String` (e.g., `"envoy.access_loggers.file"`), `typed_config: TypedConfig` (existing typed_config envelope from phase 05.3). The validator accepts only `name = "envoy.access_loggers.file"` with `@type = "type.googleapis.com/envoy.extensions.access_loggers.file.v3.FileAccessLog"` and `path: String` in 06.2; other `@type` URLs reject with `ConfigError::UnsupportedAccessLogType { actual: String }`. ~80 LoC schema + ~50 LoC validator + ~6 unit tests + ≥1 fuzz-corpus seed (`hcm_access_log_file.yaml`).

**D10.2 — HCM access-log wiring.** `envoy-http1::HCMConfig` gains an optional `access_log: Vec<Arc<dyn Sink>>` field (or `Vec<FileSink>` in the no-trait posture per D8.2 recommendation). At HCM-on-response-complete time (the existing `write_response` call site in phase-04 HCM at `crates/envoy-http1/src/hcm.rs`), the HCM builds an `AccessLogRecord` from the captured request + response + timing and dispatches the record to each configured sink via `sink.emit(&record).await`. The dispatch is fire-and-forget (errors logged via `tracing::warn!` — access-log emission failures must not affect the response-write path). ~200 LoC across `envoy-http1` + `envoy-http2` (HCM alias inherits the wiring). ~100 LoC unit tests (HCM with configured FileSink writes one line per request; HCM with no `access_log:` configured does not touch the file system; HCM emission error does not fail the request).

**D11.2 — Differential harness extensions for fixture 0012.**
  - `Driver::Http1WithAccessLog { ..., expected_access_log_lines: Vec<AccessLogLineRule> }` — extends the existing `Driver::Http1` with an optional access-log assertion. The harness reads the configured access-log file path from the proxy's config, opens the file after the request completes, and asserts each line's tokens against the per-token equivalence rules per BEHAVIOR_CONTRACT.md Section `Access log field mapping`.
  - `AccessLogLineRule` — per-token rule: `Exact(String)` / `Iso8601Format` / `DurationMs` / `Wildcard` / `EnvoyOnly` / etc. The harness parses the access-log line with a hand-rolled tokenizer (default-format-only; ~40 LoC) and matches each token against its rule.
  - Fixture `tests/fixtures/0012-access-log-file-sink/` — 5 files (`envoy.yaml` with HCM + `access_log: [{ name: envoy.access_loggers.file, typed_config: {...} }]`; `envoy-rust.yaml` per-side divergences; `inputs/payload.bin` describing the request; `expectations.yaml` driver kind `http1_with_access_log` with the per-token rules; `README.md`).
  - Docker-gated `tests/differential/tests/access_log_file_sink.rs`.

  ~400 LoC harness + 5 fixture files + the Docker-gated test.

**D12.2 — BEHAVIOR_CONTRACT.md `Access log field mapping` section populated.** Per §2.2 of this SPEC; lands at the 06.2 first-task or first-fixture commit. ~50 LoC of doc-only diff.

**D13.2 (verification deliverable, no code).** State-4 phase-done verification per the §7.5 gate, scoped to 06.2's surfaces.

### Phase 06.3 — Comprehensive stats wiring + 05.3 I1 closure + parent-06 close (~1200 LoC, ~10 tasks)

**D14.3 — 05.3 REVIEW I1 closure (Task-1 preamble).** Per `STATE.md` "Phase-05.3 rollovers" carryforward inventory: an H1-listener configured against an H2-cluster currently silently dispatches H1-on-the-wire, masking a protocol misnegotiation. The fix is a parse-time validator gate at `crates/envoy-config/src/bootstrap.rs::validate`: a new `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }` variant fires when the per-listener cluster-reachability scan finds an H2-cluster reachable from an H1-listener (or AUTO listener). Mechanical: ~50 LoC schema + ~30 LoC validator + ~5 unit tests (positive: H1-listener with H1-cluster passes; H2-listener with H2-cluster passes; H2-listener with H1-cluster passes; **H1-listener with H2-cluster rejects**; AUTO-listener with H2-cluster rejects). Mirrors phase-05.1 Task-1's posture toward phase-02.1 REVIEW I3 (a previously-identified gap closed cheaply at the start of a later phase).

**D15.3 — Comprehensive stats wiring at HCM/router/listener/cluster.** Extends 06.1's representative stats subset with the standard Envoy stat tree:
  - **Per-response-class HCM counters** — `http.<stat_prefix>.downstream_rq_2xx`, `..._3xx`, `..._4xx`, `..._5xx` (the HCM increments the per-class counter at on-response-complete time based on `response.status / 100`).
  - **Connection-lifetime gauges** — `listener.<name>.downstream_cx_active` (gauge: increment on accept, decrement on close), `cluster.<name>.upstream_cx_active` (same shape). Tests cover the gauge's monotonic-then-decreasing trajectory across a small request burst.
  - **Upstream-side HCM counters** — `cluster.<name>.upstream_rq_total`, `cluster.<name>.upstream_rq_5xx`. Increment at the router proxy-arm completion site (per phase-04.3's router-arm landing site).
  - **Access-log line counters** — `http.<stat_prefix>.access_logs_total`. Increment on every access-log line emission (fire-and-forget dispatch, but the counter increments at queue-enter time so emission failures don't deflate the count).
  - **Listener accept-failure counter** — `listener.<name>.downstream_cx_accept_failed`. Increment on `Listener.accept` errors (e.g., kernel ECONNRESET during accept).

  ~400 LoC across `envoy-http1` / `envoy-http2` / `envoy-listener` / `envoy-cluster` + ~250 LoC unit tests. Each stat lands with a per-name unit test (counter/gauge increment site verified; emission-point semantics verified).

**D16.3 — BEHAVIOR_CONTRACT.md `Stat-name mapping` section extended.** Each stat introduced in D14 lands an entry in the table with its equivalence disposition (`value-exact` or `name-required, value-may-differ` per §2.1 of this SPEC's projection rules). ~80 LoC of doc-only diff.

**D17.3 — Fixture 0011 `expectations.yaml` extended.** 06.1 lands fixture 0011 with a small expected-stat-name set; 06.3 extends the set to cover the comprehensive stats. ~30 LoC of YAML diff.

**D18.3 — Differential harness extensions if needed for fixture 0011 extension.** If the comprehensive-stat-set needs new harness rule shapes (e.g., a `BodyRule::PrometheusExposition` extension covering gauges' value-may-be-zero shape), they land here. Otherwise no new harness code. ~50 LoC max.

**D19.3 — Parent-06 state-6 close-out.** The 06.3 state-6 phase-done commit also flips parent ROADMAP row `06` from `in-progress` to `done` per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1 ("the parent flips to `done` only after all sub-phases are `done`"). Mirrors phase 04's `e626862` close-out and phase 05's `82c26b8` close-out. STATE.md advances active phase from `06.3` lifecycle state 5 to phase `07` lifecycle state 1; next-skill `superpowers:brainstorming` scoped to phase 07's `BOOTSTRAP_PROMPT.md` §8 row-07 charter (*"Filter chain framework: iteration protocol, per-route config, extension registry"*).

**D20.3 (verification deliverable, no code).** State-4 phase-done verification per the §7.5 gate, scoped to 06.3's surfaces + simultaneous green on all 0001-0012 fixtures.

---

## 4. Out of scope (deferred non-goals)

The following surfaces are **explicitly deferred** to later phases — phase 06 ships a minimal-viable observability foundation, not a comprehensive observability stack.

- **Histograms.** Counter + gauge primitives only. Histograms (per-request latency distributions; Envoy uses `circllhist` which we cannot replicate cheaply) defer to a later observability-family phase. Prometheus histogram exposition format also defers.
- **Stats labels / `tag_specifiers`.** Envoy supports tag extraction from stat names (e.g., `cluster.svc_a.upstream_cx_total` → `cluster_upstream_cx_total{cluster="svc_a"}`). Phase 06 emits stats as flat names. Tag extraction defers.
- **Access-log format-string customization.** The Envoy default format is hand-rolled in 06.2; format-string parsing (`%REQ(:METHOD)%`, `%START_TIME(%Y-%m-%dT%T.%3fZ)%`, `%REQ(X-ENVOY-ORIGINAL-PATH?:PATH)%` fallback semantics, etc.) defers.
- **Access-log sinks beyond file.** `FileSink` only in 06.2. gRPC ALS, OTLP, stdout sinks defer to the Observability family.
- **JSON-format access logs.** Phase 06 ships text-format only. Envoy's `json_format` and `typed_json_format` defer.
- **Admin endpoints beyond `/ready`, `/stats`, `/stats/prometheus`.** `/clusters`, `/listeners`, `/server_info`, `/config_dump`, `/runtime`, `/runtime_modify`, `/logging`, `/quitquitquit`, `/healthcheck/fail`, `/healthcheck/ok` defer to phase 08 (Minimum admin API per `BOOTSTRAP_PROMPT.md` §8 row 08).
- **Graceful drain.** The admin migration in 06.1 does not engage drain semantics. Drain defers to phase 08.
- **Admin-side access logs.** The admin handler does not emit access logs in 06.1 even though `Admin.access_log_path` is parsed-and-ignored per D5.1.
- **Stats sinks beyond the in-process registry.** No `metrics_service` cluster; no stats-flush-to-cluster. The Prometheus exposition is read-on-demand from the registry. External stats sinks defer to the Observability family.
- **Per-request access log filtering.** Envoy supports `access_log_filter` blocks (status-code ranges, header matchers, runtime fractions). Phase 06 ships unfiltered access logs.
- **HTTP/2 admin listener.** The admin listener in 06.1 is HTTP/1.1 only. H2 admin defers.
- **TLS admin listener.** The admin listener in 06.1 is plaintext only. TLS-protected admin defers (Envoy supports it but this project's threat model treats admin as localhost-only for now).
- **Stat-name reload / dynamic-stat lifecycle.** Stats live in the registry forever in 06.1; LRU eviction, scope-bound stats, and dynamic-cluster stat lifecycle defer to xDS-family phases.
- **`%FILTER_STATE%`, `%DYNAMIC_METADATA%` access-log tokens.** Filter-state and dynamic-metadata machinery doesn't exist yet (defers to phase 07's filter-chain framework and beyond). Phase 06.2 ships the 14 fixed default-format tokens only.
- **Stats config: `stats_config.use_all_default_tags`, `stats_matcher`, `stats_tags`.** Phase 06 ignores `stats_config` blocks if present (parse-and-ignore per ADR-0026 pattern, if needed at fixture writeup time). Stats matcher / tag filtering defers.
- **Phase 05.3 REVIEW I2** (typed-error chain dissolution at H2 dispatch site). Not engaged by phase-06 surfaces; carries forward unchanged.
- **Phase 05.2 REVIEW I1** (h2spec tarball SHA-256 verification in CI). Phase 06 may opportunistically close this if a state-4 task touches `.github/workflows/ci.yml`; otherwise carries forward unchanged.
- **Phase 05.2 REVIEW I2 / I3** (Http2Error variant rename, MalformedH2HeaderBlock split). Not engaged; carries forward unchanged.
- **Phase 04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR). Phase 06 introduces no new top-level Cargo deps if the no-foundations-grant posture holds; M5/M9 carries forward unchanged. If `async_trait` lands under a foundations-extension ADR (option (b) of D8.2's note — NOT recommended), this is the natural site to ratify a Cargo.lock cadence ADR.
- **Phase 04.1 REVIEW M7** (TLS+H2 ALPN-driven dispatch generalization). Phase 06 doesn't ship TLS or H2 surfaces; M7 carries forward unchanged.
- **Phase 02.2 REVIEW M1** (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep`). Standing carryforward chain inherited verbatim from phase-02.2; phase 06 does not parallelize `run_fixture` so M1 continues to track unchanged.

---

## 5. Sub-phase split rationale (codified at parent-06 state-2 via ADR-0029)

**Why split.** The combined LoC estimate (~4000-5000 LoC) and task estimate (~33 tasks) exceed the §6.1 split-gate (~1500 LoC, ~25 tasks) by ~2.5× and ~1.5× respectively. Single-phase 06 is not feasible.

**Why 3-way over 2-way.** A 2-way split (e.g., stats+admin / accesslog+wiring+close) would put one slice at ~2000+ LoC, leaving little headroom against drift. Phase-04.3's experience showed brainstorm-time LoC estimates can drift by ~+20% during execution; a 2-way slice at the §6.1 boundary risks re-splitting mid-execution. The 3-way split mirrors phase-04 (ADR-0020) and phase-05 (ADR-0022) precedent and lands each sub-phase at ~1300 LoC with healthy headroom.

**Why this surface boundary.** The split groups the work by subsystem coherence, not by traffic direction (which doesn't apply here):
  - **06.1** delivers the **stats + admin pair** as one coherent slice. Admin is the natural first consumer of the stats registry (it reads stats to emit Prometheus exposition); landing them together avoids an awkward "stats subsystem with no consumer" intermediate state. The "representative-only" stats wiring depth lets fixture 0011 land at 06.1 with a small assertion set.
  - **06.2** delivers the **access log subsystem** in isolation. Access logs are independent from stats (they consume request/response state and emit lines; the stats counters are tangentially incremented for `access_logs_total` but that's wired into 06.3's comprehensive pass). Splitting access log into its own sub-phase gives it a clean review surface.
  - **06.3** delivers **comprehensive stats wiring + 05.3 I1 closure + parent close** as the cleanup slice. Comprehensive stats wiring extends 06.1's foundations across the codebase systematically, exercising the registry's name-tree at scale; the 05.3 I1 closure is Task-1 preamble (mirrors phase-05.1's posture toward phase-02.1 I3).

**Alternatives considered:**
  - **(i) Single phase** — rejected per LoC/task gate.
  - **(ii) Two-way split** (stats+admin+wiring / accesslog+close) — rejected per drift-headroom argument.
  - **(iii) Two-way split** (stats / accesslog+admin) — rejected; admin's natural pair is stats, not access log.
  - **(iv) Four-way split** (stats / admin / accesslog / wiring+close) — rejected; artificially separates stats primitives from one demonstration consumer (admin/Prometheus). Each slice would be ~700-1000 LoC, too small to motivate a full state-machine cycle.
  - **(v) 3-way flat split by surface boundary** (decision).

**Sub-phase ordering invariant.** Sub-phases ship strictly in order (06.1 → 06.2 → 06.3) — they cannot be parallelized because (a) 06.2's HCM access-log wiring at the on-response-complete site benefits from the registry's existence (the access-log-line counter `access_logs_total` lands in 06.3 and references the registry registered in 06.1), (b) 06.3's comprehensive stats wiring extends 06.1's "representative" subset and 06.2's HCM hooks, (c) 06.3 closes the parent-06 ROADMAP row.

**ADR-0029 lands at parent-06 state-2** (writing-plans session) per the phase-04 / phase-05 precedent (ADR-0020 / ADR-0022 landed at their respective state-2 commits). ADR-0029 records the split decision; sub-phase SPECs land in the same state-2 commit.

---

## 6. Cross-sub-phase architectural invariants

These rules hold across all three sub-phases; they are cross-cutting design contracts that any sub-phase's deliverables must respect.

**Rule 1 — `envoy-stats` is the sole workspace dep on any stats deps; `envoy-accesslog` is the sole workspace dep on any access-log deps; `envoy-admin` is the sole workspace dep on admin-listener wiring.** Phase 06 introduces no new permitted-foundations grants under the recommended posture (no `prometheus`, no `metrics`, no `time`, no `chrono`, no `async_trait`). If a foundations grant becomes necessary at execution time (e.g., the hand-rolled ISO-8601 emitter is more painful than estimated), the planner lands an in-execution ADR-0030 or similar narrowly scoped to the affected crate — mirroring phase-05.3's ADR-0028 in-execution-ADR cadence per D-3.5.

**Rule 2 — `envoy-stats` exports primitives only; consumers register and increment. `envoy-stats` does NOT know about HCM, listeners, clusters, or admin endpoints.** Stats wiring lives at the consumer side (`envoy-listener::Listener` registers and increments its own counters; `envoy-cluster::Cluster` registers and increments its own counters; `envoy-http1::HCM` registers and increments per-stat-prefix HCM counters). This preserves the dependency direction — `envoy-stats` is a foundation library, not an integration layer. Mirrors phase-05.2's posture for `envoy-http2` (the codec-edge crate; consumers translate at the edge).

**Rule 3 — `envoy-admin` depends on `envoy-http1` only (not `envoy-http2`).** The admin listener is HTTP/1.1 only in 06.1; H2 admin defers. This avoids a hypothetical `envoy-http1 ↔ envoy-admin ↔ envoy-http2` cycle (which doesn't exist anyway since `envoy-admin` is a new crate, but recording the rule prevents future regressions).

**Rule 4 — Access-log emission is fire-and-forget at the HCM site.** Access-log emission errors must not affect the response-write path. The HCM dispatches the record to the configured sinks; sink errors are logged via `tracing::warn!` and counted in `http.<prefix>.access_logs_total` failure-side (a `..._access_logs_failed` counter lands in 06.3 if scope permits). The HCM does not await sink emission completion before writing the response.

**Rule 5 — Admin endpoint paths are exact-match only in 06.1.** Prefix matching defers to whichever phase first needs it (likely phase 08's admin extension). Path-parameter parsing (e.g., `/clusters/<name>` for per-cluster stats) defers to phase 08.

**Rule 6 — Stats registry registration is synchronous; emission is lock-free.** `StatsRegistry::register_counter(name) -> Arc<Counter>` acquires the registry's `RwLock` write lock once at construction time. Counter `Counter::inc()` is `AtomicU64::fetch_add(1, Ordering::Relaxed)` — lock-free. The registry's `RwLock` is read-locked only at scrape time (`/stats/prometheus` reads the full registry under a read lock; under load, scrapes are infrequent so the read lock is uncontended).

**Rule 7 — The "representative" stats subset in 06.1 is the minimum viable demonstration.** Three counters: one per layer (listener, cluster, HCM). 06.3 extends the set comprehensively. Fixture 0011's `expectations.yaml` is extended in 06.3 to assert the comprehensive set; in 06.1 it asserts only the representative subset.

**Rule 8 — Phase 06 does not write to the BEHAVIOR_CONTRACT.md `Header allow-list` table by default.** Admin-listener responses match Envoy's emission exactly under the Prometheus exposition standard. If empirical testing surfaces a divergence, an entry lands at the relevant 06.x state-3 task per the phase-04.3 / phase-05.x posture.

---

## 7. ADR projection

Phase 06's ADR ledger entrance state is **ADR-0028** (landed at phase 05.3 Task 6 per the 05.3 STATE.md close-out; see DECISIONS.md tail). Phase 06 projects the following ADRs:

- **ADR-0029 (parent-06 split decision).** Lands at parent-06 state-2 (writing-plans session) alongside the three sub-phase SPECs and the new ROADMAP rows for 06.1 / 06.2 / 06.3. Records the 3-way split rationale per §5 above; mirrors ADR-0020 (phase-04 split) and ADR-0022 (phase-05 split) in shape and provenance discipline. Required.

- **Conditional ADR-0030 (foundations grant for `time = "0.3"` or `async_trait = "0.1"`).** **Not pre-projected.** The recommended posture is no foundations grants in phase 06 (D8.2's option (c) — defer the `Sink` trait until N≥2 sinks; D-3.2's hand-rolled-from-scratch doctrine is honored for ISO-8601 emission). If execution-time experience shows the hand-roll is materially worse than the dep, an in-execution ADR per D-3.5 lands narrowly. The number `ADR-0030` stays available for whichever sub-phase first needs it.

- **Conditional ADR-0031 (Cargo.lock cadence ratification).** **Not pre-projected.** Phase-04.1 REVIEW M5/M9 carries forward unchanged unless ADR-0030 actually lands and forces a cadence pick. If ADR-0030 does not land, M5/M9 continues to phase 07.

- **Conditional ADR-0032+ (sub-phase-specific decisions).** Each sub-phase's brainstorm may surface unanticipated decisions worth ADR-shaped permanent records (e.g., a stats-name validation rule, a Prometheus exposition format edge case, an access-log token semantic). Numbers stay available; each sub-phase's SPEC §7 projects its own ADRs at its brainstorm time.

**ADR-renumbering provenance discipline.** If conditional ADRs do not land, their numbers stay available for later phases per the established ledger discipline (parent-04's ADR-0020 + ADR-0021 landed without renumbering; parent-05's ADR-0022 + ADR-0023 landed without renumbering; ADR-0024 / ADR-0025 / ADR-0026 / ADR-0027 / ADR-0028 reflect the actual landing sequence per their provenance footers).

---

## 8. State-machine signposts for the parent-06 state-2 session

The parent-06 state-2 session (the next session after this brainstorm; runs `superpowers:writing-plans`) operates per `SKILL_ROUTING.md` line 21: *"SPEC.md exists, PLAN.md does not → superpowers:writing-plans → output: PLAN.md → GATE: if PLAN.md > ~25 tasks OR > ~1500 LoC estimated → split into NN.1, NN.2, …; update ROADMAP + STATE; stop"*. Per the phase-04 (`1d9740d`) and phase-05 (`f1804a7`) precedents, the parent-06 state-2 session lands:

1. **ADR-0029** (split decision) appended to `docs/envoy-rust/DECISIONS.md` per D-3.5 — parallel structure to ADR-0020 / ADR-0022.
2. **Three sub-phase SPECs** at `docs/envoy-rust/phases/06.1-stats-and-admin/SPEC.md`, `06.2-access-log/SPEC.md`, `06.3-stats-wiring-and-close/SPEC.md`. Each sub-phase SPEC expands its own deliverables to per-task PLAN-ready cadence.
3. **Three new ROADMAP rows** (`06.1`, `06.2`, `06.3`) with `status: planned`.
4. **Parent ROADMAP row 06's `sub-phases` column** updated to `06.1, 06.2, 06.3`. Row 06's `status` remains `in-progress` (it flipped to `in-progress` at this brainstorm's close-out commit).
5. **STATE.md** advanced to point at `06.1` lifecycle state 1 (next-skill `superpowers:brainstorming` scoped to 06.1).

The parent-06 state-2 session does **not** land per-sub-phase PLAN.md files — those land at each sub-phase's own state-2 sessions per the precedent. The parent-state-2 session writes only the parent-level split coordination artifacts.

**Sub-phase entry point.** After parent-06 state-2 lands, the next session enters phase 06.1 lifecycle state 1 — runs `superpowers:brainstorming` scoped to 06.1's surface, lands `06.1-stats-and-admin/SPEC.md` (the sub-phase SPEC, refining D1-D7 of this parent SPEC into per-deliverable detail), and the cycle continues.

**Execution invariants (unchanged from parent-05):**
- Sub-phases ship strictly in order. 06.2 cannot start before 06.1's state-6 close-out commit.
- Each sub-phase honors the phase-done gate from `BOOTSTRAP_PROMPT.md` §7.5 in full at its own state-4.
- Each sub-phase produces its own REVIEW.md at state-5 per `superpowers:requesting-code-review`.
- The parent-06 state-6 close-out happens at 06.3's state-6 commit (the last sub-phase's commit also flips parent row 06 to `done`), per the ROADMAP-schema invariant in `BOOTSTRAP_PROMPT.md` §4.1 and the established phase-04 / phase-05 close-out shape.

---

## 9. Commit message format

The final state-6 commit at parent-06 close (the 06.3 phase-done commit; mirrors phase 05's `82c26b8`-shape) uses the standard format from `BOOTSTRAP_PROMPT.md` §5.3:

```
phase 06.3: <06.3 title> [parent 06 done] [ADR-NNNN, ...]

<summary — 1-3 sentences covering the 06.3 surface and the parent-06 close>

Differential surface: fixtures 0001-0012 green at the Docker-gated CI level; access-log + Prometheus admin scrape verified end-to-end.
Conformance: h2spec ≥95% pass (carried forward from 05.2 baseline); no new conformance suites in phase 06.
```

The `[parent 06 done]` tag attaches to the 06.3 state-6 commit's title, mirroring phase 05.3's `82c26b8` close-out. The bracketed ADR list enumerates ADRs landed across the parent-06 execution arc — at minimum ADR-0029 (split decision); plus any conditional ADRs that landed.

---

## 10. State-machine commit (this commit — parent-06 state-1 close-out)

This commit (the parent-06 state-1 brainstorm close-out) lands:

- This file (`docs/envoy-rust/phases/06-observability/SPEC.md`) — the parent-06 SPEC.
- `docs/envoy-rust/STATE.md` — advanced to point at phase 06 lifecycle state 2; next-skill `superpowers:writing-plans`.
- `docs/envoy-rust/ROADMAP.md` — row `06` flips `status: planned` → `status: in-progress` per the §4.1 invariant ("a phase enters `in-progress` only when STATE.md points at it" — STATE.md now points at phase 06 with state 2, so the row reflects that).

No code changes. No new ADRs at this commit (ADR-0029 lands at parent-06 state-2). DECISIONS.md ledger head remains ADR-0028.

The next session enters parent-06 state-2 — runs `superpowers:writing-plans` scoped to this parent SPEC, lands ADR-0029 + the three sub-phase SPECs + ROADMAP row updates per §8 above, and exits.
