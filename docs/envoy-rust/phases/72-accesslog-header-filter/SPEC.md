# Phase 72 — Observability family: the access-log FILTER subsystem — arm #3, `header_filter`

> **Status:** `in-progress` (§5 state-1 brainstorm output). This SPEC is the
> brainstorming deliverable for a stranger with zero prior context (D-3.4).
> Every load-bearing wire/behavior claim in §0 was MEASURED against the pinned
> reference `envoyproxy/envoy:v1.33.0` (D-3.3 / D-3.7) during the state-0 recon
> of this session (two read-only recon fan-outs — one in-tree code survey, one
> live-Envoy `--mode validate` + port-mapped runtime probe with graceful-stop
> flush); nothing here is asserted from memory or upstream source.
>
> **Pick + scope recorded in ADR-0148** (ledger head was ADR-0147; ADR-0148 was
> UNRESERVED — claimed by this pick). The next session is the §5 state-2
> PLAN-write (`superpowers:writing-plans`) — do NOT implement from this SPEC.

---

## §0. State-0 recon — evidence (MEASURED this session against `envoyproxy/envoy:v1.33.0`)

Phases 70 and 71 built the access-log **FILTER** subsystem
(`envoy.config.accesslog.v3.AccessLogFilter` — the per-`AccessLog`-entry
predicate deciding **whether a log record is emitted at all**) up to TWO oneof
arms: `status_code_filter` (arm #1, phase 70) and `response_flag_filter` (arm #2,
phase 71). Phase 71 also **widened the runtime `should_log` seam** from
status-only to `(status, response_flags)` and threaded it through BOTH HCM emit
loops. This phase lands the **THIRD oneof arm**, `header_filter`
(`envoy.config.accesslog.v3.HeaderFilter`, which wraps a single
`envoy.config.route.v3.HeaderMatcher`): a sink emits a record ONLY IF a named
**request header** matches the configured matcher.

It is the textbook cheapest-strong-differential arm-#3 leaf because the two most
expensive pieces already exist and are reusable **verbatim**: (1) the ENTIRE
`HeaderMatcher` config type + its 7-mode matching ENGINE
(`HeaderMatcher::matches(&[(String, String)]) -> bool`) landed in phase 04.2 for
route header matching, and (2) the request headers it needs are ALREADY IN SCOPE
at both HCM emit gates. The only genuinely-new runtime plumbing is widening
`should_log` once more to carry the request-header slice — the exact same class
of change phase 71 already performed. It yields a fully deterministic,
**backend-free** byte-exact single-line observable on a NEW axis (request-header
membership), and this phase is the designated owner to consume the phase-71
carry-forward **CF-71-1** (ordering-aware settle hardening) + **M71-2** (stale
doc phrasing) because it touches both the differential driver and the
access-log-filter surface.

### R-0.1 — the in-tree arm-#3 seam (MEASURED in-tree; `header_filter` is greenfield in the access-log path, but its matcher already exists)

`grep -rni "header_filter\|HeaderFilter" --include=*.rs` returns ZERO hits in the
access-log path — the arm is greenfield there. The reuse points phase 71 left,
plus the phase-04.2 `HeaderMatcher` this arm rides on:

- **Config oneof (`crates/envoy-config/src/bootstrap.rs:720`-`728`).**
  `AccessLogFilter` is an **Option-per-variant struct** (NOT an internally-tagged
  enum), cardinality enforced by the validator:
  ```rust
  #[serde(default, deny_unknown_fields)]
  pub struct AccessLogFilter {
      pub status_code_filter: Option<StatusCodeFilter>,
      pub response_flag_filter: Option<ResponseFlagFilter>,
  }
  ```
  Its doc comment (`bootstrap.rs:716`-`717`) states the arm-growth contract
  literally: *"future filter-family phases add further `Option` arms here rather
  than reshaping the type."* Arm #3 adds `pub header_filter: Option<HeaderFilter>`
  here + a new `HeaderFilter { header: HeaderMatcher }` type. Today
  `header_filter` is an unknown field on this `deny_unknown_fields` struct, so a
  `header_filter` config is currently **boot-fatal** (`ConfigError::Yaml`).
- **The reusable `HeaderMatcher` config type + matching ENGINE (phase 04.2).**
  `HeaderMatcher` (`bootstrap.rs:3025`-`3041`) is a standalone type
  `{ name: String, mode: HeaderMatcherMode, invert_match: bool }`, exported from
  `lib.rs`. `HeaderMatcherMode` (`bootstrap.rs:3043`-`3066`) has **all 7 arms**:
  `ExactMatch`, `PrefixMatch`, `SuffixMatch`, `SafeRegexMatch`, `RangeMatch`,
  `PresentMatch(bool)`, `StringMatch(StringMatcher)` (the modern generic union:
  `exact`/`prefix`/`suffix`/`contains`/`safe_regex`, with `ignore_case`). Its
  hand-rolled `Deserialize` (`bootstrap.rs:3068`-`3189`) accepts BOTH the flat
  deprecated keys (`exact_match`, `prefix_match`, …) AND the nested
  `string_match`/`safe_regex_match`/`range_match`/`present_match`, plus
  `invert_match`. **The evaluation engine is directly reusable**
  (`crates/envoy-config/src/matcher.rs:21`-`52`):
  ```rust
  pub fn matches(&self, headers: &[(String, String)]) -> bool {
      let value = headers.iter()
          .find(|(n, _)| n.eq_ignore_ascii_case(&self.name))
          .map(|(_, v)| v.as_str());
      let mode_result = match &self.mode { /* 7 arms */ };
      mode_result ^ self.invert_match
  }
  ```
  Its signature takes EXACTLY the shape of the request-header slice
  (`Vec<(String, String)>`) available at the emit gate — an access-log
  `header_filter` calls `matcher.matches(&req.headers)` with **zero new engine
  code**. The validators for empty header name (`ConfigError::EmptyHeaderName`,
  `lib.rs:394`) and unparseable regex (`ConfigError::InvalidRegex`, `lib.rs:398`)
  also already exist.
- **The runtime predicate + `should_log` seam (`crates/envoy-accesslog/src/filter.rs`).**
  Phase 71 left `LogFilter` a two-variant enum
  (`StatusCode(..)`, `ResponseFlag { flags }`) and the evaluate seam
  **`should_log(&self, status: u16, response_flags: &str) -> bool`**
  (`filter.rs:38`) — it takes only the status and the response-flag token; it has
  **NO access to request headers**. Arm #3 adds a
  `LogFilter::Header { matcher: HeaderMatcher }` variant AND **widens
  `should_log` a second time** to also carry the request-header slice — the one
  genuinely-new plumbing this phase introduces. The thin wrapper
  `FileSink::should_log` (`crates/envoy-accesslog/src/file_sink.rs:102`-`107`)
  mirrors the signature and must widen in lockstep.
- **The HCM emit gates already have the request headers in scope (KEY).**
  H1 `crates/envoy-http1/src/hcm.rs:1508`-`1528`: the per-sink emit loop calls
  `sink.should_log(record.response_code, &record.response_flags)`; directly above
  it the record is built from the live request `req`, and `req.headers` (a
  `Vec<(String, String)>`), `req.method`, `req.path` are all IN SCOPE at the gate
  (used heavily nearby, e.g. `find_header(&req.headers, …)`). H2
  `crates/envoy-http2/src/hcm.rs:1131`-`1150`: the sibling loop, with
  `envoy_req.headers` in scope directly above (used at
  `hcm.rs:1116`-`1119`). So `header_filter` needs **NO new record field and NO
  new request-capture plumbing** — only to PASS the already-in-scope header slice
  into the widened `should_log`.
- **The `AccessLogRecord` does NOT carry a request-header map**
  (`crates/envoy-accesslog/src/record.rs`) — only a few DERIVED, hard-coded
  header values (`forwarded_for`, `user_agent`, `request_id`, `authority`). An
  arbitrary named header like `x-log` is NOT retrievable from the record. This is
  precisely why `should_log` must widen to take the raw header slice (which IS in
  scope at the gate), rather than reading the record.
- **The `set_arms` cardinality destructuring — extends to 3 arms mechanically**
  (`crates/envoy-config/src/bootstrap.rs:5148`-`5164`, inside
  `validate_access_logs`). Phase 71 (M70-R1) converted it to a compiler-forcing
  destructuring with no `..`:
  ```rust
  let AccessLogFilter { status_code_filter, response_flag_filter } = filter;
  let set_arms = [status_code_filter.is_some(), response_flag_filter.is_some()]
      .iter().filter(|set| **set).count();
  if set_arms != 1 { /* AmbiguousAccessLogFilter { detail } */ }
  ```
  Arm #3 adds one field to the destructure and one `.is_some()` to the array —
  **the compiler errors until both are done** (the phase-71 comment says exactly
  this). The `> 1` and `== 0` branches both map to
  `ConfigError::AmbiguousAccessLogFilter { detail }` (`lib.rs:457`). The
  `header_filter` arm gets its own sibling validation block (empty-`header`
  rejection + delegating to the existing `HeaderMatcher` validators), beside the
  `runtime_key` (`bootstrap.rs:5165`) and `response_flag` token checks
  (`bootstrap.rs:5172`).
- **`compile_access_log_filter` — extends to a 3-arm match**
  (`crates/envoy-http1/src/hcm.rs:1742`-`1762`). Phase 71 made it a 2-arm tuple
  match `match (&f.status_code_filter, &f.response_flag_filter)` with an
  `unreachable!()` default. Arm #3 widens the tuple to three and adds a
  `(None, None, Some(hf))` arm building `LogFilter::Header { matcher:
  hf.header.clone() }` — mechanically identical to how arm #2 was added.
- **The differential driver — reusable UNCHANGED.** `AccessLogByteExactProbe`
  (`tests/differential/src/lib.rs:1102`-`1120`) ALREADY carries BOTH
  `extra_headers: Vec<(String, String)>` (default `[]` — *"lets a probe exercise
  request-header command operators deterministically"*) AND `expect_logged: bool`
  (default `true`, the phase-70 suppression flag). `run_http1_access_log_byte_exact_arm`
  plumbs `probe.extra_headers` into `drive_http1`
  (`lib.rs:6290`/`6302`; H2 sibling `6013`/`6022`), waits for exactly
  `expected_logged_count` lines, then asserts byte-identical. So a
  `header_filter` fixture that drives `GET /x` WITH `x-log: yes` (kept) and
  WITHOUT it (dropped) is **pure YAML — no harness code change** for the positive
  witness. The CF-71-1 hardening (R-0.5) is the only optional driver touch.

### R-0.2 — LIVE-ENVOY (`--mode validate`, networking-free): the `header_filter` wire shape

Measured with `docker run … --mode validate -c cfg.yaml` (memory
`mode-validate-probes-wire-shape-networking-free`). `HeaderFilter = { header:
envoy.config.route.v3.HeaderMatcher }`, and `header` is **PGV-required**:

| `filter:` value | Result |
|---|---|
| `header_filter: { header: { name: "x-log", string_match: { exact: "yes" } } }` | **OK** |
| `header_filter: { header: { name: "x-log", exact_match: "yes" } }` (deprecated flat) | **OK** + a `Deprecated field … HeaderMatcher.exact_match` warning (validates) |
| `header_filter: { header: { name: "x-log", present_match: true } }` | **OK** |
| `header_filter: { header: { name: "x-log", string_match: { prefix: "ye" } } }` | **OK** |
| `header_filter: { header: { name: "x-log", string_match: { safe_regex: { regex: "y.*" } } } }` | **OK** (no `google_re2` block needed in v1.33) |
| `header_filter: { header: { name: "x-log" } }` (name only, no matcher) | **OK** — means "present" (runtime-confirmed R-0.4) |
| `header_filter: { header: { name: ":path", string_match: { prefix: "/x" } } }` | **OK** (pseudo-header accepted) |
| `header_filter: { header: { name: "x-log", string_match: { exact: "yes" }, invert_match: true } }` | **OK** |
| `header_filter: {}` (no `header` key) | **REJECTED** — `HeaderFilterValidationError.Header: value is required` |

**MEASURED schema:** `HeaderFilter = { header: HeaderMatcher (required) }`. The
`HeaderMatcher` accepted-field set is: `name` (required, min-len 1); the mode
oneof — `string_match` (`StringMatcher`: `exact`/`prefix`/`suffix`/`contains`/
`safe_regex`), plus the DEPRECATED flat scalars `exact_match`/`prefix_match`/
`suffix_match`/`safe_regex_match` (accepted with a deprecation warning),
`present_match: bool`, `range_match`; the modifiers `invert_match: bool` and
`treat_missing_header_as_empty: bool`. **This is the SAME
`envoy.config.route.v3.HeaderMatcher` proto envoy-rust already models for
routes** — so the reuse in R-0.1 is exact for the mode set envoy-rust supports.

### R-0.3 — LIVE-ENVOY: `header_filter` is a mutually-exclusive `AccessLogFilter` oneof arm

Consistent with R-0.3 of phases 70/71: `status_code_filter`,
`response_flag_filter`, and `header_filter` are sibling arms of the
`AccessLogFilter` oneof — exactly one permitted. A `filter:` carrying two arms is
rejected at JSON→proto parse. This is the cardinality the `set_arms`
destructuring (R-0.1) now enforces across THREE arms (`> 1` →
`ConfigError::AmbiguousAccessLogFilter`).

### R-0.4 — LIVE-ENVOY (runtime, port-mapped, no backend, graceful-stop flush): `header_filter` keep/drop is deterministic + byte-exact

Booted live `envoyproxy/envoy:v1.33.0` (`docker -p`, memory
`state0-recon-docker-needs-port-mapping`): ONE HCM listener, a file access log
(`text_format_source` = `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)%
H=%REQ(X-LOG)%\n`) + `header_filter: { header: { name: "x-log", string_match:
{ exact: "yes" } } }`, and ONE `direct_response` route (`/x` → 200 body `hi`).
No cluster / no backend. Drove three requests; forced a full flush via
`docker stop` (SIGTERM drain, R-0.5) before reading:

| Request | `%REQ(X-LOG)%` | logged? |
|---|---|---|
| `GET /x` with `x-log: yes` (present + match) | `yes` | **YES** |
| `GET /x` with `x-log: no` (present + mismatch) | `no` | **NO** |
| `GET /x` with no `x-log` header (absent) | (empty) | **NO** |

The file contained EXACTLY ONE line: `STATUS=200 PATH=/x H=yes`. So the filter
gates on the request header: it KEEPS a record whose header matches, DROPS one
whose header is present-but-mismatched OR absent.

- **Name-only (`header: { name: "x-log" }`, no mode) runtime:** any present value
  KEPT, absent DROPPED → confirms name-only == presence match.
- **`invert_match: true` runtime (reproduced across two boots):** present+match →
  **DROPPED**; present+mismatch → **KEPT**; **absent → DROPPED (NOT kept)**.
  `invert_match` negates the result ONLY when the header is present; a MISSING
  header short-circuits to no-match/drop regardless of `invert_match` (Envoy's
  `HeaderUtility::matchHeader` missing-header behavior; `treat_missing_header_as_empty`,
  unset here, would change it). **This is a MEASURED divergence from the existing
  in-tree engine** — see PV-4 below.

### R-0.5 — LIVE-ENVOY: the FileAccessLog async-flush confound (same mechanism as CF-70-3 / CF-71-1)

Re-confirmed this session (identical to the phase-71 R-0.5 measurement):
`FileAccessLog` does NOT flush every record immediately — records batch and only
reach the file after a **~10-second** flush interval (line 1 near-instant, line 2
at t≈10s). A byte-exact harness that reads too soon sees a **false "suppressed"**
for a record that is merely un-flushed. Sound ways to force a complete flush:
wait past the ~10s interval, or a graceful `docker stop` (SIGTERM) drain (the
R-0.4 witness used graceful stop). This is EXACTLY the mechanism behind
**CF-71-1** (the ADR-0147 residual: an ordering-aware settle ≥ the flush interval
for dropped-LAST suppression fixtures). Phase 72, touching the differential
driver + the access-log-filter surface, is the designated owner (§2.1 item 6 /
PV-7).

### R-0.6 — the invert / name-only / treat-missing in-tree parity gaps (MEASURED in-tree)

The existing phase-04.2 `HeaderMatcher` (used today only by route matching)
diverges from the MEASURED upstream access-log semantics in three places. Each is
a load-bearing PV/scope decision:

1. **`invert_match` + ABSENT header.** The engine does `mode_result ^
   invert_match` UNCONDITIONALLY (`matcher.rs:51`), so absent + `ExactMatch` +
   `invert_match:true` = `false ^ true` = **true = KEEP**; and the existing unit
   test `invert_match_inverts_present_match_result` (`matcher.rs:373`) asserts
   `hm_inverted(PresentMatch(true)).matches(&[])` is **true**. But upstream
   drops absent+invert (R-0.4). Reusing the engine verbatim would therefore
   DIVERGE for the absent-under-invert case. (Whether upstream ROUTE matching
   also drops absent+invert — i.e. whether this is a latent phase-04.2 route bug
   too — is a PV-4 measurement.)
2. **Name-only `HeaderMatcher` (`{ name }`, no mode).** Upstream accepts it as a
   presence match (R-0.2/R-0.4). The existing deserializer REJECTS it —
   `mode.ok_or_else(|| … "missing mode key")` (`bootstrap.rs:3175`). So a config
   upstream accepts, envoy-rust rejects → a load-parity gap (PV-5).
3. **`treat_missing_header_as_empty: bool`.** Upstream accepts it (R-0.2). It is
   NOT in the deserializer's `ALL_KEYS` (`bootstrap.rs:3085`-`3095`), so the
   existing deserializer REJECTS it as an unknown field → a load-parity gap
   (PV-5). It is a rarely-used modifier and a pre-existing phase-04.2 boundary.

These are the "untested composition hides a divergence" traps (memory
`state5-must-probe-untested-compositions`): the opener fixture (R-0.7) uses a
NON-inverted `string_match: { exact }` — the rock-solid case that reuses the
engine verbatim — and PV-4/PV-5 decide how each gap is handled (scope out +
document as inherited phase-04.2 boundaries, model explicitly, or fix the shared
engine under an ADR).

### R-0.7 — numbering

Next ROADMAP id **72** (highest defined is `71`; `59`/`60`/`62` are intentional
gaps). Next fixture id **0078** (`0077` is the last). Next ADR **ADR-0148**
(ledger head `ADR-0147`; UNRESERVED — claimed by this pick).

---

## §1. Goal

Land the third `AccessLogFilter` oneof arm, `header_filter`
(`envoy.config.accesslog.v3.AccessLogFilter.header_filter`), behaviorally
equivalent to `envoyproxy/envoy:v1.33.0` under the differential contract (§7):

- An `AccessLog` entry MAY carry `filter: { header_filter: { header: {...} } }`;
  when present, a log record is emitted to that sink ONLY IF the named request
  header matches the `HeaderMatcher` (R-0.4). A non-matching (present-mismatch OR
  absent) header produces NO line for that sink. `header` is PGV-required — an
  empty `header_filter: {}` is fail-loud (`ConfigError`, R-0.2), matching
  upstream's `HeaderFilterValidationError.Header: value is required`.
- Reuse the existing phase-04.2 `HeaderMatcher` type + its `matches(&headers)`
  engine verbatim for the mode set envoy-rust supports (exact/prefix/suffix/
  safe_regex/range/present/string_match, with `ignore_case` and `invert_match`).
- `header_filter` is a mutually-exclusive oneof arm — exactly one of
  {`status_code_filter`, `response_flag_filter`, `header_filter`} permitted
  (R-0.3), enforced by the 3-arm `set_arms` destructuring; the
  `compile_access_log_filter` match extends from 2 to 3 arms.
- The ONLY new runtime plumbing is widening `should_log` a second time to see the
  request-header slice (R-0.1 / PV-3) and threading the already-in-scope
  `req.headers` / `envoy_req.headers` at both HCM emit gates.
- Consume **CF-71-1** (the ordering-aware settle hardening ≥ the ~10s flush
  interval for dropped-LAST suppression fixtures) + **M71-2** (the three stale
  "ordering witness" doc phrases), since this phase owns both the differential
  driver and the access-log-filter surface (R-0.5 / PV-7).

**Differential surface at phase end:** a new fixture `0078` witnessing
`header_filter { header: { name: "x-log", string_match: { exact: "yes" } } }`
byte-exact — an H1 HCM with a filtered file access log + one `direct_response`
route, driving `GET /x` WITH `x-log: yes` (kept) and WITHOUT it (dropped),
asserting the log file across both proxies is the SAME single byte-identical
`STATUS=200 PATH=/x H=yes` line — plus in-process coverage of the membership
semantics across modes, the absent-drop, the oneof cardinality (now 3 arms) +
the empty-`header` rejection, and the CF-71-1 suppression-robustness hardening.

---

## §2. Scope

### 2.1 In scope

1. **Config schema (`crates/envoy-config`).** Add
   `AccessLogFilter.header_filter: Option<HeaderFilter>` (the Option-arm
   precedent, `bootstrap.rs:722`). New type `HeaderFilter { header: HeaderMatcher }`
   under `#[serde(deny_unknown_fields)]`, REUSING the existing `HeaderMatcher`
   (`bootstrap.rs:3025`) verbatim. `header` is required — a `header_filter` with
   no `header` key is fail-loud (R-0.2; PV-1 fixes the exact `ConfigError`).
2. **Validation (`crates/envoy-config`).** (a) Extend the `set_arms`
   destructuring (`bootstrap.rs:5148`) to THREE arms — one more field in the
   destructure + one more `.is_some()` — the compiler forces it; the `> 1` and
   `== 0` branches stay `AmbiguousAccessLogFilter { detail }`. (b) The
   `header_filter` arm's sibling validation: reject the empty `header` (PV-1) and
   DELEGATE to the existing `HeaderMatcher` validators (empty name →
   `EmptyHeaderName`; unparseable regex → `InvalidRegex`) so `header_filter`
   inherits full validation parity for free. (c) **M71-1 fold** (cheap, this
   phase touches `validate_access_logs`): make the both-arm cardinality test
   assert `detail` so it distinguishes the zero-arm from the multi-arm branch,
   and pin the cardinality-before-per-arm-check precedence. (d) **M71-4 fold:**
   refresh the `validate_access_logs` validator docstring (it predates phase 71,
   omits `UnknownResponseFlag`, and still calls the `> 1` branch unreachable).
3. **Compile + runtime predicate (`crates/envoy-accesslog` + HCM).** (a) Extend
   `compile_access_log_filter`'s 2-arm tuple match (`hcm.rs:1742`) to 3 arms,
   adding `(None, None, Some(hf)) => LogFilter::Header { matcher: hf.header.clone() }`.
   (b) Add a `LogFilter::Header { matcher: HeaderMatcher }` variant
   (`filter.rs`). (c) **Widen `should_log`** (`filter.rs:38`) so it ALSO receives
   the request-header slice (PV-3 fixes the exact signature — pass
   `headers: &[(String, String)]` alongside `(status, response_flags)`), and
   implement the `Header` arm as `matcher.matches(headers)`. `StatusCode` and
   `ResponseFlag` behavior is byte-unchanged. Update the `FileSink::should_log`
   wrapper (`file_sink.rs:102`) + every in-crate call site/test in lockstep. (d)
   Thread the widened call at the per-sink emit gate in BOTH HCM emit loops (H1
   `hcm.rs:1509`, H2 `crates/envoy-http2/src/hcm.rs:1132`, PV-3) — the request
   headers already in scope (`req.headers` / `envoy_req.headers`), no new
   derivation.
4. **Differential fixture `0078-accesslog-header-filter`.** An H1 HCM listener
   with a file access log (deterministic `text_format_source`, the `0076`/`0077`
   discipline) + `header_filter: { header: { name: "x-log", string_match:
   { exact: "yes" } } }` + one `direct_response` route (`/x` → 200 `hi`). Two
   probes: `GET /x` with `extra_headers: [["x-log","no"]]` (`expect_logged:
   false`, DROPPED) FIRST, then `GET /x` with `extra_headers: [["x-log","yes"]]`
   (`expect_logged: true`, KEPT) LAST — **kept-LAST ordering** per the ADR-0147
   authoring convention (the dropped probe precedes the kept one, so the kept
   record's flush proves the dropped one would have flushed by then). Assert the
   log file across both proxies is the SAME single byte-identical
   `STATUS=200 PATH=/x H=yes` line, via the EXISTING `Http1AccessLogByteExact`
   driver (no harness change for the positive witness).
5. **In-process coverage.** The `should_log` `Header` membership across modes (a
   record with `x-log: yes` matches `exact:"yes"`, `prefix:"ye"`, `present`,
   `safe_regex:"y.*"`; misses `exact:"no"`; an ABSENT `x-log` misses every
   non-`present:false` mode → DROPPED); the oneof cardinality (zero-arm →
   `AmbiguousAccessLogFilter`, any two-arm pair → `AmbiguousAccessLogFilter`,
   with `detail` asserted per M71-1); the empty-`header` rejection; the empty
   header name / bad regex delegated rejections; a no-`filter` sink still logging
   every record (regression); `status_code_filter` / `response_flag_filter`
   unchanged (regression). The invert/name-only/treat-missing gaps are pinned per
   the PV-4/PV-5 decisions.
6. **CF-71-1 closure + M71-2 doc fixes (differential driver + docs).** Land the
   ordering-aware settle hardening the ADR-0147 residual named (PV-7: for a
   dropped-LAST suppression fixture, settle ≥ the ~10s flush interval before
   asserting no extra line; the `0078` opener orders kept-LAST so it is the
   already-sound case). Fix the three stale "ordering witness" doc phrases M71-2
   named: the `CF70_3_SETTLE` doc comment (`tests/differential/src/lib.rs:1677`-`1680`),
   the `0077` integration-test doc comment, and the `BEHAVIOR_CONTRACT.md` §F
   "as the CF-70-3 ordering witness" phrase. Keep the change surgical — do NOT
   disturb the 31 existing access-log fixtures (incl. `0076`/`0077`).
7. **`BEHAVIOR_CONTRACT.md`** — a `header_filter` subsection under the access-log
   filter section (sibling to the phase-70 `status_code_filter` §D and phase-71
   `response_flag_filter` §E/§F subsections).
8. **`known-failures.txt` / conformance** — unchanged (no protocol-conformance
   surface; never trimmed, memory `h2spec-3-5-2-preface-host-sensitive`).

### 2.2 Out of scope (deliberate, with rationale)

- **The remaining `AccessLogFilter` variants** — `duration_filter`,
  `not_health_check_filter`, `and_filter`, `or_filter`, `grpc_status_filter`,
  `runtime_filter`, `metadata_filter`, `traceable_filter`, `log_type_filter`.
  Each is a future leaf reusing this phase's now-3-arm oneof (§10 notes the next
  picks). `and_filter`/`or_filter` become especially natural now that 3 leaf arms
  exist (§4).
- **`invert_match` + ABSENT parity, and name-only / `treat_missing_header_as_empty`
  parity (R-0.6).** The opener fixture and the guaranteed in-process coverage use
  the NON-inverted, explicit-mode cases the existing engine reproduces verbatim.
  PV-4/PV-5 decide each gap; the DEFAULT disposition is to document them as
  INHERITED phase-04.2 boundaries (a route `HeaderMatcher` today has the same
  behavior) and NOT expand phase 72 into fixing route matching — unless PV-4
  measures the invert-absent case as cheaply fixable in the shared engine with no
  route-differential regression. A differential fixture that exercises invert or
  name-only is deferred until its parity is resolved.
- **The H2 `header_filter` differential fixture** — the widened `should_log` gate
  is codec-agnostic (both gates pass their in-scope header slice), so H2 is
  inert-correct and wired (PV-3, no regression); a dedicated H2 filtered fixture
  is deferred (the opener is H1, the simplest driver) — the same disposition
  phases 70/71 took. (Does NOT discharge the deferred M71-6 H2 differential.)
- **Producing/consuming request headers the record does not carry** — no new
  `AccessLogRecord` field; `header_filter` reads the raw request-header slice at
  the gate (R-0.1), it does not add derived record fields.

### 2.3 §7.4 fuzz disposition

The `filter` surface reuses the `parse_bootstrap` parser (a new sub-message field
+ the existing `HeaderMatcher` deserialize path, already fuzz-reachable via route
configs). **Default projection:** extend the existing `parse_bootstrap` corpus
seed to carry an `access_log[].filter.header_filter.header` (exercising the new
sub-message) — **no new fuzz target** (the phase-68/69/70/71 precedent, ADR-0137:
a config-only sub-message rides the existing `parse_bootstrap` target). There is
NO new byte-parser this phase. **Confirm at the state-2 PLAN-write** (PV-6). A
new/edited seed must be `!`-un-ignored (memory
`fuzz-corpus-seed-gitignored-by-default`).

---

## §3. PLAN-VERIFY items (re-confirm against the live tree at the state-2 PLAN-write)

- **PV-1 — the `HeaderFilter` serde model + empty-`header` rejection.** Confirm
  `HeaderFilter { header: HeaderMatcher }` reuses the existing `HeaderMatcher`
  verbatim (the deserializer at `bootstrap.rs:3068` accepts the flat + nested
  forms). Author the empty-`header` (`header_filter: {}`) rejection — decide a new
  `ConfigError::MissingHeaderFilterMatcher` vs. reusing an existing variant
  (native message OK per ADR-0049). Confirm where in `validate_access_logs` the
  per-arm validation attaches (sibling to the `runtime_key`/`response_flag`
  checks at `bootstrap.rs:5165`/`5172`).
- **PV-2 — the 3-arm `set_arms` destructuring + M71-1/M71-4 folds.** Extend the
  destructuring (`bootstrap.rs:5148`) to three arms (compiler-forced). Make
  `rejects_access_log_filter_with_both_arms` assert `detail` (M71-1) and pin the
  cardinality-before-per-arm-check precedence (a both-arms + bad-`HeaderMatcher`
  config must fail on cardinality first). Refresh the validator docstring (M71-4).
- **PV-3 — widen `should_log` (2nd time) + thread both HCM emit loops.** Fix the
  exact widened signature — add `headers: &[(String, String)]` to
  `should_log(&self, status, response_flags)` and the `FileSink::should_log`
  wrapper; update every in-crate call site + test. Extend
  `compile_access_log_filter` (`hcm.rs:1742`) to 3 arms. Thread the in-scope
  `req.headers` (H1 `hcm.rs:1509`) and `envoy_req.headers` (H2
  `crates/envoy-http2/src/hcm.rs:1132`) into the widened call. **MEASURE which
  header snapshot the gate sees** — the DOWNSTREAM request headers as received
  (confirm route-level `request_headers_to_*` mutations, if any apply on the
  fixture path, do not perturb the `x-log` header the filter reads; upstream's
  `header_filter` matches the request headers, and the opener uses a header the
  route does not touch, so this should be inert — confirm on both proxies).
- **PV-4 — the `invert_match` + ABSENT divergence (HEADLINE).** The shared engine
  does `mode_result ^ invert_match` unconditionally (`matcher.rs:51`), so
  absent+invert = KEEP, but upstream access-log = DROP (R-0.4/R-0.6). MEASURE
  whether upstream ROUTE matching ALSO drops absent+invert (to know if the shared
  engine is a latent route bug too). Then DECIDE: (a) scope invert out of phase
  72's differential surface + document the inherited boundary (default, cheapest);
  (b) model `header_filter`'s absent-under-invert explicitly in the access-log
  path only; or (c) fix the shared engine under an ADR IF (a-b) prove insufficient
  and the fix regresses no route differential. Pin the decision + a test.
- **PV-5 — name-only + `treat_missing_header_as_empty` load-parity (R-0.6).**
  Upstream accepts `header: { name: "x-log" }` (presence) and
  `treat_missing_header_as_empty`; the existing deserializer REJECTS both
  (`bootstrap.rs:3175`/`3169`). DECIDE per parity: accept name-only →
  `PresentMatch(true)` for the access-log path (a small, local decision) vs.
  inherit the phase-04.2 boundary (document). For `treat_missing_header_as_empty`
  — default to documenting the inherited boundary (rarely used; a route
  `HeaderMatcher` rejects it today), unless cheaply accepted-and-inert. Load
  parity forbids SILENTLY diverging; the resolution must be an explicit, tested
  decision.
- **PV-6 — §6.1 size re-derivation + §7.4.** Re-estimate net LoC / task count
  against the live tree (§8). A SMALL leaf (~500-700 LoC) — a split is very
  unlikely; PV-6 re-derives (ADR-0150 held in reserve as a formality). Confirm
  the §7.4 disposition (corpus seed edit, no new target).
- **PV-7 — the CF-71-1 sound closure mechanism + M71-2.** Land the ordering-aware
  settle (settle ≥ the ~10s flush interval before asserting no extra line for a
  dropped-LAST suppression fixture). The `0078` opener orders kept-LAST (the
  already-sound case), so the hardening is a driver capability, not a fixture
  requirement. Fix the three M71-2 doc phrases. Keep it surgical — do NOT disturb
  the 31 existing access-log fixtures.

---

## §4. Rejected / deferred alternatives (what this pick was chosen over)

- **`and_filter` / `or_filter`** (the recursive composition arms). MEASURED wire
  shape (R-0.2 recon): `{ filters: repeated AccessLogFilter }` with PGV
  `min_items = 2` on BOTH — an empty or single-element `filters` is REJECTED
  (`AndFilterValidationError.Filters: value must contain at least 2 item(s)`); a
  nested `header_filter`/`status_code_filter`/`response_flag_filter` inside
  `filters` is accepted. They compose EXISTING predicates with NO new request-data
  plumbing (a `LogFilter::And(Vec<LogFilter>)` whose `should_log` calls
  `.all(...)`), and would be the cheapest arm in signature terms IF the leaf set
  stayed status+flags-only. BUT they add a **recursive** config type (nested
  `Vec<AccessLogFilter>` deserialize + validate + compile) for a composition of
  already-covered predicates, whereas `header_filter` lights up a genuinely NEW
  observable axis (request-header membership) at near-zero new logic (the whole
  matching engine is reused). A natural NEXT leaf once ≥3 leaf arms exist (this
  phase makes it 3) — deferred, not rejected on merit.
- **`duration_filter`** — timing-based (`op(request_duration_ms, threshold)`);
  the differential needs latency bounds and is flaky; deferred to a
  timing-tolerant phase (the same rationale that deferred it at the phase-70/71
  picks).
- **The standalone H2 access-log-filter differential** (the M71-6 / ADR-0140
  §2.2 deferral) — the gate is codec-agnostic and already measured
  byte-identical, so as a standalone phase it is weak; better folded into a
  future H2 access-log-filter fixture set.
- **Re-weighed and still rejected** (re-verified against the live tree): each §9
  family opener (network-filter payload codecs, `sni_cluster`, non-deterministic
  LB, HTTP/3+QUIC, gRPC bridge/transcoding, observability SINKS [gRPC ALS, OTLP],
  runtime/RTDS, hot-restart, WASM host) — each a LARGE new subsystem far above the
  cheapest-strong-differential bar.

**`header_filter` wins:** it is the cheapest arm-#3 leaf — it reuses the ENTIRE
phase-04.2 `HeaderMatcher` type + its 7-mode matching engine (zero new matching
logic), reads request headers already in scope at both emit gates (no new record
field, no new capture plumbing), and drives its fixture with the driver's
existing `extra_headers` + `expect_logged` fields (pure YAML). It yields a fully
deterministic BACKEND-FREE byte-exact single-line observable on a NEW axis
(request-header membership), and is the natural owner to consume CF-71-1 + M71-2,
introducing exactly one new runtime seam (the second `should_log` widening) —
the same class of change phase 71 already performed once.

---

## §5. Differential surface at phase end

- **NEW fixture `0078-accesslog-header-filter`** — green cross-proxy: an H1 HCM
  listener with a file access log (deterministic `text_format_source`
  `STATUS=%RESPONSE_CODE% PATH=%REQ(:PATH)% H=%REQ(X-LOG)%\n`) + `header_filter
  { header: { name: "x-log", string_match: { exact: "yes" } } }` + one
  `direct_response` route (`/x` → 200 `hi`). Two probes ordered kept-LAST:
  `GET /x` with `x-log: no` (200, `expect_logged: false`, DROPPED) then `GET /x`
  with `x-log: yes` (200, `expect_logged: true`, KEPT). The access-log file
  across BOTH proxies is asserted the SAME single **byte-identical** line
  `STATUS=200 PATH=/x H=yes` (the mismatch DROPPED), via the EXISTING
  `Http1AccessLogByteExact` driver + the CF-71-1 suppression hardening.
- **All pre-existing fixtures `0001`–`0077` stay green** — a sink with no
  `filter` behaves exactly as today; `status_code_filter` (`0076`) and
  `response_flag_filter` (`0077`) are byte-unchanged; no existing fixture sets a
  `header_filter` (§7.5 (b)).
- **In-process:** the `should_log` `Header` membership over the supported modes +
  the absent-drop + the oneof cardinality (zero-arm + all two-arm pairs, `detail`
  asserted) + the empty-`header` and delegated (empty-name / bad-regex)
  rejections; the no-`filter`-still-logs regression; the `status_code_filter` /
  `response_flag_filter` unchanged regressions; the PV-4/PV-5 invert/name-only
  decisions pinned.

**Why the differential needs no backend:** the strong deterministic byte-exact
observable comes from a `direct_response` 200 whose emission is gated purely on a
client-supplied request header — no cluster, no upstream (R-0.4), mirroring
fixtures `0076`/`0077`.

---

## §6. `BEHAVIOR_CONTRACT.md` additions

A `header_filter` subsection under the access-log filter section (sibling to the
phase-70 `status_code_filter` and phase-71 `response_flag_filter` subsections),
recording the MEASURED facts (R-0.2–R-0.6): `filter: { header_filter: { header:
<HeaderMatcher> } }` gates emission per sink; a record is KEPT iff the named
request header matches the `HeaderMatcher` (present-mismatch AND absent both
DROP); `header` is PGV-required (empty `header_filter: {}` fail-loud); the
supported mode set reuses the phase-04.2 `HeaderMatcher` verbatim;
`header_filter`, `status_code_filter`, `response_flag_filter` are
mutually-exclusive oneof arms; the `invert_match` absent-case and name-only /
`treat_missing_header_as_empty` boundaries per the PV-4/PV-5 decisions; a
`GET /x` with `x-log: yes` renders `H=yes` (kept), a mismatch/absent is dropped;
a sink with no `filter` logs every record (unchanged). Also correct the M71-2 §F
"ordering witness" phrasing left stale by ADR-0146.

---

## §7. ADR reservations

- **ADR-0148 (FIRED this session):** the phase-72 pick + scope + rejected
  alternatives (this SPEC's decisions).
- **ADR-0149 (reserved):** the §6.2 empirical-verification reconciliation at the
  state-2 PLAN-write (PV-1..PV-7 resolutions — the `HeaderFilter` serde model +
  empty-`header` rejection, the 3-arm destructuring + M71-1/M71-4, the 2nd
  `should_log` widening + the 3-arm compile match + both HCM emit loops, the
  invert-absent divergence decision, the name-only / treat-missing parity
  decision, the CF-71-1 sound closure).
- **ADR-0150 (reserved):** the §6.1 split, if PV-6 fires it (very unlikely — a
  small single-phase arm-#3 leaf).

---

## §8. Estimated size (for the §6.1 split gate at state-2)

| Area | Net LoC (rough) |
|---|---|
| `envoy-config`: `HeaderFilter` type + the `header_filter` oneof arm (reuses `HeaderMatcher`) | ~50 |
| `envoy-config`: empty-`header` rejection + 3-arm `set_arms` destructuring + M71-1/M71-4 folds | ~60 |
| `envoy-accesslog` + HCM: `LogFilter::Header` + the 2nd `should_log` widening (+ `FileSink` wrapper + call sites) + the 3-arm compile match + both emit-loop threads | ~110 |
| fixture `0078` (2 YAMLs + expectations + README) — reuses the driver | ~120 |
| differential driver CF-71-1 ordering-aware settle + M71-2 doc fixes | ~50 |
| in-process tests (membership across modes + absent-drop + cardinality/detail + empty-header + delegated rejections + invert/name-only PV pins + regressions) | ~190 |
| `BEHAVIOR_CONTRACT.md` + ROADMAP/docs | ~60 |
| **Total** | **~640 net LoC / ~11–14 tasks** |

Well UNDER the ~1500 LoC / ~25 task gate — a **single phase**, no split projected
(comparable to phase 70 ~670 / phase 71 ~630; the `HeaderMatcher` type + engine
already exist, the driver `extra_headers` + `expect_logged` fields already exist,
and there is no new codec/primitive — only a config sub-message reusing an
existing matcher, a predicate variant, a signature widening, and one bounded
driver hardening). PV-6 re-derives at the state-2 PLAN-write; ADR-0150 is held in
reserve as a formality only.

---

## §10. Carry-forwards

**CONSUMED by this pick** (this phase touches both the differential driver and the
access-log-filter surface — the designated owner): **CF-71-1** (the
ordering-aware settle hardening ≥ the ~10s flush interval for dropped-LAST
suppression fixtures — §2.1 item 6 / PV-7) + **M71-2** (the three stale "ordering
witness" doc phrases). **CHEAPLY FOLDED** (this phase touches
`validate_access_logs`): **M71-1** (assert `detail` on the cardinality test +
pin the cardinality-before-per-arm precedence) + **M71-4** (refresh the validator
docstring).

**NOT consumed** (owner = whatever future phase touches their surface):

- **M71-3** (the all-suppressed `expected_logged_count == 0` shape untested) —
  driver-adjacent; weigh at state-2 whether it is cheaply folded into the CF-71-1
  hardening, else carry forward.
- **M71-5** (no test pins the multi-sink mixed-filter composition) — this phase
  adds a 3rd arm; weigh at state-2 whether a multi-sink mixed-arm in-process test
  is cheap to add, else carry forward.
- **M71-6** (the H2 access-log-filter differential remains deferred) — NOT
  discharged here (§2.2). **M71-7** (latent multi-flag membership; response-flag
  specific) + **M71-8** (`""`/whitespace token rejection unpinned; response-flag
  validator) — NOT touched here.
- **M70-R2 is DISCHARGED** (phase 71); **M70-R4** (`"filter": null`
  serialization), **M70-R9** (provenance note) — access-log-adjacent; weigh at
  state-2, else carry forward.
- **M69-A..I** (gRPC-HC doc/coverage), **CF-69-1/2/3/5**, **M68-1** (empty-hex
  TCP-HC validator gap), **M-1** (`CidrRange` `prefix_match` guard band),
  **CF-67-3/5/6/7** (incl. the TLS `[rbac, tcp_proxy]` establishment ordering) +
  the older Minors in `67.3/SPEC.md` §10 + the HTTP-filters-family (1)–(4) in
  `STATE.md` `## Notes`. **Not touched here.**

**Newly OPENED by this SPEC** (to be resolved at state-2 / carried as their own
carry-forwards if deferred): the **PV-4 `invert_match` + absent divergence**
(shared-engine vs. upstream access-log) and the **PV-5 name-only /
`treat_missing_header_as_empty` load-parity gaps** (R-0.6) — each an explicit,
tested decision, defaulting to a documented INHERITED phase-04.2 boundary unless
cheaply resolvable.

**Natural next cheapest-strong leaves this phase UNLOCKS** (each reuses the
now-3-arm `filter` oneof, none an obligation): `and_filter` / `or_filter`
(recursive composition — natural now that 3 leaf arms exist), `duration_filter`
(timing), the H2 access-log-filter differential (M71-6), and the remaining
predicate arms.
