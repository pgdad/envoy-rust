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
