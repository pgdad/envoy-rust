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

---

## Task 11 — BEHAVIOR_CONTRACT extension + fixture 0011 value-exact + README fix (task 11 commit)

### Work summary

**BEHAVIOR_CONTRACT.md `Stat-name mapping` table — 10 new rows (`06.3 entries:`):**

Added a `**06.3 entries:**` subsection immediately before the existing `**06.1 Prometheus
exposition shape divergence**` paragraph, covering the comprehensive stat set landed at
Tasks 4-10:
- `http.<stat_prefix>.downstream_rq_{2xx,3xx,4xx,5xx}` — value-exact; status-class
  bucketing via integer division at the factored access-log dispatch site.
- `http.<stat_prefix>.access_logs_total` — value-exact; `Counter::add(N)` at queue-enter.
- `http.<stat_prefix>.access_logs_failed` — value-exact (0-failures case); per-sink `Err` arm.
- `listener.<name>.downstream_cx_active` — value-exact (deterministic close); RAII decrement
  via Arc<Gauge> clone in spawned task; terminal-zero gauge.
- `listener.<name>.downstream_cx_accept_failed` — value-exact (0-failures case); accept `Err` arm.
- `cluster.<name>.upstream_cx_active` — value-exact (deterministic close); ConnGaugeGuard RAII.
- `cluster.<name>.upstream_rq_total` — value-exact; per upstream response received (not per connect).
- `cluster.<name>.upstream_rq_5xx` — value-exact; conditional sibling.

**Fixture 0012 README path correction (06.2 REVIEW M3):**

`tests/fixtures/0012-access-log-file-sink/README.md` "Per-side divergences" table corrected:
- `envoy` row: `/tmp/0012-envoy-access.log` → `/tmp/0012-envoy-mount/access.log`
- `envoy-rust` row: `/tmp/0012-envoy-rust-access.log` → `/tmp/0012-envoy-rust-mount/access.log`

Added a one-line note explaining that the parent directory is bind-mounted from the host
into the Envoy container (the actual paths match fixture 0012's `expectations.yaml` lines 12-13
and the bind-mount wiring in `tests/differential/src/lib.rs`).

**Fixture 0011 `expectations.yaml` value-side assertion extension:**

Added three new fields (`value_exact`, `value_must_be_zero`, `value_present_only`) to the
`prometheus_exposition` body rule, inserted BEFORE `allowlist_envoy_only` for visual grouping:

```yaml
value_exact:
  - - envoy_http_ingress_http_downstream_rq_total
    - 1
  - - envoy_http_ingress_http_downstream_rq_2xx
    - 1
  - - envoy_listener_ingress_http_downstream_cx_total
    - 1
value_must_be_zero: []
value_present_only: []
```

The set is conservative — only the 3 counters known to be incremented by the current
single-request `GET /` (direct_response 200) scenario. Confirmed via code inspection:
`envoy-stats/src/prometheus.rs` `write_exposition` emits ALL registered metrics regardless
of value (BTreeMap-backed `snapshot()` includes every registration). So `downstream_rq_3xx`,
`downstream_rq_4xx`, `downstream_rq_5xx`, `access_logs_total`, `access_logs_failed`,
`cx_active`, and `cx_accept_failed` ARE present in the exposition at value 0 — asserting
them via `value_must_be_zero` would be technically correct. The conservative choice (empty
`value_must_be_zero`) is intentional: multi-class zero-assertions make most sense when paired
with non-zero sibling assertions from a multi-request `pre_requests` setup, which is the
deferred scope described below.

### Scope deviation — multi-request pre_requests extension DEFERRED

The original PLAN scope included extending fixture 0011's `pre_requests` to drive 4 requests
(one per 2xx/3xx/4xx/5xx status class) and adding a synthetic 5xx backend to produce a
real 5xx response. This scope was narrowed before task execution (per the PLAN.md Task 11
narrowing note). The deferral rationale:

1. A synthetic 5xx backend (e.g., a direct_response 500 route) is straightforward for
   `downstream_rq_5xx` but requires adding new routes and possibly a second listener to
   fixture 0011's `envoy.yaml` + `envoy-rust.yaml` — a meaningful config-surface change that
   goes well beyond "add 3 more pre_requests".
2. The per-class counter wiring is already unit-tested end-to-end (Task 4's
   `hcm_increments_downstream_rq_Nxx_on_Nxx_response` tests cover all 4 classes; Task 7's
   `write_proxied_response_increments_upstream_rq_5xx` covers the cluster-side 5xx path).
3. The CI Docker-gated run (Task 12) will validate the 3 `value_exact` assertions added here.
   Multi-class Docker-gated validation belongs to a future fixture with multi-route setup.

Deferred item recorded per D-3.5. No ADR needed (no contract change; implementation is correct
per unit tests).

### prometheus.rs finding (read at task-execution time)

`crates/envoy-stats/src/prometheus.rs::write_exposition` iterates `registry.snapshot()` which
is a BTreeMap-backed clone of ALL registered entries. Counters appear in the output regardless
of value — a counter registered at startup but never incremented emits `<name> 0`. This means
`value_must_be_zero` assertions on the not-bumped counters would be valid and would not fail
due to "name absent" (the `value_must_be_zero` assertion would find the name with value 0 and
pass). The conservative choice to leave `value_must_be_zero: []` is doctrine-driven (pair
0-assertions with non-zero sibling assertions in a multi-request fixture), not correctness-driven.

### Carryforward closures

- **06.2 REVIEW M3** — closed substantively at this task.
  `tests/fixtures/0012-access-log-file-sink/README.md` paths corrected from the wrong
  flat `/tmp/0012-envoy-access.log` and `/tmp/0012-envoy-rust-access.log` paths to the
  bind-mount-aware `/tmp/0012-envoy-mount/access.log` and `/tmp/0012-envoy-rust-mount/access.log`
  paths. One-line bind-mount note added.

### LoC delta

- `docs/envoy-rust/BEHAVIOR_CONTRACT.md`: +~30 LoC (10 new table rows + section header).
- `tests/fixtures/0012-access-log-file-sink/README.md`: +~7 LoC (table paths corrected +
  bind-mount note; net change ~5 corrected values + 4 new lines).
- `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`: +~30 LoC (3 fields +
  comments explaining the conservative choice).
- `docs/envoy-rust/phases/06.3-stats-wiring-and-close/PROGRESS.md`: this entry (~60 LoC).
- Total: ~127 LoC (PLAN estimated ~120 LoC; within-estimate).

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

## Task 9 — admin handler idle read timeout (closes 06.1 REVIEW I1; task 9 commit)

### What landed

Added `IDLE_READ_TIMEOUT = Duration::from_secs(5)` constant next to the
existing `MAX_REQUEST_HEAD` constant in `crates/envoy-admin/src/handler.rs`
(line 27). Wrapped the `stream.read(&mut scratch[..take]).await?` call inside
`read_request` with `tokio::time::timeout(IDLE_READ_TIMEOUT, ...)`. `Elapsed`
maps to `std::io::Error::new(ErrorKind::TimedOut, "admin idle read timeout: ...")`,
which propagates through `handle_inner`'s `read_request` error arm and causes
a clean connection close (a 400 Bad Request response is attempted, then the
connection shuts down).

This closes 06.1 REVIEW §3 Important I1: prior to this change a connected-but-
silent TCP client held the connection task's `JoinSet` slot indefinitely with
no recourse short of a full shutdown signal. After this change the slot is
released within 5s of the last (or first) read returning no data.

### 5s budget rationale

The 5s budget exactly mirrors `IDLE_READ_TIMEOUT = Duration::from_secs(5)` at
`crates/envoy-http1/src/hcm.rs:24` (introduced in phase 06.1 per PLAN note at
that site). Per the 06.1 REVIEW, the admin handler is a simpler surface (no
keep-alive, no pipelining, no chunked bodies) but the same human-observable
"is the client alive?" window applies. Matching HCM's budget exactly avoids
a two-tier timeout landscape on a small codebase.

### Test design

1 unit test `admin_handler_idle_read_times_out_at_5s` in
`crates/envoy-admin/src/handler.rs::tests`:

- Binds a free port; spawns `serve` with an oneshot shutdown channel.
- Connects a TCP client and sends zero bytes.
- Wraps `client.read(buf)` in `tokio::time::timeout(7s)` as a hard upper bound.
- Before the fix: the test panics at 7s ("IDLE_READ_TIMEOUT not firing").
- After the fix: the read returns (EOF / connection reset / 400 bytes) within
  5s; the test passes in 5.02s.

The test accepts three server-side outcomes (Ok(0), Ok(n), Err) because
`handle_inner` attempts to write a 400 response before shutting down, so the
client may observe either bytes or an abrupt reset depending on TCP buffering.
All three outcomes satisfy the liveness property (connection closed within 7s).

### LoC delta

- `crates/envoy-admin/src/handler.rs`: +~55 LoC (~8 LoC constant + timeout
  match block at line 59; ~47 LoC test including doc comment).
- PLAN estimated ~10 LoC + 1 test; actual test is more verbose due to the
  three-outcome match and the doc comment explaining the test design.

### Carryforward closures

- **06.1 REVIEW I1** — closed substantively at this task.
  `read_request` now fires `IDLE_READ_TIMEOUT = 5s` per-read. A connected-but-
  silent client no longer holds the `JoinSet` slot indefinitely. Mirrors the
  HCM's established 5s idle budget at `crates/envoy-http1/src/hcm.rs:24`.
  Regression test `admin_handler_idle_read_times_out_at_5s` provides the
  named anchor.

## Task 10 — access_logs_total + access_logs_failed counters + 06.2 REVIEW I2 (task 10 commit)

### What landed

**Counter wiring:**

`HCMStats` (in `crates/envoy-http1/src/hcm.rs`) gains two new fields:

- `access_logs_total: Arc<envoy_stats::Counter>` — stat name
  `http.<stat_prefix>.access_logs_total`. Incremented at queue-enter time (BEFORE
  the per-sink await loop) via `Counter::add(config.access_log.len() as u64)`.
  Per parent SPEC §6 Rule 4: fires regardless of emit success; counts intent-to-emit,
  not successful-emit. Uses `Counter::add(N)` (not N individual `.inc()` calls)
  per 06.1 REVIEW §7 R-8.

- `access_logs_failed: Arc<envoy_stats::Counter>` — stat name
  `http.<stat_prefix>.access_logs_failed`. Incremented inside the per-sink `Err(err)`
  arm alongside the existing `tracing::warn!(... "access log emission failed")`.

Both counters are registered in `HCMStats::register` following the existing
per-class counter pattern.

Increment sites:
- H1 path: `crates/envoy-http1/src/hcm.rs` at the factored access-log dispatch
  site (inside the `if !config.access_log.is_empty()` block that precedes the
  per-sink for-loop).
- H2 path: `crates/envoy-http2/src/hcm.rs` — symmetric increment in
  `finalize_h2_stream` at the existing `if !config.access_log.is_empty()` block.

**Fixture 0012 tightening (06.2 REVIEW I2 closure):**

Row 12 of `tests/fixtures/0012-access-log-file-sink/expectations.yaml` (the
`%REQ(USER-AGENT)%` token) was previously `rule: wildcard` with comment
"drive_http1 may inject a default". Tightened to `rule: exact` + `value: "-"`.

Diagnosis is code-inspection-based (Docker-gated test unavailable locally):
`tests/differential/src/lib.rs::drive_http1` (lines 888–908) formats the wire
request as:

```
{method} {path} HTTP/1.1\r\nHost: {host}\r\n{extra_headers}\r\nConnection: close\r\n\r\n
```

No `User-Agent:` header is injected. Both proxies see no User-Agent and both
emit `"-"` in their access logs. The wildcard was unnecessarily loose. The
exact-value rule matches the existing BEHAVIOR_CONTRACT.md row 12 disposition
(`value-exact`; row was already correct — the fixture was behind the contract).
CI run at Task 12 will validate the empirical diagnosis.

**BEHAVIOR_CONTRACT.md row 12 check:**

Read `docs/envoy-rust/BEHAVIOR_CONTRACT.md` line 138. The `%REQ(USER-AGENT)%`
row already carries `value-exact` disposition — no textual change needed. The
fixture 0012 tightening brings the test expectation in line with the contract
that was already in place.

**Fixture 0011 holding entries:**

Added two `allowlist_envoy_rust_only` entries to
`tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`:

```
- envoy_http_ingress_http_access_logs_total
- envoy_http_ingress_http_access_logs_failed
```

Mirrors the Task 4–8 precedent: Envoy does not expose these counters in its
Prometheus stats; envoy-rust embeds the stat_prefix in the name. BEHAVIOR_CONTRACT
update is Task 11's territory.

### Test design

Two new unit tests in `crates/envoy-http1/src/hcm.rs::hcm::tests`:

1. `hcm_increments_access_logs_total_on_emission` — builds `HCMConfig` via
   `hcm_config_with_access_log_and_registry` (a new helper that exposes the
   shared registry), opens a writable FileSink, drives one request via the
   `drive` helper, asserts `access_logs_total == 1` and `access_logs_failed == 0`.
   Counter values read directly from `config.stats.access_logs_total` Arc (no
   re-registration needed).

2. `hcm_increments_access_logs_failed_on_emission_error_but_total_still_increments`
   — uses `FileSink::from_file_for_test` with a read-only file handle (the
   established trick from `hcm_with_file_access_log_emission_failure_does_not_fail_request`).
   Drives one request, asserts `access_logs_total == 1` AND `access_logs_failed == 1`.

**Test isolation note (deviation from task spec's expected approach):**

The failing-sink test installs a `tracing::subscriber::NoSubscriber` via
`set_default` for its duration. This prevents the `tracing::warn!` emitted by
the read-only-sink failure from being captured by the sibling test's
`WarnCapture` subscriber when tests run concurrently. Investigation showed the
interference is a thread-local-subscriber / test-harness-thread-pool interaction:
the Rust test harness can schedule multiple tests concurrently on a small thread
pool, and `set_default` is per-OS-thread. When the test harness reuses a thread
that previously had WarnCapture installed, a warn from another test running on
that thread could appear in the wrong capture buffer. The null-subscriber
installation is the minimal, clean fix — it is test-isolation hygiene, not a
change to the production code path. The existing
`hcm_with_file_access_log_emission_failure_does_not_fail_request` test (which
uses WarnCapture) continues to pass cleanly since it is the ONLY test that
checks warn capture from an emission failure.

### LoC delta

- `crates/envoy-http1/src/hcm.rs`: +~85 LoC (struct fields + rustdoc ~15;
  register calls ~4; H1 dispatch increment site ~8; test helper ~30; 2 tests
  ~38; isolation note comment ~10).
- `crates/envoy-http2/src/hcm.rs`: +~10 LoC (H2 symmetric increment ~8;
  comment ~2).
- `tests/fixtures/0012-access-log-file-sink/expectations.yaml`: +1 LoC net
  (wildcard line split into exact + value; net +1).
- `tests/fixtures/0011-admin-stats-prometheus/expectations.yaml`: +7 LoC
  (2 new entries + doc comment).
- Total net-new code+tests: ~103 LoC (PLAN estimated ~50 LoC; growth is the
  test-isolation narrative and the expanded test helper).

### Carryforward closures

- **06.2 REVIEW I2** — closed by code inspection (see diagnosis above). Fixture
  0012 row 12 tightened from `wildcard` to `exact: "-"`. BEHAVIOR_CONTRACT.md
  row 12 was already correct (`value-exact`); no doc edit needed. CI validation
  deferred to Task 12 Docker-gated run.

### Task 11 fixup — empty value_exact (bilateral-assertion mismatch)

The initial Task 11 commit (`06bdf14`) populated value_exact with 3 entries
referencing envoy-rust-only stat names (already in allowlist_envoy_rust_only
for the Prometheus name-vs-label projection divergence). Bilateral value_exact
assertion requires both proxies to emit the same name, so the assertion fails
at runtime with `envoy=None, envoy-rust=Some(N)`.

Fixup: emptied value_exact. The block carries a comment explaining the
deferral: bilateral value_exact assertions on these counters await a
StatsTagExtractor-equivalent that projects the dynamic segment back into a
Prometheus label at scrape time. Until then, value-side verification stays
at the unit-test level (per-task unit tests in Tasks 4 / 5 / 6 / 7 / 8 / 10).

Fixture 0011 differential test restored to green.

---

## Task 12 — State-4 phase-done gate verification (D20.3)

Per BOOTSTRAP_PROMPT.md §7.5 + 06.3 SPEC §1 acceptance signals (a)-(f).
This task materializes the state-3 → state-4 transition by quoting the
local gate results into PROGRESS. The CI run URL is deferred to a
state-4-followup (push not performed in this executor session per
shared-state-impact discipline).

### Local §7.5 gate results (HEAD b46025ecc3c621815b8c9005dabc9814da2c5e52)

**(e) Stable-toolchain gates:**

- `cargo build --workspace --all-targets`:
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
  ```

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
  ```

- `cargo fmt --all -- --check`:
  ```
  (no output — clean; exit code 0)
  ```

- `cargo test --workspace --lib --bins`:
  ```
  TOTAL: 499 passed; 0 failed; 2 ignored
  (all individual crate "test result:" lines: ok)
  ```

- `cargo deny check`:
  ```
     │      ━━━━ unmatched license allowance
  
  advisories ok, bans ok, licenses ok, sources ok
  ```
  (Two "unmatched license allowance" warnings for Unicode-DFS-2016 and Zlib are
  pre-existing and expected — the allow-list is intentionally conservative.
  `cargo deny` exits 0.)

**(a) + (b) Differential fixture suite (Docker-gated):**

Docker version 28.0.4 available. All 12 fixtures green:

```
admin_stats_prometheus : test result: ok. 1 passed; 0 failed
echo                   : test result: ok. 1 passed; 0 failed
tcp_proxy              : test result: ok. 1 passed; 0 failed
tls_downstream         : test result: ok. 1 passed; 0 failed
tls_upstream           : test result: ok. 1 passed; 0 failed
tls_sni                : test result: ok. 1 passed; 0 failed
http1_direct_response  : test result: ok. 1 passed; 0 failed
http1_router_upstream  : test result: ok. 1 passed; 0 failed
http2_direct_response  : test result: ok. 1 passed; 0 failed
http2_router_upstream  : test result: ok. 1 passed; 0 failed
admin_ready            : test result: ok. 1 passed; 0 failed
access_log_file_sink   : test result: ok. 1 passed; 0 failed

12/12 fixtures GREEN
```

**(c) Conformance:** h2spec ≥95% pass — carries from 05.2 D7 baseline 99.31%. CI run that will validate at push time.

**(d) Fuzz:** No new fuzz target in 06.3 per architecture decision 20; the existing `parse_bootstrap` target from 06.1 covers unchanged at 17 seeds.

**(f) REVIEW.md:** lands at state-5 in a subsequent session per BOOTSTRAP_PROMPT.md §5.1 "one state per session".

### State-4 followup (CI URL capture)

This session does not push to origin/main. The CI URL capture is deferred
to: (i) a state-4-followup task in a session where the user explicitly
authorizes the push, OR (ii) the user pushes manually and the next session
(state-5) captures the CI URL alongside REVIEW.md drafting.

The local-gate results above substantively verify §7.5 (a)-(e); the CI run
will re-verify (c) h2spec + (a)+(b) differential under the project's
standard CI environment.

### State-4 followup — CI evidence captured at state-6 close-out

CI run **<https://github.com/pgdad/envoy-rust/actions/runs/25731958773>**
at HEAD `e9c18282367bdb4d35d4dd9ce847da0c87bd3571` (the state-5 REVIEW.md
commit), conclusion `success`, created `2026-05-12T11:38:16Z`, completed
`2026-05-12T11:40:24Z` (build + test + lint: 2m07s; fuzz: 2m08s). Closes
REVIEW.md §3 Important I3 (state-4 phase-done gate evidence is local-only
— no CI run URL) under the disposition recorded at REVIEW.md §4 row I3
("close before state-6 close-out; acceptable to land alongside state-6 or
as a standalone state-4-followup commit before state-6"). Mirrors the
06.1 (`25625271032` HEAD `36fedd8` completed `2026-05-10T09:33:41Z`) and
06.2 (`25670699370` HEAD `4aba10b` completed `2026-05-11T12:42:59Z`)
state-4-anchor evidence shape — CI URL + HEAD SHA + completion timestamp
+ per-gate quoted evidence. The push to `origin/main` that fired this CI
run was authorized by the user upfront at the state-6 close-out session
entry (the executor session bundled the state-4-followup CI URL capture
into the state-6 close-out commit per next-prompt.txt's recommended
bundled path).

Per parent-06 SPEC §1 + 06.3 SPEC §1 acceptance signal (a)-(f), CI
re-verifies the local evidence captured at Task 12 above (HEAD
`7cdc1a8`) against the project's standard CI environment:

**(a) + (b) Differential fixture suite + cross-fixture coverage.** The
build + test + lint job's `test (includes differential harness → Docker)`
step ran the full workspace test suite against HEAD `e9c1828`. Per-crate
unit-test buckets all reported `test result: ok. N passed; 0 failed`:

```
envoy-accesslog        : 14 passed; 0 failed; 0 ignored
envoy-admin            : 21 passed; 0 failed; 0 ignored
envoy-bin              : 180 passed; 0 failed; 0 ignored
envoy-cluster          :  57 passed; 0 failed; 0 ignored
envoy-config           :  38 passed; 0 failed; 1 ignored
envoy-http1            :  77 passed; 0 failed; 1 ignored
envoy-http2            :  20 passed; 0 failed; 0 ignored
envoy-listener         :  10 passed; 0 failed; 0 ignored
envoy-stats            :  25 passed; 0 failed; 0 ignored
envoy-tcp              :  11 passed; 0 failed; 0 ignored
envoy-tls              :  15 passed; 0 failed; 0 ignored
differential (lib)     :  77 passed; 0 failed; 1 ignored
```

12 Docker-gated fixture single-test runs each report `test result: ok.
1 passed; 0 failed` in the same step (cross-checked against on-disk
fixture set `0001`-`0012`):

```
echo                   : finished in 6.02s
admin_ready            : finished in 0.83s
tcp_proxy              : finished in 0.97s
tls_downstream         : finished in 1.04s
tls_upstream           : finished in 0.83s
tls_sni                : finished in 2.43s
http1_direct_response  : finished in 0.82s
http1_router_upstream  : finished in 2.48s
http2_direct_response  : finished in 2.63s
http2_router_upstream  : finished in 2.78s
admin_stats_prometheus : finished in 3.04s
access_log_file_sink   : finished in 2.66s

12/12 Docker-gated fixtures GREEN simultaneously
```

**(c) Conformance.** `test h2spec_pass_rate_gate ... ok` at
`2026-05-12T11:40:06Z` in the build + test + lint job — h2spec gate
holds at ≥95% pass per the 05.2 D7 baseline 99.31% (144 passed /
1 failed / 1 skipped of 146; no H2-framing surface engaged in 06.3
so the percentage carries through unchanged).

**(d) Fuzz.** `fuzz (parse_bootstrap, 30s)` job ran 238,532 iterations
on the existing 17-seed corpus (16 pre-06.2 + 1 from 06.2 Task 5);
fuzz-engine reported `DONE` at `2026-05-12T11:40:17Z` (`cov: 7791
ft: 16540 corp: 1493/861Kb exec/s: 7694`); zero crashes. No new fuzz
target landed in 06.3 per architecture decision 20 (the validator's
reject-path is structural and serde-walk covers it).

**(e) Stable-toolchain gates.** All 5 (`cargo build --workspace
--all-targets`, `cargo clippy --workspace --all-targets --all-features
-- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`,
`cargo deny check`) reported clean in the build + test + lint job;
exit code 0 across the workflow.

**(f) REVIEW.md verdict.** Landed at state-5 commit `e9c1828` —
**Approved with M-track follow-ups** (0 Critical / 3 Important / 7 Minor);
all 3 Important findings dispositioned for state-6 / state-4-followup /
future-phase carryforward; none triggered §5.2 re-entry at state 3.

State-4 evidence chain is now anchored at a real CI run with SHA +
timestamp, honoring the 06.1 / 06.2 / 05.3 REVIEW-I3 evidence-discipline
precedent. The carryforward chain for `state-4 CI URL evidence completeness`
(05.3 REVIEW I3 → 06.1 closure → 06.2 closure → 06.3 REVIEW I3 → THIS
state-6 close-out) terminates at this commit.
