# Phase 02.1 REVIEW — Config schema + cluster manager + echo-server helper

- **Base:** `aef36ce` (phase-01 done commit)
- **Head (initial review):** `95a26a7` (phase 02.1 state-4 follow-up; state-5 input)
- **Head (after I1 close-out):** `dea4d16` (Cargo.lock sync)
- **Files:** 20 changed (+6628 / -108) in the initial review range; +1 file (+19 lines) in the I1 close-out
- **Reviewed:** 2026-04-24 (initial); 2026-04-24 (I1 close-out per §7)
- **Verdict:** **Approved** — state 5 complete. I1 and I2 closed in-phase (see §7); I3 and M1–M4 tracked forward per §4.

---

## 1. Summary

Phase 02.1 lands the envoy-config schema tree that Envoy's `envoy.filters.network.tcp_proxy` + `STATIC` cluster + `ROUND_ROBIN` LB grammar requires; ships the new `envoy-cluster` library crate (round-robin endpoint picker + `ClusterManager::from_bootstrap`); ships the `tcp-echo-server` helper binary that 02.2's fixture 0003 will dial; closes phase-01 REVIEW §9 rollover I3; and appends ADR-0014 (YAML-native `typed_config`) to the ledger. No differential fixture ships this sub-phase; parent phase 02 stays `in-progress` via the ADR-0013 split, and 02.2 closes row 02.

The work reads cleanly against doctrine on every axis I checked. D-3.2 permitted foundations are respected — `envoy-cluster`'s only non-std deps are `envoy-config` (path) and `thiserror`; `tcp-echo-server`'s deps are `anyhow` (binary crate, permitted), `thiserror`, `tokio`, `tracing`, `tracing-subscriber` — all on the `BOOTSTRAP_PROMPT.md` §3 D-3.2 list. D-3.5 append-only ADRs hold: `DECISIONS.md` shows only two additions (ADR-0013, ADR-0014), no edits to ADR-0001–0012. D-3.8 `#![forbid(unsafe_code)]` is at both new crate roots (`envoy-cluster/src/lib.rs:1`, `tests/helpers/tcp-echo-server/src/main.rs:1`). D-3.9 toolchain pin is untouched (`rust-toolchain.toml` still `channel = "1.95.0"`, not in diff).

SPEC §Deliverables D1–D7 all land in the described shape. The `envoy-cluster` public surface is a verbatim match for SPEC §D1's contract (same signatures, same error-variant shapes, same defense-in-depth rationale). The envoy-config typed_config envelope (`TypedConfig` tagged-enum with single `TcpProxy` variant, `TcpProxyConfig { stat_prefix, cluster }`) is exactly what ADR-0014 specifies and what the `#[serde(tag = "@type", deny_unknown_fields)]` idiom produces. The five new `ConfigError` variants match SPEC §D2 name-for-name. The tcp-echo-server runtime preserves the `envoy-bin` argv/tracing/exit-code convention. The four `decode_chunked` unit tests (phase-01 I3) sit at `tests/differential/src/lib.rs:769–799` in the `drive_http_get_*` adjacency PROGRESS Task 11 flags. The two fuzz seeds match SPEC §D5's literal contents. ADR-0014 is well-formed (header, status, context, three options with (ii) and (iii) rejected, decision, rationale, consequences, provenance), and the provenance note correctly cross-references the ADR-0013 renumbering.

Gate evidence is solid. I ran `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` locally on HEAD `95a26a7`; both exit 0 on first attempt. PROGRESS.md §"State 4" reports CI run `24909836488` green with both jobs (`build + test + lint` 1m20s, `fuzz (parse_bootstrap, 30s)` 1m02s) and notes "no fix-during-gate commits were needed" — a positive signal vs. phase-01's two state-4 fix rounds. The state-4 follow-up commit `95a26a7` correctly replaced a misidentified `#[ignore]`d test reference: `upstream::tests::starts_upstream_envoy_and_exposes_host_port` is indeed the only `#[ignore]` in the differential tree (verified at `tests/differential/src/upstream.rs:94`), and its attribute text + ADR-0005 rationale quoted in the PROGRESS.md update are both accurate.

The executor consistently self-audited deviations: PROGRESS tasks 2 (cluster-block YAML simplification to unblock parse-layer tests until Task 3 lands the real shape), 9 (`#[allow(dead_code)]` annotations required by clippy for test-only items pre-wiring in Task 10), 10 (two `tokio` feature flags `"time"`/`"sync"` needed by the plan's test shape), 11 (adjacency vs. literal "append after final test" placement for the I3 tests), and 12 (`fuzz/.gitignore` exception lines for the two new seeds, not in PLAN.md but mechanically required by `git add`). Each deviation is self-logged with rationale. I credit the self-audit — none of these reads as undisclosed drift.

Three Important items surfaced at initial review. (1) the working-tree `Cargo.lock` drift for the two new workspace members is known and was queued for a dedicated lockfile-sync commit before state 6, mirroring phase-01 precedent `4955252`; landed at `dea4d16` — see §7. (2) `docs/envoy-rust/STATE.md` read "state 3 (PLAN.md exists, implementation incomplete)" — stale vs. the current reality (state 4 complete; state 5 complete with this REVIEW); advanced in the same commit that lands this REVIEW.md — see §7. (3) A modest test-coverage gap on the validator — see §3 Important I3, tracked forward.

---

## 2. Strengths

- **Doctrine conformance end-to-end.** `#![forbid(unsafe_code)]` at both new crate roots; every new dep on D-3.2 list; append-only ADR ledger preserved (verified via `git diff aef36ce..95a26a7 -- docs/envoy-rust/DECISIONS.md` — only ADR-0013 and ADR-0014 appended, ADR-0001–0012 byte-identical); root-toolchain pin untouched. `/Users/esa/git/envoy-rust/crates/envoy-cluster/src/lib.rs:1`, `/Users/esa/git/envoy-rust/tests/helpers/tcp-echo-server/src/main.rs:1`.

- **ADR-0014 is shape-correct and matches the code.** The `TypedConfig` enum at `/Users/esa/git/envoy-rust/crates/envoy-config/src/bootstrap.rs:129–134` uses `#[serde(tag = "@type", deny_unknown_fields)]` with one `TcpProxy(TcpProxyConfig)` variant renamed to the full `type.googleapis.com/...` URL literal — exactly what ADR-0014 decision (i) specifies, with the stranger-readability property the rationale promises. ADR-0014 carries options considered (with (ii) `prost` and (iii) `raw_config` both rejected with rationale), decision, rationale, consequences, and a provenance note documenting the ADR-0013 renumbering.

- **`envoy-cluster` round-robin picker is textbook correct.** `/Users/esa/git/envoy-rust/crates/envoy-cluster/src/cluster.rs:23–30` uses `fetch_add(1, Ordering::Relaxed)` + `% endpoints.len()` — correct for lock-free rotation because no observer needs a happens-before relationship with the cursor value (SPEC §6 signpost 3). The empty-endpoint check at line 24 is defense-in-depth vs. the `from_bootstrap` guard at lines 122–126; the nuance is documented in the `ClusterError` doc comment at lines 68–79. Test coverage is thorough: `pick_endpoint_cycles_over_three_endpoints` asserts the exact sequence; `pick_endpoint_is_stable_under_concurrent_calls` spawns 1000 `std::thread` workers and verifies ±10% distribution; `handle_clone_shares_cursor` proves `Arc` semantics by interleaving picks across a clone (`cluster.rs:164–239`).

- **`ClusterManager::from_bootstrap` error discipline.** `HashMap::insert` returning `Some(_)` as the duplicate-key detector is idiomatic (`cluster.rs:132–136`); `.parse::<SocketAddr>()` wraps failures in `EndpointParse` with `#[source]` preserving the underlying `AddrParseError` (`cluster.rs:86–92`, `cluster.rs:114–118`); `EmptyCluster` guard is at the right layer (`cluster.rs:122–126`). The by-hand `Bootstrap` construction in `from_bootstrap_rejects_empty_cluster` and `from_bootstrap_rejects_duplicate_cluster_name` (`cluster.rs:322–409`) correctly bypasses the `envoy-config` validator to exercise the cluster-crate's invariants in isolation — proper defense-in-depth testing.

- **`tcp-echo-server` runtime separation is clean for testability.** The `run_on(listener, shutdown)` split at `/Users/esa/git/envoy-rust/tests/helpers/tcp-echo-server/src/main.rs:69–114` lets tests inject an ephemeral-port listener + oneshot shutdown; `run(port)` at lines 117–124 is the real-life wiring with ctrl_c. The 5-second `DRAIN_BUDGET` via `tokio::time::timeout` + `JoinSet::abort_all` on expiry mirrors the `envoy-bin::admin::serve` / `echo::serve` phase-01 convention. Exit-code discipline in `main()` at lines 127–161 follows the phase-01 three-way split (0 clean, 1 runtime, 2 argv).

- **Fuzz corpus regression backstop.** `fuzz_corpus_tcp_proxy_seeds_parse` at `/Users/esa/git/envoy-rust/crates/envoy-config/src/bootstrap.rs:1067–1079` reads both new seeds via `CARGO_MANIFEST_DIR` and drives `parse_bootstrap`, pinning their validity to `cargo test` — so a future schema change that breaks the seeds fails under `cargo test` before it fails under the 30-second fuzz CI job. This is a stronger gate than either would be alone.

- **SPEC §D2's coverage bullet was exceeded.** SPEC calls for sixteen new validator tests with `deny_unknown_fields` regressions on six struct levels; the implementation lands seventeen (16 from the SPEC list + the `fuzz_corpus_tcp_proxy_seeds_parse` backstop) and adds `rejects_unknown_endpoint_field` at `/Users/esa/git/envoy-rust/crates/envoy-config/src/bootstrap.rs:1041–1065` — closing the one struct level (`Endpoint`) the SPEC's sixteen-test tally would have left uncovered if the `deny_unknown_fields` coverage were read strictly per level. The `Endpoint` struct carries `#[serde(deny_unknown_fields)]` at `bootstrap.rs:88`, and this test confirms it's not vestigial.

- **Self-audited deviations.** Five PROGRESS tasks (2, 9, 10, 11, 12) each name a deviation with rationale. I cross-checked every one against the code and none reads as undisclosed drift. Task 9's `#[allow(dead_code)]` scaffolding annotations were all removed in Task 10 (`grep '#\[allow(dead_code)\]' tests/helpers/tcp-echo-server/src/main.rs` → 0 matches, per PROGRESS self-review). Task 12's `.gitignore` exception lines are at `crates/envoy-config/fuzz/.gitignore` and correctly mirror the `minimal.yaml` pattern.

- **Commit-message hygiene matches phase-01 precedent.** ADR-tagged commit `ebaa712` ("envoy-config typed_config envelope [ADR-0014]") follows the phase-01 `[ADR-NNNN]` pattern. One commit per task + paired "progress note" commits is the same cadence as phase-01. Commit `95a26a7` (state-4 follow-up) is a clean new commit, not an amend, per the Git Safety Protocol.

- **CI gate cleared on first attempt.** PROGRESS.md §"State 4" claims run `24909836488` both jobs green with no fix-during-gate commits. Local spot-checks of `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` on HEAD `95a26a7` both exit 0, consistent with the claim. This is a material improvement over phase-01's state-4, which required two fix rounds before green.

---

## 3. Issues

### Critical

None.

### Important

**I1. `Cargo.lock` drift must land before the state-6 phase-done commit.** *(Closed in-phase — see §7.)*

The dev-host working tree at HEAD `95a26a7` carried uncommitted additions to `Cargo.lock` for the two new workspace crates (`envoy-cluster` and `tcp-echo-server`). PROGRESS.md §"State 4 / Notes" explicitly queued a dedicated lockfile-sync commit before state-6, mirroring phase-01 precedent `4955252` ("phase 01: sync Cargo.lock with phase-01 dep graph"). Review credit given for the self-caught queue.

*Why it matters:* the CI gate regenerates `Cargo.lock` from scratch per invocation, so state-4 gate greenness is not harmed by the drift. However, shipping state 6 (the phase-done commit) without the lockfile synced means the next checkout on any dev machine re-emits the drift immediately, and the phase-done tree is not reproducible from a clean `cargo build`. Phase-01 precedent says the fix is a dedicated commit that stages only `Cargo.lock`.

*Fix:* landed at commit `dea4d16` before this REVIEW.md lands. See §7.

**I2. `STATE.md` is stale by two lifecycle steps; must be in sync by state 6.** *(Closed in-phase — see §7.)*

`docs/envoy-rust/STATE.md:10–13` at HEAD `95a26a7` read:

```
status: phase 02.1 lifecycle state 3 (PLAN.md exists, implementation incomplete)
```

This was the state 2→3 snapshot from commit `14e4291`. All 13 PLAN.md tasks are implementation-complete; the state-4 gate cleared; this REVIEW is the state-5 input. The phase-01 precedent (log of `docs/envoy-rust/STATE.md` — commits `a0934d7`, `793596d`, `33665f0`, `f436c29`, `aef36ce`) shows explicit STATE-advance commits at every lifecycle transition.

*Why it matters:* STATE.md is the single-source-of-truth for "what next" per its own top-of-file docstring. A stranger cold-starting at HEAD `95a26a7` and reading STATE.md would route to `superpowers:subagent-driven-development` for state-3 execution — which is already complete. This is not a D-3.5 doctrine violation (BOOTSTRAP_PROMPT.md §5.1 is "one state per session," not "one STATE.md commit per state"), but it is a material readability regression vs. phase-01 precedent.

*Fix:* State 5 closes with a commit that (a) lands this REVIEW.md and (b) advances STATE.md to state 5 (approved; implementation frozen; state-6 next). State 6 then lands the ROADMAP flip + STATE advance to state 6 (or equivalent, per the state-machine convention — see phase-01 `aef36ce` / `f436c29` for the pattern). No nested "catch-up state-3→4" commit is required; the forward path sweeps through both pending transitions in one sequence. Closed by this REVIEW.md commit — see §7.

**I3. No test covers `Cluster` construction where `cluster_type == Static` carries an address other than the serde-rename path.** *(Tracked forward.)*

`Cluster` at `bootstrap.rs:48–54` carries `cluster_type: ClusterType` renamed from YAML `type` via `#[serde(rename = "type")]`. `ClusterType` itself is a single-variant `SCREAMING_SNAKE_CASE` enum. SPEC §D2's test list includes `rejects_cluster_type_logical_dns` (lands at `bootstrap.rs:711–735`) but not a positive test asserting a bare `type: STATIC` deserializes to `ClusterType::Static` in isolation.

*Why it matters:* the positive path *is* exercised transitively — every other tcp_proxy/cluster test in the file consumes `type: STATIC` — so this is not a functional gap, and it's arguably noise in a 37-test file. Flagging it here because a future reader extending this enum in phase 04+ will want a direct `match cluster_type { Static => … }` assertion to regression-guard the variant name when adding `LogicalDns` / `StrictDns` variants later.

*Fix (optional):* one-liner test `parses_cluster_type_static` asserting `matches!(c.cluster_type, ClusterType::Static)` on a minimal cluster YAML. Not a state-6 blocker — tracked forward to whichever sub-phase extends `ClusterType` (likely phase 04 or later, outside row 02's scope).

### Minor

**M1. `Cluster.name` field-level `#[allow(dead_code)]` is justified but crate-internal read would eliminate it.** *(Tracked forward to 02.2.)*

`/Users/esa/git/envoy-rust/crates/envoy-cluster/src/cluster.rs:13–14` carries `#[allow(dead_code)]` on `pub(crate) name: String`. PROGRESS Task 7 justifies: "the HashMap key carries the lookup identity." This is accurate — `ClusterManager::get` looks up by `&str` against the `HashMap<String, Arc<Cluster>>` key, never through `cluster.name`. The `Clone` / write path at `from_bootstrap` lines 127–131 writes the same `cfg.name.clone()` that became the map key.

*Why it matters:* phase-02.2's `envoy-tcp` will need `cluster.name()` for tracing span attribution (`envoy-bin` span-wrapping convention), and phase-06's `/stats` admin endpoint will need it for stat-name attribution. Adding a zero-cost `pub(crate) fn name(&self) -> &str { &self.name }` accessor now would eliminate the `#[allow(dead_code)]` annotation without growing public API surface — but doing so is speculative API surface-carving which SPEC §6 signpost 6 explicitly discourages before the consumer crate exists.

*Fix:* leave as-is for 02.1. 02.2 should revisit and either add the accessor (and remove the allow) or leave the allow in place with a comment referencing the first consumer site.

**M2. `echoes_round_trip` test has a drop-before-send ordering that could race if `JoinSet::spawn` hasn't scheduled by the time shutdown fires.**

`tests/helpers/tcp-echo-server/src/main.rs:210–232` writes 32 bytes, reads 32 bytes, drops the client, then sends shutdown. On a multi-thread runtime this is fine because `write_all + read_exact` provides a happens-before to the server's `tokio::io::copy`. But the test is structurally fragile against a future refactor that changes the server-side spawn to detached or re-orders `listener.accept` vs. task spawning. Consider asserting with an explicit join-point or a longer shutdown timeout.

*Fix:* leave as-is. The test is not load-bearing (it's a helper crate validation) and has passed on CI. Flagging for awareness only.

**M3. `decode_chunked_truncated_size_line` accepts either `"missing CRLF"` or `"CRLF"` substring, but the actual error uses `missing CRLF`.**

`tests/differential/src/lib.rs:788–791` uses `msg.contains("missing CRLF") || msg.contains("CRLF")`. The second disjunct is dead (every possible CRLF-related error in `decode_chunked` emits "missing CRLF"). Not a correctness issue — tests pass — but the `|| msg.contains("CRLF")` is a foot-gun: if a future refactor changes the error to say "invalid hex" instead of "missing CRLF", this test would silently still pass because "invalid hex".contains("CRLF") is false but the test author might not realize the disjunct is semantically vacuous.

*Fix:* drop the `|| msg.contains("CRLF")` disjunct. Safe to apply before state 6 as a one-line edit, or tracked forward to 02.2's harness touches.

**M4. `ClusterManager::get` always clones the `Arc` even when the cluster is unknown.**

`cluster.rs:61–65` uses `self.clusters.get(name).map(|arc| ClusterHandle { inner: Arc::clone(arc) })`. This is correct but the `Arc::clone` is unnecessary on the `None` path — `.map` already guards it. Modern clippy doesn't flag this, and it's a no-op at machine-code level for the `None` branch. Flagging for style only; no fix needed.

---

## 4. Recommendations

**Forward to 02.2:**

1. **Add `Cluster::name()` accessor** at the same time `envoy-tcp::handle` first reaches for it, and remove `#[allow(dead_code)]` in the same commit. This dissolves M1.

2. **ADR-0015 and ADR-0016 will renumber twice if another ADR lands between 02.1 done and 02.2 start.** Currently 02.2 projects ADR-0015 (host-docker + host-gateway) and ADR-0016 (`enable_half_close: false`). If any doctrine-delta lands in the interim (none currently expected, but a cargo-deny trigger from a new transitive surface is always possible per SPEC §6 signpost 9), these numbers shift. 02.2's SPEC should treat its ADR numbers as provisional and resolve to the actual next-sequential values at task 1.

3. **I4 (admin 8 KiB cap tightening) and M1 (stale TODO retarget)** both land in 02.2. 02.1 did not regress either path — admin `/ready` behavior and the subject-subprocess TODO comment are both untouched in this sub-phase's diff.

4. **Differential harness `TcpProxyBackend`** is 02.2's big lift. The `tcp-echo-server` helper crate is ready as-is; 02.2 shells out to the `CARGO_BIN_EXE_tcp-echo-server` path per SPEC §D6's note (or the `target/<profile>/tcp-echo-server` fallback, since cross-package `CARGO_BIN_EXE_*` is unavailable per Cargo semantics).

**Forward to later phases:**

5. **`TypedConfig` enum will grow one variant per filter** across phases 04 (HTTP CM), 05 (HTTP/2), 06 (stats/access-logs). ADR-0014 explicitly anticipates this. The `envoy-protos` supersession ADR (xDS family, §9) will re-route `@type` URLs to prost-generated types in one sweep and retire this shim. No action in 02.1 or 02.2; surface the cross-reference when the xDS phase's SPEC is written.

6. **Round-robin distribution-equivalence assertion** remains unit-test-only (parent-brainstorm Q1 decision). If phase 06+ needs a statistical test on round-robin distribution (e.g., for access-log attribution verification), consider a differential-harness extension rather than a unit-test widening.

---

## 5. Files reviewed

Absolute paths opened during this review:

- `/Users/esa/git/envoy-rust/Cargo.toml`
- `/Users/esa/git/envoy-rust/BOOTSTRAP_PROMPT.md` (§3 D-3.2 permitted-foundations list)
- `/Users/esa/git/envoy-rust/rust-toolchain.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-cluster/Cargo.toml`
- `/Users/esa/git/envoy-rust/crates/envoy-cluster/src/lib.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-cluster/src/cluster.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-config/src/bootstrap.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-config/src/lib.rs`
- `/Users/esa/git/envoy-rust/crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_single_endpoint.yaml`
- `/Users/esa/git/envoy-rust/crates/envoy-config/fuzz/corpus/parse_bootstrap/tcp_proxy_round_robin_triple.yaml`
- `/Users/esa/git/envoy-rust/crates/envoy-config/fuzz/.gitignore`
- `/Users/esa/git/envoy-rust/tests/helpers/tcp-echo-server/Cargo.toml`
- `/Users/esa/git/envoy-rust/tests/helpers/tcp-echo-server/src/main.rs`
- `/Users/esa/git/envoy-rust/tests/differential/src/lib.rs`
- `/Users/esa/git/envoy-rust/tests/differential/src/upstream.rs` (verified `#[ignore]` text at line 94)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/DECISIONS.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/ROADMAP.md` (diff-only)
- `/Users/esa/git/envoy-rust/docs/envoy-rust/STATE.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/02.1-config-cluster/SPEC.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/02.1-config-cluster/PROGRESS.md`
- `/Users/esa/git/envoy-rust/docs/envoy-rust/phases/01-static-bootstrap-config/REVIEW.md` (shape precedent + I3 cross-reference)
- Git ranges: `git log --oneline aef36ce..95a26a7`, `git diff --stat aef36ce..95a26a7`, `git show 95a26a7`, `git diff aef36ce..95a26a7 -- docs/envoy-rust/DECISIONS.md`, `git diff aef36ce..95a26a7 -- docs/envoy-rust/ROADMAP.md`, `git diff aef36ce..95a26a7 -- rust-toolchain.toml`

Local spot-checks run: `cargo fmt --all -- --check` (exit 0), `cargo clippy --workspace --all-targets --all-features -- -D warnings` (exit 0), `grep -n '#\[ignore' tests/differential/src/**/*.rs` (one hit, matching the PROGRESS correction).

---

## 6. Initial verdict

**Approved with fixes** (initial review, HEAD `95a26a7`).

No Critical blockers. Three Important findings, none of which touch production code:

- **I1** (Cargo.lock drift): queued in PROGRESS.md §"State 4 / Notes" for a pre-state-6 commit mirroring phase-01 precedent `4955252`. Must land before the state-6 phase-done commit.
- **I2** (STATE.md stale): must advance STATE.md in the same commit that lands this REVIEW.md (state-5 transition) and again at state 6.
- **I3** (positive ClusterType::Static test): optional; tracked forward to the phase that extends `ClusterType`.

The `envoy-cluster` crate, the `envoy-config` schema extensions, the `tcp-echo-server` helper, the I3 close-out tests, the fuzz-corpus seeds, and ADR-0014 are all shape-correct, test-backed, and doctrine-compliant. The executor's self-audit discipline (PROGRESS tasks 2, 9, 10, 11, 12) and first-attempt CI-gate greenness are material positive signals. State 5 may complete in this session by committing REVIEW.md + the STATE-advance; state 6 then requires the Cargo.lock sync commit before the phase-done commit, and a final STATE-advance commit after the ROADMAP flip.

---

## 7. State-5 close-out — I1 and I2 remediation (2026-04-24)

I1 and I2 are both mechanical remediations (Cargo.lock regeneration and STATE.md text advance) that do not touch production code, do not alter doctrine, and do not change the review's technical findings. A narrow re-review by `superpowers:code-reviewer` is not warranted — phase-01 precedent `f436c29` triggered a re-review because its I1 remediation landed a *new ADR* and *nested-toolchain config*; 02.1's I1+I2 remediation touches only `Cargo.lock` (auto-generated from already-shipped `Cargo.toml` declarations) and `STATE.md` (running log of the lifecycle state).

### I1 — Cargo.lock sync commit

- Commit: `dea4d16` — `phase 02.1: sync Cargo.lock with phase 02.1 dep graph`
- Diff: `Cargo.lock` only, +19 lines, 2 new `[[package]]` stanzas (`envoy-cluster v0.0.0` + `tcp-echo-server v0.0.0`) matching the dependency sets declared in `crates/envoy-cluster/Cargo.toml` and `tests/helpers/tcp-echo-server/Cargo.toml`.
- Verification: `git diff 95a26a7..dea4d16 -- Cargo.lock` shows exactly the two expected stanzas, no version bumps and no transitive additions. `cargo check --workspace` re-runs clean without further drift.
- Phase-01 precedent followed verbatim: `4955252` shape (single-file commit, narrative message enumerating which phase tasks caused the drift, Co-Authored-By line).

### I2 — STATE.md advance

- Commit: this commit (lands alongside REVIEW.md).
- Diff: `docs/envoy-rust/STATE.md` — `status:` advanced from "state 3 (PLAN.md exists, implementation incomplete)" to "state 5 (REVIEW.md approved; state-6 next)"; "Next expected skill" rewritten for the state-6 phase-done gate; "Last commit" reference updated; "Last updated" stamp refreshed. No `Notes` section rewriting; rollover tracking (I3, M1–M4) delegated to this REVIEW.md §3–§4.
- Phase-01 precedent followed: `f436c29` shape (STATE-advance commit that lands REVIEW.md and flips STATE.md in one atomic move).

### I3 and M1–M4

Tracked forward per §3 and §4. None are state-5 or state-6 blockers.

### Final verdict

**Approved** (state 5 complete). HEAD is the commit landing this REVIEW.md; next session executes state 6 (phase-done commit + ROADMAP `02.1` flip to done + STATE advance to phase 02.2, lifecycle state 1). State 6 does not require further review work — it is a docs/ROADMAP/STATE commit with the ADR-tagged phase-done message per `BOOTSTRAP_PROMPT.md` §5.3 and SPEC §8.
