# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `03.1`
**slug:** `03.1-tls-foundation-downstream` (created during the state-2-of-parent-03 split commit `f256d2c`; SPEC.md committed alongside).
**directory:** `docs/envoy-rust/phases/03.1-tls-foundation-downstream/` (exists; contains `SPEC.md` and `PLAN.md`).
**status:** phase 03.1 lifecycle **state 3 (PLAN.md exists, implementation incomplete)** — `SPEC.md` landed at `f256d2c` (alongside the ADR-0017 split decision); `PLAN.md` landed at commit `19e14af` (the previous commit). ROADMAP row `03.1` is **`status: in-progress`** as of this commit — flipped per ROADMAP-schema invariant 3 ("a phase enters `in-progress` only when STATE.md points at it as the active phase with the directory created"). This is a small deviation from the 02.1 / 02.2 state-advance precedent (those commits did not flip the sub-phase row; both rows skipped `in-progress` and went directly to `done` at phase-done), but the schema text and the prior STATE.md's explicit directive both call for the flip — applying the schema directly here. Sibling row `03.2` remains `planned`. Parent row `03` remains `in-progress` per the existing convention.

Parent phase `03-tls-tcp` is **in-progress** with `sub-phases: 03.1, 03.2` per ADR-0017's split (this commit). Parent SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `a3f3474`); for execution purposes it is superseded by the two sub-phase SPECs (`phases/03.1-tls-foundation-downstream/SPEC.md` and `phases/03.2-tls-upstream-sni/SPEC.md`).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 23–27, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 3): the next session — operating as the state-3 session of phase 03.1 — invokes **`superpowers:subagent-driven-development`** scoped to this phase, executing `PLAN.md` task-by-task with a fresh subagent per task plus the two-stage (spec-compliance + code-quality) review cadence the skill mandates, appending a section to `PROGRESS.md` on each task completion.

Every implementation task inside `PLAN.md` enforces `superpowers:test-driven-development` per doctrine D-3.1.

Per the user's standing preference (auto-memory `feedback_execution_style`), execution uses `superpowers:subagent-driven-development` over inline `executing-plans` — do not present the two-option fork.

**Plan splitting gate already evaluated** (BOOTSTRAP_PROMPT.md §5 state 2 / §6.1; SPEC §5; PLAN self-review):

- Task count: 13 (bound: ~25).
- Estimated net LoC change: ~1400 (bound: ~1500).
- Decision: **kept unified**. Both gates hold. Per SPEC §5 closing paragraph + PLAN's "Out-of-plan execution contingencies" §9, **do not split 03.1 further**. 03.1 was already produced *by* a split (ADR-0017), so a nested split would be unusual and should trigger `superpowers:systematic-debugging` first.

Inputs the state-3 session should read, in order, before launching the first subagent:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (row 03 `in-progress` with `sub-phases: 03.1, 03.2`; row 03.1 now `in-progress`; row 03.2 `planned`; rows 02/02.1/02.2 `done`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0017`; ADR-0018 + ADR-0019 land at Task 1 — see §ADR numbering in Notes; treat numbering as provisional per the ADR-0013 / ADR-0017 renumbering precedents).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (equivalence rules — 03.1 ships fixture `0004-tls-downstream`, exercising row 2 of §7.2 only; no contract edits expected per phase 03.1 SPEC §1 baked-in defaults).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference; state 3 sub-phase machinery).
7. `docs/envoy-rust/phases/03.1-tls-foundation-downstream/SPEC.md` (the authoritative sub-phase design contract — referenced at every task under "Source of truth: SPEC.md" at the top of `PLAN.md`).
8. `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PLAN.md` (the operational plan; 13 tasks; task boundaries are the natural subagent-dispatch boundaries).
9. `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (parent SPEC — context for the full phase-03 design; execution follows the 03.1 sub-phase SPEC, not the parent).
10. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md` + `PROGRESS.md` (most recent plan + progress precedent — task cadence, TDD framing, PROGRESS-formatting conventions; especially the per-listener wiring task shape relevant to phase 03.1's per-filter-chain TlsAcceptingHandler dispatch).
11. `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` (schema-additions task shape relevant to phase 03.1's envoy-config `DownstreamTlsContext` / `UpstreamTlsContext` / `TransportSocket` additions).
12. `docs/envoy-rust/phases/03.2-tls-upstream-sni/SPEC.md` (sibling sub-phase SPEC; useful context for understanding what is *not* in 03.1's scope, especially the upstream-TLS consumer wiring and the multi-cert dispatch — both deferred to 03.2).

## Last commit

Phase 03.1 state-advance commit (this commit): touches `docs/envoy-rust/STATE.md` (lifecycle state 2 → 3; next-skill `superpowers:writing-plans` → `superpowers:subagent-driven-development`) and `docs/envoy-rust/ROADMAP.md` (row `03.1` `planned` → `in-progress` per schema invariant 3 — small deviation from 02.1 / 02.2 precedent, see status paragraph above). No code changes.

Predecessor commits:

- `19e14af` — `phase 03.1: PLAN.md — envoy-tls foundation + downstream TLS termination + fixture 0004` — landed the 4648-line PLAN.md (the previous commit; lands the state-2 artifact retroactively after a prior session wrote PLAN.md but did not commit it).
- `f256d2c` — `phase 03: split into 03.1 + 03.2 at state 2 [ADR-0017]` — landed ADR-0017 + both sub-phase SPECs; flipped ROADMAP row 03 to `in-progress` with `sub-phases: 03.1, 03.2`.
- `4c36dcf` — `phase 03: state-1 close-out — STATE advance to state 2 + ROADMAP row 03 in-progress` — the parent-phase state-1 close-out.
- `a3f3474` — the parent-phase state-1 SPEC artifact.

## Last updated

2026-04-26 (phase 03.1 lifecycle state advanced from 2 to 3; PLAN.md committed at `19e14af`; ROADMAP row 03.1 flipped `planned` → `in-progress`; next-skill flips from `superpowers:writing-plans` to `superpowers:subagent-driven-development`).

## Notes

### ADR numbering after the phase-03 split

ADR-0017 (this commit's split decision) renumbered parent-SPEC §7's three projected ADRs upward by following the phase-02 / ADR-0013 precedent: the split ADR takes the next-sequential number at split time (here, ADR-0017), and the in-execution ADRs get the next-available numbers at landing time. The actual landed sequence:

- **ADR-0017** — split phase 03 into 03.1 + 03.2 (this commit; was parent-SPEC §7's projected ADR-0019).
- **ADR-0018** — `rcgen` + `tempfile` permitted as dev-test-harness-only foundations (will land in 03.1 task 1; was parent-SPEC §7's projected ADR-0017).
- **ADR-0019** — `tokio-rustls` + `rustls-pemfile` covered by the rustls foundations grant (will land in 03.1 task 1; was parent-SPEC §7's projected ADR-0018).

The sub-phase SPECs (03.1 + 03.2) cite ADR-0017 for the renumbering and rewrite each expected ADR with its actual landed number. The parent SPEC (`docs/envoy-rust/phases/03-tls-tcp/SPEC.md`) is preserved unedited per D-3.4 / D-3.5 (it's a committed historical artifact; the projected ADR numbers in its §7 remain literally as written — readers cross-reference this Notes section + ADR-0017 for the renumbered actuals). Any interim ADR (e.g., a `cargo-deny` exemption) landing during 03.1 execution between 03.1 task 1 and 03.2 start would shift ADR-0019's number (since it lands second at task 1) — same provisional posture phase-02.2 REVIEW recommendation #2 established.

### ADR numbering after the phase-02 split (for reference)

The parent-phase-02 SPEC (`02-tcp-proxy/SPEC.md`, committed at SHA `50349da`) projected three phase-02 ADRs numbered 0013 (typed_config), 0014 (host-docker + host-gateway), 0015 (enable_half_close false default). The ADR-0013 split decision (landed at `1c38ca9`) took the actual next-sequential number at split time, so each projected ADR shifted by +1 in-tree:

- **ADR-0013** — split phase 02 into 02.1 + 02.2 (landed at `1c38ca9`).
- **ADR-0014** — YAML-native `typed_config` deserialization (landed at `6d1f8d6` during 02.1 Task 1; was parent-SPEC §7's ADR-0013).
- **ADR-0015** — cross-container host reachability via `host.docker.internal` + `host-gateway` (landed at `435c6fa` during 02.2 Task 1; was parent-SPEC §7's ADR-0014).
- **ADR-0016** — phase 02 TCP proxy runs with Envoy's default `enable_half_close: false` (landed at `435c6fa` during 02.2 Task 1; was parent-SPEC §7's ADR-0015).

### Phase-01 rollovers (final disposition)

Per ADR-0013's split decision, phase-01 REVIEW §9 starter items were distributed:

- **I3** — four unit tests for `decode_chunked` in `tests/differential/src/lib.rs`: **closed** by 02.1 Task 11 at commit `535e6f9`.
- **I4** — admin 8 KiB header cap tightening in `crates/envoy-bin/src/admin.rs`: **closed** by 02.2 Task 3 at commit `4bd0e22`.
- **M1** — retargeting the stale `TODO(phase-01)` comment in `tests/differential/src/subject.rs`: **closed** by 02.2 Task 2 at commit `8aab844`.

All phase-01 starter items are now closed. No phase-01 rollovers carry into phase 03 (or its sub-phases).

### Phase-02.1 rollovers (final disposition)

The initial 02.1 REVIEW (HEAD `95a26a7`) landed with three Important items and four Minor items. I1 (Cargo.lock drift) closed at `dea4d16`; I2 (STATE.md stale) closed by state-5 commit `379937b`. The remaining items:

- **I3** — positive `ClusterType::Static` test (`bootstrap.rs:48–54` variant name regression guard): **tracked forward to whichever phase extends `ClusterType`** (likely phase 04+ when `LogicalDns` / `StrictDns` variants land; outside row 02's and phase 03's scope).
- **M1** — add `pub(crate) fn Cluster::name(&self) -> &str` accessor and remove the field-level `#[allow(dead_code)]` at `crates/envoy-cluster/src/cluster.rs`: **tracked forward to phase 03.2 (opportunistic) or phase 06 (default)** per phase 02.2 REVIEW §4 recommendation 1 + parent-SPEC-03 §1's baked-in defaults. Phase 03.2's D4 task signpost names this.
- **M2** — `echoes_round_trip` drop-before-send ordering in `tests/helpers/tcp-echo-server/src/main.rs`: awareness-only, no action required.
- **M3** — drop the dead `|| msg.contains("CRLF")` disjunct in `tests/differential/src/lib.rs`: **closed** opportunistically by 02.2 Task 11 at commit `aa4187f`.
- **M4** — style-only: `ClusterManager::get` does `Arc::clone` inside a `.map` closure: no action required.

### Phase-02.2 rollovers (from REVIEW.md §3–§4)

The 02.2 REVIEW landed with one Important item and four Minor items. I1 (STATE.md stale) closed in-phase by the §7 close-out commit `fc87505`. The remaining items:

- **M1** — `TcpProxyBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread (`tests/differential/src/backend.rs:73-83`): **tracked forward to whichever phase first parallelizes `run_fixture` across worker threads**. Phase 03.1 + 03.2 do not parallelize fixtures; the same is anticipated for 04 + 05. The `TlsEchoBackend` 03.2 ships inherits the same posture — single-fixture-per-invocation usage avoids the worst-case 2s Drop stall.
- **M2** — `proxies_returns_err_on_upstream_connect_refused` asserts on the formatted error string rather than the typed variant (`crates/envoy-tcp/src/lib.rs:289-296`): awareness-only, no action required.
- **M3** — `proxies_closes_downstream_on_upstream_close` has implicit timing on the upstream's "tail" read (`crates/envoy-tcp/src/lib.rs:199-202`): awareness-only, no action required.
- **M4** — `Listener::serve`'s `JoinSet` type aliases a long generic (`crates/envoy-listener/src/lib.rs:113-115`): **tracked forward to phase 04 or phase 07** when a richer filter trait warrants a `pub type HandlerResult = ...` alias. Phase 03.1's TlsAcceptingHandler in envoy-bin does not reach for this.

Phase 02.2 REVIEW §4 recommendations forward to phase 03.1 / 03.2 / later:

1. Add `Cluster::name()` accessor when phase 03.2's TLS work or phase 06's stats first need it (phase-02.1 REVIEW M1 cross-reference). Phase 03.2 SPEC §3 D4 signposts the opportunistic-close evaluation at execution time.
2. Phase 03 ADR projection numbering is provisional — heeded throughout 03.1 + 03.2 SPECs; ADR-0017 codifies the renumbering scheme (see "ADR numbering after the phase-03 split" above).
3. `TypedConfig` enum will grow one variant per filter across phases 04/05/06 (carries over unchanged from phase-02.1 REVIEW §4).
4. Round-robin distribution-equivalence assertion remains unit-test-only (parent-brainstorm Q1 decision; carries over unchanged).
5. If parallel fixture execution arrives, revisit `TcpProxyBackend::Drop` per M1 above (and now `TlsEchoBackend::Drop` too).
6. `enable_half_close: true` flip-fixture is the obvious follow-on per ADR-0016 — whichever phase first needs an asymmetric-close use case lands its own ADR + extends `TcpProxy` with an explicit half-close-propagation mode.

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None block phase 03.1 or 03.2.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject subprocess): still deferred. The `nix` crate remains the stated blocker (not on D-3.2 permitted-foundations list). Phase-01 and phase-02 (across 02.1 and 02.2) all chose not to take it; phase-02.2 retargeted the stale TODO comment to reflect this open-ended deferral. Phase 03.1 + 03.2 do not need `nix` and continue the deferral. A future phase that genuinely needs `nix` adds it under a new ADR and closes this item.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4 via five new regression tests; phase 03.1's 10 new validator tests continue the discipline on the new TLS struct levels.

### Phase-01 ADR ledger (for reference)

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as fuzz-only dev deps), ADR-0010 (nightly toolchain, explicit `+nightly` CI invocation; workspace-root pin stays stable), ADR-0011 (phase-01 defers response-header equivalence to phase 04; `server: envoy-rust` tolerated until then), ADR-0012 (nested nightly pin in fuzz subcrate; narrowly supersedes ADR-0010 on that single sub-point while preserving its main decision).

### Phase-02.1 ADR ledger (for reference)

ADR-0013 (split phase 02 into 02.1 + 02.2; landed at `1c38ca9` during parent-phase 02 state 2), ADR-0014 (YAML-native `typed_config` deserialization until the xDS/protos family lands; landed at `6d1f8d6` during 02.1 Task 1).

### Phase-02.2 ADR ledger (for reference)

ADR-0015 (cross-container host reachability via `host.docker.internal` + `host-gateway`; landed at `435c6fa` during 02.2 Task 1), ADR-0016 (phase 02 TCP proxy runs with Envoy's default `enable_half_close: false`; landed at `435c6fa` during 02.2 Task 1).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The state-3 close (landing this STATE.md edit) advances phase 03.1 to lifecycle state 3. The next session enters phase 03.1 state 3 via `superpowers:subagent-driven-development`, beginning with `PLAN.md` Task 1 (ADRs 0018 + 0019).
