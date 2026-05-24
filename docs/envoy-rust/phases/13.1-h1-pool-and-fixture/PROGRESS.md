# Phase 13.1 (`13.1-h1-pool-and-fixture`) — PROGRESS

> Running log of state-3 execution. Each task closes with a `### Task N — <title>` subsection quoting test names + per-gate clean outputs + commit SHA. State-4 (Task 10) quotes per-gate evidence per the 12.2 / 12.1 / 06.3 precedent.

---

## Task 1 preamble — PLAN-write context for state-3 controller

This section is the controller's read-on-start brief; it lands at the state-2 PLAN-write commit (not Task 1's own commit) per the 12.1 / 12.2 state-2 PLAN-write precedent.

### Locked §2 facts (DO NOT re-run Docker)

The parent-13 state-2 PLAN-write performed the parent SPEC §6.2 HEAVY 9-item empirical verification against `envoyproxy/envoy:v1.33.0`. All 9 items MATCHED the parent SPEC's projections; the findings are LOCKED into the 13.1 SPEC §2 + the STATE.md `### Phase-13 state-2 split decision` subsection. The 13.1 state-3 controller MUST NOT re-run Docker for §6.2 verification; if a 13.1 implementation detail surfaces a new empirical question, verify at task time. The 9 findings (recap; full details in 13.1 SPEC §2):

- **(i)** `circuit_breakers.thresholds[<i>].{priority?, max_connections?}` shape; priority defaults DEFAULT (omitted in /config_dump when DEFAULT); max_connections default 1024.
- **(ii)** Envoy ALWAYS pools regardless of `circuit_breakers` config; default `max_connections: 1024` (never hit under fixture load). → Default-enabled pool with hardcoded defaults when `circuit_breakers` absent (PLAN lock-in #2).
- **(iii)** Idle_timeout knob lives at `typed_extension_protocol_options.envoy.extensions.upstreams.http.v3.HttpProtocolOptions.common_http_protocol_options.idle_timeout` (default 3600s). → Phase-13 hardcodes 60s; config-side knob defers.
- **(iv)** **CRITICAL nuance:** `upstream_cx_total: 1` requires a SINGLE downstream keep-alive conn issuing N sequential requests. With separate downstream conns: `upstream_cx_total: N`. → PLAN lock-in #4: fixture 0020 driver MUST use `Driver::Http1KeepAlive` (the D10 driver extension landing at Task 7 atomically with the fixture).
- **(v)** `upstream_cx_destroy` + 5 sub-siblings (local/remote/with_active_rq variants). All per-cluster. → Phase-13 wires the parent `upstream_cx_destroy` + `upstream_cx_http1_total` only at 13.1; sub-siblings defer.
- **(vi)** H2 stats namespace registration (defers to 13.2).
- **(vii)** Per-class HCM counters bilateral byte-equality (verified). → 13.1 D9.1 fixture 0020 = 06.3 REVIEW I2 (a) full-closure site (Task 7).
- **(viii)** Cluster `upstream_rq_{2,3,4,5}xx` per-class distribution byte-equal.
- **(ix)** **CONFIRMED:** HCM `downstream_rq_5xx` INCLUDES synth-503; cluster `upstream_rq_5xx` does NOT (synth bypasses upstream). Body verified byte-exact = ADR-0037's 19 bytes `no healthy upstream`.

### PLAN-time SPEC corrections (verified at state-2 PLAN-write against HEAD `a88fe26`)

The 13.1 PLAN-writer read every named seam directly and confirms:

- **`Cluster` struct at `crates/envoy-cluster/src/cluster.rs:32-76`** — confirmed `pub(crate)` fields including `name`, `endpoints`, `cursor`, `upstream_protocol`, `cx_total`, `cx_active`, `upstream_rq_total`, `upstream_rq_5xx`, `endpoint_health`, `panic_threshold`. **No `h1_pool` field yet — 13.1 D3 does NOT add one** (lock-in #1 — external `H1PoolManager` instead, per the bin-wired-injection precedent at 12.2's `envoy-health::Scheduler`).
- **`ConnGaugeGuard` at `crates/envoy-cluster/src/cluster.rs:18-26`** — confirmed. Field is private; Task 3 Step 2 adds a `ConnGaugeGuard::from_gauge(Arc<Gauge>) -> Self` pub constructor so the H1 pool can construct guards against the shared cluster gauge handle.
- **`envoy-http1::Client::connect` at `crates/envoy-http1/src/client.rs:33`** — confirmed `pub async fn connect(addr: SocketAddr, host: &str) -> Result<ClientStream, Http1Error>`.
- **`ClientStream` at `crates/envoy-http1/src/client.rs:56`** — confirmed `pub struct` with `pub(crate)` fields (`stream`, `host`, `buf`). Sibling `pool.rs` module accesses directly — **no visibility widening needed** (lock-in #10).
- **H1 `upstream_cx_total` increment site is at `crates/envoy-http1/src/hcm.rs:514`** (within the connect-on-miss `Ok(s) => { cluster.cx_total().inc(); Ok(s) }` arm immediately after `Client::connect(endpoint, &host_header)`). The surrounding `tier-1 cached_upstream` micro-cache at `hcm.rs:502-527` becomes dead code at Task 4 per lock-in #5 and is removed. **The parent-13 SPEC §8's claimed site `router.rs:85-90` was INCORRECT** — verified by grep; this PLAN supersedes that claim per D-3.4.
- **`envoy-tcp/src/lib.rs:108`** carries a `cluster.cx_total().inc()` TCP-proxy site — **UNTOUCHED at 13.1** (TCP pool defers per parent SPEC §4).
- **`Cluster` struct in envoy-config at `crates/envoy-config/src/bootstrap.rs:56`** — confirmed carries `health_checks` (12.1) + `common_lb_config` (12.1) — **no `circuit_breakers` field yet**; 13.1 D1 adds it. The defense-in-depth `Cluster`-by-hand test constructors at `crates/envoy-cluster/src/cluster.rs:803, :825` carry `common_lb_config: None` at their tail; Task 1 Step 4 extends each with `circuit_breakers: None`.
- **12.2 helper at `tests/helpers/health-aware-http1-backend/src/main.rs`** carries `--port` + `--healthz-status` + `--data-status` + `--data-body` flags. Task 6 D8 adds `--per-path PATH=STATUS,...` additively. The helper has NO existing `#[cfg(test)] mod tests` block — Task 6 adds the first.
- **`Driver` enum at `tests/differential/src/lib.rs:39`** — confirmed; variants `Http1`/`Http2`/`Http1ProbeList`/`Http2ProbeList`/`Http1AfterSettle`/`AdminScrape`. Task 7 D10 adds `Http1KeepAlive` as a sibling variant.
- **`envoy-bin/src/main.rs`** — `from_bootstrap` called at `:124`; `envoy-health::Scheduler::spawn` invoked at `:134`. Task 4 Step 4 wires `H1PoolManager::for_bootstrap` between these two calls (cluster_mgr exists; pool needs it).
- **`parse_bootstrap` fuzz corpus on disk has 25 *.yaml files**; the test SUCCESS array has 20 entries; REJECT array has 3 entries; `minimal.yaml` is asserted separately. The "20→21" framing in the SPEC refers to SUCCESS-array count. Task 9 D11 extends SUCCESS to 21 entries + adds the seed file + extends `.gitignore` (3 sibling edits). (The pre-existing `cluster_http2_protocol_options.yaml` seed on disk that is in neither test array is an unrelated pre-existing condition and is not 13.1's concern.)

### Cycle-resolution decision narrative (lock-in #1)

The SPEC §5.1 left the seam open between (a) a new trait declared in `envoy-cluster` (e.g. `pub trait H1ClientPool`) implemented by `envoy-http1::H1Pool` + held as `Cluster.h1_pool: Option<Arc<dyn H1ClientPool>>`, vs (b) bin-wired injection via an external `H1PoolManager` mirroring 12.2's `envoy-health::Scheduler` pattern (sibling-to-`ClusterManager`, NOT a field on Cluster).

**13.1 PLAN-time pick: option (b) — external `H1PoolManager`.** Rationale (per `feedback_pick_recommendation` — pick the obvious recommendation; no fork):

1. Option (a) requires interior mutability on `Cluster` (the `Arc<Cluster>` returned by `from_bootstrap` has no setter), OR widening `from_bootstrap`'s signature to take a `pool_builder` callback, OR pre-building pools before from_bootstrap (chicken-and-egg with `Cluster.cx_total` registration). All three are intrusive.
2. Option (b) parallels the 12.2 `envoy-health::Scheduler` precedent verbatim. The bin constructs `cluster_mgr` first, then `H1PoolManager` second (it walks the bootstrap's clusters, looks up each one in cluster_mgr to grab the shared `Arc<Counter>`/`Arc<Gauge>` stat handles, builds one `H1Pool` per H1 cluster). The HCM proxy arm gets the pool manager plumbed alongside `cluster_mgr`.
3. NO new trait declared in `envoy-cluster`. NO new top-level Cargo dep. NO modification to `envoy-cluster::Cluster`'s struct shape (only `ConnGaugeGuard` gains a public `from_gauge` constructor — a 4-line addition).
4. NO ADR fires (lock-in #16). The cycle resolution is ordinary structure — the same shape 12.2 used.

If state-3 execution surfaces a non-obvious lifecycle complication (e.g. the HCM-config-construction site doesn't have ergonomic reach to `Arc<H1PoolManager>` and the plumbing is uglier than expected), the controller documents the surfacing finding in PROGRESS + lands an inline ADR resolving it. NOT projected.

### Carryforward dispositions entering 13.1

**Closures attributed at 13.1 (lock-in #15):**
- **06.3 REVIEW I2 (a)** (per-class downstream_rq_3xx/4xx/5xx + cluster.upstream_rq_5xx wire-level bilateral coverage) — **FULLY CLOSED at Task 7** (fixture 0020's per-class assertions). PROGRESS at Task 7 attributes the closure honestly.
- **06.3 REVIEW I2 (b)** (`cluster.<name>.upstream_cx_total` value-exact BEHAVIOR_CONTRACT row tightening) — primitive landed at Task 3 (the H1 pool itself), but the **contract row tightening DEFERS to 13.2** (where both H1 + H2 pool uniformly; the row at `BEHAVIOR_CONTRACT.md:89` mentions no protocol carve-out, so tightening at 13.1 would falsify the H2 surface). PROGRESS at Task 5 names the 13.2 D7.1 site.

**Carryforwards entering 13.1 (none gates state-2 or state-3):**
- **12.2 REVIEW Minor carryforwards: 11 active** (A-M2 `Scheduler::task_count` visibility; A-M4 `scheduler.rs:91` `.expect` un-tested negative; B-M1..B-M6 various backend/fixture/backstop nits; C-M1 + C-M2 + C-M4 narrative-finishing-touch). Close opportunistically when 13.1 touches the named seam (Task 3-4 touch envoy-http1 — opportunistic close on A-* not applicable since A-* are envoy-health; B-* on backend touched at Task 6 — opportunistic close on B-M1..M6 case by case).
- **12.1 REVIEW M1 + M3** (no-action style nits — carry forward).
- **Phase-11 REVIEW M1–M8** (next HTTP-filter-family phase — 13.1 does NOT touch HTTP-filter files).
- **10 M2/M3/M4/D1/D2/T1; 09 M1/D1/D2/T1/T2/T3; 08.2 M1-M8 + T1-T3; 08.1 M3; 07.2 M2/M3; 06.2 M1/M2/M4/M5; 06.1 M2/M3/M5/M6; 05.3 I2; 05.2 I1/I2/I3; 04.1 M5/M9/M-claim/M1/M2/M4/M7; 02.2 M1; Phase-00 I3** — all carry forward indefinitely per their existing named-owner dispositions. **Phase 13.1 touches** `envoy-http1` (new `pool.rs` + `hcm.rs:514` modify) + `envoy-config` (Cluster.circuit_breakers schema + validator) + `tests/helpers/health-aware-http1-backend/` (D8 additive extension) + `tests/fixtures/0020-*` (new) + `tests/differential/src/lib.rs` (D10 driver) + `crates/envoy-bin/tests/upstream_connection_pooling.rs` (new) + `crates/envoy-bin/src/main.rs` (H1PoolManager wire) + `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (2 new rows) — close opportunistically at named-seam tasks.

### Task ordering rationale

The 10-task organization (PLAN §Tasks 1-10) reflects a foundation-first cadence:
- **Tasks 1-2** — envoy-config schema + validator (no runtime impact; sets up the corpus seed at Task 9).
- **Task 3** — the architecturally headline H1Pool primitive + manager + idle sweeper, unit-tested in isolation (no HCM coupling — provable correctness before integration).
- **Task 4** — H1 HCM proxy-arm migration + `H1PoolManager` plumbing into HCM config + envoy-bin wire-up + tier-1 micro-cache removal + cx_total increment-site migration (the load-bearing integration task; 19 existing fixtures must regress-equivalence here per gate (b)).
- **Task 5** — D7 stats wiring + BEHAVIOR_CONTRACT rows (largely docs; the registrations themselves land at Task 3).
- **Task 6** — D8 backend extension (test-helper-only; no production touch).
- **Task 7** — fixture 0020 + D10 driver + Docker wrapper (the only fixture-adding task; lands the I2 (a) closure surface).
- **Task 8** — D9.3 in-process H1 backstop (envoy-bin/tests/-resident; subprocess discipline per 09 REVIEW M3).
- **Task 9** — D11 fuzz corpus seed (mechanical 3-sibling-file edit).
- **Task 10** — state-4 verification + STATE advance + push + CI confirm.

This is a PLAN-writer recommendation; the state-3 controller may reorganize within the constraints (Task 3 before Task 4; Tasks 1+2 before Task 9; Task 7 atomic per lock-in #4; Task 10 last).

---

*(State-3 task subsections append below as each task closes — `### Task 1 — ...` through `### Task 10 — state-4 phase-done verification + STATE advance`.)*

---

## Task 1 — envoy-config `Cluster.circuit_breakers` schema (D1)

Extended `envoy-config` with the per-cluster circuit-breaker schema per PLAN
Task 1 + parent-13 SPEC D1 + §6.2 item-(i) (locked findings: shape
`circuit_breakers.thresholds[<i>].{priority?, max_connections?}`; priority
defaults DEFAULT and is omitted in `/config_dump` when DEFAULT; max_connections
default 1024). **Schema only at this commit** — the `validate_circuit_breakers`
sub-validator + the 3 rejection `ConfigError` variants land at Task 2 (D2).

**3 new types** added to `crates/envoy-config/src/bootstrap.rs` alongside
`HealthCheck` / `CommonLbConfig`:

- `CircuitBreakers { thresholds: Vec<Thresholds> }` — top-level block.
  `#[serde(deny_unknown_fields)]`. Derives
  `Debug, Clone, Serialize, Deserialize, PartialEq`.
- `Thresholds { priority: Option<RoutingPriority>, max_connections: Option<u32> }` —
  one entry. Both fields `Option` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  `#[serde(deny_unknown_fields)]` — this is what rejects the parent-13 SPEC §4
  phase-13-deferred fields (`max_pending_requests`, `max_requests`,
  `max_retries`, `max_connection_pools`, `track_remaining`, `retry_budget`) at
  parse time without an explicit validator arm per-field.
- `RoutingPriority { Default, High }` — derives
  `Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq`.
  `#[serde(deny_unknown_fields, rename_all = "SCREAMING_SNAKE_CASE")]` so
  `Default` → `"DEFAULT"` / `High` → `"HIGH"` on the wire (matches the upstream
  Envoy proto-JSON enum projection verified at §6.2 item-(i)). Task 2's
  validator rejects `High` explicitly with a clear error; for now the schema
  accepts both variants symmetrically.

**`Cluster` struct extension** at `crates/envoy-config/src/bootstrap.rs:91+`:
new `pub circuit_breakers: Option<CircuitBreakers>` field added immediately
after `common_lb_config` with
`#[serde(default, skip_serializing_if = "Option::is_none")]` — `None` means
defaults (PLAN lock-in #2 — the §5.4 default-enabled-pool reads
`max_connections: 1024` per upstream Envoy v1.33). The `skip_serializing_if`
preserves the 08.1 `/config_dump` regression-equivalence for the 18 existing
non-circuit-breakers-configured clusters (they continue to round-trip without an
emitted `circuit_breakers: null` field).

**`lib.rs` re-exports** extended at `crates/envoy-config/src/lib.rs:9+`:
appended `CircuitBreakers`, `RoutingPriority`, `Thresholds` alphabetically into
the existing `pub use bootstrap::{...}` block — kept the established
alphabetical-by-segment grouping.

**Defense-in-depth by-hand `envoy_config::Cluster` test constructors** at
`crates/envoy-cluster/src/cluster.rs:806, :852` extended with
`circuit_breakers: None,` after the existing `common_lb_config: None,` line
(PLAN Task 1 Step 4 — verified by `grep -n 'common_lb_config: None,'
crates/envoy-cluster/src/cluster.rs` returning exactly these 2 hits, both inside
`#[test]` constructors building `envoy_config::Cluster` literals; **no
production-code site touched**, no `pub(crate)` visibility widening).

**3 new TDD-first unit tests** in `crates/envoy-config/src/bootstrap.rs::tests`:

- `cluster_circuit_breakers_parses_minimal_shape` — the positive path: YAML
  `circuit_breakers.thresholds[0].{priority: DEFAULT, max_connections: 4}`
  round-trips to `Some(CircuitBreakers { thresholds: [Thresholds { priority:
  Some(RoutingPriority::Default), max_connections: Some(4) }] })`.
- `cluster_circuit_breakers_omitted_yields_none` — schema-is-optional: a YAML
  cluster with no `circuit_breakers` key parses to
  `circuit_breakers: None` (preserves the 18 existing fixtures' parse behavior).
- `cluster_circuit_breakers_rejects_phase13_deferred_threshold_fields` — the
  `deny_unknown_fields` proof: a YAML cluster with
  `thresholds[0].max_pending_requests: 5` fails to parse with an `unknown field`
  error mentioning `max_pending_requests` (asserts the error message contains
  either the field name or the literal `"unknown field"` substring — robust
  against the exact phrasing of `serde_yaml`'s error reporter).

TDD discipline: wrote all 3 tests first; ran `cargo test -p envoy-config --
cluster_circuit_breakers` to verify they failed with the expected compile/parse
errors; then implemented the 3 types + the `Cluster` field + the `lib.rs`
re-exports + the by-hand constructor extensions; re-ran to verify pass.

**No new top-level Cargo dep** — schema uses only existing-pulled `serde` +
`serde_yaml`. **No `unsafe` introduced.** **No new ADR** — PLAN lock-in #16
holds (the schema additions are ordinary structure; the rejection-style choices
defer to Task 2's `ConfigError` variants). DECISIONS.md ledger head stays
**ADR-0038**; next available **ADR-0039**.

`§7.5` gates (a)/(b)/(c)/(d) hold vacuously at this task (no new differential
fixture; pre-existing 19 unaffected — verified at Task 4's full-fixture
regression-equivalence pass; no H2-codec touch; no new fuzz seed — the
`cluster_circuit_breakers.yaml` corpus seed lands at Task 9). (e) the 5
stable-toolchain gates clean locally at this commit:

- `cargo build --workspace --all-targets` → `Finished \`dev\` profile [unoptimized + debuginfo] target(s)` (incremental clean).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `Finished` (zero warnings).
- `cargo fmt --all -- --check` → clean (no diff).
- `cargo test --workspace` → **847 passed / 0 failed / 2 ignored** across 72 result lines (+3 over the 12.2 baseline 844 — exactly the 3 new `cluster_circuit_breakers_*` tests; no existing-test regression).
- `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok` (benign unmatched-license-allowance notices unchanged from prior phases).

Spec ✅ (the implementation matches PLAN Task 1 Steps 1-7 + lock-in #16
verbatim; no scope creep — the validator + the 3 `ConfigError` variants are
held back to Task 2 per PLAN ordering).

---

## Task 2 — `envoy-config` `validate_circuit_breakers` + 3 `ConfigError` variants (D2)

Lands the Phase-13 D2 deliverable per PLAN Task 2: 3 new `ConfigError` variants
and a `validate_circuit_breakers` sub-validator wired at `parse_bootstrap`'s
cluster-validation loop, immediately after the 12.1 `validate_health_checks`
call. The validator enforces the phase-13 scope of `Cluster.circuit_breakers`
(landed at Task 1): at-most-one `thresholds` entry; DEFAULT priority only (or
absent); non-zero `max_connections`. Phase-13-deferred threshold fields
(`max_pending_requests`, `max_requests`, `max_retries`, `max_connection_pools`,
`track_remaining`, `retry_budget` — parent SPEC §4) are rejected at parse time
by Task 1's `deny_unknown_fields`, so the validator only handles the
in-scope-but-invalid cases. Mirrors the 12.1 health-check validator's
"first-error-wins" + per-cluster string-name discipline.

**3 new `ConfigError` variants** in `crates/envoy-config/src/lib.rs:439+` (the
12.1 health-check variant group's immediate neighborhood at file-tail):

- `UnsupportedMultipleCircuitBreakerThresholds { cluster: String }` —
  `circuit_breakers.thresholds.len() > 1`. Multi-priority circuit-breaking
  defers per parent SPEC §4 (phase-13 supports DEFAULT only).
- `UnsupportedCircuitBreakerPriority { cluster: String, priority:
  RoutingPriority }` — `thresholds[0].priority` is non-`Default`. The only
  other variant defined in upstream Envoy v1.33 is `High`, which is rejected
  explicitly (per Task 1 lock-in: `RoutingPriority` is a closed 2-variant enum
  + `deny_unknown_fields`, so any non-DEFAULT-non-HIGH spelling fails at parse
  time before this validator runs).
- `InvalidMaxConnections { cluster: String, value: u32 }` —
  `thresholds[0].max_connections == 0`. Structurally meaningless (would
  prevent any upstream connection); reject explicitly with a clear diagnostic
  rather than letting the pool quietly stall.

**`validate_circuit_breakers` sub-validator** at
`crates/envoy-config/src/bootstrap.rs:2525`: early-returns `Ok(())` when
`cluster.circuit_breakers.is_none()` (preserving the 18 existing
non-circuit-breakers-configured clusters' validator behavior — no false
rejections), then dispatches the 3 rejection arms in order (multi-thresholds →
non-DEFAULT priority → zero `max_connections`), then `Ok(())` at the tail.
Wired at `parse_bootstrap`'s cluster-validation loop at
`crates/envoy-config/src/bootstrap.rs:1727`, the line immediately after
`validate_health_checks(cluster)?;`. The 2-arm `if let Some(...) && cond`
collapsed pattern (clippy `collapsible_if` clean) matches the 12.1 panic-threshold
code style at the same site.

**4 new TDD-first unit tests** in `crates/envoy-config/src/bootstrap.rs::tests`,
grouped under a `// --- 13.1 D2: validate_circuit_breakers ---` section banner
immediately after the Task 1 13.1 D1 section:

- `validate_circuit_breakers_accepts_minimal` — positive: a YAML cluster with
  `thresholds[0].{priority: DEFAULT, max_connections: 4}` parses + validates
  via `crate::parse_bootstrap` (full end-to-end through the
  `parse_bootstrap` → `bootstrap::validate` path; not a unit-level call on
  `validate_circuit_breakers`).
- `validate_circuit_breakers_rejects_multiple_thresholds` — negative: 2
  thresholds entries yield
  `ConfigError::UnsupportedMultipleCircuitBreakerThresholds { cluster: "c" }`.
- `validate_circuit_breakers_rejects_high_priority` — negative:
  `thresholds[0].priority: HIGH` yields
  `ConfigError::UnsupportedCircuitBreakerPriority { cluster: "c", priority:
  RoutingPriority::High }`.
- `validate_circuit_breakers_rejects_zero_max_connections` — negative:
  `thresholds[0].max_connections: 0` yields
  `ConfigError::InvalidMaxConnections { cluster: "c", value: 0 }`.

TDD discipline: wrote all 4 tests first; ran `cargo test -p envoy-config --
validate_circuit_breakers` to verify 3 of them failed with compile errors
(missing `ConfigError` variants) and the positive test also failed for the
same enum-resolution reason; then added the 3 variants + the validator + the
call-site wiring; re-ran to verify all 4 pass + no envoy-config regression.

**No new top-level Cargo dep.** **No `unsafe` introduced.** **No new ADR** —
PLAN lock-in #16 holds (the variants + validator are routine extensions of the
established envoy-config error-discipline; no new architectural decision).
DECISIONS.md ledger head stays **ADR-0038**; next available **ADR-0039**.

Targeted gates clean at this commit:

- `cargo build -p envoy-config --all-targets` → `Finished` (incremental clean).
- `cargo clippy -p envoy-config --all-targets --all-features -- -D warnings` →
  `Finished` (zero warnings; required collapsing two `if let Some(...) { if cond
  { ... } }` blocks into `if let Some(...) && cond { ... }` per clippy
  `collapsible_if`, matching the 12.1 panic-threshold pattern).
- `cargo fmt --all -- --check` → clean (no diff; rustfmt rewrapped one long
  `#[error("...")]` attribute to a multi-line form, applied via `cargo fmt`).
- `cargo test -p envoy-config` → **272 passed / 0 failed / 0 ignored**
  (+4 over Task 1's envoy-config baseline 268 — exactly the 4 new
  `validate_circuit_breakers_*` tests; no existing-test regression). The
  controller verifies the full `cargo test --workspace` + `cargo deny check`
  gates separately (out-of-scope for subagent execution).

Spec ✅ (matches PLAN Task 2 Steps 1-7 verbatim).

---

## Task 3 — H1Pool primitive + PoolGuard RAII + idle sweeper + H1PoolManager (D3)

Lands the architecturally headline 13.1 D3 deliverable per PLAN Task 3: a new
`crates/envoy-http1/src/pool.rs` module carrying `H1Pool` + `PoolGuard` (RAII) +
`H1PoolManager` + `PoolError` + the idle-sweeper task — all unit-tested in
isolation (no HCM coupling; HCM proxy-arm migration defers to Task 4 per the
foundation-first cadence). Plus a 12-line `ConnGaugeGuard::from_gauge` pub
constructor in envoy-cluster (enabling pool callers to construct guards against
the shared cluster gauge handle without holding a `Cluster` reference, per
lock-in #7).

**New module `crates/envoy-http1/src/pool.rs` (~310 SLOC + 5 inline tests)**
declaring 4 public types:

- `H1Pool` — per-cluster pool. Holds `idle: tokio::sync::Mutex<HashMap<SocketAddr,
  Vec<IdleEntry>>>` (`tokio::sync::Mutex` over `std::sync::Mutex` because
  `acquire()` holds the lock across an `.await` in the connect-on-miss branch per
  lock-in #9) + `established: tokio::sync::Mutex<HashMap<SocketAddr, u32>>` (the
  per-endpoint `max_connections` counter) + 4 stat Arc handles
  (`cx_total`/`cx_destroy`/`cx_http1_total` counters + `cx_active` gauge). The
  `cx_total` + `cx_active` Arcs are the SAME handles `Cluster` holds (re-registered
  via the envoy-stats same-kind-idempotency contract verified at
  `crates/envoy-stats/src/registry.rs:141` — `registry_register_counter_idempotent_same_kind`).
  Constructor `H1Pool::new` is plain; `H1Pool::for_bootstrap` is exposed at
  `H1PoolManager::for_bootstrap` (the bin-side construction site, Task 4-wired).
- `PoolGuard` — per-acquire RAII handle. Owns the borrowed `Option<ClientStream>` +
  one `ConnGaugeGuard` (gauge decrements on guard drop per lock-in #7). Provides
  `stream_mut() -> &mut ClientStream` (the HCM proxy-arm reaches the underlying
  TCP stream via this, Task-4) + `invalidate()` (marks the stream un-returnable;
  Drop then destroys it + increments `cx_destroy` instead of returning to the
  idle list). Drop spawns a `tokio::spawn` task to push the stream onto the idle
  list (synchronous Drop cannot `.await`, per lock-in #9). The
  `_cx_active_guard` field's Drop fires last → `upstream_cx_active.dec()`.
  Hand-rolled `Debug` impl (rather than `#[derive]`) because `ConnGaugeGuard`
  doesn't impl `Debug`; surfaces `{cluster, endpoint, has_stream}` for the
  `Result::expect_err` formatting needed by the overflow test.
- `H1PoolManager` — per-bootstrap registry of `Arc<H1Pool>` keyed by cluster
  name. `H1PoolManager::for_bootstrap` iterates `bootstrap.static_resources.
  clusters`, looks up each H1 cluster via `cluster_mgr.get(&cfg.name)`
  (deliberately NOT a `Cluster`-field per lock-in #1 — external registry,
  mirroring the 12.2 `envoy-health::Scheduler` precedent verbatim), skips
  non-H1 clusters (H2 pools defer to 13.2), reads `circuit_breakers.thresholds[0].
  max_connections` (or the hardcoded 1024 default per parent SPEC §6.2 item-i),
  registers `upstream_cx_destroy` + `upstream_cx_http1_total` counters + re-fetches
  the shared `upstream_cx_total` + `upstream_cx_active` Arcs (the registry returns
  the SAME Arc per the same-kind-idempotency contract), builds one `H1Pool` per
  H1 cluster, spawns one idle sweeper per pool, and inserts. `get(cluster_name)
  -> Option<&Arc<H1Pool>>` is the lookup the HCM proxy arm uses at Task 4.
- `PoolError` — `Overflow { cluster, max }` (pool at cap + no idle) or
  `Connect(#[from] Http1Error)` (`Client::connect()` failed on the connect-on-miss
  branch). Derives `Debug, thiserror::Error`.

**Idle sweeper** per lock-in #8: `H1Pool::spawn_idle_sweeper(token:
CancellationToken) -> JoinHandle<()>` spawns a tokio task holding
`tokio::time::interval(idle_timeout / 4)` (15s with the 60s default per parent
SPEC §6.2 item-iii). Each tick walks the idle map, evicts entries past the
deadline, decrements `established` for the evicted count, and fires `cx_destroy`
once per evicted entry. `tokio::select!` against the cancellation token gives
clean shutdown — same lifecycle shape as 12.2's
`envoy-health::Scheduler::shutdown`. Eviction-collection is structured as a
two-phase sequence: collect under `idle` lock first → release → take `est`
lock (avoids any re-entrant-ordering concern with `acquire()`'s
`idle`-then-`est` sequence).

**`ConnGaugeGuard::from_gauge` constructor** added at
`crates/envoy-cluster/src/cluster.rs:22`: a new `impl ConnGaugeGuard { pub fn
from_gauge(gauge: Arc<envoy_stats::Gauge>) -> Self }` block opened before the
existing `impl Drop`. The contract: caller MUST have already called
`gauge.inc()`; Drop calls `gauge.dec()`. Mirrors `Cluster::cx_active_guard`'s
inc+wrap pattern, but exposes the wrap step independently so the pool (which
doesn't hold a `Cluster` reference; it holds the shared `Arc<Gauge>` directly)
can construct guards. The `ConnGaugeGuard` is also added to envoy-cluster's
`lib.rs` re-export block (was previously not re-exported; consumers held it
only transitively via `Cluster::cx_active_guard`'s return type).

**5 new TDD-first unit tests** in `crates/envoy-http1/src/pool.rs::tests`:

- `acquire_from_empty_pool_creates_connection_and_fires_counters` — positive
  fresh-pool path: a single `pool.acquire(addr, host)` against an in-process echo
  backend yields a guard, and asserts `cx_total.value() == 1`,
  `cx_http1_total.value() == 1`, `cx_active.value() == 1`; after `drop(guard)` +
  brief yield, `cx_active.value() == 0` (the spawn-task return-to-pool is
  asynchronous; the test sleeps 50ms after `yield_now()` for the spawned
  return-task to land).
- `acquire_after_return_reuses_idle_stream_without_incrementing_cx_total` — the
  reuse path: acquire → drop → acquire again, assert `cx_total.value() == 1`
  (reuse must NOT re-fire the counter; only the first connect-on-miss did).
- `acquire_returns_overflow_when_at_cap` — the cap-enforcement path: `max_connections:
  1`, first acquire succeeds, second yields `PoolError::Overflow { cluster: "c",
  max: 1 }` via `matches!`.
- `invalidate_destroys_stream_and_increments_cx_destroy` — the invalidation
  path: acquire → `invalidate()` → drop → assert `cx_destroy.value() == 1`
  (the destroy bookkeeping runs in Drop's `None` arm via a tokio::spawn task;
  the test sleeps 50ms for it to land).
- `idle_sweeper_evicts_past_deadline_entries` — the sweeper path: build a pool
  with a 100ms idle_timeout (sweeper tick = 25ms), spawn the sweeper, acquire +
  drop a guard, assert `cx_destroy.value() == 0` after 50ms, then sleep 300ms,
  assert `cx_destroy.value() >= 1` (the entry was past-deadline by then and at
  least one sweep tick fired during the 300ms window). Cancels the token + awaits
  the sweeper handle at tail for clean teardown.

TDD discipline: the 5 tests were authored verbatim from the PLAN Task 3 Step 1
specification (all 5 names + body shapes), then the implementation was filled
in against them. The build initially failed with `PoolGuard doesn't implement
Debug` (the `expect_err` formatting at the overflow test requires it); added a
hand-rolled `Debug` impl (12 SLOC) that surfaces just `{cluster, endpoint,
has_stream}` rather than deriving (which would require `ConnGaugeGuard: Debug`
— a downstream-API-widening change avoided per lock-in #7's "ConnGaugeGuard
REUSED unchanged"). All 5 tests pass on first post-fix run.

**Per-task adaptations from the PLAN**:

- The PLAN's `pool.rs` text uses `if let ... { if let ... { ... } }` nested
  structure on the idle-reuse branch; the workspace's clippy `collapsible_if`
  + `collapsible_match` policy (already in force per 12.1 / 13.1 Task 2
  precedent) requires `if let A && let B` collapsed form. Applied verbatim;
  no behavior change.
- The PLAN's `sweep_once` text takes the `idle` + `est` locks concurrently
  inside `iter_mut()`. The compiler accepts this, but per the PLAN's own
  "Common Adaptation Hints" the safer two-phase pattern is preferred —
  collect evictions under `idle` lock first, release, then take `est` lock.
  Applied; no behavior change (single-pass sweep regardless).
- `tokio-util` was not in `crates/envoy-http1/Cargo.toml` previously; added
  `tokio-util = { version = "0.7", features = ["rt"] }` (same version + same
  feature set as `envoy-health`'s existing declaration at
  `crates/envoy-health/Cargo.toml:19`). This is NOT a new top-level Cargo dep
  per lock-in #9 — `tokio-util` is already pulled by `envoy-bin` + `envoy-health`;
  the addition is sub-crate plumbing identical to the 12.2 precedent. The new
  `envoy_cluster::ConnGaugeGuard` re-export from envoy-cluster's `lib.rs` is
  also additive (no production caller was relying on its absence).
- The PLAN suggested an optional `cargo test -p envoy-cluster -- from_gauge`
  small test; deliberately NOT added — the 5 pool tests fully exercise the
  `from_gauge` contract end-to-end (`cx_active` increments + decrements
  exactly as the tests assert), and adding a sibling unit-level test in
  envoy-cluster would only re-cover the same surface. envoy-cluster's test
  count therefore stays at its prior baseline (36 — verified at the targeted
  test run).

**No new top-level Cargo dep.** **No `unsafe` introduced.** **No new ADR** —
PLAN lock-in #16 holds (the H1Pool is a routine application of the bin-wired
external-registry pattern established at 12.2; the cycle-resolution decision
itself is documented in the PROGRESS Task 1 preamble + the parent-13 SPEC).
DECISIONS.md ledger head stays **ADR-0038**; next available **ADR-0039**.

**§7.5 gates (a)/(b)/(c)/(d) hold vacuously** at this task (no new differential
fixture; pre-existing 19 unaffected — Task 4's HCM-migration commit performs the
full-fixture regression-equivalence pass; no H2-codec touch; no new fuzz seed —
the corpus seed lands at Task 9). (e) the targeted-toolchain gates clean
locally at this commit:

- `cargo build -p envoy-http1 -p envoy-cluster --all-targets` → `Finished`
  (incremental clean).
- `cargo clippy -p envoy-http1 -p envoy-cluster --all-targets --all-features --
  -D warnings` → `Finished` (zero warnings).
- `cargo fmt --all -- --check` → clean (no diff).
- `cargo test -p envoy-http1 -- pool` → **5 passed / 0 failed / 0 ignored /
  71 filtered** (the 5 new `pool::tests::*` tests; `+5` over the prior 71-test
  envoy-http1 baseline; the unfiltered count grows from 71 to 76 — verified
  separately).
- `cargo test -p envoy-cluster -- from_gauge` → **0 passed / 0 failed / 36
  filtered** (no `from_gauge`-named test exists by design — the contract is
  covered indirectly via the pool tests' `cx_active` assertions, and
  envoy-cluster's overall test count stays at 36 baseline). The controller
  verifies the full `cargo test --workspace` + `cargo deny check` gates
  separately (out-of-scope for subagent execution per the 12.2 / 12.1
  precedent).

Spec ✅ (matches PLAN Task 3 Steps 1-6 verbatim; the 3 small adaptations above
are non-substantive per PLAN's own "Common Adaptation Hints" section).

---

## Task 4 — H1 router-arm dispatch through `H1Pool::acquire` (D4)

Lands the load-bearing 13.1 D4 deliverable per PLAN Task 4: migrated the H1 HCM
proxy arm from per-call `Client::connect` (plus the per-downstream-conn tier-1
micro-cache that was the only reuse mechanism) to dispatch through
`H1PoolManager::get(cluster_name)` + `H1Pool::acquire(endpoint, host)`. The
`H1PoolManager` is built once bin-side between `from_bootstrap` (line 123) and
`envoy-health::Scheduler::spawn` (line 134), mirroring the 12.2 external-injection
precedent verbatim (lock-in #1). Threaded into `HCMConfig::from_config` as a
new 4th param `pool_mgr: Option<Arc<H1PoolManager>>`; `Option` so non-bin tests
that construct `HCMConfig` as a struct-literal without a pool manager fall
through to the legacy per-call `Client::connect` path (preserves every
pre-13.1 HCM unit test without a pool dependency).

**`HCMConfig` extension** at `crates/envoy-http1/src/hcm.rs:111-147`: new
`pub pool_mgr: Option<Arc<crate::pool::H1PoolManager>>` field appended after
`filter_pipeline`. The struct retains its `#[derive(Debug)]` — `H1PoolManager`
gained a hand-rolled `Debug` impl (12 SLOC at `pool.rs:302-313`) that surfaces
just the per-cluster pool names; deriving was not viable because `H1Pool`
carries `tokio::sync::Mutex<HashMap>` + per-pool `Counter`/`Gauge` Arcs whose
`Debug` reachability is non-trivial — surface the observable identifiers only.

**`HCMConfig::from_config` signature extension** at `hcm.rs:141-147`:
constructor takes the new `pool_mgr` param as the 4th positional argument;
stored into the struct field at the constructor tail. All 7 in-test call
sites (5 in `envoy-http1` tests + 3 in `envoy-http2` tests via the
`Http1HCMConfig` type-alias) updated to pass `None`. The single envoy-bin
caller at `main.rs:280-287` passes `Some(std::sync::Arc::clone(&pool_mgr))`
(production path).

**`envoy-bin` wire-up** at `crates/envoy-bin/src/main.rs:129-145`: new
`let pool_mgr = envoy_http1::H1PoolManager::for_bootstrap(&bootstrap,
&cluster_mgr, std::sync::Arc::clone(&registry), token.clone())
.context("building H1 pool manager")?;` block inserted between
`cluster_mgr` construction (the 123-127 `from_bootstrap` call) and
`health_scheduler` (the 134-140 `Scheduler::spawn` call). Reuses the existing
`token: CancellationToken` (declared at `main.rs:87`) for idle-sweeper
cancellation. Passes `&cluster_mgr` (auto-deref through Arc) to the
`H1PoolManager::for_bootstrap` signature which takes `&ClusterManager`. No
shutdown plumbing needed: the sweeper JoinHandles are owned inside
`H1PoolManager` and abort cleanly on token cancel (no `.shutdown().await`
needed on the bin's drain path — distinct from `health_scheduler.shutdown()`,
which DOES drain because the active-HC probe tasks block on real network I/O
that cancellation needs to interrupt explicitly).

**Tier-1 `cached_upstream` micro-cache removal** at `hcm.rs:268-274`: the
`let mut cached_upstream: Option<(String, std::net::SocketAddr,
ClientStream)> = None;` declaration removed (lock-in #5 — the pool subsumes
it; pool reuse spans every downstream conn, vastly more reuse than the
per-conn cache observed). Replaced with a 7-line comment block explaining the
removal + the lock-in attribution.

**`cluster.cx_total().inc()` migration off `hcm.rs:514`** (lock-in #6): the
former increment site inside the connect-on-miss `Ok(s) => { cluster.cx_total()
.inc(); Ok(s) }` arm is REMOVED on the pool path — `H1Pool::acquire`'s
connect-on-miss branch at `pool.rs:217` (landed at Task 3) is now the SOLE
incrementer for `upstream_cx_total` when the pool path is taken. Kept ONLY in
the no-pool-manager fallback `else` arm at `hcm.rs:572` (preserves test-path
counter behavior) AND in the H2-cluster (`pool_mgr.get() == None`)
fall-through arm at `hcm.rs:537` (defers H2 pool to 13.2; the per-call
`Client::connect` fallback fires the legacy counter site).

**Proxy-arm block rewrite** at `hcm.rs:496-617` (was `:496-575`): the
former `Result<ClientStream, Response>` dispatch with `Some(...) if cname ==
cluster_name && addr == endpoint => Ok(s), _ => Client::connect(...)` is
replaced with a local `enum StreamHandle { Pooled(crate::pool::PoolGuard),
OneShot(ClientStream) }` shape:

- **Pool path** (`config.pool_mgr.as_ref().is_some()` + `pool_mgr.get(...)
  .is_some()`): `pool.acquire(endpoint, &host_header).await` →
  `Ok(StreamHandle::Pooled(guard))` on success; `Err(PoolError::Connect(_))`
  surfaces as 502 (the connect-failure shape per parent SPEC §4); the new
  `Err(PoolError::Overflow {..})` arm surfaces as 503 (the pool-overflow
  shape — distinct from connect-failure per the §6.2 item-iv semantic).
- **No-pool-entry fall-through** (pool manager present but
  `pool_mgr.get(cluster) == None`, i.e. H2 cluster): per-call
  `Client::connect` + `cluster.cx_total().inc()` (lock-in #6 fallback). H2
  pool defers to 13.2; this path stays per-call until 13.2 lands the H2 pool.
- **No-pool-manager fallback** (`config.pool_mgr.is_none()` — test
  struct-literal path): per-call `Client::connect` + `cluster.cx_total()
  .inc()` (lock-in #6 fallback). Preserves every pre-13.1 HCM unit test
  that builds HCMConfig directly without a pool manager.

On the `Ok(handle)` send-result branch, the success path runs
`construct_proxied_response(&cluster, upstream_response, elapsed_ms, close)`
and falls out of scope; `PoolGuard::drop` returns the stream to the pool's
idle list (lock-in #7) and `OneShot(ClientStream)` closes cleanly on drop
(matches the pre-13.1 semantic on the no-pool fallback). On the `Err(source)`
send-result branch, `if let StreamHandle::Pooled(g) = &mut handle { g
.invalidate(); }` ensures the broken stream is destroyed rather than
returned to the idle list (fires `cx_destroy.inc()` at Drop). The `OneShot`
branch on send-failure has no invalidate analog because it has no idle list
to return to (the stream just drops).

**New TDD-first integration test** at `crates/envoy-http1/src/hcm.rs:3174-3338`:

- `h1_hcm_pool_reuses_upstream_conn_across_sequential_requests` —
  regression-equivalence proof that pool dispatch coalesces upstream
  connections. Spawns an in-process keep-alive echo backend, builds a
  single-cluster bootstrap pointing at it with a SHARED `Arc<StatsRegistry>`
  (load-bearing: the cluster_mgr-side `cx_total` and the pool-side handle
  are the SAME `Arc<Counter>` per the envoy-stats same-kind idempotent
  re-register contract verified at Task 3), wires `H1PoolManager::for_bootstrap`
  + a struct-literal HCMConfig with `pool_mgr: Some(Arc::clone(&pool_mgr))`,
  opens ONE downstream TCP keep-alive conn, drives 5 sequential GET / requests
  through it, asserts `cluster.backend.upstream_cx_total.value() == 1`. At the
  per-call-`Client::connect` regression this counter would be 5 (or 5 on
  separate downstream conns; this test pins the pool reuse at the
  single-downstream-conn boundary the Docker-gated 0020 fixture extends
  cross-downstream-conn at Task 7 per lock-in #4).

TDD discipline: authored the test first (referenced from the PLAN
Task 4 Step 1 + Step 4); ran `cargo test -p envoy-http1 -- pool_reuses` to
verify compile-failure first (the new `pool_mgr` field + the production
dispatch path did not yet exist); landed the HCMConfig extensions + the
proxy-arm rewrite + the bin wire-up; re-ran to verify pass.

**Per-task adaptations from the PLAN**:

- **`H1PoolManager: Debug` hand-roll** — the PLAN does not call out
  `H1PoolManager`'s `Debug` reachability requirement; the `HCMConfig`
  `#[derive(Debug)]` propagates the bound to every field. Added a 12-SLOC
  hand-rolled `impl Debug for H1PoolManager` at `pool.rs:302-313` (surfaces
  the per-cluster pool names only; mirrors the Task-3 `PoolGuard` Debug
  hand-roll's surface-only-the-identifiers shape). No behavior change.
- **Doc-comment rewording on `pool_mgr` field** — the literal text "(test
  sites that build HCMConfig as a struct-literal)" triggered clippy's
  `doc_lazy_continuation` lint because the leading "(" was parsed as a list
  item. Reworded the parenthetical as an em-dash aside ("— test sites that
  build HCMConfig as a struct-literal —"). No semantic change.
- **`access_log_file_sink_in_process` parallel-test flake** — observed once
  during `cargo test -p envoy-bin` under default parallelism (the test
  exceeds its 5s `wait_for_port` deadline when CPU is starved by concurrent
  tests during envoy-bin's startup). Re-ran with `--test-threads=2`: all 15
  envoy-bin tests pass cleanly. Verified at HEAD `368d6ef` (pre-Task-4) the
  same test passes cleanly in 0.73s; with Task 4 it runs in 0.61s alone
  (no per-test slowdown). The flake is pre-existing parallel-load
  scheduling — NOT a Task-4 regression. The controller's workspace gate
  should be aware that this test can flake under heavy parallelism and may
  warrant a re-run.

**Files touched** (4):

- `crates/envoy-http1/src/hcm.rs` — HCMConfig field + from_config param +
  cached_upstream removal + proxy-arm rewrite + new integration test.
- `crates/envoy-http1/src/pool.rs` — added `impl Debug for H1PoolManager`.
- `crates/envoy-http2/src/hcm.rs` — 7 `Http1HCMConfig::from_config` call
  sites updated + 1 struct-literal site updated (the type-alias
  re-exposes `HCMConfig`'s structural change to envoy-http2's tests).
- `crates/envoy-bin/src/main.rs` — H1PoolManager construction block +
  HCMConfig::from_config call-site extension.

**Lock-in #5 attribution**: the pre-13.1 tier-1 `cached_upstream` per-downstream-
conn micro-cache (declared at the former `hcm.rs:274`, populated at the former
`:548-554`, consumed at the former `:502-507`) is SUBSUMED by `H1Pool`'s shared
idle list. The pool's reuse surface is strictly wider (across every downstream
conn observing the same cluster+endpoint, not just within one downstream conn),
and its semantic at the connect-on-miss boundary is identical to the cache's
"reuse iff same cluster+endpoint AND neither side closed" gate (the pool's
default-keep-alive plus the `invalidate()`-on-send-error path together
reconstruct the same hit-vs-miss semantic). Removing the cache is a strict
upgrade.

**Lock-in #6 attribution**: the `cluster.cx_total().inc()` increment site for
`upstream_cx_total` is migrated FROM the former HCM connect-on-miss arm at
`hcm.rs:514` INTO `H1Pool::acquire`'s connect-on-miss arm at `pool.rs:217`.
On the production-bin-wired path this is the SOLE incrementer for the counter
(the pool always fires exactly once per established upstream TCP connection,
matching Envoy v1.33's `upstream_cx_total` per-cluster-per-fresh-connect
semantic verified at parent SPEC §6.2 item-iv). The legacy increment site is
PRESERVED in two fallback arms — the no-pool-manager test path
(`pool_mgr.is_none()`) and the no-pool-entry H2-fall-through path
(`pool_mgr.get(cluster).is_none()`) — to maintain the counter's value-exact
semantic when the pool path is not taken. The fallback sites will retire
entirely at 13.2 (when every cluster has a pool).

**No new top-level Cargo dep.** **No `unsafe` introduced.** **No new ADR** —
PLAN lock-in #16 holds (the migration is routine application of the bin-wired
external-registry pattern; the cycle-resolution decision itself is documented
in the Task 1 preamble + the parent-13 SPEC). DECISIONS.md ledger head stays
**ADR-0038**; next available **ADR-0039**.

**§7.5 gates (a)/(b)/(c)/(d) verification deferred to the controller's full
workspace pass** — Task 4 is the load-bearing regression-equivalence task;
the 19 pre-existing Docker-gated fixtures (0001-0019) must stay green after
this commit per PLAN gate (b). The targeted toolchain gates clean locally
at this commit:

- `cargo build -p envoy-http1 -p envoy-bin --all-targets` → `Finished` (clean).
- `cargo clippy -p envoy-http1 -p envoy-bin --all-targets --all-features --
  -D warnings` → `Finished` (zero warnings; required one doc-comment
  rewording on the `pool_mgr` field per the `doc_lazy_continuation` arm).
- `cargo fmt --all -- --check` → clean (no diff after `cargo fmt --all`
  rewrapped one long `tokio::time::timeout(...)` call in the new test).
- `cargo test -p envoy-http1` → **77 passed / 0 failed / 0 ignored** (+1
  over Task 3's 76-test envoy-http1 baseline — exactly the new
  `h1_hcm_pool_reuses_upstream_conn_across_sequential_requests` test; no
  existing-test regression).
- `cargo test -p envoy-bin -- --test-threads=2` → **15 passed / 0 failed /
  0 ignored** across 14 result lines (the 8 unit tests + 13 single-test
  integration files + 1 two-test integration file = 15 total; matches the
  HEAD-13.1-Task-3 baseline exactly; no per-test count change, no new
  envoy-bin test added at Task 4 per the H1Pool-tested-in-isolation
  posture at Task 3 + the cross-arc D9.x in-process backstop landing at
  Tasks 7-8). See also the parallel-flake note in the "Per-task
  adaptations" subsection above. The controller verifies the full
  `cargo test --workspace` + `cargo deny check` gates independently.

Spec ✅ (matches PLAN Task 4 Steps 1-6 verbatim; the 3 small adaptations
above are non-substantive per PLAN's own "Common Adaptation Hints" section).

### Code-quality review fold-in (post-commit `490bb96`)

Post-landing code-quality review caught a metrics-correctness regression
introduced by Task 4: **`cluster.<name>.upstream_cx_active` is double-counted
on the pool path while a request is in flight.** Fixed in a separate commit.

**Root cause.** Two `cx_active.inc()` sites fire against the SAME
`Arc<Gauge>` for every pool-path request:

1. The HCM proxy arm at the former `hcm.rs:501` ran `let _cx_guard =
   cluster.cx_active_guard();` unconditionally (the 06.3 D15.3.b site),
   firing `cx_active.inc()` BEFORE the pool dispatch.
2. `H1Pool::acquire` ALSO fires `self.cx_active.inc()` against the same
   `Arc<Gauge>` on BOTH the reuse path (`pool.rs:183`,
   `acquire_cx_active_guard`) AND the connect-on-miss path (`pool.rs:219`),
   into the `PoolGuard._cx_active_guard` field. The pool's `cx_active`
   handle is re-registered against the registry's idempotent same-kind
   contract at `pool.rs:350-351` (same registry name
   `cluster.<name>.upstream_cx_active` → same `Arc<Gauge>`).

Net: every in-flight pool-path request reported `cx_active.value() == 2N`
where `N` is the true in-flight count. Steady-state at-rest was correct
(paired inc/dec on both guards), but any `/stats` scrape during traffic
was doubled. No existing test caught this — the Task-4 pool-reuse test
only asserted `cx_total == 1`.

**Fix (reviewer's recommended option (a)).** Relocate the HCM-level
`_cx_guard` so it only fires on the `StreamHandle::OneShot` arms; the
pool path's `PoolGuard` already owns its own `ConnGaugeGuard` field, so
let the pool own the gauge lifecycle on the pool path.

Mechanically at `crates/envoy-http1/src/hcm.rs:~501`: removed the
unconditional outer `let _cx_guard = cluster.cx_active_guard();` (replaced
the line with a comment block explaining the relocation). Added a
conditional declaration AFTER the `stream_or_synth: Result<StreamHandle,
Response>` is built but BEFORE the `match stream_or_synth { Ok(mut handle)
=> ... }` consumes it:

```rust
let _cx_guard: Option<envoy_cluster::ConnGaugeGuard> =
    match &stream_or_synth {
        Ok(StreamHandle::OneShot(_)) => Some(cluster.cx_active_guard()),
        _ => None, // Pooled owns its guard; Err means no connect occurred
    };
```

Drop ordering: `_cx_guard` declared BEFORE the consuming `match` ⇒ drops
LAST (Rust drops in reverse declaration order). So on the OneShot success
path the `ClientStream` (moved into `handle` by the match) closes first,
then `_cx_guard` drops, firing `cx_active.dec()` after the upstream TCP
connection is gone — the correct ordering for an `active` gauge.

**Regression test** at `crates/envoy-http1/src/hcm.rs::tests`:

- `h1_hcm_pool_path_does_not_double_count_cx_active` — drives ONE
  pool-path request through the HCM with a slow backend that holds the
  response open behind a `tokio::sync::oneshot::Sender`. Scrapes
  `cx_active.value()` WHILE the request is in flight and asserts `== 1`
  (NOT 2). Then releases the response and asserts `cx_active == 0` after
  PoolGuard drops the stream back to the idle list. TDD discipline:
  authored the test, temporarily reverted the fix to verify the test
  fails with `cx_active == 2` (output: `assertion left == right failed:
  left: 2, right: 1`), restored the fix, verified the test passes.

**Minor #2 + Minor #4 comment additions (cheap fold-ins):**

- **Minor #2** at `hcm.rs::PoolError::Overflow` arm (~`hcm.rs:545`):
  added a comment explaining why no `cx_total.inc()` fires on this arm
  (overflow means NO connect was attempted; the pool refused the acquire
  before reaching `Client::connect` — symmetric with the `PoolError::Connect`
  arm where the connect failed and `cx_total` also doesn't fire per
  lock-in #6).
- **Minor #4** at `hcm.rs` `send_result::Err(_)` arm (the OneShot side
  of the `if let StreamHandle::Pooled(g) = &mut handle` check): added a
  comment clarifying that the OneShot stream simply drops cleanly on the
  send-error path — no pool to protect, no `invalidate()` analog needed.

Skipped Minor #3 (StreamHandle scoped fine), Minor #5 (cluster_name
clone deferrable to a future micro-optimization pass), and Minor #6
(PROGRESS already clear) per the reviewer's "Optional minor cleanups"
section.

**Targeted gates re-run post-fix** (controller verifies workspace +
deny separately):

- `cargo build -p envoy-http1 --all-targets` → `Finished` (clean).
- `cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings`
  → `Finished` (zero warnings).
- `cargo fmt --all -- --check` → clean (after one `cargo fmt --all`
  rewrap on the new test's `tokio::time::timeout` line).
- `cargo test -p envoy-http1` → **78 passed / 0 failed / 0 ignored**
  (+1 over the Task-4-landed 77-test baseline — exactly the new
  `h1_hcm_pool_path_does_not_double_count_cx_active` test; no
  existing-test regression).

**Files touched** (2):

- `crates/envoy-http1/src/hcm.rs` — relocated `_cx_guard` to OneShot-only
  conditional + Minor #2 + Minor #4 comments + new regression test.
- `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/PROGRESS.md` — this
  fold-in subsection.

**No new top-level Cargo dep.** **No `unsafe` introduced.** **No new ADR**
(metrics-correctness fix is a routine application of the lock-in #6
"pool owns the gauge lifecycle" principle; ADR ledger head stays
**ADR-0038**).

## Task 5 — H1 pool stats wiring + BEHAVIOR_CONTRACT rows (D7-H1)

Task 5 is the documentation + registration-presence-test deliverable
for the 2 H1-pool-introduced counters (the counter `register_counter`
calls themselves already landed at Task 3 inside
`H1PoolManager::for_bootstrap`, verified at commit `368d6ef` —
`crates/envoy-http1/src/pool.rs` lines `register_counter(... cluster.<n>.upstream_cx_destroy)`
+ `register_counter(... cluster.<n>.upstream_cx_http1_total)`).

**BEHAVIOR_CONTRACT rows landed** (2; appended as the new
`**13.1 entries (H1 connection pool):**` block, inserted between the
existing `**12.2 entries (active health checking — counters):**`
block and the `**06.1 Prometheus exposition shape divergence**`
block):

1. `cluster.<name>.upstream_cx_destroy` — `value-exact (0-failures case)`
   (3 eviction paths documented: idle-sweeper, `PoolGuard::invalidate()`,
   connect-failure rollback; fixture-window disposition: 0 for both
   proxies under the no-forced-close 60s-idle-timeout regime).
2. `cluster.<name>.upstream_cx_http1_total` — `value-exact`
   (one increment per H1 connect-on-miss; under fixture-0020's 10
   sequential keep-alive requests, both proxies emit 1 — full pool
   reuse).

**Explicit non-tightening (13.2 D7.1 deferral)**: the existing
`cluster.<name>.upstream_cx_total` row at
`docs/envoy-rust/BEHAVIOR_CONTRACT.md:89` (the 06.1 initial entry)
**STAYS `name-required, value-may-differ` at 13.1** per:

- **PLAN architecture lock-in #3** (the row-tightening defers to
  13.2 to fire only when both H1 + H2 pools tighten uniformly,
  since the existing row mentions no protocol carve-out and tightening
  at 13.1 would falsify the still-per-call-incrementing H2 surface).
- **13.1 SPEC §3 D7** (the BEHAVIOR_CONTRACT D7 obligation at 13.1
  scope is the 2 NEW rows above; the existing-row tightening is the
  **13.2 D7.1 deliverable** — the **06.3 REVIEW I2 (b) full-closure
  site**).

This deferral is named explicitly in the new
`cluster.<name>.upstream_cx_http1_total` row body itself, so the
contract reader at 13.1 sees the rationale inline (rather than only
in the deferred-SPEC).

**Registration-presence test added** (1; new `tokio::test` at the
end of `crates/envoy-http1/src/pool.rs::tests`):

- `h1_pool_manager_registers_cx_destroy_and_cx_http1_total_per_h1_cluster`
  — parses a minimal inline bootstrap YAML with one STATIC H1 cluster
  `c1`, drives `envoy_cluster::from_bootstrap` → `H1PoolManager::for_bootstrap`,
  then asserts that both `cluster.c1.upstream_cx_destroy` AND
  `cluster.c1.upstream_cx_http1_total` are present in
  `registry.snapshot()`. Test result: **`ok` (1 passed)**.

**Targeted gates** (controller verifies workspace + deny separately):

- `cargo build -p envoy-http1 --all-targets` → `Finished` (clean).
- `cargo clippy -p envoy-http1 --all-targets --all-features -- -D warnings`
  → `Finished` (zero warnings).
- `cargo fmt --all -- --check` → clean (after one `cargo fmt --all`
  rewrap on the new test's long `H1PoolManager::for_bootstrap(...)`
  call line + the 2 `snapshot.iter().any(...)` assert args).
- `cargo test -p envoy-http1 -- h1_pool_manager_registers` →
  `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;
  78 filtered out`.
- `cargo test -p envoy-http1` → **79 passed / 0 failed / 0 ignored**
  (+1 over Task 4's 78-test baseline — exactly the new
  `h1_pool_manager_registers_cx_destroy_and_cx_http1_total_per_h1_cluster`
  test; no existing-test regression).

**Files touched** (3):

- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — appended the new
  `**13.1 entries (H1 connection pool):**` block (2 rows) between
  the 12.2 health-check-counters block and the 06.1 Prometheus
  divergence block.
- `crates/envoy-http1/src/pool.rs` — added the
  `h1_pool_manager_registers_cx_destroy_and_cx_http1_total_per_h1_cluster`
  test at the end of `#[cfg(test)] mod tests`.
- `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/PROGRESS.md` —
  this Task 5 section.

**No new top-level Cargo dep.** **No `unsafe` introduced.** **No new
ADR** (BEHAVIOR_CONTRACT row addition is routine 13.1 D7-H1
bookkeeping; ADR ledger head stays **ADR-0038**).

---

## Task 6 — configurable-status backend `--per-path` flag (D8)

Task 6 is a **test-helper-only** extension to the synthetic
`health-aware-http1-backend` (the 12.2 D7.1 primitive at
`tests/helpers/health-aware-http1-backend/src/main.rs`). It adds the
`--per-path PATH=STATUS[,PATH=STATUS,...]` CLI flag + deterministic
per-class body bytes per **PLAN-time lock-in #11**, in support of
fixture 0020 (landing at Task 7) driving the per-class
`downstream_rq_{2,3,4,5}xx` counter coverage that completes the **06.3
REVIEW I2 (a) full-closure surface**. **No production code is
touched** at this task; the helper is consumed by `#[cfg(test)]`-only
sites going forward.

**Helper extension shape:**

- New `parse_per_path(s: &str) -> Result<HashMap<String, u16>>`
  module-scope function — splits on `,`, trims, skips empty
  fragments, splits each entry on `=`, parses the right side as `u16`,
  surfacing both the missing-`=` case and the non-numeric-status case
  through `anyhow::with_context` chains (so `cargo run` failures stay
  human-readable). Module scope (not nested in `parse_args`) so the
  `#[cfg(test)]` block can call it through `use super::*;`.
- New `per_class_body(status: u16) -> &'static [u8]` module-scope
  function — deterministic per-class bytes: `301 → b"moved\n"`,
  `404 → b"not found\n"`, `500 → b"server error\n"`,
  `503 → b"service unavailable\n"`; all other codes fall through to
  the empty byte-slice (defensive default that the unit test pins
  via the `200` case — fixture 0020 depends on this determinism for
  `Content-Length`-equality assertions across both proxy responses).
- `Config` extended with `per_path: HashMap<String, u16>` after
  `data_body`; the existing `#[derive(Debug, Clone)]` covers the new
  field by blanket — no extra trait work.
- `parse_args` extended with the `--per-path` arm (initialises
  `per_path = HashMap::new()` alongside the existing defaults; on the
  arm, calls `parse_per_path(&args[i + 1])?`) — unknown flag handling
  via the existing `bail!("unknown arg: {other}")` arm is preserved.
- `serve` request-dispatch chain rewritten as a 3-arm `if-let / else
  if / else`: **per-path lookup first** (per-path mapping wins —
  matches the PLAN spec verbatim), then the `/healthz` special-case,
  then the default-path arm. Per-path response bodies come from
  `per_class_body(s).to_vec()` — `Vec<u8>` cloning preserved to match
  the existing data-path's `cfg.data_body.clone()` shape (no
  borrow-shape regression in the `serve` writer).
- `status_reason` extended with 3 new arms (`301 → "Moved
  Permanently"`, `404 → "Not Found"`, `500 → "Internal Server
  Error"`) alongside the existing 200/503; the `_ => "OK"` fall-through
  is preserved so unknown codes still produce wire-valid HTTP/1.1
  status lines.
- The header doc-comment's `CLI:` block is extended with the
  `--per-path PATH=STATUS[,PATH=STATUS,...]` line + a sentence noting
  that per-path takes precedence over the `/healthz` special-case and
  bodies are deterministic per-class.

**3 new TDD-first unit tests** in
`tests/helpers/health-aware-http1-backend/src/main.rs::tests` (the
helper's first `#[cfg(test)] mod tests` block):

- `parse_per_path_parses_multiple_entries` — positive path: parses
  `"/301=301,/404=404,/500=500"` into the expected 3-entry
  `HashMap<String, u16>` with the right key/value pairs (and asserts
  `len() == 3`).
- `parse_per_path_rejects_malformed` — error path: asserts
  `parse_per_path("notakvpair").is_err()` (the missing-`=` case) and
  `parse_per_path("/x=notanumber").is_err()` (the non-numeric-status
  case).
- `per_class_body_returns_deterministic_bytes` — pins all 4 mapped
  status codes (301/404/500/503) and the empty-body fall-through
  (`200`) byte-for-byte; this is the **fixture-0020 determinism
  contract**.

TDD discipline: wrote all 3 tests first; ran `cargo test --bin
health-aware-http1-backend` to verify they failed (8 `E0425
cannot find function 'parse_per_path'/'per_class_body'` errors, as
expected); then implemented the 2 helpers + the `Config` field + the
`parse_args` arm + the `serve` per-path-wins dispatch + the
`status_reason` extension + the doc-comment CLI-block extension;
re-ran to verify pass.

**Targeted gates** (controller verifies workspace + deny separately —
**no `cargo test --workspace` run, no `cargo deny check` run, no
push** by this subagent):

- `cargo test --bin health-aware-http1-backend` →
  `test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured;
  0 filtered out` (the 3 new tests; the helper had no prior
  `#[cfg(test)]` block).
- `cargo build --bin health-aware-http1-backend` → `Finished`
  (clean).
- `cargo clippy --bin health-aware-http1-backend --all-features --
  -D warnings` → `Finished` (zero warnings — the new `if let
  Some(&s) = cfg.per_path.get(&path) { ... } else if path ==
  "/healthz"` chain stays under the `collapsible_if` lint because the
  arms are heterogeneous, not nested-`if`s).
- `cargo fmt --all -- --check` → clean (no diff; the `parse_per_path`
  function fits within the 100-column width without rewrap).

**Files touched** (2):

- `tests/helpers/health-aware-http1-backend/src/main.rs` — added
  `HashMap` import, `Config.per_path` field, `--per-path` arg arm,
  `parse_per_path` + `per_class_body` module-scope functions,
  per-path-wins dispatch in `serve`, 3 new `status_reason` arms,
  extended doc-comment, and the `#[cfg(test)] mod tests` block with
  the 3 unit tests.
- `docs/envoy-rust/phases/13.1-h1-pool-and-fixture/PROGRESS.md` —
  this Task 6 section.

**No PLAN deviation.** Implementation follows the PLAN Steps 1-6
verbatim (the test-block code, the `parse_per_path` / `per_class_body`
function bodies, the `Config` extension shape, the `serve` dispatch
chain, the `status_reason` extension — all copied from the PLAN spec
without modification). The only minor adaptation was a one-line
re-wrap of the `per_class_body` doc-comment (3 lines instead of the
PLAN's 2) to keep each line under the workspace rustdoc-style width
budget — no semantic change.

**No new top-level Cargo dep.** **No `unsafe` introduced.** **No new
ADR** (the helper is a `#[cfg(test)]`-domain primitive; the lock-in
that gave rise to the per-class bytes — PLAN lock-in #11 — is
documented in the helper's `per_class_body` doc-comment for future
fixture authors). ADR ledger head stays **ADR-0038**.
