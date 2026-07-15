# Phase 70 — Observability family: the access-log **FILTER** subsystem OPENER — `status_code_filter`

> **Status:** `in-progress` (§5 state-1 brainstorm output). This SPEC is the
> brainstorming deliverable for a stranger with zero prior context (D-3.4).
> Every load-bearing wire/behavior claim in §0 was MEASURED against the pinned
> reference `envoyproxy/envoy:v1.33.0` (D-3.3 / D-3.7) during the state-0 recon
> of this session; nothing here is asserted from memory or upstream source.
>
> **Pick + scope recorded in ADR-0140** (reclaimed — the lapsed phase-69
> §6.1-split reservation, per the lapsed-reservation convention; ledger head was
> ADR-0139). The next session is the §5 state-2 PLAN-write
> (`superpowers:writing-plans`) — do NOT implement from this SPEC.

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

This phase OPENS a **new subsystem**: the access-log **FILTER**
(`envoy.config.accesslog.v3.AccessLogFilter`), the per-`AccessLog`-entry
predicate that decides **whether a log record is emitted at all** — distinct
from the access-log **FORMATTER** (the command-operator / `text_format` /
`json_format` engine), which is already mature (phases 06, 32, 38–40). The
opener lands the single canonical filter variant `status_code_filter` (a
`ComparisonFilter` over the final response code), the textbook
cheapest-strong-differential leaf: it reuses the ENTIRE `envoy-accesslog`
subsystem + the byte-exact `Http1AccessLogByteExact` differential driver + the
27 existing access-log fixtures' discipline + `direct_response`, adds **no new
subsystem** (a `filter` field + one predicate gate), and yields a fully
deterministic **byte-exact single-line file** observable.

### R-0.1 — envoy-rust has NO access-log filter at any layer today (measured in-tree)

- **Config (`crates/envoy-config`).** The `AccessLog` struct
  (`bootstrap.rs:701`-`706`) carries ONLY `name` + `typed_config` under
  `#[serde(deny_unknown_fields)]`:
  ```rust
  #[serde(deny_unknown_fields)]
  pub struct AccessLog {
      pub name: String,
      pub typed_config: AccessLogTypedConfig,
  }
  ```
  There is **no `filter` field** — a real Envoy `filter:` block is currently
  REJECTED at parse time (`ConfigError::Yaml`), NOT silently ignored. None of the
  twelve `AccessLogFilter` variants (`status_code_filter`, `duration_filter`,
  `response_flag_filter`, `header_filter`, `and_filter`, `or_filter`, …) appears
  anywhere in the crate. `AccessLogTypedConfig` (`bootstrap.rs:714`-`719`) and
  `FileAccessLog` (`bootstrap.rs:733`-`739`, `path` + `log_format`) are unchanged.
- **Emitter (`crates/envoy-accesslog`).** No predicate module exists (crate
  modules: `command_operator`, `default_format`, `error`, `file_sink`,
  `json_format`, `log_format`, `record`, `sink`). `FileSink::emit`
  (`file_sink.rs:97`-`121`) renders and writes **one line unconditionally** —
  there is no `should_log`/gate.
- **HCM wiring.** The only emission gate is
  `log_enabled = !config.access_log.is_empty()` (`crates/envoy-http1/src/hcm.rs:820`)
  — "is any sink configured at all", NOT a per-record filter. When ≥1 sink
  exists the HCM builds one record and emits to EVERY sink unconditionally
  (`hcm.rs:1479`-`1518`). The H2 path has the sibling emit loop in
  `crates/envoy-http2/src/hcm.rs` (drives the `0064`-`0070` H2 access-log
  fixtures).
- **Differential.** The 27 access-log fixtures (`0012`, `0040`, `0046`-`0070`)
  all assert FORMATTER/field content; **none exercises log-line suppression**.

**Consequence:** the access-log filter is a genuine greenfield leaf, but its
observable and harness already exist (R-0.5).

### R-0.2 — LIVE-ENVOY (`--mode validate`, networking-free): the `status_code_filter` wire shape

Measured with `docker run … --mode validate -c cfg.yaml` (memory
`mode-validate-probes-wire-shape-networking-free`) — an HCM file access-log
carrying a `filter:` block:

| `filter:` value | Result |
|---|---|
| `status_code_filter: { comparison: { op: GE, value: { default_value: 500, runtime_key: "unused" } } }` | **OK** |
| `status_code_filter: { comparison: { op: GE, value: { default_value: 500 } } }` (no `runtime_key`) | **REJECTED** — `RuntimeUInt32ValidationError.RuntimeKey: value length must be at least 1 characters` |
| `response_flag_filter: { flags: ["UH"] }` | OK (deferred, §2.2) |
| `header_filter: { header: { name: ":path", string_match: { prefix: "/log" } } }` | OK (deferred, §2.2) |
| `and_filter: { filters: [ status_code_filter…, header_filter… ] }` | OK-shaped (its only rejection was the same nested `runtime_key` rule) — deferred, §2.2 |
| (no `filter:` at all) | OK (baseline — today's behavior) |

**MEASURED schema of `status_code_filter`** (upstream
`envoy.config.accesslog.v3.StatusCodeFilter` → `ComparisonFilter`):
`comparison: { op: <ComparisonFilter.Op>, value: <RuntimeUInt32> }`, where
`RuntimeUInt32 = { default_value: uint32, runtime_key: string }`.

### R-0.3 — LIVE-ENVOY: the `ComparisonFilter.Op` enum tokens

Validated each op token (same networking-free `--mode validate`):

| `op` | Result |
|---|---|
| `EQ` | **OK** |
| `GE` | **OK** |
| `LE` | **OK** |
| `NE` | **REJECTED** (`error initializing` — unknown enum value) |
| `BOGUS` | **REJECTED** |

So `ComparisonFilter.Op` is exactly `{ EQ, GE, LE }` — a three-value enum
comparing the record's status against `value.default_value`.

### R-0.4 — LIVE-ENVOY: `runtime_key` is PGV-mandatory (min_len 1) but RTDS-inert here

The PGV constraint `RuntimeUInt32.runtime_key` **min_len = 1** makes `runtime_key`
a REQUIRED field even when the value is never overridden. envoy-rust has **no
runtime (RTDS) subsystem** — `runtime_key` is therefore **parsed-but-inert**:
the comparison always uses `default_value`. This is a clean documented boundary
(the same posture as every other "config-accepted, runtime-override-deferred"
knob in the tree). The validator must still REQUIRE a non-empty `runtime_key`
to preserve load-time parity (PV-4).

### R-0.5 — the differential observable + harness ALREADY exist (measured in-tree)

`tests/differential/src/lib.rs` defines `Driver::Http1AccessLogByteExact
{ probes: Vec<AccessLogByteExactProbe>, expected_access_log_paths: AccessLogPaths }`
(`lib.rs:159`-`164`; the H2 sibling `Http2AccessLogByteExact` at `lib.rs:177`).
The arm driver `run_http1_access_log_byte_exact_arm` (`lib.rs:6223`+) drives each
probe against BOTH proxies, waits for the flush, `read_to_string`s both files,
and calls `assert_access_log_lines_byte_identical(&envoy_lines,
&envoy_rust_lines)` (`tests/differential/src/access_log.rs:305` — whole-line
`==`, the strongest assertion). **Measured caveat:** the arm currently asserts
`file line-count == probe count` (`lib.rs≈6357`). A FILTER SUPPRESSES some
probes, so the file has FEWER lines than probes — the driver needs a small,
bounded extension so an expected-suppressed probe contributes no line (PV-2).
This is the ONLY harness change.

### R-0.6 — LIVE-ENVOY (runtime, port-mapped, no backend): `status_code_filter GE 500` suppression is deterministic + byte-exact

Booted live `envoyproxy/envoy:v1.33.0` (`docker -p`, memory
`state0-recon-docker-needs-port-mapping`) with ONE HCM listener: a file access
log (`text_format_source` = `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%
FLAGS=%RESPONSE_FLAGS%\n`) + `status_code_filter: { comparison: { op: GE, value:
{ default_value: 500, runtime_key: "unused" } } }`, and TWO `direct_response`
routes (`/log` → 503, everything else → 200). No cluster / no backend
(`direct_response` short-circuits). Drove three requests:

| Request | HTTP status | logged? |
|---|---|---|
| `GET /nolog` | 200 | **NO** (200 < 500) |
| `GET /log` | 503 | **YES** |
| `GET /other` | 200 | **NO** |

The access-log FILE contained EXACTLY ONE line:
`STATUS=503 PATH=/log FLAGS=-` (line count 1).

**Load-bearing facts:** (1) `status_code_filter` gates on the **final response
code** — 200s below the `GE 500` threshold are dropped, the 503 is kept.
(2) `direct_response` responses ARE access-logged (so the differential needs NO
backend — the same discipline as the `direct_response`-based access-log
fixtures). (3) A `direct_response` 503 carries `%RESPONSE_FLAGS% = -` (a clean
local reply, no upstream-failure flag). The observable is a fully deterministic
byte-exact single line.

### R-0.7 — numbering

Next ROADMAP id is **70** (highest defined is `69`; `59`/`60`/`62` are
intentional gaps). Next fixture id is **0076** (`0075` is the last). Next ADR is
**ADR-0140** (reclaimed — ledger head `ADR-0139`; ADR-0140 was reserved-unfired
for the phase-69 split, which did not fire).

---

## §1. Goal

Open the access-log **FILTER** subsystem by landing `status_code_filter`
(`envoy.config.accesslog.v3.AccessLogFilter.status_code_filter`), behaviorally
equivalent to `envoyproxy/envoy:v1.33.0` under the differential contract (§7):

- An `AccessLog` entry MAY carry a `filter`; when present, a log record is
  emitted to that sink ONLY IF the filter matches. `status_code_filter` matches
  when the final response code satisfies `op(status, value.default_value)` for
  `op ∈ {EQ, GE, LE}` (R-0.2/R-0.3). A record failing the filter produces NO
  line for that sink (R-0.6).
- `runtime_key` is REQUIRED non-empty for load-parity (R-0.4) but RTDS-inert —
  the comparison always uses `default_value`.
- A `filter` block with no recognized variant (or > 1 variant) is a fail-loud
  `ConfigError` (the `AccessLogFilter` oneof cardinality, PV-1).
- Reuse the entire `envoy-accesslog` emit machinery + the `Http1AccessLogByteExact`
  differential driver unchanged except the one bounded suppression extension
  (R-0.5 / PV-2).

**Differential surface at phase end:** a new fixture `0076` witnessing
`status_code_filter GE 500` byte-exact — an H1 HCM with a filtered file access
log + two `direct_response` routes (503 kept, 200 suppressed), asserting the log
file across both proxies is the SAME single byte-identical 503 line — plus
in-process coverage of the EQ/GE/LE boundary semantics, the oneof-cardinality +
empty-`runtime_key` rejections, and the RTDS-inert `runtime_key`.

---

## §2. Scope

### 2.1 In scope

1. **Config schema (`crates/envoy-config`).** Add
   `AccessLog.filter: Option<AccessLogFilter>` (serde `default`). New types:
   - `AccessLogFilter` — models the `AccessLogFilter` proto **oneof**. Following
     the in-tree `SubstitutionFormatString` precedent (`bootstrap.rs:751`-`766`,
     the `{text_format_source | json_format}` oneof modeled as `Option` arms + a
     cardinality validator, `ConfigError::AmbiguousLogFormat`), model
     `AccessLogFilter` as a struct with `status_code_filter: Option<StatusCodeFilter>`
     (the ONLY variant this phase) under `#[serde(default, deny_unknown_fields)]`,
     with a validator enforcing **exactly one** variant present. (PV-1 confirms
     Option-struct vs. an internally-tagged enum against the precedent.)
   - `StatusCodeFilter { comparison: ComparisonFilter }`.
   - `ComparisonFilter { op: ComparisonOp, value: RuntimeUInt32 }`.
   - `ComparisonOp` — a 3-value enum `{ Eq, Ge, Le }` (serde-renamed to the
     upstream `EQ`/`GE`/`LE` tokens, R-0.3), `deny_unknown_fields`-equivalent
     rejection of any other token.
   - `RuntimeUInt32 { default_value: u32, runtime_key: String }` (both required;
     `runtime_key` non-empty, R-0.4).
   All new structs `#[serde(deny_unknown_fields)]`.
2. **Validation (`crates/envoy-config`).** (a) The `AccessLogFilter` oneof
   cardinality — zero variants OR (future) multiple → a fail-loud `ConfigError`
   (PV-1 decides reuse/rename vs a new variant). (b) `runtime_key` empty → a
   fail-loud `ConfigError` mirroring the measured PGV `min_len 1` (R-0.4; PV-4 —
   native message permitted per ADR-0049). (c) The op enum + `default_value`
   parse unchanged by the generic serde path.
3. **Emission gate (`crates/envoy-accesslog` + HCM).** A predicate — e.g.
   `AccessLogFilter::should_log(status: u16) -> bool` (or a record-scoped
   evaluator) implementing `op(status, default_value)`. Wire it into the per-sink
   emit loop so the record is emitted to a sink ONLY when that sink's `filter`
   (if any) matches; a sink with no filter behaves exactly as today (regression
   parity). H1: `crates/envoy-http1/src/hcm.rs:1479`-`1518`. H2: the sibling loop
   in `crates/envoy-http2/src/hcm.rs` (PV-3 locates it). The gate reads the final
   response status the record already carries — no new plumbing.
4. **Differential fixture `0076-accesslog-status-code-filter`.** An H1 HCM
   listener with a file access log (a deterministic `text_format_source`, the
   `0040`+ discipline) + `status_code_filter: { comparison: { op: GE, value: {
   default_value: 500, runtime_key: "unused" } } }` + two `direct_response`
   routes (`/log` → 503, `/nolog` → 200). Drive both probes; assert the log file
   across both proxies is the SAME single byte-identical 503 line (the 200
   suppressed), via `Http1AccessLogByteExact` + the PV-2 suppression extension.
5. **In-process coverage.** The `should_log` predicate for EQ/GE/LE across
   boundary statuses (e.g. GE 500 at 499/500/503; EQ 404; LE 200 at 200/201);
   the oneof-cardinality (zero-variant) + empty-`runtime_key` rejections; the
   RTDS-inert `runtime_key` (a non-`"unused"` key still uses `default_value`);
   a no-`filter` sink still logging every record (regression).
6. **`BEHAVIOR_CONTRACT.md`** — a `status_code_filter` subsection under the
   access-log section (§6).
7. **`known-failures.txt` / conformance** — unchanged (no protocol-conformance
   surface; never trimmed, memory `h2spec-3-5-2-preface-host-sensitive`).

### 2.2 Out of scope (deliberate, with rationale)

- **Every other `AccessLogFilter` variant** — `response_flag_filter`,
  `header_filter`, `duration_filter`, `not_health_check_filter`, `and_filter`,
  `or_filter`, `grpc_status_filter`, `runtime_filter`, `metadata_filter`,
  `traceable_filter`, `log_type_filter`. This is the subsystem OPENER (one
  variant), mirroring the phase-66 network-filters family opener. Each remaining
  variant is a future cheapest-strong leaf that reuses THIS phase's `filter`
  field + `should_log` seam (§10 notes the natural next picks).
- **The RTDS `runtime_key` override** — parsed-but-inert (R-0.4); envoy-rust has
  no runtime subsystem, so the comparison always uses `default_value`. A future
  runtime/RTDS phase owns the override.
- **H2 access-log-filter differential** — the `should_log` gate is codec-agnostic
  (it reads the response status), so it is inert-correct on H2; a dedicated H2
  filtered fixture is deferred (the opener is H1, the simplest driver). The H2
  emit-loop wiring IS done (PV-3) so H2 does not regress; the H2 *differential*
  fixture is the deferred slice.
- **`and_filter`/`or_filter` composition** — deferred; requires the recursive
  `AccessLogFilter` shape, a natural second phase.

### 2.3 §7.4 fuzz disposition

The `filter` surface reuses the `parse_bootstrap` parser (a new sub-message).
**Default projection:** a new `parse_bootstrap` corpus seed carrying an
`access_log[].filter.status_code_filter` — **no new fuzz target** (the
phase-68/69 precedent, ADR-0137: a config-only sub-message rides the existing
`parse_bootstrap` target). There is NO new byte-parser this phase (unlike the
phase-69 gRPC message decoder), so a dedicated target is NOT expected;
**confirm at the state-2 PLAN-write** (PV-5). If a seed is added it must be
`!`-un-ignored (memory `fuzz-corpus-seed-gitignored-by-default`) and, if a new
target were ever added, wired into `ci.yml` by hand (memory
`new-fuzz-target-needs-a-ci-yml-step`).

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

- **PV-1 — the `AccessLogFilter` serde model + cardinality validator.** Confirm
  the `SubstitutionFormatString` Option-arm + `AmbiguousLogFormat`-cardinality
  precedent (`bootstrap.rs:751`-`766`; the validator is `validate_access_logs`).
  Decide: an `Option`-per-variant struct with an "exactly one" validator (the
  precedent) vs. an internally-tagged enum. Author the `ConfigError` for
  zero-variant (and reserve the >1 path for the future multi-variant phase).
  Confirm WHERE in the access-log validator the filter cardinality is checked.
- **PV-2 — the differential driver suppression extension.** `run_http1_access_log_byte_exact_arm`
  (`lib.rs:6223`+) asserts `file line-count == probe count` (`lib.rs≈6357`).
  Extend minimally so a probe expected to be SUPPRESSED contributes no line —
  e.g. an `expect_logged: bool` on `AccessLogByteExactProbe` (`lib.rs:1104`) with
  the count/compare over the `expect_logged==true` subset. Confirm the flush/settle
  is deterministic (the existing accesslog fixtures already wait for the flush).
  Keep the change surgical — do NOT disturb the 27 existing fixtures.
- **PV-3 — the H2 emit-loop wiring point.** Locate the H2 access-log emit loop in
  `crates/envoy-http2/src/hcm.rs` (the sibling of `envoy-http1/src/hcm.rs:1479`-`1518`
  that drives `0064`-`0070`) and thread the same `should_log` gate so H2 does not
  regress. Confirm the record on both paths already carries the final response
  status the gate needs.
- **PV-4 — the empty-`runtime_key` parity.** Confirm the measured PGV `min_len 1`
  (R-0.4) and author the fail-loud `ConfigError` (native message OK per
  ADR-0049). Confirm `runtime_key` is otherwise inert (no RTDS consumer) and
  document the boundary in the SPEC/CONTRACT.
- **PV-5 — §6.1 size re-derivation + §7.4.** Re-estimate net LoC / task count
  against the live tree (§8). This is a SMALL leaf (~500-700 LoC) — a split is
  very unlikely, but PV-5 re-derives (ADR-0142 held in reserve only as a
  formality). Confirm the §7.4 disposition (corpus seed, no new target).
- **PV-6 — the op comparison semantics + which status.** Confirm `op(status,
  default_value)` for EQ/GE/LE reads the FINAL response code the access-log
  record carries (including filter-generated / local-reply codes such as the
  `direct_response` 503 and any synth-503). Re-confirm the boundary (GE is
  `status >= value`, LE is `status <= value`, EQ is `status == value`) — measured
  GE at R-0.6; EQ/LE parse-validated at R-0.3 and semantically mirror GE.

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **Outlier-detection variant `consecutive_local_origin_failure`** (upstream-
  robustness remainder). Reuses the phase-14 `EndpointEjection`/`OutlierEjectionSweeper`,
  BUT is heavier: envoy-rust models NO local-origin signal (connect failures are
  laundered through a synthetic 503 into the HTTP detectors,
  `crates/envoy-cluster/src/ejection.rs:152`-`192`) and has NO `health_flags` /
  `/failed_outlier_check` surface at all (grep-confirmed zero hits) — it would
  need the local-origin signal path + `split_external_local_origin_errors` +
  `enforcing_*` knobs. More surface than the access-log filter for a same-class
  ejection→503 observable.
- **Circuit-breaker `max_pending_requests > 0` queue** — the real pending queue
  is unbuilt (only value `0` reject-on-establish is enforced,
  `crates/envoy-http1/src/pool.rs:56`-`61`); triggering a pending overflow
  deterministically needs concurrency/timing control — a flaky differential.
- **The missing `upstream_rq_overflow` stat name** — phase 17 routes `max_requests`
  overflow to `upstream_rq_pending_overflow` (`crates/envoy-cluster/src/budget.rs:149`),
  a divergence from upstream (`upstream_rq_overflow`). Fixing it is a thin
  stat-rename with a degenerate observable and reopens a landed phase-17
  decision — too thin/risky for a standalone phase; a carry-forward at most.
- **HTTP retry `per_try_timeout` / fault `fixed_delay`** — both timing-based
  (a slow upstream + a timer); the differential needs latency bounds and is
  flaky. Deferred to a timing-tolerant phase.
- **HTTP retry `retriable_headers`** — deterministic + reuses the phase-04.2
  header-matcher, but needs a controllable retriable-response backend; heavier
  than the backend-free access-log filter.
- **Network-filters remainder / `sni_cluster` / non-deterministic LB / HTTP/3 /
  gRPC bridge / observability SINKS (gRPC ALS, OTLP) / WASM host** — each is a
  large new subsystem (a payload codec, a `tls_inspector` listener filter, a
  contract-relaxation ADR, a `quinn`/`h3` stack, a gRPC service, or a WASM
  engine) — far above the cheapest-strong-differential bar.

**The access-log `status_code_filter` wins:** it OPENS a real, named
observability-family subsystem (the access-log filter), reuses the ENTIRE mature
`envoy-accesslog` emitter + the byte-exact `Http1AccessLogByteExact` differential
driver + `direct_response` (cheapest), is fully deterministic on a byte-exact
single-line file observable with NO backend (strong), and introduces **no new
subsystem** — only a `filter` config field + a `should_log` predicate + one
bounded driver extension. It mirrors the phase-66 "family OPENER" bar and leaves
a clean seam of future cheapest-strong leaves (the remaining eleven filter
variants).

---

## §5. Differential surface at phase end

- **NEW fixture `0076-accesslog-status-code-filter`** — green cross-proxy: an H1
  HCM listener with a file access log (deterministic `text_format_source`) +
  `status_code_filter { comparison: { op: GE, value: { default_value: 500,
  runtime_key: "unused" } } }` + two `direct_response` routes (`/log` → 503,
  `/nolog` → 200). Two probes drive both routes; the access-log file across BOTH
  proxies is asserted the SAME single **byte-identical** line (the `/log` 503; the
  `/nolog` 200 SUPPRESSED), via `Http1AccessLogByteExact` + the PV-2 suppression
  extension + `set_equal_modulo_allow_list` where the format demands it.
- **All pre-existing fixtures `0001`–`0075` stay green** — a sink with no
  `filter` behaves exactly as today; no existing fixture sets a `filter` (§7.5 (b)).
- **In-process:** the `should_log` EQ/GE/LE boundary semantics, the
  oneof-cardinality (zero-variant) + empty-`runtime_key` fail-loud rejections,
  the RTDS-inert `runtime_key`, and the no-`filter`-still-logs regression.

**Why the differential needs no backend:** the strong, deterministic byte-exact
observable comes from two `direct_response` routes (503 kept / 200 suppressed) —
no cluster, no upstream (R-0.6, mirroring the `direct_response`-based access-log
fixtures).

---

## §6. `BEHAVIOR_CONTRACT.md` additions

A `status_code_filter` subsection under the access-log section recording the
MEASURED facts (R-0.2–R-0.6): an `AccessLog.filter` gates emission per sink;
`status_code_filter.comparison { op: EQ|GE|LE, value: RuntimeUInt32 {
default_value, runtime_key } }`; `op(status, default_value)` on the final
response code decides emission (`GE 500` drops a 200, keeps a 503); a
`direct_response` response IS logged and a `direct_response` 503 carries
`%RESPONSE_FLAGS% = -`; `runtime_key` is REQUIRED non-empty (load-parity, PGV
`min_len 1`) but RTDS-inert (comparison always uses `default_value`); the
`AccessLogFilter` oneof cardinality is fail-loud; a sink with no `filter` logs
every record (unchanged).

---

## §7. ADR reservations

- **ADR-0140 (FIRED this session, reclaimed):** the phase-70 pick + scope +
  rejected alternatives (this SPEC's decisions).
- **ADR-0141 (reserved):** the §6.2 empirical-verification reconciliation at the
  state-2 PLAN-write (PV-1..PV-6 resolutions — the `AccessLogFilter` serde model
  + cardinality validator, the driver suppression extension, the H2 emit-loop
  wiring, the empty-`runtime_key` parity, the op comparison semantics).
- **ADR-0142 (reserved):** the §6.1 split, if PV-5 fires it (very unlikely — this
  is a small single-phase leaf).

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `envoy-config`: `AccessLog.filter` + `AccessLogFilter`/`StatusCodeFilter`/`ComparisonFilter`/`ComparisonOp`/`RuntimeUInt32` schema | ~120 |
| `envoy-config`: cardinality + empty-`runtime_key` validators + `ConfigError` variants | ~60 |
| `envoy-accesslog` + HCM: the `should_log` predicate + the per-sink gate (H1 + H2) | ~80 |
| fixture `0076` (2 YAMLs + expectations + README) + the `Http1AccessLogByteExact` suppression extension | ~170 |
| in-process tests (EQ/GE/LE boundaries + cardinality + runtime_key + regression) | ~180 |
| `BEHAVIOR_CONTRACT.md` + ROADMAP/docs | ~60 |
| **Total** | **~670 net LoC / ~9–11 tasks** |

Well UNDER the ~1500 LoC / ~25 task gate — a **single phase**, no split projected
(lighter than phase 68 ~1050 and phase 69 ~1000–1100; there is no new
codec/primitive, only a config sub-message + a predicate + a one-line driver
extension). PV-5 re-derives at the state-2 PLAN-write; ADR-0142 is held in
reserve as a formality only.

---

## §10. Carry-forwards NOT consumed by this pick (surviving phase 69's close)

None obligate this phase; each is owned by whatever future phase touches its
surface. This phase touches the access-log config + emitter + the differential
driver — it consumes NONE of the below outright.

- **M69-A..I** — gRPC-HC doc/coverage polish (owner = the next phase touching the
  gRPC-HC surface). **Not touched here.**
- **CF-69-1/2/3/5** — the phase-69 documented boundaries / correct divergences /
  reasonable KEEPs. **Not touched here.**
- **M68-1** — empty-hex `text:""` TCP-HC validator gap (owner = next phase
  touching the TCP-HC payload validator). **Not touched here.**
- **M-1** — the `CidrRange` `prefix_match` guard band. **Not touched here.**
- **CF-67-3** — payload-visible `on_data` network-filter iteration (deferred).
- **CF-67-5** — empty `filters: []` connection behavior.
- **CF-67-6** — bound `close_with_drain`'s drain (`delayed_close_timeout`).
- **CF-67-7** — the TLS `[rbac, tcp_proxy]` establishment ordering (owner = a
  future TLS-establishment phase).
- The older still-live Minors in `67.3/SPEC.md` §10 and the HTTP-filters-family
  carry-forwards (1)–(4) in `STATE.md` `## Notes`.

**Natural next cheapest-strong leaves this phase UNLOCKS** (each reuses the
`filter` field + `should_log` seam this phase lands, none an obligation):
`response_flag_filter` (reuses the phase-48–65 `%RESPONSE_FLAGS%` work —
strong thematic continuity), `header_filter` (reuses the phase-04.2 header
matcher), `duration_filter` (timing), `and_filter`/`or_filter` (recursive
composition), and the H2 access-log-filter differential.
