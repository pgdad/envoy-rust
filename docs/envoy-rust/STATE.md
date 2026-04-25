# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `03`
**slug:** `03-tls-tcp` (chosen during state-1 brainstorming; SPEC.md committed at `a3f3474`).
**directory:** `docs/envoy-rust/phases/03-tls-tcp/` (exists; contains `SPEC.md`).
**status:** phase 03 lifecycle **state 2 (SPEC.md exists, PLAN.md does not)** — next skill is `superpowers:writing-plans` scoped to phase 03, producing `PLAN.md`. ROADMAP row `03` is `status: in-progress` (flipped to `in-progress` in this same commit per ROADMAP schema invariant 3; a phase enters `in-progress` once `STATE.md` points at it as the active phase with the directory created).

Parent phase 02 (`02-tcp-proxy`) is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**`superpowers:writing-plans`** — the next session enters phase 03 at lifecycle state 2: it consumes phase 03's `SPEC.md` and produces `PLAN.md`. Per `SKILL_ROUTING.md` state 2:

```
2. SPEC.md exists, PLAN.md does not
   → superpowers:writing-plans
   → output: PLAN.md
   → GATE: if PLAN.md > ~25 tasks OR > ~1500 LoC estimated
           → split into NN.1, NN.2, …; update ROADMAP + STATE; stop
```

`SPEC.md` §5 ("Splitting guidance for the planner") explicitly anticipates that the plan-writer will trip `BOOTSTRAP_PROMPT.md` §6.1's LoC gate (estimated ~2845 LoC across phase 03; ~90% over) and formally split phase 03 into sibling sub-phases `03.1-tls-foundation-downstream` (envoy-tls foundation + downstream TLS termination single-cert + fixture 0004; ~1400 LoC / ~13 tasks) and `03.2-tls-upstream-sni` (upstream TLS origination + multi-cert SNI cert selection + tls-echo-server helper + fixtures 0005 + 0006; ~1445 LoC / ~14 tasks). Both sub-phases comfortably under §6.1's ~1500 LoC / ~25 tasks gates. Pattern mirrors parent phase 02's ADR-0013 split.

The state-2 plan-writer lands ADR-0019 (split decision) in the same shape as ADR-0013, redistributes SPEC content into fresh sub-phase SPECs (per `BOOTSTRAP_PROMPT.md` §6.2), updates ROADMAP (row 03 keeps `in-progress` with `sub-phases: 03.1, 03.2`; new rows 03.1 + 03.2 land as `planned`), and updates STATE.md to point at `03.1` at lifecycle state 1. Per `BOOTSTRAP_PROMPT.md` §5.1 "one state per session," state 2 does not chain into 03.1's state 1; a separate next session executes 03.1 state 1 (writing `phases/03.1-tls-foundation-downstream/SPEC.md`).

If the plan-writer somehow finds the actual LoC accounting fits within §6.1's gates without splitting, it lands a single PLAN.md for phase 03 and updates STATE.md to lifecycle state 3 with next-skill `superpowers:executing-plans` (or `superpowers:subagent-driven-development`) — but SPEC §5's accounting strongly anticipates the split path. ADR-0017 (rcgen + tempfile dev-test-harness-only foundations) and ADR-0018 (tokio-rustls + rustls-pemfile covered by the rustls grant) are projected in SPEC §7 to land as task 1 of the post-split sub-phase plans; ADR numbering remains provisional per ADR-0013's renumbering precedent (Notes below).

Inputs the state-2 session for phase 03 should read, in order:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (row 03 now `in-progress`; rows 02/02.1/02.2 `done`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0016`; phase 03 SPEC §7 projects ADR-0017, ADR-0018, and ADR-0019 — treat numbering as provisional per the renumbering precedent established by ADR-0013 and discussed in the Notes section below).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (no edits expected this phase per SPEC §1 baked-in defaults; the state-2 plan-writer confirms — the currently-empty `Header allow-list`, `Stat-name mapping`, `Access log field mapping`, `xDS wire state machine`, `Timing tolerances` subsections remain empty).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference; state 2 → state 3 transition + §6.1 LoC gate + §6.2 split protocol).
7. `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (the contract this PLAN must execute — read in full, especially §3 deliverables, §5 splitting guidance + LoC accounting, §6 implementation signposts, §7 ADRs, §8 artifacts list, §9 final-commit format).
8. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PLAN.md` + `PROGRESS.md` (most recent plan + progress precedent — task cadence, TDD framing, PROGRESS-formatting conventions).
9. `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` (also recent precedent; especially the schema-additions task shape relevant to phase 03's envoy-config `DownstreamTlsContext` / `UpstreamTlsContext` / `FilterChainMatch` additions).
10. `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent SPEC of the just-completed row; precedent for the parent-SPEC-anticipates-split posture and the in-tree-historical-artifact stance the phase-03 SPEC will inherit once 03.1 + 03.2 land).
11. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/REVIEW.md` §3–§4 (M1–M4 carryforwards and §4 recommendations relevant to phase 03 — especially recommendation 1 (`Cluster::name()` accessor opportunistic close) and recommendation 2 (ADR-0017 numbering provisional)).

## Last commit

Phase 03 state-1 close-out commit: `phase 03: state-1 close-out — STATE advance to state 2 + ROADMAP row 03 in-progress` (this commit). Advances STATE.md to phase 03 lifecycle state 2 and flips `ROADMAP.md` row 03 from `planned` to `in-progress`. The state-1 SPEC artifact landed at `a3f3474` (`phase 03: SPEC — Downstream TLS termination + upstream TLS origination + SNI`), which committed `SPEC.md` only; the doctrinal STATE.md / ROADMAP.md update that should have bundled with that commit per the phase-02 precedent (e.g. `fc87505` for phase 02.2 state 5) was missed and is being closed out here as a separate commit. No SPEC content changes; no ADR; no code.

## Last updated

2026-04-25 (phase 03 state-1 close-out — STATE advanced to phase 03 lifecycle state 2; ROADMAP row 03 flipped `planned` → `in-progress`; next-skill flips from `superpowers:brainstorming` to `superpowers:writing-plans`).

## Notes

### ADR numbering after the phase-02 split

The parent-phase SPEC (`02-tcp-proxy/SPEC.md`, committed at SHA `50349da`) projected three phase-02 ADRs numbered 0013 (typed_config), 0014 (host-docker + host-gateway), 0015 (enable_half_close false default). The ADR-0013 split decision (landed at `1c38ca9`) took the actual next-sequential number, so each projected ADR shifted by +1 in-tree:

- **ADR-0013** — split phase 02 into 02.1 + 02.2 (landed at `1c38ca9`).
- **ADR-0014** — YAML-native `typed_config` deserialization (landed at `6d1f8d6` during 02.1 Task 1; was parent-SPEC §7's ADR-0013).
- **ADR-0015** — cross-container host reachability via `host.docker.internal` + `host-gateway` (landed at `435c6fa` during 02.2 Task 1; was parent-SPEC §7's ADR-0014).
- **ADR-0016** — phase 02 TCP proxy runs with Envoy's default `enable_half_close: false` (landed at `435c6fa` during 02.2 Task 1; was parent-SPEC §7's ADR-0015).

The sub-phase SPECs cite ADR-0013 for the renumbering and rewrite each expected ADR with its actual number. The parent SPEC is preserved unedited per D-3.4 / D-3.5 (it's a committed historical artifact, not a living document). Phase 03 should treat any ADR projections in its SPEC as provisional and resolve to the actual next-sequential numbers at task 1 — an interim cargo-deny-driven ADR landing between 02.2 done and 03 start would shift them.

### Phase-01 rollovers (final disposition)

Per ADR-0013's split decision, phase-01 REVIEW §9 starter items were distributed:

- **I3** — four unit tests for `decode_chunked` in `tests/differential/src/lib.rs`: **closed** by 02.1 Task 11 at commit `535e6f9`.
- **I4** — admin 8 KiB header cap tightening in `crates/envoy-bin/src/admin.rs`: **closed** by 02.2 Task 3 at commit `4bd0e22`.
- **M1** — retargeting the stale `TODO(phase-01)` comment in `tests/differential/src/subject.rs`: **closed** by 02.2 Task 2 at commit `8aab844`.

All phase-01 starter items are now closed. No phase-01 rollovers carry into phase 03.

### Phase-02.1 rollovers (final disposition)

The initial 02.1 REVIEW (HEAD `95a26a7`) landed with three Important items and four Minor items. I1 (Cargo.lock drift) closed at `dea4d16`; I2 (STATE.md stale) closed by state-5 commit `379937b`. The remaining items:

- **I3** — positive `ClusterType::Static` test (`bootstrap.rs:48–54` variant name regression guard): **tracked forward to whichever phase extends `ClusterType`** (likely phase 04+ when `LogicalDns` / `StrictDns` variants land; outside row 02's scope).
- **M1** — add `pub(crate) fn Cluster::name(&self) -> &str` accessor and remove the field-level `#[allow(dead_code)]` at `crates/envoy-cluster/src/cluster.rs`: **tracked forward to phase 03 or phase 06** per phase 02.2 REVIEW §4 recommendation 2 (whichever phase first needs name attribution for stats or trace spans).
- **M2** — `echoes_round_trip` drop-before-send ordering in `tests/helpers/tcp-echo-server/src/main.rs`: awareness-only, no action required.
- **M3** — drop the dead `|| msg.contains("CRLF")` disjunct in `tests/differential/src/lib.rs`: **closed** opportunistically by 02.2 Task 11 at commit `aa4187f`.
- **M4** — style-only: `ClusterManager::get` does `Arc::clone` inside a `.map` closure: no action required.

### Phase-02.2 rollovers (from REVIEW.md §3–§4)

The 02.2 REVIEW landed with one Important item and four Minor items. I1 (STATE.md stale) closed in-phase by the §7 close-out commit `fc87505`. The remaining items:

- **M1** — `TcpProxyBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread (`tests/differential/src/backend.rs:73-83`): **tracked forward to whichever phase first parallelizes `run_fixture` across worker threads**. Until then, single-fixture-per-invocation usage avoids the worst-case 2s Drop stall.
- **M2** — `proxies_returns_err_on_upstream_connect_refused` asserts on the formatted error string rather than the typed variant (`crates/envoy-tcp/src/lib.rs:289-296`): awareness-only, no action required.
- **M3** — `proxies_closes_downstream_on_upstream_close` has implicit timing on the upstream's "tail" read (`crates/envoy-tcp/src/lib.rs:199-202`): awareness-only, no action required.
- **M4** — `Listener::serve`'s `JoinSet` type aliases a long generic (`crates/envoy-listener/src/lib.rs:113-115`): **tracked forward to phase 04 or phase 07** when a richer filter trait warrants a `pub type HandlerResult = ...` alias.

Phase 02.2 REVIEW §4 recommendations forward to phase 03 or later:

1. Add `Cluster::name()` accessor when phase 03's TLS work or phase 06's stats first need it (phase-02.1 REVIEW M1 cross-reference).
2. Phase 03 ADR projection numbering should treat ADR-0017 as provisional (interim ADRs shift the sequence).
3. `TypedConfig` enum will grow one variant per filter across phases 04/05/06 (carries over unchanged from phase-02.1 REVIEW §4).
4. Round-robin distribution-equivalence assertion remains unit-test-only (parent-brainstorm Q1 decision; carries over unchanged).
5. If parallel fixture execution arrives, revisit `TcpProxyBackend::Drop` per M1 above.
6. `enable_half_close: true` flip-fixture is the obvious follow-on per ADR-0016 — whichever phase first needs an asymmetric-close use case lands its own ADR + extends `TcpProxy` with an explicit half-close-propagation mode.

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None block phase 03.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject subprocess): still deferred. The `nix` crate remains the stated blocker (not on D-3.2 permitted-foundations list). Phase-01 and phase-02 (across 02.1 and 02.2) all chose not to take it; phase-02.2 retargeted the stale TODO comment to reflect this open-ended deferral. A future phase that genuinely needs `nix` adds it under a new ADR and closes this item.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4 via five new regression tests.

### Phase-01 ADR ledger (for reference)

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as fuzz-only dev deps), ADR-0010 (nightly toolchain, explicit `+nightly` CI invocation; workspace-root pin stays stable), ADR-0011 (phase-01 defers response-header equivalence to phase 04; `server: envoy-rust` tolerated until then), ADR-0012 (nested nightly pin in fuzz subcrate; narrowly supersedes ADR-0010 on that single sub-point while preserving its main decision).

### Phase-02.1 ADR ledger (for reference)

ADR-0013 (split phase 02 into 02.1 + 02.2; landed at `1c38ca9` during parent-phase 02 state 2), ADR-0014 (YAML-native `typed_config` deserialization until the xDS/protos family lands; landed at `6d1f8d6` during 02.1 Task 1).

### Phase-02.2 ADR ledger (for reference)

ADR-0015 (cross-container host reachability via `host.docker.internal` + `host-gateway`; landed at `435c6fa` during 02.2 Task 1), ADR-0016 (phase 02 TCP proxy runs with Envoy's default `enable_half_close: false`; landed at `435c6fa` during 02.2 Task 1).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. State-1 of phase 03 already landed (`SPEC.md` at commit `a3f3474`; STATE.md / ROADMAP advance in this commit). The next session (state-2 of phase 03) consumes `SPEC.md` and writes `PLAN.md` — and per SPEC §5's LoC accounting will almost certainly trip §6.1's gate and land ADR-0019 splitting phase 03 into 03.1 + 03.2. State 2 does not chain into 03.1's state 1; a separate next session executes that.
