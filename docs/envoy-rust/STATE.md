# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `04.3`
**slug:** `04.3-router-upstream` (created during the parent-04 state-2 commit `1d9740d`; SPEC.md committed alongside ADR-0020 + sibling sub-phase SPECs).
**directory:** `docs/envoy-rust/phases/04.3-router-upstream/` exists; contains `SPEC.md` (769 lines).
**status:** phase 04.3 lifecycle **state 2 (SPEC.md exists, PLAN.md does not)** — SPEC.md landed at parent-04 state-2 commit `1d9740d` alongside ADR-0020 (split decision), the sibling 04.1 + 04.2 SPECs, and ROADMAP row appends. ROADMAP row `04.3` flips from `planned` to `in-progress` at this commit (the phase-04.2 state-6 close-out) per ROADMAP-schema invariant ("a phase enters `in-progress` when STATE.md points at it as the active phase with the directory created"). Both conditions are satisfied here: STATE.md now points at 04.3 as active, and the 04.3 directory has existed since `1d9740d`. Mirrors the phase-04.1-state-6 precedent (`c5c40ec`) which atomically flipped 04.1 to `done` AND 04.2 to `in-progress` in the same commit. Parent row `04` stays `in-progress` per the ROADMAP-schema invariant ("parent flips to `done` only after all sub-phases are `done`"); it will flip to `done` in 04.3's final state-6 commit (mirrors phase 03's `ca81226`-shape close-out where 03.2's phase-done commit also closed parent 03).

Phase 04 (`04-http1`) parent is **in-progress** with `sub-phases: 04.1, 04.2, 04.3` per **ADR-0020**'s split (landed at parent-04 state-2 commit `1d9740d`). Parent SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `805433e`, the parent-04 state-1 brainstorm output); for execution purposes it is superseded by the three sub-phase SPECs (`phases/04.1-hcm-direct-response/SPEC.md`, `phases/04.2-route-matchers/SPEC.md`, `phases/04.3-router-upstream/SPEC.md`).

Sibling sub-phases:

- **04.1 (`04.1-hcm-direct-response`)** — `status: done` as of the phase-04.1 state-6 close-out commit `c5c40ec`. SPEC.md (1074 lines) + PLAN.md (3956 lines) + PROGRESS.md (296 lines) + REVIEW.md (242 lines) all in tree. Phase delivered: the new `envoy-http1` library crate (codec/headers/date/response/hcm modules; 19 unit tests; `#![forbid(unsafe_code)]`); HCM as a `ConnectionHandler` impl walking inline `RouteConfiguration` first-match-wins on Host then path; hardcoded router-filter call site emitting `direct_response`; envoy-config schema growth (`HttpConnectionManagerConfig` + `RouteConfiguration` + `DirectResponse` + `DataSource.inline_string` extension; 10 new `ConfigError` variants; `validate_hcm` + `validate_data_source` + `is_valid_dns_name` + private `Required` enum; 14 + 1 review-fix new tests; 2 fuzz-corpus seeds); envoy-bin HCM dispatch arm + factored `build_downstream_tls_for_listener` helper + HCM+TLS detect-and-bail; in-process `crates/envoy-bin/tests/http1_direct_response.rs` integration backstop (209 LoC); differential harness extensions (`Driver::Http1` + `drive_http1` + `HEADER_ALLOW_LIST` + `diff_headers` + 3 unit tests); fixture 0007-http1-direct-response (5 files) + Docker-gated acceptance test; `BEHAVIOR_CONTRACT.md`'s Header allow-list table populated with `server` + `date`. Two in-phase review-fix commits (`4e7c050` Task 2; `a6f7b5e` Task 10). REVIEW verdict **Approved with M-track follow-ups** at `b6e305d`; M1–M7 tracked forward — see "Phase-04.1 rollovers" below.
- **04.2 (`04.2-route-matchers`)** — `status: done` as of this commit (state-6 close-out). SPEC.md (698 lines) + PLAN.md (3562 lines) + PROGRESS.md (196 lines) + REVIEW.md (243 lines) all in tree. Phase delivered: all 7 of Envoy's `HeaderMatcher` modes (`exact_match`, `prefix_match`, `suffix_match`, `safe_regex_match`, `range_match`, `present_match`, `string_match`) plus the 5-variant `StringMatcher` tagged union plus `invert_match: bool` plus `RouteMatch.headers: Vec<HeaderMatcher>`, with three hand-rolled `Deserialize` visitors modeling Envoy's field-name oneof discrimination; `regex = "1"` runtime dep narrowly permitted under ADR-0021 at Task 1; `Arc<regex::Regex>` compiled at config-load time; new sibling module `crates/envoy-config/src/matcher.rs` (356 LoC) carrying `HeaderMatcher::matches` + `StringMatcher::matches` inherent methods + 28 unit tests; HCM route walker AND-combination (path-AND-headers short-circuit per Envoy default `headers_match_options: ALL`) + 5 HCM integration tests; differential harness `Driver::Http1ProbeList` + `Http1Probe` + `extra_headers` parameter + dispatch arm with deferred-teardown (mirrors 03.2's `Driver::TlsTcpProbeList` shape) + 2 harness unit tests; fixture 0007 amendment with two-probe shape (matcher route at head: `prefix: "/api/"` + `headers: [{ name: "x-foo", exact_match: "bar" }]` → 418 teapot; default catch-all 200 ok); 1 new fuzz seed `route_with_header_matchers.yaml` exercising 5-of-7 modes; 5 new `ConfigError` variants; ADR-0021 narrow scope (header / route matching at config-load only). +63 tests delta in-phase (envoy-config 75→131; envoy-http1 19→24; differential lib 47→49). Six in-phase review-fix commits (`5a6b950` Task 2 doc-comment; `3d9f985` Task 3 assertion-tightening; `17f991a` Task 4 MODE_KEYS const + multi-mode-key test; `48e615c` Task 5 TCP_PROXY as_ref intent comment; `81c6dde` Task 6 `str::get` panic-safety + expect-message disambiguation + 2 tests; `8330a86` Task 7 `Connection: close` on new HCM tests). REVIEW verdict **Approved with M-track follow-ups** at `c1ff7b6`; M1–M11 tracked forward into 04.3 / phase 05 / hardening pass — see "Phase-04.2 rollovers" below.

Phase 03 (`03-tls-tcp`) is **done** as of commit `ca81226`. Both sub-phases are done: `03.1-tls-foundation-downstream` (commit `64ea760`) and `03.2-tls-upstream-sni` (commit `ca81226`). ROADMAP rows `03`, `03.1`, and `03.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `a3f3474`); for execution purposes it was superseded by the two sub-phase SPECs (`phases/03.1-tls-foundation-downstream/SPEC.md` and `phases/03.2-tls-upstream-sni/SPEC.md`).

Phase 04.2 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `c1ff7b6`; no Critical or Important findings; 4 new Minor findings (M8 `safe_regex_partial_eq` opaque-equality, M9 ADR-0021 prose ↔ Cargo.lock cadence contradiction, M10 PLAN.md late-landing process consistency, M11 `Http1Probe.extra_headers` duplicate semantics coupled with M1) + 7 carryforwards from 04.1 (M1–M7) + 3 earlier-phase carryforwards (Cluster::name, x509-parser, enable_half_close); 11 M-track items tracked forward — see "Phase-04.2 rollovers" below). Six in-phase review-fix commits closed substantive findings before propagation (Tasks 2/3/4/5/6/7).

Phase 04.1 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `b6e305d`; no Critical or Important findings; 4 Minor findings (M1 `diff_headers` duplicate-header semantics, M2 body-drain idle timeout silent close, M4 `strip_port` IPv6 correctness, M5 Cargo.lock sync cadence) and 3 awareness-only Minor findings (M3 `envoy-cluster` pre-staged dep, M6 `drive_http1` per-function unit test, M7 `TlsAcceptingHandler` generalization for HCM+TLS); 7 M-track items tracked forward into 04.2 / 04.3 / phase 05 / hardening pass — see Notes below). Two in-phase review-fix commits closed substantive findings before propagation (Task 2 `4e7c050`; Task 10 `a6f7b5e`).

Phase 03.2 `REVIEW.md` verdict is **Approved with fixes** (state 5 complete; I1 closed in-phase via the §7 close-out commit `f0b4a48`; M1–M5 tracked forward — see Notes below). Phase 03.1 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `1748cd2`; M1–M5 tracked forward — see Notes below).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 16–22, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 2): the next session — operating as the state-2 session of phase 04.3 — invokes **`superpowers:writing-plans`** scoped to phase 04.3, producing `docs/envoy-rust/phases/04.3-router-upstream/PLAN.md`. The PLAN converts 04.3 SPEC §3's deliverables into a per-task checklist (estimated per 04.3 SPEC §5; no new ADRs anticipated per 04.3 SPEC §7 — ADR-0020 + ADR-0021 are both landed before 04.3 starts; if execution surfaces a need, the next-sequential ADR number is ADR-0022). Each task respects `superpowers:test-driven-development` per doctrine D-3.1.

**PLAN.md cadence (carryforward from 04.2 REVIEW M10):** the 04.3 planner should commit PLAN.md at clean state-2 close-out — i.e. as a dedicated single-file commit BEFORE any Task 1 commit — to break the 04.1 → 04.2 inline-PLAN precedent. Per `BOOTSTRAP_PROMPT.md` §5, the doctrine-prescribed flow is state 2 → write PLAN.md → commit → state 3 → execute tasks. 04.1 + 04.2 both deviated by inlining PLAN with the first task / late commit at state-4 (well-disclosed but not ideal); 04.3 is the natural place to restore the precedent.

Per the user's standing preference (auto-memory `feedback_execution_style`), state-3 execution will use `superpowers:subagent-driven-development` over inline `executing-plans` — do not present the two-option fork at state-3 entry.

**Plan splitting gate evaluation** (BOOTSTRAP_PROMPT.md §5 state 2 / §6.1; 04.3 SPEC §5; parent SPEC §5):

- Estimated 04.3 surface per 04.3 SPEC §5: **~17 tasks, ~1490 LoC.** Comfortably under both gates (~25 tasks / ~1500 LoC bounds; the LoC estimate sits right at the soft ceiling but does not breach it).
- **Decision: 04.3 stays unified.** No nested split. Per 04.3 SPEC §5 + parent SPEC §5 closing paragraph + the parent-04 brainstorm's express avoidance of nested splits, if either gate fires mid-PLAN-write invoke `superpowers:systematic-debugging` first — nested splits of an already-split sub-phase deserve a fresh root-cause analysis.

Inputs the 04.3 state-2 session should read, in order, before drafting `PLAN.md`:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (rows 04 `in-progress` with `sub-phases: 04.1, 04.2, 04.3`; row 04.1 `done`; row 04.2 `done`; row 04.3 `in-progress` (depends-on 04.2, satisfied)).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0021`; no new ADRs anticipated in 04.3 per 04.3 SPEC §7; if surfaced, ADR-0022+).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `Header allow-list` section was populated by 04.1 with `server` + `date`; 04.3 adds `x-envoy-upstream-service-time` per 04.3 SPEC §2).
6. `docs/envoy-rust/SKILL_ROUTING.md` (state-2 routing).
7. **`docs/envoy-rust/phases/04.3-router-upstream/SPEC.md`** (the authoritative sub-phase design contract — referenced at every task under "Source of truth: SPEC.md" at the top of the resulting PLAN.md).
8. `docs/envoy-rust/phases/04-http1/SPEC.md` (parent SPEC — context for the full phase-04 design + cross-sub-phase architectural rules at §3 closing; execution follows the 04.3 sub-phase SPEC, not the parent).
9. **`docs/envoy-rust/phases/04.2-route-matchers/PLAN.md` + `PROGRESS.md` + `REVIEW.md`** (immediate-prior sub-phase precedent — task cadence, TDD framing, PROGRESS-formatting conventions; 04.2 REVIEW §3 M1–M11 + §4 forward-work items inform 04.3's PLAN where applicable: M1 + M11 `diff_headers` / `Http1Probe.extra_headers` duplicate-header semantics surface in 04.3 if upstream emits `Set-Cookie`/`Vary`; M2 body-drain timeout surfaces with non-trivial bodies; M3 `envoy-cluster` dep is consumed (or replaced) at 04.3; M4 `strip_port` IPv6 defense-in-depth; M6 `drive_http1` per-function unit test natural at the second `Driver::Http1`-shape consumer).
10. `docs/envoy-rust/phases/04.1-hcm-direct-response/PLAN.md` + `PROGRESS.md` + `REVIEW.md` (sibling sub-phase precedent; 04.1 SPEC + PLAN landed the `RouteAction` enum 04.3 extends, the HCM router invocation site 04.3 generalizes from one-arm to two-arm, and the differential `Driver::Http1` shape 04.3 may extend or supplement).
11. `docs/envoy-rust/phases/04.1-hcm-direct-response/SPEC.md` + `docs/envoy-rust/phases/04.2-route-matchers/SPEC.md` (sibling sub-phase SPECs — 04.3 inherits the `RouteAction` enum from 04.1 and walks the matcher schema 04.2 added; the route schema 04.3 extends is the same schema 04.1 + 04.2 landed).
12. `docs/envoy-rust/phases/03.2-tls-upstream-sni/PLAN.md` + `PROGRESS.md` + `REVIEW.md` (cadence cross-check for the upstream-side helper-server precedent; `tls-echo-server` introduced in 03.2 is the closest sibling shape for `http1-echo-server` introduced in 04.3 — argv parsing + echo loop + integration into the differential harness via a backend type).
13. `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` (phase-02.1 introduced `tcp-echo-server` — the original helper-crate cadence + `ClusterManager` + `Cluster::name` deferral that 04.3 closes per SPEC §3 D5).

## Last commit

Phase 04.2 state-6 close-out commit (this commit): lands the ROADMAP flip + STATE advance per the phase-04.1 `c5c40ec` precedent (sub-phase done, sibling enters `in-progress`, parent stays `in-progress`). Touches:

- **`docs/envoy-rust/ROADMAP.md`** — flips row `04.2` status from `in-progress` to `done`. Also flips row `04.3` status from `planned` to `in-progress` per ROADMAP-schema invariant 3 (a phase enters `in-progress` when STATE.md points at it as the active phase with the directory created); both conditions are satisfied here since the 04.3 directory has existed since `1d9740d`. Parent row `04` stays `in-progress` per the schema invariant ("parent flips to `done` only after all sub-phases are `done`") — 04.3 is the closing sub-phase and parent row `04` flips to `done` in 04.3's state-6 commit. Mirrors the phase-04.1 state-6 commit `c5c40ec` shape (which atomically flipped 04.1 to `done` AND 04.2 to `in-progress` in one commit while parent 04 stayed `in-progress`).
- **`docs/envoy-rust/STATE.md`** (this file) — advances active phase from `04.2` lifecycle state 5 to `04.3` lifecycle state 2 (sub-phase SPEC.md exists from `1d9740d`, PLAN.md does not). Refreshes the Notes section to add "Phase-04.2 rollovers" with the 11 M-track items from 04.2 REVIEW §4 (4 new in 04.2 + 7 carryforwards from 04.1) + 3 earlier-phase carryforwards.

No code changes. Mirrors phase-04.1 state-6 commit `c5c40ec` shape (which touched ROADMAP.md + STATE.md only).

Predecessor commits in phase 04.2:

- `c1ff7b6` — `phase 04.2: state 5 REVIEW.md Approved with M-track follow-ups` (landed REVIEW.md only; verdict **Approved with M-track follow-ups**; no in-phase fix needed at state 5 since substantive review findings were already closed in-phase by Task 2/3/4/5/6/7 review-fix commits).
- `e00f638` — `phase 04.2: state-4 phase-done gate verification (task 12)` (state-4 gate green on first attempt: fmt/build/clippy/test/deny all clean; 275 passed + 1 ignored).
- `160caf0` — `phase 04.2: state-2 PLAN.md (late-landing per 04.1 inline-at-Task-1 precedent)` (PLAN.md landed late, immediately before the state-4 gate — well-disclosed deviation; carryforward to 04.3 per M10: 04.3's planner should commit PLAN.md cleanly at state 2 to break the precedent).
- `984aedd` — `phase 04.2: envoy-config — Task 1 ADR-0021 (regex permitted) + 4 ConfigError stubs + Cargo.lock` (state-3 entry; ADR-0021 + `regex = "1"` dep landed inline at Task 1 per phase-03.1 ADR-0018+ADR-0019 precedent; Cargo.lock landed inline at Task 1 contradicting the ADR-0021 prose's "dedicated state-4 commit" wording — well-disclosed in PROGRESS Task 1 deviation 1; tracked as M9).
- `c5c40ec` — `phase 04.1: HTTP/1.1 codec + HCM scaffold + direct_response + fixture 0007 [ADR-0020]` (phase-04.1 state-6 close-out; landed ROADMAP row 04.1 → done + row 04.2 → in-progress + STATE advance from 04.1 state 5 to 04.2 state 2 in one commit per the phase-03.1 `64ea760` precedent).

## Last updated

2026-04-27 (phase 04.2 state-6 close-out; ROADMAP row 04.2 flipped `in-progress` → `done`; ROADMAP row 04.3 flipped `planned` → `in-progress`; STATE.md advanced active phase from 04.2 lifecycle state 5 to 04.3 lifecycle state 2; next-skill `superpowers:writing-plans` scoped to phase 04.3).

## Notes

### ADR numbering after the phase-03 split

The parent-phase-03 SPEC (`03-tls-tcp/SPEC.md`, committed at SHA `a3f3474`) projected three phase-03 ADRs numbered 0017 (`rcgen` + `tempfile`), 0018 (`tokio-rustls` + `rustls-pemfile`), 0019 (split phase 03). The ADR-0017 split decision (landed at `f256d2c`) took the actual next-sequential number at split time, so each projected ADR shifted in-tree:

- **ADR-0017** — split phase 03 into 03.1 + 03.2 (landed at `f256d2c`; was parent-SPEC §7's projected ADR-0019).
- **ADR-0018** — `rcgen` + `tempfile` permitted as dev-test-harness-only foundations (landed at `f93a062` during 03.1 Task 1; was parent-SPEC §7's projected ADR-0017).
- **ADR-0019** — `tokio-rustls` + `rustls-pemfile` covered by the rustls foundations grant (landed at `f93a062` during 03.1 Task 1; was parent-SPEC §7's projected ADR-0018).

The sub-phase SPECs (03.1 + 03.2) cite ADR-0017 for the renumbering and rewrite each expected ADR with its actual landed number. The parent SPEC (`docs/envoy-rust/phases/03-tls-tcp/SPEC.md`) is preserved unedited per D-3.4 / D-3.5 (it's a committed historical artifact; the projected ADR numbers in its §7 remain literally as written — readers cross-reference this Notes section + ADR-0017 for the renumbered actuals). Phase 03.2's projected ADRs (currently slated for 0020+) follow the same provisional-numbering posture per phase-02.2 REVIEW recommendation #2 + phase-03.1 REVIEW §4 recommendation 1.

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
- **M1** — add `pub(crate) fn Cluster::name(&self) -> &str` accessor and remove the field-level `#[allow(dead_code)]` at `crates/envoy-cluster/src/cluster.rs`: **tracked forward to phase 04.3** per 04.3 SPEC §3 D5 ("opportunistic close-out of the multi-phase `Cluster::name()` carryforward"). Phase 04.3's router-proxy arm gives the natural use site (per-cluster proxy attribution in error variants and `tracing` log lines).
- **M2** — `echoes_round_trip` drop-before-send ordering in `tests/helpers/tcp-echo-server/src/main.rs`: awareness-only, no action required.
- **M3** — drop the dead `|| msg.contains("CRLF")` disjunct in `tests/differential/src/lib.rs`: **closed** opportunistically by 02.2 Task 11 at commit `aa4187f`.
- **M4** — style-only: `ClusterManager::get` does `Arc::clone` inside a `.map` closure: no action required.

### Phase-02.2 rollovers (from REVIEW.md §3–§4)

The 02.2 REVIEW landed with one Important item and four Minor items. I1 (STATE.md stale) closed in-phase by the §7 close-out commit `fc87505`. The remaining items:

- **M1** — `TcpProxyBackend::Drop` polling loop blocks on `std::thread::sleep` from a tokio-runtime thread (`tests/differential/src/backend.rs:73-83`): **tracked forward to whichever phase first parallelizes `run_fixture` across worker threads**. Phase 03.1 + 03.2 + 04.1 + 04.2 do not parallelize fixtures; the same is anticipated for 04.3 + 05. The `TlsEchoBackend` 03.2 ships and the `Http1EchoBackend` 04.3 will ship inherit the same posture — single-fixture-per-invocation usage avoids the worst-case 2s Drop stall.
- **M2** — `proxies_returns_err_on_upstream_connect_refused` asserts on the formatted error string rather than the typed variant (`crates/envoy-tcp/src/lib.rs:289-296`): awareness-only, no action required.
- **M3** — `proxies_closes_downstream_on_upstream_close` has implicit timing on the upstream's "tail" read (`crates/envoy-tcp/src/lib.rs:199-202`): awareness-only, no action required.
- **M4** — `Listener::serve`'s `JoinSet` type aliases a long generic (`crates/envoy-listener/src/lib.rs:113-115`): **tracked forward to phase 04.3 or phase 07** when a richer filter trait warrants a `pub type HandlerResult = ...` alias. 04.1's TlsAcceptingHandler in envoy-bin and 04.2's HCM dispatch arm did not reach for this.

Phase 02.2 REVIEW §4 recommendations forward to phase 03.1 / 03.2 / later:

1. Add `Cluster::name()` accessor when phase 03.2's TLS work or phase 06's stats first need it (phase-02.1 REVIEW M1 cross-reference). 04.3 SPEC §3 D5 explicitly closes this carryforward in 04.3 (the router-proxy arm provides the use site).
2. Phase 03 ADR projection numbering is provisional — heeded throughout 03.1 + 03.2 SPECs; ADR-0017 codifies the renumbering scheme (see "ADR numbering after the phase-03 split" above).
3. `TypedConfig` enum will grow one variant per filter across phases 04/05/06 (carries over unchanged from phase-02.1 REVIEW §4).
4. Round-robin distribution-equivalence assertion remains unit-test-only (parent-brainstorm Q1 decision; carries over unchanged).
5. If parallel fixture execution arrives, revisit `TcpProxyBackend::Drop` per M1 above (and now `TlsEchoBackend::Drop` + `Http1EchoBackend::Drop` too).
6. `enable_half_close: true` flip-fixture is the obvious follow-on per ADR-0016 — whichever phase first needs an asymmetric-close use case lands its own ADR + extends `TcpProxy` with an explicit half-close-propagation mode.

### Phase-03.1 rollovers (from REVIEW.md §3–§4)

The 03.1 REVIEW landed with one Important item and five Minor items. I1 (STATE.md stale at state 3) closed in-phase by the §7 close-out commit `1748cd2` (advanced STATE.md from state 3 to state 5 alongside REVIEW.md). The remaining items:

- **M1** — `tls_upstream_returns_err_on_connect_refused` exercises a plaintext upstream-connect path, not a TLS handshake (`crates/envoy-tls/src/tests.rs`): awareness-only, no action required. The kernel-refused 127.0.0.1:1 dial fails before any TLS handshake bytes are sent, so the test still proves `UpstreamTls::connect` propagates the connect error correctly even though the variant isn't `TlsError::Handshake`.
- **M2** — `single_cert_resolver_returns_same_cert_regardless_of_sni` accepts both `Ok` and `Err` on non-matching SNIs (`crates/envoy-tls/src/tests.rs`): awareness-only, no action required. The single-cert resolver shape is intentionally permissive on the rustls side — phase 03.2's `SniResolver` will tighten this to strict-`None`-on-miss.
- **M3** — `check_cn_or_san` uses a DER-substring scan rather than `x509-parser`-style structured introspection (`tests/differential/src/lib.rs`): awareness-only, no action required for 03.1; **tracked forward to phase 03.2** if `x509-parser` is needed for the 0006-tls-sni fixture's per-probe SAN/CN assertion (would land under a follow-up ADR per PLAN line 4054).
- **M4** — in-process integration test `crates/envoy-bin/tests/tls_downstream.rs` does not assert `expected_cn` (only proves the round-trip succeeds): awareness-only. The Docker-gated differential fixture `0004-tls-downstream` exercises the same path with the harness driver's `check_cn_or_san` assertion via `expected_cn`; the in-process test is a non-Docker regression gate, not a duplicate of the differential gate.
- **M5** — differential's TLS deps (`rcgen`, `rustls`, `rustls-pemfile`, `rustls-pki-types`, `tokio-rustls`) live under `[dependencies]` rather than `[dev-dependencies]` (`tests/differential/Cargo.toml`): awareness-only, no action required. The `tests/differential/` crate is itself a `[lib]` consumed by Docker-gated `tests/` integration tests; the deps are runtime-needed by the lib's `tls.rs` module that those tests link against. Moving them to `[dev-dependencies]` would break the `tests/differential/tests/tls_downstream.rs` link.

Phase 03.1 REVIEW §4 recommendations forward to phase 03.2 / later:

1. Phase 03.2 ADR projection numbering (ADR-0020 onward) is provisional per the ADR-0013 / ADR-0017 renumbering precedents.
2. `Cluster::name()` accessor (phase-02.1 REVIEW M1 cross-reference): unchanged from phase-02.2 §4 recommendation 1; **scheduled to close in 04.3** per 04.3 SPEC §3 D5.
3. If `x509-parser` is added in phase 03.2 for the 0006-tls-sni fixture's SAN assertion, it lands under a follow-up ADR (per phase-03.1 REVIEW §3 M3).
4. The `aws-lc-rs` crypto provider is now wired via `envoy-tls::install_default_crypto_provider`; phase 03.2's upstream-TLS consumer wiring inherits this — no duplicate crypto-provider install in `envoy-bin`.
5. `enable_half_close: true` flip-fixture deferral (carries over unchanged from phase-02.2 §4 recommendation 6).
6. Round-robin distribution-equivalence assertion remains unit-test-only (carries over unchanged from phase-02.2 §4 recommendation 4).
7. If parallel fixture execution arrives, revisit `TcpProxyBackend::Drop` and `TlsEchoBackend::Drop` (carries over unchanged from phase-02.2 §4 recommendation 5).

### Phase-03.2 rollovers (from REVIEW.md §3–§4)

The 03.2 REVIEW landed with one Important item and five Minor items. I1 (STATE.md stale) closed in-phase by the §7 close-out commit. The remaining items:

- **M1** — `needs_tls_pki` token check in `tests/differential/src/lib.rs:572-578` is asymmetric across upstream and subject templates: 5 token substrings on the upstream side, only 2 on the subject side: awareness-only, no action required for 03.2 (every fixture's subject YAML references at least one of the two checked tokens). **Tracked forward to phase 04** if a future fixture's subject template references only `{{LEAF_B_*}}` or `{{SERVER_*}}` without `{{LEAF_A_CERT_PATH}}` or `{{CA_PATH}}`. The defensive form would extract a `template_needs_tls_pki(template: &str) -> bool` helper.
- **M2** — vestigial `_server_cert` / `_server_key` alive-keeper fields on `TlsEchoBackend` (`tests/differential/src/backend.rs:102-103`): awareness-only, no action required. The fields cost two `PathBuf` allocations per backend and document the alive-keeper discipline at the type level.
- **M3** — `drive_tls` and `drive_tls_probes` share ~80% body (`tests/differential/src/lib.rs:213-266` and `:288-357`): awareness-only for 03.2. **Tracked forward to phase 04 / 05** when a third `drive_*` helper would benefit from a shared `drive_one_tls_probe` factoring. 04.1's `drive_http1` is a different shape (no TLS) so the factoring did not surface; 04.2's `Driver::Http1ProbeList` extension reused the existing `drive_http1` rather than introducing a new helper.
- **M4** — fixture 0006's `expectations.yaml` carries `equivalence.response_body: byte_exact` even though the `Driver::TlsTcpProbeList` dispatch arm in `run_fixture` does not call `assert_equivalence` (byte-equality is enforced inside `drive_tls_probes` per probe): awareness-only, no action required. Decorative-but-consistent with fixtures 0001-0005's expectations YAML shape; `Driver::TlsTcpProbeList`'s rustdoc explains the implicit-conjunction discipline. Phase 04.2's `Driver::Http1ProbeList` adopts the same posture.
- **M5** — `tls-echo-server` argv tests (4 + 1 round-trip = 5 total) miss three negative cases that `tcp-echo-server` covers: awareness-only, no action required. **Tracked forward as an optional polish pass** to bring `tls-echo-server` to coverage parity with `tcp-echo-server`. Phase 04.3's `http1-echo-server` should land with full argv-test parity from the start to avoid extending the gap.

Phase 03.2 REVIEW §4 recommendations forward to phase 04 / later:

1. `Cluster::name()` accessor (phase-02.1 REVIEW M1 cross-reference): **scheduled to close in 04.3** per 04.3 SPEC §3 D5 (the router-proxy arm's per-cluster attribution).
2. `x509-parser` is still deferred (phase-03.1 §4 rec 3 unchanged); phase 04 / 05 will likely need structured cert introspection if mTLS or peer-cert-attribution headers land. The 03.2 fixtures (only 2 leafs each; in-process `tls_sni.rs` uses byte-exact peer-cert DER comparison) sidestepped the need; 04.1 + 04.2 + 04.3 do not introduce mTLS.
3. `with_copy_to(target, source)` API quirk inline comment in `tests/differential/src/upstream.rs:73-77` remains intact; carry forward verbatim. Task 9 PROGRESS deviation 1 of 03.2 confirms `upstream.rs` was not touched in 03.2 because the 03.1 implementation pre-staged the iterator-driven mount loop walking `pki.container_mounts()`.
4. `TypedConfig` enum will grow one variant per filter across phases 04/05/06 (carries over unchanged). 04.1 added the `HttpConnectionManager` variant; 04.3 does not add a new TypedConfig variant (router proxy is HCM-internal).
5. Round-robin distribution-equivalence assertion remains unit-test-only (carries over unchanged from parent-brainstorm Q1 decision).
6. If parallel fixture execution arrives, revisit `TcpProxyBackend::Drop` AND `TlsEchoBackend::Drop` AND (now) `Http1EchoBackend::Drop` per phase-02.2 M1 + phase-03.2 M2 + 04.3-incoming.
7. `enable_half_close: true` flip-fixture deferral (carries over unchanged from phase-02.2 §4 rec 6 / phase-03.1 §4 rec 5); the 03.2 branched dial preserves the ADR-0016 `tokio::select!`-on-`tokio::io::copy` posture for both arms.
8. `tls_params` floor (TLS 1.3 only) under a new ADR if rustls-vs-Envoy version negotiation drifts during the 03.2 Docker-gated CI runs (carries over unchanged from phase-03.1 §4 rec 9).
9. Optional: factor out `drive_one_tls_probe` helper when phase 04 / 05 adds a third `drive_*` helper (M3 cross-reference). 04.1 + 04.2 did not surface the third helper; 04.3 may if upstream-TLS-on-HCM is added (deferred per 04.3 SPEC §4 non-goal).
10. Optional: bring `tls-echo-server` argv-test coverage to parity with `tcp-echo-server` (M5 cross-reference). 04.3's `http1-echo-server` should ship with full coverage from the start.

### Phase-04.1 rollovers (from REVIEW.md §3–§4)

The 04.1 REVIEW landed with zero Critical and zero Important items, 4 Minor findings, and 3 awareness-only Minor findings. Two in-phase review-fix commits (`4e7c050` Task 2, `a6f7b5e` Task 10) closed the substantive findings before propagation; no state-5 close-out commit was needed. The 7 M-track items (forwarded into 04.2 — 04.2 closed M5 partially via PROGRESS disclosure but the substance remains forwarded to 04.3 alongside the new 04.2 items below):

- **M1** — `diff_headers` value-comparison uses `find()` for value lookup (`tests/differential/src/lib.rs:200-209`), silently ignoring duplicate-header value mismatches. 04.2 did not exercise duplicate-header response shapes (fixture 0007's two probes have unique headers). **Tracked forward to phase 04.3** (upstream proxying — `Set-Cookie` / `Vary` are the natural triggers); coupled with 04.2 REVIEW M11.
- **M2** — Body-drain idle timeout returns `Ok(())` silently on read timeout (`crates/envoy-http1/src/hcm.rs:167`), dropping the pending response without surfacing an error. 04.2 did not introduce non-trivial bodies (fixture 0007's amended matcher route also returns `direct_response`). **Tracked forward to phase 04.3** (upstream proxying with non-trivial bodies) or hardening pass.
- **M3** — `envoy-http1` carries a forward-looking `envoy-cluster` path-dep at `crates/envoy-http1/Cargo.toml:10` that has no consumer in 04.1 or 04.2. **Tracked forward to 04.3** when the cluster-manager wiring lands; alternatively the dep can be dropped if a different shape emerges.
- **M4** — `strip_port` uses `rfind(':')` (`crates/envoy-http1/src/hcm.rs:246-251`) which is incorrect for bare-IPv6 Host (no port). The validator's `is_valid_dns_name` rejects `:` in domain matchers, so the bug is unobservable for valid configs. **Tracked forward to phase 04.3** — defense-in-depth + a full IPv6-Host fixture would be the natural close site.
- **M5** — Cargo.lock sync cadence diverges from phase-01/02.x/03.x precedent: phase 04.1 landed the lock inline at Task 4; 04.2 also landed the lock inline at Task 1 (Task 1 review-fix added a PROGRESS disclosure in lieu of a substantive correction; ADR-0021 prose contradicts the actual cadence per 04.2 REVIEW M9). Both 04.1 and 04.2 are doctrine-conformant per `BOOTSTRAP_PROMPT.md`. **Tracked forward to the next phase that adds a workspace member** — the planner can decide consciously rather than by accident; coupled with 04.2 REVIEW M9.
- **M6** — `drive_http1` has no per-function unit test (`tests/differential/src/lib.rs`); first coverage is via the Docker-gated fixture 0007 test. 04.2 Task 9 added 2 tests for `Http1Probe` parsing + `extra_headers` default but **NOT for `drive_http1` itself** (PROGRESS Task 11 line 133 explicitly carries M6 forward). **Tracked forward to 04.3** — fixture 0008 introduces a second `Driver::Http1ProbeList` consumer (or a fresh `Driver::Http1RouterProxy` shape); whichever lands is the natural anchor for an in-process unit test for `drive_http1` (mirroring the `hcm.rs::tests::drive` shape).
- **M7** — `TlsAcceptingHandler.inner: Arc<TcpProxy>` field (`crates/envoy-bin/src/tls_handler.rs:13-16`) is concrete-typed by design. Wrapping HCM in `TlsAcceptingHandler` would not typecheck; the 04.1 dispatch arm at `crates/envoy-bin/src/main.rs:230-236` detects-and-bails. **Tracked forward to phase 05+ brainstorm** — the choice between (a) trait-level boxing, (b) parallel `TlsAcceptingHcmHandler`, or (c) fully-erased boxed `ConnectionHandler::handle` is a design decision that depends on whether HTTP/2 lands its own `Http2Connection` ConnectionHandler shape.

Phase 04.1 REVIEW §4 forward-work — earlier-phase carryforwards that 04.1 + 04.2 did not close:

8. `Cluster::name()` accessor (M1 from phase-02.1 / 02.2 / 03.1 / 03.2 REVIEWs): **scheduled to close in 04.3** per 04.3 SPEC §3 D5 (the router-proxy arm's per-cluster proxy attribution in `RouterError` variants and `tracing` log lines is the named use site).
9. `x509-parser` deferred ADR (phase-03.1 REVIEW §4 rec 1 / 03.2 REVIEW §4 rec 2): still deferred — 04.1, 04.2, and 04.3 (per SPEC §4 non-goals) do not introduce mTLS or peer-cert-attribution headers.
10. `enable_half_close: true` flip-fixture (phase-03.2 REVIEW §4 rec 7): still deferred — 04.x does not introduce asymmetric-close semantics; 04.3 SPEC §4 explicitly defers.

Phase 04.1 ADR ledger: no new ADRs landed in phase 04.1 (per SPEC §7). The DECISIONS.md ledger reached ADR-0020 at parent-04 state-2 commit `1d9740d` and ADR-0021 at 04.2 Task 1 commit `984aedd`.

### Phase-04.2 rollovers (from REVIEW.md §3–§4)

The 04.2 REVIEW landed with zero Critical and zero Important items, 4 new Minor findings (M8–M11) plus 7 carryforwards from 04.1 (M1–M7 — see Phase-04.1 rollovers above) plus 3 earlier-phase carryforwards (#12–#14 below). Six in-phase review-fix commits (`5a6b950` Task 2 doc-comment; `3d9f985` Task 3 assertion-tightening; `17f991a` Task 4 MODE_KEYS const + multi-mode-key test; `48e615c` Task 5 TCP_PROXY as_ref intent comment; `81c6dde` Task 6 `str::get` panic-safety + expect-message disambiguation + 2 tests; `8330a86` Task 7 `Connection: close` on new HCM tests) closed substantive findings before propagation; no state-5 close-out commit was needed. The 4 new M-track items:

- **M8** — `safe_regex_partial_eq_compares_only_regex_string` test asserts `compiled: None == compiled: Some(_)` is true (`crates/envoy-config/src/bootstrap.rs:362-366` PartialEq compares only `regex: String`). Non-bug for 04.2 (no consumer compares SafeRegex values post-validate; the route walker only calls `compiled.expect`). **Tracked forward to the first phase that compares `RouteConfiguration` values post-validate** (e.g. xDS config-diff). The right shape if needed: add an `is_compiled()` accessor on SafeRegex or compare unparsed-pattern + a compile-state boolean explicitly.
- **M9** — ADR-0021's Consequences section's "dedicated state-4 commit" prose contradicts the actual Cargo.lock-inline cadence at Task 1 commit `984aedd`. Per D-3.5 ADRs are append-only; the contradiction is permanent at the ADR's date-of-landing. PROGRESS Task 1 deviation 1 is the audit trail. **Tracked forward alongside M5**: the next phase that adds a workspace dep or workspace member should pick a cadence consciously and either (a) supersede ADR-0021 with prose that matches the chosen cadence or (b) state in PLAN/PROGRESS that the inline cadence is the project's standardized posture. 04.3 SPEC §6 signpost 12 is the natural place to settle this.
- **M10** — PLAN.md late-landing at state-4 mirrors 04.1's same pattern (`docs/envoy-rust/phases/04.2-route-matchers/PLAN.md` 3562 lines, committed at `160caf0` immediately before the state-4 gate `e00f638`). Functionally reviewable (PLAN is on disk at HEAD); concern is that a fresh session entering at state-4 would have no PLAN.md to consult during the brief window before the late commit. **Tracked forward**: 04.3 planner should commit PLAN.md at clean state-2 close-out (i.e. as a dedicated single-file commit BEFORE any Task 1 commit) per BOOTSTRAP_PROMPT.md §5 to break the 04.1 → 04.2 inline-PLAN precedent. Also see "Next expected skill" above.
- **M11** — `Http1Probe.extra_headers: Vec<(String, String)>` (`tests/differential/src/lib.rs:165-169`) preserves order and allows duplicates on the wire (`drive_http1` request-builder at `lib.rs:707-711` emits each pair separately). However, the response-side `diff_headers` (per 04.1 REVIEW M1) uses `find()` for value lookup. For 04.2 fixture 0007 amendment this is non-bug (matcher-route 418 + default-route 200 responses both have unique header rows). Phase 04.3's upstream proxying may surface fixtures where the upstream emits `Set-Cookie` or duplicate `Vary` rows; at that point the response-side `diff_headers` and the request-side `extra_headers` need a coordinated fix. **Tracked forward into 04.3 alongside the M1 carryforward.**

Phase 04.2 REVIEW §4 forward-work — earlier-phase carryforwards still relevant at 04.2 close:

12. **`Cluster::name()` accessor (M1 from phase-02.1 / 02.2 / 03.1 / 03.2 REVIEWs).** **Scheduled to close in 04.3** per 04.3 SPEC §3 D5 — 04.2 did not need typed cluster-name accessors (the matcher-runtime tests reference cluster names only as opaque strings; HCM's `clusters: []` empty in fixture 0007). Phase 04.3 (upstream HTTP/1.1) is the natural close site.
13. **`x509-parser` deferred ADR.** Still deferred — 04.2 did not introduce mTLS or peer-cert-attribution headers (matcher-runtime is plaintext header introspection). 04.3's SPEC §4 also defers.
14. **`enable_half_close: true` flip-fixture.** Still deferred — 04.2 did not introduce asymmetric-close semantics (matcher-runtime is request-side; response is already direct_response). 04.3 SPEC §4 defers.

Phase 04.2 ADR ledger: ADR-0021 (`regex` permitted as a foundation for header / route matching at config-load time) landed at 04.2 Task 1 commit `984aedd`, narrowly scoped (general-purpose use requires a scope-extension ADR). The DECISIONS.md ledger advances from ADR-0020 (last landed at parent-04 state-2 commit `1d9740d`) to ADR-0021 in this phase — the only in-phase ADR per SPEC §7.

### Phase-04 ADR ledger (for reference)

ADR-0020 (split phase 04 into 04.1 + 04.2 + 04.3; landed at parent-04 state-2 commit `1d9740d`). ADR-0021 (`regex` permitted as a foundation for header / route matching; landed at 04.2 Task 1 commit `984aedd`). No further ADRs anticipated in 04.3 per 04.3 SPEC §7 — if execution surfaces a need (e.g. TLS-on-upstream-HCM, header allow-list extension beyond `BEHAVIOR_CONTRACT.md` policy, `Cluster::name()` posture decision, chunked-request-body forwarding posture), the next-sequential available number is ADR-0022.

Unlike phase 03's split decision (ADR-0017), which renumbered three projected ADRs because the split decision took the next-sequential number at split time, phase 04's split lands cleanly at ADR-0020 with no renumbering needed (ADR-0019 was the latest landed ADR before `1d9740d`; no inter-ADR landings occurred between phase-03's close at `ca81226` and `1d9740d`). The parent-04 SPEC's projected ADR-0020 + ADR-0021 numbers match the actual landed numbers.

### Phase-00 deferrals still open

- Minors M1, M2, M4, M5, M6, M7, M8 (see `docs/envoy-rust/phases/00-bootstrap/REVIEW.md`). None block phase 04.3.
- Important I3 (SIGKILL → SIGTERM graceful termination of the subject subprocess): still deferred. The `nix` crate remains the stated blocker (not on D-3.2 permitted-foundations list). Phase-01, phase-02 (across 02.1 and 02.2), phase-03 (across 03.1 and 03.2), and phase-04 (across 04.1 and 04.2) all chose not to take it. Phase 04.3 does not need `nix`. A future phase that genuinely needs `nix` adds it under a new ADR and closes this item.
- N2 (phase-00 deferred Minor — `deny_unknown_fields` regression-test gap on deeper struct levels): **closed** by phase-01 Task 4 Step 4 via five new regression tests; phase 03.1's 10 new validator tests + phase 04.1's HCM/RouteConfiguration/DirectResponse validator tests + phase 04.2's HeaderMatcher/StringMatcher/SafeRegex validator tests continue the discipline on the new struct levels.

### Phase-01 ADR ledger (for reference)

ADR-0008 (envoy-config extraction), ADR-0009 (cargo-fuzz + libfuzzer-sys as fuzz-only dev deps), ADR-0010 (nightly toolchain, explicit `+nightly` CI invocation; workspace-root pin stays stable), ADR-0011 (phase-01 defers response-header equivalence to phase 04; `server: envoy-rust` tolerated until then — closed by 04.1's BEHAVIOR_CONTRACT.md `server` allow-list row), ADR-0012 (nested nightly pin in fuzz subcrate; narrowly supersedes ADR-0010 on that single sub-point while preserving its main decision).

### Phase-02.1 ADR ledger (for reference)

ADR-0013 (split phase 02 into 02.1 + 02.2; landed at `1c38ca9` during parent-phase 02 state 2), ADR-0014 (YAML-native `typed_config` deserialization until the xDS/protos family lands; landed at `6d1f8d6` during 02.1 Task 1).

### Phase-02.2 ADR ledger (for reference)

ADR-0015 (cross-container host reachability via `host.docker.internal` + `host-gateway`; landed at `435c6fa` during 02.2 Task 1), ADR-0016 (phase 02 TCP proxy runs with Envoy's default `enable_half_close: false`; landed at `435c6fa` during 02.2 Task 1).

### Phase-03.1 ADR ledger (for reference)

ADR-0017 (split phase 03 into 03.1 + 03.2; landed at `f256d2c` during parent-phase 03 state 2), ADR-0018 (`rcgen` + `tempfile` permitted as dev-test-harness-only foundations; landed at `f93a062` during 03.1 Task 1), ADR-0019 (`tokio-rustls` + `rustls-pemfile` covered by the rustls foundations grant; landed at `f93a062` during 03.1 Task 1).

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The phase 04.2 state-6 close-out (this commit) advances STATE.md to phase 04.3 lifecycle state 2. The next session enters phase 04.3 state 2 via `superpowers:writing-plans`, producing `docs/envoy-rust/phases/04.3-router-upstream/PLAN.md` per SKILL_ROUTING.md, committed cleanly at state-2 close-out (per M10 carryforward).
