# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `05`
**slug:** `05-http2`
**directory:** `docs/envoy-rust/phases/05-http2/` exists; contains `SPEC.md` (committed at this state-1 commit; 490 lines).
**status:** phase 05 lifecycle **state 2 (parent SPEC.md exists; sub-phase SPECs do not)** — ROADMAP row `05` is `status: in-progress` as of this state-1 commit (flipped from `planned`); `sub-phases: 05.1, 05.2, 05.3` projected (codified at parent-05 state-2 via ADR-0022). The parent-05 state-2 session lands ADR-0022, the three sub-phase SPECs (`docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md`, `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md`, `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md`), and three new ROADMAP rows for the sub-phases. Mirrors the phase-04 state-2 commit `1d9740d` shape (ADR-0020 + three sub-phase SPECs + ROADMAP rows in a single commit).

Phase 04 (`04-http1`) is **done** as of commit `e626862` (the phase-04.3 state-6 close-out, predecessor of this state-1 commit). All three sub-phases are done: `04.1-hcm-direct-response` (commit `c5c40ec`), `04.2-route-matchers` (commit `04163c5`), and `04.3-router-upstream` (commit `e626862`). ROADMAP rows `04`, `04.1`, `04.2`, and `04.3` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `805433e`); for execution purposes it was superseded by the three sub-phase SPECs.

Phase 04.3 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `eb030d1`; no Critical or Important findings in the 04.3 surface itself; one cross-phase Important carryforward (C-1 Docker-gated `host.docker.internal`/`STATIC` regression originating at phase-02.2 ADR-0015, latent across five phases, surfaced by 04.3's CI push cadence and fixture 0008 inheritance) + 4 awareness-only Minor findings (M3-correction PROGRESS Task 16 imprecise on M3 closure attribution, M-claim drive_http1 per-function unit test never landed, M-payload payload.bin empty by design, M-spec-equiv expectations.yaml SPEC drift), all explicitly named in REVIEW §3 + §4 — see "Phase-04.3 rollovers" below). 4 in-phase items closed (M3 / M6 / M10 / #12 Cluster::name() carryforward); 11 forward-track items propagated. Eight in-phase review-fix commits closed substantive findings before propagation (Tasks 2/8/9/11/12/13/14/16).

Phase 04.2 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `c1ff7b6`; closed in 04.3 are M3 / M6 / M10 plus the multi-phase #12 `Cluster::name()` carryforward — see "Phase-04.3 rollovers" below).

Phase 04.1 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `b6e305d`; M3 closed in 04.3; M1 / M2 / M4 / M5 / M7 carry forward to phase 05+ / hardening — see "Phase-04.3 rollovers" below).

Phase 03 (`03-tls-tcp`) is **done** as of commit `ca81226`. Both sub-phases are done: `03.1-tls-foundation-downstream` (commit `64ea760`) and `03.2-tls-upstream-sni` (commit `ca81226`). ROADMAP rows `03`, `03.1`, and `03.2` are all `status: done`.

Phase 03.2 `REVIEW.md` verdict is **Approved with fixes** (state 5 complete; I1 closed in-phase; M1–M5 tracked forward — see "Phase-03.2 rollovers" below). Phase 03.1 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase; M1–M5 tracked forward — see "Phase-03.1 rollovers" below).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`.

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase; M1–M4 tracked forward — see "Phase-02.2 rollovers" below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see "Phase-02.1 rollovers" below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 17–22, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 2): the next session — operating as the parent-05 state-2 session — invokes **`superpowers:writing-plans`**. State-2 output for the parent-05 split: **ADR-0022** (split decision) lands in `docs/envoy-rust/DECISIONS.md`; three sub-phase SPECs land at `docs/envoy-rust/phases/{05.1-fixture-hardening,05.2-http2-downstream,05.3-http2-upstream}/SPEC.md`; three new ROADMAP rows for the sub-phases. The state-2 commit also advances STATE.md to point at sub-phase 05.1 as the next active phase (lifecycle state 2; sub-phase SPEC exists, sub-phase PLAN.md does not), with next-skill `superpowers:writing-plans` for sub-phase 05.1.

The split decision was projected at parent-05 state-1 (this commit) per parent SPEC §5 / §7. Three sub-phases by surface boundary:

- **05.1 `fixture-hardening`** — `ClusterType::StrictDns` + validator + 5-fixture coordinated edit (`tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}`) + closes phase-02.1 REVIEW I3 (positive `Static` regression guard) + closes phase-04.3 REVIEW C-1 (Docker-gated regression). NO H2 work. ~400 LoC, ~5 tasks. Lands **ADR-0023** at 05.1 Task 1.
- **05.2 `http2-downstream`** — `envoy-http2` foundation (sole workspace dep on `h2 = "0.4"`; mirrors `envoy-http1`'s sole-owner-of-`httparse` posture) + `CodecType::HTTP2` accept path + listener-side `Http2ProtocolOptions` + HCM-on-H2 dispatch (reuses 04.x `HCMConfig` end-to-end; only the codec layer changes) + fixture `0009-http2-direct-response` + `tests/conformance/h2spec/` runner crate at the **≥95% pass** gate with catalogued failures + `Driver::Http2` harness extensions. ~1300 LoC, ~14 tasks. May land **ADR-0024** (`http` crate scoping) and/or **ADR-0025** (`h2spec` integration posture) conditionally per parent SPEC §7.
- **05.3 `http2-upstream`** — `envoy-http2::Client` (per-connection plaintext H2 client; no pooling) + cluster-side `Http2ProtocolOptions` via `typed_extension_protocol_options` + router H2-arm (extends 04.3's `RouteAction::Route` arm to dispatch H1-or-H2 based on cluster's protocol options) + new helper crate `tests/helpers/http2-echo-server` + fixture `0010-http2-router-upstream` + parent-05 close-out. ~1300 LoC, ~14 tasks.

Strict ordering 05.1 → 05.2 → 05.3 (cannot be parallelized; 05.2 depends on 05.1's restored Docker-gated baseline; 05.3 extends 05.2's schema and runtime).

The DECISIONS.md ledger head is **ADR-0021** (last landed in 04.2 Task 1 commit `984aedd`); phase 05's projected ADRs land at **ADR-0022, ADR-0023**, with **ADR-0024 / ADR-0025 conditional**.

**Standing context for the parent-05 state-2 session:**

- **The brainstorm scope decisions are codified in the parent-05 SPEC** at `docs/envoy-rust/phases/05-http2/SPEC.md`. The state-2 planner does NOT re-litigate these. Specifically: (i) C-1 fix is in-scope as 05.1; (ii) plaintext H2C prior-knowledge for both downstream and upstream (TLS+ALPN deferred per parent SPEC §4 — M7 carryforward continues); (iii) two H2 fixtures (0009 + 0010); (iv) `h2spec` ≥95% pass with catalogued failures targeting 100%; (v) `server_name` HCM field re-deferred; (vi) `codec_type: AUTO` stays HTTP/1.1-only; (vii) cross-protocol H2↔H1 translation deferred.
- **Per the user's standing preference** (auto-memory `feedback_execution_style`), state-3 execution will use `superpowers:subagent-driven-development` over inline `executing-plans` — do not present the two-option fork at state-3 entry.
- **PLAN.md cadence** continues the 04.3 standardized posture: each sub-phase's planner commits PLAN.md cleanly at state-2 close-out, before any Task 1 commit (the 04.3 standalone pre-Task-1 PLAN.md commit `c02eea7` is the precedent). The 04.1/04.2 inline-PLAN deviation is no longer the model.
- **`http` crate ADR (ADR-0024) and `h2spec` integration ADR (ADR-0025) are CONDITIONAL** per parent SPEC §7. The 05.2 planner decides at sub-phase Task 1 time whether either warrants landing. If neither lands, ADR-0024 / ADR-0025 numbers stay available for phase-06 ADRs.

Inputs the parent-05 state-2 session should read, in order:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (row 05 `in-progress`; sub-phases column `05.1, 05.2, 05.3`; rows 04 + 04.x and earlier all `done`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0021`; phase-05's projected ADRs land at `ADR-0022+`).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (Header allow-list section has 3 phase-04 rows; phase 05 anticipates no edits per parent SPEC §2).
6. `docs/envoy-rust/SKILL_ROUTING.md` (state machine).
7. **`docs/envoy-rust/phases/05-http2/SPEC.md` (parent-05 SPEC; 490 lines)** — the design contract for the split. §3 enumerates ~13 deliverables organized by sub-phase that the sub-phase SPECs decompose into per-task plans; §5 projects the 3-way split with LoC/tasks estimates; §6 lists 23 implementation signposts; §7 names ADR-0022, ADR-0023, and the conditional ADR-0024/0025; §8 lists artifacts created/amended; §9 names the parent-05 close-out commit format.
8. `docs/envoy-rust/phases/04.3-router-upstream/{SPEC,PLAN,PROGRESS,REVIEW}.md` (most recent phase precedent — task cadence, TDD framing, in-phase review-fix discipline, REVIEW.md verdict shape).
9. `docs/envoy-rust/phases/04.1-hcm-direct-response/SPEC.md` + `04.2-route-matchers/SPEC.md` (HCM scaffold + matcher schema phase 05 inherits unchanged; HCM-on-H2 reuses `HCMConfig` + route-walk + router invocation site wholesale).
10. `BOOTSTRAP_PROMPT.md` §5 state 2 (writing-plans skill routing) + §6.1 / §6.2 (split gate + how-to-split — the parent-05 state-2 session lands the projected split decision following §6.2 steps).

## Last commit

Phase 05 state-1 close-out commit (this commit): touches `docs/envoy-rust/phases/05-http2/SPEC.md` (NEW; 490 lines), `docs/envoy-rust/ROADMAP.md` (row 05 `status` `planned` → `in-progress`; `sub-phases` column `—` → `05.1, 05.2, 05.3`), and `docs/envoy-rust/STATE.md` (this file; advance from phase 05 lifecycle state 1 → state 2; rewrite of active sections). No code changes. Mirrors the phase-04 state-1 commit `805433e`-shape (parent SPEC + ROADMAP flip + STATE advance, all atomic).

Predecessor commits:

- `e626862` — `phase 04.3: HTTP/1.1 upstream origination + router proxy arm [parent 04 done]` (state-6 phase-done close-out for phase 04.3 AND parent phase 04; advanced STATE.md to phase 05 lifecycle state 1 with next-skill `superpowers:brainstorming`).
- `eb030d1` — `phase 04.3: state 5 REVIEW.md Approved with M-track follow-ups` (the REVIEW.md that surfaced C-1 as the cross-phase carryforward 05.1 now resolves).
- `c02eea7` — `phase 04.3: state-2 PLAN.md (inline-at-Task-1 precedent: pre-Task-1 standalone)` — the standalone-pre-Task-1 PLAN.md commit that established the standardized cadence inherited by phase 05's sub-phase planners.

## Last updated

2026-05-01 (phase 05 state-1 close-out; parent SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` projects 3-way split into 05.1/05.2/05.3 with C-1 fix folded into 05.1; ROADMAP row 05 flipped to `in-progress`; STATE advances to phase 05 lifecycle state 2; next-skill `superpowers:writing-plans` for parent-05 state-2 — lands ADR-0022 + 3 sub-phase SPECs + 3 new ROADMAP rows).

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

### Earlier-phase carryforwards still open at 04.3 close

- **#13 — `x509-parser` deferred ADR.** Still deferred — 04.3 introduces no mTLS or peer-cert-attribution headers (parent SPEC §4 line 568 deferral). Track forward to phase 05+ or whichever phase first needs structured cert introspection.
- **#14 — `enable_half_close: true` flip-fixture.** Still deferred — 04.3 introduces no asymmetric-close semantics (SPEC §4 deferral; ADR-0016 posture unchanged). Track forward to phase 05+ or whichever phase first needs asymmetric-close semantics.

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

ADR-0020 (split phase 04 into 04.1 + 04.2 + 04.3; landed at parent-04 state-2 commit `1d9740d`). ADR-0021 (`regex` permitted as a foundation for header / route matching; landed at 04.2 Task 1 commit `984aedd`). No ADRs landed in phase 04.3 (per SPEC §7). The DECISIONS.md ledger head remains at **ADR-0021**; phase 05's projected ADRs land at **ADR-0022+**.

Unlike phase-03's split (ADR-0017) which renumbered three projected ADRs, phase-04's split landed cleanly at ADR-0020 with no renumbering needed (parent-04 SPEC's projected ADR-0020 + ADR-0021 numbers match the actual landed numbers).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The phase 04.3 state-6 close-out (this commit) advances STATE.md to phase 05 lifecycle state 1. The next session enters phase 05 state 1 via `superpowers:brainstorming` scoped to phase 05, producing `docs/envoy-rust/phases/05-<slug>/SPEC.md`.
