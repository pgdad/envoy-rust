# Phase 06.1 — `envoy-stats` foundation + `envoy-admin` HCM-backed listener migration + Prometheus exposition + fixture 0011

- **Phase id:** `06.1`
- **Parent phase:** `06-observability` (split per **ADR-0029**; parent SPEC at `docs/envoy-rust/phases/06-observability/SPEC.md`, committed at parent-06 state-1).
- **Slug:** `06.1-stats-and-admin`
- **Title:** Land the project's first stats subsystem (counter + gauge primitives, hierarchical `StatsRegistry`, Prometheus text-exposition emitter) as a new workspace member `crates/envoy-stats/` (sole-dep-owner of any stats deps; phase 06 introduces no new permitted-foundations grant under the recommended posture); land a new workspace member `crates/envoy-admin/` (sole-dep-owner of admin-listener wiring; HTTP/1.1 only; exact-match path routing; serves `/ready`, `/stats`, `/stats/prometheus`); migrate phase-01's bare-bones `/ready` admin handler at `crates/envoy-bin/src/main.rs` to a real HCM-backed listener constructed from `envoy_admin::AdminHandler` (regression-guarded by an in-process backstop test plus the existing Docker-gated fixture 0002); wire a representative stats subset across listener / cluster / HCM (one counter per layer); land `envoy-config` schema additions (`Admin.access_log_path` parse-and-ignore per ADR-0026; `HttpConnectionManagerConfig.stat_prefix` parse-and-consume into the HCM stats namespace); land harness extensions (`Driver::AdminScrape`, `BodyRule::PrometheusExposition`); land fixture `0011-admin-stats-prometheus`; populate the BEHAVIOR_CONTRACT.md `Stat-name mapping` section's initial entries.
- **Depends on:** `05` (parent ROADMAP row `done` as of `82c26b8`, the 05.3 phase-done commit that also flipped parent-05 `done`); transitively the full phase-04 surface (HCM, route-walk, router upstream) and phase-05 surface (H2 listener-side codec — irrelevant to admin which is H1-only, but must remain green at acceptance time). Strictly precedes `06.2` (access-log foundation + HCM wiring + fixture 0012) and `06.3` (comprehensive stats wiring + 05.3 I1 closure + parent-06 close).
- **Differential surface when done:**
  - **Pre-existing fixtures unchanged:** `tests/fixtures/{0001-tcp-echo, 0002-static-admin-ready, 0003-tcp-proxy, 0004-tls-downstream, 0005-tls-upstream, 0006-tls-sni, 0007-http1-direct-response, 0008-http1-router-upstream, 0009-http2-direct-response, 0010-http2-router-upstream}/` — all 10 stay green at the Docker-gated CI level. Notably fixture `0002-static-admin-ready` continues green: 06.1's admin migration preserves its `/ready` semantics byte-equivalent, regression-guarded by the new in-process backstop test at `crates/envoy-bin/tests/admin_ready.rs` plus the Docker-gated fixture itself.
  - **New fixture green:** `tests/fixtures/0011-admin-stats-prometheus/` — admin listener with `/ready` + `/stats` + `/stats/prometheus` endpoints + a sibling HCM listener that drives one request through HCM/cluster/listener (so the representative counters increment); the harness scrapes `/stats/prometheus` after the request and asserts the metric-name set matches between the two proxies modulo the per-fixture allow-list.
  - **Conformance suites unchanged:** `tests/conformance/h2spec/` continues at the **≥95% pass** gate landed in 05.2 D7; 06.1 does not engage H2-framing surfaces and does not edit the runner or the gate. If any 06.1 task surfaces unanticipated H2-framing impact, the planner re-runs h2spec at state-4.
- **Seeded by:** parent-06 SPEC §3 D1.1–D7.1 (the seven 06.1 deliverables); §1 (the acceptance signal scoped to all three sub-phases — 06.1 carries the (a)/(b)/(c)/(d)/(e)/(f) gate scoped to its own surface); §2.1 (`Stat-name mapping` initial entries for the representative subset); §6 (cross-sub-phase architectural rules 1–8 — load-bearing on 06.1 since it introduces the new crates); §7 (ADR-0029 lands at parent-06 state-2 with this SPEC; ADR-0030 / ADR-0031 stay conditional); §8 (state-machine signposts for the parent-06 state-2 session — 06.1 enters its own state-2 session next); plus `BOOTSTRAP_PROMPT.md` §8 row 06 charter and §7.5 phase-done gate.

This SPEC is the design contract for sub-phase 06.1. The next session converts it into `PLAN.md` per the phase lifecycle (§5 of `BOOTSTRAP_PROMPT.md` / `SKILL_ROUTING.md`). It is self-contained per doctrine D-3.4; a stranger reading only this file plus the stable doctrine documents (`MISSION.md`, `BEHAVIOR_CONTRACT.md`, `DECISIONS.md`, `BOOTSTRAP_PROMPT.md`) and the landed phase-05 surface (via `git log` and the in-tree `envoy-{bin, cluster, config, http1, http2, listener, tcp, tls}` shape at HEAD `82c26b8`) must be able to execute it without consulting the parent `06-observability/SPEC.md`. The 06.1-binding subset of the parent's projection is reproduced inline below.

---

## 1. Goal and acceptance signal

**Goal.** Land observability foundations on the stats + admin surface in seven coordinated deliverables that all ship in this single sub-phase:

1. **New workspace member `crates/envoy-stats/`.** Sole-dep-owner of any stats deps. Phase 06 introduces **no new permitted-foundations grant** under the recommended posture — counter primitives use `std::sync::atomic::AtomicU64`, gauge primitives use `std::sync::atomic::AtomicI64`, the registry uses `std::sync::RwLock<BTreeMap<String, StatHandle>>` for the name-tree map, the Prometheus exposition emitter is hand-rolled (~30 LoC of formatter writing into `String` or `bytes::BytesMut`). Cargo deps: `bytes = "1"` (already in workspace; for the exposition buffer if the planner picks Bytes over String per signpost 8), `tracing = "0.1"`, `thiserror = "2"`. **No `tokio` dep** — the stats primitives and the registry are runtime-agnostic; consumers (HCM, listener, cluster, admin) own the tokio context. Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

2. **New workspace member `crates/envoy-admin/`.** Sole-dep-owner of admin-listener wiring. Cargo deps: `tokio = { version = "1", features = ["net", "io-util", "macros", "sync"] }`, `bytes = "1"`, `thiserror = "2"`, `tracing = "0.1"`, `envoy-config = { path = "../envoy-config" }`, `envoy-http1 = { path = "../envoy-http1" }` (for the request/response value-types and HCM-style request handling — admin endpoints are HTTP/1.1 only in 06.1), `envoy-stats = { path = "../envoy-stats" }` (to read the registry for `/stats` and `/stats/prometheus`), `envoy-listener = { path = "../envoy-listener" }` (the `ConnectionHandler` trait is the integration surface to the listener accept loop). **No `envoy-http2` dep** per cross-sub-phase architectural rule 3 (admin is H1 only in 06.1). Crate root `lib.rs` carries `#![forbid(unsafe_code)]` per D-3.8.

3. **Phase-01 admin migration in `envoy-bin`.** The bare-bones admin server at `crates/envoy-bin/src/main.rs` (the existing `bootstrap.admin.as_ref()` arm at line 305 of HEAD `82c26b8`, which delegates to the in-package `mod admin` for a hand-coded `/ready` 200 emission) is replaced by a `ConnectionHandler` constructed from `envoy_admin::AdminHandler`. The migration:
   1. Replaces the existing `admin::serve(lst, …)` task-spawn shape with a `Listener::serve` call against an `envoy_admin::AdminHandler`-shaped `ConnectionHandler`, threading the global `Arc<StatsRegistry>` and the parsed `Admin` config from `envoy-config`.
   2. Removes (or empties) the in-package `mod admin;` once the `envoy-admin` crate covers the surface.
   3. Lands an **in-process backstop test** at `crates/envoy-bin/tests/admin_ready.rs` (sibling of `crates/envoy-bin/tests/http1_direct_response.rs` from 04.1 and `crates/envoy-bin/tests/http2_direct_response.rs` from 05.2) that spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin` against a fixture-0002-style admin-only bootstrap, drives a `GET /ready` HTTP/1.1 request, and asserts a 200 response with `LIVE\n` body — proving the migration preserves phase-01's `/ready` semantics regardless of CI's Docker availability.
   4. Fixture `0002-static-admin-ready/` is **not edited at the YAML level**; both proxies' admin emission shapes were aligned in phase 01 and remain aligned through the migration.

4. **Representative stats wiring.** One counter per layer demonstrates the registry / consumer pattern end-to-end:
   - `listener.<name>.downstream_cx_total` — incremented in `envoy-listener` once per accepted TCP connection.
   - `cluster.<name>.upstream_cx_total` — incremented in `envoy-cluster` once per upstream connection establishment (per-call under the no-pooling regime; see §2 below for the BEHAVIOR_CONTRACT.md disposition).
   - `http.<stat_prefix>.downstream_rq_total` — incremented in `envoy-http1::HCM` and inherited by `envoy-http2::HCM` (per the HCMConfig type-alias from 05.2 SPEC §3 D1) once per HCM-handled request.
   06.3 extends comprehensively (per-response-class counters, connection-lifetime gauges, upstream-rq counters, accept-failure counter, access-log line counter); 06.1 ships the minimum demonstration.

5. **`envoy-config` schema additions.** Two coordinated edits in `crates/envoy-config/src/bootstrap.rs`:
   - (a) `Admin.access_log_path: Option<String>` parse-and-ignore per the **ADR-0026 pattern** (`listener_filters` parse-and-ignore precedent). The field is parsed for fixture compatibility with upstream Envoy admin configs that include it; envoy-rust never inspects or executes it (admin-side access logging defers indefinitely from 06.1).
   - (b) `HttpConnectionManagerConfig.stat_prefix` is **already a required `String` field** at HEAD `82c26b8` (verifiable at task-1 time by `grep -n 'stat_prefix' crates/envoy-config/src/bootstrap.rs` — line 351). The 06.1 edit does **not** change the schema; instead, 06.1's HCM wiring **consumes** the existing field and threads it into the `HCMStats` registration namespace at HCM construction time. The parent-06 SPEC §3 D5.1 phrasing "Option<String> parse-and-consume" is corrected here to "consume the existing required `String` field" — see §6 signpost 9.

6. **Differential harness extensions.** Two coordinated edits in `tests/differential/src/lib.rs`:
   - `Driver::AdminScrape { path, expected_status, expected_content_type, expected_body_rule }` — a new driver variant on the existing `Driver` enum (sibling of 04.1's `Driver::Http1`, 04.2's `Driver::Http1ProbeList`, 05.2's `Driver::Http2`). Drives an HTTP/1.1 `GET <path>` against the admin listener; reuses 04.x's `drive_http1` helper internally; asserts on status, `content-type`, and a body rule.
   - `BodyRule::PrometheusExposition { allowlist_envoy_only: Vec<String>, allowlist_envoy_rust_only: Vec<String> }` — a new body-rule variant. Parses the body as Prometheus text-exposition format; asserts that the **set of metric names** (after stripping any optional `# HELP`/`# TYPE` lines) is equal between the two proxies modulo the per-fixture allow-lists. Per BEHAVIOR_CONTRACT.md `Stat-name mapping`'s value-vs-name disposition, the rule does **not** assert numeric values; values may diverge per the matrix's disposition column.

7. **Fixture `0011-admin-stats-prometheus`.** 5 files in `tests/fixtures/0011-admin-stats-prometheus/` (mirroring 05.2 fixture 0009's shape): `envoy.yaml` (admin block + a sibling HCM listener exercising the representative stats); `envoy-rust.yaml` (per-side divergences); `inputs/payload.bin` (the request sequence — one HCM request to drive the counters, then one admin scrape); `expectations.yaml` (driver kind `admin_scrape`, path `/stats/prometheus`, status 200, content-type `text/plain; version=0.0.4; charset=utf-8`, body rule `prometheus_exposition` with allow-lists); `README.md`. Plus a Docker-gated wrapper at `tests/differential/tests/admin_stats_prometheus.rs`. The harness runs the request flow against both proxies and the assertion runs over the post-request scrape.

**Cross-phase items closed at 06.1.** None directly. The 05.3 REVIEW I1 carryforward (Http2ClusterFromHttp1Listener parse-time validator gate) **defers to 06.3** per parent-06 SPEC §3 D14.3, mirroring the phase-05.1 Task-1 posture toward phase-02.1 REVIEW I3. 06.1's surface (stats + admin) does not engage the H1-listener / H2-cluster validator gate.

**Cross-phase items unblocked but not closed at 06.1.** None.

**Acceptance signal** — the phase-done gate from §7.5 of `BOOTSTRAP_PROMPT.md`, scoped to 06.1's feature surface:

- **(a)** the new differential fixture `tests/fixtures/0011-admin-stats-prometheus/` is green at the Docker-gated CI level, with the CI run URL + the test result quoted inline in `PROGRESS.md`;
- **(b)** the 10 pre-existing differential fixtures `tests/fixtures/{0001-tcp-echo, 0002-static-admin-ready, 0003-tcp-proxy, 0004-tls-downstream, 0005-tls-upstream, 0006-tls-sni, 0007-http1-direct-response, 0008-http1-router-upstream, 0009-http2-direct-response, 0010-http2-router-upstream}/` all remain green at the Docker-gated CI level (they are not edited in 06.1; fixture 0002's `/ready` semantics are preserved byte-equivalent through the admin migration per §5 below);
- **(c)** the conformance suite `tests/conformance/h2spec/` continues to pass at **≥95%** with `known-failures.txt` unchanged (06.1 does not engage H2-framing; if any 06.1 task surfaces new H2 impact, the planner re-runs h2spec at state-4 and quotes the gate evidence inline);
- **(d)** the existing fuzz target `parse_bootstrap` runs clean for its short-budget CI run (`cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30`) against the corpus extended in 06.1 with **one new seed** (`crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_with_stats_route.yaml`; a full bootstrap with one HCM listener + an admin block carrying `access_log_path`); no new fuzz target ships in 06.1;
- **(e)** `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, and `cargo deny check` are all clean on the stable-toolchain CI job. `cargo deny check` clearance is a no-op (06.1 introduces no new top-level deps under the recommended posture);
- **(f)** `REVIEW.md` for this sub-phase is approved.

The 06.1 phase-done commit flips ROADMAP row `06.1` from `in-progress` to `done`. Parent row `06` stays `in-progress` until 06.3's phase-done commit (per the ROADMAP-schema invariant "parent flips to `done` only after all sub-phases are `done`"). STATE.md advances to phase `06.2` lifecycle state 2 (06.2's SPEC was already landed at parent-06 state-2 alongside this SPEC; the next session runs `superpowers:writing-plans` scoped to sub-phase 06.2).

---

## 2. Behavior-contract scope for sub-phase 06.1

06.1 is the **first phase to populate the `Stat-name mapping` section** of `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the section's standing comment at HEAD `82c26b8` is *"populated starting phase 06"*). The section's preamble already states the default assumption — *"stat names match Envoy's documented tree one-to-one; entries are recorded only when envoy-rust must produce a stat under a different internal label or with a different value disposition"*. 06.1 lands three initial rows mapped to the representative stats subset.

**`Stat-name mapping` initial entries (06.1 lands; 06.3 extends):**

| Stat name | Equivalence | Rationale |
|---|---|---|
| `listener.<name>.downstream_cx_total` | value-exact | Counter; one increment per accepted TCP connection on the listener. envoy-rust internal label matches Envoy's documented name one-to-one. Both proxies emit on every accept; under deterministic harness load (a fixed connection count) the values are byte-equal. |
| `cluster.<name>.upstream_cx_total` | name-required, value-may-differ | Counter; one increment per established upstream TCP connection. Envoy's stat semantics are "per-established-connection-from-the-pool" with default connection pooling enabled; envoy-rust under the no-pooling regime (per phase-04.3 / 05.3 posture) increments once per upstream call. Both are correct under their respective contracts. When connection pooling lands (upstream-robustness family), the disposition tightens to value-exact. |
| `http.<stat_prefix>.downstream_rq_total` | value-exact | Counter; one increment per HCM-handled request (any response code; any method). Both proxies emit on every request; under deterministic harness load (a fixed request count) the values are byte-equal. The `<stat_prefix>` segment is sourced from `HttpConnectionManagerConfig.stat_prefix` (a required field at HEAD `82c26b8`). |

The disposition column drives the harness rule: rows marked `value-exact` produce an exact-numeric assertion in the `BodyRule::PrometheusExposition` matcher (06.3 extends the body-rule shape to support per-name value rules; 06.1 asserts on metric-name presence only — see §3 D6 below); rows marked `name-required, value-may-differ` produce a metric-name-presence assertion only.

**`Header allow-list` — no new entries anticipated in 06.1.** Per parent-06 SPEC §2.3, both proxies emit the standard Prometheus content-type for `/stats/prometheus` (`text/plain; version=0.0.4; charset=utf-8`) and `text/plain` for `/ready` and `/stats`. The existing 04.x `HEADER_ALLOW_LIST` (3 rows: `server`, `date`, `x-envoy-upstream-service-time`) covers admin responses adequately — `server`/`date` apply to admin responses too (HCM machinery synthesizes them on the admin listener responses just as it does on the data-plane listener responses), `x-envoy-upstream-service-time` is N/A on admin (no upstream proxy). If empirical testing surfaces an unexpected admin-response-header divergence at fixture-0011 implementation time, BEHAVIOR_CONTRACT.md grows a row in lockstep with the in-code `HEADER_ALLOW_LIST` constant per the established phase-04.3 / phase-05.x posture.

**Equivalence-matrix engagement (per BEHAVIOR_CONTRACT.md §7.2):**
- **Row 1 (Response status)** — fixture 0011 exercises this via the admin scrape (`/stats/prometheus` returns 200; `/ready` returns 200).
- **Row 2 (Response body)** — fixture 0011 asserts the body via `BodyRule::PrometheusExposition` (set-equal on metric names modulo allow-list); the HCM-side request response is asserted via the existing `BodyRule::byte_exact` shape inherited from 04.x.
- **Row 3 (Response headers)** — fixture 0011's responses carry the existing 04.x `HEADER_ALLOW_LIST`; no new rows.
- **Rows 4 / 5 / 6 / 8** — N/A in 06.1 (no HTTP/2 framing; no TLS; the admin listener is plaintext H1).

**`Access log field mapping`, `xDS wire state machine`, `Timing tolerances` subsections — untouched in 06.1.** Access-log work is entirely 06.2's surface; xDS and timing-sensitive features are not engaged.

The `HEADER_ALLOW_LIST` constant in `tests/differential/src/lib.rs` is unedited in 06.1 (the 3-row 04.3 shape).

---

## 3. Deliverables

### Cross-sub-phase architectural rules inherited from parent-06 SPEC §6

These rules are non-negotiable across the three sub-phases of parent phase 06; sub-phase 06.1 inherits them verbatim per parent-06 SPEC §6. Reproduced here in brief paraphrase with parent-SPEC pointers; **rules 1, 2, 3, 5, 6, 7 are load-bearing in 06.1 since 06.1 introduces both new crates.**

1. **`envoy-stats` is the SOLE workspace dep on any stats deps; `envoy-admin` is the SOLE workspace dep on admin-listener wiring.** No other crate calls into the registry's serialization machinery directly. (Parent-06 SPEC §6 rule 1.) **Bearing on 06.1:** load-bearing. 06.1 introduces `crates/envoy-stats/Cargo.toml` and `crates/envoy-admin/Cargo.toml`. Phase 06 introduces no new permitted-foundations grants under the recommended posture (no `prometheus`, no `metrics`, no `time`, no `chrono`, no `async_trait`).

2. **`envoy-stats` exports primitives only; consumers register and increment.** `envoy-stats` does NOT know about HCM, listeners, clusters, or admin endpoints. (Parent-06 SPEC §6 rule 2.) **Bearing on 06.1:** load-bearing. 06.1's stats wiring lives at the consumer side (`envoy-listener::Listener` registers and increments its `downstream_cx_total`; `envoy-cluster::Cluster` registers and increments its `upstream_cx_total`; `envoy-http1::HCM` registers and increments per-`stat_prefix` HCM counters via a per-HCM `HCMStats` struct). This preserves the dependency direction — `envoy-stats` is a foundation library, not an integration layer.

3. **`envoy-admin` depends on `envoy-http1` only (not `envoy-http2`).** The admin listener is HTTP/1.1 only in 06.1; H2 admin defers indefinitely. (Parent-06 SPEC §6 rule 3.) **Bearing on 06.1:** load-bearing. `crates/envoy-admin/Cargo.toml` lists `envoy-http1 = { path = "../envoy-http1" }` and does not list `envoy-http2`. The admin handler synthesizes responses as `envoy_http1::codec::Response` value types and serves them through a path that mirrors HCM-style request reading.

4. **Access-log emission is fire-and-forget at the HCM site.** (Parent-06 SPEC §6 rule 4.) **Bearing on 06.1:** trivially satisfied — 06.1 does not ship any access-log emission (06.2's surface). The rule is reproduced for completeness.

5. **Admin endpoint paths are exact-match only in 06.1.** Prefix matching defers to whichever phase first needs it (likely phase 08's admin extension). (Parent-06 SPEC §6 rule 5.) **Bearing on 06.1:** load-bearing. 06.1's `AdminHandler` routes by exact path equality only — `/ready` matches the literal string `/ready`, not `/ready/foo`; `/stats` and `/stats/prometheus` are likewise exact. Path-parameter parsing (`/clusters/<name>`, etc.) defers.

6. **Stats registry registration is synchronous; emission is lock-free.** (Parent-06 SPEC §6 rule 6.) **Bearing on 06.1:** load-bearing. `StatsRegistry::register_counter(&self, name: &str) -> Result<Arc<Counter>, StatsError>` acquires the registry's `RwLock` write lock once at consumer construction time. `Counter::inc()` is `AtomicU64::fetch_add(1, Ordering::Relaxed)` — lock-free. The registry's `RwLock` is read-locked only at scrape time (`/stats` and `/stats/prometheus` walk the registry under a read lock; under load, scrapes are infrequent so the read lock is uncontended).

7. **The "representative" stats subset in 06.1 is the minimum viable demonstration.** Three counters: one per layer (listener, cluster, HCM). 06.3 extends the set comprehensively. (Parent-06 SPEC §6 rule 7.) **Bearing on 06.1:** load-bearing scope guard. The planner does NOT extend to per-response-class counters, gauges, or upstream-rq counters in 06.1; those defer to 06.3.

8. **Phase 06 does not write to the `Header allow-list` table by default.** (Parent-06 SPEC §6 rule 8.) **Bearing on 06.1:** load-bearing on the negative side — 06.1 must NOT pre-add admin-response-header rows to the allow-list. Rows land if-and-only-if empirical testing surfaces a divergence at fixture-implementation time.

The rules are listed for completeness; rules 1–3 and 5–7 are load-bearing in 06.1; rules 4 and 8 are trivially satisfied or load-bearing-on-negative-side. They become load-bearing again in 06.2 (rules 1, 4) and 06.3 (rules 1, 7).

---

### D1 — New workspace member `crates/envoy-stats/`

New library crate at `crates/envoy-stats/`; appended to root `Cargo.toml` `[workspace] members` alongside the existing `envoy-bin`, `envoy-cluster`, `envoy-config`, `envoy-http1`, `envoy-http2`, `envoy-listener`, `envoy-tcp`, `envoy-tls`, `tests/differential`, `tests/conformance/h2spec`, `tests/helpers/{tcp,tls,http1,http2}-echo-server` entries. Sole-dep-owner of any stats deps per cross-sub-phase architectural rule 1, mirroring `envoy-http1`'s sole-owner-of-`httparse` posture from 04.1, `envoy-http2`'s sole-owner-of-`h2` posture from 05.2, and `envoy-tls`'s sole-owner-of-`rustls` posture from 03.1.

**`Cargo.toml`:**

```toml
[package]
name = "envoy-stats"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_stats"
path = "src/lib.rs"

[dependencies]
bytes = "1"
thiserror = "2"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util"] }
```

No `tokio` runtime dep on the library side: the registry and primitives are runtime-agnostic; consumers (HCM, listener, cluster, admin) bring their own tokio context. `dev-dependencies` carries `tokio` only for the multi-threaded torture tests in D1's unit-test surface (D1 §unit-tests below).

**Module decomposition** (final shape per the planner's brainstorm at 06.1 state-2):

```
crates/envoy-stats/src/
  lib.rs        // crate root: #![forbid(unsafe_code)]; public re-exports; module-level docs
  counter.rs    // Counter (AtomicU64 increment-only)
  gauge.rs      // Gauge (AtomicI64 set/inc/dec)
  registry.rs   // StatsRegistry (RwLock<BTreeMap<String, StatHandle>>)
  prometheus.rs // write_exposition emitter
  error.rs      // StatsError typed-error enum
```

**Public surface re-exported at `lib.rs`:**

```rust
#![forbid(unsafe_code)]

//! envoy-stats — counter / gauge primitives + hierarchical stats registry +
//! Prometheus text-exposition emitter.
//!
//! Owns no workspace dep on any stats-specific crate (no `prometheus`, no
//! `metrics`, etc.); primitives are hand-rolled atop std atomics. Other
//! workspace crates (envoy-listener, envoy-cluster, envoy-http1, envoy-http2,
//! envoy-admin) consume `envoy_stats::*` via `Arc<StatsRegistry>` injection.
//! See parent-phase-06 SPEC §6 architectural rule 1 + ADR-0029.

pub mod counter;
pub mod gauge;
pub mod registry;
pub mod prometheus;
mod error;

pub use counter::Counter;
pub use gauge::Gauge;
pub use registry::{StatsRegistry, StatHandle};
pub use error::StatsError;
```

**Counter (`counter.rs`) public surface:**

```rust
pub struct Counter {
    value: std::sync::atomic::AtomicU64,
}

impl Counter {
    pub(crate) fn new() -> Self { /* AtomicU64::new(0) */ }
    pub fn inc(&self)       { /* fetch_add(1, Relaxed) */ }
    pub fn add(&self, n: u64) { /* fetch_add(n, Relaxed) */ }
    pub fn value(&self) -> u64 { /* load(Relaxed) */ }
}
```

**Gauge (`gauge.rs`) public surface:**

```rust
pub struct Gauge {
    value: std::sync::atomic::AtomicI64,
}

impl Gauge {
    pub(crate) fn new() -> Self { /* AtomicI64::new(0) */ }
    pub fn set(&self, v: i64)  { /* store(v, Relaxed) */ }
    pub fn inc(&self)          { /* fetch_add(1, Relaxed) */ }
    pub fn dec(&self)          { /* fetch_sub(1, Relaxed) */ }
    pub fn value(&self) -> i64 { /* load(Relaxed) */ }
}
```

**StatsRegistry (`registry.rs`) public surface:**

```rust
pub enum StatHandle {
    Counter(std::sync::Arc<Counter>),
    Gauge(std::sync::Arc<Gauge>),
}

pub struct StatsRegistry {
    map: std::sync::RwLock<std::collections::BTreeMap<String, StatHandle>>,
}

impl StatsRegistry {
    pub fn new() -> Self { /* empty */ }

    pub fn register_counter(&self, name: &str) -> Result<std::sync::Arc<Counter>, StatsError>;
    pub fn register_gauge(&self, name: &str) -> Result<std::sync::Arc<Gauge>, StatsError>;

    /// Returns a snapshot of all registered (name, handle) pairs in
    /// lexicographic order. Used by the Prometheus emitter (D1.5) and by the
    /// `/stats` text dump (D2). Re-snapshots on every call so writers may
    /// continue to update concurrently.
    pub fn snapshot(&self) -> Vec<(String, StatHandle)>;
}

impl Default for StatsRegistry { /* delegates to new() */ }
```

`BTreeMap` is chosen over `HashMap` (per signpost 6) for deterministic snapshot ordering — Prometheus text exposition has no required ordering but lexicographic ordering produces a stable diff at scrape time. `register_counter` / `register_gauge` return `Err(StatsError::DuplicateRegistration { name })` if `name` is already registered with the same kind, or `Err(StatsError::ConflictingKind { name, expected: "counter", got: "gauge" })` if kinds collide. Idempotent re-registration of the same kind returns the existing `Arc` (consumers re-construct safely).

**Stat-name validation** is delegated to a small `is_valid_name(name: &str) -> bool` helper in `registry.rs`: matches `[a-zA-Z_:][a-zA-Z0-9_:.\-]*` (Prometheus name rules per https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels — the `.` and `-` are intentionally permitted because Envoy's stat tree uses dots as separators; the Prometheus emitter translates dots to underscores at emission time per signpost 6). Invalid names produce `StatsError::InvalidName { name: String, reason: &'static str }`.

**Prometheus exposition (`prometheus.rs`) public surface:**

```rust
/// Writes the registry's snapshot in Prometheus text-exposition format
/// (https://prometheus.io/docs/instrumenting/exposition_formats/#text-format-example).
/// Format per metric:
///
///     # HELP <name> <description>          (optional; emitted as a generic
///     #                                     "envoy-rust counter/gauge" line in
///     #                                     06.1; richer descriptions defer)
///     # TYPE <name> counter|gauge
///     <name> <value>
///
/// Counter values are emitted as decimal u64; gauge values as decimal i64.
/// Names with dots are translated to underscores per the convention in
/// Envoy's prom emitter: `listener.foo.downstream_cx_total` becomes
/// `envoy_listener_foo_downstream_cx_total` (the `envoy_` prefix matches
/// upstream's emit-side convention).
pub fn write_exposition(registry: &StatsRegistry, w: &mut bytes::BytesMut);
```

The signature picks `bytes::BytesMut` over `&mut dyn std::io::Write` per signpost 8 (rationale: `BytesMut` is already the buffer shape used by `envoy-http1`'s response builder; the admin handler hands the resulting `Bytes` directly into the response value-type without a copy). The alternative — `std::io::Write` returning `std::io::Result<()>` — is flagged in §6 as a swappable choice if the planner discovers a constraint at task time.

**Error (`error.rs`) public surface:**

```rust
#[derive(Debug, thiserror::Error)]
pub enum StatsError {
    #[error("stat '{name}' is already registered with a different kind (expected {expected}, got {got})")]
    ConflictingKind { name: String, expected: &'static str, got: &'static str },

    #[error("stat name '{name}' is invalid: {reason}")]
    InvalidName { name: String, reason: &'static str },
}
```

`DuplicateRegistration` is **not** a separate variant — duplicate same-kind registration returns the existing `Arc<Counter>` / `Arc<Gauge>` (consumers re-construct safely; idempotency is the documented contract).

**Unit tests (~250 LoC across modules):**

In `counter.rs::tests`:
1. `counter_starts_at_zero` — fresh counter; `.value() == 0`.
2. `counter_inc_increments` — `.inc()` × 3; `.value() == 3`.
3. `counter_add_increments_by_n` — `.add(7)` × 1; `.value() == 7`.
4. `counter_inc_under_torture` — spawn 8 threads, each calling `.inc()` 10_000 times; `.value() == 80_000`. (`#[tokio::test(flavor = "multi_thread")]` for runtime + `tokio::task::spawn_blocking` for the threads, OR plain `std::thread::spawn` since `Counter` is runtime-agnostic.)

In `gauge.rs::tests`:
5. `gauge_starts_at_zero` — fresh gauge; `.value() == 0`.
6. `gauge_set_then_inc_then_dec` — `.set(10)`; `.inc()`; `.dec()`; `.dec()`; `.value() == 9`.
7. `gauge_under_torture` — spawn 4 inc-threads + 4 dec-threads, each 10_000 ops; `.value() == 0`.
8. `gauge_negative_value_permitted` — `.set(0)`; `.dec()` × 5; `.value() == -5`.

In `registry.rs::tests`:
9. `registry_register_counter_returns_handle` — register `"listener.foo.downstream_cx_total"`; receive `Arc<Counter>`; snapshot lists exactly that name.
10. `registry_register_counter_idempotent_same_kind` — register the same counter twice; both calls return the same `Arc` (Arc::ptr_eq); snapshot lists once.
11. `registry_register_gauge_then_counter_same_name_errors` — register a gauge; register a counter with the same name; `Err(ConflictingKind)`.
12. `registry_invalid_name_errors` — register a counter named `"bad name with spaces"`; `Err(InvalidName)`.
13. `registry_snapshot_is_lexicographic` — register `"b"`, `"a"`, `"c"`; snapshot returns `["a", "b", "c"]`.
14. `registry_concurrent_register_safe` — spawn 4 threads each registering 100 distinct counter names; final snapshot contains 400 entries.

In `prometheus.rs::tests`:
15. `write_exposition_empty_registry` — empty registry; output is `""` or has only a leading comment per emitter shape; goldens against an in-test string constant.
16. `write_exposition_single_counter` — register one counter, increment to 5; output matches the golden:
    ```text
    # TYPE envoy_listener_foo_downstream_cx_total counter
    envoy_listener_foo_downstream_cx_total 5
    ```
17. `write_exposition_single_gauge` — register one gauge, set to -3; output emits `# TYPE … gauge` and `… -3`.
18. `write_exposition_mixed_counter_and_gauge` — both kinds; assert lexicographic ordering.
19. `write_exposition_dot_to_underscore` — register `"http.ingress.downstream_rq_total"`; output emits `envoy_http_ingress_downstream_rq_total`.

In `error.rs::tests`:
20. `errors_format_to_diagnostic_strings` — assert `Display` outputs match the doctrine strings.

**LoC estimate D1:** ~50 LoC `Cargo.toml` + workspace-member registration + ~30 LoC `lib.rs` + ~50 LoC `counter.rs` + ~60 LoC `gauge.rs` + ~120 LoC `registry.rs` (incl. validation) + ~80 LoC `prometheus.rs` + ~30 LoC `error.rs` + ~250 LoC unit tests. Total D1: **~670 LoC** (~400 impl + ~250 tests, matching parent-06 SPEC §3 D1.1's projection).

---

### D2 — New workspace member `crates/envoy-admin/`

New library crate at `crates/envoy-admin/`; appended to root `Cargo.toml` `[workspace] members`. Sole-dep-owner of admin-listener wiring per cross-sub-phase architectural rule 1 + 3.

**`Cargo.toml`:**

```toml
[package]
name = "envoy-admin"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_admin"
path = "src/lib.rs"

[dependencies]
bytes = "1"
thiserror = "2"
tokio = { version = "1", features = ["net", "io-util", "macros", "sync"] }
tracing = "0.1"
envoy-config = { path = "../envoy-config" }
envoy-http1 = { path = "../envoy-http1" }
envoy-listener = { path = "../envoy-listener" }
envoy-stats = { path = "../envoy-stats" }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

`envoy-http2` is intentionally absent per cross-sub-phase architectural rule 3.

**Module decomposition:**

```
crates/envoy-admin/src/
  lib.rs        // crate root: #![forbid(unsafe_code)]; public re-exports
  config.rs     // AdminConfig (parsed from envoy-config Admin block)
  endpoint.rs   // AdminEndpoint enum + per-endpoint response builders
  handler.rs    // AdminHandler: implements envoy_listener::ConnectionHandler
  error.rs      // AdminError typed-error enum
```

**Public surface re-exported at `lib.rs`:**

```rust
#![forbid(unsafe_code)]

//! envoy-admin — HCM-style HTTP/1.1 admin listener serving the project's
//! built-in admin endpoints (`/ready`, `/stats`, `/stats/prometheus` in
//! 06.1; extended in later phases). Sole-dep-owner of admin-listener wiring;
//! depends on envoy-http1 for request/response value types and HCM-style
//! request handling. HTTP/1.1 only — H2 admin defers indefinitely (parent-06
//! SPEC §6 rule 3 + §4 deferred non-goal).

pub mod config;
pub mod endpoint;
pub mod handler;
mod error;

pub use config::AdminConfig;
pub use endpoint::AdminEndpoint;
pub use handler::AdminHandler;
pub use error::AdminError;
```

**`AdminConfig` (`config.rs`) public surface:**

```rust
pub struct AdminConfig {
    /// Bind address; sourced from `Bootstrap.admin.address.socket_address`.
    pub address: std::net::SocketAddr,

    /// Optional admin-side access log path; parsed from
    /// `Bootstrap.admin.access_log_path` per ADR-0026 parse-and-ignore
    /// pattern. envoy-rust does NOT inspect or honor this field in 06.1;
    /// admin-side access logging defers indefinitely. Storing it allows
    /// fixtures with upstream Envoy admin configs to round-trip cleanly.
    pub access_log_path: Option<std::path::PathBuf>,
}

impl AdminConfig {
    /// Build from a parsed `envoy_config::Admin` block.
    pub fn from_envoy_config(admin: &envoy_config::Admin) -> Result<Self, AdminError>;
}
```

**`AdminEndpoint` (`endpoint.rs`) public surface:**

```rust
pub enum AdminEndpoint {
    /// `GET /ready` — returns 200 "LIVE\n" once the server has bound its
    /// listeners. Phase-08's drain semantics introduce 503 "PRE_INITIALIZING"
    /// and 503 "DRAINING" states; in 06.1 the endpoint always returns 200.
    Ready,

    /// `GET /stats` — returns 200 with body in plain-text "name: value\n"
    /// per-line format (one stat per line; matches Envoy's default
    /// `/stats` format under `format=` absence).
    Stats,

    /// `GET /stats/prometheus` — returns 200 with body in Prometheus
    /// text-exposition format per envoy_stats::prometheus::write_exposition.
    StatsPrometheus,
}

impl AdminEndpoint {
    /// Exact-match URL path lookup; returns None for unknown paths.
    pub fn from_path(path: &str) -> Option<Self>;

    /// Render the response for this endpoint. Reads the registry only on
    /// the Stats / StatsPrometheus arms; Ready ignores the registry.
    pub fn render(
        &self,
        registry: &envoy_stats::StatsRegistry,
    ) -> envoy_http1::codec::Response;
}
```

**`AdminHandler` (`handler.rs`) public surface:**

```rust
pub struct AdminHandler {
    config: std::sync::Arc<AdminConfig>,
    registry: std::sync::Arc<envoy_stats::StatsRegistry>,
}

impl AdminHandler {
    pub fn new(
        config: std::sync::Arc<AdminConfig>,
        registry: std::sync::Arc<envoy_stats::StatsRegistry>,
    ) -> Self;
}

impl envoy_listener::ConnectionHandler for AdminHandler {
    fn handle(
        &self,
        stream: tokio::net::TcpStream,
    ) -> envoy_listener::BoxFuture<'_, std::io::Result<()>>;
}
```

The trait shape mirrors `envoy_listener::ConnectionHandler` exactly — at HEAD `82c26b8` the trait uses a `BoxFuture` returning `std::io::Result<()>` (not `async_trait`) per signpost 1. The handler reads the request via 04.1's H1 parser (consuming the existing `envoy_http1::codec::request_parse` or whatever entry point the planner verifies at task-1 time), dispatches via `AdminEndpoint::from_path`, renders via `AdminEndpoint::render`, serializes via the existing 04.1 response writer, and returns. **Per-request serial handling** is fine in 06.1 (admin endpoints are scrape-driven, not request-rate-driven); 06.1 does not implement HTTP/1.1 keep-alive on the admin listener (each request closes the connection — sufficient for Prometheus scrape, curl `/ready`, and the test harness's single-request driver).

**404 / 405 handling.** `AdminEndpoint::from_path` returning `None` produces `404 Not Found` with body `"unknown admin endpoint\n"`. Non-`GET` methods produce `405 Method Not Allowed` with body `"admin endpoints are GET-only\n"` and `Allow: GET` header. Per signpost 7, GET-only is the recommended posture; future endpoints (`/quitquitquit`, `/healthcheck/fail`, `/runtime_modify`) are POST-shaped at Envoy and will land an extension to `from_path`'s shape (`from_path_and_method`) at the phase that introduces them.

**`AdminError` (`error.rs`) public surface:**

```rust
#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error("admin address {raw} is not a parseable SocketAddr: {source}")]
    BadAddress {
        raw: String,
        #[source]
        source: std::net::AddrParseError,
    },
}
```

The `BadAddress` variant fires only during `AdminConfig::from_envoy_config` if the upstream `socket_address.address:port_value` pair fails to parse as an `IpAddr:u16`; this duplicates what `envoy-bin` already does at the existing line range but lifts the parse into `envoy-admin` for cleaner separation.

**Unit tests (~200 LoC across modules):**

In `config.rs::tests`:
1. `from_envoy_config_round_trips_address` — input `Admin { address: "127.0.0.1:9901", access_log_path: None }`; output `AdminConfig` with parsed `SocketAddr`.
2. `from_envoy_config_carries_access_log_path` — input with `access_log_path: Some("/tmp/admin.log")`; output preserves the field; envoy-rust never inspects it (test asserts the field is in the struct but no other code path consumes it).
3. `from_envoy_config_rejects_unparseable_address` — input `address: "not-a-host:9901"`; output `Err(BadAddress)`.

In `endpoint.rs::tests`:
4. `from_path_ready_matches_exact` — `from_path("/ready") == Some(Ready)`; `from_path("/ready/") == None`; `from_path("/Ready") == None` (case-sensitive per Envoy).
5. `from_path_stats_matches_exact` — `from_path("/stats") == Some(Stats)`.
6. `from_path_stats_prometheus_matches_exact` — `from_path("/stats/prometheus") == Some(StatsPrometheus)`.
7. `from_path_unknown_returns_none` — `from_path("/clusters") == None`; `from_path("") == None`.
8. `render_ready_returns_200_LIVE` — `Ready.render(empty_registry).status == 200`; body bytes equal `b"LIVE\n"`.
9. `render_stats_text_format` — registry with one counter at 7; `Stats.render(reg)` body contains `listener.foo.downstream_cx_total: 7\n`.
10. `render_stats_prometheus_format` — registry with one counter at 7; `StatsPrometheus.render(reg)` body matches the prom golden including `# TYPE` and `envoy_*` prefix.
11. `render_response_carries_correct_content_type` — Stats body carries `content-type: text/plain` (charset omitted); StatsPrometheus body carries `content-type: text/plain; version=0.0.4; charset=utf-8`.

In `handler.rs::tests`:
12. `handler_serves_ready_in_process` — bind a tokio listener on an ephemeral port; `AdminHandler::new(...)`; spawn the handler against the listener; open a TCP client, send `GET /ready HTTP/1.1\r\nHost: x\r\n\r\n`, parse response: status 200, body `LIVE\n`.
13. `handler_serves_stats_prometheus_in_process` — same shape; assert the response body parses as Prometheus exposition.
14. `handler_returns_404_for_unknown_path` — same shape; `GET /unknown`; response 404.
15. `handler_returns_405_for_post_method` — `POST /ready`; response 405; carries `Allow: GET` header.

**LoC estimate D2:** ~30 LoC `Cargo.toml` + ~30 LoC `lib.rs` + ~50 LoC `config.rs` + ~120 LoC `endpoint.rs` (the per-endpoint response builders are the bulk) + ~120 LoC `handler.rs` (the H1 read/dispatch/write loop) + ~30 LoC `error.rs` + ~200 LoC unit tests. Total D2: **~580 LoC** (~350 impl + ~200 tests, matching parent-06 SPEC §3 D2.1's projection).

---

### D3 — Phase-01 admin migration in `crates/envoy-bin/`

The bare-bones admin server at `crates/envoy-bin/src/main.rs` (the `bootstrap.admin.as_ref()` arm at line 305 of HEAD `82c26b8`, calling into the in-package `mod admin` for a hand-coded `/ready` 200 emission) is replaced by a full `ConnectionHandler` constructed from `envoy_admin::AdminHandler`. The migration:

1. **Add `envoy-admin = { path = "../envoy-admin" }` and `envoy-stats = { path = "../envoy-stats" }`** to `crates/envoy-bin/Cargo.toml`'s `[dependencies]`. (envoy-stats appears here because envoy-bin owns the `Arc<StatsRegistry>` constructor at process start; see step 4 below.)

2. **Construct the global `Arc<StatsRegistry>` once** at envoy-bin startup, before any listener-walk:
   ```rust
   let registry = std::sync::Arc::new(envoy_stats::StatsRegistry::new());
   ```
   The `Arc<StatsRegistry>` is threaded into the listener-walk and into the cluster-manager construction so that listener / cluster / HCM consumers all see the same registry.

3. **Thread the registry into `cluster_mgr` and into the listener-walk.** `envoy-cluster::from_bootstrap` grows an additional argument `registry: Arc<StatsRegistry>` (per D4 below); listener construction grows the same argument.

4. **Replace the `admin::serve(...)` task-spawn block.** The existing block (lines 305-320 of HEAD `82c26b8`) becomes:
   ```rust
   if let Some(admin_cfg) = bootstrap.admin.as_ref() {
       let admin_config = envoy_admin::AdminConfig::from_envoy_config(admin_cfg)
           .with_context(|| "building AdminConfig")?;
       let admin_handler = std::sync::Arc::new(envoy_admin::AdminHandler::new(
           std::sync::Arc::new(admin_config),
           registry.clone(),
       )) as std::sync::Arc<dyn envoy_listener::ConnectionHandler>;
       let lst = tokio::net::TcpListener::bind(admin_config.address).await
           .with_context(|| format!("binding admin listener to {}", admin_config.address))?;
       tracing::info!(addr = %admin_config.address, "envoy-rust listening (admin)");
       let shutdown = token.clone();
       set.spawn(async move {
           envoy_listener::Listener::serve(lst, admin_handler, async move { shutdown.cancelled().await }).await
       });
   }
   ```
   The exact serve-shape mirrors the existing data-plane listeners' serve shape (verifiable at task-1 time by reading the existing `set.spawn(...)` blocks at the lines preceding `305` for `TcpProxy` and `HttpConnectionManager` arms).

5. **Empty or remove the in-package `mod admin;`** at `crates/envoy-bin/src/main.rs:8` once the `envoy-admin` crate covers the surface. The planner picks at task time between (a) deleting `crates/envoy-bin/src/admin.rs` outright (recommended; cleanest) and (b) leaving the file empty as a deletion-deferral marker (rejected; leaves dead code).

6. **In-process backstop test** at `crates/envoy-bin/tests/admin_ready.rs`:

```rust
//! In-process backstop for fixture 0002's `/ready` semantics post-admin-migration.
//! Spawns envoy-bin via CARGO_BIN_EXE_envoy-bin against a fixture-0002-style
//! admin-only bootstrap, drives a `GET /ready` HTTP/1.1 request, asserts a
//! 200 "LIVE\n" response. Independent of Docker availability; runs under
//! plain `cargo test --workspace`.

use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

const ADMIN_BOOTSTRAP_YAML: &str = r#"
node: { id: backstop, cluster: backstop }
admin: { address: { socket_address: { address: 127.0.0.1, port_value: 0 } } }
static_resources: { listeners: [], clusters: [] }
"#;

#[tokio::test]
async fn admin_ready_returns_200_post_migration() {
    // 1. Write bootstrap to tempfile.
    // 2. Spawn envoy-bin pointed at tempfile; capture stdout to scrape the
    //    "envoy-rust listening (admin) addr=127.0.0.1:NNNNN" line.
    // 3. Open TCP, send `GET /ready HTTP/1.1\r\nHost: x\r\n\r\n`.
    // 4. Read response; assert status `200` and body contains `LIVE`.
    // 5. SIGKILL the spawned envoy-bin (per the 04.x integration-test posture;
    //    inherits the phase-02.2 REVIEW M1 awareness-only carryforward).
    // ... ~80 LoC implementation ...
}
```

The backstop binds the admin listener to `port_value: 0` (kernel-assigned ephemeral port) so the test runs in parallel with other integration tests. The port is captured from envoy-bin's own log emission — the existing `tracing::info!(%addr, "envoy-rust listening (admin)")` line at HEAD `82c26b8` line 315 emits the bound port, which the test parses with a small regex.

7. **Fixture 0002 unchanged at the YAML level.** The migration is internal to envoy-bin; the `envoy.yaml` and `envoy-rust.yaml` for fixture 0002 are not touched. The Docker-gated test at `tests/differential/tests/admin_ready.rs` (the existing 02.x landing) continues green.

**LoC estimate D3:** ~50 LoC delta in `crates/envoy-bin/src/main.rs` (replacing the existing 15-line block plus the registry-construction lines plus the cluster_mgr / listener-walk threading) + ~30 LoC removed from `crates/envoy-bin/src/admin.rs` deletion + ~120 LoC in-process backstop. Total D3: **~150 LoC** (matching parent-06 SPEC §3 D3.1's projection).

---

### D4 — Stats wiring "representative" subset

Three coordinated edits across `envoy-listener`, `envoy-cluster`, and `envoy-http1` (with `envoy-http2` inheriting via the HCMConfig type-alias landed in 05.2 D1):

**D4.a — Listener-side: `envoy-listener::Listener` gains `cx_total: Arc<Counter>`.** At construct time, the listener registers its counter via `registry.register_counter(&format!("listener.{}.downstream_cx_total", listener_name))?`. The accept loop increments on every `accept().await` success:

```rust
// crates/envoy-listener/src/lib.rs — Listener::serve loop:
loop {
    tokio::select! {
        biased;
        _ = &mut shutdown => break,
        res = listener.accept() => {
            match res {
                Ok((stream, _peer)) => {
                    self.cx_total.inc();   // 06.1 NEW
                    // ... existing handler dispatch ...
                }
                Err(e) => {
                    // existing error path; cx_accept_failed counter lands in 06.3
                    tracing::warn!(error = %e, "listener accept");
                }
            }
        }
    }
}
```

The `Listener` constructor signature grows `registry: Arc<StatsRegistry>` and `name: String`; the listener-walk caller (`envoy-bin/src/main.rs`) provides both. ~30 LoC + 1 unit test (`listener_increments_cx_total_on_accept` — bind ephemeral; open 3 TCP connections; assert counter at 3).

**D4.b — Cluster-side: `envoy-cluster::Cluster` gains `cx_total: Arc<Counter>`.** Registered at `Cluster::new(...)` time as `registry.register_counter(&format!("cluster.{}.upstream_cx_total", cluster_name))?`. Incremented at the upstream-connection-establishment site (the existing `tokio::net::TcpStream::connect(...)` call site in cluster.rs's connection establishment path; verifiable at task-1 time by `grep -n 'TcpStream::connect' crates/envoy-cluster/src/cluster.rs`):

```rust
// in the cluster's per-call connect path:
let stream = tokio::net::TcpStream::connect(endpoint_addr).await?;
self.cx_total.inc();   // 06.1 NEW
```

`envoy-cluster::from_bootstrap` grows a `registry: Arc<StatsRegistry>` argument and threads it through cluster construction. ~30 LoC + 1 unit test (`cluster_increments_cx_total_on_connect` — spawn a tokio TcpListener echoing `Ready`; build a `Cluster` against its address; call the connect-and-establish helper; assert counter at 1).

**D4.c — HCM-side: per-HCM `HCMStats` struct.** New struct in `crates/envoy-http1/src/hcm.rs`:

```rust
pub struct HCMStats {
    pub downstream_rq_total: std::sync::Arc<envoy_stats::Counter>,
    // 06.3 extends with downstream_rq_2xx, _3xx, _4xx, _5xx and access_logs_total
}

impl HCMStats {
    pub fn register(
        registry: &envoy_stats::StatsRegistry,
        stat_prefix: &str,
    ) -> Result<Self, envoy_stats::StatsError> {
        Ok(Self {
            downstream_rq_total: registry.register_counter(
                &format!("http.{}.downstream_rq_total", stat_prefix),
            )?,
        })
    }
}
```

`envoy_http1::HCMConfig` (and `envoy_http2::HCMConfig` via the type-alias) gains `pub stats: std::sync::Arc<HCMStats>` set at HCM construction time. The HCM increments `stats.downstream_rq_total` on every request:

```rust
// in envoy_http1::hcm::build_response (or the call site one level up):
self.stats.downstream_rq_total.inc();   // 06.1 NEW; before the route-walk
```

`envoy-http2::HCM` inherits the increment via the type-aliased `HCMConfig` consumed in `crates/envoy-http2/src/hcm.rs` — the listener-side dispatch in `envoy-http2/src/hcm.rs`'s per-stream task path increments on entry. Per signpost 5, the planner verifies at task-1 time that the H2 path's increment site matches the H1 path's semantically (one increment per HCM-handled request, regardless of `BuildOutcome::{Synth, Proxy, Reject}` arm).

~50 LoC across `envoy-http1` + `envoy-http2` + 2 unit tests (`hcm_increments_downstream_rq_total_on_request` for H1; `hcm2_increments_downstream_rq_total_on_request` for H2 — both shapes mirror the existing 04.1 / 05.2 in-process integration tests, asserting the counter post-request).

**D4.d — Admin-side: no new counters in 06.1.** The admin handler reads the registry on `/stats` and `/stats/prometheus` requests; no `admin.*` counters land in 06.1. Admin-side counters defer to whichever later phase first needs them.

**LoC estimate D4:** ~30 LoC listener + ~30 LoC cluster + ~50 LoC HCM + ~70 LoC unit tests across the three crates + the constructor-signature ripple (~20 LoC at envoy-bin to thread the registry). Total D4: **~200 LoC** (matching parent-06 SPEC §3 D4.1's projection).

---

### D5 — `envoy-config` schema additions

Two coordinated edits in `crates/envoy-config/src/bootstrap.rs`:

**D5.a — `Admin.access_log_path: Option<String>` parse-and-ignore per the ADR-0026 pattern.** At HEAD `82c26b8` the `Admin` struct is two fields wide (`address: Address`); 06.1 grows it:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Admin {
    pub address: Address,
    #[serde(default)]
    pub access_log_path: Option<String>,    // 06.1 NEW — parse-and-ignore per ADR-0026
}
```

**No new `ConfigError` variant** for this field — the parse-and-ignore disposition means absence is tolerated and presence is stored without validation (envoy-rust does not check that the path exists / is writable / etc.). Mirrors ADR-0026's `Listener.listener_filters` posture exactly.

**D5.b — `HttpConnectionManagerConfig.stat_prefix` is already required.** At HEAD `82c26b8` line 351, the field reads `pub stat_prefix: String` (required, no `Option`). The parent-06 SPEC §3 D5.1 phrasing "Option<String> parse-and-consume" is **inaccurate** for the current codebase shape — see §6 signpost 9 below. The 06.1 edit is **schema-no-op**: the existing required field is consumed at HCM construction time per D4.c above. The field's existing parse-test surface in `crates/envoy-config/src/bootstrap.rs::tests` (which asserts `stat_prefix` round-trips) is unchanged.

**D5.c — `TcpProxy.stat_prefix` continues unchanged.** TcpProxy's `stat_prefix` (line 262 of HEAD `82c26b8`) is also a required `String` field; 06.1 does not wire TCP-proxy-side counters (those defer to whichever later phase first needs them — 06.3 may extend if scope permits, but is not currently planned).

**Validator unit tests appended** (~3 tests):

1. `parses_admin_with_access_log_path` — full bootstrap with `admin: { address: …, access_log_path: "/var/log/envoy_admin.log" }`; validator accepts; the parsed struct's `Admin.access_log_path == Some(...)`.
2. `parses_admin_without_access_log_path` — `admin: { address: … }`; validator accepts; `Admin.access_log_path == None`.
3. `rejects_admin_with_unknown_field` — `admin: { address: …, profile_path: "/tmp" }` (a real Envoy field 06.1 doesn't ship); `serde deny_unknown_fields` rejects with the standard "unknown field" error.

**Fuzz corpus extension.** `crates/envoy-config/fuzz/corpus/parse_bootstrap/` gains 1 new seed:

- `admin_with_stats_route.yaml` — full bootstrap with `admin: { address: { socket_address: { address: 127.0.0.1, port_value: 9901 } }, access_log_path: /tmp/admin.log }` + one HCM listener with `stat_prefix: ingress_http` + a single VH single-route `direct_response 200 "fuzz\n"` + `clusters: []`. Mirrors the existing 04.x / 05.x seed shape. The seed exercises the validator's accept-path on the new `access_log_path` field and the existing `stat_prefix` field; the fuzzer never runs the admin handler.

Allow-list entry `!corpus/parse_bootstrap/admin_with_stats_route.yaml` added to `crates/envoy-config/fuzz/.gitignore`.

**LoC estimate D5:** ~10 LoC schema delta (the optional `access_log_path` field) + ~30 LoC unit tests + ~20 LoC fuzz seed YAML. Total D5: **~30 LoC** (in line with parent-06 SPEC §3 D5.1's projection of ~30 LoC + 3 unit tests).

---

### D6 — Differential harness extensions for fixture 0011

Three coordinated edits to `tests/differential/`:

**D6.a — `Driver::AdminScrape` variant.** New variant on the existing `Driver` enum (sibling of 04.1's `Driver::Http1`, 04.2's `Driver::Http1ProbeList`, 05.2's `Driver::Http2`):

```rust
// tests/differential/src/lib.rs Driver enum extension:
AdminScrape {
    /// Pre-scrape request flow: a sequence of HCM-side requests run before
    /// the admin scrape so the registry has counters incremented.
    pre_requests: Vec<PreRequest>,

    /// The admin endpoint to scrape.
    path: String,

    expected_status: u16,
    expected_content_type: String,
    expected_body_rule: BodyRule,
}
```

`PreRequest` is a new minimal struct (`{ method: String, path: String, host: String, port_key: String }`) wrapping the request shape needed to drive the HCM listener before scraping the admin listener. Reuses 04.x's `drive_http1` internally.

**D6.b — `BodyRule::PrometheusExposition` variant.** New variant on the existing `BodyRule` enum (sibling of `BodyRule::ByteExact` etc.):

```rust
PrometheusExposition {
    /// Metric names emitted by upstream Envoy that envoy-rust does not (yet) emit.
    /// Allows the symmetric difference assertion to ignore them.
    allowlist_envoy_only: Vec<String>,

    /// Metric names emitted by envoy-rust that upstream Envoy does not emit.
    /// Allows the symmetric difference assertion to ignore them.
    allowlist_envoy_rust_only: Vec<String>,
}
```

The matcher logic (~80 LoC):

1. Parse the body bytes as Prometheus text exposition (~30 LoC of hand-rolled parser: split on `\n`; skip lines starting with `#`; parse remaining lines as `<name> <value>`; emit `BTreeSet<String>` of names).
2. Compute the symmetric difference between the envoy-side parsed name set and the envoy-rust-side parsed name set.
3. After removing names in `allowlist_envoy_only` from the envoy-only side and names in `allowlist_envoy_rust_only` from the envoy-rust-only side, the remaining symmetric difference must be empty.
4. **No value assertions in 06.1.** Per BEHAVIOR_CONTRACT.md `Stat-name mapping`'s value disposition (some rows are `name-required, value-may-differ`), 06.1 asserts on metric-name presence only. 06.3 may extend the rule shape with a per-name value-disposition map; 06.1's body-rule shape is forward-compatible (additional fields land as `#[serde(default)]` extensions).

**D6.c — `run_fixture` dispatch arm on `Driver::AdminScrape`.** The existing `run_fixture` cascade in `tests/differential/src/lib.rs` grows a new arm dispatching `Driver::AdminScrape` to a new `drive_admin_scrape` async helper. The helper:
1. For each `PreRequest`, calls `drive_http1` against the per-fixture HCM listener port (resolved from the `port_key` per the existing template-substitution pattern).
2. Sleeps a small grace period (per signpost 11; recommended 50ms) to let the registry finish writing through `Relaxed` ordering on slow CI runners.
3. Calls `drive_http1` against the per-fixture admin listener port with a `GET <path>` request.
4. Returns the admin response tuple `(StatusCode, Vec<(String, String)>, Vec<u8>)` for assertion against the expectations.

**Template marker extension.** `run_fixture`'s template-substitution pass grows `{{ADMIN_PORT}}` (the admin listener's bound port) alongside the existing `{{PORT}}` and 05.3-era `{{HTTP2_BACKEND_PORT}}` markers. The fixture YAMLs use both `{{PORT}}` (HCM listener) and `{{ADMIN_PORT}}` (admin listener) so the harness can drive both.

**Unit test appended** to `tests/differential/src/lib.rs::tests`:

1. `drive_admin_scrape_round_trip_against_in_process_listeners` — spawns an envoy-bin subprocess with a 2-listener bootstrap (HCM + admin); calls `drive_admin_scrape(...)`; asserts the returned tuple matches expectations including the Prometheus-formatted body containing the representative counter names.

**LoC estimate D6.harness:** ~120 LoC harness extensions (`Driver::AdminScrape` variant + `BodyRule::PrometheusExposition` matcher + `drive_admin_scrape` helper + dispatch arm + template-marker extension) + ~50 LoC unit test. Subtotal: **~170 LoC**.

**Fixture `0011-admin-stats-prometheus/` — 5 files:**

**`envoy.yaml`** (~50 lines):

```yaml
node: { id: envoy-rust-phase-06.1-fixture-0011, cluster: envoy-rust-phase-06.1 }
admin:
  address: { socket_address: { address: 0.0.0.0, port_value: {{ADMIN_PORT}} } }
static_resources:
  listeners:
    - name: ingress_http
      address: { socket_address: { address: 0.0.0.0, port_value: {{PORT}} } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http
                codec_type: HTTP1
                generate_request_id: false
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
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
```

**`envoy-rust.yaml`:** identical to `envoy.yaml` modulo per-side divergences:
- bind `127.0.0.1` instead of `0.0.0.0`.
- `generate_request_id: false` is omitted (envoy-rust does not inject `x-request-id` per 04.3 SPEC §4 non-goal).
- `admin.address` listens on `127.0.0.1`.

**`inputs/payload.bin`:** empty (0 bytes) — the harness drives the request flow synthetically via the `Driver::AdminScrape` shape; the file is present for harness-shape consistency.

**`expectations.yaml`** (~30 lines):

```yaml
driver:
  kind: admin_scrape
  pre_requests:
    - method: GET
      path: "/"
      host: envoy-rust.test
      port_key: PORT
  path: "/stats/prometheus"
  expected_status: 200
  expected_content_type: "text/plain; version=0.0.4; charset=utf-8"
  expected_body_rule:
    kind: prometheus_exposition
    allowlist_envoy_only:
      # Envoy emits a much wider stat tree than envoy-rust at 06.1's
      # representative-only depth. The full list is populated at fixture-
      # implementation time after running the scrape against upstream Envoy
      # once and inspecting the resulting metric-name set. Examples include:
      #   server.live, server.uptime, server.memory_allocated,
      #   listener_manager.total_listeners_active, etc.
      - "server.live"
      - "server.uptime"
      # ... ~30-50 entries; populated at task time per signpost 12.
    allowlist_envoy_rust_only:
      # Names envoy-rust emits that upstream Envoy does not. Should be empty
      # at 06.1 if envoy-rust correctly mirrors Envoy's name shape; populated
      # at task time only if a divergence surfaces.
      []
```

**`README.md`** (~40 lines): describes the fixture surface, the request flow (pre-request + admin scrape), the per-name allow-list rationale, and cross-references this SPEC §3 D6.

**Docker-gated test:** `tests/differential/tests/admin_stats_prometheus.rs` — 7-line wrapper:

```rust
#[tokio::test]
async fn admin_stats_prometheus() {
    differential::run_fixture("0011-admin-stats-prometheus")
        .await
        .expect("fixture green");
}
```

**LoC estimate D6.fixture:** ~80 LoC fixture YAMLs + ~40 LoC README + ~30 LoC expectations.yaml + 7 LoC Docker-gated test. Subtotal: **~160 LoC**.

**LoC estimate D6 total:** ~170 (harness) + ~160 (fixture) = **~330 LoC** (slightly under parent-06 SPEC §3 D6.1's projection of ~400 LoC harness + 5 fixture files).

---

### D7 — State-4 phase-done verification (verification deliverable, no code)

State-4 phase-done verification per the `BOOTSTRAP_PROMPT.md` §7.5 gate, scoped to 06.1's surfaces. The PROGRESS.md final task entry quotes the CI run URL + per-fixture results inline, mirroring 05.3's PROGRESS-task-12 shape. Specifically:

- (a) fixture `0011-admin-stats-prometheus/` green link;
- (b) fixtures 0001–0010 green links;
- (c) `tests/conformance/h2spec/` ≥95% pass link (carry-forward, not re-run);
- (d) `parse_bootstrap` fuzz short-budget run clean;
- (e) `cargo build / clippy / fmt / test / deny check` clean on stable-toolchain CI;
- (f) REVIEW.md verdict `Approved`.

**No code in D7.** ~0 LoC.

---

**LoC budget total for 06.1:** D1 ~670 + D2 ~580 + D3 ~150 + D4 ~200 + D5 ~30 + D6 ~330 + D7 0 = **~1960 LoC**, ~50% drift over parent-06 SPEC §3's projection of ~1300 LoC. The drift is concentrated in D1 (the stats primitives' multi-module decomposition + thorough torture-test surface) and D2 (the admin handler's per-endpoint-per-method test surface). Per parent-06 SPEC §5's drift-headroom argument, 06.1 stays within the §6.1 split-gate's "~1500 LoC" guardrail by leaning on the parent-06 SPEC §5 rule "do not nest-split a sub-phase that was itself produced by a split". The PLAN-write planner records the drift posture in PROGRESS Task 1 per the established 05.2 / 05.3 cadence.

---

## 4. Out of scope (deferred non-goals)

The following surfaces are **explicitly deferred** to later sub-phases or later phases. The list is a subset of parent-06 SPEC §4, scoped to items that are predictably tempting to fold into 06.1 by a planner reading only this SPEC.

**Deferred to sub-phase 06.2:**
- Access log subsystem (new `crates/envoy-accesslog/` crate; `AccessLogRecord` + `Sink` trait + `FileSink` + `default_format` emitter; `HttpConnectionManagerConfig.access_log` schema; HCM on-response-complete wiring; fixture `0012-access-log-file-sink`; BEHAVIOR_CONTRACT.md `Access log field mapping` section). Parent-06 SPEC §3 D8.2–D13.2.
- `Admin.access_log_path` is parsed-and-ignored at 06.1 per D5.a; admin-side access-log emission defers indefinitely (06.2 ships the data-plane HCM access-log; admin-side access-log is not on 06.2's scope).

**Deferred to sub-phase 06.3:**
- **Comprehensive stats wiring.** Per-response-class HCM counters (`http.<prefix>.downstream_rq_2xx`, `_3xx`, `_4xx`, `_5xx`); connection-lifetime gauges (`listener.<name>.downstream_cx_active`, `cluster.<name>.upstream_cx_active`); upstream-side HCM counters (`cluster.<name>.upstream_rq_total`, `_5xx`); access-log line counter (`http.<prefix>.access_logs_total`); listener accept-failure counter (`listener.<name>.downstream_cx_accept_failed`). 06.1 ships only the representative subset (one counter per layer).
- Fixture 0011's `expectations.yaml` extension to assert the comprehensive set of counters/gauges. 06.1 lands the fixture with the representative subset only.
- 05.3 REVIEW I1 closure (Http2ClusterFromHttp1Listener parse-time validator gate). Lands as 06.3 Task 1 preamble per parent-06 SPEC §3 D14.3.
- Per-name value assertions in `BodyRule::PrometheusExposition`. 06.1 asserts on metric-name presence only; 06.3 extends to a per-name value-disposition map (or its forward-compatible equivalent).
- Parent ROADMAP row `06` flip to `done`. Happens at sub-phase 06.3's state-6 phase-done commit, not 06.1's.

**Deferred to later phases (per parent-06 SPEC §4 — items relevant to 06.1's surface):**
- **Histograms.** Counter + gauge primitives only in 06.1. Per-request latency distributions defer to a later observability-family phase. Prometheus histogram exposition format also defers.
- **Stats labels / `tag_specifiers`.** Envoy supports tag extraction from stat names (e.g., `cluster.svc_a.upstream_cx_total` → `cluster_upstream_cx_total{cluster="svc_a"}`). 06.1 emits stats as flat names. Tag extraction defers.
- **JSON-format stats.** 06.1 ships text-format only (`/stats` plain-text + `/stats/prometheus` Prometheus exposition). Envoy's `/stats?format=json` defers to whichever phase first needs it.
- **Stats sinks beyond the in-process registry.** No `metrics_service` cluster; no stats-flush-to-cluster. The Prometheus exposition is read-on-demand from the registry. External stats sinks defer to the Observability family.
- **Admin endpoints beyond `/ready`, `/stats`, `/stats/prometheus`.** `/clusters`, `/listeners`, `/server_info`, `/config_dump`, `/runtime`, `/runtime_modify`, `/logging`, `/quitquitquit`, `/healthcheck/fail`, `/healthcheck/ok` defer to phase 08 (Minimum admin API per `BOOTSTRAP_PROMPT.md` §8 row 08).
- **Admin endpoint prefix matching / path parameters.** Per cross-sub-phase architectural rule 5, exact-match only in 06.1. `/clusters/<name>` and similar defer to phase 08.
- **Admin endpoint POST / non-GET methods.** 06.1 returns 405 for non-GET. POST endpoints (`/quitquitquit`, `/healthcheck/fail`, `/runtime_modify`) defer to whichever phase first ships them.
- **HTTP/2 admin listener.** Per cross-sub-phase architectural rule 3, HTTP/1.1 only in 06.1. H2 admin defers indefinitely.
- **TLS admin listener.** Plaintext only in 06.1. TLS-protected admin defers (Envoy supports it but this project's threat model treats admin as localhost-only for now).
- **Graceful drain.** The admin migration in 06.1 does not engage drain semantics. Drain defers to phase 08.
- **HTTP/1.1 keep-alive on the admin listener.** 06.1 closes the connection after each admin request (sufficient for Prometheus scrape, curl `/ready`, and the test harness's single-request driver). Keep-alive defers if/when scrape rates demand it.
- **Stat-name reload / dynamic-stat lifecycle.** Stats live in the registry forever in 06.1; LRU eviction, scope-bound stats, and dynamic-cluster stat lifecycle defer to xDS-family phases.
- **Stats config: `stats_config.use_all_default_tags`, `stats_matcher`, `stats_tags`.** 06.1 ignores `stats_config` blocks if present (parse-and-ignore per ADR-0026 pattern, if needed at fixture writeup time). Stats matcher / tag filtering defers.
- **TcpProxy-side stats.** TcpProxy has a `stat_prefix` field (line 262) but no stats are wired in 06.1. Defers; potentially absorbed by 06.3 if scope permits, otherwise deferred indefinitely.

**Deferred carryforwards (per parent-06 SPEC §4):**
- **Phase 05.3 REVIEW I1** (Http2ClusterFromHttp1Listener validator gate). Defers to 06.3 Task 1 preamble. 06.1 does not engage.
- **Phase 05.3 REVIEW I2** (typed-error chain dissolution at H2 dispatch site). Not engaged by 06.1; carries forward unchanged.
- **Phase 05.2 REVIEW I1** (h2spec tarball SHA-256 verification in CI). 06.1 does not edit `.github/workflows/ci.yml`; carries forward unchanged.
- **Phase 05.2 REVIEW I2 / I3** (Http2Error variant rename, MalformedH2HeaderBlock split). Not engaged; carries forward unchanged.
- **Phase 04.1 REVIEW M5/M9** (Cargo.lock cadence ratification ADR). 06.1 introduces no new top-level Cargo deps under the recommended posture; M5/M9 carries forward unchanged. If conditional ADR-0030 lands (foundations grant), this is the natural site to ratify a Cargo.lock cadence ADR-0031.
- **Phase 04.1 REVIEW M7** (TLS+H2 ALPN-driven dispatch generalization). Not engaged; carries forward unchanged.
- **Phase 04.1 REVIEW M-claim** (drive_http1 per-function unit test). Not engaged; carries forward unchanged.
- **Phase 02.2 REVIEW M1** (`*EchoBackend::Drop` polling loop blocks on `std::thread::sleep`). Standing carryforward; 06.1's in-process backstop test (D3 step 6) inherits the SIGKILL-on-Drop posture per the 04.x / 05.x integration-test precedent. 06.1 does not parallelize `run_fixture`; M1 continues unchanged.

**Not deferred — confirmed in scope for 06.1** (for clarity, since these have predictable confusion points):
- `crates/envoy-stats/` and `crates/envoy-admin/` are BOTH created in 06.1. The split between them mirrors the cross-sub-phase architectural rule 1 separation.
- The phase-01 admin migration (D3) is in 06.1, not 06.2 or 06.3. Fixture 0002's regression is the load-bearing pre-existing-fixture regression-guard.
- `Admin.access_log_path` is added (parse-and-ignore) at 06.1 per D5.a, NOT at 06.2 (06.2 owns the data-plane HCM `access_log` field, not the admin-side log path).
- The representative stats subset (3 counters) is fully wired in 06.1; 06.3 extends.
- `Driver::AdminScrape` and `BodyRule::PrometheusExposition` are introduced in 06.1; reused unchanged in 06.3's fixture-0011 expectations extension.
- `BEHAVIOR_CONTRACT.md` `Stat-name mapping` section is populated at 06.1 (3 initial rows), NOT at 06.3 (which extends the table only).

---

## 5. Phase-01 admin-migration regression-equivalence posture

Fixture `0002-static-admin-ready/` is the load-bearing regression-guard for D3's admin migration. The migration replaces the bare-bones `/ready` handler at `crates/envoy-bin/src/main.rs:305-320` with a full `envoy_admin::AdminHandler`-shaped `ConnectionHandler`; the fixture's `/ready` semantics must remain byte-equivalent against upstream Envoy.

**The dual-track guard:**

1. **Docker-gated fixture 0002** (existing, unchanged at the YAML level). The fixture spawns both proxies via testcontainers, drives a `GET /ready` HTTP/1.1 request via `drive_http1`, and asserts byte-equivalence on status + body + allow-listed headers. The fixture continues green at 06.1's state-4 acceptance gate (b). If the migration accidentally changes the response (e.g., emits `200 OK\r\n\r\nLive\n` instead of `200 OK\r\n\r\nLIVE\n`, or omits the trailing `\n`, or changes the `content-type`), fixture 0002 turns red and the migration is rejected.

2. **In-process backstop test** at `crates/envoy-bin/tests/admin_ready.rs` (NEW in 06.1; see D3 step 6). Spawns envoy-bin via `CARGO_BIN_EXE_envoy-bin`, drives the `GET /ready` request via tokio TcpStream (no Docker), and asserts status 200 + body `LIVE\n`. Runs under `cargo test --workspace` regardless of CI's Docker availability. Fast (sub-second); independent of testcontainers/network.

**Why both:** The Docker-gated fixture is the differential gate (against upstream Envoy); the in-process backstop is the local regression gate (under all developer machines and all CI shapes regardless of Docker). Together they catch (i) envoy-rust-side regressions detectable locally and (ii) divergence-from-Envoy regressions detectable only in CI. Mirrors the phase-04.1 backstop posture (`crates/envoy-bin/tests/http1_direct_response.rs` covers fixture 0007's regression equivalence locally; the Docker-gated `tests/differential/tests/http1_direct_response.rs` covers the differential side).

**Migration-time cross-checks** (recorded in PROGRESS.md at the migration task's commit):

- The status line is `200 OK` (not `200`, not `200 LIVE`, not `200 ok`).
- The body is exactly `LIVE\n` (5 bytes; final newline mandatory; matches the pre-migration shape).
- The `content-type` response header is `text/plain` (matches the pre-migration shape; a stricter `text/plain; charset=utf-8` would diverge from Envoy's emission and is **not** introduced in 06.1; if Envoy emits the longer form, the fixture's allow-list grows; if not, both proxies match exactly).
- The pre-migration shape's `content-length` header is preserved (`5`); the response is non-chunked.
- The pre-migration shape's `server` header is preserved (envoy-rust default `server: envoy-rust`; covered by the existing 04.x `HEADER_ALLOW_LIST` row).
- The pre-migration shape's `date` header is preserved (covered by the existing allow-list row).
- The pre-migration shape's connection-close behavior is preserved (the connection closes after the `/ready` response; no keep-alive in 06.1 per §4 above).

If any of these cross-checks turn up a divergence at task time, the planner adjusts `envoy_admin::endpoint::AdminEndpoint::Ready.render(...)` to match the pre-migration shape exactly, and records the adjustment in PROGRESS.md. **Mirroring the pre-migration shape is non-negotiable** — the migration is a regression-equivalence migration, not a behavior-extension.

---

## 6. Implementation signposts for the planner

Notes flagging predictable planner questions so the 06.1 planner resolves them in-plan rather than mid-execution. Format mirrors 05.2 SPEC §6.

**Inherited from parent-06 SPEC + 05.2/05.3 precedent:**

1. **`async_trait` posture on `envoy_listener::ConnectionHandler` — load-bearing at D2.** The trait at HEAD `82c26b8` (`crates/envoy-listener/src/lib.rs:29`) reads `pub trait ConnectionHandler: Send + Sync + 'static { fn handle(&self, …) -> BoxFuture<'_, std::io::Result<()>>; }` (verifiable inline; signpost 19 of the 05.2 SPEC documented this same shape). 06.1's `AdminHandler` mirrors the trait shape verbatim. **Do not introduce `async_trait` ad-hoc** — match what's already in-tree. The `BoxFuture` pattern is also documented in `envoy_listener`'s lib.rs preamble.

2. **`tokio` re-export vs `std::sync::Arc` in `envoy-stats` — picked at D1 task-1 time.** `envoy-stats` exports `Arc<Counter>` / `Arc<Gauge>` from the registry. Two options: (a) use `std::sync::Arc` directly (no tokio dep); (b) re-export `tokio::sync::Arc` (which is just `std::sync::Arc` aliased; no advantage). **Recommendation: (a)** — `envoy-stats` is runtime-agnostic; the `[dependencies]` block in D1's `Cargo.toml` lists no `tokio` runtime dep. Matches D-3.4 (foundation crates do not transitively pull async runtimes unless they need them).

3. **`Counter` AtomicU64 ordering — `Relaxed` per recommendation.** Stats counters' `inc()` / `add()` use `Ordering::Relaxed` because the program does not synchronize control flow on stats values (they are read-only at scrape time, no happens-before contract is needed). **Alternative considered:** `Ordering::Release` for inc + `Ordering::Acquire` for read — rejected, no performance gain because stats reads/writes don't observe memory written before the inc. Test 4 (multi-thread torture) verifies the Relaxed ordering is sound under realistic load.

4. **`AdminHandler` as `ConnectionHandler` vs adapter at envoy-bin — `AdminHandler` directly per recommendation.** Two options: (a) `AdminHandler` directly implements `envoy_listener::ConnectionHandler` (06.1 chooses this); (b) `AdminHandler` is a struct with a `serve(stream)` method, and a small adapter at envoy-bin wraps it into `ConnectionHandler`. **Recommendation: (a)** — keeps the surface flat and matches the data-plane HCM patterns (`envoy_http1::HCM`, `envoy_http2::HCM` both implement `ConnectionHandler` directly). The adapter pattern (b) only earns its keep when multiple `ConnectionHandler` consumers want to share an `AdminHandler` instance, which is not 06.1's case.

5. **HCM-side increment site — entry vs exit path.** The `http.<stat_prefix>.downstream_rq_total` counter increments on every HCM-handled request. Two options for the increment site: (a) entry — increment on first request-line read (counts attempts including malformed requests that 400); (b) exit — increment after a `BuildOutcome` is reached (counts only well-formed requests). Envoy's behavior is (a) entry (verifiable against upstream by inspecting Envoy's HCM impl). **Recommendation: (a)** for fidelity. The counter increments once per call to the per-request HCM handler, before the request bytes are parsed.

6. **`StatsRegistry` map — `BTreeMap` vs `HashMap` — `BTreeMap` per recommendation.** Two options: (a) `BTreeMap<String, StatHandle>` (sorted by name; deterministic snapshot order); (b) `HashMap<String, StatHandle>` (faster lookup, non-deterministic order). Stats registration is one-shot at consumer construction time (sub-ms total); scrape lookup is O(log n) under BTreeMap, O(1) under HashMap. n is bounded at ~50–100 in 06.1 (representative subset) and will grow to ~200–500 across the project's lifetime. **Recommendation: (a)** — diff-friendly snapshot order outweighs the negligible lookup cost.

7. **Admin endpoint Method allow-list — GET-only per recommendation.** Three options: (a) GET-only (current); (b) GET + HEAD (HEAD is essentially GET with body suppression; trivial extension); (c) GET + HEAD + POST (anticipates `/quitquitquit` etc.). **Recommendation: (a)** for 06.1; future endpoints land their method extensions at the phase that introduces them. HEAD support is trivially extensible at the `AdminEndpoint::render` level if/when a fixture demands it.

8. **Prometheus exposition emitter — `bytes::BytesMut` vs `std::io::Write` — `BytesMut` per recommendation.** Two options: (a) `pub fn write_exposition(registry: &StatsRegistry, w: &mut bytes::BytesMut)` (allocates into a Bytes-friendly buffer; consumed by `envoy-http1::codec::Response::body: Bytes` directly with no copy); (b) `pub fn write_exposition(registry: &StatsRegistry, w: &mut dyn std::io::Write) -> std::io::Result<()>` (more general; supports streaming to file, etc.; requires `?`-able call sites). **Recommendation: (a)** — admin exposition is read-on-demand into an HTTP response body; the BytesMut shape avoids a copy. Swap to (b) only if a future stats sink needs streaming.

9. **`stat_prefix` schema disposition — already-required, consume-as-is.** Per parent-06 SPEC §3 D5.1's projection of `stat_prefix: Option<String>`: the field is **already required** at HEAD `82c26b8` (`pub stat_prefix: String` at line 351). No schema change in D5; the existing field is consumed at HCM construction time per D4.c. The parent SPEC's projection was inaccurate for the current codebase shape; 06.1 corrects it inline rather than landing an unnecessary `Option<String>` migration. The PLAN-write planner records this correction in PROGRESS Task 1.

10. **Fuzz corpus seed — `admin_with_stats_route.yaml` shape.** New seed lands at `crates/envoy-config/fuzz/corpus/parse_bootstrap/admin_with_stats_route.yaml`. Mirrors the existing 04.x / 05.x seed shape (full Bootstrap with one HCM listener + one admin block + `clusters: []`); the new field exercised is `Admin.access_log_path`. Allow-list entry `!corpus/parse_bootstrap/admin_with_stats_route.yaml` added to `crates/envoy-config/fuzz/.gitignore`.

**06.1-local signposts:**

11. **Counter-write-then-scrape grace period — 50ms per recommendation.** The `Driver::AdminScrape`'s `drive_admin_scrape` helper (D6.c) sleeps ~50ms between the last pre-request and the admin scrape so the registry's `Relaxed`-ordered counter writes are observable to the scrape's read. Under x86-64 TSO this is overkill; under aarch64 with weaker ordering it provides headroom against subtle visibility timing. **Alternative considered:** use `Ordering::Release` on `inc()` + `Ordering::Acquire` on `value()` — rejected per signpost 3 (no other synchronization couples to stats values). The 50ms sleep is harness-level coverage, not a primitive-level requirement.

12. **`expectations.yaml` allow-list seeding — empirical at task time.** The fixture 0011 `expectations.yaml`'s `allowlist_envoy_only` list is **not pre-populated** at SPEC writeup time — the actual list of Envoy-only metric names is discovered when the harness first runs the scrape against upstream Envoy v1.33. The planner runs the harness once with an empty allow-list, captures the resulting "envoy-only metric names" diff from the failure message, populates the allow-list with a one-line doctrine reason per name (e.g., `# server.uptime — server-state stat; envoy-rust does not track in 06.1`), and reruns. Mirrors phase 05.2 D7's `known-failures.txt` empirical-population posture. The list is anticipated to be ~30–50 entries; the planner trims to a stable set at task time.

13. **In-process backstop port-discovery — log-scrape per recommendation.** The backstop test at `crates/envoy-bin/tests/admin_ready.rs` binds the admin listener to `port_value: 0` (kernel ephemeral); the bound port is captured by parsing envoy-bin's `tracing::info!(%addr, "envoy-rust listening (admin)")` log line via a small regex. **Alternative considered:** have envoy-bin write the bound ports to a JSON status file at a known path — rejected, larger surface change. The log-scrape is consistent with 04.1's / 05.2's in-process integration-test posture.

14. **Listener / Cluster constructor signature ripple.** D4 introduces a `registry: Arc<StatsRegistry>` argument on `envoy-listener::Listener::new` (or whatever constructor exists at HEAD `82c26b8`) and `envoy-cluster::from_bootstrap`. The ripple touches every call site that constructs a Listener or runs `from_bootstrap`. The planner verifies at D4 task-1 time that the call sites are limited to `crates/envoy-bin/src/main.rs` + the existing `crates/envoy-listener/src/lib.rs::tests` + `crates/envoy-cluster/src/cluster.rs::tests`. Test sites can use `Arc::new(StatsRegistry::new())` ad-hoc.

15. **Per-stat `# HELP` lines — minimal in 06.1.** The Prometheus exposition emitter emits `# TYPE` lines (counter/gauge) per metric but emits `# HELP` lines only as a generic placeholder (e.g., `# HELP <name> envoy-rust stat`). Rich descriptions per metric defer; populating per-metric help text is a 06.3+ concern (Envoy's own `# HELP` lines are derived from per-stat documentation strings). The harness body rule asserts on metric-name presence only and does not assert on `# HELP` content, so the minimal emission is sufficient for fixture 0011.

16. **`admin_with_stats_route.yaml` fuzz seed — corpus bookkeeping.** Per signpost 10 + the established corpus-walk acceptance test pattern (each new seed gets a `fuzz_corpus_<seed>_seed_parses` test in `crates/envoy-config/src/bootstrap.rs::tests`), 06.1 lands the seed + the corpus-walk acceptance test in lockstep at D5's task. The fuzz target itself (`parse_bootstrap`) is unchanged in 06.1; only the corpus grows.

17. **`#![forbid(unsafe_code)]` on both new crates.** Per D-3.8, `crates/envoy-stats/src/lib.rs` and `crates/envoy-admin/src/lib.rs` both carry the attribute. No `unsafe` in 06.1.

18. **`BEHAVIOR_CONTRACT.md` edit cadence — Task 1 inline, not at fixture commit.** Per the established phase-04.3 / phase-05.2 posture (BEHAVIOR_CONTRACT.md `Header allow-list` rows landed alongside the in-code `HEADER_ALLOW_LIST` constant at the introducing task), 06.1 lands the 3 `Stat-name mapping` rows at the D4 task that wires the counters (or one task earlier if the planner sequences D4 after D1). The doc edit is inline with the code that produces the stats; a standalone "BEHAVIOR_CONTRACT.md update" task is not needed.

19. **PLAN.md cadence — standalone pre-Task-1 commit.** Per the established `c02eea7` precedent (phase-04.3) and 05.1/05.2/05.3's continuation, 06.1's PLAN.md is committed standalone at state-2 close-out, before any Task 1 commit.

20. **LoC-budget reality check at PLAN-write time.** The parent-06 SPEC §3 / ADR-0029 brainstorm projected 06.1 at "~1300 LoC, ~12 tasks." This SPEC's §3 D1–D7 deliverable estimates total **~1960 LoC** — a ~50% drift from the parent's projection, larger than phase-05.2's 58% (similar magnitude; D1 + D2 are larger-than-estimated due to thorough test coverage). Per parent-06 SPEC §5, do not nest-split a sub-phase that was itself produced by a split. The PLAN-write planner accepts the estimate and proceeds; if the actual PLAN-time refinement crosses 25 tasks, the planner invokes `superpowers:systematic-debugging` first to confirm the scope is genuine, not creep.

---

## 7. ADRs expected from this sub-phase

Phase 06.1's ADR ledger entrance state is **ADR-0029** (the parent-06 split decision; lands at parent-06 state-2 alongside this SPEC; predecessor is ADR-0028 from phase 05.3 task 6 / commit `83e4da7`).

**No required ADRs in 06.1** under the recommended posture. Conditional ADRs available:

### ADR-0030 (CONDITIONAL) — foundations grant for `time = "0.3"` or `async_trait = "0.1"`

- **Status:** **NOT pre-projected** to land. Recommended posture is no foundations grants in phase 06 (06.1 in particular). The phase-06 hand-roll posture is deliberate: counters/gauges over `std::sync::atomic::{AtomicU64, AtomicI64}`; registry over `std::sync::RwLock<BTreeMap<...>>`; Prometheus exposition emitter over `bytes::BytesMut`; trait shapes over `BoxFuture` (matching 04.x's `ConnectionHandler` posture) — none of these surface a foundations-grant pressure point.
- **Lands at 06.1 IF:** execution-time experience surfaces a hand-roll cost materially exceeding the dep cost (e.g., the `BoxFuture` shape for `AdminHandler`'s `ConnectionHandler` impl is unexpectedly painful and `async_trait = "0.1"` would land cleanly). The planner records the rationale in DECISIONS.md per the established phase-05.3 / phase-04.2 inline-ADR-landing precedent.
- **Provenance:** projected as conditional in parent-06 SPEC §7. The number `ADR-0030` stays available at this entrance. If 06.1 does not land it, 06.2 / 06.3 may; if no sub-phase of phase 06 lands it, the number stays available for phase 07+.

### ADR-0031 (CONDITIONAL) — Cargo.lock cadence ratification

- **Status:** **NOT pre-projected** to land. Conditional on ADR-0030 actually landing (per parent-06 SPEC §7 + phase-04.1 REVIEW M5/M9 carryforward chain). If 06.1 introduces no new top-level Cargo deps (recommended posture), there's nothing to ratify.
- **Lands at 06.1 IF:** ADR-0030 lands AND the dep introduction surfaces a Cargo.lock cadence question worth ADR-grade documentation.
- **Provenance:** projected as conditional in parent-06 SPEC §7. The number stays available if not landed.

### ADR-0032+ (CONDITIONAL) — sub-phase-specific decisions

If a Y/N decision surfaces during 06.1 execution that isn't covered by ADR-0030 / ADR-0031 (e.g., a `BEHAVIOR_CONTRACT.md` allow-list extension forced by an unexpected admin-response-header surface, a `BodyRule::PrometheusExposition` parse-edge ratification, an unexpected divergence in `/ready` body shape), the planner appends the next-sequential ADR (ADR-0030 if 0030 has not yet landed; ADR-0031 if 0030 landed but 0031 has not; etc.) at the time it lands. Mirrors the established phase-05.4 ADR-renumbering cadence (where 05.4 landed ADR-0024 / ADR-0025 / ADR-0026 in task-execution order).

**Default expectation:** 06.1 lands no ADRs. The ledger head before 06.1 Task 1 is ADR-0029; after 06.1 close, it likely remains ADR-0029.

---

## 8. State-machine signposts for 06.1's own state-2 session

The 06.1 state-2 session (the next session after this brainstorm; runs `superpowers:writing-plans`) operates per `SKILL_ROUTING.md` line 21: *"SPEC.md exists, PLAN.md does not → superpowers:writing-plans → output: PLAN.md → GATE: if PLAN.md > ~25 tasks OR > ~1500 LoC estimated → split into NN.1, NN.2, …; update ROADMAP + STATE; stop"*.

**At 06.1 state-2 the session lands:**

1. **PLAN.md** at `docs/envoy-rust/phases/06.1-stats-and-admin/PLAN.md` — refines D1–D7 into per-task atomic units. Estimated ~12 tasks per parent-06 SPEC §3; concrete task breakdown is the planner's output. The PLAN-write planner records the LoC-drift posture per signpost 20 in PROGRESS Task 1.
2. **ROADMAP.md update** — row `06.1` flips `status: planned` → `status: in-progress` per the §4.1 invariant ("a phase enters `in-progress` only when STATE.md points at it" — STATE.md now points at 06.1 with state 3 once PLAN.md lands).
3. **STATE.md update** — active phase id stays `06.1`; lifecycle state advances 2 → 3; next-skill `superpowers:subagent-driven-development` per the user's standing preference (per auto-memory `feedback_execution_style`; matches 05.x's posture).

**The 06.1 state-2 session does NOT land:**

- Per-task PROGRESS.md entries (those land per-task during execution).
- REVIEW.md (lands at state 5).
- The `envoy-stats` / `envoy-admin` crate scaffolds (those land at PLAN's Task 1).
- ADR-0030 / ADR-0031 (those land conditionally during execution per §7 above).

**The §6.1 split-gate at 06.1 state-2.** If PLAN.md surfaces >~25 tasks or >~1500 LoC estimated (the LoC drift per signpost 20 already projects ~1960 LoC, so the LoC gate is materially in play), the standard §6.2 protocol applies:
- The planner invokes `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1 to confirm the scope is genuine vs creep.
- If genuine, the planner invokes the parent-06 SPEC §5 rule "do not nest-split a sub-phase that was itself produced by a split" — i.e., 06.1 does not re-split into 06.1.1 / 06.1.2; it accepts the estimate and proceeds.
- If creep, the planner trims (e.g., reducing test coverage in D1 / D2 to stay under 1500 LoC; recommendation per signpost 20: do NOT trim, accept the estimate).

**Sub-phase entry point.** After 06.1 state-2 lands PLAN.md, the next session enters 06.1 lifecycle state 3 — runs `superpowers:subagent-driven-development` against PLAN.md tasks, executes each task to TDD discipline, and the cycle continues through state 4 (verification) and state 5 (review).

---

## 9. Final commit message format (for state 6 of the 06.1 lifecycle)

The 06.1 phase-done commit flips ROADMAP row `06.1` `in-progress` → `done`; parent row `06` stays `in-progress` (flips at 06.3's phase-done commit per the ROADMAP-schema invariant). Format models the 04.x / 05.x sub-phase shape per `BOOTSTRAP_PROMPT.md` §5.3:

```
phase 06.1: envoy-stats + envoy-admin + admin migration + fixture 0011 [<ADR-list>]

New workspace member crates/envoy-stats/ ships counter / gauge
primitives + StatsRegistry over std::sync::RwLock<BTreeMap> +
Prometheus text-exposition emitter over bytes::BytesMut. Hand-rolled
atop std atomics; no new permitted-foundations grant under the
recommended posture (no `prometheus`, no `metrics`, no `time`,
no `chrono`, no `async_trait`).

New workspace member crates/envoy-admin/ ships AdminHandler +
AdminEndpoint::{Ready, Stats, StatsPrometheus} + AdminConfig +
AdminError. HTTP/1.1 only; exact-match path routing; GET-only.
ConnectionHandler-shaped to plug into envoy-listener's accept loop
verbatim.

Phase-01 admin migration: the bare-bones /ready handler at
crates/envoy-bin/src/main.rs is replaced by an HCM-backed listener
constructed from envoy_admin::AdminHandler, threading the global
Arc<StatsRegistry>. New in-process backstop test
crates/envoy-bin/tests/admin_ready.rs proves the migration preserves
fixture 0002's /ready semantics regardless of Docker availability.

Representative stats wiring across listener / cluster / HCM:
- listener.<name>.downstream_cx_total (per-accept; value-exact).
- cluster.<name>.upstream_cx_total (per-upstream-connect;
  name-required-value-may-differ — Envoy default-pool semantics
  diverge from envoy-rust's no-pooling regime).
- http.<stat_prefix>.downstream_rq_total (per-request; value-exact).

envoy-config schema: Admin.access_log_path: Option<String> parse-
and-ignore per ADR-0026 pattern. HttpConnectionManagerConfig.
stat_prefix is already required (no schema change); consumed at HCM
construction time. ~3 new validator unit tests + 1 fuzz corpus seed
(admin_with_stats_route.yaml).

Differential harness: Driver::AdminScrape + BodyRule::
PrometheusExposition (asserts on metric-name set modulo per-fixture
allow-list; values not asserted — value-exact rows defer to 06.3's
extension).

Fixture 0011-admin-stats-prometheus (5 files): admin block with
/ready + /stats + /stats/prometheus + a sibling HCM listener that
drives one request through HCM/cluster/listener so the representative
counters increment; harness scrapes /stats/prometheus after the
request and asserts the metric-name set.

BEHAVIOR_CONTRACT.md Stat-name mapping section populated for the
first time: 3 initial rows for the representative subset. 06.3
extends comprehensively. Header allow-list unchanged.

Phase-04.1 REVIEW M5/M9 carries forward unchanged (no new top-level
Cargo deps under the recommended posture). Phase-05.3 REVIEW I1
(Http2ClusterFromHttp1Listener validator gate) defers to 06.3 Task 1
preamble.

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged at YAML;
    /ready semantics preserved byte-equivalent through admin migration);
  tests/fixtures/0003-tcp-proxy through 0010-http2-router-upstream
    green (unchanged);
  tests/fixtures/0011-admin-stats-prometheus green (NEW; admin
    Prometheus scrape returns the representative-subset metric names
    matching upstream Envoy modulo the per-fixture allow-list).
Conformance: tests/conformance/h2spec at ≥95% pass (carry-forward,
  not re-run; 06.1 does not engage H2 framing).
```

The bracketed `<ADR-list>` is empty `[]` if no ADR landed (recommended posture); `[ADR-0030]` if the foundations grant landed; `[ADR-0030, ADR-0031]` if both conditional ADRs landed; etc. The tag does **NOT** include `[parent 06 done]` since 06.1 is not the closing sub-phase.

ROADMAP row `06.1` flips `in-progress` → `done` at this commit. Parent row `06` stays `in-progress` (flips at 06.3's state-6 phase-done commit per the ROADMAP-schema invariant). STATE.md advances to phase `06.2` lifecycle state 2 (06.2's SPEC was landed at parent-06 state-2 alongside this one); next-skill `superpowers:writing-plans` scoped to sub-phase 06.2 (access-log foundation + HCM wiring + fixture 0012 per parent-06 SPEC §3 D8.2–D13.2).

---

## 10. State-machine commit (parent-06 state-1 → state-2 close-out reference)

This SPEC lands at the **parent-06 state-2 commit**, alongside:

- `docs/envoy-rust/DECISIONS.md` — appends **ADR-0029** (the parent-06 split decision; predecessor ADR-0028).
- `docs/envoy-rust/phases/06.2-access-log/SPEC.md` — sub-phase-06.2 SPEC.
- `docs/envoy-rust/phases/06.3-stats-wiring-and-close/SPEC.md` — sub-phase-06.3 SPEC.
- `docs/envoy-rust/ROADMAP.md` — three new rows (`06.1`, `06.2`, `06.3`) with `status: planned`; parent row `06`'s `sub-phases` column updated to `06.1, 06.2, 06.3`; row `06`'s `status` stays `in-progress`.
- `docs/envoy-rust/STATE.md` — advanced to point at `06.1` lifecycle state 1; next-skill `superpowers:brainstorming` scoped to 06.1 (which produces the next session's SPEC for 06.1 — but per the parent-04 / parent-05 precedents the parent-06 state-2 session also writes the sub-phase SPECs, including this one, so 06.1 enters lifecycle state 2 directly with the SPEC already in tree).

The parent-06 state-2 session does **not** land per-sub-phase PLAN.md files — those land at each sub-phase's own state-2 sessions per the precedent. The parent-state-2 session writes only the parent-level split-coordination artifacts.

**No code changes at the parent-06 state-2 commit.** No new crates, no schema changes, no harness changes. The state-2 commit is documentation + ledger + roadmap + state advancement only.

After the parent-06 state-2 commit lands, the next session enters 06.1 lifecycle state 2 — runs `superpowers:writing-plans` scoped to this SPEC, lands `06.1-stats-and-admin/PLAN.md`, advances ROADMAP row `06.1` to `in-progress`, advances STATE.md to 06.1 lifecycle state 3, and exits.
