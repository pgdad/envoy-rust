# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `03`
**slug:** to be determined during state-1 brainstorming (ROADMAP row 03 title: "Downstream TLS termination + upstream TLS origination + SNI"; conventional slug `03-tls-tcp` or similar).
**directory:** `docs/envoy-rust/phases/03-<slug>/` (does **not** yet exist; created at the start of state 1 per `BOOTSTRAP_PROMPT.md` §5 state 1).
**status:** phase 03 lifecycle **state 1 (phase in ROADMAP, directory does not exist)** — next skill is `superpowers:brainstorming` scoped to phase 03, producing `SPEC.md`. ROADMAP row `03` is `status: planned`; it flips to `in-progress` at the same time the phase directory is created (per ROADMAP schema invariant 3).

Parent phase 02 (`02-tcp-proxy`) is **done** as of this commit. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (this commit). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

**`superpowers:brainstorming`** — the next session enters phase 03 at lifecycle state 1: it picks the slug, creates `docs/envoy-rust/phases/03-<slug>/`, runs the brainstorm scoped to phase 03, and produces `SPEC.md`. Per `SKILL_ROUTING.md` state 1:

```
1. Phase in ROADMAP, directory does not exist
   → create docs/envoy-rust/phases/NN-slug/
   → superpowers:brainstorming (scoped to THIS phase)
   → output: SPEC.md
```

Per `BOOTSTRAP_PROMPT.md` §8, ROADMAP row `03` covers "Downstream TLS termination + upstream TLS origination + SNI" with the differential surface "TLS TCP fixture green." The state-1 brainstorm decides slug, scope cuts, fixture shape (likely `0004-tls-tcp` building on fixture 0003's tcp_proxy backbone), the TLS stack approach (`rustls` + `aws-lc-rs` per D-3.2), and SNI semantics. Per `SKILL_ROUTING.md` state 2's gate, if the resulting PLAN.md exceeds ~25 tasks or ~1500 LoC, the phase is split (§6.2) before plan execution begins.

State 1 ends by writing `SPEC.md` and updating this STATE.md (active = phase 03 with the actual slug; lifecycle state 2; next-skill `superpowers:writing-plans`). Per `BOOTSTRAP_PROMPT.md` §5.1 "one state per session," state 1 does not chain into state 2.

Inputs the state-1 session for phase 03 should read, in order:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (row 03 + summary; rows 02/02.1/02.2 now `done`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0016`; phase 03 picks ADR-0017 onward — treat the projection as provisional per the renumbering precedent established by ADR-0013 and discussed in the Notes section below).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (equivalence rules — phase 03 is the first phase to add TLS framing, so the brainstorm should consider whether `Header allow-list` or `Timing tolerances` need their first phase-03 entries).
6. `docs/envoy-rust/SKILL_ROUTING.md` (routing reference; state 1 → state 2 transition).
7. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/SPEC.md` (consumed dependencies: `envoy-listener::Listener`, `envoy-tcp::TcpProxy`, the `ConnectionHandler` trait that phase 03 likely wraps with TLS termination — exact shape decided during the brainstorm).
8. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/REVIEW.md` (§4 recommendations — especially #2 ("`Cluster::name()` accessor when phase 03's TLS work or phase 06's stats first need it") and #3 ("ADR-0017 projection should be provisional"); the M1–M4 forwards are in §3 and may inform phase 03's harness scope).
9. `docs/envoy-rust/phases/02.1-config-cluster/SPEC.md` and `REVIEW.md` (further upstream context: `envoy-config` typed_config envelope grammar, `envoy-cluster` API, the cluster-manager surface phase 03 may extend with upstream-TLS metadata).
10. `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent SPEC of the just-completed row; useful for the "configs are initially identical" fixture principle and the ADR-0006/0007/0016 half-close precedent that any TLS-over-TCP fixture inherits).
11. `docs/envoy-rust/phases/01-static-bootstrap-config/PLAN.md` + `PROGRESS.md` (shape reference for plan-writing cadence, TDD framing, and PROGRESS-formatting conventions; phase 02.1 / 02.2 also serve as more recent precedents).

## Last commit

Phase 02.2 phase-done final commit: `phase 02.2: Listener + TCP proxy filter + fixture 0003 [ADR-0015,ADR-0016]`. Flips `ROADMAP.md` rows `02.2` and parent `02` → `done` in the same commit (per ROADMAP schema "parent flips to `done` only after all sub-phases are `done`"), and advances this STATE.md to phase 03 at lifecycle state 1. Preceded by `fc87505` (state-5 REVIEW.md landing + I1 close-out) and `02a9add` (state-4 phase-done gate verification).

## Last updated

2026-04-25 (phase 02.2 complete; parent phase 02 closed; STATE advanced to phase 03 at lifecycle state 1; next-skill flips to `superpowers:brainstorming`).

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
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The next session (state-1 of phase 03) ends by writing `SPEC.md` and advancing this STATE.md to phase 03 lifecycle state 2; the session after that (state-2 of phase 03) writes `PLAN.md`.
