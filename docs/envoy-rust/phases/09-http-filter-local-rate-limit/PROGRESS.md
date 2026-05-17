# Phase 09 (`09-http-filter-local-rate-limit`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x /
> 08.x cadence. State-2 PLAN-write lands this skeleton + the Task 1 preamble; state-3
> dispatch appends `### Task N — <name>` subsections in execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PLAN.md` (the
  state-2 PLAN.md per `BOOTSTRAP_PROMPT.md` §5 state 2; ~1180 lines; 8 tasks; full
  `- [ ]` checkbox steps per task per the project's mature TDD cadence).
- **CREATE** `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md`
  (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `09` `status: planned` →
  `status: in-progress`. Earlier rows unchanged.
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill;
  Last commit; Last updated; new `Phase-09 state-2 PLAN-write` subsection in Notes.

**Predecessor commit:** `3025594` — `phase 09: state-1 brainstorm — http-filter-local-rate-limit SPEC.md (HTTP-filter-family first phase; 07.2 REVIEW M1 named close site)` (the phase-09 state-1 brainstorm commit; immediate prologue).

**SPEC commit:** `3025594` (same — state-1 + state-2 are adjacent commits in the phase's
lifecycle; the SPEC didn't change between state-1 and state-2).

**ROADMAP status before this commit:** row `09` `planned` (added at state-1).
**ROADMAP status after this commit:** row `09` `in-progress`.

**STATE.md "Active phase" status before:** `phase 09 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist)`.
**STATE.md "Active phase" status after:** `phase 09 lifecycle state 2-complete / state-3-next (PLAN.md landed; first task commit pending)`.

**DECISIONS.md status before AND after:** **ADR-0032** (parent-08 state-2 split
decision). No ADR lands at this commit per PLAN lock-in #34.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged. The 4 stat-name mapping
rows land at Task 3 commit; the 1 header allow-list row lands at Task 5 commit per PLAN
lock-ins #30 + #31 (SPEC §6.5 cadence).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **8 tasks** per PLAN §4. Under SPEC §6.1's ~10-13 projection on the lower end.
  Subagent-driven execution at state 3 per PLAN lock-in #37 + `feedback_execution_style`.
- **~1100-1400 LoC projected** per PLAN §3 (production ~380, tests ~655, fixture/doc
  ~260). Under SPEC §6.1's ~1500-LoC gate.
- **Single-phase; no nest-split** per PLAN lock-in #36 + parent-08 SPEC §6.1
  alternative (vi) accept-drift discipline.

---

## Task 1 preamble

### PLAN-write SPEC corrections (7 — verified against HEAD `3025594`)

Each verified by reading the on-disk surface; corrections land in execution at the
named task. Per the 06.2 → 06.3 → 07.x → 08.x precedent (06.1 0 corrections / 06.2 4
corrections / 06.3 5 corrections / 07.1 6 corrections / 07.2 8 corrections / 08.1 6
corrections / 08.2 6 corrections), the 7 corrections recorded here track the mature
PLAN-write cadence:

1. **`ConfigError` enum lives in `crates/envoy-config/src/lib.rs`, NOT
   `crates/envoy-config/src/bootstrap.rs`** as SPEC §3 D2 implies. The validator
   function `validate_http_filters` IS in `bootstrap.rs` (lines 1597-1652). Existing
   HeaderMutation `ConfigError` variants (`EmptyHeaderMutationKey` /
   `InvalidHeaderMutationKey` / `UnsupportedHeaderMutationAppendAction`) land in
   `lib.rs` lines 266-294. **Action at Task 1:** the 4 new ConfigError variants land
   in `lib.rs`; the sub-validator + LocalRateLimit dispatch arm land in
   `bootstrap.rs`. Lock-in #16.

2. **The HCM filter-pipeline build site is the HCMConfig constructor, NOT the
   request-handling functions `serve_connection`/`handle_one_stream`** as SPEC §3 D4
   says. Reading HEAD: H1 builds at `crates/envoy-http1/src/hcm.rs:185` inside
   `Http1HCMConfig::from_config`; H2 reuses the pre-built `config.filter_pipeline`
   (cloned per-stream at `crates/envoy-http2/src/hcm.rs:148`). Both H1 and H2 use
   the same `Http1HCMConfig` (H2 re-exports), so there is exactly ONE call site to
   widen. The constructor already holds `registry: Arc<StatsRegistry>` in scope.
   **Action at Task 4:** one-line extension to the call at `envoy-http1/src/hcm.rs:185`.
   Lock-in #24.

3. **`Http1HeaderRule` is a unit-variant enum with only `SetEqualModuloAllowList`**
   (line 589 of `tests/differential/src/lib.rs`), NOT `Option<Vec<HeaderRule>>` as
   SPEC §3 D8.1 hedge text implies. `Http1Probe::expected_headers` is
   `Option<Http1HeaderRule>` (line 634). **No harness extension needed.** The
   differential fixture relies on `SetEqualModuloAllowList` (both proxies emit
   `x-envoy-ratelimited: true` on 429; set-equal passes). **The direct
   per-header `x-envoy-ratelimited: true` assertion lives at the in-process backstop
   (Task 7, D8.3), not the differential fixture (Task 5, D8.1).** Mirrors the 07.2
   fixture-0013 `x-filter-response-stamp: phase-07` pattern exactly. Lock-in #29 +
   #33.

4. **`StatsRegistry::register_counter` takes `&self`, NOT `&Arc<Self>`** per
   `crates/envoy-stats/src/registry.rs:31`. `&Arc<StatsRegistry>` works via
   `Deref<Target = StatsRegistry>`. PLAN threads `&Arc<StatsRegistry>` for shared
   ownership semantics through the pipeline build path. **No API change required;**
   only a typing clarification. Lock-in #14.

5. **`HttpFilterInstance` carries 2 `#[cfg(feature = "test-util")]` variants**
   (`TestStopAndSendOnDecode(FilterResponse)` + `TestStopAndSendOnEncode(FilterResponse)`)
   in addition to `Router` + `HeaderMutation` (lines 17-25 of `instance.rs`). These
   were landed at 07.1 / 07.2 to support cross-crate HCM integration tests. SPEC §3
   D4 + D5 don't reference them. **Action at Task 4:** the new `LocalRateLimit`
   variant goes between `HeaderMutation` and the `#[cfg(feature = "test-util")]`
   block; test-util variants preserved verbatim. The `build` signature change (drop
   `_position`; add `registry`) is orthogonal — test-util variants are constructed
   via separate `test_stop_and_send_on_decode`/`test_stop_and_send_on_encode`
   constructors, NOT via `build`, so no test-util-arm edit is needed.

6. **`HeaderMutationFilter::build_from_config` is single-arg** (line 48-62 of
   `header_mutation.rs`). The new `LocalRateLimitFilter::build_from_config` has a
   **two-arg** shape (the registry param is needed for counter registration). This is
   a deliberate new precedent for any future filter that needs stats — NOT a drift
   from SPEC, recorded here for the subagent's awareness. Lock-in #10 +
   #14.

7. **SPEC §3 D1 names `LocalRateLimitFilterConfig` and `HttpStatusCode`**; PLAN
   renames them to `LocalRateLimitConfig` and `HttpStatus` respectively for
   consistency with existing schema naming (`HeaderMutationConfig`, `RouterConfig`
   — no `*Filter*` infix). Lock-in #20.

### Architecture-decision lock-ins (39 — see PLAN.md §2)

Per `feedback_pick_recommendation` ("always pick the recommended option; do not
ask"), 39 lock-ins recorded in the PLAN's lock-in table (§2). Grouped by topic for
in-execution lookup:

- **#1-#2** — module placement + new path-dep (`envoy-stats` on `envoy-filter`).
- **#3-#9** — token bucket primitive shape (state shape; numeric type; lazy-fill
  formula; CAS atomicity discipline; Mutex hold scope; poisoning posture;
  concurrency torture test REQUIRED per SPEC §6.3).
- **#10-#13** — filter struct shape (fields; decode/encode method semantics; 429
  synth response shape).
- **#14** — counter registration discipline.
- **#15-#21** — envoy-config schema (`fill_interval` as `serde_yaml::Value`; 4 new
  ConfigError variants; validator dispatch; sub-validator shape;
  `HttpFilterTypedConfig::LocalRateLimit` variant; schema struct shapes; renames
  per #7 above; `default_status` helper).
- **#22-#23** — D5 (07.2 M1 closure) co-located with D4 at Task 4; hardcoded
  `position: 0` at `header_mutation.rs::map_entry` left AS-IS per SPEC §3 D5.
- **#24-#27** — pipeline integration (HCM build site threading; widened
  `build_from_config` signature; widened `HttpFilterInstance::build` signature;
  unit test update).
- **#28-#29** — fixture 0016 (bootstrap shape; probe list).
- **#30-#31** — BEHAVIOR_CONTRACT cadence (4 stat rows at Task 3 commit; 1 header
  row at Task 5 commit per SPEC §6.5).
- **#32-#33** — fuzz corpus seed; in-process backstop.
- **#34** — no ADR landing (ledger head stays ADR-0032; conditional ADR-0033 +
  ADR-0034 stay reserved).
- **#35** — `#![forbid(unsafe_code)]` posture (inherited from crate root).
- **#36** — split-gate verdict (single-phase; no split; accept up to ~+50% drift).
- **#37** — subagent-driven execution at state 3.
- **#38** — PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md
  (this commit).
- **#39** — Cargo.lock cadence (empty diff expected).

Full text + rationale per lock-in lives in PLAN.md §2. PROGRESS sub-sections at
state-3 reference lock-ins by `#NN` rather than re-explaining.

### PLAN-write deviations beyond the SPEC corrections (1)

1. **Lock-in #20: schema struct renames** (`LocalRateLimitFilterConfig` →
   `LocalRateLimitConfig`; `HttpStatusCode` → `HttpStatus`). SPEC §3 D1 named the
   types with the `*Filter*Config` / `HttpStatus*Code*` suffixes; PLAN renames per
   existing schema convention (`HeaderMutationConfig`, `RouterConfig`). Surface
   effect: zero — the types are envoy-config-internal; no project-wide rename.
   Recorded here for transparency.

### Carryforward dispositions

| ID | Severity | Item | Disposition at 09 |
|---|---|---|---|
| **07.2 REVIEW M1** | Minor | Severed `position` plumbing (`_position: usize` parameter on `HttpFilterInstance::build` + `.enumerate()` on `FilterPipeline::build_from_config`) | **PROJECTED-CLOSE at Task 4 (D5).** Co-located with D4 per SPEC §6.2 lock-in #22. The PROGRESS subsection at Task 4 commit will record the closure attribution. The chain 07.2 → 09 ends. |
| **07.2 REVIEW M2** | Minor | `apply_mutations` Overwrite O(n²) YAGNI | **Carry forward indefinitely.** Phase 09 does NOT touch `header_mutation.rs` per lock-in #23 + SPEC §3 D5 rationale. Activates only if a future filter-family phase amplifies the apply_mutations call rate. |
| **07.2 REVIEW M3** | Minor | fixture-0013 `expected_body` coupling | **Carry forward indefinitely.** Phase 09's fixture 0016 uses a different bootstrap shape (direct_response without backend echo). Not engaged. |
| **08.1 REVIEW M3** | Minor | Forward-looking `Arc<BTreeMap<...>>` on `command_line_options` | **Carry forward indefinitely.** Not engaged. |
| **08.2 REVIEW M1-M8** | Minor | Various code-quality / doc-polish items per 08.2 REVIEW §3-§7 | **Carry forward indefinitely.** None engaged by phase 09's surface (the filter does not touch DrainState, AdminEndpoint, Listener::serve, or other 08.2 surfaces). |
| **08.2 REVIEW T1-T3** | Minor | Test / audit-trail polish | **Carry forward indefinitely.** Not engaged. |
| **08.2 REVIEW D1-D5** | Doc | Fixture-0015 / BEHAVIOR_CONTRACT doc-staleness | **CLOSED at 08.2 state-6 close-out commit `304ce98`.** Chain ended before phase 09 began. Recorded here for completeness. |
| **06.3 REVIEW I2** | Important | Synthetic 5xx backend + 4-class `pre_requests` deferred | **Carry forward indefinitely.** Upstream-robustness family is the natural close site. Not engaged. |
| **06.2 REVIEW M1 / M2 / M4 / M5** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **06.1 REVIEW M2 / M3 / M5 / M6** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **05.3 REVIEW I2** | Important | Typed-error chain dissolution at H2 dispatch site | **Carry forward indefinitely.** Not engaged. |
| **05.2 REVIEW I1 / I2 / I3** | Important | Various | **Carry forward indefinitely.** Not engaged. |
| **04.1 REVIEW M5 / M9** | Minor | Cargo.lock cadence ratification ADR | **Carry forward unchanged.** Phase 09 introduces zero new top-level Cargo deps per lock-in #2 + #39. The cadence pick stays unforced. |
| **04.1 REVIEW M-claim / M1 / M2 / M4 / M7** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **02.2 REVIEW M1** | Minor | `*EchoBackend::Drop` polling loop blocks on `std::thread::sleep` | **Carry forward unchanged.** Phase 09's fixture 0016 uses direct_response (no Echo backend); the chain continues unchanged. |
| **Phase-00 I3** | — | SIGKILL → SIGTERM graceful termination of subject subprocess (`nix` crate deferral) | **Carry forward unchanged.** Phase 09 drives the filter via deterministic HTTP request bursts; no signal-based subprocess termination. The `nix` crate stays off the permitted-foundations list. |

### State-3 entry routing

The next session reads STATE.md, sees `state 2-complete / state-3-next (PLAN.md
landed; first task commit pending)` + Next expected skill `superpowers:subagent-driven-development`,
and dispatches Task 1 per the PLAN.

---

## Tasks 1-8

_(Per-task `### Task N — <name>` subsections append at state-3 task commits per the
06.x / 07.x / 08.x cadence. State-2 commit lands this skeleton only.)_

### Task 1 — D1 envoy-config schema + D2 validator (co-located)

_(Pending state-3 dispatch.)_

### Task 2 — D3 hand-rolled token bucket primitive + concurrency torture test

_(Pending state-3 dispatch.)_

### Task 3 — D3 LocalRateLimitFilter runtime + D6 stats wiring + D7.1 4 stat-mapping rows

_(Pending state-3 dispatch.)_

### Task 4 — D4 HttpFilterInstance::LocalRateLimit variant + D5 07.2 REVIEW M1 closure

_(Pending state-3 dispatch.)_

### Task 5 — D8.1 fixture 0016 + Docker-gated wrapper + D7.2 x-envoy-ratelimited row

_(Pending state-3 dispatch.)_

### Task 6 — D8.2 parse_bootstrap fuzz corpus seed

_(Pending state-3 dispatch.)_

### Task 7 — D8.3 in-process backstop http_filter_local_rate_limit.rs

_(Pending state-3 dispatch.)_

### Task 8 — state-4 phase-done verification + STATE advance to state-5-next

_(Pending state-3 dispatch.)_

---

*End of PROGRESS skeleton. State-3 task commits append per-task narrative sections per
the 06.x / 07.x / 08.x cadence.*
