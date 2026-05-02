# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** TBD (next sub-phase under parent-05 — a free-standing follow-up to 05.1 that diagnoses + closes the residual fixture-0008 defect surfaced after 05.1's STRICT_DNS preamble; canonical id + slug picked at the state-1 brainstorm session per `BOOTSTRAP_PROMPT.md` §1 / SKILL_ROUTING state 0 → state 1 transition).
**slug:** TBD (brainstorm-time; reviewer working name was `05.1-followup` per phase-05.1 `REVIEW.md` §5 R1).
**directory:** does not yet exist; the state-1 brainstorm session creates `docs/envoy-rust/phases/<id>-<slug>/`. Sibling directories `05.1-fixture-hardening/` (now closed; contains `SPEC.md` + `PLAN.md` + `PROGRESS.md` + `REVIEW.md`), `05.2-http2-downstream/`, and `05.3-http2-upstream/` exist with their own `SPEC.md` files committed at the parent-05 state-2 commit `f1804a7`. Parent directory `docs/envoy-rust/phases/05-http2/` continues to hold the parent SPEC at `SPEC.md` (committed at parent-05 state-1 commit `cd1a70e`; 490 lines, unedited per D-3.4 / D-3.5 — the parent SPEC remains the historical artifact projecting the original 3-way split, superseded for execution by the three sub-phase SPECs and now augmented by this commit's pending follow-up).
**status:** the next sub-phase is at lifecycle **state 0 (phase not yet in ROADMAP)** — the state-1 brainstorm session adds the row, creates the directory, and lands `SPEC.md` per `SKILL_ROUTING.md` line 9 (state 0 → state 1 transition). ROADMAP row `05` remains `status: in-progress` (will flip to `done` only after ALL parent-05 sub-phases close, per ROADMAP-schema invariant — the new follow-up sub-phase must be added to row `05`'s `sub-phases` column at its brainstorm-time row-add commit). ROADMAP row `05.1` is `status: done` as of this commit (was `planned` at the parent-05 state-2 commit `f1804a7`; flipped to `done` here without an intermediate `in-progress` flip per the same compressed-flip pattern observed across the active-phase execution, since the parent-05 state-2 commit established the row and the sub-phase ran inside STATE.md's active-phase pointer without a discrete ROADMAP-side `planned → in-progress` event). ROADMAP rows `05.2` and `05.3` remain `status: planned`; **05.2 is soft-gated by STATE.md** (this file does NOT advance to 05.2's PLAN-state until the C-1 follow-up sub-phase closes, because parent-05 SPEC §3 explicitly notes "05.2 depends on 05.1's restored Docker-gated baseline" and the 05.1 state-4 phase-done gate left fixture 0008 red — see "Phase-05.1 rollovers" below). The C-1 follow-up sub-phase's depends-on chain in ROADMAP will read `05.1`; 05.2's `depends-on: 05.1` ROADMAP-row content stays unedited per the ROADMAP append-only rule (only `status` and `sub-phases` columns mutate).

Phase 05.1 (`05.1-fixture-hardening`) is **done** as of this commit (the phase-05.1 state-6 close-out). The work landed in 4 substantive tasks across 11 commits between base `f1804a7` and head `a64d9fc`: Task 1 (`bfabcb6` + review-fix `7391a4e`) extended `crates/envoy-config/src/bootstrap.rs::ClusterType` from single-variant `Static` to `Static | StrictDns` and landed ADR-0023; Task 2 (`f7a555d`) promoted `crates/envoy-cluster/src/cluster.rs::from_bootstrap` to `async fn` with a `tokio::net::lookup_host` STRICT_DNS resolution branch + new `ClusterError::DnsResolutionFailed` variant + the I3-closing `static_cluster_constructs_with_literal_ip` test; Task 3 (`0ce0aa2`) flipped `type: STATIC` → `type: STRICT_DNS` in 10 YAML files across the 5 Docker-gated fixtures; Task 4 (`b7fe910` + backfill `a64d9fc`) materialized the state-4 phase-done gate evidence honestly recording a RED CI run on fixture 0008. Phase 05.1 `REVIEW.md` (landed at `283a4b9`) verdict is **Approved with M-track follow-ups** — see "Phase-05.1 rollovers" below for the carryforward disposition.

Phase 04 (`04-http1`) is **done** as of commit `e626862` (the phase-04.3 state-6 close-out). All three sub-phases are done: `04.1-hcm-direct-response` (commit `c5c40ec`), `04.2-route-matchers` (commit `04163c5`), and `04.3-router-upstream` (commit `e626862`). ROADMAP rows `04`, `04.1`, `04.2`, and `04.3` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `805433e`); for execution purposes it was superseded by the three sub-phase SPECs.

Phase 04.3 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `eb030d1`; no Critical or Important findings in the 04.3 surface itself; one cross-phase Important carryforward (C-1 Docker-gated `host.docker.internal`/`STATIC` regression originating at phase-02.2 ADR-0015, latent across five phases, surfaced by 04.3's CI push cadence and fixture 0008 inheritance) + 4 awareness-only Minor findings (M3-correction PROGRESS Task 16 imprecise on M3 closure attribution, M-claim drive_http1 per-function unit test never landed, M-payload payload.bin empty by design, M-spec-equiv expectations.yaml SPEC drift), all explicitly named in REVIEW §3 + §4 — see "Phase-04.3 rollovers" below). 4 in-phase items closed (M3 / M6 / M10 / #12 Cluster::name() carryforward); 11 forward-track items propagated. Eight in-phase review-fix commits closed substantive findings before propagation (Tasks 2/8/9/11/12/13/14/16).

Phase 04.2 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `c1ff7b6`; closed in 04.3 are M3 / M6 / M10 plus the multi-phase #12 `Cluster::name()` carryforward — see "Phase-04.3 rollovers" below).

Phase 04.1 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `b6e305d`; M3 closed in 04.3; M1 / M2 / M4 / M5 / M7 carry forward to phase 05+ / hardening — see "Phase-04.3 rollovers" below).

Phase 03 (`03-tls-tcp`) is **done** as of commit `ca81226`. Both sub-phases are done: `03.1-tls-foundation-downstream` (commit `64ea760`) and `03.2-tls-upstream-sni` (commit `ca81226`). ROADMAP rows `03`, `03.1`, and `03.2` are all `status: done`.

Phase 03.2 `REVIEW.md` verdict is **Approved with fixes** (state 5 complete; I1 closed in-phase; M1–M5 tracked forward — see "Phase-03.2 rollovers" below). Phase 03.1 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase; M1–M5 tracked forward — see "Phase-03.1 rollovers" below).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`.

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase; M1–M4 tracked forward — see "Phase-02.2 rollovers" below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see "Phase-02.1 rollovers" below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` line 9, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 0): the next session — operating as the **state-1 brainstorm session of a free-standing post-05.1 follow-up sub-phase under parent-05** — invokes **`superpowers:brainstorming`** scoped to the residual fixture-0008 defect surfaced after 05.1's STRICT_DNS preamble landed. Output: a new ROADMAP row + a new `docs/envoy-rust/phases/<id>-<slug>/SPEC.md`, plus the parent ROADMAP row `05`'s `sub-phases` column extended to include the new id (and STATE advance to that new sub-phase's lifecycle state 2). The brainstorm session is also responsible for naming the new sub-phase (`REVIEW.md` §5 R2 working name `05.1-followup`; canonical id is the brainstorm session's call — see "Disposition decision" subsection below).

**Disposition decision (codified at this commit per phase-05.1 REVIEW.md §5 R2):** option (b) — treat 05.1 as fully closed at the preamble landing, and track the residual fixture-0008 defect as a **free-standing post-05.1 sub-phase under parent-05**. The reviewer's option (a) — retroactively splitting 05.1 into 05.1.1 (preamble) + 05.1.2 (follow-up) — was rejected because (i) it would imply renaming what already landed at the 05.1 head commit `a64d9fc`, against the spirit of D-3.5 / D-3.6 audit-trail discipline; (ii) `BOOTSTRAP_PROMPT.md` §6.1 explicitly flags nested splits of an already-split sub-phase as suspicious and requires `superpowers:systematic-debugging` before proceeding; and (iii) the 05.1 SPEC itself (`docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` line ~35 routing reference) reaffirms "Sub-phase 05.1 does NOT re-split" — preserving that invariant by adding a sibling rather than nesting is the cleanest reading of the M-track-follow-ups verdict shape. The new follow-up sub-phase is therefore a SIBLING under parent-05, NOT a child of 05.1.

After the C-1 follow-up sub-phase closes (its own state-6 phase-done commit), STATE.md will advance to sub-phase 05.2 lifecycle state 2, then to 05.3 lifecycle state 2 in the same way; the **last** sub-phase commit (05.3's state-6) ALSO flips parent ROADMAP row `05` `in-progress` → `done` per the ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"; mirrors phase 04's `e626862`-shape close-out).

The original 3-way split decision is codified in **ADR-0022** (landed at parent-05 state-2 commit `f1804a7`; see DECISIONS.md). The C-1 follow-up sub-phase is an addition to (not a supersession of) ADR-0022's projected sub-phase set — its state-1 brainstorm may land an ADR documenting the parent-05 sub-phase set extension if doctrine requires (the brainstorm session is the natural place to make that call). The DECISIONS.md ledger head is currently **ADR-0023** (landed at 05.1 Task 1 commit `bfabcb6`); the follow-up sub-phase's projected ADRs (if any) start at ADR-0024 — which had been conditionally projected for ADR-0022 / 05.2 Task 1 (`http` crate scoping per parent-05 SPEC §7), and may land at the follow-up's brainstorm if the diagnosis surface includes a permitted-foundations question, otherwise stays available for 05.2.

Updated parent-05 sub-phase set (after this commit's disposition decision):

- **05.1 `fixture-hardening`** — DONE at this commit. `ClusterType::StrictDns` schema variant + `Cluster::from_bootstrap` async-promotion with STRICT_DNS resolution branch + 5-fixture coordinated YAML edit + closes phase-02.1 REVIEW I3 + ADR-0023 landed at Task 1. **Did NOT close phase-04.3 REVIEW C-1** — the schema + runtime + YAML preamble was necessary but not sufficient for the upstream-routing path through `host.docker.internal`; fixture 0008 surfaces a different defect post-flip (upstream Envoy returns 503 vs envoy-rust returns 200 under `response_status: exact`). Closure deferred to the C-1 follow-up sub-phase below.
- **TBD `<C-1 follow-up>`** — PENDING brainstorm. Scope per phase-05.1 REVIEW.md §5 R1: diagnose fixture 0008's `response_status: exact` mismatch (likely an upstream-routing defect at the `host.docker.internal` host-gateway boundary; the 503 from upstream Envoy suggests its upstream cluster construction or http1-echo-server reachability through host-gateway is failing differently than the 02.1-era `STATIC`+`host.docker.internal` parse failure); per-fix LoC budget with explicit ADR projections (the 6 root-cause patches preserved on local branch `backup/task4-scope-creep-2026-05-02` may need 1-3 ADRs to anchor the fixes); verification gate (5-fixture green Docker-gated CI run as the substantive close-out of phase-04.3 REVIEW C-1).
- **05.2 `http2-downstream`** — PLANNED; soft-gated by STATE.md until C-1 follow-up closes (do NOT advance STATE.md here when 05.1 closes; 05.2's PLAN-state entry waits for the follow-up's state-6).
- **05.3 `http2-upstream`** — PLANNED; depends on 05.2.

Strict ordering: 05.1 (done) → C-1-followup (TBD) → 05.2 → 05.3. The follow-up is inserted between 05.1 and 05.2 in dependency order even though its ROADMAP id will be lexicographically after 05.3 (or differently named per brainstorm); STATE.md is the soft-gate.

**Standing context for the C-1 follow-up state-1 brainstorm session:**

- **The 6 root-cause patches preserved on `backup/task4-scope-creep-2026-05-02`** are an existing-but-not-graded artifact from the in-session aborted Task 4 expansion attempt (commits `9279895` / `2d3d679` / `339b3c7`, since reset and force-pushed away from `main` at the 05.1 state-4 gate). They were preserved for SPEC-anchored re-adoption and should be reviewed at brainstorm time — some may be re-usable as-is; some may need re-architecture under the follow-up's discipline.
- **CI run `25258722850`** captures the canonical red Docker-gated state at HEAD `4768fcd` (Task 3's coordinated YAML flip + Task 2's `tokio::net::lookup_host` runtime + Task 1's `STRICT_DNS` schema). The brainstorm should pull the testcontainers logs from this run (URL: `https://github.com/pgdad/envoy-rust/actions/runs/25258722850`) to anchor the diagnosis. Per-fixture matrix per phase-05.1 PROGRESS.md Task 4: 0001/0002/0007 GREEN; 0008 RED with `response_status: exact` mismatch (upstream 503, subject 200); 0003/0004/0005/0006 NOT RUN (cargo test exits at first failing binary).
- **Per the user's standing preference** (auto-memory `feedback_execution_style`), state-3 execution of the C-1 follow-up will use `superpowers:subagent-driven-development` over inline `executing-plans` — do not present the two-option fork at state-3 entry.
- **PLAN.md cadence** continues the 04.3 / 05.1 standardized posture: PLAN.md commits cleanly at state-2 close-out as a standalone pre-Task-1 commit (precedent: 04.3's `c02eea7`, 05.1's `f23d08f`).
- **The 05.1 SPEC's "do NOT re-split" invariant binds 05.1 itself, NOT the C-1 follow-up.** The follow-up sub-phase is a sibling under parent-05, free to be split if its scope warrants per `BOOTSTRAP_PROMPT.md` §6.1.

Inputs the C-1 follow-up state-1 brainstorm session should read, in order, before launching the brainstorm:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing + the disposition decision codified above).
3. `docs/envoy-rust/ROADMAP.md` (row 05.1 `done` after this commit; row 05 `in-progress`; rows 05.2 + 05.3 `planned`; rows 04 + 04.x and earlier all `done`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0023`; the follow-up's projected ADRs start at ADR-0024).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (Header allow-list section has 3 phase-04 rows; the C-1 follow-up may or may not edit depending on whether its diagnosis touches the request/response header surface — likely no edits given the diagnosis is at the connect/upstream-routing layer).
6. `docs/envoy-rust/SKILL_ROUTING.md` (state machine).
7. **`docs/envoy-rust/phases/05.1-fixture-hardening/REVIEW.md`** — the source of the disposition decision codified above. §3 I1 enumerates the unmet-D4 surface; §5 R1/R2/R3 enumerate the recommended forward-work shape; §6 enumerates the verifications-run-locally context.
8. **`docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md`** — Task 4 lines 102-159 carry the state-4 gate evidence + the executor's narrative on the aborted in-session expansion + the per-fixture CI matrix.
9. `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` — for the deliverable contract that 05.1 honored (D1/D2/D3/D5 closed; D4 unmet, deferred here).
10. `docs/envoy-rust/phases/05-http2/SPEC.md` (parent-05 SPEC; 490 lines) — for cross-sub-phase architectural rules + non-goals the follow-up inherits.
11. `docs/envoy-rust/phases/04.3-router-upstream/REVIEW.md` — the C-1 carryforward's origin documentation.
12. `docs/envoy-rust/phases/02.2-listener-tcp-proxy/SPEC.md` + DECISIONS.md ADR-0015 — the original `host.docker.internal` + `host-gateway` reachability decision; the follow-up's diagnosis may need to touch this.
13. The 6 patches on local branch `backup/task4-scope-creep-2026-05-02` — review for re-adoption per the standing context bullet above.
14. CI run `25258722850` testcontainers logs — anchor the diagnosis surface.
15. `BOOTSTRAP_PROMPT.md` §5 state 0 + state 1 (brainstorming skill routing) + §6.1 / §6.2 (split gate — the follow-up may split if scope warrants, mirroring parent-05's split decision shape).

## Last commit

Phase 05.1 state-6 phase-done commit (this commit): touches `docs/envoy-rust/ROADMAP.md` and `docs/envoy-rust/STATE.md` only. Flips ROADMAP row `05.1` `status` from `planned` to `done` (the row was added as `planned` at parent-05 state-2 commit `f1804a7`; never carried an intermediate `in-progress` flip — the active-phase pointer in STATE.md was the operative gate during execution). Parent row `05` stays `status: in-progress` per ROADMAP-schema invariant ("the parent flips to `done` only after all sub-phases are `done`"; rows `05.2` and `05.3` are still `planned`, plus the C-1 follow-up sub-phase will be added at its state-1 brainstorm commit). The 05.1 row's `summary` column is also amended to honestly reflect the partial-close: "phase-04.3 REVIEW C-1 partially closed; full close deferred to follow-up sub-phase" + "Docker-gated 5-fixture re-baseline NOT met (0008 surfaces a different defect post-flip; closure deferred to a follow-up sub-phase per REVIEW.md §5 R1)" — this departs from the original ROADMAP `summary` ("fixtures 0003/0004/0005/0006/0008 restored to Docker-gated green") to match reality. STATE.md advances active phase from sub-phase 05.1 lifecycle state 5 (REVIEW.md Approved) to a free-standing post-05.1 follow-up sub-phase under parent-05 at lifecycle state 0 (phase not yet in ROADMAP). Next-skill `superpowers:brainstorming` scoped to the C-1 follow-up sub-phase. Adds the "Phase-05.1 rollovers" Notes section (carryforward bookkeeping for I3 closed, C-1 deferred to the follow-up sub-phase, M-claim continued deferral, plus the disposition-decision rationale per REVIEW.md §5 R2 option (b)).

No code changes. `ENVOY_TARGET.md` and `rust-toolchain.toml` untouched (D-3.7 / D-3.9). DECISIONS.md unchanged (ADR-0001 through ADR-0023 byte-identical; D-3.5 append-only). Mirrors phase 04.3's `e626862`-shape sub-phase-done close-out (sub-phase done; STATE advance to next-planned brainstorm; parent stays in-progress until all sub-phases close), with the deviation that the next-planned step is the C-1 follow-up's state-1 brainstorm rather than 05.2's state-2 PLAN-write — soft-gated per the disposition decision codified in the "Next expected skill" section above.

Predecessor commits:

- `283a4b9` — `phase 05.1: state 5 REVIEW.md Approved with M-track follow-ups` (state-5 close-out; landed REVIEW.md only; verdict **Approved with M-track follow-ups**; one Important issue I1 — D4 Docker-gated 5-fixture green re-baseline unmet, fixture 0008 surfaces a different defect post-STRICT_DNS-flip; six awareness-only Minor findings A1-A6; three Recommendations R1-R3 codifying the disposition guidance the orchestrator follows in this commit's Next-skill block).
- `a64d9fc` — `phase 05.1: progress note (task 4 — backfill verification SHA)` (Task 4 backfill of the verification commit's SHA into PROGRESS.md narrative; last 05.1 commit before state 5).
- `b7fe910` — `phase 05.1: state-4 phase-done gate verification (task 4)` (state-4 close-out; state-4 gate **RED** on Docker-gated fixture 0008 with 0003/0004/0005/0006 NOT RUN; commit message body honestly states "§7.5 NOT met at this commit" + "Phase-04.3 REVIEW C-1 NOT materially closed; closure deferred to a follow-up sub-phase under proper SPEC + ADR discipline"; preserved the executor's discipline of not folding root-cause diagnosis into 05.1's narrowly-scoped fixture-hardening preamble).
- `0ce0aa2` — `phase 05.1: 5-fixture coordinated YAML edit — STATIC → STRICT_DNS` (Task 3; the bundled 10-file YAML flip).
- `f7a555d` — `phase 05.1: tokio dep + async from_bootstrap + STRICT_DNS branch + I3 close` (Task 2; the runtime-side landing including the I3-closing positive Static regression guard `static_cluster_constructs_with_literal_ip` at `crates/envoy-cluster/src/cluster.rs:569`).
- `bfabcb6` — `phase 05.1: ClusterType::StrictDns + ADR-0023 + 6 validator tests + fuzz seed` (Task 1; the schema landing including ADR-0023 inline).
- `f23d08f` — `phase 05.1: state-2 PLAN.md (pre-Task-1 standalone per c02eea7 precedent)` (state-2 PLAN.md as standalone pre-Task-1 commit per the 04.3-established cadence — M10 closure precedent continued).
- `f1804a7` — `phase 05: state-2 split formalization — ADR-0022 + 3 sub-phase SPECs (05.1/05.2/05.3)` (parent-05 state-2 close-out that added the 05.1 row to ROADMAP and the 05.1 SPEC at `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md`).
- `e626862` — `phase 04.3: HTTP/1.1 upstream origination + router proxy arm [parent 04 done]` — phase 04 close-out + the C-1 carryforward origin that 05.1 partially closed.

## Last updated

2026-05-02 (phase 05.1 state-6 close-out; ROADMAP row 05.1 `planned` → `done`; ROADMAP row 05.1 `summary` amended to reflect the partial close-out per REVIEW.md §5 R2; STATE advances to a free-standing post-05.1 follow-up sub-phase under parent-05 at lifecycle state 0; next-skill `superpowers:brainstorming` scoped to the C-1 follow-up. Phase 05.1 deliverables landed: D1 (`ClusterType::StrictDns` schema variant), D2 (`Cluster::from_bootstrap` async-promotion + STRICT_DNS resolution branch via `tokio::net::lookup_host` + `ClusterError::DnsResolutionFailed` variant), D3 (5-fixture coordinated YAML flip), D5 (phase-02.1 REVIEW I3 closed via `static_cluster_constructs_with_literal_ip`). Phase 05.1 deliverable NOT met: D4 (Docker-gated 5-fixture green re-baseline; fixture 0008 still RED with a different defect than the original C-1 trace; closure deferred to the C-1 follow-up sub-phase). ADR-0023 landed at Task 1 commit `bfabcb6`).

## Notes

### ADR numbering after the phase-03 split

The parent-phase-03 SPEC (`03-tls-tcp/SPEC.md`, committed at SHA `a3f3474`) projected three phase-03 ADRs numbered 0017 (`rcgen` + `tempfile`), 0018 (`tokio-rustls` + `rustls-pemfile`), 0019 (split phase 03). The ADR-0017 split decision (landed at `f256d2c`) took the actual next-sequential number at split time, so each projected ADR shifted in-tree:

- **ADR-0017** — split phase 03 into 03.1 + 03.2 (landed at `f256d2c`; was parent-SPEC §7's projected ADR-0019).
- **ADR-0018** — `rcgen` + `tempfile` permitted as dev-test-harness-only foundations (landed at `f93a062` during 03.1 Task 1; was parent-SPEC §7's projected ADR-0017).
- **ADR-0019** — `tokio-rustls` + `rustls-pemfile` covered by the rustls foundations grant (landed at `f93a062` during 03.1 Task 1; was parent-SPEC §7's projected ADR-0018).

The sub-phase SPECs (03.1 + 03.2) cite ADR-0017 for the renumbering and rewrite each expected ADR with its actual landed number. The parent SPEC (`docs/envoy-rust/phases/03-tls-tcp/SPEC.md`) is preserved unedited per D-3.4 / D-3.5.

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

All phase-01 starter items are now closed. No phase-01 rollovers carry into phase 05 (or its sub-phases).

### Phase-02.1 rollovers (final disposition)

The initial 02.1 REVIEW (HEAD `95a26a7`) landed with three Important items and four Minor items. I1 (Cargo.lock drift) closed at `dea4d16`; I2 (STATE.md stale) closed by state-5 commit `379937b`. The remaining items:

- **I3** — positive `ClusterType::Static` test (`bootstrap.rs:48–54` variant name regression guard): **tracked forward to whichever phase extends `ClusterType`**. Phase 04.3 did not extend `ClusterType` (router proxy reuses the existing `Static` variant); the C-1 fixture-hardening sub-phase OR phase 05 brainstorm scope is the natural close site (see Phase-04.3 rollovers C-1 below: `ClusterType::StrictDns` is the proposed schema growth and would close I3 in the same scope).
- **M1** — `pub(crate) fn Cluster::name(&self) -> &str` accessor: **CLOSED in 04.3 Task 9** at commit `3fdf960`. The accessor visibility was lifted from the originally-projected `pub(crate)` to `pub` because the consumer lives in `envoy-http1` (different crate from `envoy-cluster`); per 04.3 SPEC §3 D5 this lift is authorized. Field-level `#[allow(dead_code)]` removed; consumed by router-arm `tracing::warn!` log lines at `crates/envoy-http1/src/hcm.rs:208`/`:248`/`:265`. The carryforward chain phase-02.1 → 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 ends here.
- **M2** — `echoes_round_trip` drop-before-send ordering in `tests/helpers/tcp-echo-server/src/main.rs`: awareness-only, no action required.
- **M3** — drop the dead `|| msg.contains("CRLF")` disjunct in `tests/differential/src/lib.rs`: **closed** opportunistically by 02.2 Task 11 at commit `aa4187f`.
- **M4** — style-only: `ClusterManager::get` does `Arc::clone` inside a `.map` closure: no action required.

### Phase-02.2 rollovers (from REVIEW.md §3–§4)

The 02.2 REVIEW landed with one Important item and four Minor items. I1 (STATE.md stale) closed in-phase by the §7 close-out commit `fc87505`. The remaining items:

- **M1** — `TcpProxyBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread: **tracked forward to whichever phase first parallelizes `run_fixture` across worker threads**. Phase 03.1 + 03.2 + 04.1 + 04.2 + 04.3 do not parallelize fixtures; the same is anticipated for 05+. The `TlsEchoBackend` 03.2 ships and the `Http1EchoBackend` 04.3 ships inherit the same posture.
- **M2** — `proxies_returns_err_on_upstream_connect_refused` asserts on the formatted error string rather than the typed variant: awareness-only, no action required.
- **M3** — `proxies_closes_downstream_on_upstream_close` has implicit timing on the upstream's "tail" read: awareness-only, no action required.
- **M4** — `Listener::serve`'s `JoinSet` type aliases a long generic: **tracked forward to phase 07** when a richer filter trait warrants a `pub type HandlerResult = ...` alias.

Phase 02.2 REVIEW §4 recommendations: items 1 (`Cluster::name()`) closed in 04.3; items 2/3/5/6 carry forward unchanged to phase 05+; item 4 (round-robin distribution-equivalence assertion remains unit-test-only) carries unchanged.

### Phase-03.1 rollovers (from REVIEW.md §3–§4)

The 03.1 REVIEW landed with one Important item and five Minor items. I1 (STATE.md stale at state 3) closed in-phase by the §7 close-out commit `1748cd2`. The remaining items M1–M5 are awareness-only or tracked-forward; M3 (`x509-parser`-style structured introspection) is still deferred — 04.3 introduces no mTLS or peer-cert-attribution headers so the carryforward continues to phase 05+.

### Phase-03.2 rollovers (from REVIEW.md §3–§4)

The 03.2 REVIEW landed with one Important item and five Minor items. I1 closed in-phase. M1–M5 are awareness-only or tracked-forward (M3 `drive_*` factoring still deferred — 04.1 + 04.2 + 04.3 did not surface the third helper; 05+ may if HTTP/2's response reader shares structural shape; M5 `tls-echo-server` argv-test parity still optional polish — 04.3's `http1-echo-server` shipped with full coverage from the start, mooting M5 for new helpers but leaving the original gap).

Phase 03.2 REVIEW §4 forward-recommendations: items 1 (`Cluster::name()`) closed in 04.3; items 2 (`x509-parser`), 5 (round-robin), 6 (parallel fixtures), 7 (`enable_half_close: true`), 8 (`tls_params` floor), 9/10 (optional polish) carry forward unchanged to phase 05+.

### Phase-04.1 rollovers (from REVIEW.md §3–§4)

The 04.1 REVIEW landed with zero Critical and zero Important items, 4 Minor findings (M1/M2/M4/M5) and 3 awareness-only Minor findings (M3/M6/M7). 04.2 did not close any. 04.3 closed M3 (structurally consumed via `HCMConfig.cluster_mgr: Arc<envoy_cluster::ClusterManager>` at Task 9 commit `3fdf960`) and M6 (practically — Task 6/9/13 added end-to-end exercise, hedged closure noted; the strict per-function `drive_http1` unit test in isolation was never added). Items still carrying forward:

- **M1** — `diff_headers` value-comparison uses `find()` for value lookup, silently ignoring duplicate-header value mismatches. 04.3 fixture 0008 has no duplicate-header response shape (single `Set-Cookie`/`Vary` not exercised). **Tracked forward to whichever phase first emits duplicate response headers** (HTTP/2's HPACK-derived header semantics may surface this, or hardening pass).
- **M2** — Body-drain idle timeout returns `Ok(())` silently on read timeout. 04.3 fixture 0008 deterministic-echo body is small and well-framed; not exercised. **Tracked forward to hardening pass or whichever phase first introduces non-trivial bodies that may stall.**
- **M4** — `strip_port` uses `rfind(':')`; incorrect for bare-IPv6 Host. 04.3 used a DNS-name Host so not exercised. **Tracked forward to hardening pass or first IPv6-Host fixture.** May also surface in phase-05 H2 if `:authority` pseudo-header carries IPv6.
- **M5** — Cargo.lock sync cadence diverges from phase-01/02.x/03.x precedent. 04.1, 04.2, 04.3 all used inline-at-scaffold; the next phase that adds a workspace member should pick a cadence consciously and either supersede ADR-0021 or document inline. **Tracked forward to phase 05+** — coupled with M9.
- **M7** — `TlsAcceptingHandler.inner: Arc<TcpProxy>` field is concrete-typed; HCM-in-TLS would not typecheck. 04.3 introduces no TLS-bearing HCM fixtures. **Tracked forward to phase 05+ brainstorm** — phase 05's H2-on-TLS will likely force this since H2 typically requires ALPN, which means the dispatch layer needs a trait-level boxing or parallel `TlsAcceptingHcmHandler`.

### Phase-04.2 rollovers (from REVIEW.md §3–§4)

The 04.2 REVIEW landed with zero Critical and zero Important items, 4 new Minor findings (M8–M11). 04.3 closed M10 cleanly (the standalone pre-Task-1 PLAN.md commit `c02eea7` broke the 04.1 → 04.2 inline-PLAN precedent and is now the standardized cadence). Items still carrying forward:

- **M8** — `safe_regex_partial_eq_compares_only_regex_string` test asserts opaque equality; not exercised by 04.3 (no consumer compares RouteConfiguration values post-validate). **Tracked forward to first phase that does config-diff** (e.g. xDS family).
- **M9** — ADR-0021's "dedicated state-4 commit" Consequences prose contradicts the actual Cargo.lock-inline cadence. 04.3 inherited inline. Per D-3.5 ADRs are append-only. **Tracked forward alongside M5**: the next phase (phase 05+) that adds a workspace member should supersede ADR-0021 or document inline as the project's standardized posture.
- **M11** — `Http1Probe.extra_headers` duplicate semantics, coupled with M1. 04.3 fixture 0008 uses `Driver::Http1` (single probe), not `Driver::Http1ProbeList`; not exercised. **Tracked forward alongside M1** to whichever phase first emits duplicate request/response headers.

Phase 04.2 closed M5 partially (PROGRESS-disclosure form) but not substantively; M5 remains carried forward per Phase-04.1 rollovers above.

### Phase-04.3 rollovers (from REVIEW.md §3–§4)

The 04.3 REVIEW landed with zero Critical items, zero Important items in the 04.3 surface itself, 1 Important cross-phase carryforward (C-1), and 4 awareness-only Minor findings. Eight in-phase review-fix commits closed substantive findings before propagation (Tasks 2/8/9/11/12/13/14/16). Items closed in 04.3:

- **04.1 M3** (`envoy-http1`'s forward-looking `envoy-cluster` path-dep): structurally closed via `HCMConfig.cluster_mgr: Arc<envoy_cluster::ClusterManager>` at Task 9 commit `3fdf960`. (Note: PROGRESS Task 16 attribution to Task 5 is imprecise — see M3-correction below; closure verdict is accurate.)
- **04.1 M6** (`drive_http1` per-function unit test): "practically closed" — Task 6/9/13 added end-to-end exercise; the strict per-function in-isolation unit test was never added. Hedged closure documented.
- **04.2 M10** (PLAN.md late-landing cadence): closed cleanly. The 04.3 planner committed PLAN.md as standalone pre-Task-1 commit `c02eea7` (2026-04-27 16:43) before any task commits, breaking the 04.1 → 04.2 inline-PLAN precedent.
- **#12 / phase-02.1 M1 (`Cluster::name()` accessor, multi-phase carryforward)**: closed at Task 9 commit `3fdf960`. `pub fn Cluster::name(&self) -> &str` lands at `crates/envoy-cluster/src/cluster.rs:24-26`; `pub fn ClusterHandle::name(&self) -> &str` at `:60-62`; field-level `#[allow(dead_code)]` removed; consumed by router-arm `tracing::warn!` log lines at `crates/envoy-http1/src/hcm.rs:208`/`:248`/`:265`. The carryforward chain phase-02.1 → 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 ends here.

Items carrying forward to phase 05+ / hardening / fixture-hardening sub-phase:

**C-1 (Important, cross-phase systemic regression).** Docker-gated `host.docker.internal`/`STATIC` regression on fixtures 0003/0004/0005/0006/0008. Originates at phase-02.2's ADR-0015 landing (where `host.docker.internal` was introduced as the BACKEND_HOST substitution for cross-container reachability via `host-gateway`); subsequent phases 02.2 / 03.1 / 03.2 / 04.1 / 04.2 did not push to CI between phase-02.1 close (run `24913934580`) and 04.3 task 14 (run `25106213773`), so the regression has been latent across **five phases**. Envoy v1.33's tightened `socket_address.address` parse semantics expect either a literal IP (under `STATIC`) or DNS resolution opt-in (under `STRICT_DNS` / `LOGICAL_DNS`):

```
[critical][main] [source/server/server.cc:416] error initializing config '/etc/envoy/envoy.yaml':
malformed IP address: host.docker.internal. Consider setting resolver_name or setting cluster type
to 'STRICT_DNS' or 'LOGICAL_DNS'
```

The 04.3 REVIEW reviewer's verdict choice was **(b) "Approved with M-track follow-ups" — defer to a dedicated post-04.3 fixture-hardening sub-phase** (or roll into phase 05 brainstorm scope), on the rationale of cross-phase scope spanning 5 fixtures and 4 phases, schema growth not in 04.3's planned deliverable set, original-budget-fit concerns, and the fact that fixture 0008 inherits the broken pattern uniformly with the four pre-04.3 fixtures. The recommended forward work:

1. Add `ClusterType::StrictDns` to the envoy-config schema at `crates/envoy-config/src/bootstrap.rs::ClusterType` (currently single-variant `Static` enum at lines 60-62); this also closes the dormant phase-02.1 REVIEW I3 (positive `ClusterType::Static` variant-name regression guard) in the same scope by giving the second variant.
2. Validator accept path for `STRICT_DNS` cluster type.
3. Coordinated-edit of the 5 affected fixtures (`tests/fixtures/{0003,0004,0005,0006,0008}/envoy.yaml` + per-fixture `envoy-rust.yaml` mirror) to use `type: STRICT_DNS` where `host.docker.internal` is the backend host literal.
4. Re-push to CI to confirm green Docker-gated runs across all 5 fixtures including 0008.

The phase-05 brainstorm session is the natural place to choose between (a) folding C-1 into phase 05's scope as a Task-1 preamble, (b) splitting C-1 into a separate fixture-hardening sub-phase (e.g. `04.4-fixture-hardening` or `05.0-fixture-hardening`) that lands before phase-05 HTTP/2 work begins, or (c) ratifying the deferral and continuing against the unit-test gate. Doctrinally option (b) is the cleanest shape per the M-track follow-up posture.

**M3-correction** (awareness-only). PROGRESS Task 16 says M3 closed at Task 5; the actual structural consumption of `envoy-cluster` from `envoy-http1` lands at Task 9 (commit `3fdf960`) via `HCMConfig.cluster_mgr`, not Task 5 (which only confirmed the path-dep is active in the build graph). Verdict (M3 closed) is accurate; the proximate-cause attribution is imprecise. **Track forward only if a future audit cites M3's closure commit** — leave PROGRESS-on-disk as-is.

**M-claim** (awareness-only, 04.1 M6 carryforward). Strict per-function `drive_http1` unit test never landed; first end-to-end exercise via fixture 0008 is currently masked by C-1's Docker-gated regression. **Track forward to whichever phase first adds a third Driver::Http1 consumer OR the C-1 fixture-hardening sub-phase** (which would unblock the end-to-end exercise via fixture 0008 once the cluster-type fix lands).

**M-payload** (awareness-only). `payload.bin` for fixture 0008 is empty (0 bytes), not the literal request bytes the 04.3 SPEC §3 D4 worked example shows. `Driver::Http1` constructs the request from driver fields, not by reading `payload.bin`. **Track forward only if a future phase adds a `Driver::Http1Raw`** (or similar) that reads `payload.bin` directly; if so, fixture 0008 may need to be amended to populate the file.

**M-spec-equiv** (awareness-only). Fixture 0008's `expectations.yaml` carries the working shape (per-driver `expected_headers` + 2 `equivalence` keys) rather than the SPEC §3 D4 worked example's 3-key `equivalence` shape. SPEC drift was anticipated and well-disclosed in PROGRESS Task 15. SPEC §3 D4 is now slightly stale on this point but is closed at this commit; **no follow-up needed**.

**M-payload-divergence** (awareness-only). `request_headers_to_remove` + `generate_request_id: false` on Envoy side only is intentional and load-bearing — neutralizes Envoy v1.33's default 6-header injection (x-forwarded-for, x-forwarded-proto, x-request-id, x-envoy-expected-rq-timeout-ms, x-envoy-internal, x-envoy-external-address) that would otherwise land in the deterministic-echo body and break byte-equivalence. The right long-term resolution is to extend envoy-rust to emit the same headers per parent SPEC §4's "default plan (b)" / "follow-on (a)" decision tree. **Track forward to a future phase that adds these headers to envoy-rust's HCM emission set** — natural fit at the access-log family (phase 06+) or whichever phase first needs request-side header injection for production realism.

**M-architectural-claim** (awareness-only, pre-existing carve-out). `httparse` lives at three Cargo.toml entries (`envoy-http1`, `envoy-bin`'s admin endpoint, differential harness). 04.3's new `httparse::Response::parse` use site is correctly inside `envoy-http1`. The pre-existing carve-outs at envoy-bin's admin endpoint and differential harness are tracked from 04.1 forward; eventual consolidation is well outside 04.3's scope. **Track forward to whichever phase next touches admin or routes the differential harness's response parser through `envoy-http1::Client::send_request`.**

### Phase-05.1 rollovers (from REVIEW.md §3–§4)

The 05.1 REVIEW landed with zero Critical items, **one Important item I1 (procedural / well-disclosed; D4 + D5's "Closes phase-04.3 REVIEW C-1" unmet at this commit; closure deferred to a follow-up sub-phase)**, and 6 awareness-only Minor findings (A1 forced cross-crate test-helper churn in envoy-tcp + envoy-http1 not enumerated in PLAN.md; A2 SPEC §3 D2 "tokio already pulled" claim was incorrect at HEAD; A3 PROGRESS Task 4 phase-04.1 REVIEW M-claim line is a continued deferral; A4 the in-tree `static_cluster_constructs_with_literal_ip` test is structurally meaningful not vacuous — confirms I3 close-out validity; A5 `crates/envoy-config/fuzz/Cargo.lock` is generated and untracked-not-ignored — pre-existing concern; A6 PROGRESS Task 1 LoC count double-counts the ADR-0023 block — minor accuracy nit). 3 Recommendations R1-R3 — R1 (brainstorm the C-1 follow-up against captured CI artifacts + backup-branch patches) drives this commit's "Next expected skill" Standing-context bullets; R2 (3-carryforwards bookkeeping at state-6) drives this section; R3 (PLAN's "If a fixture remains red → re-enter state 3" guidance worked correctly — doctrine validation, no fix needed) is observed. No in-phase review-fix commits at state 5 — substantive review findings were already closed in-phase by Task 1 review-fix `7391a4e` (extending `fuzz_corpus_seeds_parse_or_reject_cleanly` walk-list with the new strict_dns_cluster.yaml seed).

Items closed in 05.1:

- **02.1 REVIEW I3** (positive `ClusterType::Static` variant-name regression guard): **closed cleanly at Task 2 commit `f7a555d`** via `static_cluster_constructs_with_literal_ip` at `crates/envoy-cluster/src/cluster.rs:569`. The test parses a STATIC-typed cluster YAML through `parse_bootstrap`, calls `from_bootstrap(&bootstrap).await` (which dispatches into the new two-arm `match cfg.cluster_type` and exercises the literal-IP path), and asserts `mgr.get("backend").unwrap().pick_endpoint().unwrap() == "127.0.0.1:7000".parse().unwrap()`. Structurally meaningful only because Task 1 added the second `ClusterType` variant — was un-writable in 02.1 / 02.2 / 03.1 / 03.2 / 04.1 / 04.2 / 04.3 because the single-variant enum had no other arm against which to discriminate `Static`. The carryforward chain phase-02.1 → 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 → 05.1 ends at this commit.

Items partially closed in 05.1 (deferred to the C-1 follow-up sub-phase):

- **04.3 REVIEW C-1** (Important, cross-phase systemic regression): **partially closed**. The schema (D1 `ClusterType::StrictDns`) + runtime (D2 `tokio::net::lookup_host` STRICT_DNS branch + `ClusterError::DnsResolutionFailed` variant) + YAML edit (D3 5-fixture `STATIC` → `STRICT_DNS` flip) preamble landed cleanly (Tasks 1-3, commits `bfabcb6` / `7391a4e` / `f7a555d` / `0ce0aa2`) and the original C-1 trace (Envoy v1.33's `malformed IP address: host.docker.internal` startup error under `type: STATIC`) is no longer triggered — both proxies now start cleanly with the STRICT_DNS-flipped fixtures. **However, fixture 0008 surfaces a different defect at the upstream-routing path** (CI run `25258722850`: `response_status: exact` mismatch with upstream Envoy returning 503 vs envoy-rust subject returning 200), suggesting an upstream-cluster-construction or http1-echo-server-reachability issue at the `host.docker.internal` host-gateway boundary. Fixtures 0003/0004/0005/0006 were NOT RUN because `cargo test` exits at the first failing test binary. **Track forward to the C-1 follow-up sub-phase** (per the disposition decision in the "Next expected skill" section above; per REVIEW.md §5 R1's brainstorm scope guidance). The Docker-gated 5-fixture green re-baseline that closes C-1 substantively is a deliverable of THAT sub-phase's state-4 gate, not 05.1's.

Items continuing forward unchanged from earlier-phase carryforwards:

- **04.1 REVIEW M-claim** (`drive_http1` per-function unit test): **stays deferred per the 04.3 disposition.** 05.1 was projected (per its SPEC §1 "Cross-phase items unblocked but not closed at 05.1") to UNBLOCK the M-claim's masking by removing the Docker-gated regression — but the unblocking is partial since the C-1 fix is itself partial (the original-trace masking is removed; a different 0008-specific masking remains). The strict per-function `drive_http1` unit test in `tests/differential/src/lib.rs::tests` was never landed and 05.1 explicitly does not close it. **Track forward to the C-1 follow-up sub-phase** (which would unblock the end-to-end exercise via fixture 0008 once 0008's residual defect is fixed) **OR** whichever later phase first adds a third `Driver::Http1` consumer.

Awareness-only minor findings (per REVIEW §4 A1-A6):

- **A1** — Forced cross-crate test-helper churn in envoy-tcp + envoy-http1 was not enumerated in PLAN.md but is mechanically minimal and behaviour-neutral (`mk_handle` in envoy-tcp + `cluster_mgr_with_endpoint` / `cluster_mgr_empty` / `hcm_config_single_route` / `build_test_config` in envoy-http1 all promoted from `fn` to `async fn` with cascading `.await` updates; ~24 call-site updates total). **Process improvement only**: planner-time `grep -rn "from_bootstrap(" crates/` discipline before declaring call-site enumeration would catch this class of cross-crate sync→async dependency. Not a forward carryforward.
- **A2** — SPEC §3 D2's "Cross-crate dependency note" claimed envoy-cluster's Cargo.toml already pulled `tokio`; this was incorrect at HEAD `e626862`. The implementation correctly added `tokio` as a NEW direct dep on envoy-cluster (per D-3.2 — `tokio` was already pulled by envoy-bin / envoy-listener / envoy-tcp / envoy-http1 / envoy-tls per the existing 02.1+ shape, so no new top-level workspace dep). **Process improvement only**: SPEC writeup-time `grep "tokio" crates/<target>/Cargo.toml` discipline before declaring "already pulled" would catch this. Not a forward carryforward.
- **A3** — PROGRESS.md Task 4 phase-04.1 REVIEW M-claim line is a continued deferral, not a closure. Correctly handled. Carryforward narrative continues at the C-1 follow-up sub-phase's state-6 STATE.md edit.
- **A4** — The in-tree `static_cluster_constructs_with_literal_ip` test (D5's I3-closing test) is structurally meaningful, not vacuous. Confirms the I3 close-out is valid. No fix needed.
- **A5** — `crates/envoy-config/fuzz/Cargo.lock` is generated by `cargo fuzz` and is untracked-not-ignored. Pre-existing concern (not a 05.1 regression; the file has never been committed). The fuzz directory's `.gitignore` lists `corpus/parse_bootstrap/*` + `artifacts/` + `target/` but not `Cargo.lock`. **Track forward to a future tidy-up phase or a one-line root `.gitignore` extension for `crates/*/fuzz/Cargo.lock`** — not blocking.
- **A6** — PROGRESS.md Task 1 line 7's LoC count "Total: ~145 LoC" double-counts the ADR-0023 block (lives in DECISIONS.md, not in any source file). Self-cancelling minor accuracy nit. No follow-up needed.

Backup-branch artifact (preserved for the C-1 follow-up sub-phase):

- **`backup/task4-scope-creep-2026-05-02`** (local-only branch; not pushed to remote): preserves the 6 root-cause patches from the in-session aborted Task 4 expansion (commits `9279895` / `2d3d679` / `339b3c7`, since reset and force-pushed away from `main`). Verified existence via `git branch --list 'backup/*'` per REVIEW §6. The C-1 follow-up state-1 brainstorm session should review these patches for re-adoption — some may be re-usable as-is; some may need re-architecture under the new SPEC's discipline. The patches were discarded from `main` because they introduced inline root-cause changes without a SPEC anchor or an ADR landing — the doctrinally correct discipline preserves SPEC §7's "no new ADRs at this task" invariant + PLAN's "0 LoC of code changes" Task 4 contract.

### Earlier-phase carryforwards still open at 05.1 close

- **#13 — `x509-parser` deferred ADR.** Still deferred — 05.1 introduces no mTLS or peer-cert-attribution headers (no TLS work in 05.1 per its SPEC §2). Track forward to phase 05+ (post-C-1-followup) or whichever phase first needs structured cert introspection.
- **#14 — `enable_half_close: true` flip-fixture.** Still deferred — 05.1 introduces no asymmetric-close semantics (no transport-level changes in 05.1 per its SPEC §2; ADR-0016 posture unchanged). Track forward to phase 05+ (post-C-1-followup) or whichever phase first needs asymmetric-close semantics.
- **02.1 REVIEW I3 (positive `Static` regression guard).** **CLOSED at this commit** via 05.1 Task 2 commit `f7a555d` — see "Phase-05.1 rollovers" Items closed section above.

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None block phase 05.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject subprocess): still deferred. The `nix` crate remains the stated blocker (not on D-3.2 permitted-foundations list). Phases 01 / 02.1 / 02.2 / 03.1 / 03.2 / 04.1 / 04.2 / 04.3 all chose not to take it. Phase 05 may not need `nix` (depends on whether HTTP/2 fixtures benefit from graceful termination of long-lived streams).
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4 via five new regression tests; phases 03.1 / 04.1 / 04.2 / 04.3 continue the discipline on the new struct levels (HCM, RouteConfiguration, DirectResponse, HeaderMatcher, StringMatcher, SafeRegex, RouteAction validators).

### Phase-01 ADR ledger (for reference)

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as fuzz-only dev deps), ADR-0010 (nightly toolchain), ADR-0011 (phase-01 defers response-header equivalence to phase 04 — closed by 04.1's BEHAVIOR_CONTRACT.md `server` allow-list row, extended in 04.3 with `x-envoy-upstream-service-time`), ADR-0012 (nested nightly pin in fuzz subcrate).

### Phase-02 ADR ledger (for reference)

ADR-0013 (split phase 02; landed at `1c38ca9`), ADR-0014 (YAML-native `typed_config`; landed at `6d1f8d6`), ADR-0015 (cross-container host reachability; landed at `435c6fa` — see C-1 carryforward above; the `STATIC`/`host.docker.internal` interaction is the regression source), ADR-0016 (TCP proxy `enable_half_close: false` default; landed at `435c6fa`).

### Phase-03 ADR ledger (for reference)

ADR-0017 (split phase 03; landed at `f256d2c`), ADR-0018 (`rcgen` + `tempfile`; landed at `f93a062`), ADR-0019 (`tokio-rustls` + `rustls-pemfile`; landed at `f93a062`).

### Phase-04 ADR ledger (for reference)

ADR-0020 (split phase 04 into 04.1 + 04.2 + 04.3; landed at parent-04 state-2 commit `1d9740d`). ADR-0021 (`regex` permitted as a foundation for header / route matching; landed at 04.2 Task 1 commit `984aedd`). No ADRs landed in phase 04.3 (per SPEC §7).

Unlike phase-03's split (ADR-0017) which renumbered three projected ADRs, phase-04's split landed cleanly at ADR-0020 with no renumbering needed (parent-04 SPEC's projected ADR-0020 + ADR-0021 numbers match the actual landed numbers).

### Phase-05 ADR ledger (current)

Landed ADRs:

- **ADR-0022** (split phase 05 into sub-phases 05.1, 05.2, 05.3 by surface boundary) — landed at parent-05 state-2 commit `f1804a7`.
- **ADR-0023** (`ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred) — landed at 05.1 Task 1 commit `bfabcb6` (inline at Task 1 per the ADR-0021 inline-at-Task-1 precedent). Closes phase-02.1 REVIEW I3 (positive `Static` regression guard, enabled by introducing the second `ClusterType` variant). **Partially closes phase-04.3 REVIEW C-1** (cross-phase Docker-gated regression — the original Envoy v1.33 `malformed IP address: host.docker.internal` startup error is no longer triggered; fixture 0008's residual upstream-routing defect requires the C-1 follow-up sub-phase for substantive close).

The DECISIONS.md ledger head is now **ADR-0023**.

Phase 05's projected ADRs after this commit (open for landing):

- **ADR-0024** (CONDITIONAL — may land at the C-1 follow-up sub-phase if its diagnosis surface includes a permitted-foundations question, OR may land at 05.2 Task 1 as `http` crate typed-surface scoping per parent-05 SPEC §7). The `h2` crate exposes `http::*` types as part of its API; the question is whether `http` goes on the foundations list directly or stays transitive-only. Recommendation per parent SPEC §7: lands as a brief permitted-foundations grant. The follow-up sub-phase brainstorm has first priority on this number; if the follow-up does not land an ADR, ADR-0024 stays available for 05.2 Task 1.
- **ADR-0025** (CONDITIONAL — `h2spec` integration posture) — lands at 05.2 Task 1 IF the gate-mechanics surface a non-trivial doctrine choice (binary provisioning, known-failures format, partial-pass gate semantics). Brainstorm anticipates this is mostly mechanical and may not warrant an ADR. The follow-up sub-phase may also land its own diagnosis-surface ADRs (e.g. if the fix touches the upstream-routing path or the http1-echo-server contract); those would take the next-sequential numbers from ADR-0024 onward.

If ADR-0024 / ADR-0025 do not land at the C-1 follow-up OR 05.2 Task 1, those numbers stay available for phase-06+ ADRs. Like phase-04's split (ADR-0020), phase-05's split landed cleanly at ADR-0022 with no renumbering needed (parent-05 SPEC's projected ADR-0022 / ADR-0023 numbers matched the actual landed numbers).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The phase-05.1 state-6 close-out (this commit) advances STATE.md to a free-standing post-05.1 follow-up sub-phase under parent-05 at lifecycle state 0 (phase not yet in ROADMAP). The next session enters that sub-phase's state-1 brainstorm via `superpowers:brainstorming` scoped to the residual fixture-0008 defect, producing a new `SPEC.md` + adding the new sub-phase row to `docs/envoy-rust/ROADMAP.md` + extending parent ROADMAP row `05`'s `sub-phases` column to include the new id.
- The reviewer's R2 disposition decision (option (a) retroactive split of 05.1 vs option (b) free-standing post-05.1 sub-phase) was settled at this commit in favor of option (b) — see "Next expected skill" Disposition-decision subsection above for the rationale. Future-reviewers reading this commit's STATE.md should understand that 05.1 is structurally closed at the preamble landing; the follow-up sub-phase is a SIBLING under parent-05, not a child of 05.1.
