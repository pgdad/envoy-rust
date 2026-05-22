# Phase 12.1 (`12.1-endpoint-health-and-lb-integration`) — PROGRESS

> Per-task narrative log. Appended at every task commit per the 06.2 / 06.3 / 07.x / 08.x /
> 09 / 10 / 11 cadence. State-2 PLAN-write lands this skeleton + the Task 1 preamble; state-3
> dispatch appends `### Task N — <name>` subsections in execution order.

---

## State-2 commit context

This commit (the state-2 standalone PLAN-write commit) lands:

- **CREATE** `docs/envoy-rust/phases/12.1-endpoint-health-and-lb-integration/PLAN.md` (the
  state-2 PLAN.md per `BOOTSTRAP_PROMPT.md` §5 state 2; 7 tasks; 20 architecture lock-ins;
  full `- [ ]` checkbox TDD steps with complete code per task).
- **CREATE** `docs/envoy-rust/phases/12.1-endpoint-health-and-lb-integration/PROGRESS.md` (this file).
- **MODIFY** `docs/envoy-rust/ROADMAP.md` — flip row `12.1` `status: planned` →
  `status: in-progress`. No other row touched (parent row `12` stays `in-progress`; `12.2`
  stays `planned`).
- **MODIFY** `docs/envoy-rust/STATE.md` — Active phase status; Next expected skill; Last
  commit; Last updated; new `### Phase-12.1 state-2 PLAN-write` subsection in Notes.

**Predecessor commit:** `4f9ba04` — `phase 12: state-2 split decision + sub-phase SPECs
(12.1/12.2) [ADR-0036, ADR-0037]` (the parent-12 state-2 split commit; immediate prologue;
HEAD == origin/main at this PLAN-write's prologue; CI run `26290931448` settled `success`).

**SPEC commit base:** `4f9ba04`. **This state-2 commit makes NO inline SPEC.md edits** — the
§6.2 empirical verification was completed at the parent-12 split (`4f9ba04`); the 12.1
PLAN-writer baked the locked facts into the PLAN lock-ins without re-running Docker.

**ROADMAP status before this commit:** row `12.1` `planned` (added at the parent-12 split).
**ROADMAP status after this commit:** row `12.1` `in-progress`.

**STATE.md "Active phase" status before:** `phase 12.1 lifecycle state 2 (SPEC.md exists; PLAN.md does NOT)`.
**STATE.md "Active phase" status after:** `phase 12.1 lifecycle state 2-complete / state-3-next (PLAN.md + PROGRESS.md skeleton + Task 1 preamble landed; first task implementation pending)`.

**DECISIONS.md status before AND after:** **ADR-0037** (count 38). **No ADR lands at this
state-2 commit** (SPEC §7 + PLAN lock-in #2 — 12.1 introduces no new crate, no foundations
grant, no wire-level contract revision; the `EndpointHealth` `Relaxed`-ordering choice is
covered by the existing `cluster.rs` `pick()` precedent). Next available number stays
**ADR-0038**.

**BEHAVIOR_CONTRACT.md status before AND after:** Unchanged at this commit. The 1 new
`Stat-name mapping` row (`cluster.<name>.membership_healthy`, under a new `**12.1 entries
(active health checking):**` header) lands at Task 5 per the 06.x → 11 cadence (contract
extensions land at the task where the surface is first wired, NOT at PLAN-write time). The 3
`health_check.{attempt,success,failure}` counter rows defer to 12.2 (lock-in #4).

**ENVOY_TARGET.md + rust-toolchain.toml:** Unchanged (D-3.7 / D-3.9).

---

## PLAN scope summary

- **7 tasks** per PLAN §File-Structure + tasks 1-7. Subagent-driven execution at state 3 per
  PLAN lock-in #18 + `feedback_execution_style`.
- **~900-1000 LoC projected** (production ~430, tests ~470, doc/corpus ~80) — comfortably
  under the `BOOTSTRAP_PROMPT.md` §6.1 ~1500-LoC / ~25-task gate. The parent-12 split
  (ADR-0036) already absorbed the over-gate scope into 12.1 + 12.2; 12.1 does NOT nest-split
  (lock-in #1).
- **ZERO ADR landings** (lock-in #2; SPEC §7).
- **NO new differential fixture** — regression-equivalence via the 18 existing Docker-gated
  fixtures (`0001`-`0018`) staying green simultaneously proves the machinery is inert when
  `health_checks` is unconfigured (the 05.1/07.1 foundation-slice pattern; §5.4).

---

## Task 1 preamble

### §6.2 empirical-verification findings — locked at the parent-12 split (`4f9ba04`); NOT re-run at this PLAN-write

Per the 12.1 SPEC §2 + §6 + STATE.md `### Phase-12 state-2 split decision` + ADR-0037, the
HEAVY 6-item §6.2 verification against `envoyproxy/envoy:v1.33.0` was performed at the
parent-12 state-2 split commit `4f9ba04` (a Docker bridge network with a synthetic
health-aware backend + an active-HC Envoy; admin `/stats` + data-plane probes). The findings
binding 12.1 are LOCKED FACTS (PLAN lock-in #3); the 12.1 PLAN-writer baked them into the
PLAN without re-running Docker:

1. **Initial endpoint health state = Unhealthy/pending-until-first-pass** (MATCHED projection)
   → D3 `EndpointHealth` initial state = Unhealthy (lock-in #12); the inert-and-unexercised-seam
   consequence (12.1 ships no configured-HC traffic-serving fixture — the path stays inert
   until 12.2 wires the probe task + fixture).
2. **No-healthy-upstream synth body = `no healthy upstream` (19 bytes)** (DIVERGES → ADR-0037)
   — a **12.2 deliverable** (D6.2). 12.1 does NOT touch the synth-503 writer path
   (`hcm.rs:582`/`:918`); lock-in #14.
3. **Panic threshold: default 50%, strictly-below (`<`), `Percent { value: f64 }`, panic-mode
   round-robins over ALL** (MATCHED) → D1 `Percent` (lock-in #8) + D5 panic logic (lock-in #13).
4. **Health-check stat names `cluster.<name>.health_check.{attempt,success,failure}` +
   `membership_healthy`** (MATCHED) → D6; 12.1 wires the `membership_healthy` gauge, defers
   the 3 counters to 12.2 (lock-in #4).
5. **HTTP probe shape: default `expected_statuses` = exactly 200; `Int64Range` half-open
   `[start, end)`** (MATCHED) → D1 reuses `Int64Range` directly.
6. **Duration config shape: integer seconds only is the shared form** (DIVERGES from the
   parent fixture sketch's `0.5s`) → D1 reuses `parse_duration` as-is (rejects `0.5s`); D2's
   validator surfaces a sub-second `0.5s` as `InvalidHealthCheckTiming`. The integer-second
   fixture duration is a 12.2 concern (12.1 has no fixture).

### PLAN-write SPEC corrections (read against HEAD `4f9ba04`)

The 8 corrections in PLAN §1 (verified against HEAD): (1) config `Cluster` derives
`Debug, Serialize, Deserialize, PartialEq` (NO `Clone`; HAS `Serialize`) — the new structs
match this, not the parent SPEC's `Clone, Deserialize` sketch; (2) `http_health_check` is
made `Option<HttpHealthCheck>` (validator-required) to keep `UnsupportedHealthCheckType` a
live (constructed) variant — avoids a `dead_code` lint under `-D warnings`; (3) `ConfigError`
lives in `lib.rs:43`, `validate()` + `validate_health_checks` in `bootstrap.rs`; (4)
`parse_duration` at `bootstrap.rs:2289` is `Result<Duration, String>`, integer-only; (5)
`Int64Range` at `bootstrap.rs:1080` is `{ start: i64, end: i64 }`, half-open, validated via
`InvalidInt64Range`; (6) runtime `Cluster` at `cluster.rs:32` gains `endpoint_health` +
`panic_threshold`; `pick()` at `:129`, `pick_endpoint()` at `:152`; (7) `register_gauge ->
Arc<Gauge>`, `Gauge::{inc,dec,set,value}`; (8) `synth_status` at `hcm.rs:918` NOT touched at
12.1 (the no-healthy-body reconciliation is 12.2/ADR-0037). Two cross-crate compile-fix
surfaces are also pre-identified: adding fields to `envoy_config::Cluster` (Task 1) +
the runtime `Cluster` (Task 4) breaks the by-hand `Cluster { }` struct literals in
`crates/envoy-cluster/src/cluster.rs`, `crates/envoy-config/src/bootstrap.rs`,
`crates/envoy-http2/src/hcm.rs`, and `crates/envoy-bin/tests/http2_router_upstream.rs`
(lock-in #16 + #17).

### Carryforward disposition

12.1 engages **no** carryforward (lock-in #19). The 06.3 REVIEW I2 synthetic-backend
down-payment is a 12.2 deliverable (12.1 ships no fixture/harness). The inherited
carryforward inventory (parent SPEC standing list) carries forward UNCHANGED — 12.1 touches
no HTTP-filter file.

---

<!-- state-3 task subsections append below this line -->
