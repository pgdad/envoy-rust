# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `04.2`
**slug:** `04.2-route-matchers` (created during the parent-04 state-2 commit `1d9740d`; SPEC.md committed alongside ADR-0020 + sibling sub-phase SPECs).
**directory:** `docs/envoy-rust/phases/04.2-route-matchers/` exists; contains `SPEC.md` (698 lines).
**status:** phase 04.2 lifecycle **state 2 (SPEC.md exists, PLAN.md does not)** — SPEC.md landed at parent-04 state-2 commit `1d9740d` alongside ADR-0020 (split decision), the sibling 04.1 + 04.3 SPECs, and ROADMAP row appends. ROADMAP row `04.2` flips from `planned` to `in-progress` at this commit (the phase-04.1 state-6 close-out) per ROADMAP-schema invariant ("a phase enters `in-progress` when STATE.md points at it as the active phase with the directory created"). Both conditions are satisfied here: STATE.md now points at 04.2 as active, and the 04.2 directory has existed since `1d9740d`. Mirrors the phase-03.1-state-6 precedent (`64ea760`) which atomically flipped 03.1 to `done` AND 03.2 to `in-progress` in the same commit. Parent row `04` stays `in-progress` per the ROADMAP-schema invariant ("parent flips to `done` only after all sub-phases are `done`"); it will flip to `done` in 04.3's final commit (mirrors phase 03's `ca81226`-shape close-out where 03.2's phase-done commit also closed parent 03).

Phase 04 (`04-http1`) parent is **in-progress** with `sub-phases: 04.1, 04.2, 04.3` per **ADR-0020**'s split (landed at parent-04 state-2 commit `1d9740d`). Parent SPEC at `docs/envoy-rust/phases/04-http1/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `805433e`, the parent-04 state-1 brainstorm output); for execution purposes it is superseded by the three sub-phase SPECs (`phases/04.1-hcm-direct-response/SPEC.md`, `phases/04.2-route-matchers/SPEC.md`, `phases/04.3-router-upstream/SPEC.md`).

Sibling sub-phases:

- **04.1 (`04.1-hcm-direct-response`)** — `status: done` as of this commit (state-6 close-out). SPEC.md (1074 lines) + PLAN.md (3956 lines) + PROGRESS.md (296 lines) + REVIEW.md (242 lines) all in tree. Phase delivered: the new `envoy-http1` library crate (codec/headers/date/response/hcm modules; 19 unit tests; `#![forbid(unsafe_code)]`); HCM as a `ConnectionHandler` impl walking inline `RouteConfiguration` first-match-wins on Host then path; hardcoded router-filter call site emitting `direct_response`; envoy-config schema growth (`HttpConnectionManagerConfig` + `RouteConfiguration` + `DirectResponse` + `DataSource.inline_string` extension; 10 new `ConfigError` variants; `validate_hcm` + `validate_data_source` + `is_valid_dns_name` + private `Required` enum; 14 + 1 review-fix new tests; 2 fuzz-corpus seeds); envoy-bin HCM dispatch arm + factored `build_downstream_tls_for_listener` helper + HCM+TLS detect-and-bail; in-process `crates/envoy-bin/tests/http1_direct_response.rs` integration backstop (209 LoC); differential harness extensions (`Driver::Http1` + `drive_http1` + `HEADER_ALLOW_LIST` + `diff_headers` + 3 unit tests); fixture 0007-http1-direct-response (5 files) + Docker-gated acceptance test; `BEHAVIOR_CONTRACT.md`'s Header allow-list table populated with `server` + `date`. Two in-phase review-fix commits (`4e7c050` Task 2 → `Required` enum + `rejects_empty_domains`; `a6f7b5e` Task 10 → tracing warns + empty-Host reject + 2 additional tests). REVIEW verdict **Approved with M-track follow-ups** at `b6e305d`; M1–M7 tracked forward into 04.2 / 04.3 / phase 05 / hardening pass — see "Phase-04.1 rollovers" below.
- **04.3 (`04.3-router-upstream`)** — `status: planned`. SPEC.md exists (769 lines; landed at parent-04 state-2 commit `1d9740d`). Will enter active when 04.2 closes; depends-on `04.2` per the strict ordering (04.3 extends both 04.1's HCM and 04.2's matcher schema).

Phase 03 (`03-tls-tcp`) is **done** as of commit `ca81226`. Both sub-phases are done: `03.1-tls-foundation-downstream` (commit `64ea760`) and `03.2-tls-upstream-sni` (commit `ca81226`). ROADMAP rows `03`, `03.1`, and `03.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `a3f3474`); for execution purposes it was superseded by the two sub-phase SPECs (`phases/03.1-tls-foundation-downstream/SPEC.md` and `phases/03.2-tls-upstream-sni/SPEC.md`).

Phase 04.1 `REVIEW.md` verdict is **Approved with M-track follow-ups** (state 5 complete; landed at `b6e305d`; no Critical or Important findings; 4 Minor findings (M1 `diff_headers` duplicate-header semantics, M2 body-drain idle timeout silent close, M4 `strip_port` IPv6 correctness, M5 Cargo.lock sync cadence) and 3 awareness-only Minor findings (M3 `envoy-cluster` pre-staged dep, M6 `drive_http1` per-function unit test, M7 `TlsAcceptingHandler` generalization for HCM+TLS); 7 M-track items tracked forward into 04.2 / 04.3 / phase 05 / hardening pass — see Notes below). Two in-phase review-fix commits closed substantive findings before propagation (Task 2 `4e7c050`; Task 10 `a6f7b5e`).

Phase 03.2 `REVIEW.md` verdict is **Approved with fixes** (state 5 complete; I1 closed in-phase via the §7 close-out commit `f0b4a48`; M1–M5 tracked forward — see Notes below). Phase 03.1 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `1748cd2`; M1–M5 tracked forward — see Notes below).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 16–22, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 2): the next session — operating as the state-2 session of phase 04.2 — invokes **`superpowers:writing-plans`** scoped to phase 04.2, producing `docs/envoy-rust/phases/04.2-route-matchers/PLAN.md`. The PLAN converts 04.2 SPEC §3's deliverables into a per-task checklist (estimated per 04.2 SPEC §5; ADR-0021 (`regex` permitted foundation) lands inline at 04.2 Task 1 per the phase-03.1 ADR-0018+ADR-0019 inline-Task-1 precedent). Each task respects `superpowers:test-driven-development` per doctrine D-3.1.

Per the user's standing preference (auto-memory `feedback_execution_style`), state-3 execution will use `superpowers:subagent-driven-development` over inline `executing-plans` — do not present the two-option fork at state-3 entry.

**Plan splitting gate evaluation** (BOOTSTRAP_PROMPT.md §5 state 2 / §6.1; 04.2 SPEC §5; parent SPEC §5):

- Estimated 04.2 surface per parent SPEC §5: smaller than 04.1 (no new fixture; fixture 0007 amended to exercise a header-matcher route; envoy-config gains all 7 `HeaderMatcher` modes + `StringMatcher` + `invert_match`; `regex` dep landed under ADR-0021). Comfortably under both gates (~25 tasks / ~1500 LoC bounds).
- **Decision: 04.2 stays unified.** No nested split. Per parent SPEC §5 closing paragraph + the parent-04 brainstorm's express avoidance of nested splits, if either gate fires mid-PLAN-write invoke `superpowers:systematic-debugging` first — nested splits of an already-split sub-phase deserve a fresh root-cause analysis.

Inputs the 04.2 state-2 session should read, in order, before drafting `PLAN.md`:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (rows 04 `in-progress` with `sub-phases: 04.1, 04.2, 04.3`; row 04.1 `done`; row 04.2 `in-progress` (depends-on 04.1, satisfied); row 04.3 `planned` (depends-on 04.2)).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0020`; ADR-0021 (`regex`) projected to land at 04.2 Task 1 per parent-04 SPEC §7 / 04.2 SPEC §7).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `Header allow-list` section was populated by 04.1 with `server` + `date` entries; 04.2 may extend it depending on header-matcher response-header surface).
6. `docs/envoy-rust/SKILL_ROUTING.md` (state-2 routing).
7. **`docs/envoy-rust/phases/04.2-route-matchers/SPEC.md`** (the authoritative sub-phase design contract — referenced at every task under "Source of truth: SPEC.md" at the top of the resulting PLAN.md).
8. `docs/envoy-rust/phases/04-http1/SPEC.md` (parent SPEC — context for the full phase-04 design + cross-sub-phase architectural rules at §3 closing; execution follows the 04.2 sub-phase SPEC, not the parent).
9. **`docs/envoy-rust/phases/04.1-hcm-direct-response/PLAN.md` + `PROGRESS.md` + `REVIEW.md`** (immediate-prior sub-phase precedent — task cadence, TDD framing, PROGRESS-formatting conventions; 04.1 REVIEW §3 M1–M7 + §4 forward-work items inform 04.2's PLAN where applicable: M1 `diff_headers` duplicate-header semantics may surface in 04.2's header-matcher response shapes; M6 `drive_http1` per-function unit test naturally lands when the second `Driver::Http1` consumer arrives).
10. `docs/envoy-rust/phases/04.1-hcm-direct-response/SPEC.md` (sibling sub-phase SPEC — 04.2 amends 04.1's fixture 0007 to add a header-matcher route; the route schema 04.2 extends is the exact schema 04.1 landed).
11. `docs/envoy-rust/phases/03.2-tls-upstream-sni/PLAN.md` + `PROGRESS.md` + `REVIEW.md` (second-most-recent plan + progress + review precedent for cadence cross-check).
12. `docs/envoy-rust/phases/02.1-config-cluster/PLAN.md` (phase-02.1 introduced the schema/validator cadence that 04.2's `HeaderMatcher` + `StringMatcher` schema work mirrors for shape).

## Last commit

Phase 04.1 state-6 close-out commit (this commit): lands the ROADMAP flip + STATE advance per the phase-03.1 `64ea760` precedent (sub-phase done, sibling enters `in-progress`, parent stays `in-progress`). Touches:

- **`docs/envoy-rust/ROADMAP.md`** — flips row `04.1` status from `planned` to `done` (the prior `planned` → `in-progress` flip was elided since the state-3 entry session inlined PLAN.md inside Task 1's commit `c41ae7f` rather than landing a dedicated state-2 STATE-advance commit; the state-6 close-out resolves this by going `planned` → `done` directly, well-disclosed here). Also flips row `04.2` status from `planned` to `in-progress` per ROADMAP-schema invariant 3 (a phase enters `in-progress` when STATE.md points at it as the active phase with the directory created); both conditions are satisfied here since the 04.2 directory has existed since `1d9740d`. Parent row `04` stays `in-progress` per the schema invariant ("parent flips to `done` only after all sub-phases are `done`") — 04.3 is still `planned`. Mirrors the phase-03.1 state-6 commit `64ea760` shape (which atomically flipped 03.1 to `done` AND 03.2 to `in-progress` in one commit while parent 03 stayed `in-progress`).
- **`docs/envoy-rust/STATE.md`** (this file) — advances active phase from `04.1` lifecycle state 5 to `04.2` lifecycle state 2 (sub-phase SPEC.md exists from `1d9740d`, PLAN.md does not). Refreshes the Notes section to add "Phase-04.1 rollovers" with the 7 M-track items from 04.1 REVIEW §4.

No code changes. Mirrors phase-03.1 state-6 commit `64ea760` shape (which touched ROADMAP.md + STATE.md only).

Predecessor commits in phase 04.1:

- `b6e305d` — `phase 04.1: state 5 REVIEW.md Approved with M-track follow-ups` (landed REVIEW.md only; verdict **Approved with M-track follow-ups**; no in-phase fixes needed at state 5 since the substantive review findings were already closed in-phase by Task 2 review-fix `4e7c050` and Task 10 review-fix `a6f7b5e`).
- `05a5f23` — `phase 04.1: state-4 phase-done gate verification (task 17)` (state-4 gate green on first attempt: fmt/build/clippy/test/deny all clean; 212 passed + 1 ignored).
- `c41ae7f` — `phase 04.1: envoy-config — HCM TypedConfig variant + ... (task 1)` (state-3 entry; PLAN.md landed inline rather than via a dedicated state-2 close-out commit — well-disclosed deviation from the phase-03.x precedent).
- `1d9740d` — `phase 04: state-2 split formalization — ADR-0020 + 3 sub-phase SPECs (04.1/04.2/04.3)` (parent-04 state-2 close-out; landed ADR-0020 + 04.1/04.2/04.3 SPEC.md + ROADMAP rows + STATE advance in one atomic move per phase-03 `f256d2c` precedent).
- `805433e` — `phase 04: state-1 brainstorm — parent SPEC.md projecting 3-way split (04.1/04.2/04.3)` (parent SPEC + ROADMAP row 04 flip + STATE.md state-1→state-2 advance; mirrored phase-03 state-1 commit `a3f3474` shape).

## Last updated

2026-04-27 (phase 04.1 state-6 close-out; ROADMAP row 04.1 flipped `planned` → `done`; ROADMAP row 04.2 flipped `planned` → `in-progress`; STATE.md advanced active phase from 04.1 lifecycle state 5 to 04.2 lifecycle state 2; next-skill `superpowers:writing-plans` scoped to phase 04.2).

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

### Phase-03.1 rollovers (from REVIEW.md §3–§4)

The 03.1 REVIEW landed with one Important item and five Minor items. I1 (STATE.md stale at state 3) closed in-phase by the §7 close-out commit `1748cd2` (advanced STATE.md from state 3 to state 5 alongside REVIEW.md). The remaining items:

- **M1** — `tls_upstream_returns_err_on_connect_refused` exercises a plaintext upstream-connect path, not a TLS handshake (`crates/envoy-tls/src/tests.rs`): awareness-only, no action required. The kernel-refused 127.0.0.1:1 dial fails before any TLS handshake bytes are sent, so the test still proves `UpstreamTls::connect` propagates the connect error correctly even though the variant isn't `TlsError::Handshake`.
- **M2** — `single_cert_resolver_returns_same_cert_regardless_of_sni` accepts both `Ok` and `Err` on non-matching SNIs (`crates/envoy-tls/src/tests.rs`): awareness-only, no action required. The single-cert resolver shape is intentionally permissive on the rustls side — phase 03.2's `SniResolver` will tighten this to strict-`None`-on-miss.
- **M3** — `check_cn_or_san` uses a DER-substring scan rather than `x509-parser`-style structured introspection (`tests/differential/src/lib.rs`): awareness-only, no action required for 03.1; **tracked forward to phase 03.2** if `x509-parser` is needed for the 0006-tls-sni fixture's per-probe SAN/CN assertion (would land under a follow-up ADR per PLAN line 4054).
- **M4** — in-process integration test `crates/envoy-bin/tests/tls_downstream.rs` does not assert `expected_cn` (only proves the round-trip succeeds): awareness-only. The Docker-gated differential fixture `0004-tls-downstream` exercises the same path with the harness driver's `check_cn_or_san` assertion via `expected_cn`; the in-process test is a non-Docker regression gate, not a duplicate of the differential gate.
- **M5** — differential's TLS deps (`rcgen`, `rustls`, `rustls-pemfile`, `rustls-pki-types`, `tokio-rustls`) live under `[dependencies]` rather than `[dev-dependencies]` (`tests/differential/Cargo.toml`): awareness-only, no action required. The `tests/differential/` crate is itself a `[lib]` consumed by Docker-gated `tests/` integration tests; the deps are runtime-needed by the lib's `tls.rs` module that those tests link against. Moving them to `[dev-dependencies]` would break the `tests/differential/tests/tls_downstream.rs` link.

Phase 03.1 REVIEW §4 recommendations forward to phase 03.2 / later:

1. Phase 03.2 ADR projection numbering (ADR-0020 onward) is provisional per the ADR-0013 / ADR-0017 renumbering precedents.
2. `Cluster::name()` accessor (phase-02.1 REVIEW M1 cross-reference): unchanged from phase-02.2 §4 recommendation 1; phase 03.2 D4 is the next opportunistic close site.
3. If `x509-parser` is added in phase 03.2 for the 0006-tls-sni fixture's SAN assertion, it lands under a follow-up ADR (per phase-03.1 REVIEW §3 M3).
4. The `aws-lc-rs` crypto provider is now wired via `envoy-tls::install_default_crypto_provider`; phase 03.2's upstream-TLS consumer wiring inherits this — no duplicate crypto-provider install in `envoy-bin`.
5. `enable_half_close: true` flip-fixture deferral (carries over unchanged from phase-02.2 §4 recommendation 6).
6. Round-robin distribution-equivalence assertion remains unit-test-only (carries over unchanged from phase-02.2 §4 recommendation 4).
7. If parallel fixture execution arrives, revisit `TcpProxyBackend::Drop` and `TlsEchoBackend::Drop` (carries over unchanged from phase-02.2 §4 recommendation 5).

### Phase-04.1 rollovers (from REVIEW.md §3–§4)

The 04.1 REVIEW landed with zero Critical and zero Important items, 4 Minor findings, and 3 awareness-only Minor findings. Two in-phase review-fix commits (`4e7c050` Task 2, `a6f7b5e` Task 10) closed the substantive findings before propagation; no state-5 close-out commit was needed. The 7 M-track items:

- **M1** — `diff_headers` value-comparison uses `find()` for value lookup (`tests/differential/src/lib.rs:200-209`), silently ignoring duplicate-header value mismatches. Awareness-only for 04.1 (fixture 0007's HCM-direct-response output has no duplicate headers). **Tracked forward to phase 04.2** (header matchers — duplicate-header response shapes may appear); the fix would evolve the `find()` form to a `filter().collect::<Vec<_>>()` ordered-multiset comparison.
- **M2** — Body-drain idle timeout returns `Ok(())` silently on read timeout (`crates/envoy-http1/src/hcm.rs:167`), dropping the pending response without surfacing an error. Awareness-only for 04.1 (fixture 0007 is `GET /healthz` with no body; the body-drain path never fires). **Tracked forward to phase 04.3** (upstream proxying with non-trivial bodies) or hardening pass — the slow-body-attack mitigation should surface as a typed error rather than a silent close.
- **M3** — `envoy-http1` carries a forward-looking `envoy-cluster` path-dep at `crates/envoy-http1/Cargo.toml:10` that has no consumer in 04.1. PROGRESS Task 4 documents the rationale (forward-looking for 04.3 + HCM error paths). Awareness-only; **tracked forward to 04.3** when the cluster-manager wiring lands; alternatively the dep can be dropped at 04.3 if a different shape emerges (cheap to add back).
- **M4** — `strip_port` uses `rfind(':')` (`crates/envoy-http1/src/hcm.rs:246-251`) which is incorrect for bare-IPv6 Host (no port) — for `Host: [::1]` the slice would be `"[::"`. The validator's `is_valid_dns_name` rejects `:` in domain matchers, so the bug is unobservable for valid configs. Awareness-only for 04.1. **Tracked forward to phase 04.3** — defense-in-depth would be `if host.starts_with('[') && let Some(end) = host.rfind(']') { ... } else { ... }`; a full IPv6-Host fixture would be the right place to land the tightening.
- **M5** — Cargo.lock sync cadence diverges from phase-01/02.x/03.x precedent: phase 04.1 landed the lock inline at Task 4 (`37e074c`) when the new `envoy-http1` workspace member was added, rather than as a dedicated single-file post-state-4 commit. PROGRESS Task 17 names the deviation explicitly; both shapes are doctrine-conformant per `BOOTSTRAP_PROMPT.md`. Awareness-only; **tracked forward to the next phase that adds a workspace member** — the planner can decide consciously rather than by accident.
- **M6** — `drive_http1` has no per-function unit test (`tests/differential/src/lib.rs`); first coverage is via the Docker-gated fixture 0007 test. PROGRESS Task 14 "Open concern" names this explicitly. Awareness-only — the helper is a thin wrapper around well-tested deps and the in-process `crates/envoy-bin/tests/http1_direct_response.rs` exercises the same response-parsing shape. **Tracked forward to 04.2 / 04.3** — if either phase adds a second `Driver::Http1` consumer, that's the natural place to land an in-process unit test for `drive_http1` (mirroring the `hcm.rs::tests::drive` shape).
- **M7** — `TlsAcceptingHandler.inner: Arc<TcpProxy>` field (`crates/envoy-bin/src/tls_handler.rs:13-16`) is concrete-typed by design (per the inherent-generic `handle::<S>` precedent). Wrapping HCM in `TlsAcceptingHandler` would not typecheck; the 04.1 dispatch arm at `crates/envoy-bin/src/main.rs:230-236` detects-and-bails with a clear error message. **Tracked forward to phase 05+ brainstorm** — the choice between (a) trait-level boxing, (b) parallel `TlsAcceptingHcmHandler`, or (c) fully-erased boxed `ConnectionHandler::handle` is a design decision that depends on whether HTTP/2 lands its own `Http2Connection` ConnectionHandler shape.

Phase 04.1 REVIEW §4 forward-work — earlier-phase carryforwards still relevant at 04.1 close:

8. `Cluster::name()` accessor (M1 from phase-02.1 / 02.2 / 03.1 / 03.2 REVIEWs): still deferred to phase 06 (stats family) per phase-03.2 REVIEW §4 rec 1 — 04.1 did not need typed cluster-name accessors (HCM's `clusters: []` empty). Phase 04.3 (upstream HTTP/1.1) will revisit when per-cluster error attribution wants a typed accessor.
9. `x509-parser` deferred ADR (phase-03.1 REVIEW §4 rec 1 / 03.2 REVIEW §4 rec 2): still deferred — 04.1 did not introduce mTLS or peer-cert-attribution headers.
10. `enable_half_close: true` flip-fixture (phase-03.2 REVIEW §4 rec 7): still deferred — 04.1 did not introduce asymmetric-close semantics.

Phase 04.1 ADR ledger: no new ADRs landed in phase 04.1 (per SPEC §7). The DECISIONS.md ledger remains at ADR-0020 (last landed at parent-04 state-2 commit `1d9740d`). ADR-0021 (`regex` permitted as a foundation for header / route matching) projected to land at 04.2 Task 1 — mirrors phase 03.1's inline-Task-1 ADR-landing pattern.

### Phase-03.2 rollovers (from REVIEW.md §3–§4)

The 03.2 REVIEW landed with one Important item and five Minor items. I1 (STATE.md stale) closed in-phase by the §7 close-out commit (this commit) — STATE.md advanced from state 3 to state 5 alongside REVIEW.md. The remaining items:

- **M1** — `needs_tls_pki` token check in `tests/differential/src/lib.rs:572-578` is asymmetric across upstream and subject templates: 5 token substrings on the upstream side, only 2 on the subject side: awareness-only, no action required for 03.2 (every fixture's subject YAML references at least one of the two checked tokens). **Tracked forward to phase 04** if a future fixture's subject template references only `{{LEAF_B_*}}` or `{{SERVER_*}}` without `{{LEAF_A_CERT_PATH}}` or `{{CA_PATH}}`. The defensive form would extract a `template_needs_tls_pki(template: &str) -> bool` helper.
- **M2** — vestigial `_server_cert` / `_server_key` alive-keeper fields on `TlsEchoBackend` (`tests/differential/src/backend.rs:102-103`): awareness-only, no action required. The fields cost two `PathBuf` allocations per backend and document the alive-keeper discipline at the type level.
- **M3** — `drive_tls` and `drive_tls_probes` share ~80% body (`tests/differential/src/lib.rs:213-266` and `:288-357`): awareness-only for 03.2. **Tracked forward to phase 04 / 05** when a third `drive_*` helper would benefit from a shared `drive_one_tls_probe` factoring.
- **M4** — fixture 0006's `expectations.yaml` carries `equivalence.response_body: byte_exact` even though the `Driver::TlsTcpProbeList` dispatch arm in `run_fixture` does not call `assert_equivalence` (byte-equality is enforced inside `drive_tls_probes` per probe): awareness-only, no action required. Decorative-but-consistent with fixtures 0001-0005's expectations YAML shape; `Driver::TlsTcpProbeList`'s rustdoc explains the implicit-conjunction discipline.
- **M5** — `tls-echo-server` argv tests (4 + 1 round-trip = 5 total) miss three negative cases that `tcp-echo-server` covers (`argv_rejects_missing_port_flag`, `argv_rejects_non_numeric_port`, `argv_rejects_trailing_argument`): awareness-only, no action required. **Tracked forward as an optional polish pass** to bring `tls-echo-server` to coverage parity with `tcp-echo-server`.

Phase 03.2 REVIEW §4 recommendations forward to phase 04 / later:

1. `Cluster::name()` accessor (phase-02.1 REVIEW M1 cross-reference; phase-02.2 §4 rec 1; phase-03.1 §4 rec 2): unchanged from prior carryforwards. Phase 04 (HCM) or phase 05 (HTTP/2) may surface the need; phase 06 (stats) is the natural close-out target. Phase 03.2 Task 5 explicitly evaluated and re-deferred per SPEC §3 D4.
2. `x509-parser` is still deferred (phase-03.1 §4 rec 3 unchanged); phase 04 / 05 will likely need structured cert introspection if mTLS or peer-cert-attribution headers land. The 03.2 fixtures (only 2 leafs each; in-process `tls_sni.rs` uses byte-exact peer-cert DER comparison) sidestepped the need.
3. `with_copy_to(target, source)` API quirk inline comment in `tests/differential/src/upstream.rs:73-77` remains intact; carry forward verbatim. Task 9 PROGRESS deviation 1 confirms `upstream.rs` was not touched in 03.2 because the 03.1 implementation pre-staged the iterator-driven mount loop walking `pki.container_mounts()`.
4. `TypedConfig` enum will grow one variant per filter across phases 04/05/06 (carries over unchanged).
5. Round-robin distribution-equivalence assertion remains unit-test-only (carries over unchanged from parent-brainstorm Q1 decision).
6. If parallel fixture execution arrives, revisit `TcpProxyBackend::Drop` AND `TlsEchoBackend::Drop` per phase-02.2 M1 + phase-03.2 M2 (carries over unchanged; `TlsEchoBackend` inherits the same SIGKILL-Drop posture).
7. `enable_half_close: true` flip-fixture deferral (carries over unchanged from phase-02.2 §4 rec 6 / phase-03.1 §4 rec 5); the 03.2 branched dial preserves the ADR-0016 `tokio::select!`-on-`tokio::io::copy` posture for both arms.
8. `tls_params` floor (TLS 1.3 only) under a new ADR if rustls-vs-Envoy version negotiation drifts during the 03.2 Docker-gated CI runs (carries over unchanged from phase-03.1 §4 rec 9).
9. Optional: factor out `drive_one_tls_probe` helper when phase 04 / 05 adds a third `drive_*` helper (M3 cross-reference).
10. Optional: bring `tls-echo-server` argv-test coverage to parity with `tcp-echo-server` (M5 cross-reference).

### Phase-03.2 ADR ledger (for reference)

No new ADRs landed in phase 03.2 (per SPEC §7). The DECISIONS.md ledger remains at ADR-0019 (last landed in 03.1 Task 1 commit `f93a062`). If phase 04 or later phases surface the need for any of the deferred ADRs (TLS protocol-version pin, wildcard SNI semantics, `x509-parser`, `Cluster::name()`-attribution variant), they land at the next-sequential available number (ADR-0020+).

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

### Phase-03.1 ADR ledger (for reference)

ADR-0017 (split phase 03 into 03.1 + 03.2; landed at `f256d2c` during parent-phase 03 state 2), ADR-0018 (`rcgen` + `tempfile` permitted as dev-test-harness-only foundations; landed at `f93a062` during 03.1 Task 1), ADR-0019 (`tokio-rustls` + `rustls-pemfile` covered by the rustls foundations grant; landed at `f93a062` during 03.1 Task 1).

### Phase-04 ADR ledger (for reference)

ADR-0020 (split phase 04 into 04.1 + 04.2 + 04.3; landed at parent-04 state-2 commit `1d9740d`). ADR-0021 (`regex` permitted as a foundation for header / route matching; projected by parent-04 SPEC §7 / 04.2 SPEC §7) projected to land at 04.2 Task 1 — mirrors phase 03.1's inline-Task-1 ADR-landing pattern (ADR-0018 + ADR-0019 landed inline at 03.1 Task 1).

Unlike phase 03's split decision (ADR-0017), which renumbered three projected ADRs because the split decision took the next-sequential number at split time, phase 04's split lands cleanly at ADR-0020 with no renumbering needed (ADR-0019 was the latest landed ADR before `1d9740d`; no inter-ADR landings occurred between phase-03's close at `ca81226` and `1d9740d`). The parent-04 SPEC's projected ADR-0020 + ADR-0021 numbers match the actual landed (or projected-landing) numbers.

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The phase 04.1 state-6 close-out (this commit) advances STATE.md to phase 04.2 lifecycle state 2. The next session enters phase 04.2 state 2 via `superpowers:writing-plans`, producing `docs/envoy-rust/phases/04.2-route-matchers/PLAN.md` per SKILL_ROUTING.md.
