# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `03.1`
**slug:** `03.1-tls-foundation-downstream` (created during this state-2-of-parent-03 split commit; SPEC.md committed alongside).
**directory:** `docs/envoy-rust/phases/03.1-tls-foundation-downstream/` (exists; contains `SPEC.md`).
**status:** phase 03.1 lifecycle **state 2 (SPEC.md exists, PLAN.md does not)** — next skill is `superpowers:writing-plans` scoped to phase 03.1, producing `PLAN.md`. ROADMAP row `03.1` is `status: planned` (sibling row `03.2` also `planned`); row `03.1` flips to `in-progress` at the start of the next session per the ROADMAP-schema invariant 3 ("a phase enters `in-progress` only when STATE.md points at it as the active phase with the directory created"). The directory exists and STATE points at 03.1 — the next session's first concrete act is to flip ROADMAP row `03.1` to `in-progress` (the existing convention; same as how phase 03 flipped at commit `4c36dcf`).

Parent phase `03-tls-tcp` is **in-progress** with `sub-phases: 03.1, 03.2` per ADR-0017's split (this commit). Parent SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `a3f3474`); for execution purposes it is superseded by the two sub-phase SPECs (`phases/03.1-tls-foundation-downstream/SPEC.md` and `phases/03.2-tls-upstream-sni/SPEC.md`).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**`superpowers:writing-plans`** — the next session enters phase 03.1 at lifecycle state 2: it consumes phase 03.1's `SPEC.md` and produces `PLAN.md`. Per `SKILL_ROUTING.md` state 2:

```
2. SPEC.md exists, PLAN.md does not
   → superpowers:writing-plans
   → output: PLAN.md
   → GATE: if PLAN.md > ~25 tasks OR > ~1500 LoC estimated
           → split into NN.1, NN.2, …; update ROADMAP + STATE; stop
```

Phase 03.1's SPEC §5 estimates ~1400 LoC across ~13 tasks, comfortably under both `BOOTSTRAP_PROMPT.md` §6.1 gates. The state-2 plan-writer for phase 03.1 should produce a single `PLAN.md` and **not** trigger a further split (a nested split of an already-split sub-phase deserves a fresh root-cause analysis via `superpowers:systematic-debugging` per phase 03.1 SPEC §5's guidance — likely scope creep or planner overdecomposition).

Inputs the state-2 session for phase 03.1 should read, in order:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (row 03 `in-progress` with `sub-phases: 03.1, 03.2`; rows 03.1 + 03.2 `planned`; rows 02/02.1/02.2 `done`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0017`; phase 03.1 SPEC §7 projects ADR-0018 + ADR-0019 — treat numbering as provisional per ADR-0013's renumbering precedent and ADR-0017's renumbering of parent-SPEC §7's projected numbers).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (no edits expected this sub-phase per phase 03.1 SPEC §1 baked-in defaults; the state-2 plan-writer confirms — the currently-empty `Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances` subsections remain empty).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference; state 2 → state 3 transition + §6.1 LoC gate).
7. `docs/envoy-rust/phases/03.1-tls-foundation-downstream/SPEC.md` (the contract this PLAN must execute — read in full; especially §3 deliverables D1–D10, §5 splitting guidance + LoC accounting, §6 implementation signposts, §7 ADRs ADR-0018 + ADR-0019, §8 artifacts list, §9 final-commit format).
8. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md` + `PROGRESS.md` (most recent plan + progress precedent — task cadence, TDD framing, PROGRESS-formatting conventions; especially the per-listener wiring task shape relevant to phase 03.1's per-filter-chain TlsAcceptingHandler dispatch).
9. `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` (also recent precedent; especially the schema-additions task shape relevant to phase 03.1's envoy-config `DownstreamTlsContext` / `UpstreamTlsContext` / `TransportSocket` additions).
10. `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (parent SPEC; preserved unedited as the historical design artifact; cross-reference for any ambiguity in the sub-phase SPEC, but the sub-phase SPEC is operative for execution).
11. `docs/envoy-rust/phases/03.2-tls-upstream-sni/SPEC.md` (sibling sub-phase SPEC; useful context for understanding what is *not* in 03.1's scope, especially the upstream-TLS consumer wiring and the multi-cert dispatch — both deferred to 03.2).

## Last commit

Phase 03 state-2 split commit: `phase 03: split into 03.1 + 03.2 at state 2 [ADR-0017]` (this commit). Lands ADR-0017 (split decision; takes the next-sequential number, renumbering parent-SPEC §7's projected ADR-0019 → ADR-0017, projected ADR-0017 → ADR-0018, projected ADR-0018 → ADR-0019), creates `phases/03.1-tls-foundation-downstream/SPEC.md` (~734 lines) and `phases/03.2-tls-upstream-sni/SPEC.md` (~686 lines), updates ROADMAP (row 03 keeps `in-progress` and gains `sub-phases: 03.1, 03.2`; new rows 03.1 + 03.2 land as `planned`), and advances STATE.md to point at `03.1` at lifecycle state 2 with next-skill `superpowers:writing-plans`. Mirrors the phase-02 split-commit precedent at SHA `1c38ca9`. No code changes.

The state-1 close-out commit `4c36dcf` (`phase 03: state-1 close-out — STATE advance to state 2 + ROADMAP row 03 in-progress`) is the immediate predecessor; the state-1 SPEC artifact landed at `a3f3474`.

## Last updated

2026-04-25 (phase 03 state-2 split — ADR-0017 lands the split decision; both sub-phase SPECs land in this commit; ROADMAP row 03 keeps `in-progress` and gains sub-phases 03.1 + 03.2; rows 03.1 + 03.2 land planned; STATE advances to phase 03.1 at lifecycle state 2; next-skill flips from `superpowers:writing-plans` (scoped to phase 03) to `superpowers:writing-plans` (scoped to phase 03.1)).

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
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. State-2 of phase 03 (this commit) is the §6.2 split protocol applied — it is one state advance (the writing-plans skill's GATE clause invocation), not two. Per the parent-phase-02 split-commit precedent at `1c38ca9`, the §6.2 step 3 "redistribute spec content — each sub-phase gets its own SPEC.md" happens as part of state 2's split work, not as a separate state-1 invocation on the sub-phase. The next session executes 03.1's lifecycle state 2 (writing `phases/03.1-tls-foundation-downstream/PLAN.md`).
