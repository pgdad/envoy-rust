# Phase 05.4 REVIEW — Fixture-hardening follow-up: 6 root-cause fixes substantively closing phase-04.3 REVIEW C-1

- **Base:** `1d05cd0` (phase-05.1 state-6 close-out — parent of 05.4 SPEC commit `06b46a9`).
- **Head:** `a8c2364` (Task 7 state-4 phase-done verification commit).
- **Files:** 21 changed (+2510 / −69). Net Rust delta is approximately ~10 LoC envoy-config schema growth (`DnsLookupFamily` enum + 2 fields + 2 doc comments at `crates/envoy-config/src/bootstrap.rs:56-63`, `82-95`, `135-143`), ~85 LoC envoy-config parse tests (the two `parses_*` tests appended to the `tests` module), ~10 LoC envoy-http1 client behaviour change (`crates/envoy-http1/src/client.rs:94-108`) + assertion flip at lines 461-472, 7 LoC harness settle conditional (`tests/differential/src/upstream.rs:88-94`), 9 LoC across 3 echo-server helpers (bind + tracing + doc-comment), 3 LoC envoy-cluster initialiser updates at `crates/envoy-cluster/src/cluster.rs:435,477` + 1 envoy-tls initialiser update at `crates/envoy-tls/src/tests.rs:923`, 6 LoC YAML fixture diffs across 5 fixture envoy.yaml files + a 5-line `tls_inspector` block in fixture 0006, and 2 LoC fixture 0008 expectations. The headline figure is inflated by the 1397-line PLAN.md, the 277-line PROGRESS.md, the 541-line SPEC.md, and the 60-line DECISIONS.md ADR delta.
- **Reviewed:** 2026-05-03.
- **Verdict:** **Approved with M-track follow-ups** — state 5 complete. No Critical or Important findings on the 05.4 surface itself. **The C-1 carryforward chain ends here.** The state-4 phase-done gate is GREEN end-to-end: all 8 Docker-gated fixtures (0001-0008) green simultaneously in CI run `25276504502`; all 5 stable-toolchain commands clean locally and in CI; the fuzz short-budget run is clean (630154 runs / 9458 cov locally, 273161 runs on CI's nightly toolchain); `cargo deny check` is a no-op as projected; `Cargo.lock` is a no-op as projected; ADRs 0024/0025/0026 land in the projected landing-time order (ADR-0023 → 0024 → 0026 → 0025) with no renumbering. The 6 root-cause fixes are mechanically minimal, doctrinally honest (ADR-0024 explicitly bounds runtime non-consumption; ADR-0026 explicitly bounds the new parse-and-ignore pattern), and faithful to the SPEC's "re-derive per task under TDD" discipline (the backup-branch `9279895` patches were the diagnostic reference, not a merge source). Five awareness-only Minor findings track forward (DECISIONS.md line-415 ledger summary not extended; `Cluster.dns_lookup_family` runtime non-consumption has no positive parse test for V6Only/Auto; the parse-and-ignore field is structurally observable but not asserted at any current call site; one CI-only annotation about Node.js 20 deprecation; PROGRESS Task 1 / Task 5 ADR-count grep semantics conflate the template marker at `DECISIONS.md:10` with real ADRs).

---

## §1 Summary

Phase 05.4 is the dedicated follow-up sub-phase that substantively closes phase-04.3 REVIEW C-1 — the cross-phase Docker-gated `host.docker.internal`/`STATIC` regression that originated at phase-02.2's ADR-0015 landing (`435c6fa`), latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3, partially closed at 05.1 state-6 (`1d05cd0`). The 6 root-cause fixes that 05.1's STRICT_DNS preamble proved necessary but not sufficient land here under proper SPEC + ADR + per-task TDD discipline: helper bind 0.0.0.0 (D4), `dns_lookup_family: V4_ONLY` knob (D2 + D1 schema + ADR-0024), `Listener.listener_filters` parse-and-ignore + fixture 0006 `tls_inspector` block (D3 + ADR-0026), STRICT_DNS settle 500ms→2000ms for host-gateway fixtures (D6), and `envoy-http1::Client` empty-body Content-Length suppression (D5 + ADR-0025). 7 tasks across 9 commits between base `1d05cd0` and head `a8c2364`; ~250 LoC of net code change matches the SPEC §3 estimate.

The C-1 carryforward chain ends at the Task 7 verification commit `a8c2364`. CI run `25276504502` against HEAD `06c706d` (the Task 6 commit that immediately precedes the verification-only Task 7 commit) shows all 8 Docker-gated fixtures green simultaneously: 0001/0002/0007 unchanged from 05.1 (settle 500ms preserved, no host-gateway), and 0003/0004/0005/0006/0008 RESTORED from the 05.1-head red baseline (CI run `25258722850`'s "0008 RED + 4 NOT RUN" matrix is now "5 RESTORED + 3 unchanged"). The procedural defect of the 05.1 aborted attempt (the inline 6-patch expansion at `9279895` that blew Task 4's 0-LoC contract and landed code without a SPEC anchor or ADR) is corrected here, not the technical content — 05.4's brainstorm ratifies the same 6 patches under SPEC + ADR + per-task PROGRESS narration. Phase-04.1 REVIEW M-claim (the per-function `drive_http1` unit test) is **substantively unblocked** (fixture 0008 now exercises `drive_http1` end-to-end at every CI run) but stays deferred per the 04.3 disposition; the M-claim's scope is additive and doesn't fit 05.4's "regression closure" SPEC.

---

## §2 Strengths

- **The C-1 close-out is materially demonstrated, not just claimed.** PROGRESS.md Task 7 lines 246-256 carry the full per-fixture matrix from CI run `25276504502` (Run ID 25276504502, HEAD SHA `06c706d`, result SUCCESS). I cross-checked the CI run via `gh run view 25276504502 --log` and confirmed each of the 8 fixture-binary lines independently: `echo_fixture` (0001, 1.06s), `admin_ready_fixture` (0002, 6.53s), `tcp_proxy_fixture` (0003, 2.67s), `tls_downstream_fixture` (0004, 2.81s), `tls_upstream_fixture` (0005, ~2.70s — `2026-05-03T10:21:08`), `tls_sni_fixture` (0006, ~3.08s — `2026-05-03T10:21:05`), `http1_direct_response_fixture` (0007, 0.85s), `http1_router_upstream_fixture` (0008, 2.47s). All `test result: ok. 1 passed; 0 failed`. The fuzz job is also green (parse_bootstrap, 30s, 1m54s wall). 8/8 Docker-gated fixtures green at the same commit is the SPEC §1 acceptance signal verbatim.

- **6 root-cause fixes correctly re-derived from the backup branch under TDD discipline.** SPEC §6 signpost 10 explicitly forbade cherry-picking from `backup/task4-scope-creep-2026-05-02` (`9279895`); per-task PROGRESS narratives at Tasks 1/3/5 record TDD-style fail-first → impl → green-test loops with line-drift confirmations against the backup-branch diff. The technical content matches the backup; the procedural defect (no SPEC anchor, no ADRs, blew Task 4's 0-LoC contract) is corrected here.

- **ADR-0024 / ADR-0025 / ADR-0026 land in projected landing-time order with no renumbering.** `grep -n '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -5` confirms `ADR-0023 (line 424)` → `ADR-0024 (line 437)` → `ADR-0026 (line 459)` → `ADR-0025 (line 478)` — exactly the SPEC §6 signpost 9 + §7 projection. The landing-time-order vs numeric-order divergence is correctly documented in each ADR's Provenance footer (e.g., DECISIONS.md:493 `the landing-time order in DECISIONS.md is by task-execution order: ADR-0024 (Task 1) → ADR-0026 (Task 3) → ADR-0025 (Task 5). The ledger remains append-only with no renumbering.`).

- **All three ADRs are well-shaped and self-consistent.** Each has Date / Status / Context / Options-considered / Decision / Rationale / Consequences / Provenance. ADR-0024 (`DECISIONS.md:437-455`) weighs 4 alternatives including the explicit rejection of "type the field as a runtime knob and filter `lookup_host` results by family" with the doctrine cite "would land code with no test that exercises it" (D-3.6 minimalism). ADR-0025 (`DECISIONS.md:478-493`) weighs 4 alternatives including the rejected "always emit; bend the upstream Envoy fixture via `request_headers_to_add`" with the cite "fixture YAML bending around envoy-rust's misbehaviour rather than fixing envoy-rust; doesn't honor the RFC". ADR-0026 (`DECISIONS.md:459-474`) introduces the parse-and-ignore pattern as a documented envoy-config posture and explicitly bounds future extensions ("Whichever later phase first needs to ACTUALLY EXECUTE a listener filter lands a typed-variant extension on the field plus a runtime dispatch arm — not a new ADR (extending an existing pattern).").

- **`DnsLookupFamily` enum + `Cluster.dns_lookup_family` field are minimum viable scope.** `crates/envoy-config/src/bootstrap.rs:82-95` adds the 3-variant enum exactly as ADR-0024 projects (V4Only / V6Only / Auto) with the established `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]` derive — mechanically the same shape as `ClusterType` from 05.1. The struct field at `bootstrap.rs:56-63` carries a 5-line doc comment that directly cites ADR-0024 and the `D2 of phase 05.4` cross-reference. The 05.1-landed `tokio::net::lookup_host` resolution path is unchanged (verified — only `crates/envoy-cluster/src/cluster.rs:435,477` gain the mechanical `dns_lookup_family: None` initialiser update; no runtime semantics change). 1 new parse test (`parses_cluster_with_dns_lookup_family_v4_only`) verified passing locally (`test result: ok. 1 passed`).

- **`Listener.listener_filters: Vec<serde_yaml::Value>` parse-and-ignore field is correctly sized.** `crates/envoy-config/src/bootstrap.rs:135-143` adds the field with a 7-line doc comment that names the parse-and-ignore semantics, the rustls-layer-SNI-dispatch architectural choice (phase 03.2), and the explicit upstream-Envoy-only consumer. The `Vec<serde_yaml::Value>` type is correct for the criteria ADR-0026 lays out — multiple filter types possible; future Envoy versions may surface more; opaque storage preserves the ability to inspect without typing. 1 new parse test (`parses_listener_with_tls_inspector_listener_filter`) verified passing locally (`test result: ok. 1 passed`); the test asserts `listener.listener_filters.len() == 1` AND smoke-checks the opaque value contains the `tls_inspector` filter name via re-serialisation, which is the right shape per ADR-0026's "preserves the ability to inspect (e.g., a test could assert 'fixture 0006 declares the tls_inspector filter' without typing the inspector itself)".

- **Empty-body Content-Length suppression is bounded correctly.** `crates/envoy-http1/src/client.rs:94-108` composes the existing `request_has_cl` check with the new `body_is_nonempty` predicate via `&&`. The predicate uses `request.body_bytes().is_some_and(|b| !b.is_empty())` — which is the idiomatic `Option<&[u8]>` non-empty form (stable since Rust 1.70, well before the toolchain pin). Pass-through unchanged for explicit Content-Length AND for non-empty bodies (preserving existing behaviour). The 1 affected unit test `send_request_writes_serialized_request_bytes` at `crates/envoy-http1/src/client.rs:445-474` correctly flips its assertion to `!s.contains("content-length: 0\r\n")` with a 4-line ADR-0025 cross-reference comment; locally verified passing.

- **Fixture 0008 expectations.yaml drop is mechanically coupled to the client change.** `tests/fixtures/0008-http1-router-upstream/expectations.yaml:9` removes `  content-length: 0\n` from the expected echo body in the same commit (`01edie3`) as the envoy-http1 client behaviour change, per SPEC §6 signpost 11 + PLAN signpost 11. Splitting these would have left an intermediate red state.

- **Fixture 0006 `tls_inspector` block is mechanically coupled to the schema change.** `tests/fixtures/0006-tls-sni/envoy.yaml:7-10` adds the explicit `tls_inspector` listener-filter block in the same commit (`f1db1e2`) as the `Listener.listener_filters` schema growth, per SPEC §6 signpost 12. The block is added to envoy.yaml only (NOT envoy-rust.yaml) because envoy-rust performs SNI dispatch at the rustls layer (phase 03.2 architectural choice).

- **3 echo-server bind flips are mechanically uniform and behavior-neutral.** `grep -n "TcpListener::bind" tests/helpers/{tcp,tls,http1}-echo-server/src/main.rs` confirms the 3 production binds at lines 118 / 109 / 98 all flip from `("127.0.0.1", port)` to `("0.0.0.0", port)`; the 4 test-internal ephemeral binds (`"127.0.0.1:0"` at lines 212/236/281/332) are correctly NOT touched per PROGRESS Task 4 narrative. Tracing log strings + 3 doc-comment headers updated for consistency. `cargo test -p {tcp,tls,http1}-echo-server` re-verified locally (5 + 8 + 5 = 18 tests passing).

- **STRICT_DNS settle bump is conditional and bounded.** `tests/differential/src/upstream.rs:93` `let settle_ms = if host_gateway { 2000 } else { 500 };` correctly preserves the 500ms path for the 3 unaffected fixtures (0001/0002/0007 don't set `host_gateway = true` — verified at `tests/differential/src/lib.rs:989` via `let host_uses_host_gateway = upstream_yaml.contains("host.docker.internal");` derivation). The 5-line doc comment names the SPEC §3 D6 cross-reference and the 3-fixture exemption rationale. CI cost impact bounded at ~7.5s total (1.5s × 5 fixtures), as PROGRESS Task 6 records.

- **The runtime non-consumption of `Cluster.dns_lookup_family` is doctrine-correct, not a deferred bug.** ADR-0024's "Decision" + "Rationale" sections explicitly bound this: "The field is parsed-and-stored on envoy-rust's typed Cluster struct but NOT consumed at runtime in 05.4". This is D-3.6 minimalism in action — code with no test does not land. The runtime-extension trigger is named: "whichever later phase first needs envoy-rust to filter resolved addresses by family lands the runtime extension then, with its own test." The non-consumption is structurally observable (envoy-rust uses `127.0.0.1` literal IP at the substituted `{{BACKEND_HOST}}` site; DNS family selection has no runtime semantics on envoy-rust because there's no DNS to do).

- **PROGRESS.md cadence is excellent.** 277 lines for 7 tasks; every task carries Commit / Deliverables / ADR landed / Files modified / LoC / Verification / Deviations / Carryforward sections; each "Deviations from PLAN" subsection types its named items (Task 1's Step-1/Step-3 corrections; Task 3's `inline_string:` → `filename:` shape correction; Task 5's line-drift; Task 7's CI-warnings count + corpus seed projection + git-status output disclosures). PROGRESS Task 7 lines 154-227 quote `cargo build` / `clippy` / `fmt` / `test --workspace` / `deny check` / fuzz outputs verbatim with tail-quoted `Finished ...` lines and the `advisories ok, bans ok, licenses ok, sources ok` final-line gate signal.

- **Local stable-toolchain gate is GREEN end-to-end (re-verified at HEAD `a8c2364`):** `cargo build --workspace --all-targets` clean (`Finished dev profile target(s) in 0.09s`); `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (no warnings); `cargo fmt --all -- --check` clean (no output); `cargo test --workspace` clean (workspace aggregate 339 passed; 0 failed; 1 ignored, the 1 ignored is `differential::upstream::tests::starts_upstream_envoy_and_exposes_host_port` which is Docker-gated and runs in CI per PLAN signpost K); `cargo deny check` clean with `advisories ok, bans ok, licenses ok, sources ok`; `git diff Cargo.lock` empty (no transitive surface change). The 2 new parse tests + the 1 flipped client test all pass: `parses_cluster_with_dns_lookup_family_v4_only` ok; `parses_listener_with_tls_inspector_listener_filter` ok; `send_request_writes_serialized_request_bytes` ok.

- **Append-only ADR ledger discipline preserved exactly.** `git diff 1d05cd0..a8c2364 -- docs/envoy-rust/DECISIONS.md` shows +60 lines: ADR-0024 (~13 lines) + ADR-0026 (~13 lines) + ADR-0025 (~13 lines) + 3 separator-blocks. ADR-0001 through ADR-0023 are byte-identical (D-3.5 append-only). No retroactive edits to landed historical artifacts.

- **Architectural rules from parent-05 SPEC §3 honored.** All 7 inherited rules (sole h2 dep / HCM-on-H2 reuse / `:authority` mapping / hop-by-hop strip / no H2 schema edits / `codec_type: AUTO` behavior / `http` as transitive only) remain trivially satisfied — 05.4 introduces no H2 surface, no new crate, no new top-level dep. Cargo.lock no-op as projected by SPEC §6 signpost 2 + PLAN signpost I.

- **Commit-message hygiene matches the established 04.x / 05.1 precedent.** Substantive landings carry `phase 05.4: <feature> [+ <ADR-NNNN>] (task N)`; the verification commit `a8c2364` carries `phase 05.4: state-4 phase-done gate verification (task 7)` — mirrors 05.1's `b7fe910` shape exactly. The Task 7 commit message names the C-1 closure in the body.

---

## §3 Issues

### Critical

None.

### Important

None on the 05.4 surface itself. The C-1 carryforward chain ends here as projected. Nothing in the 6 root-cause fixes blocks state-6 close-out.

### Minor

**M-1. The `DECISIONS.md:415` parent-05 sub-phase row in the ledger summary is not extended to 05.4.** *(Awareness-only; documentation cosmetic.)*

The DECISIONS.md ledger summary at the parent-05 row (line 415, the row that enumerates parent-05's landed sub-phase ADRs) was written at the parent-05 state-2 commit `f1804a7` and projected `ADR-0023 (05.1 Task 1)` only; the row text doesn't reflect that 05.4 has now landed three additional ADRs (0024, 0025, 0026). The omission is consistent with how 05.1 left the ledger (the 05.1 phase-done commit didn't touch this summary either), and editing it would be a retroactive edit per D-3.5. Forward observation only — the next parent-05 state-6 commit (after 05.3 closes) is the natural place to consolidate the summary.

**Disposition:** awareness-only. Carry forward to parent-05 state-6 as a documentation-cleanup task; no action needed in 05.4.

**M-2. `Cluster.dns_lookup_family` has no positive parse test for `V6Only` or `Auto`.** *(Awareness-only; planner discretion.)*

`crates/envoy-config/src/bootstrap.rs::tests::parses_cluster_with_dns_lookup_family_v4_only` exercises the V4_ONLY parse path only; the V6_ONLY and AUTO variants are accepted by the SCREAMING_SNAKE_CASE serde derive (mechanical) but no in-tree test asserts the round-trip. ADR-0024 explicitly chose to accept the full v1.33 surface upfront (rejecting the "V4_ONLY-only" alternative (iii) on brittleness grounds), but the test suite only covers the V4_Only case. SPEC §3 D1 said "1 minimum, per the backup-branch precedent; planner may add more if scope warrants" — the planner elected the minimum. Whichever later phase first needs to test V6_Only/Auto behavior will likely add the parse coverage there.

**Disposition:** awareness-only. Track forward to whichever phase first uses V6_Only / Auto in a fixture YAML or runtime path. No action needed in 05.4.

**M-3. The parse-and-ignore field is structurally observable but no current call site asserts on it.** *(Awareness-only; SPEC §6 signpost 4's open question.)*

SPEC §6 signpost 4 documented an OPEN question: which test path actually parses `envoy.yaml` through envoy-config? The planner verified the answer at PLAN-write time (none currently — envoy-rust's binary parses `envoy-rust.yaml` only; the differential harness doesn't parse envoy.yaml through envoy-config; the most plausible consumer is the fuzz corpus walk). The 05.4 schema additions are defensive — `Listener.listener_filters` is parsed-and-stored on every parse path, but no current path actually parses fixture 0006's envoy.yaml through envoy-config. The 1 parse test (`parses_listener_with_tls_inspector_listener_filter`) is the only consumer. This is doctrine-correct per ADR-0026 ("defensive acceptance is doctrinally cleaner than perpetual field-set divergence") but means the field's presence is currently exercised only by its own test.

**Disposition:** awareness-only. Track forward to whichever phase first introduces a test path that parses envoy.yaml through envoy-config (most plausible: a future fuzz seed exercising listener_filters). No action needed in 05.4 — the SPEC explicitly defers fuzz seed extension per signpost (d) + PLAN signpost H.

**M-4. CI annotation: `actions/checkout@v4` Node.js 20 deprecation.** *(Awareness-only; pre-existing across 05.x baseline.)*

CI run `25276504502` carries 1 advisory annotation: "Node.js 20 actions are deprecated. The following actions are running on Node.js 20 and may not work as expected: actions/checkout@v4." This is a GitHub Actions runner deprecation notice (not a 05.4 regression); the same warning was present on the 05.1 baseline. The hard cutover is June 2nd, 2026 (per the GitHub announcement); a future hardening pass should bump `actions/checkout` to v5 or whichever post-Node20 version exists by then.

**Disposition:** awareness-only. Carry forward to a future workflow-maintenance pass before June 2nd, 2026. No action in 05.4.

**M-5. PROGRESS Task 1 / Task 5 ADR-count grep semantics conflate the template marker at `DECISIONS.md:10` with real ADRs.** *(Awareness-only; cosmetic.)*

PROGRESS Task 1 (line 26) discloses: "the `^## ADR-` count returned `24`, not the projected `23`, because the count includes the `## ADR-NNNN: <title>` template marker at line 10 of DECISIONS.md (not a real ADR)." Task 5 (line 120) similarly notes the off-by-one: "`grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md` = `27` (was 26 pre-task; matches plan's controller-calibration projection of 27 = 24 active ADRs + the template-marker line at line 10 + ADR-0024 + ADR-0026 + ADR-0025 = 27 sectioned headings". The executor narrated the off-by-one correctly each time, so the gate semantics held — but the grep regex itself does include the template marker. A more precise regex (e.g., `'^## ADR-[0-9]\{4\}:'`) would avoid the template-marker false-positive without forcing the executor to mentally subtract 1. Pre-existing across the 04.x / 05.1 baseline; not a 05.4 regression.

**Disposition:** awareness-only. Track forward as a future doctrine-tooling cleanup. No action in 05.4.

---

## §4 Recommendations

- **R-1 (forward-track for 05.2 PLAN-writing session).** STATE.md's "Phase-05.4 rollovers" subsection at the 05.4 state-6 commit must record (per SPEC §6 signpost 17): "Phase-04.3 REVIEW C-1 — closed at this commit's CI run; the C-1 carryforward chain (originating at phase-02.2's ADR-0015 landing `435c6fa`, latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3, partially closed at the 05.1 state-6 commit) ends here." AND: "Phase-04.1 REVIEW M-claim — substantively unblocked by the 05.4 fix's restoration of fixture 0008's end-to-end exercise, but stays deferred per the 04.3 disposition." The Important-track follow-ups list shrinks by C-1.

- **R-2 (forward-track for 05.2/05.3 fixture authoring).** The new `dns_lookup_family: V4_ONLY` knob is now the recommended posture for any future fixture using `STRICT_DNS` cluster type with `host.docker.internal` on macOS Docker — fixture 0010 (the H2-router-upstream fixture in 05.3) will likely need it. Document this in 05.3's SPEC at brainstorm time.

- **R-3 (forward-track for whichever phase first needs listener-filter execution).** ADR-0026 explicitly bounds the parse-and-ignore pattern and names the trigger for a typed-variant extension: "Whichever later phase first needs to ACTUALLY EXECUTE a listener filter lands a typed-variant extension on the field plus a runtime dispatch arm — not a new ADR (extending an existing pattern)." If a future phase (e.g., adding HTTP/2 or proxy-protocol surface) needs `original_dst` / `proxy_protocol` listener filters, the typed extension lands there.

- **R-4 (forward-track for 05.2's H2 codec or 05.3's H2 client).** The `body_is_nonempty` predicate in `envoy-http1::Client` is a natural template for the analogous H2 client codec emission decision (HEADERS frame on empty-body GET should also omit `content-length: 0`). 05.3's brainstorm should reference ADR-0025 + the predicate shape.

- **R-5 (defer to a future hardening pass).** Settle-time tightening (D6's 2000ms ceiling) is recommended NOT to tighten in 05.4 per SPEC §6 signpost 16 + PLAN signpost L; the empirical 2000ms is green; tightening to e.g. 1000ms would require additional CI runs to validate and risks reintroducing flake. Defer to a future hardening pass.

- **R-6 (low-priority maintenance).** The `^## ADR-NNNN: <title>` template marker at `DECISIONS.md:10` causes the controller-calibration off-by-one in PROGRESS Task 1 / Task 5 narratives (M-5 above). A future doctrine-tooling pass could either (a) tighten the grep regex to `'^## ADR-[0-9]\{4\}:'` in BOOTSTRAP_PROMPT.md / SKILL_ROUTING.md, or (b) drop the template marker entirely (the same shape exists in the SPEC.md / PLAN.md templates).

---

## §5 Carryforward verdict

| Item | Origin | Status before 05.4 | Status at 05.4 close | Disposition |
|---|---|---|---|---|
| C-1 (Docker-gated `host.docker.internal`/STATIC regression) | phase-02.2 ADR-0015 (`435c6fa`); surfaced at phase-04.3 task 14 (`eb6f972`) | Partially closed at 05.1 state-6 (`1d05cd0`) — schema + runtime + YAML preamble landed; fixture 0008 + 0003/0004/0005/0006 still RED-or-NOT-RUN | **CLOSED** at 05.4 Task 7 verification commit `a8c2364`; CI run `25276504502` shows 8/8 fixtures GREEN simultaneously | Closed; STATE.md "Phase-05.4 rollovers" records closure |
| Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) | phase-04.1 REVIEW | Deferred per 04.3 disposition; masked by C-1 on fixture 0008 | **UNBLOCKED** but **NOT closed** — fixture 0008 now exercises drive_http1 end-to-end at every CI run, but the M-claim's additive-test scope is not consumed in 05.4 | Continues forward unchanged; track to whichever later phase first adds a third Driver::Http1 consumer |
| Phase-04.1 REVIEW M1 / M2 / M4 / M5 / M7 (general M-track carryforwards from 04.1) | phase-04.1 REVIEW | Deferred to phase-05+ / hardening | Unchanged in 05.4 (no surface engaged) | Continues forward unchanged |
| Phase-04.2 REVIEW M-track residuals (post-04.3 closures of M3/M6/M10/#12) | phase-04.2 REVIEW | Already closed in 04.3 | N/A in 05.4 | N/A |
| Phase-05.1 REVIEW M-track findings (A1-A6) | phase-05.1 REVIEW (`283a4b9`) | Awareness-only | Unchanged in 05.4 (no surface engaged) | Continues forward unchanged |
| ADR-0023 numbering provenance (parent-05 split projected ADR-0024/0025 conditionally) | parent-05 SPEC §7 | Conditional projection | **Resolved**: ADR-0024 / 0025 / 0026 land at 05.4 in the SPEC §7-projected order with no renumbering | Closed |

**Summary:**
- **Items closed in 05.4:** **C-1** (the cross-phase Docker-gated regression — substantively closed at the Task 7 verification commit; CI run `25276504502` is the load-bearing evidence). The conditional ADR-0024/0025 projection from parent-05 SPEC §7 is also resolved (numbers landed as projected; no renumbering needed).
- **Items partially closed:** None expected; 05.4 is the substantive C-1 close.
- **Items unblocked but not closed:** Phase-04.1 M-claim (drive_http1 per-function unit test). The fixture-mask removal substantively unblocks the test surface, but the M-claim's own scope (an additive in-isolation test) is not consumed in 05.4.
- **Items continuing forward unchanged:** Phase-04.1 M1/M2/M4/M5/M7; phase-05.1 A1-A6; the 5 Minor findings recorded in §3 above (M-1 through M-5).

---

## §6 Verification gate observation

**Did Task 7's state-4 evidence in PROGRESS.md actually demonstrate the SPEC §1 acceptance signal?** Yes, on every dimension I checked.

- **All 8 fixtures green simultaneously in CI** — confirmed via independent `gh run view 25276504502 --log` retrieval. Per-fixture binary-result grep returns `test result: ok. 1 passed; 0 failed` for each of `echo_fixture` / `admin_ready_fixture` / `tcp_proxy_fixture` / `tls_downstream_fixture` / `tls_upstream_fixture` / `tls_sni_fixture` / `http1_direct_response_fixture` / `http1_router_upstream_fixture`. PROGRESS Task 7 lines 246-256's per-fixture matrix matches the CI logs exactly. The "RESTORED" annotation on the 5 affected fixtures correctly identifies which root-cause fixes touched each (Task 2 + Task 4 + Task 6 across 0003/0004/0005; same trio + Task 3 for 0006; same trio + Task 5 for 0008).
- **`cargo build/clippy/fmt/test/deny/fuzz` all green locally + in CI** — confirmed at HEAD `a8c2364` locally:
  - `cargo build --workspace --all-targets`: `Finished dev profile target(s) in 0.09s`.
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`: no warnings, clean exit.
  - `cargo fmt --all -- --check`: no output (clean).
  - `cargo test --workspace`: 339 passed; 0 failed; 1 ignored (the 1 ignored is `differential::upstream::tests::starts_upstream_envoy_and_exposes_host_port` per PLAN signpost K — Docker-gated, runs in CI).
  - `cargo deny check`: `advisories ok, bans ok, licenses ok, sources ok`. Same 4 `license-not-encountered` warnings as the 05.x baseline (BSD-2-Clause / MPL-2.0 / Unicode-DFS-2016 / Zlib at deny.toml:40/47/43/45) — pre-existing, not gated by `-D warnings` semantics. Final-line gate signal is the pass.
  - `git diff Cargo.lock`: empty (no-op as projected per SPEC §6 signpost 2 + PLAN signpost I).
  - The CI fuzz job (parse_bootstrap, 30s) ran 273161 runs / 7394 cov / 16280 ft / corp 1435 in 31 seconds; no crash. Schema additions (`Cluster.dns_lookup_family` Option + `Listener.listener_filters` Vec, both `#[serde(default)]`) parse cleanly through the existing 12-seed corpus + libFuzzer's mutation surface. No new fuzz seed in 05.4 per SPEC §1(d) + PLAN signpost H.
- **ADR ledger: 3 ADRs landed in landing-time order; numbering correct.** Confirmed via `grep -n '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -5`: lines 408 (ADR-0022), 424 (ADR-0023), 437 (ADR-0024), 459 (ADR-0026), 478 (ADR-0025). Landing-time order ADR-0023 → 0024 → 0026 → 0025 matches SPEC §6 signpost 9 + §7 verbatim. Each ADR's Provenance footer correctly names the conditional projection from 05.1 STATE.md and the next-sequential-no-renumbering disposition.

The two acceptance-signal sub-conditions (a) and (b) from SPEC §1 are both met:
- **(a) all 5 affected Docker-gated fixtures restored to green simultaneously** — `tcp_proxy_fixture` / `tls_downstream_fixture` / `tls_upstream_fixture` / `tls_sni_fixture` / `http1_router_upstream_fixture` all GREEN in CI run `25276504502`.
- **(b) all 3 unaffected fixtures remain green** — `echo_fixture` / `admin_ready_fixture` / `http1_direct_response_fixture` all GREEN in the same CI run; PROGRESS Task 7 line 248-249, 252 explicitly notes "unchanged from 05.1; no host_gateway, settle 500ms" / "unchanged from earlier phases" / "unchanged; no host_gateway, settle 500ms" (the harness settle-time bump correctly DOES NOT engage these because their upstream YAMLs don't contain `host.docker.internal` per the `host_gateway = upstream_yaml.contains("host.docker.internal")` derivation at `tests/differential/src/lib.rs:989`).

(c) is N/A (no conformance suites in 05.4; h2spec attaches in 05.2). (d) is GREEN (fuzz). (e) is GREEN (5 stable-toolchain commands). (f) is what this REVIEW is producing.

The state-4 phase-done gate is met cleanly.

---

## §7 Final verdict + reasoning

**Approved with M-track follow-ups.**

The 6 root-cause fixes substantively close phase-04.3 REVIEW C-1 at CI run `25276504502` with all 8 Docker-gated fixtures green simultaneously; ADRs 0024/0025/0026 land in landing-time order with no renumbering and explicitly bounded scope; doctrine conformance is end-to-end (D-3.1 TDD per task, D-3.2 zero new top-level deps, D-3.4 cold-readable PROGRESS, D-3.5 append-only ADRs, D-3.6 minimalism preserved via the deliberate runtime non-consumption posture, D-3.7 / D-3.9 frozen pins untouched, D-3.8 unsafe-code forbid maintained); 5 awareness-only Minor findings carry forward without blocking close-out.
