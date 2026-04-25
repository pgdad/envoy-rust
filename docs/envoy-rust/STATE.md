# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `02.2`
**slug:** `02.2-listener-tcp-proxy`
**directory:** `docs/envoy-rust/phases/02.2-listener-tcp-proxy/` (exists; contains `SPEC.md`, `PLAN.md`, `PROGRESS.md`, and `REVIEW.md`).
**status:** phase 02.2 lifecycle **state 5 (REVIEW.md approved; state-6 phase-done commit next)** — `SPEC.md` landed at `1c38ca9`; `PLAN.md` landed at `ad90db6`; all 13 PLAN.md tasks implemented across `435c6fa..02a9add`; state-4 phase-done gate cleared at task 13 with the local stable-toolchain gate clean on first attempt (PROGRESS.md §"Task 13 / State 4" — 114 tests passing, `cargo deny check` clean); `Cargo.lock` sync at `2146014` (phase-01/02.1 precedent); REVIEW.md landed with verdict **Approved with fixes → Approved** (REVIEW.md §6/§7) in the same commit that flips this STATE.md from state 3 to state 5. ROADMAP row `02.2` remains `status: planned` until the phase-done commit; parent row `02` remains `in-progress`.

Phase 02.1 (`02.1-config-cluster`) is **done** as of commit `d447f53`. ROADMAP row `02.1` is `status: done`. Phase 02.1 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 and I2 closed in-phase; I3 and M1–M4 tracked forward — see Notes below).

Parent phase 02 (`02-tcp-proxy`) stays `in-progress` per ROADMAP schema: the parent flips to `done` only when all sub-phases land, i.e., in the same commit as 02.2's final phase-done commit. Parent `SPEC.md` at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed design artifact from SHA `50349da`.

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 44–48, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 6): the next session — operating as the state-6 session of phase 02.2 — lands the **phase-done commit** with message format per `BOOTSTRAP_PROMPT.md` §5.3:

```
phase 02.2: Listener + TCP proxy filter + fixture 0003 [ADR-0015,ADR-0016]

<3–6 paragraph narrative covering landed surface (envoy-listener crate +
envoy-tcp crate + envoy-bin ClusterManager wiring + tcp_proxy filter dispatch
+ fixture 0003-tcp-proxy + differential TcpProxyBackend + admin 8 KiB cap
tightening + phase-01 M1 retarget); differential/conformance status
(114 tests green; Docker-gated tcp_proxy_fixture runs in CI;
crates/envoy-bin/tests/tcp_proxy.rs covers the non-Docker integration path);
gate evidence (PROGRESS.md §"Task 13 / State 4" first-attempt clean,
cargo deny clean, REVIEW §6 Approved with fixes → §7 close-out → Approved);
rollovers carried forward to phase 03 or later (REVIEW.md §3 M1–M4 + §4
recommendations 2–7).>
```

The commit flips `ROADMAP.md` row `02.2` `status` from `planned` to `done` AND parent row `02` from `in-progress` to `done` in the same commit (per ROADMAP schema "parent flips to `done` only after all sub-phases are `done`" and SPEC §8). It also advances this STATE.md from phase 02.2 to phase 03 (slug to be determined during phase 03's state-1 brainstorming session per `BOOTSTRAP_PROMPT.md` §5 state 1; ROADMAP row 03 summary: "Downstream TLS termination + upstream TLS origination + SNI"; lifecycle state 1; next-skill `superpowers:brainstorming` scoped to the new phase). State 6 is a docs-only commit (no code changes); no further review is required — REVIEW.md's §7 final verdict stands.

Inputs the state-6 session should read, in order:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (rows 02, 02.2, 03; ensure the flip touches both 02 and 02.2 atomically).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0016`).
5. `docs/envoy-rust/SKILL_ROUTING.md` state 6 block.
6. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/SPEC.md` §8 (commit discipline / phase-done commit conventions).
7. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/REVIEW.md` (Approved verdict — §7 close-out; mine the §1–2 narrative for the commit message body).
8. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/PROGRESS.md` (state-4 gate evidence and per-task narrative for the commit message body).
9. `docs/envoy-rust/phases/02.1-config-cluster/` (shape precedent — `d447f53` is the state-6 phase-done commit; `379937b` is the state-5 REVIEW-landing precedent for the same atomic "REVIEW.md + STATE.md advance" commit shape this session lands).
10. `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` (parent SPEC — last touched at SHA `50349da`; remains in-tree unedited as the committed historical artifact, but referenced for the commit narrative's "parent phase closes" framing).

## Last commit

REVIEW.md landing commit (this session): lands `docs/envoy-rust/phases/02.2-listener-tcp-proxy/REVIEW.md` with state-5 verdict **Approved** (per §7 I1 close-out) and advances this STATE.md from state 3 to state 5. Preceded in this lifecycle by `02a9add` (Task 13 state-4 phase-done gate verification — workspace test/clippy/fmt/deny all clean) and `2146014` (Cargo.lock sync — phase-01/02.1 precedent shape).

## Last updated

2026-04-25 (phase 02.2 lifecycle state advanced from 3 to 5; REVIEW.md committed with verdict Approved; next-skill flips from `superpowers:subagent-driven-development` to the state-6 phase-done commit per `BOOTSTRAP_PROMPT.md` §5 state 6).

## Notes

### ADR numbering after the phase-02 split

The parent-phase SPEC (`02-tcp-proxy/SPEC.md`, committed at SHA `50349da`) projected three phase-02 ADRs numbered 0013 (typed_config), 0014 (host-docker + host-gateway), 0015 (enable_half_close false default). The ADR-0013 split decision (landed at `1c38ca9`) took the actual next-sequential number, so each projected ADR shifts by +1 in-tree:

- **ADR-0013** — split phase 02 into 02.1 + 02.2 (landed at `1c38ca9`).
- **ADR-0014** — YAML-native `typed_config` deserialization (landed at `6d1f8d6` during 02.1 Task 1; was parent-SPEC §7's ADR-0013).
- **ADR-0015** — cross-container host reachability via `host.docker.internal` + `host-gateway` (lands during 02.2 execution; was parent-SPEC §7's ADR-0014).
- **ADR-0016** — phase 02 TCP proxy runs with Envoy's default `enable_half_close: false` (lands during 02.2 execution; was parent-SPEC §7's ADR-0015).

The sub-phase SPECs cite ADR-0013 for the renumbering and rewrite each expected ADR with its actual number. The parent SPEC is preserved unedited per D-3.4 / D-3.5 (it's a committed historical artifact, not a living document). Per phase-02.1 REVIEW.md §4 recommendation #2, 02.2 should treat its own ADR-0015 / ADR-0016 projections as provisional and resolve to the actual next-sequential numbers at task 1 (an interim cargo-deny-driven ADR landing between 02.1 done and 02.2 start would shift both).

### Phase-01 rollovers

Per ADR-0013's split decision, phase-01 REVIEW §9 starter items were distributed:

- **I3** — four unit tests for `decode_chunked` in `tests/differential/src/lib.rs`: **closed** by 02.1 Task 11 at commit `535e6f9`.
- **I4** — admin 8 KiB header cap tightening in `crates/envoy-bin/src/admin.rs:158–170`: **open**; lands in 02.2 (touches `envoy-bin::admin`, alongside 02.2's `envoy-bin` wiring).
- **M1** — retargeting the stale `TODO(phase-01)` comment in `tests/differential/src/subject.rs:25–32`: **open**; lands in 02.2 (sits alongside 02.2's harness work).

### Phase-02.1 rollovers (from REVIEW.md §3–§4)

The initial 02.1 REVIEW (HEAD `95a26a7`) landed with three Important items and four Minor items. I1 (Cargo.lock drift) was closed at `dea4d16`; I2 (STATE.md stale) was closed by the state-5 commit `379937b`. The remaining items carry forward:

- **I3** — positive `ClusterType::Static` test (`bootstrap.rs:48–54` variant name regression guard): **tracked forward to whichever phase extends `ClusterType`** (likely phase 04+ when `LogicalDns` / `StrictDns` variants land; outside row 02's scope).
- **M1** — add `pub(crate) fn Cluster::name(&self) -> &str` accessor and remove the field-level `#[allow(dead_code)]` at `crates/envoy-cluster/src/cluster.rs:13–14` when `envoy-tcp::handle` first reaches for it: **lands in 02.2** (the first consumer site).
- **M2** — `echoes_round_trip` drop-before-send ordering in `tests/helpers/tcp-echo-server/src/main.rs:210–232`: awareness-only, no action required.
- **M3** — drop the dead `|| msg.contains("CRLF")` disjunct in `tests/differential/src/lib.rs:788–791`: one-line edit; **tracked forward to 02.2 harness touches** (or an opportunistic cleanup).
- **M4** — style-only: `ClusterManager::get` does `Arc::clone` inside a `.map` closure at `cluster.rs:61–65` (no-op on `None`; modern clippy doesn't flag): no action required.

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None block 02.1 or 02.2.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject subprocess): still deferred. The `nix` crate remains the stated blocker (not on D-3.2 permitted-foundations list). Phase-01 and phase-02 (across 02.1 and 02.2) all chose not to take it. A future phase that genuinely needs `nix` adds it under a new ADR and closes this item. 02.2's rollover M1 retargets the stale TODO comment to reflect this open-ended deferral.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4 via five new regression tests.

### Phase-01 ADR ledger (for reference)

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as fuzz-only dev deps), ADR-0010 (nightly toolchain, explicit `+nightly` CI invocation; workspace-root pin stays stable), ADR-0011 (phase-01 defers response-header equivalence to phase 04; `server: envoy-rust` tolerated until then), ADR-0012 (nested nightly pin in fuzz subcrate; narrowly supersedes ADR-0010 on that single sub-point while preserving its main decision).

### Phase-02.1 ADR ledger (for reference)

ADR-0013 (split phase 02 into 02.1 + 02.2; landed at `1c38ca9` during parent-phase 02 state 2), ADR-0014 (YAML-native `typed_config` deserialization until the xDS/protos family lands; landed at `6d1f8d6` during 02.1 Task 1).

### Parent phase 02 lifecycle after the split

Per `ROADMAP.md` schema ("The parent flips to `done` only after all sub-phases are `done`"), row 02 stays `in-progress` until both 02.1 and 02.2 have landed their final commits. The canonical flip-to-`done` for row 02 happens in the **same commit** as 02.2's final commit (see `02.2-listener-tcp-proxy/SPEC.md` §8).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The state-2 closure commit (landing this STATE.md edit) advances phase 02.2 to lifecycle state 3. The next session enters phase 02.2 state 3 via `superpowers:subagent-driven-development`, beginning with PLAN.md Task 1.
