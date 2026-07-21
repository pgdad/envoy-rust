# Phase 73 — Observability family: the access-log FILTER subsystem — arms #4 & #5, the recursive `and_filter` / `or_filter` composition

> **State-0/1 pick session** (per `BOOTSTRAP_PROMPT.md` §5 state-0/1 + memory
> `closeout-and-pick-are-separate-sessions`). This SPEC is the brainstorm output
> for a NEW phase `73`. Every §0 wire/behavior claim below was **MEASURED this
> session against `envoyproxy/envoy:v1.33.0`** (`docs/envoy-rust/ENVOY_TARGET.md`
> pin, digest `sha256:56da5afd…770c2`) via two read-only recon fan-outs: an
> in-tree change-surface survey and a live `--mode validate` + port-mapped
> runtime probe with graceful-stop flush.

**Pick in one line:** now that the `AccessLogFilter` oneof has THREE leaf arms
(`status_code_filter` / `response_flag_filter` / `header_filter`), add the two
**recursive composition arms** `and_filter` / `or_filter` — each of upstream
shape `{ filters: repeated AccessLogFilter }` (PGV `min_items = 2`) — that AND /
OR their nested child predicates. This is the arm the phase-72 SPEC §4 named as
"a natural NEXT leaf once ≥3 leaf arms exist … deferred, not rejected on merit."

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

### R-0.1 — the in-tree change surface (MEASURED in-tree; both arms are greenfield in the access-log path, but every piece they need already exists)

The access-log FILTER seam built by phases 70/71/72 is directly extensible; the
whole matching engine and differential driver are reused verbatim. MEASURED
seams (re-confirm exact line numbers at the state-2 PLAN-write — they drift):

- **Config struct** — `crates/envoy-config/src/bootstrap.rs:721-733`:
  `AccessLogFilter { status_code_filter, response_flag_filter, header_filter }`,
  each `Option<…>`, `#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]`
  `#[serde(default, deny_unknown_fields)]`. Adding `and_filter: Option<AndFilter>`
  + `or_filter: Option<OrFilter>` where `AndFilter { filters: Vec<AccessLogFilter> }`
  (and `OrFilter` likewise) is **safe with NO `Box`** — the recursion runs
  *through* `Vec<T>` (a fixed-size heap pointer), so the type stays finite-size;
  serde derive + `#[serde(default, deny_unknown_fields)]` compose over recursive
  types natively; `Debug/Default/Serialize/Deserialize/PartialEq` all compose
  across `Vec<Self>`. `AccessLogFilter` does **not** derive `Clone` (all consumers
  take `&AccessLogFilter`); `AndFilter`/`OrFilter` match that.
- **Validator** — `validate_access_logs` (`bootstrap.rs`, ~5159-5254): a
  compiler-forced full destructuring of every arm (no `..`, per the M70-R1 comment
  it carries) → a `set_arms` count over the arm-set booleans → cardinality reject
  `set_arms != 1` as
  `AmbiguousAccessLogFilter { detail }`; then per-arm leaf checks (status-code
  `runtime_key` non-empty; response-flag token membership; header via
  `validate_header_matcher(&mut hf.header)` which compiles the SafeRegex in place).
  The two new arms **force** two more destructure bindings + two more `set_arms`
  entries (5→arms), and need (a) a `filters.len() >= 2` check per arm, (b)
  **recursive** validation of every nested filter. The per-filter validation body
  is currently *inline* in the `for entry` loop; recursion requires **extracting
  it into a `&mut`-taking helper** (e.g. `validate_access_log_filter(&mut AccessLogFilter)`)
  that calls itself over `filters` — the single largest mechanical change, and the
  one to pin with a nested-cardinality / nested-bad-leaf negative test.
- **Compile** — `compile_access_log_filter` (`crates/envoy-http1/src/hcm.rs`,
  ~1745-1775): today a 3-tuple `match` mapping each leaf arm to a runtime
  `LogFilter`, `_ => unreachable!()` fallback, called at `hcm.rs:208`. It already
  takes `&AccessLogFilter` and returns `LogFilter`, so it is **recursion-ready**:
  the tuple widens to 5, the two new arms map to
  `LogFilter::And(af.filters.iter().map(compile_access_log_filter).collect())` /
  `LogFilter::Or(…)`.
- **Runtime enum** — `crates/envoy-accesslog/src/filter.rs:43-97`:
  `#[derive(Debug, Clone)] pub enum LogFilter { StatusCode(…), ResponseFlag{…},
  Header{ matcher: Arc<dyn HeaderMatch> } }` — **NO `Eq`/`PartialEq`** (ADR-0150).
  `should_log(&self, status: u16, response_flags: &str, headers: &[(String,String)])`.
  Adding `And(Vec<LogFilter>)` / `Or(Vec<LogFilter>)` recurses through `Vec` (no
  `Box`), composes under `Debug + Clone`, introduces **NO** `Eq`/`PartialEq` and
  **NO** `envoy-config` dependency → **ADR-0150 holds**. `should_log` arms:
  `And(fs) => fs.iter().all(|f| f.should_log(…))`, `Or(fs) => fs.iter().any(…)`.
- **Errors** — `crates/envoy-config/src/lib.rs`: existing filter arms
  `AmbiguousAccessLogFilter { detail }`, `EmptyStatusCodeFilterRuntimeKey`,
  `UnknownResponseFlag { token }`. The min-items rule needs **one** new variant
  (a shared `and_filter/or_filter must have ≥2 filters` message, `count` carried);
  nested leaf failures REUSE the existing variants (no per-leaf new errors). The
  two new config structs re-export from `bootstrap::{…}`.
- **Differential driver** — `tests/differential/src/lib.rs`: the
  `Http1AccessLogByteExact` driver + its `AccessLogByteExactProbe`
  (`method`/`path`/`host`/`extra_headers`/`body`/`expected_status`/`expect_logged`;
  the expected line count is computed by the driver's `expected_logged_count`
  helper, NOT a probe field) **already supports an and/or fixture with ZERO
  driver change** — a new fixture is pure config.yaml + probes.json + a ~10-line
  test file cloned from `access_log_header_filter.rs`. (The H2 sibling
  `AccessLogH2ByteExact` also exists but is NOT required — H1 matches the
  0076/0077/0078 precedent.)

### R-0.2 — LIVE-ENVOY (`--mode validate`, networking-free): the `and_filter` / `or_filter` wire shape

Six probe configs run against `envoyproxy/envoy:v1.33.0 --mode validate`
(`AccessLog.filter` in an H1 HCM), MEASURED:

- **`and_filter` / `or_filter` shape** = `{ filters: [<AccessLogFilter>, …] }`, a
  mutually-exclusive `AccessLogFilter` **oneof** arm (siblings of
  status/response-flag/header).
- **PGV `min_items = 2` on `filters`, BOTH arms** — a single-element or empty
  `filters` is REJECTED:
  `AndFilterValidationError.Filters: value must contain at least 2 item(s)`
  (empty `and_filter: {}` fails identically).
- **Recursion accepted** — a `header_filter` / `status_code_filter` /
  `response_flag_filter` nested inside `filters` validates; a **nested
  `and_filter` inside an `or_filter`** validates (depth-2). Validation descends
  INTO each child (a probe with a `status_code_filter` child missing its
  PGV-required `RuntimeUInt32.runtime_key` failed with the error rooted deep at
  `AndFilterValidationError.Filters[1] … RuntimeUInt32ValidationError.RuntimeKey:
  value length must be at least 1 characters` — proving the recursive descent).
- **oneof mutual exclusivity** — setting BOTH `header_filter` and `and_filter` on
  one `filter` is REJECTED at parse: `'and_filter' has already been set (either
  directly or as part of a oneof)`.
- **Positive control** — `and_filter: { filters: [header_filter, response_flag_filter] }`
  and `or_filter: { filters: [header_filter, header_filter] }` both report
  `configuration OK`.

### R-0.3 — LIVE-ENVOY (runtime, port-mapped, no backend, graceful-stop flush): `and_filter` keep/drop is deterministic + byte-exact

A port-mapped H1 HCM, file access log
`text_format_source "S=%RESPONSE_CODE% A=%REQ(X-A)% B=%REQ(X-B)%\n"`,
`filter: { and_filter: { filters: [ header_filter{x-a=1}, header_filter{x-b=1} ] } }`,
one `direct_response` `/x → 200`, **no cluster / no upstream**. Four requests,
then `docker stop` (SIGTERM graceful flush). Access-log file (MEASURED):

```
S=200 A=1 B=1        # probe 1: x-a:1 x-b:1  → AND true  → KEPT
S=200 A=1 B=1        # probe 4: x-a:1 x-b:1  → AND true  → KEPT
```

probe 2 (`x-a:1` only) and probe 3 (neither) — AND false → **DROPPED** (absent
from the file). AND = **all children match**. Two byte-identical kept lines.

### R-0.4 — LIVE-ENVOY (runtime): `or_filter` keep/drop is deterministic + byte-exact

Same fixture with `or_filter` swapped in. Four requests, graceful flush. Access
log (MEASURED):

```
S=200 A=1 B=-        # probe 1: x-a:1 only → OR true → KEPT
S=200 A=- B=1        # probe 2: x-b:1 only → OR true → KEPT
S=200 A=1 B=1        # probe 4: both      → OR true → KEPT
```

probe 3 (neither) — OR false → **DROPPED**. OR = **any child matches**. Three
byte-exact kept lines.

### R-0.5 — LIVE-ENVOY (runtime): depth-2 recursion witnessed differentially

`or_filter: { filters: [ and_filter{ [header_filter{x-a=1}, header_filter{x-b=1}] },
header_filter{x-c=1} ] }`, format
`"S=%RESPONSE_CODE% A=%REQ(X-A)% B=%REQ(X-B)% C=%REQ(X-C)%\n"`. Four requests,
graceful flush (MEASURED):

```
S=200 A=1 B=1 C=-    # x-a:1 x-b:1  → AND-child true  → OR true → KEPT
S=200 A=- B=- C=1    # x-c:1        → leaf true       → OR true → KEPT
```

`x-a:1` only (AND-child false [b absent], leaf false [c absent]) → DROPPED;
neither → DROPPED. A single depth-2 fixture witnesses OR-of-(nested-AND, leaf)
byte-exact and deterministic — the recursion is observable cross-proxy.

### R-0.6 — recursion is hazard-free in Rust (MEASURED in-tree)

No infinite-size type (both the config `AccessLogFilter` and the runtime
`LogFilter` recurse through `Vec<_>`, a fixed-size pointer → **no `Box` at either
layer**); no serde recursion problem; no derive break; the runtime addition
introduces no `Eq`/`PartialEq` and no `envoy-config` dependency (**ADR-0150
intact**). The only real refactor is extracting the validator's inline per-filter
body into a `&mut`-taking recursive helper (R-0.1) — mechanical, pinned by a
nested-negative test. Unbounded nesting depth is a theoretical stack risk;
upstream does not bound it and fixtures stay at depth 1-2, so no depth guard is
required for parity (documented deferred non-goal, §2.2).

### R-0.7 — numbering

Next ROADMAP id **73**; next fixture ids **0079** (and_filter) + **0080**
(or_filter, depth-2); next ADR **ADR-0152** (ledger head ADR-0151, next-available
ADR-0152).

---

## §1. Goal

Land the two **recursive composition arms** of `envoy.config.accesslog.v3.AccessLogFilter`
— `and_filter` / `or_filter` — end-to-end over the EXISTING phase-70/71/72
`filter` seam, the reused matching predicates, and the byte-exact
`Driver::Http1AccessLogByteExact` differential driver (ZERO driver change). A
sink whose `filter` is an `and_filter`/`or_filter` emits a record iff **all** /
**any** of its nested child predicates match; children may be any
`AccessLogFilter` (leaf OR another composition — arbitrary depth). This lights up
a genuinely NEW observable axis (boolean COMPOSITION of the existing predicates)
at near-zero new logic — every leaf predicate + the whole differential driver are
reused; the only new machinery is one recursive config type + one recursive
validation helper + two `LogFilter` variants (`.all()` / `.any()`).

Concretely add: (i) the `AndFilter { filters: Vec<AccessLogFilter> }` +
`OrFilter { filters: Vec<AccessLogFilter> }` config structs + the `and_filter` /
`or_filter` oneof arms (compiler-forced 5-arm destructuring in the validator +
5-arm compile match); (ii) the `filters.len() >= 2` fail-loud validation +
recursive per-child validation (extracted `&mut` helper); (iii) the
`LogFilter::And(Vec<LogFilter>)` / `LogFilter::Or(Vec<LogFilter>)` runtime
variants with `should_log` evaluating `.all()` / `.any()` (unchanged signature —
the phase-72 widened `should_log(status, response_flags, headers)` already carries
everything a composition needs); (iv) the recursive `compile_access_log_filter`;
(v) TWO NEW backend-free byte-exact fixtures `0079` (and_filter) + `0080`
(or_filter, depth-2 recursion) proving the keep/drop set cross-proxy.

---

## §2. Scope

### 2.1 In scope

1. **`AndFilter` / `OrFilter` config structs** (`envoy-config`) — each
   `{ filters: Vec<AccessLogFilter> }`, `#[derive(Debug, Default, Serialize,
   Deserialize, PartialEq)] #[serde(default, deny_unknown_fields)]` (mirror the
   leaf arms; NO `Clone`; NO `Box` — the `Vec` breaks the recursion, R-0.1/R-0.6).
2. **The `and_filter` / `or_filter` oneof arms** on `AccessLogFilter` (two new
   `Option<…>` fields) + the two `bootstrap::{…}` re-exports.
3. **Fail-loud validation** (`validate_access_logs`): (a) the compiler-forced
   5-arm destructuring + 5-entry `set_arms` cardinality array (still exactly-one
   across the WHOLE oneof — an `and_filter` set alongside any other arm is
   ambiguous); (b) `filters.len() >= 2` per composition arm (ONE new
   `ConfigError` variant, shared); (c) **recursive** validation of every nested
   `AccessLogFilter` — extract the inline per-filter body into a `&mut`-taking
   helper `validate_access_log_filter` that recurses over `filters` (so nested
   `header_filter` SafeRegex still compiles in place, nested cardinality /
   bad-leaf still fail-loud).
4. **`LogFilter::And(Vec<LogFilter>)` / `LogFilter::Or(Vec<LogFilter>)`** runtime
   variants (`envoy-accesslog`) + the two `should_log` arms (`.all()` / `.any()`)
   — signature UNCHANGED (phase-72 already widened it to carry `headers`). No new
   `Eq`/`PartialEq`; no `envoy-config` dep (ADR-0150).
5. **Recursive `compile_access_log_filter`** (the 5-arm tuple match; the two
   composition arms map each child via `.iter().map(compile_access_log_filter)`).
6. **NEW fixture `0079-accesslog-and-filter`** — H1 HCM, file access log,
   `and_filter { filters: [ header_filter{x-a=1}, header_filter{x-b=1} ] }`, one
   `direct_response` `/x → 200`, NO backend. Probes ordered **kept-LAST** per the
   ADR-0147 authoring convention: a DROPPED probe (`x-a:1` only) then a KEPT probe
   (`x-a:1 x-b:1`) → a single byte-identical line asserted the SAME across both
   proxies. Reuses the driver `extra_headers` + `expect_logged` fields verbatim.
7. **NEW fixture `0080-accesslog-or-filter`** — H1 HCM, `or_filter { filters: [
   and_filter{ [header_filter{x-a=1}, header_filter{x-b=1}] }, header_filter{x-c=1} ] }`
   (depth-2, witnessing the recursion differentially, R-0.5), NO backend. Probes
   ordered kept-LAST → the kept lines byte-identical cross-proxy.
8. **In-process tests:** `should_log` for `And`/`Or` over match/no-match child
   sets (incl. the all-drop and any-keep boundaries); the recursive
   `compile_access_log_filter` (nested composition → nested `LogFilter`); the
   validator's `filters.len() < 2` fail-loud (both arms, `detail` asserted); the
   nested-cardinality / nested-bad-leaf fail-loud (a `header_filter{}` or a
   `status_code_filter{runtime_key:""}` nested inside `filters` → the existing leaf
   error surfaces through the recursion); the oneof cardinality with the two new
   arms (an `and_filter` alongside a `header_filter` → ambiguous); the
   no-`filter`-still-logs regression; the `status_code_filter` /
   `response_flag_filter` / `header_filter` leaf-arm-unchanged regressions.
9. **`BEHAVIOR_CONTRACT.md`** — an `and_filter` / `or_filter` subsection under the
   access-log filter section (§6).

### 2.2 Out of scope (deliberate, with rationale)

- **A recursion-depth guard / limit.** Upstream does not bound nesting depth
  (R-0.6); fixtures stay at depth 1-2; a guard would be a DIVERGENCE. Documented
  deferred non-goal — revisit only if a stack-safety phase is ever planned.
- **The standalone H2 access-log-filter differential** (M71-6). The `should_log`
  gate is codec-agnostic and already measured byte-identical on H1; an H2
  and/or fixture adds no new composition logic. Both `0079`/`0080` are H1 (the
  0076/0077/0078 precedent). M71-6 stays live for a dedicated H2 filter phase.
- **`duration_filter` / `grpc_status_filter` / `metadata_filter` /
  `runtime_filter`** — the remaining leaf arms. `duration_filter` needs latency
  tolerance (flaky differential); `metadata_filter` needs dynamic-metadata
  plumbing; `runtime_filter` needs RTDS. Each is its own future pick (§4).
- **Any change to the differential driver.** R-0.1 confirmed ZERO driver change
  is needed; `0079`/`0080` are pure config + probes + a ~10-line test each.

### 2.3 §7.4 fuzz disposition

The `filter` surface reuses the `parse_bootstrap` parser (a new recursive
sub-message field over the already-fuzz-reachable `AccessLogFilter` path). **Default
projection:** extend the existing `parse_bootstrap` corpus seed to carry an
`access_log[].filter.and_filter.filters[…]` (exercising the recursion) — **no new
fuzz target** (the phase-68/69/70/71/72 precedent, ADR-0137: a config-only
sub-message rides the existing `parse_bootstrap` target). There is NO new
byte-parser this phase. **Confirm at the state-2 PLAN-write** (PV-7). A new/edited
seed must be `!`-un-ignored (memory `fuzz-corpus-seed-gitignored-by-default`).

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

- **PV-1 — serde model.** Confirm `AndFilter`/`OrFilter { filters: Vec<AccessLogFilter> }`
  deserialize cleanly with `#[serde(default, deny_unknown_fields)]` and NO `Box`;
  confirm the recursive `AccessLogFilter` still round-trips (`Default` reachable,
  `PartialEq` composes). Re-confirm `AccessLogFilter` does not derive `Clone`.
- **PV-2 — validator recursion refactor.** Re-confirm the exact
  `validate_access_logs` line span; confirm extracting the per-filter body into a
  `&mut`-taking `validate_access_log_filter` helper preserves the SafeRegex
  in-place compile for nested `header_filter`; confirm the 5-arm destructuring is
  compiler-forced (no `..`) and the `set_arms` array grows to 5. Pin the recursion
  with a nested-negative test (a bad leaf nested inside `filters`).
- **PV-3 — `filters.len() >= 2`.** Re-confirm ONE shared `ConfigError` variant
  suffices (or whether `and`/`or` want distinct messages) and that upstream's
  message is `value must contain at least 2 item(s)` (R-0.2) — our text need not
  match byte-for-byte (fail-loud class parity, D-3.3), but the REJECTION must
  occur. Confirm the empty `and_filter: {}` path also rejects (serde `default` →
  empty `filters` → len 0 → reject).
- **PV-4 — `should_log` `.all()` / `.any()`.** Re-confirm the phase-72 widened
  `should_log(status, response_flags, headers)` signature carries everything a
  composition needs (no further widening); confirm `And(fs) => fs.iter().all(…)` /
  `Or(fs) => fs.iter().any(…)` matches R-0.3/R-0.4 (AND=all, OR=any). Confirm the
  min_items=2 config guard means the empty-vec iterator edge (all→true, any→false)
  is unreachable at runtime.
- **PV-5 — recursive compile.** Re-confirm `compile_access_log_filter`'s location
  (`envoy-http1/src/hcm.rs`) + its single call site + that it is recursion-ready
  (`&AccessLogFilter → LogFilter`); confirm the 5-tuple widening + the two
  composition arms mapping children via `.iter().map(compile_access_log_filter)`.
- **PV-6 — driver reuse.** Re-confirm `Http1AccessLogByteExact` +
  `AccessLogByteExactProbe` need ZERO change for `0079`/`0080` (the `extra_headers`
  + `expect_logged` probe fields + the `expected_logged_count` line-count helper
  cover both fixtures);
  re-confirm the kept-LAST probe ordering + the CF-71-1 suppression settle already
  handle a dropped-then-kept sequence.
- **PV-7 — fuzz.** Re-confirm §2.3 (the recursive sub-message rides
  `parse_bootstrap`; seed extension only; no new target).
- **PV-8 — split gate.** Re-run the §8 estimate against the live tree; confirm the
  single-phase projection holds (both arms share ALL scaffolding; §4 / R-0.1).

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **Splitting into two phases (`and_filter` alone, then `or_filter`).** REJECTED
  on merit: the two arms are **structurally identical** (`{ filters:
  Vec<AccessLogFilter> }`), share the same struct shape, the same `min_items`
  validator, the same recursion-refactor helper, and the same compile widening —
  differing by exactly one word (`.all()` vs `.any()`). Splitting would duplicate
  the entire recursion/refactor scaffolding across two phases for a one-line
  semantic delta, and would leave the validator's recursion helper half-wired
  between them. The §8 estimate (~670 net LoC / ~9-12 tasks) sits WELL under the
  §6.1 split gate as one phase. The single expensive item (the validator recursion
  refactor) is paid ONCE and serves both arms.
- **`duration_filter`** — timing-based (`op(request_duration_ms, threshold)`); the
  differential needs latency bounds and is flaky. Deferred to a timing-tolerant
  phase (the same rationale that deferred it at the phase-70/71/72 picks).
- **`grpc_status_filter` / `metadata_filter` / `runtime_filter`** — each needs a
  new data axis (gRPC trailer status / dynamic metadata plumbing / RTDS
  respectively) far above the near-zero-new-logic bar this composition pick meets.
  Deferred as independent future picks.
- **The standalone H2 access-log-filter differential** (M71-6 / ADR-0140 §2.2) —
  the gate is codec-agnostic and already byte-identical, so as a standalone phase
  it is weak; better folded into a future H2 access-log-filter fixture set.
- **Re-weighed and still rejected** (re-verified against the live tree): each §9
  family opener (network-filter payload codecs, `sni_cluster`, non-deterministic
  LB, HTTP/3+QUIC, gRPC bridge/transcoding, observability SINKS [gRPC ALS, OTLP],
  runtime/RTDS, hot-restart, WASM host) — each a LARGE new subsystem far above the
  cheapest-strong-differential bar.

**`and_filter` / `or_filter` win:** they are the cheapest arms #4/#5 — they
compose the THREE already-covered leaf predicates with **zero new request-data
plumbing** (the phase-72 `should_log(status, response_flags, headers)` already
carries everything), reuse the ENTIRE differential driver unchanged (R-0.1), and
introduce exactly one recursive config type + one recursive validation helper +
two `LogFilter` variants (`.all()` / `.any()`). They yield fully deterministic
BACKEND-FREE byte-exact single-line observables on a NEW axis (boolean
COMPOSITION), witnessing recursion depth-2 cross-proxy (R-0.5). They are the
natural arm the phase-72 SPEC §4 pre-identified as next once ≥3 leaf arms exist.

---

## §5. Differential surface at phase end

- **NEW fixture `0079-accesslog-and-filter`** — green cross-proxy: an H1 HCM
  listener with a file access log (deterministic `text_format_source`
  `S=%RESPONSE_CODE% A=%REQ(X-A)% B=%REQ(X-B)%\n`) + `and_filter { filters: [
  header_filter{ name: x-a, string_match: { exact: "1" } }, header_filter{ name:
  x-b, exact: "1" } ] }` + one `direct_response` route (`/x → 200 hi`). Probes
  ordered kept-LAST: `GET /x` with `x-a:1` only (`expect_logged: false`, AND-false
  DROP) then `GET /x` with `x-a:1 x-b:1` (`expect_logged: true`, AND-true KEEP).
  The access-log file across BOTH proxies is asserted the SAME single
  **byte-identical** line `S=200 A=1 B=1`, via the EXISTING
  `Http1AccessLogByteExact` driver + the CF-71-1 suppression settle.
- **NEW fixture `0080-accesslog-or-filter`** — green cross-proxy: `or_filter {
  filters: [ and_filter{ [header_filter{x-a=1}, header_filter{x-b=1}] },
  header_filter{x-c=1} ] }`, format `S=%RESPONSE_CODE% A=%REQ(X-A)% B=%REQ(X-B)%
  C=%REQ(X-C)%\n`, one `direct_response` `/x → 200`. Probes ordered kept-LAST: a
  DROPPED probe (`x-a:1` only → AND-child false, leaf false) then the KEPT probes
  (`x-a:1 x-b:1` → AND-child true, and `x-c:1` → leaf true). The kept lines
  (`S=200 A=1 B=1 C=-`, `S=200 A=- B=- C=1`) asserted byte-identical cross-proxy —
  witnessing OR-of-(nested-AND, leaf) recursion differentially (R-0.5).
- **All pre-existing fixtures `0001`–`0078` stay green** — a sink with no
  `filter`, or a leaf `status_code_filter`/`response_flag_filter`/`header_filter`,
  is byte-unchanged; no existing fixture sets an `and_filter`/`or_filter` (§7.5 (b)).
- **In-process:** the `should_log` `And`/`Or` over match/no-match child sets + the
  recursive compile + the `filters.len() < 2` fail-loud (both arms, `detail`) +
  the nested-cardinality / nested-bad-leaf fail-loud + the 5-arm oneof cardinality
  + the no-`filter`-still-logs and leaf-arm-unchanged regressions.

**Why the differential needs no backend:** the strong deterministic byte-exact
observable comes from a `direct_response` 200 whose emission is gated purely on
client-supplied request headers composed by a boolean predicate — no cluster, no
upstream (R-0.3/R-0.4/R-0.5), mirroring fixtures `0076`/`0077`/`0078`.

---

## §6. `BEHAVIOR_CONTRACT.md` additions

An `and_filter` / `or_filter` subsection under the access-log filter section
(sibling to the phase-70 `status_code_filter`, phase-71 `response_flag_filter`,
and phase-72 `header_filter` subsections), recording the MEASURED facts
(R-0.2–R-0.6): `filter: { and_filter: { filters: [<AccessLogFilter>, …] } }` (and
`or_filter` likewise) gates emission per sink; a record is KEPT iff **all**
(`and_filter`) / **any** (`or_filter`) of the nested child predicates match;
`filters` is PGV `min_items = 2` (fewer → fail-loud); children may be any
`AccessLogFilter` (leaf or a nested composition — arbitrary depth, no depth
guard, matching upstream); `and_filter`/`or_filter`/`status_code_filter`/
`response_flag_filter`/`header_filter` are mutually-exclusive oneof arms; a sink
with no `filter` logs every record (unchanged); the depth-2 `or[and[a,b], c]`
worked example (R-0.5).

---

## §7. ADR reservations

- **ADR-0152 (FIRED this session):** the phase-73 pick + scope + rejected
  alternatives (this SPEC's decisions).
- **ADR-0153 (reserved):** the §6.2 empirical-verification reconciliation at the
  state-2 PLAN-write (PV-1..PV-8 resolutions — the `AndFilter`/`OrFilter` serde
  model, the validator recursion refactor + `filters.len() >= 2`, the two
  `LogFilter` variants + the recursive compile, the driver-reuse confirmation,
  the fuzz disposition).
- **ADR-0154 (reserved):** the §6.1 split, if PV-8 fires it (very unlikely — both
  arms share ALL scaffolding; §4).

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `envoy-config`: `AndFilter`/`OrFilter` structs + the 2 oneof arms + re-exports | ~25 |
| `envoy-config`: validator recursion refactor (`&mut` helper) + `filters.len() >= 2` + 1 error variant + 5-arm cardinality | ~60 |
| `envoy-accesslog` + HCM: `LogFilter::And`/`Or` + 2 `should_log` arms + the recursive `compile_access_log_filter` (5-arm tuple) | ~30 |
| fixtures `0079` + `0080` (2× config + probes + expectations + README) — reuses the driver | ~260 |
| differential test entrypoints (2× ~10 LoC cloned from `access_log_header_filter.rs`) | ~20 |
| in-process tests (should_log all/any + recursive compile + min-items + nested-negative + 5-arm cardinality + regressions) | ~220 |
| `BEHAVIOR_CONTRACT.md` + ROADMAP/docs | ~55 |
| **Total** | **~670 net LoC / ~9–12 tasks** |

Well UNDER the ~1500 LoC / ~25 task gate — a **single phase**, no split projected
(comparable to phase 70 ~670 / phase 71 ~630 / phase 72 ~640; every leaf
predicate + the whole differential driver already exist, and the two arms share
ALL scaffolding — the single expensive item, the validator recursion refactor, is
paid once). PV-8 re-derives at the state-2 PLAN-write; ADR-0154 is held in reserve
as a formality only.

---

## §10. Carry-forwards

**CHEAPLY FOLDED** (this phase touches `validate_access_logs` + the access-log
filter surface):

- **M71-3** (the all-suppressed `expected_logged_count == 0` driver shape
  untested) — this phase adds two suppression fixtures; weigh at state-2 whether
  an all-drop probe set (`expected_logged_count: 0`) is cheaply added as a third
  probe group or an in-process assertion, else carry forward.

**OPENED by this pick** (owner = whatever future phase touches the surface):

- **CF-73-1** — the recursive `and_filter`/`or_filter` accepts arbitrary nesting
  depth with NO stack guard (parity with upstream, R-0.6). A deeply-nested config
  is a theoretical stack risk; deferred as a documented non-goal. Owner = a future
  stack-safety / DoS-hardening phase, if ever planned.

**NOT consumed** (owner = whatever future phase touches their surface):

- **CF-72-1** (the shared-engine value-matcher `absent+invert` divergence —
  MODE-SCOPED, memory `envoy-headermatcher-invert-absent-is-mode-dependent`) +
  **CF-72-2** (name-only `{name}` → `PresentMatch(true)` + `treat_missing_header_as_empty`)
  — `HeaderMatcher`-parity, not touched here (this phase composes filters, it does
  not alter the header-match engine). A future `HeaderMatcher`-parity phase owns them.
- **M71-6** (H2 access-log-filter differential) — NOT discharged here (§2.2). **M71-7**
  (latent multi-flag membership) + **M71-8** (`""`/whitespace token rejection) —
  response-flag-specific, NOT touched.
- **M70-R4** (`"filter": null` serialization) + **M70-R9** (provenance note) —
  access-log-adjacent; weigh at state-2 whether cheaply folded, else carry forward.
- **M69-A..I, CF-69-1/2/3/5, M68-1, M-1, CF-67-3/5/6/7**, the older Minors in
  `67.3/SPEC.md` §10, and the HTTP-filters-family (1)–(4) in `STATE_HISTORY.md` —
  all untouched by a composition arm; carry forward.
