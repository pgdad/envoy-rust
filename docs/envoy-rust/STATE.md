# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `03.1`
**slug:** `03.1-tls-foundation-downstream` (created during the state-2-of-parent-03 split commit `f256d2c`; SPEC.md committed alongside).
**directory:** `docs/envoy-rust/phases/03.1-tls-foundation-downstream/` (exists; contains `SPEC.md`, `PLAN.md`, `PROGRESS.md`, and `REVIEW.md`).
**status:** phase 03.1 lifecycle **state 5 (REVIEW.md approved; state-6 phase-done commit next)** — `SPEC.md` landed at `f256d2c` (alongside the ADR-0017 split decision); `PLAN.md` landed at `19e14af`; all 13 PLAN.md tasks implemented across `f93a062..5897d94`; state-4 phase-done gate cleared at task 13 with the local stable-toolchain gate clean on first attempt (PROGRESS.md §"Task 13 / State 4" — 146 tests passing, 1 ignored Docker-gated, `cargo deny check` clean) at `beefd8e`; `Cargo.lock` sync at `eb039e6` (phase-01/02.1/02.2 precedent shape); REVIEW.md landed with verdict **Approved with fixes → Approved** (REVIEW.md §6/§7) in the same commit that flips this STATE.md from state 3 to state 5. ROADMAP row `03.1` remains `status: in-progress` until the phase-done commit (which flips it to `done`); parent row `03` stays `in-progress` per the ROADMAP-schema invariant ("parent flips to `done` only after all sub-phases are `done`") — 03.2 has not landed yet, so the parent flip waits.

Parent phase `03-tls-tcp` is **in-progress** with `sub-phases: 03.1, 03.2` per ADR-0017's split (landed at `f256d2c`). Parent SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `a3f3474`); for execution purposes it is superseded by the two sub-phase SPECs (`phases/03.1-tls-foundation-downstream/SPEC.md` and `phases/03.2-tls-upstream-sni/SPEC.md`).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 44–48, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 6): the next session — operating as the state-6 session of phase 03.1 — lands the **phase-done commit** with message format per `BOOTSTRAP_PROMPT.md` §5.3:

```
phase 03.1: envoy-tls foundation + downstream TLS termination + fixture 0004 [ADR-0017,ADR-0018,ADR-0019]

<3–6 paragraph narrative covering landed surface (envoy-config TransportSocket
envelope + DownstreamTlsContext + UpstreamTlsContext + 12 new validator tests +
5 new ConfigError variants + 3 fuzz-corpus seeds; envoy-tls new crate with
DownstreamTls + UpstreamTls + SingleCertResolver + install_default_crypto_provider
helper + 10 tests; envoy-tcp generic-stream lift + 4 TLS-flavored unit tests;
envoy-bin TlsAcceptingHandler in tls_handler.rs + filter-chain TLS dispatch +
crypto-provider install routed through envoy-tls + Rust-native integration test
in tests/tls_downstream.rs; differential harness TlsTestPki rcgen-driven +
Driver::TlsTcp + drive_tls + with_copy_to PEM mounts + 6 new harness tests;
fixture 0004-tls-downstream Docker-gated acceptance test); equivalence/
conformance status (146 tests green + 1 ignored Docker-gated; CI runs
tls_downstream_fixture against real Envoy); gate evidence (PROGRESS.md
§"Task 13 / State 4" first-attempt clean — build/clippy/fmt/test/deny;
REVIEW §6 Approved with fixes → §7 close-out → Approved); rollovers carried
forward to phase 03.2 / later (REVIEW.md §3 M1–M5 + §4 recommendations).>
```

The commit flips `ROADMAP.md` row `03.1` `status` from `in-progress` to `done`. **Parent row `03` stays `in-progress`** per the ROADMAP schema invariant ("parent flips to `done` only after all sub-phases are `done`") — phase `03.2-tls-upstream-sni` has not yet landed, so parent-03 waits for the 03.2 phase-done commit. This is a deviation from the phase-02.2 state-6 shape (which atomically flipped both the sub-phase row and the parent row), and is the correct interpretation of the schema for the 03.1-first-of-two-sub-phases case.

The commit also advances this STATE.md from phase `03.1-tls-foundation-downstream` to phase `03.2-tls-upstream-sni` (lifecycle state 2 — `SPEC.md` already exists from the ADR-0017 split commit `f256d2c`; `PLAN.md` does not; next-skill `superpowers:writing-plans` scoped to phase 03.2). State 6 is a docs-only commit (no code changes); no further review is required — REVIEW.md's §7 final verdict stands.

Inputs the state-6 session should read, in order:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (rows 03, 03.1, 03.2; ensure the flip touches row 03.1 only — parent row 03 stays `in-progress`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0019`).
5. `docs/envoy-rust/SKILL_ROUTING.md` state 6 block.
6. `docs/envoy-rust/phases/03.1-tls-foundation-downstream/SPEC.md` §9 (commit discipline / phase-done commit conventions).
7. `docs/envoy-rust/phases/03.1-tls-foundation-downstream/REVIEW.md` (Approved verdict — §7 close-out; mine the §1–2 narrative for the commit message body).
8. `docs/envoy-rust/phases/03.1-tls-foundation-downstream/PROGRESS.md` (state-4 gate evidence and per-task narrative for the commit message body).
9. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/` (shape precedent — `f04e21a` is the state-6 phase-done commit; `fc87505` is the state-5 REVIEW-landing precedent for the atomic "REVIEW.md + STATE.md advance" commit shape this session lands; note however that 02.2's state-6 atomically flipped both 02.2 and parent 02, whereas 03.1's state-6 flips only 03.1).
10. `docs/envoy-rust/phases/03.2-tls-upstream-sni/SPEC.md` (sibling sub-phase SPEC — the state-6 successor activates phase 03.2 at lifecycle state 2; this SPEC is the design contract the next state-2 session will operationalize into a PLAN.md).
11. `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` (parent SPEC — last touched at SHA `a3f3474`; remains in-tree unedited as the committed historical artifact, but referenced for the commit narrative's "parent phase mid-flight" framing).

## Last commit

REVIEW.md landing commit (this session): lands `docs/envoy-rust/phases/03.1-tls-foundation-downstream/REVIEW.md` with state-5 verdict **Approved** (per §7 I1 close-out) and advances this STATE.md from state 3 to state 5. Preceded in this lifecycle by `beefd8e` (Task 13 state-4 phase-done gate verification — workspace test/clippy/fmt/deny all clean) and `eb039e6` (Cargo.lock sync — phase-01/02.1/02.2 precedent shape).

## Last updated

2026-04-26 (phase 03.1 lifecycle state advanced from 3 to 5; REVIEW.md committed with verdict Approved; next-skill flips from `superpowers:subagent-driven-development` to the state-6 phase-done commit per `BOOTSTRAP_PROMPT.md` §5 state 6).

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
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The state-5 close (landing this STATE.md edit alongside REVIEW.md) advances phase 03.1 to lifecycle state 5. The next session enters phase 03.1 state 6 — the phase-done commit per `BOOTSTRAP_PROMPT.md` §5.3 / `SKILL_ROUTING.md` state 6.
