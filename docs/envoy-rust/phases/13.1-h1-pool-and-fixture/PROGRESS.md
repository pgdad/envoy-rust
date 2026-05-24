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
