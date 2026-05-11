# Phase 06.3 (`06.3-stats-wiring-and-close`) — PROGRESS

> Per-task narrative log appended at each substantive commit. Stranger-readable
> per D-3.4. Task 1 lands AT the state-2 standalone PLAN.md commit (no separate
> Task 1 commit per the standalone-PLAN cadence established by 06.1
> `505653d` and 06.2 `dc00750`).

## Task 1 — PROGRESS.md preamble + LoC drift posture + 5 SPEC corrections + 22 architecture decisions (state-2 commit)

This task lands AT the state-2 standalone PLAN.md commit. The remaining Tasks
2-12 land at state-3 each as their own commit.

### LoC drift posture (per BOOTSTRAP_PROMPT.md §6.1 + parent-06 SPEC §5 alternative (vi))

06.3 SPEC §3 projects ~770 LoC code+tests + ~80 review/state-6 overhead.
Task-count projection: 12 tasks. Both projections are comfortably under the
§6.1 split-gate (~25 tasks or ~1500 LoC of net change).

Per parent-06 SPEC §5 alternative (vi), 06.3 may NOT nest-split itself even
if execution-time drift pushes a task over its task-local budget — the
accept-drift posture is the established release valve. The 06.1 + 06.2
precedent ratifies this: 06.1 SPEC projected ~1300 LoC and PLAN landed
~2010 LoC; 06.2 SPEC projected ~1300 LoC and PLAN landed ~1875 LoC; both
honored the no-nest-split posture and absorbed the ~+50% PLAN-vs-SPEC
narrative-density growth without re-splitting.

The PLAN-write planner cross-checked the SPEC's 770-LoC code estimate against
the in-tree code surfaces touched by each task. The estimate holds at
PLAN-write time (no surprise scope discovered at code-read).

### PLAN-write SPEC corrections (recorded for the executor; 5 corrections)

Mirrors 06.1's 4 corrections + 06.2's 4 + 1 clarifying. Per D-3.5, the
SPEC remains in-tree unedited; corrections are recorded HERE so a stranger
reading PROGRESS catches the SPEC-vs-implementation diff:

1. **SPEC §3 D15.3.a wrongly co-locates per-class HCM counter increment with
   06.1's `downstream_rq_total` increment site.** Empirically the 06.1
   increment fires at request-entry time (`crates/envoy-http1/src/hcm.rs:251`,
   not at on-response-complete. Resolution: per-class counters land at the
   factored access-log dispatch site (post-`match outcome` block, lines 459+),
   after `response_status_for_log` is populated. 06.1's request-entry
   `downstream_rq_total.inc()` continues unchanged at line 251. PLAN Task 4
   names the exact insertion point.

2. **SPEC §3 D15.3.b's listener gauge claim needs to factor 06.1 D4.a's
   `let cx_total = self.cx_total;` hoist for the `tokio::select!` accept-arm
   capture.** Empirical at `crates/envoy-listener/src/lib.rs:143-160`. The
   new `cx_active` gauge follows the same hoist pattern. Per signpost 7 the
   gauge scopes to data-path listeners only — the planner threads a
   `count_active: bool` field through `ListenerConfig`, defaulting to `true`
   and overridden to `false` at envoy-bin's admin-listener construction.
   PLAN Task 5 names the exact wiring.

3. **SPEC §3 D15.3.c proposes adding `cluster: &ClusterHandle` to
   `write_proxied_response`** — straightforward at H1's call site
   (`crates/envoy-http1/src/hcm.rs:418-424`) but the H2 router-arm does NOT
   call `write_proxied_response` (it builds the downstream `Response` inline
   at `crates/envoy-http2/src/hcm.rs:280-318`, verified). Resolution: H2
   lands inline `upstream_rq_total.inc()` + `upstream_rq_5xx.inc()` at the
   proxy-resp construction site, parallel to the H1 helper's increments.
   PLAN Task 7 names both sites separately.

4. **SPEC §3 D14.3 validator scan reuses the existing
   `for vh in &mut hcm.route_config.virtual_hosts { for r in &mut vh.routes }`
   walk shape at `crates/envoy-config/src/bootstrap.rs:1346-1401`.** The new
   H1×H2 reachability check sits inside the existing `RouteAction::Route(ar)`
   arm at line 1387-1394 alongside the `UnknownCluster` check. No new walk
   structure; the cluster-name HashMap is built once at the start of the
   listener walk per signpost 1's eager single-pass recommendation. PLAN
   Task 2 sets out the exact code.

5. **SPEC §3 D15.3.b cluster-side gauge increment site is at the HCM
   proxy-arm call sites** (`crates/envoy-http1/src/hcm.rs:389-396` +
   `crates/envoy-http2/src/hcm.rs:222-244`), NOT inside `envoy-http1::Client`
   or `envoy-http2::Client`. Per parent-06 SPEC §6 Rule 2 (consumers
   increment), putting the increment inside the codec crates would couple
   them to the cluster-stats namespace. The decrement is RAII-style via
   `ConnGaugeGuard` from envoy-cluster (architecture decision 13). PLAN Task 6
   defines the RAII guard.

### Architecture decisions locked at PLAN-write time (22 decisions)

See PLAN.md "Architecture decisions locked at PLAN-write time (signpost
choices)" section for the full 22-entry table covering all 10 SPEC §7
signposts plus 12 PLAN-write-time decisions on adjacent concerns
(`access_logs_failed` sibling counter ships; TCP-proxy `cx_active` wired;
ConnGaugeGuard RAII; listener cx_active decrement via Arc<Gauge> clone in
spawned task; co-location of 06.2 REVIEW I1 fix with Task 4; etc.).

### Task-ordering rationale

Per PLAN.md "Task summary > Sequencing rationale": Task 2 (D14.3) first
per SPEC §5 close-out posture (mirrors 05.1 Task-1 / 05.3 Task-1 / 06.2
Task-1 preamble cadence); Task 3 (D18.3 harness) before Task 11 (D17.3
fixture) so the fixture references the new BodyRule fields; Tasks 4-8 wire
the comprehensive stats in per-stat-family order; Task 9 (06.1 REVIEW I1)
isolated mid-PLAN; Task 10 (D15.3.e + 06.2 REVIEW I2 diagnosis); Task 11
(D16.3 + D17.3 + 06.2 M3 doc fix) lands LAST among substantive tasks
(extends contract before allow-list per 06.1 REVIEW §7 R-1); Task 12
(D20.3) state-4 verification.

### Carryforwards closed in 06.3 (planned)

- **05.3 REVIEW I1** (closed at Task 2 via `ConfigError::Http2ClusterFromHttp1Listener` parse-time gate). Mirrors phase-05.1 Task-1's posture toward phase-02.1 REVIEW I3.
- **06.1 REVIEW I1** (closed at Task 9 via admin handler idle read timeout). Per user recommendation to fold opportunistically into 06.3 when it touches the admin handler surface.
- **06.2 REVIEW I1** (closed at Task 4 via H1 state-init tightening, mechanically co-located with per-class HCM counter wiring at the same `match outcome { ... }` block).
- **06.2 REVIEW I2** (closed at Task 10 via empirical diagnosis — tighten fixture 0012 expectations.yaml row 12 from `wildcard` to `exact: "-"`, observe outcome, update BEHAVIOR_CONTRACT.md row 12 OR commit the fixture tightening).
- **06.2 REVIEW M3** (closed at Task 11 via fixture 0012 README.md path correction; ~5 LoC).

### Standing carryforwards untouched in 06.3 (per parent-06 SPEC §4 + 06.1/06.2 REVIEW §4 inventories)

- 06.2 REVIEW M1 (`Http1Error::AccessLogOpen` source-chain typing) — indefinite.
- 06.2 REVIEW M2 (`BodyRule::ByteExact` literal-body assertion) — indefinite.
- 06.2 REVIEW M4 (`/tmp/0012-envoy-mount` process-shared path) — activates under nextest sharding.
- 06.2 REVIEW M5 (H2 access-log test 200ms sleep) — 02.2 M1 chain.
- 06.1 REVIEW I2 + M1 + M4 — phase 08.
- 06.1 REVIEW M2 / M3 / M5 / M6 — indefinite / 02.2 chain.
- 05.3 REVIEW I2 (typed-error chain dissolution at H2 dispatch) — defers to phase that next touches H2 router-arm.
- 05.2 REVIEW I1 + I2 + I3 — defers to whichever phase next touches `.github/workflows/ci.yml` or the h2 codec.
- 04.1 REVIEW M5/M9 (Cargo.lock cadence ratification) — couples with conditional ADR-0031.
- 04.1 REVIEW M7 (TLS+H2 ALPN dispatch generalization) — defers to phase that ships H2+TLS.
- 04.1 REVIEW M1/M2/M4 — defers to phase exercising duplicate response headers / stalled body / IPv6 Host.
- 02.2 REVIEW M1 (EchoBackend Drop polling) — standing.

### DECISIONS.md ledger head at state-2

**ADR-0029** (parent-06 split decision; landed at `1f7661a`). No ADRs landed
in 06.1 or 06.2; recommended posture honored. No new ADRs projected for 06.3
per SPEC §8. Conditional ADR-0030 (foundations grant) + ADR-0031 (Cargo.lock
cadence) stay available; recommendation per parent-06 SPEC §7 is no
foundations grants in phase 06.

### State-2 commit composition

This commit lands ONE doc-only commit:
1. `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PLAN.md` (NEW; this PLAN).
2. `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md` (NEW; THIS file's Task 1 preamble).
3. `docs/envoy-rust/ROADMAP.md` (row 06.3 status: planned → in-progress).
4. `docs/envoy-rust/STATE.md` (advance Active phase to state-3-next; next-skill `superpowers:subagent-driven-development`).

NO code changes. NO new ADRs. NO test runs. NO CI push. Tasks 2-12 land at
state-3 each as their own commit.

Per BOOTSTRAP_PROMPT.md §5.1 "one state per session": this session lands the
state-2 commit and exits; the next session enters state 3 and executes Task 2
(D14.3 validator gate) via `superpowers:subagent-driven-development`.

## Task 2 — D14.3 `Http2ClusterFromHttp1Listener` parse-time validator gate (task 2 commit)

### Work summary

Added a new `ConfigError::Http2ClusterFromHttp1Listener { listener: String, cluster: String }`
variant to `crates/envoy-config/src/lib.rs` and wired a H1×H2 reachability gate
inside `validate_hcm` in `crates/envoy-config/src/bootstrap.rs`. The gate fires
when a listener with `codec_type: HTTP1` or `AUTO` has a route pointing to a
cluster whose `typed_extension_protocol_options.HttpProtocolOptions.
explicit_http_config.http2_protocol_options` is set. This closes 05.3 REVIEW I1
substantively: ADR-0028's option-(B) deferral of the H1-listener × H2-arm
dispatch remains correct doctrine, but the deferred path is now visibly rejected
at config-load time so operators don't get a confusing 502 (or silent H1-on-the-
wire to an H2-only backend) at runtime.

### Tests landed (6 unit tests; ~245 LoC total new test code)

All six tests reside in `crates/envoy-config/src/bootstrap.rs::tests`:

1. `validates_h1_listener_with_h1_cluster_passes` — codec_type HTTP1 + cluster
   with no `typed_extension_protocol_options` → validator accepts.
2. `validates_h2_listener_with_h2_cluster_passes` — codec_type HTTP2 + cluster
   with `http2_protocol_options` → validator accepts.
3. `validates_h2_listener_with_h1_cluster_passes` — codec_type HTTP2 + cluster
   with no `typed_extension_protocol_options` → validator accepts (load-bearing
   per 05.3 D4: H2 listener proxying to H1 cluster must keep working).
4. `rejects_h1_listener_with_h2_cluster` — codec_type HTTP1 + H2 cluster →
   `ConfigError::Http2ClusterFromHttp1Listener { listener: "ingress_http",
   cluster: "backend" }`. Both fields asserted.
5. `rejects_auto_listener_with_h2_cluster` — codec_type AUTO + H2 cluster →
   same rejection. AUTO treated as H1-only per parent §4.
6. `tcp_proxy_listener_with_h2_cluster_unaffected` — TCP-proxy listener (no HCM,
   no codec_type) + H2 cluster → validator accepts. Gate is HCM-scoped only.

### Implementation notes

- `validate_hcm`'s signature extended with `listener_name: &str`; call site at
  line 1214 updated to pass `&listener.name`.
- The H1×H2 check uses `if let Some(teo) = ... && teo...is_some()` (collapsed
  per `clippy::collapsible_if` lint) after the existing `UnknownCluster` guard,
  using `iter().find()` consistent with the adjacent code per the PLAN's
  minimal-diff guidance.

### Deviations from PLAN

1. **Two pre-existing tests updated** (not mentioned in PLAN): two tests from
   prior phases used `codec_type: HTTP1` + H2 cluster to test unrelated
   parse-surface behavior, not codec-gate behavior; both now reject under the new
   gate. Updated to `codec_type: HTTP2` with a brief comment:
   - `crates/envoy-config/src/bootstrap.rs::parses_cluster_with_typed_extension_protocol_options_http2`
     (05.3-landed; tests typed_extension parse path).
   - `crates/envoy-cluster/src/cluster.rs::cluster_upstream_protocol_http2_set_from_typed_extension_protocol_options`
     (05.3-landed; tests UpstreamProtocol::Http2 resolution).
   The semantics of both tests are preserved; only the listener codec_type was
   wrong relative to the gate's intent.
2. **Clippy `collapsible_if` lint** triggered on the nested `if let Some(teo)` +
   `if teo...is_some()` pattern. Applied the clippy-suggested `&&`-collapse; the
   resulting form (`if let Some(teo) = ... && teo...is_some()`) is idiomatic
   and passes `cargo clippy --workspace --all-targets --all-features -D warnings`.

### LoC delta

- `crates/envoy-config/src/lib.rs`: +16 LoC (variant + rustdoc).
- `crates/envoy-config/src/bootstrap.rs`: +256 LoC (6 tests + gate logic +
  signature extension); 2 existing tests updated (~8 LoC changed, not net-new).
- `crates/envoy-cluster/src/cluster.rs`: +7 LoC (comment in updated test).
- Total net-new code+tests: ~279 LoC (PLAN estimated ~130; growth is test
  verbosity — each YAML fixture is ~40 LoC; PLAN's ~100-LoC test estimate
  assumed more compact fixtures).

### Carryforward closures

- **05.3 REVIEW I1** — closed substantively at this task.
  The H1-listener × H2-cluster combination is now rejected at parse/config-load
  time with a descriptive error naming both the listener and cluster. The ADR-0028
  option-(B) H1-listener H2-arm dispatch deferral remains correct doctrine; the
  deferred path is now guarded rather than silently mis-wired.

## Task 3 — D18.3 `BodyRule::PrometheusExposition` value-side assertion (task 3 commit)

### Work summary

Extended `BodyRule::PrometheusExposition` in `tests/differential/src/lib.rs` with
three new `#[serde(default)]` fields per 06.3 SPEC §3 D18.3 + signpost 9 option (1):

- `value_exact: Vec<(String, u64)>` — each pair must match exactly on both proxies'
  scrapes.
- `value_must_be_zero: Vec<String>` — each named stat must equal 0 on both proxies.
- `value_present_only: Vec<String>` — each named stat must be present on both proxies;
  value may differ.

Added sibling parser `parse_prometheus_samples(body: &[u8]) -> BTreeMap<String, u64>`
to extract name→value pairs from a Prometheus text-exposition body (labels dropped,
non-parseable values silently skipped; `BTreeMap` for deterministic error message
ordering). Extended `assert_body_rule`'s `PrometheusExposition` arm to dispatch
`value_exact` → `value_must_be_zero` → `value_present_only` in order, using `bail!`
with descriptive error messages including per-proxy observed values.

Three pre-existing tests that constructed `BodyRule::PrometheusExposition` with the
old 2-field form were updated to include the three new fields (all set to `vec![]`);
two match arms destructuring the variant in parse-round-trip tests were updated to
add `..` (pattern completeness). All changes are purely additive; existing
behavior is unchanged.

Backwards-compat verified: fixture 0011 `expectations.yaml` with `kind:
prometheus_exposition` (no new fields) deserializes correctly via the
`#[serde(default)]` fallback.

### Tests landed (2 new unit tests)

1. `assert_body_rule_prometheus_exposition_passes_on_value_exact_match` — body with
   `foo 5` and `bar 0`; rule asserts `value_exact: [("foo", 5)]` and
   `value_must_be_zero: ["bar"]`; both proxies identical → `Ok(())`.

2. `assert_body_rule_prometheus_exposition_fails_on_value_mismatch` — envoy body
   `foo 5`, rust body `foo 6`; rule asserts `value_exact: [("foo", 5)]`; error
   message contains `"value_exact mismatch"`.

### Deviations from PLAN

1. **rustfmt reformatted the `parse_prometheus_samples` body** (the inline
   `name_end` assignment was split to chained method form, matching
   `parse_prometheus_metric_names` above it) and reformatted two test variable
   declarations (`rust_body  =` → `rust_body =`) + one `assert!` call
   (split to multi-line). No semantic diff; all PLAN-verbatim snippets are
   functionally identical after auto-format.

2. **Three existing tests updated** (not mentioned in PLAN): the pre-existing
   `assert_body_rule_prometheus_exposition_*` tests in `mod tests` constructed
   `BodyRule::PrometheusExposition` with the old 2-field form and required the
   three new fields. Added `value_exact: vec![], value_must_be_zero: vec![],
   value_present_only: vec![]` to each. Two parse-round-trip match arms got `..`
   for pattern completeness. All semantics preserved.

### LoC delta

- `tests/differential/src/lib.rs`: +~85 LoC net (3 new fields + rustdoc on variant
  + `parse_prometheus_samples` ~35 LoC + `assert_body_rule` extension ~40 LoC +
  2 new tests ~30 LoC; minus the small field-addition boilerplate already counted
  in the enum). PLAN estimated ~50 LoC code + 2 tests; actual is modestly higher
  due to the three existing-test updates and the value-assertion logic being
  spelled out in full per-field rather than compressed.

### Carryforward closures

None. This task closes no open carryforwards.

## Task 4 — per-response-class HCM counters + 06.2 REVIEW I1 H1 state-init tightening (task 4 commit)

### Work summary

Extended `HCMStats` with 4 new per-class counter fields
(`downstream_rq_2xx`, `downstream_rq_3xx`, `downstream_rq_4xx`,
`downstream_rq_5xx`) registered under the `http.<stat_prefix>.*`
namespace. Wired the increment block at the factored post-`match outcome`
site in `crates/envoy-http1/src/hcm.rs` (after all 5 writer arms have
populated `response_status_for_log`, BEFORE the 06.2 access-log dispatch).
Added the symmetric increment block inside `finalize_h2_stream` in
`crates/envoy-http2/src/hcm.rs` immediately before the `if !config.access_log.is_empty()`
guard. Status codes outside [200, 600) silently no-op per the `_ => {}`
arm (1xx informational and non-standard 6xx not in the per-class family
per Envoy v1.33.0 stats docs).

Co-located fix for 06.2 REVIEW I1: tightened H1 state-init at
`crates/envoy-http1/src/hcm.rs` from `let mut x = 0/default` to
`let x;` / `let mut x;` (no initializer), mirroring H2's stricter posture.
The tightening required restructuring the Proxy arm's no-endpoint path
from a `match pick_endpoint() { None => { ... assign ... None } } / if let
Some(endpoint) = endpoint` pattern into a direct `if let Some(endpoint) =
cluster.pick_endpoint() { ... } else { ... assign ... }` pattern (see
deviation note below).

Also updated the `0011-admin-stats-prometheus` fixture's
`allowlist_envoy_rust_only` to include the 4 new counter names (the
differential test caught these as `envoy-rust-only` entries, expected
per the current StatsRegistry name-embedding convention that defers label
projection to a later phase).

### Tests landed (5 new unit tests in `crates/envoy-http1/src/hcm.rs::tests`)

1. `hcm_increments_downstream_rq_2xx_on_2xx_response` — 200 direct-response
   route; asserts `downstream_rq_2xx = 1`, `3xx/4xx/5xx = 0`, `total = 1`.
2. `hcm_increments_downstream_rq_3xx_on_3xx_response` — 301 direct-response
   route; asserts `downstream_rq_3xx = 1`, others 0, `total = 1`.
3. `hcm_increments_downstream_rq_4xx_on_4xx_response` — 404 direct-response
   route; asserts `downstream_rq_4xx = 1`, others 0, `total = 1`.
4. `hcm_increments_downstream_rq_5xx_on_5xx_response` — 503 direct-response
   route; asserts `downstream_rq_5xx = 1`, others 0, `total = 1`.
5. `hcm_h1_state_init_writes_in_all_5_writer_arms` — drives a 200
   direct-response request with a file access-log sink; asserts the emitted
   line contains "200". Acts as a regression witness for the I1 state-init
   tightening: any future refactor that removes a writer arm's assignment
   fails at compile time (E0381). The existing 06.2 Task-6 access-log tests
   cover the 4 proxy sub-arms indirectly; this test adds an explicit named
   regression target for the I1 fix.

### Deviations from PLAN

1. **H1 proxy arm restructured for state-init tightening.** The PLAN's
   verbatim after-shape shows `let x;` (plain immutable definite-init).
   Achievable for `response_status_for_log` and `response_body_len` only after
   restructuring the no-endpoint path from the original
   `match pick_endpoint() { None => { assign; None } }` + `if let Some`
   pattern to `if let Some(endpoint) = cluster.pick_endpoint() { ... } else
   { assign; }`. The restructuring is semantically identical (same warn log,
   same 503 synthesis, same fall-through to the dispatch site) but required
   moving the no-endpoint warn + response-write into the `else` branch to give
   Rust's flow analysis a clean split. `response_headers_for_log` stays `let
   mut` (no initializer) because the proxy-success arm calls `.push()` after
   initial assignment. The net result is the same no-sentinel guarantee the
   PLAN intended: any arm that fails to assign produces an E0381
   use-of-possibly-uninitialized compile error. Deviation recorded per D-3.5.

2. **Fixture 0011 `allowlist_envoy_rust_only` updated** (not mentioned in
   PLAN). The differential `admin_stats_prometheus` test caught the 4 new
   counter names as `envoy-rust-only` entries. Added with a doc comment
   linking to the Prometheus shape divergence rationale (same as the existing
   `downstream_rq_total` / `downstream_cx_total` entries). This is expected
   behavior: the stat_prefix-embedded naming convention defers label projection
   per the standing StatsTagExtractor carryforward.

### LoC delta

- `crates/envoy-http1/src/hcm.rs`: +~170 LoC (4 new struct fields + register
  calls +3 LoC; per-class increment block ~10 LoC; state-init tightening
  with restructured proxy arm ~10 LoC change; 5 new tests + helper ~170 LoC).
- `crates/envoy-http2/src/hcm.rs`: +~10 LoC (per-class increment block).
- `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`: +~12 LoC
  (4 new allow-list entries with doc comment).
- Total net-new code+tests: ~195 LoC (PLAN estimated ~50 LoC; actual is
  higher due to the test helper + 5 verbose tests and the proxy-arm
  restructuring needed for the state-init tightening).

### Carryforward closures

- **06.2 REVIEW I1** — closed substantively at this task.
  H1 state-vars at `crates/envoy-http1/src/hcm.rs` now use `let x;` /
  `let mut x;` (no default sentinel) mirroring H2's stricter posture at
  `crates/envoy-http2/src/hcm.rs:134`. Rust flow analysis enforces the
  write-before-read invariant at compile time. Regression test
  `hcm_h1_state_init_writes_in_all_5_writer_arms` provides a named test
  as the regression anchor.

## Task 5 — listener `cx_active` gauge (data-path scope; task 5 commit)

### Work summary

Added `cx_active: Arc<envoy_stats::Gauge>` to `Listener` struct and registered
it at bind time as `listener.<name>.downstream_cx_active`. The accept loop
increments the gauge on every accepted TCP connection; the per-connection spawned
task decrements after `h.handle(stream).await` returns (both success and error
paths, unconditionally, via a cloned `Arc<Gauge>`).

Stat is automatically data-path-scoped: the admin listener at
`crates/envoy-bin/src/main.rs:324-345` uses `tokio::net::TcpListener` +
`envoy_admin::serve` directly (not `envoy_listener::Listener::bind`), so
the gauge is never registered for admin traffic. No `ListenerConfig.count_active`
flag needed.

Also updated the `0011-admin-stats-prometheus` fixture's
`allowlist_envoy_rust_only` to include the new
`envoy_listener_ingress_http_downstream_cx_active` entry (same treatment as
`downstream_cx_total` in Task 4).

### Deviation from PLAN

**Deviation: SPEC §3 D15.3.b's signpost 7 posture is implemented by simply
not threading the gauge through any code path the admin listener takes.**
`envoy-bin::main.rs:324-345` constructs the admin listener via
`tokio::net::TcpListener` + `envoy_admin::serve` (not via
`envoy_listener::Listener::bind`), so the `cx_active` gauge is automatically
scoped to data-path listeners. The PLAN-projected `ListenerConfig.count_active:
bool` flag is unnecessary and not added. There is also no existing
`ListenerConfig` type in envoy-listener (confirmed: `Listener::bind` takes
`cfg: &envoy_config::Listener` directly), which further ratifies the
deviation.

### Tests landed (2 unit tests in `crates/envoy-listener/src/lib.rs::tests`)

1. `listener_cx_active_increments_on_accept_decrements_on_close` — binds a
   listener with `HoldHandler` (a new test-only handler that holds each
   connection open until a `broadcast::Sender` fires), connects 1 client,
   asserts `cx_active == 1` while held, releases the handler, asserts
   `cx_active == 0` after settle (~100ms).

2. `listener_cx_active_monotonic_then_decreasing_under_burst` — same pattern
   with N=5 simultaneous connections; asserts peak `cx_active == 5`, releases
   all 5, asserts `cx_active == 0`.

`HoldHandler` uses `tokio::sync::broadcast` to coordinate release, avoiding
a shared `Mutex` or per-connection channels. The broadcast channel is
created at test scope; `HoldHandler` holds a `Sender` clone and subscribes
per connection.

Note: The PLAN-suggested `NullHandler` (drops stream immediately) cannot
hold the connection open long enough to observe the gauge at peak. `HoldHandler`
is necessary for deterministic gauge-peak assertions.

### LoC delta

- `crates/envoy-listener/src/lib.rs`: +~115 LoC (struct field + rustdoc ~15;
  bind registration ~6; serve hoist + inc + Arc clone + task epilogue ~12;
  `HoldHandler` test helper ~20; 2 new tests ~62).
- `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`: +~7 LoC
  (1 new allowlist entry with doc comment).
- Total net-new code+tests: ~122 LoC (PLAN estimated ~80; growth is the
  `HoldHandler` helper + verbose test bodies with broadcast coordination).

### Carryforward closures

None. This task closes no open carryforwards.

---

## Task 6 — Cluster cx_active gauge + ConnGaugeGuard RAII (D15.3.b)

### Structure correction (SPEC pseudocode vs actual code)

The PLAN's pseudocode refers to `ClusterInner` as the struct holding the gauge.
In the actual codebase there is no `ClusterInner`: the struct hierarchy is
`ClusterHandle { inner: Arc<Cluster> }` where `Cluster` is the concrete type.
`ConnGaugeGuard`, `Cluster::cx_active_guard()`, and
`ClusterHandle::cx_active_guard()` are all wired against `Cluster` directly,
not against an intermediate `ClusterInner`. Documented here per D-3.5.

### Implementation summary

- **`ConnGaugeGuard`** (new pub struct in `crates/envoy-cluster/src/cluster.rs`):
  holds `Arc<Gauge>`. `Drop` calls `self.gauge.dec()`. Construction is via
  `Cluster::cx_active_guard()` which calls `self.cx_active.inc()` then wraps
  the `Arc::clone`.

- **`Cluster` struct** extended with `pub(crate) cx_active: Arc<envoy_stats::Gauge>`.

- **`from_bootstrap`**: `register_gauge("cluster.<name>.upstream_cx_active")`
  immediately after the existing `register_counter` call; both are in the
  same per-cluster loop body. One construction site only (confirmed via
  `grep -n "cx_total:" cluster.rs`).

- **`Cluster::cx_active_guard()`** and **`ClusterHandle::cx_active_guard()`**:
  delegates mirror the existing `cx_total()` / `cx_total()` pair shape.

- **Call-site wiring (4 sites)**:
  - `crates/envoy-http1/src/hcm.rs` — `let _cx_guard = cluster.cx_active_guard();`
    before `Client::connect`.
  - `crates/envoy-http2/src/hcm.rs` — single `let _cx_guard = cluster.cx_active_guard();`
    before the `match cluster.upstream_protocol()` block, covering both H1 and
    H2 arms.
  - `crates/envoy-tcp/src/lib.rs` — `let _cx_guard = self.cluster.cx_active_guard();`
    before `tokio::net::TcpStream::connect(addr).await`. Guard fires even on
    connect error because Drop triggers on the `?`-exit path too.

- **Cargo.toml (`crates/envoy-cluster`)**: added `"time"` to dev-dependency
  tokio features (required by `tokio::time::sleep` in the concurrent test).

### Deviations

**Test #2 simplification:** `cluster_cx_active_round_trip_through_h1_call`
uses a guard-held-across-await approach (tokio::time::sleep) rather than an
actual H1 client call. Running an H1 client call inside the `envoy-cluster`
crate tests would require adding `envoy-http1` as a dev-dependency, which is
heavyweight and inverts the crate-dependency direction. The RAII correctness
contract (inc at guard construction + observable gauge peak during async hold
+ dec on drop) is fully verified here; cross-crate wiring is verified by the
HCM integration tests in envoy-http1 / envoy-http2.

**Concurrent test barrier:** `cluster_cx_active_monotonic_then_decreasing_under_concurrent_calls`
uses synchronous guard acquisition in the main task (a tight `(0..N).map(|_|
handle.cx_active_guard()).collect()` loop) to guarantee the peak observation,
rather than `tokio::sync::Barrier` (not available without the `sync` feature)
or an ad-hoc sleep-then-peek. This approach is strictly correct: no async
yield occurs between guard acquisitions, so the gauge reaches N atomically
from the perspective of the subsequent assertion.

**Fixture 0011 holding-pattern entry:** `envoy_cluster_backend_upstream_cx_active`
added to `allowlist_envoy_rust_only` in fixture 0011. This is technically Task
11 territory (BEHAVIOR_CONTRACT + fixture expectations), but adding the entry
here follows the established Task 4 + Task 5 holding-pattern precedent. The
comment on the entry makes the holding-pattern explicit.

### Tests landed (3 unit tests in `crates/envoy-cluster/src/cluster.rs::tests`)

1. `cluster_cx_active_guard_increments_on_construct_and_decrements_on_drop` —
   direct RAII contract test: call `cx_active_guard()`; assert gauge == 1;
   drop; assert gauge == 0.

2. `cluster_cx_active_round_trip_through_h1_call` — guard held across
   `tokio::time::sleep(10ms)`; asserts gauge == 1 before and during sleep,
   == 0 after drop.

3. `cluster_cx_active_monotonic_then_decreasing_under_concurrent_calls` —
   10 guards acquired synchronously; peak assertion at 10; 10 tasks each hold
   a guard for 50ms; gauge == 0 after all join.

### LoC delta

- `crates/envoy-cluster/src/cluster.rs`: +~150 LoC (`ConnGaugeGuard` struct +
  `cx_active` field + `cx_active_guard()` impls + `mk_handle` gauge field + 3
  tests).
- `crates/envoy-cluster/Cargo.toml`: +1 LoC (`"time"` feature in dev-deps).
- `crates/envoy-http1/src/hcm.rs`: +6 LoC (guard + doc comment).
- `crates/envoy-http2/src/hcm.rs`: +7 LoC (guard + doc comment).
- `crates/envoy-tcp/src/lib.rs`: +7 LoC (guard + doc comment).
- `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`: +6 LoC.
- Total net-new code+tests: ~177 LoC (PLAN estimated ~120; growth is the 3
  verbose test bodies + holding-pattern deviation notes).

### Carryforward closures

None. This task closes no open carryforwards.

## Task 7 — D15.3.c Upstream-side router counters + H2 inline increments

### What landed

Two new counters registered per cluster in `from_bootstrap`:
- `cluster.<name>.upstream_rq_total` — incremented once per upstream response received on the success path.
- `cluster.<name>.upstream_rq_5xx` — incremented conditionally when `upstream_resp.status / 100 == 5`.

**`Cluster` struct** (`crates/envoy-cluster/src/cluster.rs`): added `upstream_rq_total: Arc<Counter>` and `upstream_rq_5xx: Arc<Counter>` fields alongside the existing `cx_total`/`cx_active` fields. Accessors `upstream_rq_total()` / `upstream_rq_5xx()` added on both `Cluster` and `ClusterHandle` mirroring the `cx_total()` pattern.

**`write_proxied_response` signature** (`crates/envoy-http1/src/router.rs`): extended with `cluster: &envoy_cluster::ClusterHandle` as second parameter (after `downstream`, before `upstream_response`). Prologue fires `cluster.upstream_rq_total().inc()` unconditionally; `cluster.upstream_rq_5xx().inc()` fires conditionally on `upstream_resp.status / 100 == 5`.

**H1 HCM call site** (`crates/envoy-http1/src/hcm.rs:~447`): updated to pass `&cluster` (already in scope in the proxy arm).

**H2 inline increments** (`crates/envoy-http2/src/hcm.rs`): per PLAN-write SPEC correction 3, the H2 router-arm does NOT call `write_proxied_response`. Inline 2-line increments (`cluster.upstream_rq_total().inc()` + conditional `cluster.upstream_rq_5xx().inc()`) land immediately after `let upstream_resp = match upstream_resp_result { ... }` resolves to the success arm (around line 280, BEFORE the header-copy loop).

**Fixture 0011** (`tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`): added `envoy_cluster_backend_upstream_rq_total` and `envoy_cluster_backend_upstream_rq_5xx` to `allowlist_envoy_rust_only`, following the Tasks 4-6 holding-pattern precedent.

### Deviations

**`write_proxied_response` parameter order:** `cluster` is the second parameter (immediately after `downstream: &mut W`), before `upstream_response`. This keeps the writer (destination) first and the cluster (stats context) second, matching idiomatic "state object before value" order in the existing API surface.

**H2 test scaffolding decision:** The two H2 tests land in `crates/envoy-http2/src/hcm.rs::tests`, exercising the full proxy path through `handle_one_stream`. This matches the existing `h2_proxy_outcome_dispatches_to_upstream` / `h2_proxy_outcome_dispatches_to_h1_upstream_when_cluster_is_http1` precedent in that module. The test for 200 uses an H2 upstream; the test for 503 uses a minimal H1 upstream (raw TCP write returning 503) wired as an H1-protocol cluster, because `spawn_upstream_h2_server` always returns 200.

**router.rs test helper:** `drive_proxy` is now `async fn` because `mk_test_cluster` must `await` `from_bootstrap`. Direct `Cluster` construction from `envoy-http1` tests is not possible (`pub(crate)` fields). Using `from_bootstrap` via YAML + shared registry is the established cross-crate test pattern.

### Tests landed (4 unit tests)

1. `write_proxied_response_increments_upstream_rq_total_on_200` (in `crates/envoy-http1/src/router.rs::tests`) — drives 200 upstream response; asserts `upstream_rq_total == 1`, `upstream_rq_5xx == 0`.

2. `write_proxied_response_increments_upstream_rq_5xx_on_503` (same module) — drives 503 upstream response; asserts both counters == 1.

3. `h2_hcm_increments_upstream_rq_total_on_200` (in `crates/envoy-http2/src/hcm.rs::tests`) — full H2 proxy round-trip via in-process H2 upstream returning 200; asserts counters after 100ms settle.

4. `h2_hcm_increments_upstream_rq_5xx_on_503` (same module) — full H2 proxy round-trip via raw H1 upstream returning 503; asserts both counters == 1.

### LoC delta

- `crates/envoy-cluster/src/cluster.rs`: +~65 LoC (fields, accessors, registration, test updates).
- `crates/envoy-http1/src/router.rs`: +~75 LoC (signature, prologue, helper, 2 tests).
- `crates/envoy-http1/src/hcm.rs`: +1 LoC.
- `crates/envoy-http2/src/hcm.rs`: +~70 LoC (inline increments + 2 integration tests).
- `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`: +8 LoC.
- Total: ~219 LoC (PLAN estimated ~70; growth is test verbosity + from_bootstrap-based test construction).

### Carryforward closures

None. This task closes no open carryforwards.

## Task 8 — D15.3.d Listener accept-failure counter (task 8 commit)

### Work summary

Added `cx_accept_failed: Arc<envoy_stats::Counter>` field to the `Listener`
struct (sibling of `cx_total` and `cx_active`). Registered at bind-time as
`listener.<name>.downstream_cx_accept_failed` following the same idempotent
`register_counter` pattern as `cx_total`. Hoisted in `serve` via `let
cx_accept_failed = self.cx_accept_failed;` alongside the existing hoists for
`cx_total` / `cx_active`. Incremented inside the `Err(err) =>` arm of
`listener.accept()` as the very first statement, BEFORE the `tracing::warn!`
call, per signpost 6 ("ALL accept errors count, no carve-outs").

Also added holding-pattern entry
`envoy_listener_ingress_http_downstream_cx_accept_failed` to
`allowlist_envoy_rust_only` in fixture 0011. Entry mirrors the Tasks 4–7
holding-pattern precedent; BEHAVIOR_CONTRACT update is Task 11's territory.

### Test choice and Err-arm testing limitation

Test `listener_cx_accept_failed_increments_on_accept_error` uses the
"counter-registered + zero-init + no-spurious-increment" pattern:

1. Bind a `Listener`.
2. Re-register `"listener.test_listener.downstream_cx_accept_failed"` on the
   same registry to obtain the same Arc (idempotent contract).
3. Assert `value() == 0` immediately after bind.
4. Drive 3 successful connections; assert counter remains 0 (increment is
   gated to the `Err(err)` arm, not the `Ok` arm).

**Testing limitation:** Inducing a real `tokio::net::TcpListener::accept()`
error deterministically is not straightforward without either platform-specific
`setrlimit`/fd exhaustion tricks or refactoring `Listener::serve` to accept a
trait-abstracted accept call. Neither is in scope for this task. The increment
at the `Err(err)` arm is verified by code-inspection (the `cx_accept_failed.inc()`
call appears as the first statement before `tracing::warn!`). This matches the
06.1 / 06.2 precedent ("happy path + counter-existence" coverage with the
increment site visible-by-inspection).

### LoC delta

- `crates/envoy-listener/src/lib.rs`: +~55 LoC (struct field + rustdoc ~10;
  bind registration ~7; serve hoist + Err-arm increment ~5; test ~33).
- `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`: +~8 LoC
  (1 new allow-list entry with doc comment).
- Total net-new code+tests: ~63 LoC (PLAN estimated ~30 code + 1 test; growth
  is test verbosity and the testing-limitation narrative comment).

### Carryforward closures

None. This task closes no open carryforwards.
