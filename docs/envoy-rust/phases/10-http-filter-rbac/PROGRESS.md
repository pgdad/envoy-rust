# Phase 10 (`10-http-filter-rbac`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x /
> 08.x / 09 cadence. State-2 PLAN-write lands this skeleton + the Task 1 preamble;
> state-3 dispatch appends `### Task N — <name>` subsections in execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/10-http-filter-rbac/PLAN.md` (the state-2 PLAN.md
  per `BOOTSTRAP_PROMPT.md` §5 state 2; ~2610 lines; 8 tasks; full `- [ ]` checkbox
  steps per task per the project's mature TDD cadence).
- **CREATE** `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `10` `status: planned` →
  `status: in-progress`. Earlier rows unchanged.
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill;
  Last commit; Last updated; new `Phase-10 state-2 PLAN-write` subsection in Notes.
- **MODIFY** `docs/envoy-rust/DECISIONS.md` — append **ADR-0034** (the §6.2
  empirical-verification body-bytes correction per SPEC §7 option A recommended
  posture; ledger head advances `ADR-0033 → ADR-0034`).
- **MODIFY** `docs/envoy-rust/phases/10-http-filter-rbac/SPEC.md` — 3 inline ADR-0034
  revisions (§2.2 body bytes projection; §3 D8.1 fixture body bytes assertion; §5.9
  filter response shape) — each replaces `"RBAC: access denied\n"` (20 bytes) with
  `"RBAC: access denied"` (19 bytes per ADR-0034 empirical evidence).

**Predecessor commit:** `c73f44f` — `phase 10: state-1 brainstorm — http-filter-rbac SPEC.md (HTTP-filter-family second phase; 09 REVIEW M2 + M3 named close sites)`
(the phase-10 state-1 brainstorm commit; immediate prologue).

**SPEC commit base:** `c73f44f` (the state-1 brainstorm commit). This state-2 commit
edits SPEC.md inline at 3 sites per ADR-0034 — the inline edits ratify the §6.2
empirical-verification findings.

**ROADMAP status before this commit:** row `10` `planned` (added at state-1).
**ROADMAP status after this commit:** row `10` `in-progress`.

**STATE.md "Active phase" status before:** `phase 10 lifecycle state 1-complete / state-2-next (SPEC.md landed; PLAN.md does not exist)`.
**STATE.md "Active phase" status after:** `phase 10 lifecycle state 2-complete / state-3-next (PLAN.md landed; first task commit pending)`.

**DECISIONS.md status before:** **ADR-0033** (phase-09 SPEC §2.2 revision).
**DECISIONS.md status after:** **ADR-0034** (phase-10 SPEC §2.2 + §3 D8.1 + §5.9
body-bytes revision per §6.2 empirical verification; recommended posture per SPEC §7
option A). The ledger head advances by ONE per D-3.5 sequential numbering. The
remaining 3 conditional ADR-0034 slots (option B per-route deferral; option C
foundations grant; option D D5 superseding-ADR shape) all DEFER per recommended
posture — option B deferred to whichever future filter actually needs per-route
config; option C no grant projected; option D in-place amendment per SPEC §2.3 +
PLAN lock-in #32. Next available number after THIS commit is **ADR-0035**.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged. The 2 stat-name mapping
rows under "**10 entries (RBAC filter):**" land at Task 3 commit per PLAN lock-in
#36 (SPEC §6.6 cadence — contract extensions land at empirical-engagement task time,
NOT at PLAN-write time).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **8 tasks** per PLAN §4. Aligned with SPEC §6.1's ~9-11 projection on the lower end.
  Subagent-driven execution at state 3 per PLAN lock-in #43 + `feedback_execution_style`.
- **~1525 LoC projected** per PLAN §3 (production ~510, tests ~735, fixture/doc ~280).
  Marginally at SPEC §6.1's ~1500-LoC gate (+1.7%); accept the projection per the soft
  gate (lock-in #42).
- **Single-phase; no nest-split** per PLAN lock-in #42 + parent-08 SPEC §6.1
  alternative (vi) accept-drift discipline.
- **ONE ADR landing at state-2:** ADR-0034 (option A per SPEC §7 — empirical-verification
  body-bytes correction).

---

## Task 1 preamble

### SPEC §6.2 empirical-verification findings (3 — performed at PLAN-write per ADR-0033 process-gap-awareness doctrine)

Per SPEC §6.2's process-improvement directive (the ADR-0033-derived discipline note —
"state-1 brainstorming should empirically verify upstream wire shapes... but no
doctrine-level enforcement is introduced; the empirical-discovery-at-Task-5 → ADR-at-state-3
path is a viable correction route per D-3.5"), the PLAN-writer performed all 3 verifications
at THIS state-2 commit against `envoyproxy/envoy:v1.33.0` Docker, using the SPEC §3 D8.1
canonical bootstrap (HCM + envoy.filters.http.rbac + envoy.filters.http.router +
direct_response action; 1 ALLOW policy `pass_with_header` with `permissions: [- any: true]`
+ `principals: [- header: { name: x-rbac-pass, string_match: { exact: yes } }]`).

**Verification methodology:** wrote canonical bootstrap to `/tmp/phase10-spec62-verify/envoy.yaml`;
ran `docker run --rm -d -p 10000:10000 -p 9901:9901 -v ... envoyproxy/envoy:v1.33.0 --config-path /etc/envoy/envoy.yaml`;
issued 2 probe pairs:
- **Pair A (curl convenience probes, default keep-alive):** `curl -i` against allow + deny.
- **Pair B (harness-shape probes with `Connection: close` request framing):** Python TCP
  client mirroring `tests/differential/src/lib.rs::drive_http_get`'s exact request shape
  (`GET / HTTP/1.1\r\nHost: envoy-rust.test\r\nConnection: close\r\n[+optional headers]\r\n`).
  Scraped `/stats` from the admin endpoint to capture stat names.

**Finding (a) — Stats namespace shape:** MATCHES SPEC §2.1 projection exactly.

Empirical observation (post-1-allowed + 1-denied probe scrape):
```
http.ingress_http.rbac.allowed: 1
http.ingress_http.rbac.denied: 1
http.ingress_http.rbac.shadow_allowed: 0
http.ingress_http.rbac.shadow_denied: 0
```

SPEC §2.1 + §6.5 project `http.<hcm_stat_prefix>.rbac.{allowed,denied}`. Upstream
emits 2 additional counters (`shadow_allowed`, `shadow_denied`) at 0 unconditionally —
these are emitted even when shadow_rules is unconfigured per upstream Envoy v1.33's
behavior. **Phase-10 registers only the 2 primary counters** since shadow_rules defers
per SPEC §4. The differential fixture does NOT scrape RBAC stats (only the 4 HTTP
status probes), so the 2-vs-4 stat-name-set divergence is not exercised bilaterally
at phase-10's verification surface. The BEHAVIOR_CONTRACT.md "Stat-name mapping" rows
landing at Task 3 document the 2 primary counters' value-exact equivalence per the
06.x convention. **No SPEC revision needed; no ADR triggered by this finding.**

**Finding (b) — 403 response body bytes:** DIFFERS from SPEC §2.2 projection by 1 byte.

Empirical body bytes (under both Pair A and Pair B request framing — identical body):
```
content-length: 19
body bytes (hex): 52 42 41 43 3a 20 61 63 63 65 73 73 20 64 65 6e 69 65 64
body string:     "RBAC: access denied"  (19 bytes, NO trailing newline)
```

SPEC §2.2 + §3 D8.1 + §5.9 project `"RBAC: access denied\n"` (20 bytes including
trailing newline). Reality is 19 bytes (no trailing `\n`). **MATERIAL DIFFERENCE** —
1-byte projection error.

**Disposition:** **ADR-0034 (option A per SPEC §7) lands inline at THIS state-2
PLAN-write commit.** The ADR records the empirical evidence + the 3 inline SPEC
revisions (§2.2 + §3 D8.1 + §5.9 — each replaces the 20-byte projection with the
19-byte reality + cross-refs ADR-0034 as the revision authority). PLAN lock-ins #13 +
#14 lock the production-code shape `body: Bytes::from_static(b"RBAC: access denied")`
(19 bytes; no `\n`). The fixture 0017 + in-process backstop assertion shapes adopt the
19-byte body bytes directly.

This is the **first phase to leverage the SPEC §6.2 empirical-verification process-improvement**
directly — phase-09 surfaced the analogous ADR-0033 only at Task 5 subagent dispatch
(a process gap the ADR-0033 Provenance section called out). Phase-10's state-2
PLAN-write performs the verification BEFORE locking the PLAN lock-ins, AVOIDING the
process gap. The ADR-0034 landing pattern (inline at PLAN-write with SPEC inline
edits) mirrors the 05.1 / 05.4 / 09 Task-1-fixup ADR-inline precedent.

**Finding (c) — 403 response header set:** MATCHES SPEC §2.2 projection under
harness-shape `Connection: close` request framing.

Empirical header set (under Pair B — harness-shape probes; the differential harness's
`drive_http_get` sends `Connection: close` on every request per
`tests/differential/src/lib.rs:1039`):

```
content-length: 19
content-type: text/plain
date: Mon, 18 May 2026 17:03:41 GMT
server: envoy
connection: close
```

5 standard HTTP/1.1 headers — exactly matching SPEC §2.2's projection
`{server, date, content-length, content-type, connection}`. The `connection: close`
appears because the harness sends `Connection: close`; if a client sends
`Connection: keep-alive` (or no Connection header — HTTP/1.1 default), upstream omits
the `connection` response header. envoy-rust's `decorate_filter_synth_response` helper
(landed at phase-09 ADR-0033 Commit C) decorates the same 5 headers when present, so
both proxies emit the same 5-header set under the harness's request framing.

**No SPEC revision needed; no ADR triggered by this finding.** Fixture 0017's
`expected_headers: set_equal_modulo_allow_list` correctly matches under the 04.1-landed
`server` + `date` allow-list rows.

### PLAN-write SPEC corrections (7 — verified against HEAD `c73f44f`)

Each verified by reading the on-disk surface; corrections land in execution at the
named task. Per the 06.2 → 06.3 → 07.x → 08.x → 09 precedent (06.1 0 corrections /
06.2 4 corrections / 06.3 5 corrections / 07.1 6 corrections / 07.2 8 corrections /
08.1 6 corrections / 08.2 6 corrections / 09 7 corrections), the 7 corrections recorded
here track the mature PLAN-write cadence:

1. **`HeaderMatcher::matches` takes `&[(String, String)]`, NOT `&[Header]`** as SPEC
   §3 D3 prose implies. The 04.2-landed signature at
   `crates/envoy-config/src/matcher.rs:19` is `pub fn matches(&self, headers: &[(String, String)]) -> bool`.
   `FilterRequest::headers: Vec<(String, String)>` (per
   `crates/envoy-filter/src/types.rs:28-32`) matches directly — no adapter needed.
   **Action at Task 3:** call `m.matches(&req.headers)` inside the recursive
   evaluator's Header arms.

2. **`ConfigError` enum lives in `crates/envoy-config/src/lib.rs`, NOT
   `crates/envoy-config/src/bootstrap.rs`** as SPEC §3 D2 implies (same correction
   as phase-09 PLAN §1 item 1). The validator function `validate_http_filters` IS in
   `bootstrap.rs` (line 1661 at HEAD `c73f44f`). Existing HeaderMutation +
   LocalRateLimit `ConfigError` variants land in `lib.rs`. **Action at Task 1:** 6
   new RBAC ConfigError variants land in `lib.rs`; sub-validator + Rbac dispatch arm
   land in `bootstrap.rs`. Lock-in #26.

3. **The HCM filter-pipeline build site is `Http1HCMConfig::from_config` at
   `crates/envoy-http1/src/hcm.rs:185`** (same correction as phase-09 PLAN §1 item 2).
   The current 09-widened signature is
   `FilterPipeline::build_from_config(&cfg.http_filters, &registry)`. Phase 10
   widens to `(&cfg.http_filters, &registry, &cfg.stat_prefix)` — one additional
   argument at the SINGLE call site. H2 reuses the same `Http1HCMConfig` via re-export
   per the 09 wiring discipline; no second call site exists. Lock-in #5 + #29.

4. **`HttpFilterInstance` carries 2 `#[cfg(feature = "test-util")]` variants**
   (`TestStopAndSendOnDecode(FilterResponse)` + `TestStopAndSendOnEncode(FilterResponse)`)
   at `instance.rs` lines 30-35 — landed at 07.1/07.2 + preserved through 09. SPEC §3
   D4 doesn't reference them. **Action at Task 4:** the new `Rbac(RbacFilter)` variant
   goes between `LocalRateLimit` and the `#[cfg(feature = "test-util")]` block;
   test-util variants preserved verbatim.

5. **`RbacFilter::build_from_config` is THREE-arg** `(cfg, registry, hcm_stat_prefix)`
   — the third arg is needed because the RBAC stat namespace `http.<hcm_stat_prefix>.rbac.*`
   embeds the HCM's stat_prefix at counter-registration time (vs LocalRateLimit whose
   stat_prefix is a filter-level config field). This is a new precedent for any filter
   whose stat namespace embeds the HCM's stat_prefix at register time. Recorded for
   subagent awareness — NOT a SPEC drift. Lock-in #5 + #15.

6. **Empirical-verification body-bytes correction per ADR-0034.** SPEC §2.2 + §3 D8.1
   + §5.9 project the 403 body bytes as `"RBAC: access denied\n"` (20 bytes). Per the
   §6.2 empirical verification at THIS state-2 PLAN-write, upstream Envoy v1.33 emits
   the 403 body as `"RBAC: access denied"` (19 bytes; NO trailing newline). **ADR-0034
   lands at THIS state-2 PLAN-write commit** per SPEC §7 option A recommended posture.
   PLAN lock-in #14 locks the production-code shape; SPEC.md gets 3 inline edits
   ratifying the revised body. The Task 5 fixture + Task 7 backstop assertion shapes
   adopt the 19-byte body directly. Lock-in #13 + #14 + #41.

7. **Stats namespace + header-set §6.2 verifications MATCH SPEC projections.** Per
   the same Docker run: stats namespace is `http.ingress_http.rbac.{allowed,denied}`
   (matches SPEC §2.1 + §6.5); 403 header set under harness `Connection: close`
   framing is 5 headers `{content-length, content-type, date, server, connection}`
   (matches SPEC §2.2 + §5.9). Upstream additionally emits `shadow_allowed` +
   `shadow_denied` at 0 unconditionally even when shadow_rules is unconfigured; phase-10
   registers only the 2 primary counters since shadow_rules defers per SPEC §4 (the
   differential fixture does not scrape RBAC stats so the 2-vs-4 name-set divergence
   is not exercised). **No SPEC revision needed for (a) or (c).**

### Architecture-decision lock-ins (46 — see PLAN.md §2)

Per `feedback_pick_recommendation` ("always pick the recommended option; do not
ask"), 46 lock-ins recorded in the PLAN's lock-in table (§2). Grouped by topic for
in-execution lookup:

- **#1-#2** — module placement + zero new path-deps (no Cargo.toml edits).
- **#3-#10** — RbacFilter struct shape + RuntimeAction/RuntimePolicy + recursive
  evaluator shape + short-circuit semantics + decision computation per SPEC §5.6.
- **#11-#12** — decode/encode method semantics + counter-increment discipline.
- **#13-#14** — 403 synth response shape + body bytes locked per ADR-0034.
- **#15** — counter registration namespace `http.<hcm_stat_prefix>.rbac.{allowed,denied}`.
- **#16-#25** — envoy-config schema (RbacConfig + Rules + Action + default_action
  + Policy + Permission + PermissionSet + Principal + PrincipalSet + BTreeMap
  deterministic iteration).
- **#26-#29** — validator (6 new ConfigError variants + RBAC_TREE_MAX_DEPTH const +
  validate_rbac_config sub-validator + validator dispatch arm).
- **#30** — HttpFilterTypedConfig::Rbac variant.
- **#31** — D5 + D7 + D6 task organization (D6 + D7.1 co-located at Task 3 per SPEC §6.6;
  D5 + D7.2 co-located at Task 4 per SPEC §6.3).
- **#32** — D5 in-place amendment shape (NOT superseding ADR-0034 per SPEC §7 option D
  recommended posture).
- **#33-#35** — D8.1 fixture 0017 shape (bootstrap; 4-probe burst; per-probe
  request_headers harness extension).
- **#36** — D7.1 BEHAVIOR_CONTRACT row landing cadence (2 stat-name rows at Task 3;
  no Header allow-list row needed per SPEC §2.2).
- **#37** — D7.2 ADR-0033 amendment landing co-located with D5 at Task 4.
- **#38** — D8.2 fuzz corpus seed (same-commit `.gitignore` + SUCCESS-array edit per
  09 Task 6 follow-up lesson).
- **#39-#40** — D8.3 in-process backstop with 09 REVIEW M3 kill_on_drop discipline +
  direct code-spot-check of 07.2/08.2 backstop precedents required.
- **#41** — ADR landings: ONE (ADR-0034 option A inline at state-2; ledger head
  advances `ADR-0033 → ADR-0034`).
- **#42** — split-gate verdict (single-phase; no split; accept up to ~+15% drift).
- **#43** — subagent-driven execution at state 3.
- **#44** — PROGRESS.md skeleton + Task 1 preamble land alongside PLAN.md (this
  commit).
- **#45** — Cargo.lock cadence (empty diff expected — zero new deps).
- **#46** — `#![forbid(unsafe_code)]` posture (inherited from crate root).

Full text + rationale per lock-in lives in PLAN.md §2. PROGRESS sub-sections at
state-3 reference lock-ins by `#NN` rather than re-explaining.

### PLAN-write deviations beyond the SPEC corrections (0)

None beyond the 7 SPEC corrections above. Unlike phase-09 (which deviated on schema
struct renames at lock-in #20), phase-10's lock-ins mirror the SPEC's projected type
names directly (`RbacConfig`, `Rules`, `Action`, `Policy`, `Permission`, `PermissionSet`,
`Principal`, `PrincipalSet` per SPEC §3 D1 verbatim).

### Carryforward dispositions

| ID | Severity | Item | Disposition at 10 |
|---|---|---|---|
| **09 REVIEW M2** | Minor | H2 HCM filter-synth header decoration gap + ADR-0033 Consequences misrepresentation | **PROJECTED-CLOSE at Task 4 (D5).** ADR-0033 Consequences §iii(c)-end amendment per preferred close shape (a); ~10 LoC docs-only edit. The chain 09 → 10 ends. |
| **09 REVIEW M3** | Minor | Task 7 in-process backstop subprocess discipline regression from 07.2/08.2 precedents (`std::process::Command` instead of `tokio::process::Command + kill_on_drop`) | **PROJECTED-CLOSE at Task 7 (D8.3).** Phase-10's Task 7 backstop adopts `tokio::process::Command + .kill_on_drop(true) + Stdio::null()` directly per SPEC §6.4 + PLAN lock-in #39. Direct code-spot-check of 07.2/08.2 precedents required per lock-in #40. The chain 09 → 10 ends. |
| **09 REVIEW M1** | Minor | Token-bucket CAS-shape race on refill path | **Carry forward indefinitely.** Not engaged by RBAC (no token bucket). |
| **09 REVIEW M4 / M5** | Minor | (CLOSED at phase-09 state-6 commit `518140c` via fold-in) | Already CLOSED; not engaged by phase 10. Recorded here for completeness. |
| **09 REVIEW D1 / D2** | Doc | ADR-0033 fictional `RequestPath::SynthFromEncode` references; Cargo.lock cadence refinement | **Carry forward indefinitely.** Neither engaged. |
| **09 REVIEW T1 / T2 / T3** | Test/audit | parse_duration silent `+` acceptance; torture test refill-path gap; Task 3 brittle assertion | **Carry forward indefinitely.** None engaged by phase-10's surface. |
| **08.1 REVIEW M3** | Minor | Forward-looking `Arc<BTreeMap<...>>` on `command_line_options` | **Carry forward indefinitely.** Not engaged. |
| **08.2 REVIEW M1-M8** | Minor | Various code-quality / doc-polish items | **Carry forward indefinitely.** None engaged by phase-10's surface. |
| **08.2 REVIEW T1-T3** | Minor | Test / audit-trail polish | **Carry forward indefinitely.** Not engaged. |
| **08.2 REVIEW D1-D5** | Doc | (CLOSED at 08.2 state-6 close-out commit `304ce98` via fold-in) | Already CLOSED; chain ended before phase 09. Recorded for completeness. |
| **07.2 REVIEW M1** | Minor | (CLOSED at phase-09 Task 4 commit `78128f4`) | Already CLOSED in 09. Chain 07.2 → 09 ended; nothing carries forward to 10. |
| **07.2 REVIEW M2 / M3** | Minor | `apply_mutations` Overwrite O(n²) YAGNI / fixture-0013 `expected_body` coupling | **Carry forward indefinitely.** Phase 10 does NOT touch `header_mutation.rs`. |
| **06.3 REVIEW I2** | Important | Synthetic 5xx backend + 4-class `pre_requests` deferred | **Carry forward indefinitely.** Upstream-robustness family is the natural close site. Not engaged. |
| **06.2 REVIEW M1 / M2 / M4 / M5** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **06.1 REVIEW M2 / M3 / M5 / M6** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **05.3 REVIEW I2** | Important | Typed-error chain dissolution at H2 dispatch site | **Carry forward indefinitely.** Not engaged. |
| **05.2 REVIEW I1 / I2 / I3** | Important | Various | **Carry forward indefinitely.** Not engaged. |
| **04.1 REVIEW M5 / M9** | Minor | Cargo.lock cadence ratification ADR | **Carry forward unchanged.** Phase 10 introduces zero new top-level Cargo deps; zero new workspace path-deps. The cadence pick stays unforced. |
| **04.1 REVIEW M-claim / M1 / M2 / M4 / M7** | Minor | Various | **Carry forward indefinitely.** Not engaged. |
| **02.2 REVIEW M1** | Minor | `*EchoBackend::Drop` polling loop blocks on `std::thread::sleep` | **Carry forward unchanged.** Phase 10's fixture 0017 uses direct_response (no Echo backend); the chain continues unchanged. |
| **Phase-00 I3** | — | SIGKILL → SIGTERM graceful termination of subject subprocess (`nix` crate deferral) | **Carry forward unchanged.** Phase 10's backstop uses `tokio::process::Command + kill_on_drop(true)` per PLAN lock-in #39 (NOT `nix`); the carryforward continues unchanged. |

### State-3 entry routing

The next session reads STATE.md, sees `state 2-complete / state-3-next (PLAN.md
landed; first task commit pending)` + Next expected skill
`superpowers:subagent-driven-development` (per `feedback_execution_style`), and
dispatches Task 1 per the PLAN.

---

## Tasks 1-8

_(Per-task `### Task N — <name>` subsections append at state-3 task commits per the
06.x / 07.x / 08.x / 09 cadence. State-2 commit lands this skeleton only.)_

### Task 1 — D1 envoy-config schema + D2 validator (co-located)

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `55abc61` — `phase 10: state-2 standalone PLAN.md [ADR-0034]`.

**Work summary.** Landed the RBAC envoy-config schema + parse-time validator per
PLAN Task 1 (SPEC §3 D1 + D2). The schema adds 8 new items (`RbacConfig`,
`Rules`, `Action`, `Policy`, `Permission`, `PermissionSet`, `Principal`,
`PrincipalSet`) + 1 new `HttpFilterTypedConfig` variant (`Rbac(RbacConfig)`) +
1 `default_action` helper + 1 `pub(crate) const RBAC_TREE_MAX_DEPTH: u32 = 16`.
The validator adds 6 new `ConfigError` variants (`EmptyRbacPolicies`,
`EmptyRbacPolicyPermissions`, `EmptyRbacPolicyPrincipals`, `EmptyRbacPermissionSet`,
`EmptyRbacPrincipalSet`, `RbacTreeTooDeep`) + 1 new dispatch arm in
`validate_http_filters` (the 4th, after Router / HeaderMutation / LocalRateLimit) +
3 new sub-validators (`validate_rbac_config` + `validate_permission_tree` +
`validate_principal_tree`). 14 unit tests landed under a new `rbac_tests`
submodule beneath the existing `local_rate_limit_tests` peer. A transient bridge
arm in `crates/envoy-filter/src/instance.rs::build` returns
`FilterError::UnsupportedFilterType` for the new `Rbac` variant during the
Tasks 1-3 interim; Task 4 replaces it with the proper
`HttpFilterInstance::Rbac(RbacFilter)` dispatch.

**Files modified (4):**
- `crates/envoy-config/src/lib.rs` — 6 new `ConfigError` variants
  (`EmptyRbacPolicies`, `EmptyRbacPolicyPermissions`, `EmptyRbacPolicyPrincipals`,
  `EmptyRbacPermissionSet`, `EmptyRbacPrincipalSet`, `RbacTreeTooDeep`); 8 new
  `pub use bootstrap::{...}` re-exports (`Action`, `Permission`, `PermissionSet`,
  `Policy`, `Principal`, `PrincipalSet`, `RbacConfig`, `Rules`) placed
  alphabetically within the existing block. `RBAC_TREE_MAX_DEPTH` deliberately
  NOT re-exported (per lock-in #27 it's `pub(crate)`).
- `crates/envoy-config/src/bootstrap.rs` — new `HttpFilterTypedConfig::Rbac`
  variant; 8 new schema definitions (`RbacConfig`, `Rules`, `Action`, `Policy`,
  `Permission` + hand-rolled `Deserialize`, `PermissionSet`, `Principal` +
  hand-rolled `Deserialize`, `PrincipalSet`); `default_action` helper; the
  `pub(crate) const RBAC_TREE_MAX_DEPTH: u32 = 16` constant; new match arm on
  `validate_http_filters`; 3 new sub-validator functions
  (`validate_rbac_config`, `validate_permission_tree`, `validate_principal_tree`);
  14 new unit tests in the new `rbac_tests` submodule under the existing
  `mod tests` block (immediately after `local_rate_limit_tests`).
- `crates/envoy-filter/src/instance.rs` — cross-crate bridge arm: the new
  `HttpFilterTypedConfig::Rbac` variant must be handled in the
  `HttpFilterInstance::build` match (otherwise non-exhaustive match breaks the
  workspace build). The interim arm returns `FilterError::UnsupportedFilterType`;
  Task 4 replaces it with the proper `HttpFilterInstance::Rbac` dispatch. A
  comment in the source explains the deferral.
- `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` — this subsection
  (per-task PROGRESS cadence).

**Tests landed (14 new; 225 → 239 in envoy-config lib).**
1. `deserialize_rbac_minimal_allow_succeeds`
2. `deserialize_rbac_default_action_is_allow`
3. `deserialize_rbac_deny_action_succeeds`
4. `deserialize_rbac_rejects_unknown_field`
5. `deserialize_rbac_permission_and_or_not_combinators_succeed`
6. `deserialize_rbac_principal_and_or_not_combinators_succeed`
7. `validate_accepts_rbac_followed_by_router`
8. `validate_rejects_empty_policies`
9. `validate_rejects_empty_policy_permissions`
10. `validate_rejects_empty_policy_principals`
11. `validate_rejects_empty_permission_set`
12. `validate_rejects_empty_principal_set`
13. `validate_rejects_tree_too_deep`
14. `validate_rejects_rbac_with_wrong_name`

(The PLAN brief named "13 unit tests" + 3 helpers as the verbatim canonical
block; the actual `#[test]`-annotated function count in the verbatim block is
**14** — the brief's narrative count was off by one. The verbatim block was
landed without semantic modification; the 14-fn count is canonical.)

**LoC delta (production + tests; doc-comments included in the raw counts
below).** Per `git diff --cached --shortstat HEAD` at this commit: 3 files
changed, 689 insertions(+), 3 deletions(-). Breakdown:
- `crates/envoy-config/src/bootstrap.rs`: +625 lines (schema + 2 hand-rolled
  Deserialize impls + 3 sub-validators + 14 unit tests + doc-comments). Of these,
  ~269 lines are tests in `rbac_tests`; the remaining ~356 are production
  (schema + Deserialize impls + validators + dispatch arm extension).
- `crates/envoy-config/src/lib.rs`: +56 / -2 (50 production lines — 6 new
  `ConfigError` variants + doc-comments; 8 new re-exports interleaved).
- `crates/envoy-filter/src/instance.rs`: +9 / -1 (bridge arm + comment).

Production total ~+415 LoC (includes ~70 LoC of hand-rolled Deserialize impls
not anticipated in the PLAN); tests ~+269 LoC; combined ~+684 LoC. PLAN §3's
Task-1 projection was ~210 production + ~250 tests = ~465. The +47% overshoot
is dominated by (a) the two hand-rolled `Deserialize` impls (~70 LoC each in
combined code+docs ~ +120 LoC) required by the discovered-at-task-time `serde_yaml`
limitation (see deviation #1 below) and (b) doc-comments on every public schema
item per the established envoy-config discipline. Excluding doc-comments the
overshoot would be smaller; still material.

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain:
- `cargo fmt --all -- --check` — PASS (after one `cargo fmt --all` to fix
  thiserror `#[error(...)]` line-length wraps on three of the 6 new variants).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  (0 warnings; no clippy lints triggered by the new code after the
  unused-imports pruning in `rbac_tests`).
- `cargo build --workspace --all-targets` — PASS.
- `cargo test --workspace` — PASS (760 passed, 0 failed, 2 ignored; +31 vs the
  phase-09 Task 1 snapshot of 729 — the delta covers the 14 new rbac_tests plus
  the test growth that landed across phase-09 Tasks 2-8 between the two
  snapshots).
- `cargo deny check` — PASS (advisories ok, bans ok, licenses ok, sources ok;
  pre-existing unencountered-license warnings unchanged).

**Per-task deviations from PLAN (1; the 8th discovered-at-task-time SPEC
correction — extends the PLAN §1 list of 7).**

1. **Discovered-at-task-time PLAN-write SPEC correction (8th — extends the
   PLAN §1 list of 7).** PLAN Step 4 schema decls for `Permission` and
   `Principal` use `#[derive(Debug, Clone, Deserialize, PartialEq)]` with
   variant-level `#[serde(rename = "any" / "header" / "and_rules" / ...)]`
   annotations, relying on serde's externally-tagged enum representation to
   parse YAML maps like `{any: true}`, `{header: {...}}`, `{and_rules: {rules:
   [...]}}` (which is how upstream Envoy's bootstrap YAML emits the RBAC tree).
   At task time the resulting `cargo test rbac_tests` run fails 6 of the 6
   deserialization tests with `serde_yaml`'s
   `Error("invalid type: map, expected a YAML tag starting with '!'", line: N,
   column: N)`. Investigation confirms this is a **known `serde_yaml` 0.9
   limitation**: externally-tagged enums via plain YAML maps are not
   supported — `serde_yaml::Deserializer::deserialize_enum` requires YAML's
   native `!Tag` syntax (not `!any true` / `!and_rules {rules: ...}`, which is
   not how Envoy bootstraps are written). The codebase's pre-existing
   convention for this exact problem is **hand-rolled `Deserialize`** mirroring
   the 04.2 `HeaderMatcher` / `StringMatcher` precedent (which use the same
   map-of-one-key-with-known-discriminator shape). **Resolution:** add
   hand-rolled `Deserialize` impls for both `Permission` and `Principal` that
   visit a `MapAccess` with exactly one key and dispatch to the matching
   variant (including the recursive `NotRule(Box<Permission>)` /
   `NotId(Box<Principal>)` arms). The variant-level `#[serde(rename = "...")]`
   attrs are RETAINED for `Serialize`-derive use (derive-Serialize emits the
   correct `{"any":true}` JSON shape; YAML serialization via derive would emit
   `!Tag` syntax which is asymmetric to the map-form Deserialize, but YAML
   emission of `Permission` / `Principal` is not exercised by any code path —
   the serialize_roundtrip_tests round-trip via JSON only, and no production
   path serializes these types to YAML). The `#[serde(deny_unknown_fields)]`
   attr on the two enums was dropped because (a) it conflicts with hand-rolled
   `Deserialize` (which already enforces single-key + known-key via explicit
   error returns) and (b) the existing `HeaderMatcher` / `StringMatcher`
   precedent uses the same shape (hand-rolled Deserialize + `unknown_field`
   error in the visitor). The 6 affected schema struct decls (`RbacConfig`,
   `Rules`, `Policy`, `Action`, `PermissionSet`, `PrincipalSet`) retain
   `#[serde(deny_unknown_fields)]` per PLAN. Additionally, all 8 new schema
   items were given a `Serialize` derive (PLAN omits it from the decls); the
   `HttpFilterTypedConfig` enum derives `Serialize` so its new `Rbac(RbacConfig)`
   variant cannot embed a non-`Serialize` type without breaking the parent
   enum's `Serialize` derive. Net surface: 2 hand-rolled `impl Deserialize`
   blocks (~70 LoC combined) + `Serialize` derives added to 8 schema items
   (zero-line cost beyond the derive list). Both adjustments preserve the
   PLAN's wire-format intent (the externally-tagged YAML shape) and the
   established codebase conventions. Sub-action: 4 unused imports
   (`HeaderMatcher`, `HeaderMatcherMode`, `StringMatcher`, `StringMatcherMode`)
   were pruned from the `rbac_tests` submodule's `use` block — they were
   present in the PLAN-verbatim test block but the tests' YAML inputs
   deserialize *through* these types without referencing them by name, so
   under `-D warnings` they would fail clippy. The PLAN-named test fn count
   (13) is off by one from the verbatim block's actual `#[test]`-annotated
   function count (14); the brief explicitly flags this as a count-narration
   drift, not a content drift, so the 14-fn block was landed verbatim.

**Carryforward dispositions unchanged.** The 09 REVIEW M2 + M3 projected-close
sites both target Tasks 4 + 7 respectively; not engaged at Task 1.

**STATE.md / ROADMAP.md / BEHAVIOR_CONTRACT.md / DECISIONS.md / ENVOY_TARGET.md /
rust-toolchain.toml diffs at this commit:** None (per the state-3 per-task
cadence — these all stayed at state-2 values).

### Task 2 — D3 hand-rolled recursive tree-walk evaluator

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `3fbe9f5` — `phase 10: task 1 — D1 envoy-config schema + D2 validator`.

**Work summary.** Landed the pure-compute Permission/Principal recursive
tree-walk evaluator per PLAN Task 2 (SPEC §3 D3) as a new module
`crates/envoy-filter/src/rbac.rs`. The module ships 2 runtime enums
(`RuntimePermission`, `RuntimePrincipal`) + 2 synchronous recursive
evaluators (`eval_permission`, `eval_principal`) + 12 unit tests. Per PLAN
lock-in #6, the wire-form `PermissionSet { rules: Vec<Permission> }` wrapper
is flattened on the runtime enum to a direct `Vec<RuntimePermission>` on
`AndRules` / `OrRules` (and symmetrically for `PrincipalSet` → `AndIds` /
`OrIds`); `Box` indirection appears only on `NotRule` / `NotId` per PLAN
lock-ins #6 + #7. The evaluator short-circuits via `Iterator::all`
(conjunction) and `Iterator::any` (disjunction) per PLAN lock-in #9; `Any(b)`
returns `*b`; `Header(m)` delegates to the existing
`HeaderMatcher::matches(&[(String, String)])` predicate landed in 04.2. No
async, no `dyn`, no I/O — pure recursive descent over the borrowed tree.
The `RbacFilter` struct + `build_from_config` + `decode_headers` glue +
stats wiring are deferred to Task 3 per the PLAN scope split; Task 1's
transient bridge arm in `crates/envoy-filter/src/instance.rs` stays as-is.

**Files modified (3):**
- `crates/envoy-filter/src/lib.rs` — one-line addition of `pub mod rbac;`
  in alphabetical position between `pub mod pipeline;` and `pub mod router;`.
  No `pub use rbac::RbacFilter;` re-export — that ships at Task 3 per the
  PLAN scope split.
- `crates/envoy-filter/src/rbac.rs` — new module (243 LoC total). Module
  doc-comment, 2 `pub(crate)` enums with per-variant doc-comments, 2
  `pub(crate)` recursive evaluator fns with per-fn doc-comments, and a
  `#[cfg(test)] mod tests` block with the 12 PLAN-canonical unit tests + 2
  helper fns (`req_with`, `header_matcher_exact`). The crate-root
  `#![forbid(unsafe_code)]` at `lib.rs:1` is inherited per PLAN lock-in
  #46 — no per-module override, consistent with `header_mutation.rs` and
  `local_rate_limit.rs`.
- `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` — this
  subsection (per-task PROGRESS cadence; replaces the state-2 skeleton's
  `_(Pending Task 2 dispatch.)_` placeholder).

**Tests landed (12 new; 760 → 772 in workspace).** All under
`rbac::tests` per the PLAN-canonical naming:
1. `any_true_permission_matches`
2. `any_false_permission_does_not_match`
3. `header_permission_matches_when_value_equals`
4. `header_permission_does_not_match_when_value_differs`
5. `header_permission_does_not_match_when_header_absent`
6. `and_rules_short_circuits_on_first_false`
7. `and_rules_all_true_matches`
8. `or_rules_short_circuits_on_first_true`
9. `or_rules_all_false_does_not_match`
10. `not_rule_negates_inner`
11. `nested_and_or_not_evaluates_correctly`
12. `principal_evaluator_mirrors_permission_evaluator`

The permission side exercises all 5 variants (`Any` ×2 polarity, `Header` ×3
present/differs/absent, `AndRules` ×2 short-circuit + all-true, `OrRules` ×2
short-circuit + all-false, `NotRule` ×1 dual-polarity); the principal side
exercises `OrIds` + `Header` only — the symmetric-evaluator coverage
discipline relies on Task 3's `RbacFilter` tests to exercise the remaining
Principal variants end-to-end. See "Per-task deviations" #2 below.

**LoC delta (production + tests; doc-comments included).** Per
`git diff --cached --shortstat HEAD` at this commit: 3 files changed,
~244 insertions (1 in `lib.rs`; 243 in the new `rbac.rs`; PROGRESS.md
changes excluded from production-LoC count per cadence). Breakdown of
`rbac.rs` by region (split at the `#[cfg(test)]` marker):
- Production: ~107 LoC (module doc-comment + 2 enums w/ per-variant
  doc-comments + 2 evaluator fns w/ doc-comments + 4 `#[allow(dead_code)]`
  attrs for the Tasks 2-3 interim — see deviation #1 below).
- Tests: ~136 LoC (12 `#[test]` fns + 2 helper fns + `use` block).

PLAN §3's Task-2 projection was ~60 production + ~140 tests = ~200 LoC.
Production overshoot (~+47 LoC) is dominated by (a) the 4 PLAN-not-anticipated
`#[allow(dead_code)]` attrs with explanatory doc-comments (deviation #1) and
(b) the project-mandated per-`pub(crate)` doc-comments on every enum +
variant + fn (PLAN's verbatim Step-2 block omits these; the envoy-filter
crate convention requires them). Test count matches PLAN exactly (12).

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain:
- `cargo fmt --all -- --check` — PASS (no formatting changes needed; the
  written-from-scratch file matched the project style first try).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  PASS (after the 4 `#[allow(dead_code)]` annotations per deviation #1;
  see that deviation for the production-profile dead-code rationale).
- `cargo build --workspace --all-targets` — PASS.
- `cargo test --workspace` — PASS (772 passed, 0 failed, 2 ignored; +12 vs
  the phase-10 Task 1 snapshot of 760 — exactly the 12 new `rbac::tests`).
- `cargo deny check` — PASS (advisories ok, bans ok, licenses ok, sources
  ok; pre-existing unencountered-license warnings unchanged from Task 1).

**Per-task deviations from PLAN (2).**

1. **Discovered-at-task-time: 4 `#[allow(dead_code)]` attrs needed for the
   Tasks 2-3 interim production-profile build.** PLAN Step 2's verbatim
   code block has no `#[allow(dead_code)]` annotations. Under the test
   profile (which the PLAN's Step 3 `cargo test -p envoy-filter --lib
   rbac::tests` exercises) the 2 enums + 2 evaluator fns have consumers
   (the test module), so `cargo test` alone reports clean. But under the
   non-test profile (which Step 4's `cargo clippy --workspace
   --all-targets --all-features -- -D warnings` exercises), the
   `#[cfg(test)] mod tests` block is excluded, leaving the 2 `pub(crate)`
   enums and 2 `pub(crate)` evaluator fns with zero consumers — clippy
   errors with `dead_code` (promoted from warn to error by `-D warnings`).
   The PLAN-named-precedent fix is per-item `#[allow(dead_code)]` mirroring
   `LocalRateLimitFilter::stat_prefix` at `crates/envoy-filter/src/local_rate_limit.rs:49`.
   Applied to: `RuntimePermission` enum, `RuntimePrincipal` enum,
   `eval_permission` fn, `eval_principal` fn. Each carries an explanatory
   doc-comment paragraph naming the Task 3 site that will retire the allow
   (`RbacFilter::build_from_config` constructs all variants; the two eval
   fns get called from `RbacFilter::decode_headers`). Net surface: 4 attrs
   + ~10 lines of explanatory doc-comments. Zero behavioral change. Could
   alternatively have been a single `#![allow(dead_code)]` at the module
   root, but per-item is the established `local_rate_limit.rs` precedent
   and makes the Task-3 cleanup more granular (the allows go away
   individually as each item gains a consumer).

2. **No PLAN deviation; recorded for the reviewer's context: principal-side
   test coverage is intentionally narrower than permission-side.** Per the
   PLAN-canonical 12-test list, the Principal side has exactly one test
   (`principal_evaluator_mirrors_permission_evaluator`) exercising
   `OrIds(Vec<RuntimePrincipal::Header>)`. The remaining Principal
   variants (`Any`, `AndIds`, `NotId`) get coverage at Task 3 via the
   `RbacFilter`-level end-to-end tests against the full policy tree.
   This is structural symmetry trust + downstream-test coverage, not a
   coverage gap — and it's explicitly what the PLAN authored. Calling it
   out so the reviewer doesn't double-count it as "missing tests" at
   spec-compliance review.

The 4 PLAN-named corrections (PLAN-narrated Corrections 1-3 + the
controller-discovered Correction 4 to the `body` field type at
`crates/envoy-filter/src/types.rs:34`) were applied at write-time without
incident. The dispatch brief named them all; no surprises.

**Carryforward dispositions unchanged.** The 09 REVIEW M2 + M3
projected-close sites both target Tasks 4 + 7 respectively; not engaged at
Task 2.

**STATE.md / ROADMAP.md / BEHAVIOR_CONTRACT.md / DECISIONS.md /
ENVOY_TARGET.md / rust-toolchain.toml diffs at this commit:** None (per
the state-3 per-task cadence — these all stay at state-2 values until
Task 4 / Task 8).

### Task 3 — D3 RbacFilter runtime + D6 stats wiring + D7.1 2 contract rows

**Commit:** `da32137` — `phase 10: task 3 — D3 RbacFilter runtime + D6 stats + D7.1 2 contract rows`
**Parent:** `14a842c` — `phase 10: task 2 — D3 RBAC recursive tree-walk evaluator`.

**Work summary.** Landed the `RbacFilter` runtime struct + `build_from_config` +
`decode_headers` + `encode_headers` per PLAN Task 3 (SPEC §3 D3 extension, D6
stats wiring, D7.1 contract rows). The module already had `RuntimePermission`,
`RuntimePrincipal`, `eval_permission`, `eval_principal` from Task 2; this task
extends it with the filter struct shape, lowering helpers, and the full
TDD-driven test suite.

`RbacFilter` holds 4 fields: `action: RuntimeAction`, `policies:
Arc<Vec<RuntimePolicy>>`, `allowed_counter: Arc<Counter>`, and `denied_counter:
Arc<Counter>`. `build_from_config` lowers an `envoy_config::RbacConfig` into
the runtime struct via the `lower_permission` + `lower_principal` helpers
(which flatten the `PermissionSet { rules }` / `PrincipalSet { ids }` wrappers
per PLAN lock-ins #6 + #7) and registers 2 stat counters under
`http.{hcm_stat_prefix}.rbac.{allowed,denied}` via `StatsRegistry::register_counter`
per PLAN lock-in #15.

`decode_headers` implements the SPEC §5.6 decision matrix: iterates `policies`
in `BTreeMap` alphabetical order, short-circuits on first matching policy
(permission AND principal both matching), and resolves the `(action, match)`
combination via `matches!(...)` to either `Decision::Continue` (increment
`allowed_counter`) or `Decision::StopAndSend(FilterResponse { status: 403,
reason: Some("Forbidden"), headers: vec![], body: Bytes::from_static(b"RBAC:
access denied") })` (increment `denied_counter`). `encode_headers` is a no-op
per SPEC §5.4. Two `BEHAVIOR_CONTRACT.md` rows added under a new "**10 entries
(RBAC filter):**" subheading per PLAN lock-in #36. `pub use rbac::RbacFilter;`
re-exported from `lib.rs` per alphabetical position.

The 4 `#[allow(dead_code)]` attrs from Task 2 were removed and replaced with a
new set of per-item attrs covering the Tasks 3-4 interim: the whole `RbacFilter`
block has no production-profile construction site until Task 4 wires
`HttpFilterInstance::Rbac(RbacFilter::build_from_config(...))` in `instance.rs`.
See "Per-task deviations" below for detail on this deviation from Correction 4's
ideal expectation.

**Files modified (4):**
- `crates/envoy-filter/src/rbac.rs` — extended with `RbacFilter` struct +
  `RuntimeAction` enum + `RuntimePolicy` struct + `impl RbacFilter` block
  (`build_from_config`, `decode_headers`, `encode_headers`) + `lower_permission`
  + `lower_principal` helper fns + 6 new unit tests; removed Task-2's 4
  `#[allow(dead_code)]` attrs and their explanatory doc-comment paragraphs;
  added new interim attrs covering the Tasks 3-4 production-profile dead-code.
- `crates/envoy-filter/src/lib.rs` — added `pub use rbac::RbacFilter;`
  alphabetically between `pub use pipeline::{Decision, FilterPipeline};` and
  `pub use router::RouterTerminus;`.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — inserted 2 new "Stat-name mapping"
  rows under new "**10 entries (RBAC filter):**" subheading after the 09 entries
  table per PLAN lock-in #36.
- `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` — this subsection
  (per-task PROGRESS cadence; replaces the state-2 skeleton's
  `_(Pending Task 3 dispatch.)_` placeholder).

**Tests landed (6 new; 772 → 778 in workspace test count).** All under
`rbac::tests` per PLAN-canonical naming:
1. `build_from_config_allow_with_header_principal_creates_filter`
2. `decode_headers_allow_action_no_header_returns_deny`
3. `decode_headers_allow_action_with_header_returns_continue`
4. `decode_headers_deny_action_inverts_semantics`
5. `decode_headers_counters_increment_correctly`
6. `encode_headers_is_noop`

**LoC delta (production + tests; doc-comments included).** Per
`git diff HEAD -- crates/envoy-filter/src/rbac.rs crates/envoy-filter/src/lib.rs
docs/envoy-rust/BEHAVIOR_CONTRACT.md | diffstat` at this commit:
3 files changed, 409 insertions(+), 19 deletions(-) (the deletions are the
Task-2 `#[allow(dead_code)]` attrs + explanatory doc-comment paragraphs removed
per Correction 4, plus the re-org of the module header). Net `rbac.rs` delta:
~+390 LoC. Breakdown of new production code: `RbacFilter` struct + `RuntimeAction`
+ `RuntimePolicy` + `impl RbacFilter` block + 2 lowering fns ≈ ~130 LoC;
tests ≈ ~190 LoC; interim `#[allow(dead_code)]` attrs + updated doc-comment
lines ≈ ~30 LoC; `lib.rs` +1 LoC; `BEHAVIOR_CONTRACT.md` +7 LoC. PLAN §3's
Task-3 projection was ~130 production + ~135 tests = ~265 LoC; the ~+65 LoC
overshoot is dominated by (a) per-item `#[allow(dead_code)]` attrs for the
Tasks 3-4 interim (not anticipated by PLAN Correction 4's ideal expectation)
and (b) per-`pub(crate)` doc-comments per crate convention.

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain:
- `cargo fmt --all -- --check` — PASS (after one `cargo fmt --all` to fix the
  `RbacFilter::build_from_config(...).expect("build succeeds")` line-length
  wrap in the first test).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS
  (0 warnings; with per-item `#[allow(dead_code)]` attrs on the Tasks 3-4
  interim items — see "Per-task deviations" #1).
- `cargo build --workspace --all-targets` — PASS.
- `cargo test --workspace` — PASS (778 passed, 0 failed, 2 ignored; +6 vs the
  phase-10 Task 2 snapshot of 772 — exactly the 6 new `rbac::tests`).
- `cargo deny check` — PASS (advisories ok, bans ok, licenses ok, sources ok;
  pre-existing unencountered-license warnings unchanged from Task 2).

**Per-task deviations from PLAN (4 — the 4 inline corrections from the dispatch brief).**

1. **Correction 1 applied: `registry.counter(...)` → `registry.register_counter(...)`
   in test `decode_headers_counters_increment_correctly`.** PLAN Step 1 test
   (lines 1469-1474) called `.counter(name)` — a method that does not exist on
   `StatsRegistry`. The canonical API at
   `crates/envoy-stats/src/registry.rs:45` is `pub fn register_counter(&self,
   name: &str) -> Result<Arc<Counter>, StatsError>`. Verified against phase-09
   Task 3 pattern at `crates/envoy-filter/src/local_rate_limit.rs:419-422`.
   Correction applied as specified.

2. **Correction 2 applied: `reason: Some("Forbidden")` (no `.to_string()`).**
   PLAN Step 3 production code (line 1620) wrote `reason: Some("Forbidden".to_string())`.
   `crates/envoy-filter/src/types.rs:45` declares `pub reason: Option<&'static
   str>` — NOT `Option<String>`. Verified against phase-09 precedent at
   `crates/envoy-filter/src/local_rate_limit.rs:161` (`reason: Some("Too Many
   Requests")`). Correction applied as specified.

3. **Correction 3 applied: `FilterError::InvalidConfig { message: ... }` reused,
   NOT `FilterError::StatsRegistration`.** PLAN Step 3 production code (lines
   1582-1593) used a `FilterError::StatsRegistration` variant that does not exist.
   At HEAD `14a842c`, `crates/envoy-filter/src/error.rs` has exactly 5 variants:
   `EmptyChain`, `RouterNotTerminal`, `DuplicateRouter`, `UnsupportedFilterType`,
   `InvalidConfig`. The canonical pattern is at
   `crates/envoy-filter/src/local_rate_limit.rs:92-120`:
   `.map_err(|e| FilterError::InvalidConfig { message: format!("StatsRegistry:
   {e}") })?`. Correction applied as specified; no new variant added.

4. **Correction 4 partially applied: 4 Task-2 `#[allow(dead_code)]` attrs
   removed; replaced with new Tasks 3-4 interim attrs.** The dispatch brief
   specified removing all 4 Task-2 attrs and expected all 4 items to have live
   consumers after Task 3's wiring. This proved incorrect: the entire `RbacFilter`
   block (`RuntimePermission`, `RuntimePrincipal`, `eval_permission`,
   `eval_principal`, `RbacFilter` struct fields, `RuntimeAction` variants,
   `RuntimePolicy` fields, `build_from_config`, `decode_headers`, `encode_headers`,
   `lower_permission`, `lower_principal`) has no production-profile construction
   site because `crates/envoy-filter/src/instance.rs` still returns
   `FilterError::UnsupportedFilterType` for the `Rbac` variant (the Task-1
   bridge arm). Task 4 will replace that stub with
   `HttpFilterInstance::Rbac(RbacFilter::build_from_config(...))`, making all
   items live. The Task-2 `#[allow(dead_code)]` attrs (plus their
   dead-code-explanation doc-comment paragraphs) were removed as specified; NEW
   per-item attrs covering the Tasks 3-4 interim were added to the 10 affected
   items (`RuntimePermission`, `RuntimePrincipal`, `eval_permission`,
   `eval_principal`, `RbacFilter` struct, `RuntimeAction`, `RuntimePolicy`,
   `build_from_config`, `decode_headers`, `encode_headers`, `lower_permission`,
   `lower_principal`). The `RuntimePolicy::name` field retains its permanent
   `#[allow(dead_code)]` attr (per the struct's "retained for future
   tracing::debug! diagnostics" rationale — this attr is NOT an interim and
   should survive Task 4's wiring). Correction 4 is partially satisfied:
   old interim attrs removed, new interim attrs added, permanent attr preserved.

**Carryforward dispositions unchanged.** The 09 REVIEW M2 + M3 projected-close
sites both target Tasks 4 + 7 respectively; not engaged at Task 3.

**STATE.md / ROADMAP.md / DECISIONS.md / ENVOY_TARGET.md / rust-toolchain.toml
diffs at this commit:** None. (BEHAVIOR_CONTRACT.md IS modified at this commit
per PLAN lock-in #36.)

### Task 4 — D4 HttpFilterInstance::Rbac variant + D5 ADR-0033 amendment (closes 09 REVIEW M2)

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `65181c1` — `phase 10: task 3 — record commit SHA da32137 in PROGRESS.md`.

**Work summary.** Landed the D4 `HttpFilterInstance::Rbac(RbacFilter)` variant
+ proper dispatch per PLAN Task 4 (SPEC §3 D4 + D5 + D7.2). Replaced the
Task-1 transient bridge arm in `crates/envoy-filter/src/instance.rs` (which
returned `Err(FilterError::UnsupportedFilterType)` for the `Rbac` typed-config
variant) with the proper construction
`HttpFilterInstance::Rbac(RbacFilter::build_from_config(cfg, registry, hcm_stat_prefix)?)`.
Widened `HttpFilterInstance::build` + `FilterPipeline::build_from_config`
signatures with a new `hcm_stat_prefix: &str` 3rd parameter so the new Rbac
arm can register its 2 stat counters under
`http.{hcm_stat_prefix}.rbac.{allowed,denied}` at build time per PLAN lock-in
#15. Threaded `&cfg.stat_prefix` at the single H1 HCM production call site
(`crates/envoy-http1/src/hcm.rs:185`); H2 reuses the same `Http1HCMConfig`
via re-export per the 09 wiring discipline so no second production call site
exists. The `Rbac` enum variant was placed between `LocalRateLimit` and the
`#[cfg(feature = "test-util")]` block per PLAN lock-in #30; `decode_headers`
+ `encode_headers` dispatch arms were extended symmetrically.

**CLOSES 09 REVIEW M2** at the named site (D5 ADR-0033 Consequences
amendment per preferred close shape (a)). Landed an in-place clarification
paragraph after ADR-0033 Consequences §iii(c) bullet in `DECISIONS.md`
correcting the "naturally inherits via the shared Http1HCMConfig re-export"
claim — empirically the H2 HCM has its own `build_http_response` helper
that does NOT include the standard-header decoration the H1
`decorate_filter_synth_response` helper adds. The amendment names the close
site for the implementation deferral ("next HTTP-filter-family phase
exercising filters bilaterally on H2") and confirms phase 10's RBAC fixture
exercises H1 only (matching the 07.2 + 09 single-codec cadence). Per PLAN
lock-in #32, this is an in-place clarification — DECISIONS.md ledger head
stays at ADR-0034 (no new ADR added). The D7.2 1-sentence cross-ref note
appended at the end of phase-09 PROGRESS's `### Task 4 fixup — H1 HCM
filter-synth header decoration per ADR-0033 (Commit C)` subsection points
forward to this amendment.

**Dead-code-attr retirement.** Removed all 12 of the Task-3 interim
`#[allow(dead_code)]` attrs in `crates/envoy-filter/src/rbac.rs` that
became live once `instance.rs::build` constructs `RbacFilter`: on
`RuntimePermission` enum, `RuntimePrincipal` enum, `eval_permission` fn,
`eval_principal` fn, `RbacFilter` struct, `RuntimeAction` enum,
`RuntimePolicy` struct, `RbacFilter::build_from_config`,
`RbacFilter::decode_headers`, `RbacFilter::encode_headers`,
`lower_permission`, and `lower_principal`. Also pruned the explanatory
"covers the Tasks 3-4 interim" doc-comment paragraphs that accompanied
several of these attrs. The permanent `#[allow(dead_code)]` on
`RuntimePolicy::name` (kept for future `tracing::debug!` diagnostics per
PLAN lock-in #5) was preserved verbatim. Clippy is clean (0 warnings)
after the retirement — no item required keeping its interim attr.

**Signature widening propagation (9 widened call sites — 1 production +
8 tests/test-helpers).**
1. `crates/envoy-http1/src/hcm.rs:185` — production `Http1HCMConfig::from_config` call (adds `&cfg.stat_prefix`).
2. `crates/envoy-http1/src/hcm.rs:1055` — `test_router_only_pipeline()` test helper.
3. `crates/envoy-http1/src/hcm.rs:2773` — `header_mutation_pipeline()` test helper.
4. `crates/envoy-filter/src/header_mutation.rs:250` — `HttpFilterInstance::build` call in `http_filter_instance_build_on_header_mutation_produces_header_mutation_variant`.
5. `crates/envoy-filter/src/header_mutation.rs:433` — `FilterPipeline::build_from_config` in `round_trip_via_filter_pipeline_decode`.
6. `crates/envoy-filter/src/header_mutation.rs:462` — `FilterPipeline::build_from_config` in `iteration_order_on_encode_via_filter_pipeline`.
7. `crates/envoy-filter/src/instance.rs` — `build_router_succeeds` test (3-arg call).
8. `crates/envoy-filter/src/instance.rs` — `build_local_rate_limit_succeeds` test (3-arg call).
9. `crates/envoy-filter/src/pipeline.rs` — all 4 tests
   (`build_from_config_rejects_empty_list`,
   `build_from_config_with_single_router_succeeds`,
   `decode_headers_on_single_router_returns_continue`,
   `encode_headers_on_single_router_returns_continue`) widened to pass `"test_prefix"`.

(Item count is 9 distinct widening sites; pipeline.rs's 4 widenings are
grouped under item 9 because they all sit in the same `mod tests` block.)

**Files modified (8):**
- `crates/envoy-filter/src/instance.rs` — module doc-comment updated
  ("Three" → "Four"); `use crate::rbac::RbacFilter;` import added;
  `Rbac(RbacFilter)` variant added with doc-comment; `build` signature
  widened with `hcm_stat_prefix: &str` parameter + doc-comment paragraph;
  Task-1 transient bridge arm replaced with proper
  `HttpFilterInstance::Rbac(RbacFilter::build_from_config(...)?)`
  construction; `decode_headers` + `encode_headers` dispatch arms extended
  with `Rbac` cases; 2 existing tests widened to 3-arg `build` calls;
  new `build_rbac_succeeds` test added.
- `crates/envoy-filter/src/pipeline.rs` — `build_from_config` signature
  widened with `hcm_stat_prefix: &str` parameter; doc-comment paragraph
  added; 4 test sites widened to pass `"test_prefix"`.
- `crates/envoy-filter/src/rbac.rs` — removed all 12 interim
  `#[allow(dead_code)]` attrs + their explanatory doc-comment paragraphs;
  preserved the permanent `#[allow(dead_code)]` on `RuntimePolicy::name`.
- `crates/envoy-filter/src/header_mutation.rs` — 3 test call sites
  widened (1 `HttpFilterInstance::build` + 2 `FilterPipeline::build_from_config`).
- `crates/envoy-http1/src/hcm.rs` — 1 production call (passes
  `&cfg.stat_prefix`) + 2 test helpers (`test_router_only_pipeline`,
  `header_mutation_pipeline`) widened to pass `"test_prefix"`.
- `docs/envoy-rust/DECISIONS.md` — ADR-0033 Consequences amendment
  (~1 paragraph; in-place clarification per PLAN lock-in #32). Ledger
  head stays at ADR-0034 (NOT a new ADR).
- `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md` —
  1-sentence D7.2 cross-ref note appended at end of `### Task 4 fixup`
  subsection pointing forward to the ADR-0033 amendment.
- `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` — this
  subsection (per-task PROGRESS cadence; replaces the state-2 skeleton's
  `_(Pending Task 4 dispatch.)_` placeholder).

**Tests landed (1 new; 778 → 779 in workspace test count).**
1. `build_rbac_succeeds` (under `instance::tests`) — constructs an
   `HttpFilter` carrying a minimal `RbacConfig` (1 ALLOW policy
   `permissions: [Any(true)]` × `principals: [Any(true)]`) and asserts
   `HttpFilterInstance::build(&hf, &registry, "test_prefix")` returns
   `HttpFilterInstance::Rbac(_)`.

The 6 Task-3 `rbac::tests` continue passing; the 2 widened existing
`instance::tests` (`build_router_succeeds`, `build_local_rate_limit_succeeds`)
continue passing; the 4 widened `pipeline::tests` continue passing; the 3
widened `header_mutation` test sites continue passing; the 2 widened H1
HCM test helpers continue feeding their callers without behavior change.

**LoC delta (production + tests + docs; doc-comments included).** Per
`git diff --stat HEAD` at this commit: 7 files changed, 86 insertions(+),
51 deletions(-). Net `+35 LoC`. Breakdown:
- `instance.rs`: +71 / -16 (variant + dispatch arms + import + doc-comment
  expansion + widened tests + new `build_rbac_succeeds` test).
- `pipeline.rs`: +19 / -8 (signature widening + doc-comment + 4 test sites).
- `rbac.rs`: -27 net (12 attrs removed + several explanatory doc-comment
  paragraphs removed; no production code added).
- `header_mutation.rs`: +9 / -5 (3 test sites widened).
- `hcm.rs`: +7 / -2 (1 production + 2 test helpers widened).
- `DECISIONS.md`: +2 (1 markdown paragraph + 1 blank line).
- `09 PROGRESS.md`: +2 (1 sentence + 1 blank line).

PLAN §3's Task-4 projection was ~30 production + ~15 tests = ~45 LoC
combined (essentially "the variant + dispatch arm + 1 widened test").
The actual delta (+35 net) is on the low side because the dead-code-attr
retirement happens to subtract ~27 LoC, masking the per-call-site
widening growth — a coincidence of opposed-direction edits rather than
overshoot or undershoot. Production code grew exactly per projection;
the larger-than-anticipated test-widening surface (9 sites vs the PLAN's
3-ish narrative) was offset by the attr removals.

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain:
- `cargo fmt --all -- --check` — PASS (after one `cargo fmt --all` to fix a
  single line-wrap drift on the new `build_rbac_succeeds` test's
  `HttpFilterInstance::build(...).expect(...)` chain in `instance.rs`).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  PASS (0 warnings; the 12 interim `#[allow(dead_code)]` attrs were all
  retired without any clippy regrowth; the permanent attr on
  `RuntimePolicy::name` stays).
- `cargo build --workspace --all-targets` — PASS.
- `cargo test --workspace` — PASS (779 passed, 0 failed, 2 ignored; +1 vs
  the phase-10 Task 3 snapshot of 778 — exactly the new `build_rbac_succeeds`
  test).
- `cargo deny check` — PASS (advisories ok, bans ok, licenses ok, sources
  ok; pre-existing unencountered-license warnings unchanged from Task 3).

**Per-task deviations from PLAN (0).** PLAN Task 4's Steps 1-7 landed
verbatim (modulo the order-of-operations: TDD-first the new `build_rbac_succeeds`
test before flipping the signature, then the signature widening + bridge-arm
replacement + dispatch arms + production threading + dead-code-attr
retirement + ADR-0033 amendment + phase-09 cross-ref). The 6th opportunistic
test-import-hoist refactor in `rbac.rs::tests` (Task 3 code-quality review's
Minor #1) was deliberately skipped — left as Task 8 cleanup per the PLAN's
explicit "opportunistic, NOT required" guidance and to keep this commit
narrowly scoped to D4 + D5 + D7.2.

**Carryforward closure.** 09 REVIEW M2 (H2 HCM filter-synth header
decoration gap + ADR-0033 Consequences misrepresentation) is **CLOSED** at
this commit per the preferred close shape (a) — in-place ADR-0033
Consequences clarification paragraph + named close-site for the
implementation deferral. The chain 09 → 10 ends. 09 REVIEW M3 still targets
Task 7 per the original disposition; not engaged at Task 4.

**Carryforward dispositions otherwise unchanged.** The 09 REVIEW M3 +
M1/T1/T2/T3/D1/D2 + all earlier carryforwards continue per the Task-1
preamble's table.

**STATE.md / ROADMAP.md / BEHAVIOR_CONTRACT.md / ENVOY_TARGET.md /
rust-toolchain.toml diffs at this commit:** None (state-3 per-task cadence;
these stay at state-2 values until Task 8). `DECISIONS.md` IS modified
(ADR-0033 in-place amendment — ledger head stays at ADR-0034; NOT a new
ADR). `docs/envoy-rust/phases/09-http-filter-local-rate-limit/PROGRESS.md`
IS modified (1-sentence D7.2 cross-ref note).

### Task 5 — D8.1 fixture 0017 + Docker-gated wrapper

**Commit:** _(this commit; SHA emitted at `git commit` time)_
**Parent:** `84b508e` — `phase 10: task 4 — D4 variant + D5 ADR-0033 amendment (closes 09 REVIEW M2)`.

**Work summary.** Landed the D8.1 differential acceptance fixture
`tests/fixtures/0017-http-filter-rbac/` (4 files: `envoy.yaml`,
`envoy-rust.yaml`, `expectations.yaml`, `README.md`) + the Docker-gated
wrapper `tests/differential/tests/http_filter_rbac.rs` per PLAN Task 5
(SPEC §3 D8). The fixture drives 4 sequential `GET /` probes through an
HCM filter chain of `[envoy.filters.http.rbac,
envoy.filters.http.router]` under `action: ALLOW` with a single policy
`pass_with_header` requiring the `x-rbac-pass: yes` request header. Both
proxies must produce the deterministic status sequence `[403, 200, 403,
200]` with body `"RBAC: access denied"` (19 bytes per ADR-0034) on 403
probes and `"ok\n"` on 200 probes. Per-probe `extra_headers` variation
(no-header / `yes` / `no` / `yes`) drives the alternation; this is the
first fixture to exercise the per-probe distinct-headers axis of the
`Http1Probe` shape (the field has been present at
`tests/differential/src/lib.rs:619-635` since phase 04.2 with
`#[serde(default)]` but sat unused by every prior fixture). The fixture
is the **first non-LocalRateLimit bilateral consumer** of the phase-09
H1 HCM `decorate_filter_synth_response` helper (landed at ADR-0033
Commit C `ae2cef0`, at `crates/envoy-http1/src/hcm.rs:932`); the 2 deny
probes engage the helper end-to-end against both proxies, while the 2
allow probes pass through to the direct_response route, demonstrating
that the helper is filter-agnostic by design.

**Files modified (6; all CREATE except PROGRESS.md):**
- CREATE `tests/fixtures/0017-http-filter-rbac/envoy.yaml` (+97 LoC).
- CREATE `tests/fixtures/0017-http-filter-rbac/envoy-rust.yaml` (+55 LoC).
- CREATE `tests/fixtures/0017-http-filter-rbac/expectations.yaml` (+69 LoC).
- CREATE `tests/fixtures/0017-http-filter-rbac/README.md` (+126 LoC).
- CREATE `tests/differential/tests/http_filter_rbac.rs` (+33 LoC).
- MODIFY `docs/envoy-rust/phases/10-http-filter-rbac/PROGRESS.md` (this
  subsection; replaces the state-2 skeleton's `_(Pending Task 5
  dispatch.)_` placeholder).

**Total LoC delta:** +380 insertions / 0 deletions across the 5 new
files (excluding the PROGRESS.md self-narration). The 380-LoC count is
dominated by comments + the README's narrative + per-probe expectations
block — total production-equivalent code is ~30 LoC of YAML structure
and ~30 LoC of Rust wrapper boilerplate.

**Tests landed (1 new; 779 → 780 in workspace test count).**
1. `http_filter_rbac_fixture` (under `differential::tests::http_filter_rbac`)
   — bare `#[tokio::test]` that constructs the fixture-dir `PathBuf` and
   awaits `differential::run_fixture(&dir)`. Skipped by the harness when
   Docker is unavailable; PASS locally with Docker available.

**LoC delta detail (per file).**
- `envoy.yaml`: +97 (HCM filter chain + RBAC policy + per-side asymmetry comments).
- `envoy-rust.yaml`: +55 (symmetric narrow shape; no admin block; 127.0.0.1 bind).
- `expectations.yaml`: +69 (4-probe `Driver::Http1ProbeList` with per-probe `extra_headers`).
- `README.md`: +126 (filter-chain narrative + ADR-0034 contract + per-side asymmetry rationale).
- `http_filter_rbac.rs`: +33 (doc-comment + `#[tokio::test]` + `PathBuf` + `run_fixture` call).

**5-stable-toolchain attestation.** All 5 gates PASS on stable toolchain:
- `cargo fmt --all -- --check` — PASS (no output).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  — PASS (0 warnings).
- `cargo build --workspace --all-targets` — PASS.
- `cargo test --workspace` — PASS (780 passed, 0 failed, 2 ignored;
  +1 vs the phase-10 Task 4 snapshot of 779 — exactly the new
  `http_filter_rbac_fixture` Docker-gated wrapper).
- `cargo deny check` — PASS (advisories ok, bans ok, licenses ok,
  sources ok; pre-existing unencountered-license warnings unchanged
  from Task 4).
- (Locally) `cargo test -p differential --test http_filter_rbac --
  --nocapture`: PASS (`1 passed; 0 failed; 0 ignored; finished in 0.88s`;
  envoy-rust subprocess + upstream Docker container both completed the
  4-probe burst with matching status / body / header sets).

**Per-task deviations from PLAN (4 — adapted from the dispatch brief's
"Critical PLAN drifts" preflight).**

1. **YAML key rename: `request_headers:` → `extra_headers:`** in the 4
   probe entries in `expectations.yaml`. PLAN Step 4 (lines 2026-2065)
   used `request_headers:` but the on-disk `Http1Probe` struct at
   `tests/differential/src/lib.rs:619-635` declares the field as
   `extra_headers: Vec<(String, String)>` with `#[serde(default)]`
   (landed at phase 04.2). The 4 probes use `extra_headers:`; probe 1
   writes `extra_headers: []` explicitly for narrative clarity (the
   serde default is also an empty Vec so omission would be equivalent).
   No harness modification needed; the per-probe distinct-headers axis
   was already supported.

2. **YAML `port_value: {{PORT}}` substitution + admin `port_value: 0`.**
   PLAN Step 2 verbatim wrote `port_value: 10000` for the data-plane
   listener and `port_value: 9901` for the admin block. The harness
   substitutes `{{PORT}}` to a per-fixture-run kernel-ephemeral port (see
   `tests/differential/src/lib.rs:1635` → `reserve_port()`); the
   fixture-0016 precedent uses `port_value: {{PORT}}` on the data-plane
   listener on BOTH sides and `port_value: 0` (kernel-ephemeral) for the
   upstream-Envoy admin block. Adapted both sides to the precedent.

3. **Wrapper test signature + cfg gating.** PLAN Step 6 verbatim showed
   `run_fixture("0017-http-filter-rbac")` (string-literal arg) AND
   `#[cfg(differential_docker)] mod docker { ... }` gating. Both were
   wrong against disk: `differential::run_fixture` signature is
   `pub async fn run_fixture(fixture_dir: &Path) -> Result<()>`
   (verified at `tests/differential/src/lib.rs:1632`); there is NO
   `differential_docker` cfg feature in `tests/differential/Cargo.toml`;
   the fixture-0016 wrapper at
   `tests/differential/tests/http_filter_local_rate_limit.rs` is a bare
   `#[tokio::test]` with NO cfg gating (Docker-gating happens at the
   harness cluster level when `DOCKER_HOST` is unavailable). Wrapper
   adapted to mirror the 0016 shape verbatim: `PathBuf::from(env!(
   "CARGO_MANIFEST_DIR")).join(...).join(...).join("tests/fixtures/
   0017-http-filter-rbac")` + `differential::run_fixture(&dir).await`.
   Step 7's PLAN command line `cargo test ... --features
   differential_docker` was correspondingly dropped (no such feature).

4. **YAML quoting on `string_match: { exact: "yes" }`.** Bare `yes` /
   `no` parse as YAML booleans (`true` / `false`); the RBAC matcher
   requires the literal strings `"yes"` / `"no"`. Quoted both
   occurrences in both `envoy.yaml` and `envoy-rust.yaml` (the matcher
   exact value) and in `expectations.yaml`'s `extra_headers` entries
   (the request-header values). The PLAN Step 2 verbatim already
   correctly quoted `exact: "yes"`; this deviation entry documents that
   the convention is intentional and enforced across all 3 YAML
   surfaces of the fixture.

**Carryforward dispositions unchanged.** The 09 REVIEW M3 still targets
Task 7 per the original disposition; not engaged at Task 5. All other
carryforwards continue per the Task-1 preamble's table; 09 REVIEW M2
remains CLOSED at Task 4.

**STATE.md / ROADMAP.md / DECISIONS.md / BEHAVIOR_CONTRACT.md /
ENVOY_TARGET.md / rust-toolchain.toml diffs at this commit:** None.
(`DECISIONS.md` ledger head stays at ADR-0034; no new ADR projected at
Task 5.) `tests/differential/src/lib.rs` is also NOT modified (the
state-2 PLAN's anticipated harness extension for per-probe
`request_headers` was obviated by the existing `extra_headers` field).

### Task 6 — D8.2 fuzz corpus seed

_(Pending Task 6 dispatch.)_

### Task 7 — D8.3 in-process backstop (closes 09 REVIEW M3)

_(Pending Task 7 dispatch.)_

### Task 8 — state-4 phase-done verification + STATE advance to state-5-next

_(Pending Task 8 dispatch.)_
