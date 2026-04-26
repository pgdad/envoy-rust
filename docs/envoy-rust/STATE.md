# envoy-rust Project State

> This file is the single source of truth for "what next." Cold-start reads it
> first after `MISSION.md`. It names the active phase directory and the
> next expected skill invocation. Any session mutating project state must end
> by updating this file.

## Active phase

**id:** `04`
**slug:** `04-http1`
**directory:** `docs/envoy-rust/phases/04-http1/` exists; contains `SPEC.md` (parent SPEC; landed at this commit via state-1 brainstorm).
**status:** phase 04 lifecycle **state 2 (parent SPEC.md exists; sub-phase SPECs do not)** — parent SPEC landed at this commit per the state-1 brainstorm output. The brainstorm decided a 3-way split into sub-phases `04.1`, `04.2`, `04.3` (rationale codified in parent SPEC §5; will be formalized as **ADR-0020** at parent-04 state-2). ROADMAP row `04` is `status: in-progress` as of this commit (flipped from `planned` per ROADMAP-schema invariant 3); `sub-phases` column populated as `04.1, 04.2, 04.3`. Sub-phase rows themselves are not yet appended — those land at parent-04 state-2 alongside ADR-0020 and the sub-phase SPECs.

Phase 03 (`03-tls-tcp`) is **done** as of commit `ca81226`. Both sub-phases are done: `03.1-tls-foundation-downstream` (commit `64ea760`) and `03.2-tls-upstream-sni` (commit `ca81226`). ROADMAP rows `03`, `03.1`, and `03.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `a3f3474`); for execution purposes it was superseded by the two sub-phase SPECs (`phases/03.1-tls-foundation-downstream/SPEC.md` and `phases/03.2-tls-upstream-sni/SPEC.md`).

Phase 03.2 `REVIEW.md` verdict is **Approved with fixes** (state 5 complete; I1 closed in-phase via the §7 close-out commit `f0b4a48`; M1–M5 tracked forward — see Notes below). Phase 03.1 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `1748cd2`; M1–M5 tracked forward — see Notes below).

Parent phase `02-tcp-proxy` is **done** as of commit `f04e21a`. Both sub-phases are done: `02.1-config-cluster` (commit `d447f53`) and `02.2-listener-tcp-proxy` (commit `f04e21a`). ROADMAP rows `02`, `02.1`, and `02.2` are all `status: done`. Parent SPEC at `docs/envoy-rust/phases/02-tcp-proxy/SPEC.md` remains in-tree unedited as the committed historical artifact (last touched at SHA `50349da`).

Phase 02.2 `REVIEW.md` verdict is **Approved** (state 5 complete; I1 closed in-phase via the §7 close-out commit `fc87505`; M1–M4 tracked forward — see Notes below). Phase 02.1 `REVIEW.md` verdict is **Approved** (I1 + I2 closed in-phase; I3 + M1–M4 tracked forward — see Notes below).

Phase 01 (`01-static-bootstrap-config`) is **done** as of commit `aef36ce`; phase 00 (`00-bootstrap`) is **done** as of commit `e5afc35`.

## Next expected skill

Per the phase lifecycle state machine (`SKILL_ROUTING.md` lines 16–22, verbatim from `BOOTSTRAP_PROMPT.md` §5 state 2): the next session — operating as the parent-04 state-2 session — invokes **`superpowers:writing-plans`** adapted for the split-output shape. Per the phase-03 precedent (parent-03 state-2 commit `f256d2c` landed ADR-0017 + both sub-phase SPECs), parent-04 state-2 lands:

1. **ADR-0020** — formalizes the 3-way split decision projected in parent SPEC §5. Provenance footer cites the parent-04 brainstorm scope choices (all 7 `HeaderMatcher` modes; `prefix` + `path` + `headers` on `RouteMatch`) that pushed a 2-way 04.α over the §6.1 gate.
2. **`docs/envoy-rust/phases/04.1-<slug>/SPEC.md`** — sub-phase 04.1's design contract (codec + HCM + minimal routing + direct_response + fixture 0007). Slug picked at SPEC writeup; working sketch `04.1-codec-hcm`.
3. **`docs/envoy-rust/phases/04.2-<slug>/SPEC.md`** — sub-phase 04.2's design contract (header matcher fan-out + ADR-0021 (`regex`) + fixture 0007 amendment). Slug working sketch `04.2-matchers`.
4. **`docs/envoy-rust/phases/04.3-<slug>/SPEC.md`** — sub-phase 04.3's design contract (upstream HTTP/1.1 + router proxy arm + fixture 0008 + http1-echo-server helper). Slug working sketch `04.3-router-upstream`.
5. **ROADMAP edits:** append rows `04.1`, `04.2`, `04.3` (each `status: planned`; will flip to `in-progress` as STATE.md points at each per ROADMAP-schema invariant 3).

NO sub-phase PLAN.md is written at parent-04 state-2 — that's each sub-phase's own state-2 (after its SPEC.md exists). Each sub-phase then enters its own state 3 with its own subagent-driven implementation cadence.

Inputs the parent-04 state-2 session should read, in order, before drafting the sub-phase SPECs:

1. `docs/envoy-rust/MISSION.md` (mission — unchanged).
2. `docs/envoy-rust/STATE.md` (this file — to confirm routing).
3. `docs/envoy-rust/ROADMAP.md` (row 04 `in-progress` with `sub-phases: 04.1, 04.2, 04.3`).
4. `docs/envoy-rust/DECISIONS.md` (all landed ADRs through `ADR-0019`; ADR-0020 lands at parent-04 state-2; ADR-0021 lands at 04.2 Task 1).
5. `docs/envoy-rust/BEHAVIOR_CONTRACT.md` (the `Header allow-list` section gets populated in 04.1 per parent SPEC §2; 04.2 adds none; 04.3 adds `x-envoy-upstream-service-time`).
6. `docs/envoy-rust/SKILL_ROUTING.md` (state machine).
7. **`docs/envoy-rust/phases/04-http1/SPEC.md`** (the parent SPEC just landed — the authoritative design contract for sub-phase splits; deliverables D1.1–D12.3 mapped to sub-phases).
8. `docs/envoy-rust/phases/03-tls-tcp/SPEC.md` + the renumbering note in DECISIONS.md (phase 03 split-precedent — same parent-SPEC-projects-split pattern).
9. `docs/envoy-rust/phases/03.2-tls-upstream-sni/SPEC.md` + `REVIEW.md` (most recent sub-phase SPEC precedent; format / sectioning / signpost cadence to mirror).
10. `BOOTSTRAP_PROMPT.md` §5 state 2 + §6.1 (split gate; sub-phase SPECs each must hold the gate at SPEC writeup time) + §3 (D-3.1–D-3.9 doctrine).

## Last commit

Phase 04 state-1 brainstorm close-out commit (this commit): lands `docs/envoy-rust/phases/04-http1/SPEC.md` (parent SPEC; ~700 lines projecting the 3-way split into 04.1 / 04.2 / 04.3 + ADR-0020 + ADR-0021 + the Header allow-list edits 04.x will land in BEHAVIOR_CONTRACT.md), flips ROADMAP row `04` from `planned` to `in-progress` with `sub-phases: 04.1, 04.2, 04.3` populated, and advances STATE.md from lifecycle state 1 to state 2. No code changes; no sub-phase SPECs (those land at parent-04 state-2 per the phase-03 `f256d2c` precedent). Mirrors phase-03 state-1 commit `a3f3474` shape.

Predecessor commit:

- `ca81226` — `phase 03.2: TLS upstream origination + multi-cert SNI [parent 03 done]` — phase 03.2 phase-done commit; closed parent 03; advanced STATE to phase 04 state 1.

## Last updated

2026-04-26 (phase 04 lifecycle state advanced from 1 to 2; parent SPEC landed at this commit; ROADMAP row 04 flipped to `in-progress` with `sub-phases: 04.1, 04.2, 04.3`; next-skill `superpowers:writing-plans` adapted for the parent-04 state-2 split-output — ADR-0020 + 3 sub-phase SPECs).

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

### Doctrine reminders

- Any deviation from the state machine requires `superpowers:systematic-debugging` before proceeding — see §1 Step E of `BOOTSTRAP_PROMPT.md`.
- Consult `docs/envoy-rust/SKILL_ROUTING.md` for the full phase lifecycle state machine.
- `BOOTSTRAP_PROMPT.md` §5.1: one state per session; do not chain states. The state-2 close (landing this STATE.md edit alongside `df91b06` PLAN.md) advances phase 03.2 to lifecycle state 3. The next session enters phase 03.2 state 3 via `superpowers:subagent-driven-development`, beginning with `PLAN.md` Task 1 (envoy-config FilterChainMatch struct).
