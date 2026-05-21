# Phase 11 (`11-http-filter-fault`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x /
> 08.x / 09 / 10 cadence. State-2 PLAN-write lands this skeleton + the Task 1 preamble;
> state-3 dispatch appends `### Task N — <name>` subsections in execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/11-http-filter-fault/PLAN.md` (the state-2 PLAN.md per
  `BOOTSTRAP_PROMPT.md` §5 state 2; 8 tasks; full `- [ ]` checkbox steps per task per the
  project's mature TDD cadence).
- **CREATE** `docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `11` `status: planned` →
  `status: in-progress`. Earlier rows unchanged.
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last
  commit; Last updated; new `Phase-11 state-2 PLAN-write` subsection in Notes.

**Predecessor commit:** `1370aaa` — `phase 11: state-1 brainstorm — http-filter-fault SPEC.md
(HTTP-filter-family third phase; 09 REVIEW M2 H2-decoration impl close site)` (the phase-11
state-1 brainstorm commit; immediate prologue).

**SPEC commit base:** `1370aaa` (the state-1 brainstorm commit). **This state-2 commit makes
NO inline SPEC.md edits** — all 3 §6.2 empirical-verification projections matched (see Task 1
preamble); no ADR triggered.

**ROADMAP status before this commit:** row `11` `planned` (added at state-1).
**ROADMAP status after this commit:** row `11` `in-progress`.

**STATE.md "Active phase" status before:** `phase 11 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist)`.
**STATE.md "Active phase" status after:** `phase 11 lifecycle state 2-complete / state-3-next (PLAN.md + PROGRESS.md skeleton + Task 1 preamble landed; first task implementation pending)`.

**DECISIONS.md status before AND after:** **ADR-0034** (phase-10 SPEC §2.2 + §3 D8.1 + §5.9
body-bytes revision). **No ADR lands at this state-2 commit** — the §6.2 verification surfaced
no revision (all 3 projections matched; see Task 1 preamble finding (a)/(b)/(c)). The 4
conditional ADR-0035 slots (A empirical-verification revision; B fractional-deferral
durability; C foundations grant; D `Driver::Http2ProbeList` durability) ALL DEFER per the SPEC
§7 recommended posture. Next available number stays **ADR-0035**.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged. The 1 stat-name mapping row under
the new `**11 entries (Fault filter):**` header lands at Task 2 commit per PLAN lock-in #27
(SPEC §6.6 cadence — contract extensions land at empirical-engagement task time, NOT at
PLAN-write time).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **8 tasks** per PLAN §4. The SPEC §6.1 ~10-12 projection collapses to 8 via the recommended
  deliverable co-locations (D2+D3 with D1 at Task 1; D7+D7.1 with D4 at Task 2). Subagent-driven
  execution at state 3 per PLAN lock-in #39 + `feedback_execution_style`.
- **~1520 LoC projected** per PLAN §3 (production ~480, tests ~745, fixture/harness/doc ~295).
  Marginally at SPEC §6.1's ~1500-LoC soft gate (+1.3%); accept the projection per the soft
  gate — same posture as phase-10's ~1525 single-phase landing (lock-in #38).
- **Single-phase; no nest-split** per PLAN lock-in #38 + parent-08 SPEC §6.1 alternative (vi)
  accept-drift discipline. **Release valve:** D8.1 option (b) trim (single-probe `Driver::Http2`
  + backstop pass-through) if state-3 drifts past ~1600 LoC.
- **ZERO ADR landings at state-2** (all 3 §6.2 projections matched; SPEC correction #7).

---

## Task 1 preamble

### SPEC §6.2 empirical-verification findings (3 — performed at PLAN-write per the phase-10-ratified verify-at-PLAN-write doctrine)

Per SPEC §6.2 (the phase-10-ratified process improvement + the ADR-0033 process-gap-awareness
doctrine), the PLAN-writer performed all 3 verifications at THIS state-2 commit against
`envoyproxy/envoy:v1.33.0` Docker on an **HTTP/2** listener, using the SPEC §3 D8.1 canonical
bootstrap (HCM `codec_type: HTTP2` + `envoy.filters.http.fault` abort 503 @ 100% gated by
`x-fault: abort` + `envoy.filters.http.router` + `direct_response` 200).

**Verification methodology:** wrote the canonical bootstrap to `/tmp/p11verify/envoy.yaml`;
ran `docker run -d -p 10000:10000 -p 9901:9901 -v ... envoyproxy/envoy:v1.33.0 -c
/etc/envoy/envoy.yaml -l info`; waited for admin `/ready`; drove probes with
`curl --http2-prior-knowledge -D - ...`:
- **Probe 1** (`-H 'x-fault: abort'`): gated-in → 503 abort.
- **Probe 2** (no header): gate miss → 200 pass-through (`direct_response`).
- **Counter bump:** 3 additional abort+pass probe pairs, then scraped admin `/stats`.

**Finding (a) — Stats namespace shape:** MATCHES SPEC §2.1 projection exactly.

Empirical scrape (after 4 abort probes + 4 pass probes):
```
http.ingress_http.fault.aborts_injected: 4
http.ingress_http.fault.active_faults: 0
http.ingress_http.fault.delays_injected: 0
http.ingress_http.fault.faults_overflow: 0
http.ingress_http.fault.response_rl_injected: 0
```

SPEC §2.1 + §6.5 project `http.<hcm_stat_prefix>.fault.aborts_injected` (with `hcm_stat_prefix
= ingress_http` → `http.ingress_http.fault.aborts_injected`). Value-exact: 4 aborts → counter
4. Upstream additionally emits 4 sibling fault counters (`active_faults` gauge,
`delays_injected`, `faults_overflow`, `response_rl_injected`) at 0 — these are the
delay/response-rate-limit/concurrency surfaces that **defer per SPEC §4**; phase-11 registers
only `aborts_injected`. The differential fixture does NOT scrape fault stats (it asserts only
the 4-probe status sequence + body + header set), so the 1-vs-5 stat-name-set divergence is not
exercised bilaterally. **No SPEC revision needed; no ADR triggered by this finding.**

**Finding (b) — Abort response body bytes:** MATCHES SPEC §2.2 projection exactly (NO off-by-one).

Empirical abort-response body (hex-dumped from `curl -o`):
```
content-length: 18
body bytes (hex): 66 61 75 6c 74 20 66 69 6c 74 65 72 20 61 62 6f 72 74
body string:      "fault filter abort"  (18 bytes)
```

SPEC §2.2 projects `"fault filter abort"`. Reality is exactly `"fault filter abort"` (18
bytes). **MATCHES — no projection error.** This is notable because the phase-10 RBAC body
projection was off by 1 byte (the SPEC §6.2 + §7 signpost explicitly warned this verification
was "the most likely ADR-0035 trigger; do NOT assume"). The empirical run confirms the fault
abort body projection is correct. **No SPEC revision needed; no ADR triggered.** The
production-code body shape is `Bytes::from_static(b"fault filter abort")` (PLAN lock-in #16).

**Finding (c) — H2 abort response header set:** MATCHES SPEC §2.2 + D6 projection exactly.

Empirical H2 abort response (`curl --http2-prior-knowledge -D -`):
```
HTTP/2 503
content-length: 18
content-type: text/plain
date: Thu, 21 May 2026 13:08:50 GMT
server: envoy
```

The 503 abort response carries exactly 4 standard headers `{server, content-length,
content-type, date}` — **NO `connection`** (an H2-forbidden hop-by-hop header). This matches
the SPEC §2.2 + D6 recommended projection `{server, date, content-length, content-type}`
exactly. (Upstream emits `server: envoy`; envoy-rust emits `server: envoy-rust` — covered by
the 04.1-landed BEHAVIOR_CONTRACT `server` allow-list row. `content-type: text/plain` matches
envoy-rust's `DEFAULT_CONTENT_TYPE`.) This confirms the D6 `decorate_filter_synth_response_h2`
target set: add `content-length` always + `server`/`date`/`content-type` only-if-missing; NO
`connection`. The pass-through 200 carried `{content-length: 3, content-type, date, server}`
+ body `"ok\n"` (3 bytes) — confirming the direct_response pass-through arm. **No SPEC revision
needed; no ADR triggered.**

**§6.2 disposition:** all 3 projections MATCH. **No inline ADR-0035; no SPEC.md edit at this
state-2 commit.** DECISIONS.md ledger head stays at ADR-0034. This is the recommended posture
per SPEC §7 option A ("verify all 3; land ADR only if any differ" — none differ).

### PLAN-write SPEC corrections (7 — full text in PLAN §1)

1. **`error.rs:51` is NOT a reject list** — it is a test fixture string in
   `display_router_not_terminal_includes_position_and_name`. The fault filter is rejected today
   by serde (`HttpFilterTypedConfig` `#[serde(tag = "@type", deny_unknown_fields)]` → unknown
   variant). **D5 does NOT touch `error.rs`;** adding the `Fault` variant (D1) + the validator
   dispatch arm (D2) is the entire "rejected → supported" move. The `error.rs:57` test stays
   verbatim (still a valid `RouterNotTerminal` Display test).
2. **`ConfigError` lives in `crates/envoy-config/src/lib.rs`** (not `bootstrap.rs`); the 3 new
   fault variants land there. The schema + validator land in `bootstrap.rs`.
3. **`HeaderMatcher::matches(&[(String, String)])`** RE-CONFIRMED at `matcher.rs:19`
   (HEAD `1370aaa`) — matches `FilterRequest::headers: Vec<(String, String)>` directly.
4. **The 3-arg `build_from_config`/`build` (`filters, registry, hcm_stat_prefix`) already
   exists** (`pipeline.rs:40` + `instance.rs:73`, threaded at phase 10) — **phase 11 widens NO
   signature** (unlike phase 10, which widened the H1 HCM call site).
5. **`FilterResponse { status: u16, reason: Option<&'static str>, headers: Vec<(String,
   String)>, body: Bytes }`** RE-CONFIRMED at `types.rs:43`; abort response uses `reason: None`.
6. **H2 writer-arm sites RE-CONFIRMED:** decode-side `SynthFromDecode(r)` at `hcm.rs:373`
   (constructed at `:176-182`); encode-side `StopAndSend(replacement)` at `:436`;
   `build_http_response` at `response.rs:29` strips `H2_FORBIDDEN_HOP_BY_HOP` (contains
   `"connection"` per `lib.rs:34`).
7. **All 3 §6.2 projections MATCH** (finding (a)/(b)/(c) above) — no inline ADR-0035, no SPEC
   edit, ledger stays at ADR-0034.

### Architecture-decision lock-ins (42 — full table in PLAN §2)

Grouped summary:
- **Schema (lock-ins #3-#10):** `FaultConfig`/`FaultAbort`/`FractionalPercent`/`DenominatorType`
  + `default_denominator` + `DenominatorType::value()` + `FractionalPercent::selects_deterministic()`
  + the `Fault(FaultConfig)` `HttpFilterTypedConfig` variant. `FractionalPercent` +
  `DenominatorType` are the only genuinely-new *shared* config types (reusable by future filters).
- **Validator (lock-ins #11-#13):** 3 `ConfigError` variants (`InvalidFaultAbortStatus`,
  `FaultPercentageOutOfRange`, `UnsupportedFractionalFaultPercentage`) + `validate_fault_config`
  (out-of-range check BEFORE fractional check) + the dispatch arm mirroring Rbac.
- **Filter runtime (lock-ins #14-#19):** `FaultFilter { abort_status, abort_selects,
  header_gate, aborts_injected }` + 3-arg `build_from_config` + gate-then-select decode +
  no-op encode + `header_gate_matches` (AND semantics; empty ⇒ all) + `FAULT_ABORT_BODY`
  (`b"fault filter abort"`, 18 bytes, §6.2-verified).
- **Framework integration (lock-ins #20-#21):** `HttpFilterInstance::Fault(FaultFilter)` variant
  + build/decode/encode dispatch arms (mirror Rbac). No `error.rs` edit (correction #1).
- **D6 H2 decoration (lock-ins #22-#25):** `decorate_filter_synth_response_h2` (content-length
  always + server/date/content-type only-if-missing; NO connection, NO `close` param) + 2
  wirings (`hcm.rs:373` + `:436`) + 2 unit tests. **Closes 09 REVIEW M2 implementation arm.**
- **Stats + contract (lock-ins #26-#27):** 1 counter `http.{hcm_stat_prefix}.fault.aborts_injected`
  + 1 BEHAVIOR_CONTRACT row at Task 2.
- **Fixture + harness + fuzz + backstop (lock-ins #28-#35):** fixture 0018 (H2) +
  `Driver::Http2ProbeList { probes: Vec<Http1Probe> }` (recommended option (a)) + Docker wrapper
  + fuzz seed + H1 in-process backstop with 503-probe header assertion (heeds 10 M1).
- **Process (lock-ins #36-#42):** 09 M2 close at Task 4; NO ADR (all 4 §7 options defer);
  single-phase split-gate verdict; subagent-driven execution; PROGRESS skeleton + Task 1
  preamble at state-2; empty Cargo.lock diff; `#![forbid(unsafe_code)]` inherited.

### Carryforward dispositions (phase-11-relevant)

| Carryforward | Disposition in phase 11 |
|---|---|
| **09 REVIEW M2** (H2 HCM filter-synth decoration gap — implementation arm) | **CLOSES at D6 (Task 4).** The documentation arm closed at phase-10 D5 (ADR-0033 Consequences amendment, `DECISIONS.md:699`). D6 lands `decorate_filter_synth_response_h2` + 2 wirings. **After Task 4, the 09 → 10 → 11 M2 chain ENDS.** No new ADR (SPEC §2.3). |
| **10 REVIEW M1** (Task 7 backstop omitted 5-header presence assertion without disclosure) | **Heeded proactively at D8.3 (Task 7)** per SPEC §6.4 option (a) — the fault backstop INCLUDES the per-probe standard-header presence assertion on the 503 probes (lock-in #34). |
| **10 REVIEW M2** (`hcm.rs:~187` H1 call-site doc-comment polish) | Phase 11 widens NO H1 HCM signature (correction #4) — the H1 call site is not touched. Carries forward; close opportunistically (not engaged). |
| **10 REVIEW M3** (Permission/Principal hand-rolled Deserialize — documented decision) | Not engaged (no RBAC tree). Carries forward. |
| **10 REVIEW M4** (RbacFilter test `use`-block dup) | Not engaged. Carries forward. |
| **10 REVIEW D1/D2 + T1** | Carry forward; not engaged by fault. |
| **09 REVIEW M1** (token-bucket CAS race) + **09 D1/D2/T1/T2/T3** | Not engaged (fault has no token bucket). Carry forward. |
| **08.2 / 08.1 / 07.2 / 06.x / 05.x / 04.1 / 02.2 / 00 carryforwards** | Carry forward indefinitely unless coincidentally engaged (none engaged by the abort-only fault surface). |

---

## State-3 execution log

_(Appended per-task by the state-3 subagent dispatch. Empty at state-2 PLAN-write.)_

### Task 1 — D1 envoy-config schema + D2 validator + D3 eval helper

Extends `crates/envoy-config` with the abort-path fault config surface and its parse-time
gate. Four new schema items land in `bootstrap.rs`: `FaultConfig` (`abort: FaultAbort` +
`#[serde(default)] headers: Vec<HeaderMatcher>`), `FaultAbort` (`http_status: u16` +
`percentage: FractionalPercent`), `FractionalPercent` (`numerator: u32` +
`#[serde(default = "default_denominator")] denominator: DenominatorType`), and the fieldless
`DenominatorType` enum (`HUNDRED`/`TEN_THOUSAND`/`MILLION` via `SCREAMING_SNAKE_CASE`). All
struct items carry `#[serde(deny_unknown_fields)]`; the unit-variant enum does not (meaningless
there) and uses `rename_all` instead. `FractionalPercent` + `DenominatorType` are authored as
general shared types (the first percent types in envoy-config), reusable by future filters.

The D3 deterministic-percentage eval helper is co-located as
`FractionalPercent::selects_deterministic(&self) -> bool` (returns `numerator ==
denominator.value()`), backed by `DenominatorType::value(self) -> u32` (100 / 10_000 /
1_000_000) and the `default_denominator()` serde-default fn (`DenominatorType::Hundred`). The
helper is a pure boolean — no PRNG — because the validator guarantees `numerator ∈ {0,
denominator.value()}`.

The D2 validator adds three `ConfigError` variants in `lib.rs` (`InvalidFaultAbortStatus`,
`FaultPercentageOutOfRange`, `UnsupportedFractionalFaultPercentage`) and the
`validate_fault_config` sub-validator in `bootstrap.rs`. Check order is load-bearing: (1)
`http_status ∈ 100..=599`; (2) `numerator > denominator` → `FaultPercentageOutOfRange` (the
operator-typo case, which MUST precede the fractional check); (3) `numerator != 0 && numerator
!= denominator` → `UnsupportedFractionalFaultPercentage` (the deterministic-only scope gate per
SPEC §4 + §5.6). The `Fault(FaultConfig)` variant joins `HttpFilterTypedConfig` after `Rbac`,
and a dispatch arm in `validate_http_filters` (mirroring the `Rbac` arm: name-vs-typed_config
check then sub-validate) wires it in. The four schema types are re-exported alphabetically from
`lib.rs`. No `error.rs` edit (SPEC correction #1).

**Tests landed (11 new; envoy-config lib 239 → 250):**
- `fault_config_parses_full_abort_with_header_gate`
- `fault_config_denominator_defaults_to_hundred`
- `fault_config_rejects_unknown_field`
- `denominator_type_value_maps_correctly`
- `fractional_percent_selects_deterministic`
- `validate_accepts_fault_abort_100_percent`
- `validate_accepts_fault_abort_0_percent`
- `validate_rejects_invalid_abort_status`
- `validate_rejects_percentage_out_of_range`
- `validate_rejects_fractional_percentage`
- `validate_rejects_name_typed_config_mismatch`

**Deviations from PLAN:**
1. **No `error.rs` edit** — as the PLAN itself directs (correction #1). Recorded for completeness.
2. **`crates/envoy-filter/src/instance.rs` interim bridge arm (1 file beyond the PLAN's "exactly
   two files").** Adding the `Fault` variant to the closed `HttpFilterTypedConfig` enum makes the
   exhaustive `match` in `HttpFilterInstance::build` non-exhaustive, breaking the workspace build
   (and the clippy/build gates). The fix mirrors the phase-10 Task 1 precedent verbatim (commit
   `3fbe9f5`): a transient arm returning `FilterError::UnsupportedFilterType { position: 0, name:
   hf.name.clone() }` with a comment deferring to the FaultFilter-runtime task. The PLAN's
   "exactly two files" scope was an oversight; this is the established, required move.
3. **`Serialize` added to all 4 fault schema derives.** The PLAN's verbatim Step-4 block derived
   only `Deserialize`, but `HttpFilterTypedConfig` derives `Serialize`, so its variant payloads
   must too (compile error otherwise). Matches the precedent of every existing variant config
   (`RbacConfig`, `RouterConfig`, etc.).

**LoC delta (per file, `git diff --numstat`):**

| File | Added | Removed |
|---|---|---|
| `crates/envoy-config/src/bootstrap.rs` | 343 | 0 |
| `crates/envoy-config/src/lib.rs` | 42 | 11 |
| `crates/envoy-filter/src/instance.rs` | 10 | 0 |
| **Total** | **395** | **11** |

(The `lib.rs` 11 removals are re-flow of the re-export block + the `FaultPercentageOutOfRange` /
`UnsupportedFractionalFaultPercentage` `#[error(...)]` attributes that `cargo fmt` wrapped.)

**5-gate attestation (stable toolchain):**

- **fmt** — PASS. `cargo fmt --all -- --check` clean after one `cargo fmt --all` pass (rustfmt
  wrapped two long `#[error(...)]` attribute strings).
- **clippy** — PASS. `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  `Finished` with zero warnings (after the instance.rs bridge arm closed the non-exhaustive match).
- **build** — PASS. `cargo build --workspace --all-targets`: `Finished` clean.
- **test** — PASS. `cargo test --workspace`: 559 passed; 0 failed; 1 ignored across the
  workspace. envoy-config lib: `test result: ok. 250 passed; 0 failed` (the 11 new fault_tests).
- **deny** — PASS. `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`
  (the unmatched-license-allowance warnings are pre-existing, unchanged from phase 10).

### Task 2 — D4 FaultFilter runtime + D7 stats wiring + D7.1 BEHAVIOR_CONTRACT row

`crates/envoy-filter/src/fault.rs` lands the `FaultFilter` runtime filter following the
`rbac.rs` + `local_rate_limit.rs` sibling-filter precedent. The decode path implements the
gate-then-select logic: `header_gate_matches` (AND semantics over `Vec<HeaderMatcher>`; empty
gate ⇒ all requests) is checked first, then `abort_selects` (the build-time-lowered
`FractionalPercent::selects_deterministic()` bool). When both are true, `aborts_injected.inc()`
fires and `Decision::StopAndSend(FilterResponse { status: abort_status, reason: None, headers:
vec![], body: Bytes::from_static(b"fault filter abort") })` is returned. `encode_headers` is a
no-op (decode-only filter at phase-11 scope). `build_from_config` registers the
`http.{hcm_stat_prefix}.fault.aborts_injected` counter via the standard `StatsRegistry::register_counter` +
`map_err(FilterError::InvalidConfig)` chain matching the `RbacFilter` precedent.

`crates/envoy-filter/src/lib.rs` gains `pub mod fault;` (alphabetically between `error` and
`header_mutation`) and `pub use fault::FaultFilter;` (alphabetically between `FilterError` and
`HeaderMutationFilter`). The re-export satisfies clippy's dead-code gate: `FaultFilter` is
`pub` at the crate root, so the type itself is considered reachable. The methods and helper
function are `pub(crate)` and not yet wired into `HttpFilterInstance` dispatch (that is Task 3),
so `#[allow(dead_code)]` attributes are placed on the impl block, the struct fields, the
`FAULT_ABORT_BODY` const, and `header_gate_matches` — the established pre-wiring posture for
transient task-boundary gaps. Task 3 removes these suppression attributes when it wires the
dispatch arm.

The `docs/envoy-rust/BEHAVIOR_CONTRACT.md` "Stat-name mapping" section gains the `**11 entries
(Fault filter):**` block immediately after the RBAC entries, one row:
`http.<hcm_stat_prefix>.fault.aborts_injected` (value-exact; §6.2-verified).

**Tests landed (5 new; envoy-filter lib 64 → 69):**
- `abort_100_percent_no_gate_aborts_every_request`
- `abort_0_percent_never_aborts`
- `header_gate_match_aborts_miss_passes`
- `aborts_injected_counter_increments_once_per_abort_only`
- `encode_headers_is_noop`

**Deviations from PLAN:**
1. **`#[allow(dead_code)]` attributes added** on the impl block, all four struct fields,
   `FAULT_ABORT_BODY`, and `header_gate_matches`. The PLAN's spec noted "the `pub use` keeps
   it from being dead-code" — but that applies only to the `FaultFilter` type itself; the
   `pub(crate)` methods + private helper are only called from the test module until Task 3
   wires the dispatch. Without the allow-attributes `cargo clippy -D warnings` fails. This is
   the correct pre-wiring posture; the attributes are transient (removed at Task 3). The PLAN
   did not anticipate the per-method granularity of rustc's dead_code lint.
2. **Test helpers adjusted from verbatim spec** (3 changes; see report). The spec's verbatim
   tests assumed `FilterRequest: Default`, `registry.counter_value(name)`, and
   `serde_yaml::from_str` for `header_matcher_exact`. All three differ from the actual APIs
   (confirmed by reading `types.rs`, `registry.rs`, and `rbac.rs` before writing tests).
   Adjustments: (a) `req()` uses explicit struct fields (no `Default`); (b) counter-read uses
   idempotent `registry.register_counter(name).expect(...).value()` matching the `rbac.rs`
   precedent; (c) `header_matcher_exact` uses direct struct construction
   (`HeaderMatcherMode::StringMatch(StringMatcher { ... })`) matching `rbac.rs`.

**LoC delta (per file, `git diff --numstat` + new-file count):**

| File | Added | Removed |
|---|---|---|
| `crates/envoy-filter/src/fault.rs` (new) | 214 | 0 |
| `crates/envoy-filter/src/lib.rs` | 2 | 0 |
| `docs/envoy-rust/BEHAVIOR_CONTRACT.md` | 6 | 0 |
| `docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md` | (this section) | 0 |
| **Total** | **~280** | **0** |

**5-gate attestation (stable toolchain):**

- **fmt** — PASS. `cargo fmt --all -- --check` clean after one `cargo fmt --all` pass (rustfmt
  reformatted the `assert!(matches!(...))` call in `header_gate_match_aborts_miss_passes`).
- **clippy** — PASS. `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  `Finished` with zero errors after adding `#[allow(dead_code)]` suppression attributes.
- **build** — PASS. `cargo build --workspace --all-targets`: `Finished` clean.
- **test** — PASS. `cargo test --workspace`: all test results ok; no failures.
  envoy-filter lib: `test result: ok. 69 passed; 0 failed` (5 new fault tests).
- **deny** — PASS. `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`
  (pre-existing unmatched-license-allowance warnings unchanged).

### Task 3 — D5 HttpFilterInstance::Fault variant + dispatch

`crates/envoy-filter/src/instance.rs` receives the fifth production variant and its three
dispatch arms, completing the "rejected → supported" move established by the Task-1 transient
bridge. The `use crate::fault::FaultFilter;` import joins the existing use-block alphabetically
(between `error` and `header_mutation`). The `Fault(FaultFilter)` variant lands between
`Rbac(RbacFilter)` and the `#[cfg(feature = "test-util")]` block, with a doc-comment mirroring
the `Rbac` variant's style. The Task-1 transient bridge arm (which returned
`FilterError::UnsupportedFilterType`) is replaced by the real build arm:
`HttpFilterInstance::Fault(FaultFilter::build_from_config(cfg, registry, hcm_stat_prefix)?)`.
The decode and encode dispatch arms mirror the `Rbac` arm shapes exactly, with `resp_arg` used
as the encode parameter name to match all existing encode arms.

`crates/envoy-filter/src/fault.rs` receives the anticipated dead-code sweep: all 7
`#[allow(dead_code)]` attributes placed at Task 2 are removed, along with the "until then"
comments that described the pre-wiring state. With the dispatch arms in place, every previously
suppressed item (`FAULT_ABORT_BODY`, the 4 struct fields, the impl block, and
`header_gate_matches`) is now genuinely reachable via `build_from_config` / `decode_headers` /
`encode_headers`. Clippy's `-D warnings` flag enforces this: any remaining unnecessary
`#[allow(dead_code)]` attribute would itself trigger an `unused_attributes` lint failure.

The integration test is placed in `pipeline.rs` (the home of all `FilterPipeline::build_from_config`
integration tests, confirmed by spot-check before writing). It constructs a 2-filter chain
(`Fault` at 100% abort + no gate, then `Router`) and drives a `GET /` request through
`decode_headers`, asserting `Decision::StopAndSend` with `resp.status == 503`. `FilterRequest`
is constructed with explicit fields (no `Default` impl) matching the `req()` helper in
`fault.rs`'s own test module. `cargo fmt --all` reformatted the `build_from_config` call
from the 3-line indent form used in the initial edit to a 2-line form — the only formatting
adjustment required.

**Tests landed (1 new; envoy-filter lib 69 → 70):**
- `pipeline::tests::build_from_config_wires_fault_then_router`

**Deviations from PLAN:**
1. **`FilterRequest::default()` → explicit construction.** The PLAN draft test used
   `crate::FilterRequest::default()`, which does not compile (`FilterRequest` has no `Default`
   impl). Replaced with explicit field construction `FilterRequest { method: "GET".to_string(),
   path: "/".to_string(), headers: vec![], body: None }`, confirmed by reading `types.rs`.
2. **`fault.rs` 7×`#[allow(dead_code)]` sweep makes Task 3 a 2-file change.** The PLAN's
   "Files:" line listed only `instance.rs`, but the Task Description (Critical Adjustment #2)
   mandated removal of the 7 transient dead-code attributes from `fault.rs` as an anticipated,
   self-enforcing clippy-driven follow-up. This closes the Task-2 M1 minor (attributes placed
   as a pre-wiring bridge, explicitly scoped to Task 3 removal). Not scope creep — the task
   description calls this out verbatim as a "MANDATED, anticipated cross-file edit."

**LoC delta (per file, `git diff --numstat`):**

| File | Added | Removed |
|---|---|---|
| `crates/envoy-filter/src/fault.rs` | 0 | 14 |
| `crates/envoy-filter/src/instance.rs` | 11 | 10 |
| `crates/envoy-filter/src/pipeline.rs` | 41 | 0 |
| `docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md` | (this section) | 0 |
| **Total** | **~52** | **24** |

**5-gate attestation (stable toolchain):**

- **fmt** — PASS. `cargo fmt --all -- --check` reported one-line drift (the
  `build_from_config` call indent); fixed with `cargo fmt --all`, re-check clean.
- **clippy** — PASS. `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  `Finished` with zero warnings (removal of dead_code attrs verified no `unused_attributes`
  or `dead_code` lints remain).
- **build** — PASS. `cargo build --workspace --all-targets`: `Finished` clean.
- **test** — PASS. `cargo test --workspace`: all test results ok; no failures.
  envoy-filter lib: `test result: ok. 70 passed; 0 failed` (1 new pipeline integration test;
  +1 from the 69 that landed at Task 2).
- **deny** — PASS. `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`
  (pre-existing unmatched-license-allowance warnings unchanged).

### Task 4 — D6 H2 decorate_filter_synth_response_h2 + 2 wirings + 2 tests

`crates/envoy-http2/src/response.rs` receives `decorate_filter_synth_response_h2`, the H2-side
peer of H1's `decorate_filter_synth_response` (`crates/envoy-http1/src/hcm.rs:968`). The helper
is `pub(crate)` and placed immediately before `send_envoy_response`, adjacent to
`build_http_response`. It is functionally symmetric to the H1 helper in every respect except
one: it emits NO `connection` header, because `connection` is an H2-forbidden hop-by-hop header
(RFC 7540 §8.1.2.2) that `build_http_response` would strip anyway via
`H2_FORBIDDEN_HOP_BY_HOP`. The implementation semantics: `content-length` is always derived
from `resp.body.len()` and overwrites any existing value (an incorrect filter-set value is never
forwarded); `server`, `date`, and `content-type` are added only-if-missing (a filter that sets
its own value wins). `date` is sourced from `envoy_http1::date::now_imf_fixdate()` (the same
cached-second variant the H1 helper uses).

`crates/envoy-http2/src/hcm.rs` receives the 2 wirings. The decode-side arm
`H2RequestPath::SynthFromDecode(r)` (previously line 373) is updated to `(mut r)` and calls
`crate::response::decorate_filter_synth_response_h2(&mut r)` before returning `r`. The
encode-side arm `Decision::StopAndSend(replacement)` (inside `finalize_h2_stream`, previously
line 436) calls `crate::response::decorate_filter_synth_response_h2(&mut resp)` immediately
after constructing the replacement `Response`. The `mut` on `r` in the decode arm is justified
by the `&mut r` borrow on the next line; clippy's `-D warnings` gate confirmed no
`unused_mut` warning.

**09 REVIEW M2 implementation-arm close:** This task closes the 09 REVIEW M2 implementation arm
(the H2 HCM filter-synth decoration gap). The documentation arm closed at phase-10 D5 via the
ADR-0033 Consequences amendment at `docs/envoy-rust/DECISIONS.md:699`, which explicitly named
"the next HTTP-filter-family phase exercising filters bilaterally on H2" (i.e. this phase 11
D6) as the M2 implementation close site. The phase-09 PROGRESS Commit C forward-reference
records that H1's `decorate_filter_synth_response` helper first landed under ADR-0033 Commit C
— the H2 writer path now reaches parity. **After this task, the 09 → 10 → 11 M2 chain ENDS.**
No new ADR was required (the close shape is ordinary deliverable work, not an architectural
decision).

**Tests landed (2 new; envoy-http2 lib 42 → 44 passed, 43 → 45 running including 1 pre-existing ignored):**
- `response::tests::decorate_h2_adds_standard_headers_when_filter_provides_none`
- `response::tests::decorate_h2_preserves_filter_headers_and_overwrites_content_length`

**Deviations from PLAN:**
1. **`cargo fmt` reformatted two spans** in `response.rs`: the
   `if !resp.headers.iter().any(...)` predicate (split across 4 lines per rustfmt's
   line-length limit) and the `assert!(name("connection").is_none(), ...)` test assertion
   (split to 3-arg form). No semantic change; reformatted with `cargo fmt --all` before
   the fmt-gate check.
2. **PLAN template used `DEFAULT_SERVER_NAME`/`DEFAULT_CONTENT_TYPE` symbolically (lock-in
   #23).** These consts are private and unexported from `crates/envoy-http1/src/hcm.rs:21-22`.
   Per lock-in #23, the helper instead uses the literal values `"envoy-rust"` and `"text/plain"`
   + `envoy_http1::date::now_imf_fixdate()` — exactly the same convention already used in the
   `synth_h2_502()` helper (`hcm.rs:562-563`) and the H2 proxy arm (`hcm.rs:347,356`). This is
   NOT a divergence: the literals match the H1 helper's const values and the existing envoy-http2
   convention. Adding a `pub` to the H1 consts would be an out-of-scope third-file edit; the
   lock-in #23 directive was followed exactly.

**LoC delta (per file, `git diff --numstat`):**

| File | Added | Removed |
|---|---|---|
| `crates/envoy-http2/src/response.rs` | 102 | 0 |
| `crates/envoy-http2/src/hcm.rs` | 10 | 3 |
| `docs/envoy-rust/phases/11-http-filter-fault/PROGRESS.md` | (this section) | 0 |
| **Total** | **~112** | **3** |

**5-gate attestation (stable toolchain):**

- **fmt** — PASS. `cargo fmt --all -- --check` reported two formatting drifts (line-length
  wrapping in helper + test assertion); fixed with `cargo fmt --all`, re-check clean.
- **clippy** — PASS. `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  `Finished` with zero warnings (the `mut r` in the decode arm is justified by `&mut r` usage;
  no `unused_mut` warning).
- **build** — PASS. `cargo build --workspace --all-targets`: `Finished` clean.
- **test** — PASS. `cargo test --workspace`: all test results ok; no failures.
  envoy-http2 lib: `test result: ok. 44 passed; 0 failed; 1 ignored` (2 new `decorate_h2` tests;
  +2 from the 42 that landed before Task 4).
- **deny** — PASS. `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`
  (pre-existing unmatched-license-allowance warnings unchanged).
