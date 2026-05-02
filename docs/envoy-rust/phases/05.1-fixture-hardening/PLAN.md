# Phase 05.1 — Fixture-hardening preamble: `ClusterType::StrictDns` + 5-fixture coordinated edit + phase-02.1 I3 close — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` (committed at parent-05 state-2 commit alongside ADR-0022). This plan operationalizes SPEC §§D1–D5. Where the plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-05 SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` (committed at SHA `cd1a70e`) is preserved unedited as a historical artifact; for execution it is superseded by sub-phase SPECs (this 05.1 SPEC, plus `05.2-http2-downstream/SPEC.md` and `05.3-http2-upstream/SPEC.md` for later sub-phases).

**Goal:** Land the fixture-hardening preamble for parent phase 05 in three coordinated parts that ship in a single sub-phase: (1) **Schema growth** in `crates/envoy-config/src/bootstrap.rs::ClusterType` — extend the single-variant `Static` enum at lines 58–62 to `Static | StrictDns`. ADR-0023 (`ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred) lands inline at Task 1. (2) **Runtime growth** in `crates/envoy-cluster/src/cluster.rs::from_bootstrap` — extend the existing constructor at lines 112–153 with a `STRICT_DNS` resolution branch via `tokio::net::lookup_host(format!("{}:{}", address, port)).await`; promote the function to `async`; add a new `ClusterError::DnsResolutionFailed { cluster, address, source: std::io::Error }` variant; add `tokio = { version = "1", features = ["net"] }` (existing tokio feature set + `net`) to `crates/envoy-cluster/Cargo.toml` (which today carries no tokio dep at all); update the single `envoy-bin` call site at `crates/envoy-bin/src/main.rs:83` to `await` the now-async function. (3) **Coordinated 5-fixture YAML edit** — flip `type: STATIC` → `type: STRICT_DNS` on the cluster whose endpoints reference `{{BACKEND_HOST}}` in `tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}` (10 YAML files; ~30 LoC of YAML diff; one bundled commit per SPEC §3 D3 + §6 signpost 8). After this edit, the 5 affected Docker-gated tests pass against upstream Envoy v1.33.0 again, materially closing **phase-04.3 REVIEW C-1** (cross-phase Docker-gated `host.docker.internal`/`STATIC` regression latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3). The new positive `static_cluster_constructs_with_literal_ip` test in D2 closes **phase-02.1 REVIEW I3** (positive `Static` regression guard, deferred since phase-02.1 close because the single-variant enum had no second variant against which to discriminate — adding `StrictDns` unblocks the discriminator). **NO HTTP/2 work in 05.1.** The `envoy-http2` crate, the `h2 = "0.4"` dep, the HCM-on-H2 dispatch, fixtures 0009/0010, and the h2spec ≥95% conformance gate all defer to sub-phases 05.2 and 05.3 per ADR-0022 (parent-05 split decision). 05.1 introduces no new top-level Cargo deps (`tokio::net::lookup_host` lives in the existing `tokio` foundation already pulled by `envoy-bin`/`envoy-listener`/etc.; envoy-cluster gains a new direct dep on `tokio` but the workspace's transitive `tokio` graph is unchanged). ~4 tasks, ~270 LoC per SPEC §§3 D1–D4 (~110 LoC schema + ~100 LoC runtime + ~30 LoC YAML + ~30 LoC misc); comfortably under both `BOOTSTRAP_PROMPT.md` §6.1 split-gates (~25 tasks / ~1500 LoC). One ADR (ADR-0023) anticipated per SPEC §7. **Do not split 05.1 further** — the scope is well below the §6.1 gates and a nested split of an already-split sub-phase is explicitly flagged as suspicious in `BOOTSTRAP_PROMPT.md` §6.1; if execution surfaces drift, invoke `superpowers:systematic-debugging` first.

**Architecture.** The schema delta is mechanical: the existing `ClusterType { Static }` enum at `crates/envoy-config/src/bootstrap.rs:60-62` carries `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]`, so adding a `StrictDns` variant is a one-line addition that automatically lights up the `STRICT_DNS` serde tag and continues to reject `LOGICAL_DNS` / `WEIRD_TYPE` / etc. as `"unknown variant"` errors via the existing serde machinery (no new `ConfigError` variant for parse-side rejection — the SPEC §3 D1 reasoning is reproduced verbatim). The runtime delta in `envoy-cluster::from_bootstrap` adds a `match cluster_def.cluster_type` arm: the existing literal-IP `parse::<SocketAddr>()` path stays unchanged for `ClusterType::Static`; for `ClusterType::StrictDns` the constructor calls `tokio::net::lookup_host(format!("{}:{}", address, port)).await` and stores the resolved `SocketAddr`s in the cluster's endpoint list. **Planner-time signpost on the new error variant placement** (resolves a SPEC ambiguity, see §6 signpost A below): the SPEC §3 D1 prose locates the new error variant on `envoy_config::ConfigError` (`ConfigError::ClusterDnsResolutionFailed`), but SPEC §6 signpost 14 simultaneously claims envoy-cluster returns `ClusterError` (typed) from its constructor unchanged, and the SPEC §3 D2 pseudocode mixes `ClusterError::EndpointParse` and `ConfigError::ClusterDnsResolutionFailed` returns from the same `?` chain — three statements that cannot all hold simultaneously. The planner's resolution: **add the new variant to `ClusterError`** (`ClusterError::DnsResolutionFailed { cluster: String, address: String, source: std::io::Error }`) at `crates/envoy-cluster/src/cluster.rs:95-107`, NOT to `ConfigError`. Reasoning: (a) the DNS resolution lives in `envoy_cluster::from_bootstrap` which returns `ClusterError` today; (b) signpost 14's claim that envoy-cluster's typed-error chain is preserved holds with this placement; (c) implementation is a single new variant + a single new arm, vs. the cross-crate wrapper-variant gymnastics required to fold ConfigError into envoy-cluster's return type; (d) `envoy-bin`'s existing `?`-to-`anyhow` boundary at `main.rs:83` absorbs the typed error identically regardless of which enum carries the variant. ADR-0023's prose in DECISIONS.md uses `ClusterError::DnsResolutionFailed` in its Decision and Consequences sections; this is a faithful refinement of the SPEC §7 ADR-0023 projection (the projection used `ConfigError::ClusterDnsResolutionFailed` but the planner discovered the cross-statement contradiction at PLAN-write time and resolved it per D-3.5). The 5-fixture YAML edit is purely mechanical — 10 files × 1 line each (`type: STATIC` → `type: STRICT_DNS`); one bundled commit per SPEC recommendation §3 D3 / §6 signpost 8 (the 10 edits are mechanically identical, the differential property is "all 5 fixtures green simultaneously," and the signal is cleanest in one diff). Per-side substitutions — `envoy.yaml` rendered with `BACKEND_HOST=host.docker.internal` (Docker host-gateway per ADR-0015), `envoy-rust.yaml` rendered with `BACKEND_HOST=127.0.0.1` (envoy-rust host-process posture) — are unchanged; both proxies receive the new `type: STRICT_DNS` cluster shape and resolve their respective hostnames at startup (Envoy via its STRICT_DNS resolver consulting `/etc/hosts`; envoy-rust via `tokio::net::lookup_host` against literal-IP loopback which trivially resolves). Fixtures 0001/0002/0007 are NOT edited — they don't reference `host.docker.internal` at any cluster (verified by `grep -L 'host.docker.internal\|BACKEND_HOST' tests/fixtures/000{1,2,7}*/envoy*.yaml` returns all three; fixture 0001 has no upstream cluster, fixture 0002 is admin-only, fixture 0007 is `direct_response`-only with no upstream).

**Tech stack.** Rust edition 2024 on pinned stable (`rust-toolchain.toml` D-3.9). No new top-level Cargo deps in the workspace; `tokio` and its `net` feature are already in the workspace's transitive graph (tokio is a top-level dep on `envoy-bin`, `envoy-listener`, `envoy-tcp`, `envoy-http1`, `envoy-tls`, `tests/differential`, `tests/helpers/{tcp,tls,http1}-echo-server`, all using `features = ["...", "net", ...]` already; envoy-cluster's new direct tokio dep activates the same feature set via the workspace's already-resolved feature unification). `tokio::net::lookup_host` is the chosen DNS resolver primitive per D-3.2 + ADR-0023 — explicitly NOT `trust-dns-resolver` / `hickory-resolver` (neither on D-3.2's permitted list; would require its own permitted-foundations-extension ADR which 05.1 does not warrant). New Cargo manifest entry: `tokio = { version = "1", features = ["net", "rt", "macros"] }` on `crates/envoy-cluster/Cargo.toml`'s `[dependencies]`. New runtime API surface: `ClusterError::DnsResolutionFailed { cluster, address, source }` variant (one new variant on the existing public `ClusterError` enum); `from_bootstrap`'s signature changes from `pub fn from_bootstrap(...) -> Result<ClusterManager, ClusterError>` to `pub async fn from_bootstrap(...) -> Result<ClusterManager, ClusterError>` (the return type is unchanged; only the `async` qualifier is added). New schema surface: `ClusterType::StrictDns` variant. New fuzz corpus seed: `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml`. No changes to `.github/workflows/ci.yml`, `deny.toml` (no new transitive licenses anticipated; cross-checked at Task 4), `BEHAVIOR_CONTRACT.md` (per SPEC §2), `rust-toolchain.toml`, `ENVOY_TARGET.md`, or root `Cargo.toml` `[workspace] members` (no new crates).

---

## File structure (created / modified / not touched)

**Created:**

- `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md` (appended once per task during execution; created by Task 1 alongside the ADR-0023 landing).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml` — new fuzz seed exercising the `ClusterType::StrictDns` parse path. Mirrors the existing 04.x seed shape (e.g., `route_with_header_matchers.yaml`, `hcm_route_to_cluster.yaml` at the same directory).

**Modified:**

- `docs/envoy-rust/DECISIONS.md` — append **ADR-0023** (`ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred) at Task 1, immediately after the existing ADR-0022 block ending at line 422. The ADR ledger head before this commit is ADR-0022; ADR-0023 lands at the next-sequential number with no renumbering needed.
- `crates/envoy-config/src/bootstrap.rs` — extend the existing `ClusterType` enum at lines 58–62 from `Static` to `Static | StrictDns`; append ~6 new validator unit tests to the `#[cfg(test)] mod tests` block (`parses_cluster_with_type_strict_dns`, `parses_cluster_with_type_static_unchanged`, `rejects_cluster_with_type_logical_dns`, `rejects_cluster_with_unknown_type_value`, `parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment`, `validates_strict_dns_cluster_does_not_require_literal_ip_endpoints`) covering the parse-path surface.
- `crates/envoy-config/src/lib.rs` — no changes. The existing `pub use bootstrap::{... ClusterType ...}` re-export at lines 11–18 already re-exports `ClusterType` (and its `Static` and `StrictDns` variants implicitly); no new `ConfigError` variant in 05.1 (per SPEC §3 D1: the parse-side rejection of `LOGICAL_DNS` / unknown variants surfaces via the existing `ConfigError::Yaml(serde_yaml::Error)` arm which already wraps serde's `"unknown variant"` errors).
- `crates/envoy-cluster/src/cluster.rs` — extend `ClusterError` enum at lines 95–107 with one new variant `DnsResolutionFailed { cluster: String, address: String, source: std::io::Error }`; promote `from_bootstrap` at line 112 to `async fn`; restructure the per-cluster endpoint-build loop at lines 120–135 into a `match cfg.cluster_type` two-arm dispatch (the `Static` arm reuses the existing `parse::<SocketAddr>()` path unchanged; the `StrictDns` arm calls `tokio::net::lookup_host(format!("{}:{}", sa.address, sa.port_value)).await` and `.collect::<Vec<_>>()`s the resolved `SocketAddr`s, with a defensive zero-result guard returning `ClusterError::DnsResolutionFailed`); append ~3 new unit tests to the `#[cfg(test)] mod tests` block (`static_cluster_constructs_with_literal_ip` — closes phase-02.1 REVIEW I3; `strict_dns_cluster_resolves_localhost_at_build_time`; `strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain`); update the existing 5 unit tests that call `crate::from_bootstrap` to add `.await` (mechanical).
- `crates/envoy-cluster/Cargo.toml` — add `tokio = { version = "1", features = ["net", "rt", "macros"] }` to `[dependencies]` (currently the file has only `envoy-config = { path = "../envoy-config" }` and `thiserror = "2"`); add `tokio = { version = "1", features = ["macros", "rt"] }` to `[dev-dependencies]` for the new `#[tokio::test]`-flavored tests in Task 2's test additions (the dev-dep entry uses the same crate as the runtime dep but is listed separately for clarity per the workspace convention).
- `crates/envoy-bin/src/main.rs` — at line 83 (`envoy_cluster::from_bootstrap(&bootstrap).context("building cluster manager")?`) add `.await` before `.context(...)`. The change is one token: `from_bootstrap(&bootstrap).await.context(...)`. No other changes.
- `crates/envoy-config/fuzz/.gitignore` — append one allow-list entry `!corpus/parse_bootstrap/strict_dns_cluster.yaml` (the corpus directory's `.gitignore` lists each non-ignored seed by name; current entries cover 11 seeds; this adds a 12th entry).
- `tests/fixtures/0003-tcp-proxy/envoy.yaml` (line 27) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml` (line 21) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0004-tls-downstream/envoy.yaml` (line 37) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0004-tls-downstream/envoy-rust.yaml` (line 31) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0005-tls-upstream/envoy.yaml` (line 16) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0005-tls-upstream/envoy-rust.yaml` (line 15) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0006-tls-sni/envoy.yaml` (line 40) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0006-tls-sni/envoy-rust.yaml` (line 39) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0008-http1-router-upstream/envoy.yaml` (line 49) — `type: STATIC` → `type: STRICT_DNS`.
- `tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml` (line 27) — `type: STATIC` → `type: STRICT_DNS`.
- `docs/envoy-rust/ROADMAP.md` — at state 6 only (NOT a state-3 task), flip row `05.1` `status` `planned` → `done`. Parent row `05` stays `in-progress` (flips at sub-phase 05.3's state-6 commit per the ROADMAP-schema invariant). State-6 close-out is a separate session per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session") — not part of this PLAN's tasks.
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase to sub-phase `05.2-http2-downstream` lifecycle state 3 (the 05.2 SPEC was landed at parent-05 state-2 alongside 05.1's; PLAN.md does not exist yet for 05.2 so the lifecycle state is "SPEC.md exists, PLAN.md does not"). Next-skill `superpowers:writing-plans` scoped to sub-phase 05.2. Notes section gains the carryforward bookkeeping per SPEC §6 signpost 17 (C-1 closed; I3 closed; M-claim still deferred).
- `Cargo.lock` — sync at state-4 phase-done gate (Task 4) per the established phase-precedent (phase-01 `4955252`, phase-02.1 `dea4d16`, phase-02.2 `2146014`, phase-03.1 `eb039e6`, phase-03.2 `85685a3`, phase-04.x inline). Expected diff: minimal — possibly a single-line feature-resolution difference if `tokio`'s `net` feature wasn't already activated in the workspace's resolved feature set (but tokio's `net` is already pulled by other workspace crates per phase 02.x onwards, so likely no diff). Cross-checked at Task 4.
- `deny.toml` — no edits anticipated. `tokio` is already on the deny.toml allow-list since phase 00; no new transitive licenses surface from adding `tokio` as a direct dep on `envoy-cluster` (the workspace already has the full `tokio` transitive graph from phase 00 onwards). Cross-checked at Task 4.

**Not touched in 05.1** (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `cd1a70e`.
- `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` (this sub-phase) — landed at parent-05 state-2 commit alongside 05.2/05.3 SPECs; unedited in 05.1 execution.
- `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md`, `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` — landed at parent-05 state-2 alongside this SPEC; unedited in 05.1 execution (their PLAN/PROGRESS/REVIEW land in their own sub-phase execution windows).
- `docs/envoy-rust/phases/04*` (parent-04 + 04.1 + 04.2 + 04.3) — closed at the 04.3 phase-done commit `e626862`; unedited in 05.1.
- `docs/envoy-rust/phases/{00,01,02,02.1,02.2,03,03.1,03.2}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.1 (per SPEC §2: 05.1 produces no new responses / no new headers / no new wire shapes; the equivalence-matrix engagement is transitive — the 5 restored fixtures continue exercising the same matrix dimensions they did at phase-04.3 close).
- `docs/envoy-rust/MISSION.md` — frozen per its self-described durability discipline.
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-http1/`, `crates/envoy-listener/` — unchanged. The schema growth is in envoy-config + envoy-cluster only; downstream consumers don't see the new `ClusterType` variant directly (the `Cluster` struct's `cluster_type: ClusterType` field is matched only at `from_bootstrap` time; consumers see resolved `SocketAddr`s through `ClusterHandle::pick_endpoint`).
- `tests/helpers/{tcp,tls,http1}-echo-server/` — unchanged. The echo servers are spawned by the differential harness at fixture-render time and don't see the cluster's `type:` value; they only observe an incoming TCP/TLS/HTTP-1 connection.
- `tests/differential/src/{lib,backend,upstream,subject,tls}.rs`, `tests/differential/Cargo.toml` — unchanged. The harness's per-side YAML render mechanism (the `{{BACKEND_HOST}}` substitution per ADR-0015) is unchanged; it now substitutes into a fixture YAML that declares `type: STRICT_DNS` instead of `type: STATIC`, but the harness doesn't parse the YAML — it just text-substitutes and hands the rendered file to the upstream Envoy container / envoy-rust subprocess.
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/` — unchanged. The existing `parse_bootstrap` fuzz target picks up the new `strict_dns_cluster.yaml` seed automatically via the corpus directory.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0007-http1-direct-response/` — unedited. Verified by `grep -L 'host.docker.internal\|BACKEND_HOST' tests/fixtures/000{1,2,7}*/envoy*.yaml` returning all three at PLAN-write time; their fixtures must remain green at the 05.1 state-4 phase-done gate.
- Root `Cargo.toml` — no `[workspace] members` changes (no new crates in 05.1).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `.github/workflows/ci.yml` — untouched (per SPEC §3 D5 / phase-precedent: existing `cargo test --workspace` + `cargo +nightly fuzz run parse_bootstrap` jobs pick up the additions automatically).

---

## Task index

Each task ends with a commit. `PROGRESS.md` gets a new section per task in the phase-04.x style (task id, commit SHA, change summary, verification tail, deviations from PLAN). Use the follow-up `phase 05.1: progress note (task N)` commit convention from 04.x if a post-hoc note is needed.

Ordering rationale (SPEC §3 deliverable order + §6 signposts 4 + 16):

- **Task 1 lands the schema growth + ADR-0023 + fuzz seed first** because every subsequent task references `ClusterType::StrictDns` at compile time (Task 2's `from_bootstrap` `match cluster_def.cluster_type` arm needs the variant defined; Task 3's fixture YAMLs need the parser to accept the new tag — though serde's `deny_unknown_fields` would reject them at fixture-render time if Task 1 didn't land first).
- **Task 2 lands the runtime extension second** because it consumes Task 1's `ClusterType::StrictDns` variant and produces the runtime behavior the fixtures will exercise; Task 2 also closes phase-02.1 REVIEW I3 via the `static_cluster_constructs_with_literal_ip` test.
- **Task 3 lands the 5-fixture coordinated YAML edit third** because the YAMLs become valid only after Tasks 1 + 2 land — before Task 1 the parser rejects `type: STRICT_DNS`; before Task 2 the runtime crashes at endpoint-build time on `host.docker.internal` even with the new tag accepted.
- **Task 4 closes the state-4 phase-done gate last** with the full `cargo build` / `clippy` / `fmt` / `test` / `deny` / fuzz short-budget run + Cargo.lock sync (likely no-op) + Docker-gated CI re-push that materially closes phase-04.3 REVIEW C-1 by demonstrating green runs on fixtures 0003/0004/0005/0006/0008.

Tasks:

1. **`envoy-config` — `ClusterType::StrictDns` schema variant + ADR-0023 inline + 6 validator unit tests + 1 new fuzz seed (`strict_dns_cluster.yaml`) + `.gitignore` allow-list extension**
2. **`envoy-cluster` — `tokio` direct dep + async promotion of `from_bootstrap` + `ClusterError::DnsResolutionFailed` variant + `STRICT_DNS` resolution branch + 3 new unit tests (incl. phase-02.1 REVIEW I3 close-out) + 5 existing-test `.await` updates + `envoy-bin` call-site `.await`**
3. **5-fixture coordinated YAML edit — flip `type: STATIC` → `type: STRICT_DNS` on `tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}` (10 files; bundled commit per SPEC §6 signpost 8)**
4. **State-4 phase-done gate — run all 5 stable commands + fuzz short-budget + observe Docker-gated CI; quote outputs into PROGRESS.md; sync `Cargo.lock` per the phase-precedent**

Estimated total: 4 tasks, ~270 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold with massive headroom (4 < 25, ~270 ≪ 1500). **Do not split 05.1 further.** Per parent-05 SPEC §5 + ADR-0022's express avoidance of nested splits, a 05.1.1 / 05.1.2 split would be a strong scope-creep signal and warrants `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1. Closest scope-creep vector at PLAN-write time: Task 2's runtime extension folds the async promotion + the new error variant + the new resolution branch + the I3 close-out test + the existing-test `.await` updates; if Task 2's sub-step count exceeds ~12 at execution, factor the I3 close-out test (`static_cluster_constructs_with_literal_ip`) into a Task 2.5 standalone commit instead of nested-splitting the phase.

---

## Implementation signposts (planner-time clarifications + ambiguity resolutions)

**Signpost A — `ClusterError::DnsResolutionFailed` (NOT `ConfigError::ClusterDnsResolutionFailed`).**

The 05.1 SPEC §3 D1 prose locates the new error variant on `envoy_config::ConfigError` ("`ConfigError` extension in `crates/envoy-config/src/lib.rs`: add one new variant for the DNS-resolution-failure case"). SPEC §3 D2 pseudocode contains both `.map_err(|e| ClusterError::EndpointParse { ... })?` and `.map_err(|e| ConfigError::ClusterDnsResolutionFailed { ... })?` inside the same `from_bootstrap` body. SPEC §6 signpost 14 simultaneously claims "envoy-cluster returns `ClusterError` (typed) from its constructor; envoy-config returns `ConfigError` (typed) from validate." These three statements cannot all hold simultaneously: a single function can return only one error type, and the existing `from_bootstrap` lives in `envoy-cluster` and returns `ClusterError`.

The planner resolves the ambiguity by adding the new variant to `ClusterError`, NOT `ConfigError`:

- `crates/envoy-cluster/src/cluster.rs` `ClusterError` enum gains one new variant: `DnsResolutionFailed { cluster: String, address: String, source: std::io::Error }`.
- `from_bootstrap`'s signature remains `Result<ClusterManager, ClusterError>` (modulo the `async` qualifier added in Task 2).
- `envoy_config::ConfigError` is unchanged.
- ADR-0023's prose in DECISIONS.md uses `ClusterError::DnsResolutionFailed` in its Decision and Consequences sections (faithful refinement of the SPEC §7 ADR-0023 projection per D-3.5).

Rationale: (a) the DNS resolution code site is `from_bootstrap` in envoy-cluster; (b) signpost 14's claim that envoy-cluster's typed-error chain is preserved holds with this placement; (c) the alternative (`ConfigError::ClusterDnsResolutionFailed` + a `From<ClusterError> for ConfigError` wrapper variant + changing `from_bootstrap`'s return type to `Result<_, ConfigError>`) is a substantially larger rework that touches every existing test in `crates/envoy-cluster/src/cluster.rs::tests` and adds a cross-crate error-coupling that doesn't exist today; (d) `envoy-bin`'s existing `?`-to-`anyhow` chain at `crates/envoy-bin/src/main.rs:83` absorbs the typed error identically regardless of which enum carries the variant. PROGRESS.md at Task 1 records the deviation explicitly.

**Signpost B — `tokio` is a NEW direct dep on `envoy-cluster`, not a feature flip.**

`crates/envoy-cluster/Cargo.toml` at HEAD `e626862` lists only `envoy-config = { path = "../envoy-config" }` and `thiserror = "2"` under `[dependencies]` — there is NO existing tokio dep. The 05.1 SPEC §3 D2 cross-crate dependency note (`"crates/envoy-cluster/Cargo.toml already pulls tokio = { version = '1', features = ['net', 'rt', ...] } per the 04.x shape"`) is INCORRECT at HEAD `e626862` — verifiable by `cat crates/envoy-cluster/Cargo.toml` returning only `envoy-config` + `thiserror`. The planner discovers the SPEC error at PLAN-write time and records the correction here. Task 2 adds `tokio` as a new direct dep with `features = ["net", "rt", "macros"]` (`net` for `lookup_host`, `rt` for the runtime types referenced in async-fn signatures, `macros` for `#[tokio::test]` in the new dev-dep tests). This is NOT a new top-level dep at the workspace level — `tokio` is already a top-level dep on `envoy-bin`/`envoy-listener`/`envoy-tcp`/`envoy-http1`/`envoy-tls`/`tests/differential`/`tests/helpers/{tcp,tls,http1}-echo-server`; the workspace's transitive `tokio` graph and Cargo.lock entries are unchanged. SPEC §1 acceptance signal (e) (`cargo deny check` clearance) holds — no new transitive licenses.

**Signpost C — `from_bootstrap` async promotion is mechanical; envoy-bin call site update is one token.**

At HEAD `e626862` `from_bootstrap` is `pub fn from_bootstrap(...) -> Result<ClusterManager, ClusterError>` (sync) and is called once from envoy-bin at `crates/envoy-bin/src/main.rs:83` as `envoy_cluster::from_bootstrap(&bootstrap).context("building cluster manager")?`. The `main.rs` function is already `async fn main(...) -> Result<()>` (verifiable at task-2 time by `head -30 crates/envoy-bin/src/main.rs`), so adding `.await` requires no further async-context plumbing. Total churn: 1 token at the call site (`from_bootstrap(&bootstrap).await.context(...)`); 1 token at the function signature (`pub fn` → `pub async fn`); 5 `.await` additions to existing tests in `cluster.rs::tests` that call `crate::from_bootstrap(...)` (lines 281, 322, 367/368 — wait, the existing tests call `from_bootstrap` via `crate::from_bootstrap(&bootstrap).expect(...)` — see Task 2 Step 11 below for the exact list of test locations to update).

**Signpost D — `localhost` is the chosen unit-test target for the DNS resolution test (per SPEC §6 signpost 10).**

The `strict_dns_cluster_resolves_localhost_at_build_time` test in Task 2 uses the literal string `"localhost"` as the DNS-name endpoint address. Universally resolvable on any developer machine and in CI; loopback-bound; matches the `parse_bootstrap` fuzz seed in Task 1 (`strict_dns_cluster.yaml` also uses `localhost`). Alternatives considered + rejected per SPEC §6 signpost 10: `127.0.0.1` (literal IP — works but doesn't exercise DNS-layer behavior); `host.docker.internal` (environment-dependent — only resolves on a Docker-running host; exercised at fixture level via Task 3 / Task 4, NOT at unit level); `example.com` (externally-resolved DNS — introduces network dependency in the test). Decision is documented in the test's rustdoc per Task 2 Step 8.

**Signpost E — `.invalid` TLD is the chosen NXDOMAIN test target (per SPEC §6 signpost 11).**

The `strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain` test in Task 2 uses `"this-host-does-not-exist.invalid"` as the unresolvable endpoint address. RFC 6761 §6.4 reserves `.invalid` as non-resolvable. Fallback per SPEC §6 signpost 11: if CI flakes (a misconfigured DNS resolver could synthesise a positive answer), switch to a target string guaranteed-malformed at the resolver layer (e.g., `tokio::net::lookup_host("")` with the empty-string host returns a typed `io::Error` reliably — but the empty-string case requires a slightly different test shape since the address is rendered from the cluster's configured `address` + `port_value`; the planner uses `.invalid` as the primary target and documents the fallback at Task 2 Step 9 commentary). PROGRESS.md notes the choice for forward auditability.

**Signpost F — `dns_lookup_family` is NOT parsed in 05.1.**

Envoy's `STRICT_DNS` cluster has an optional `dns_lookup_family` field (default `AUTO`; alternatives `V4_ONLY` / `V6_ONLY` / `V4_PREFERRED` / `ALL`). 05.1 does NOT add this field to the `Cluster` struct in `crates/envoy-config/src/bootstrap.rs`; serde's existing `deny_unknown_fields` on `Cluster` continues to reject any fixture YAML that adds it. The 5-fixture YAML edit in Task 3 omits the field — both proxies rely on the `AUTO` default. If a CI host's resolver picks AAAA where the test expected A (or vice versa) and the fixture flakes (unlikely; loopback and Docker host-gateway both have stable IPv4 representations), the planner re-enters state 3 and adds `dns_lookup_family: V4_ONLY` to the affected fixture YAML + a small `Cluster` struct extension (~10 LoC + 1 unit test) to parse the field. Recommended posture per SPEC §3 D3 + §6 signpost 6: do NOT extend the schema preemptively.

**Signpost G — One bundled commit for the 5-fixture YAML edit (per SPEC §3 D3 / §6 signpost 8).**

Task 3 lands all 10 YAML edits in one commit. Cleanest in `git log`; one easy-to-read diff; the differential property is "all 5 fixtures green simultaneously," and splitting into 5 per-fixture commits would muddy the gate signal. The 04.3 per-fixture cadence applied to landing *new* fixtures (each fixture was a substantive addition); 05.1's cadence applies to *editing* existing fixtures uniformly with mechanically-identical edits.

---

### Task 1: `envoy-config` — `ClusterType::StrictDns` schema variant + ADR-0023 inline + 6 validator unit tests + 1 new fuzz seed + `.gitignore` allow-list extension

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md` (append ADR-0023 immediately after the existing ADR-0022 block ending at line 422).
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `ClusterType` enum at lines 60-62 with `StrictDns` variant; append 6 unit tests to the existing `#[cfg(test)] mod tests` block at the end of the file).
- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml` (new fuzz seed).
- Modify: `crates/envoy-config/fuzz/.gitignore` (append `!corpus/parse_bootstrap/strict_dns_cluster.yaml` to the allow-list).
- Create: `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md` (new file with Task 1 section).

**Why first:** every subsequent task references `ClusterType::StrictDns` at compile time — Task 2's `from_bootstrap` `match cluster_def.cluster_type` arm needs the variant defined; Task 3's fixture YAMLs would be rejected by the parser at fixture-render time with serde's `"unknown variant 'STRICT_DNS'"` error if Task 1 didn't land first; Task 4's fuzz short-budget run picks up the new corpus seed automatically.

**Scope.** ~15 LoC schema delta (one enum variant) + ~80 LoC unit tests (6 tests × ~13 LoC each) + ~25 LoC new fuzz seed YAML + 1 line `.gitignore` + 1 new ADR (~25 lines DECISIONS.md). Total ~145 LoC. ADR-0023 lands inline at this task per SPEC §7 (mirrors ADR-0021 inline-at-Task-1 pattern, commit `984aedd`).

- [ ] **Step 1: Verify ADR ledger head + STATE.md routing + ClusterType enum shape.**

```bash
grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -3
grep -A2 '^## Active phase' docs/envoy-rust/STATE.md | head -5
grep -n 'enum ClusterType\|^    Static' crates/envoy-config/src/bootstrap.rs
ls crates/envoy-config/fuzz/corpus/parse_bootstrap/ | wc -l
```

Expected: ADR count `22` (latest ADR-0022 from parent-05 state-2). The third grep returns `id: 05.1`, `slug: 05.1-fixture-hardening`. The fourth grep returns `60:pub enum ClusterType {` and `61:    Static,`. The fifth returns `11` seeds.

If any unexpected `ADR-00NN` appears beyond ADR-0022, debug per `superpowers:systematic-debugging` before continuing — phase 05.1 anticipates exactly one new ADR (ADR-0023) and none thereafter (per SPEC §7).

If `crates/envoy-config/src/bootstrap.rs::ClusterType` shape differs from the expected single-variant `Static` (e.g., another variant has been added by an unrelated change), invoke `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §1 step E.

- [ ] **Step 2: Append ADR-0023 to `docs/envoy-rust/DECISIONS.md`.**

Find the end of the file (the ADR-0022 block ends at line 422 with the closing `---` separator; append after that). The ADR text:

```markdown
## ADR-0023: `ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred

- Date: 2026-05-01
- Status: accepted
- Context: Phase 05.1 is the fixture-hardening preamble for parent phase 05 (HTTP/2 cleartext data plane). The cross-phase Docker-gated `host.docker.internal`/`type: STATIC` regression — originating at phase-02.2's ADR-0015 landing (commit `435c6fa`) and discovered at phase-04.3 task 14 (commit `eb6f972`) per the C-1 trace in parent-05 SPEC §1 — must be closed before any new H2 surfaces are layered on top of the 5 affected fixtures (0003/0004/0005/0006/0008). Upstream Envoy v1.33.0's `socket_address.address` parse semantics expect either a literal IP (under `type: STATIC`) or DNS resolution opt-in (under `type: STRICT_DNS` or `type: LOGICAL_DNS`); envoy-rust's parser currently accepts only `STATIC` (single-variant `ClusterType { Static }` enum at `crates/envoy-config/src/bootstrap.rs:60-62`). Phase-02.1 REVIEW I3 (positive `ClusterType::Static` variant-name regression guard) has been deferred since phase-02.1 close because the single-variant enum had no second variant against which to discriminate `Static` structurally; adding a second variant unblocks I3 mechanically.
- Options considered: (i) **Add only `StrictDns`.** Resolves DNS at cluster-build time; results cached for the cluster's lifetime. Sufficient for the C-1 fix because `host.docker.internal` resolves to a single Docker-host-gateway address that doesn't change during the fixture run. **Chosen.** (ii) Add both `StrictDns` and `LogicalDns`. Mirrors Envoy's full proto more completely. Rejected: `LogicalDns`'s per-request re-resolution semantics require a non-trivial runtime extension (the cluster must drop the cached resolution after the resolved addresses are picked once, vs. round-robining over the cached set indefinitely under `StrictDns`); no 05.1 fixture exercises this distinction; D-3.6's "every phase is a green build" + the §6.1 split-gate reward minimal forward landings. (iii) Add `StrictDns` + a configurable `dns_refresh_rate` knob to enable periodic re-resolution. Rejected: same as (ii); no 05.1 fixture needs it; defers to a later phase per parent-05 SPEC §4. (iv) Defer the entire `STRICT_DNS` extension; fix C-1 by replacing `host.docker.internal` with a literal IP across the 5 fixtures. Rejected: would require either (a) testcontainers-side IP discovery at fixture-render time (the host-gateway IP varies across Docker setups), which is brittle, or (b) baking a static IP into the YAMLs, which is platform-specific. ADR-0015's `host.docker.internal` posture is the right cross-platform choice; the right fix is to make envoy-rust accept the DNS-name shape, not to abandon it. (v) Use a different DNS resolver (e.g., `trust-dns-resolver` / `hickory-resolver`) instead of `tokio::net::lookup_host`. Rejected: `tokio::net::lookup_host` is part of the existing `tokio` permitted foundation per D-3.2 (no new dep needed; no new ADR scope-extension required). A third-party resolver crate is not on D-3.2's permitted list and would require its own permitted-foundations-extension ADR, which 05.1 does not need.
- Decision: Extend `crates/envoy-config/src/bootstrap.rs::ClusterType` from single-variant `Static` to `Static | StrictDns`. Validator accepts the `STRICT_DNS` serde tag (`#[serde(rename_all = "SCREAMING_SNAKE_CASE")]` is already on the enum, so the tag mechanically maps to the new variant); runtime resolution lives in `crates/envoy-cluster/src/cluster.rs::from_bootstrap` (the cluster-manager constructor) via `tokio::net::lookup_host(format!("{}:{}", address, port)).await`. Resolution failures surface as a new `ClusterError::DnsResolutionFailed { cluster: String, address: String, source: std::io::Error }` variant on the existing `crates/envoy-cluster/src/cluster.rs::ClusterError` enum (planner-time refinement of SPEC §3 D1's projected `ConfigError::ClusterDnsResolutionFailed` placement — the existing `from_bootstrap` returns `ClusterError`, signpost 14 confirms envoy-cluster's typed-error chain stays unchanged, and adding the variant to ClusterError is mechanically simpler than introducing a cross-crate `From<ClusterError> for ConfigError` wrapper; see PLAN.md signpost A for the full reasoning). The `LOGICAL_DNS` variant is **NOT** added in 05.1; a future phase that needs per-request DNS re-resolution lands `LogicalDns` then. The existing `Static` variant's parse + runtime paths are unchanged (regression-guarded by the new positive `static_cluster_constructs_with_literal_ip` test in 05.1 D2, which closes phase-02.1 REVIEW I3).
- Rationale: `STRICT_DNS` is the simpler, more common case and is mechanically sufficient for the C-1 fix (`host.docker.internal` resolves locally via Docker's `host-gateway` mechanism per ADR-0015, and the resolved address doesn't change during the fixture run, so per-request re-resolution offers no value). `tokio::net::lookup_host` is the chosen resolver primitive because it's part of the existing `tokio` foundation under D-3.2 and requires no new permitted-foundations grant. Deferring `LOGICAL_DNS` follows D-3.6's minimalism principle ("every phase is a green build" — narrow scope = clean acceptance gate). Adding the second `ClusterType` variant unblocks the multi-phase phase-02.1 REVIEW I3 carryforward at zero additional cost (the I3 close-out test is one of D2's 3 unit tests).
- Consequences: `crates/envoy-config/src/bootstrap.rs::ClusterType` gains the `StrictDns` variant (~5 LoC including doc comment). `crates/envoy-cluster/src/cluster.rs::ClusterError` gains the `DnsResolutionFailed { cluster, address, source }` variant (~7 LoC). `crates/envoy-cluster/src/cluster.rs::from_bootstrap` is promoted to `async fn` and gains a `STRICT_DNS` resolution branch via `tokio::net::lookup_host(..).await` (~50 LoC including the per-cluster resolution loop, the zero-result defensive branch, and the existing-test `.await` updates). `crates/envoy-cluster/Cargo.toml` gains a direct `tokio = { version = "1", features = ["net", "rt", "macros"] }` dep (was previously absent; `tokio` is already a top-level dep on other workspace crates so the workspace's transitive graph is unchanged). `crates/envoy-bin/src/main.rs:83` adds `.await` to the `from_bootstrap` call (one token). `tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}` flip `type: STATIC` → `type: STRICT_DNS` (~30 LoC YAML diff total across 10 files). `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml` is added (1 new seed). **Phase-02.1 REVIEW I3 closes** at this commit (the positive `Static` regression guard `static_cluster_constructs_with_literal_ip` is one of D2's 3 unit tests). **Phase-04.3 REVIEW C-1 closes** at the 05.1 state-4 phase-done verification commit (the 5 affected Docker-gated fixtures pass simultaneously). **Phase-04.1 REVIEW M-claim** (drive_http1 per-function unit test) is unblocked but stays deferred per the 04.3 disposition; carryforward chain continues. `cargo deny check` remains clean: no new top-level Cargo deps; the `tokio` `net` feature is already activated in the workspace's resolved feature set (verified at PLAN-write time via `grep '"net"' crates/*/Cargo.toml`). Cargo.lock sync at state-4 is expected to be a no-op or near-no-op. Future phases that need `LogicalDns` (per-request DNS re-resolution), `dns_refresh_rate` (periodic re-resolution under `StrictDns`), `dns_lookup_family` (A/AAAA selection control), `respect_dns_ttl` (TTL-driven re-resolution), or `dns_resolvers` (custom resolver pool) extend then; this ADR's narrow scope is deliberate.
- Provenance: this ADR was projected as the next-sequential available ADR number in parent-05 SPEC §7 (`docs/envoy-rust/phases/05-http2/SPEC.md`, committed at parent-05 state-1 SHA `cd1a70e`); ADR-0022 (parent-05 split decision) lands at parent-05 state-2 alongside the three sub-phase SPECs (mirrors phase-04 state-2 commit `1d9740d`); ADR-0023 lands at this commit (05.1 Task 1). The DECISIONS.md ledger head before this commit is ADR-0022; ADR-0023 lands at the next-sequential number with no renumbering needed (no inter-ADR landings between parent-05 state-2 and this commit). Closes phase-02.1 REVIEW I3 (positive `Static` variant-name regression guard, deferred since phase-02.1 close, rolled forward unchanged through phases 02.2/03.1/03.2/04.1/04.2/04.3). Materially closes phase-04.3 REVIEW C-1 at the 05.1 state-4 phase-done verification commit (the C-1 carryforward chain originated at phase-02.2's ADR-0015 landing `435c6fa`, was discovered at phase-04.3 task 14 commit `eb6f972`, dispositioned at the phase-04.3 STATE.md handoff commit `e626862`, and ends at the 05.1 state-4 verification commit). Phase-04.1 REVIEW M-claim is unblocked but stays deferred. Implementation refinement: SPEC §3 D1 projected the new error variant on `envoy_config::ConfigError` as `ClusterDnsResolutionFailed`; the planner refined this at PLAN-write time to `envoy_cluster::ClusterError::DnsResolutionFailed` because (a) the existing `from_bootstrap` constructor lives in envoy-cluster and returns `ClusterError`, (b) SPEC §6 signpost 14 explicitly preserves envoy-cluster's typed-error chain, and (c) the alternative cross-crate wrapper-variant approach is mechanically larger; the diagnostic shape (`{cluster, address, source: std::io::Error}`) is identical, so the C-1 fix's user-facing error message is identical.

---
```

Then verify the file shape:

```bash
grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
tail -30 docs/envoy-rust/DECISIONS.md
```

Expected: ADR count `23`. The tail shows the closing of ADR-0023 + a trailing `---` separator.

- [ ] **Step 3: Write 6 failing parse-shape tests in `crates/envoy-config/src/bootstrap.rs::tests`.**

Append to the existing `#[cfg(test)] mod tests { ... }` block. The existing tests at line 1763 already include `assert!(matches!(c.cluster_type, ClusterType::Static))` at the parsing tests block; find the end of the existing tests block via `grep -n '^mod tests\|^#\[cfg(test)\] mod tests\|^}' crates/envoy-config/src/bootstrap.rs | tail -5` (the `tests` block is the last `mod tests` in the file).

Append the following 6 tests (at the end of the `tests` block, before the closing `}` of `mod tests`):

```rust
#[test]
fn parses_cluster_with_type_strict_dns() {
    // 05.1 NEW: ClusterType gains StrictDns variant. The serde tag STRICT_DNS
    // maps mechanically via the existing #[serde(rename_all = "SCREAMING_SNAKE_CASE")].
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
    let c = &bootstrap.static_resources.clusters[0];
    assert!(
        matches!(c.cluster_type, ClusterType::StrictDns),
        "expected ClusterType::StrictDns, got {:?}",
        c.cluster_type,
    );
    assert_eq!(c.name, "backend");
    assert_eq!(
        c.load_assignment.endpoints[0].lb_endpoints[0]
            .endpoint
            .address
            .socket_address
            .address,
        "localhost",
    );
}

#[test]
fn parses_cluster_with_type_static_unchanged() {
    // 05.1 NEW: regression guard — the existing STATIC parse path stays
    // unchanged after StrictDns lands. (Phase-02.1 REVIEW I3 originally
    // requested this discriminator test; the positive Static runtime test
    // lands separately in envoy-cluster as static_cluster_constructs_with_literal_ip.)
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
    let c = &bootstrap.static_resources.clusters[0];
    assert!(
        matches!(c.cluster_type, ClusterType::Static),
        "expected ClusterType::Static, got {:?}",
        c.cluster_type,
    );
}

#[test]
fn rejects_cluster_with_type_logical_dns() {
    // 05.1 NEW: documents the ADR-0023 LOGICAL_DNS deferral at the parser surface.
    // serde rejects with an "unknown variant" error naming LOGICAL_DNS. If a
    // future phase lifts the deferral, this test gets renamed to
    // parses_cluster_with_type_logical_dns and the assertion flips.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: LOGICAL_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: example.com
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject LOGICAL_DNS");
    let s = err.to_string();
    assert!(
        s.contains("LOGICAL_DNS") || s.contains("unknown variant"),
        "expected LOGICAL_DNS unknown-variant error, got: {s}",
    );
}

#[test]
fn rejects_cluster_with_unknown_type_value() {
    // 05.1 NEW: covers the deny_unknown_fields-equivalent posture on the
    // variant tag — any tag that isn't STATIC or STRICT_DNS rejects.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: WEIRD_TYPE
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let err = crate::parse_bootstrap(yaml).expect_err("must reject WEIRD_TYPE");
    let s = err.to_string();
    assert!(
        s.contains("WEIRD_TYPE") || s.contains("unknown variant"),
        "expected WEIRD_TYPE unknown-variant error, got: {s}",
    );
}

#[test]
fn parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment() {
    // 05.1 NEW: verifies that DNS-name endpoints are stored as raw strings at
    // config-parse time (resolution lands at runtime in envoy-cluster's
    // from_bootstrap, NOT at parse time). Two endpoints with the same DNS name
    // but different ports parse cleanly into the Vec<LbEndpoint>.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7001
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
    let c = &bootstrap.static_resources.clusters[0];
    assert!(matches!(c.cluster_type, ClusterType::StrictDns));
    let lbe = &c.load_assignment.endpoints[0].lb_endpoints;
    assert_eq!(lbe.len(), 2);
    assert_eq!(lbe[0].endpoint.address.socket_address.address, "localhost");
    assert_eq!(lbe[0].endpoint.address.socket_address.port_value, 7000);
    assert_eq!(lbe[1].endpoint.address.socket_address.address, "localhost");
    assert_eq!(lbe[1].endpoint.address.socket_address.port_value, 7001);
}

#[test]
fn validates_strict_dns_cluster_does_not_require_literal_ip_endpoints() {
    // 05.1 NEW: explicit assertion that envoy-config's validator passes the
    // parse stage for STRICT_DNS clusters even though the endpoint address is
    // a DNS name (not a literal IP). The runtime-side endpoint parse via
    // SocketAddr::from_str (which would fail on "host.docker.internal") lives
    // in envoy-cluster's from_bootstrap, NOT in envoy-config's validator —
    // and envoy-cluster's STRICT_DNS arm uses tokio::net::lookup_host instead
    // of SocketAddr::from_str on the StrictDns path, so the DNS-name endpoint
    // is fine end-to-end.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: host.docker.internal
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    // The validator passes the parse stage cleanly; runtime resolution is
    // out of scope for this test (envoy-cluster's from_bootstrap is not
    // invoked here).
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses + validates");
    let c = &bootstrap.static_resources.clusters[0];
    assert!(matches!(c.cluster_type, ClusterType::StrictDns));
    assert_eq!(
        c.load_assignment.endpoints[0].lb_endpoints[0]
            .endpoint
            .address
            .socket_address
            .address,
        "host.docker.internal",
    );
}
```

- [ ] **Step 4: Run the 6 new tests to verify they fail at compile time.**

```bash
cargo test --package envoy-config --lib -- bootstrap::tests::parses_cluster_with_type_strict_dns bootstrap::tests::parses_cluster_with_type_static_unchanged bootstrap::tests::rejects_cluster_with_type_logical_dns bootstrap::tests::rejects_cluster_with_unknown_type_value bootstrap::tests::parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment bootstrap::tests::validates_strict_dns_cluster_does_not_require_literal_ip_endpoints 2>&1 | head -30
```

Expected: compile error with `error[E0599]: no variant or associated item named StrictDns found for enum ClusterType` at the `matches!(c.cluster_type, ClusterType::StrictDns)` lines (3 tests reference `StrictDns`). The other 3 tests fail at runtime because `parse_bootstrap` with `type: STRICT_DNS` rejects with `unknown variant 'STRICT_DNS'` (since `Static` is the only variant).

If the tests pass unexpectedly, debug per `superpowers:systematic-debugging` — the schema may have already been extended by an unrelated change.

- [ ] **Step 5: Add the `StrictDns` variant to `ClusterType` in `crates/envoy-config/src/bootstrap.rs`.**

Replace lines 58-62:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ClusterType {
    Static,
}
```

with:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ClusterType {
    /// Static cluster type — endpoints' `address` fields are literal IPs
    /// (parsed via `SocketAddr::from_str` at cluster-build time in
    /// `envoy-cluster::from_bootstrap`).
    Static,
    /// STRICT_DNS cluster type — endpoints' `address` fields are DNS names
    /// (resolved via `tokio::net::lookup_host` at cluster-build time in
    /// `envoy-cluster::from_bootstrap`; the resolved `SocketAddr`s are
    /// cached for the cluster's lifetime, matching Envoy v1.33's STRICT_DNS
    /// semantics with default `dns_refresh_rate`). 05.1 NEW per ADR-0023;
    /// `LOGICAL_DNS` deferred to a later phase.
    StrictDns,
}
```

- [ ] **Step 6: Run the 6 new tests to verify they pass.**

```bash
cargo test --package envoy-config --lib -- bootstrap::tests::parses_cluster_with_type_strict_dns bootstrap::tests::parses_cluster_with_type_static_unchanged bootstrap::tests::rejects_cluster_with_type_logical_dns bootstrap::tests::rejects_cluster_with_unknown_type_value bootstrap::tests::parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment bootstrap::tests::validates_strict_dns_cluster_does_not_require_literal_ip_endpoints 2>&1 | tail -20
```

Expected: `test result: ok. 6 passed; 0 failed`.

If any test fails, examine the failure and fix in the schema. The 6 tests are independent (no fixture shared state), so a failure in one doesn't mask others.

- [ ] **Step 7: Run the full envoy-config test suite to confirm no regression.**

```bash
cargo test --package envoy-config 2>&1 | tail -20
```

Expected: all envoy-config tests pass (the existing test count + 6 new tests).

If a pre-existing test fails (e.g., one of the existing parse-tests at lines around 1760+ that constructs a `Cluster` value by hand and may need an explicit `ClusterType::Static` variant to disambiguate now that two variants exist), inspect the failure. The pre-existing test at line 1763 (`assert!(matches!(c.cluster_type, ClusterType::Static))`) should pass unchanged — `matches!` against `ClusterType::Static` continues to match a `Static` variant whether or not other variants exist.

- [ ] **Step 8: Create the new fuzz seed `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml`.**

```bash
mkdir -p crates/envoy-config/fuzz/corpus/parse_bootstrap
```

Write `crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml`:

```yaml
node:
  id: fuzz-test
  cluster: fuzz-test
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
static_resources:
  listeners:
    - name: tcp_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: 10000
      filter_chains:
        - filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
```

Note: the seed uses `localhost` (universally resolvable on any developer machine + CI; not Docker-host-dependent) per SPEC §3 D1's choice. The fuzz target only exercises the parser + validator, NOT the runtime; `lookup_host` is never called from the fuzz harness.

- [ ] **Step 9: Append the new seed to the fuzz `.gitignore` allow-list.**

`crates/envoy-config/fuzz/.gitignore` currently lists 11 seed allowlist entries. Append one line at the end:

```
!corpus/parse_bootstrap/strict_dns_cluster.yaml
```

Verify:

```bash
tail -3 crates/envoy-config/fuzz/.gitignore
```

Expected: the last 3 lines list `route_with_header_matchers.yaml` (or whichever was the previous tail entry), `hcm_route_to_cluster.yaml`, and the new `strict_dns_cluster.yaml`.

- [ ] **Step 10: Run a short-budget fuzz run to confirm the new seed parses cleanly.**

```bash
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=15 2>&1 | tail -20
```

Expected: the run starts, reads the corpus including `strict_dns_cluster.yaml`, and reports `Done <N> runs` after ~15s with no crashes. The seed is a valid parse-target; no `panicked` lines should appear in the output.

If the seed crashes the parser, debug per `superpowers:systematic-debugging` — likely a typo in the YAML or a missing required field.

- [ ] **Step 11: Create `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md` with the Task 1 section.**

```markdown
# Phase 05.1 Progress

## Task 1 — envoy-config: ClusterType::StrictDns + ADR-0023 + 6 validator tests + fuzz seed (2026-MM-DD)

**Commit:** `<SHA>`

**Change summary.** Lands ADR-0023 (`ClusterType::StrictDns` accepted; `LOGICAL_DNS` deferred) inline at this Task 1 commit per SPEC §7. Extends `crates/envoy-config/src/bootstrap.rs::ClusterType` from single-variant `Static` (lines 60-62 at HEAD `e626862`) to two-variant `Static | StrictDns`. Appends 6 unit tests to `bootstrap::tests` covering: (1) `STRICT_DNS` parse + variant-match; (2) `STATIC` parse-path regression-guard (unchanged); (3) `LOGICAL_DNS` rejection with `"unknown variant"` error documenting the ADR-0023 deferral; (4) unknown-tag rejection (`WEIRD_TYPE`); (5) multi-endpoint `STRICT_DNS` load-assignment shape; (6) `STRICT_DNS` cluster with a DNS-name endpoint passes the validator stage cleanly. Adds 1 new fuzz corpus seed (`crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml`; full bootstrap with one `STRICT_DNS` cluster whose endpoint resolves to `localhost`); appends one `.gitignore` allow-list entry. Total: ~145 LoC (15 schema + 80 unit tests + 25 fuzz seed YAML + 1 .gitignore + 25 ADR).

**Verification tail.**

```
$ cargo test --package envoy-config 2>&1 | tail -3
test result: ok. <N+6> passed; 0 failed; 0 ignored

$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=15 2>&1 | tail -3
Done <N> runs in 15 second(s)
```

**Deviations from PLAN.** Signpost A applied at Task 1 — ADR-0023's prose in DECISIONS.md uses `ClusterError::DnsResolutionFailed` (NOT `ConfigError::ClusterDnsResolutionFailed` as projected in SPEC §3 D1) per planner-time refinement. Implementation lands the variant on envoy-cluster's `ClusterError` at Task 2, not on envoy-config's `ConfigError` (which is unchanged in 05.1). Reasoning: SPEC §3 D2 pseudocode mixed both error types in the same `?` chain, which is mechanically inconsistent; SPEC §6 signpost 14 preserves envoy-cluster's typed-error chain unchanged; the simpler placement is on `ClusterError` where the DNS resolution code lives. ADR-0023's diagnostic shape (`{cluster, address, source: std::io::Error}`) is identical regardless of which enum carries the variant. PROGRESS.md Task 2 notes the variant landing location.
```

- [ ] **Step 12: Verify file shapes + run the four state-3 in-the-loop gates.**

```bash
cargo build --workspace --all-targets 2>&1 | tail -5
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -5
cargo fmt --all -- --check 2>&1 | tail -5
cargo test --package envoy-config 2>&1 | tail -3
```

Expected: all four pass clean. `cargo build` reports no errors/warnings; `cargo clippy` reports no `error[clippy::*]` lines; `cargo fmt` exits 0 (no formatting drift); `cargo test --package envoy-config` reports `test result: ok. <N+6> passed`.

- [ ] **Step 13: Commit Task 1.**

```bash
git add docs/envoy-rust/DECISIONS.md \
        crates/envoy-config/src/bootstrap.rs \
        crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml \
        crates/envoy-config/fuzz/.gitignore \
        docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md

git commit -m "$(cat <<'EOF'
phase 05.1: ClusterType::StrictDns + ADR-0023 + 6 validator tests + fuzz seed

Lands ADR-0023 inline at this Task 1 commit per SPEC §7. Extends
crates/envoy-config/src/bootstrap.rs::ClusterType from single-variant
Static (lines 60-62 at HEAD e626862) to two-variant Static | StrictDns.

Appends 6 unit tests to bootstrap::tests:
- parses_cluster_with_type_strict_dns
- parses_cluster_with_type_static_unchanged (regression guard)
- rejects_cluster_with_type_logical_dns (documents ADR-0023 deferral)
- rejects_cluster_with_unknown_type_value
- parses_cluster_with_type_strict_dns_with_multi_endpoint_load_assignment
- validates_strict_dns_cluster_does_not_require_literal_ip_endpoints

Adds 1 new fuzz corpus seed
(crates/envoy-config/fuzz/corpus/parse_bootstrap/strict_dns_cluster.yaml;
full bootstrap with one STRICT_DNS cluster + localhost endpoint) plus the
matching .gitignore allow-list entry.

NO HTTP/2 work in 05.1. Per ADR-0022 (parent-05 split) the H2 codec,
HCM-on-H2 dispatch, h2spec gate, fixtures 0009/0010, and upstream H2
client all defer to sub-phases 05.2 and 05.3.

Planner-time signpost: ADR-0023's prose uses ClusterError::DnsResolutionFailed
(not ConfigError::ClusterDnsResolutionFailed as projected in SPEC §3 D1)
per the cross-statement ambiguity resolution documented in PLAN.md
signpost A. Implementation of the variant lands at Task 2 on envoy-cluster.
EOF
)"
```

Verify:

```bash
git log -1 --oneline
git status
```

Expected: commit message starts with `phase 05.1: ClusterType::StrictDns`; working tree clean.

---

### Task 2: `envoy-cluster` — `tokio` direct dep + async `from_bootstrap` + `ClusterError::DnsResolutionFailed` + STRICT_DNS resolution branch + 3 new tests + 5 existing-test `.await` updates + envoy-bin call-site `.await`

**Files:**
- Modify: `crates/envoy-cluster/Cargo.toml` (add `tokio = { version = "1", features = ["net", "rt", "macros"] }` to `[dependencies]`; add `tokio = { version = "1", features = ["macros", "rt"] }` to `[dev-dependencies]` for `#[tokio::test]` flavor).
- Modify: `crates/envoy-cluster/src/cluster.rs` (extend `ClusterError` enum at lines 95-107 with `DnsResolutionFailed { cluster, address, source: std::io::Error }`; promote `from_bootstrap` at line 112 from `pub fn` to `pub async fn`; restructure the per-cluster endpoint-build loop at lines 120-135 with a `match cfg.cluster_type` two-arm dispatch; append 3 new unit tests; update 5 existing tests to add `.await`).
- Modify: `crates/envoy-bin/src/main.rs` (line 83: add `.await` to the `from_bootstrap` call).
- Modify: `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md` (append Task 2 section).

**Why second:** Task 2 consumes Task 1's `ClusterType::StrictDns` variant at the `match cluster_def.cluster_type` arm. Task 2 also closes phase-02.1 REVIEW I3 via the new `static_cluster_constructs_with_literal_ip` test (a positive `Static` regression guard that wasn't writable before Task 1's second variant existed). Task 3's fixture YAMLs would crash the runtime even with the new `STRICT_DNS` tag accepted by the parser if Task 2 didn't land first — the existing literal-IP `parse::<SocketAddr>()` path at line 125 would reject `host.docker.internal` with `ClusterError::EndpointParse`.

**Scope.** ~5 LoC Cargo.toml + ~7 LoC `ClusterError` variant + ~50 LoC runtime delta (the new `match` arm + the async promotion + the `lookup_host` resolve loop with zero-result guard) + ~50 LoC new tests (3 tests × ~17 LoC each) + ~5 LoC existing-test `.await` updates + ~1 token envoy-bin call-site update. Total ~120 LoC.

- [ ] **Step 1: Verify the current `from_bootstrap` shape + envoy-bin call site + Cargo.toml current deps.**

```bash
grep -n 'pub fn from_bootstrap\|pub enum ClusterError\|pub async fn from_bootstrap' crates/envoy-cluster/src/cluster.rs
grep -n 'envoy_cluster::from_bootstrap' crates/envoy-bin/src/main.rs
cat crates/envoy-cluster/Cargo.toml
```

Expected: `from_bootstrap` is `pub fn` (sync) at line 112; `ClusterError` is `pub enum` at line 95. envoy-bin's call site is at line 83 of `main.rs`. Cargo.toml lists only `envoy-config = { path = "../envoy-config" }` and `thiserror = "2"` under `[dependencies]`; no tokio dep present (confirms signpost B).

If the shape differs (e.g., `from_bootstrap` is already async, or tokio is already a direct dep on envoy-cluster), invoke `superpowers:systematic-debugging` first — the SPEC's HEAD `e626862` baseline may have drifted since PLAN-write.

- [ ] **Step 2: Write 3 failing unit tests in `crates/envoy-cluster/src/cluster.rs::tests`.**

The tests reference `ClusterError::DnsResolutionFailed`, `tokio::net::lookup_host`, and `#[tokio::test]` — all of which fail at compile time before the implementation lands.

Append to the existing `#[cfg(test)] mod tests { ... }` block (find the closing `}` of `mod tests` near line 491; append before it):

```rust
#[tokio::test]
async fn static_cluster_constructs_with_literal_ip() {
    // 05.1 NEW (closes phase-02.1 REVIEW I3): positive Static regression guard.
    // Was un-writable before phase 05.1 because ClusterType had only one variant
    // (`Static`); with `StrictDns` now landing in 05.1 the `match cluster_type`
    // arm is structurally meaningful, so the Static path is exercised here as
    // an explicit guard against accidental schema/runtime regressions.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: 127.0.0.1
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
    let mgr = crate::from_bootstrap(&bootstrap)
        .await
        .expect("Static cluster constructs cleanly");
    let handle = mgr.get("backend").expect("cluster present");
    let picked = handle.pick_endpoint().expect("non-empty");
    assert_eq!(picked, "127.0.0.1:7000".parse::<SocketAddr>().unwrap());
}

#[tokio::test]
async fn strict_dns_cluster_resolves_localhost_at_build_time() {
    // 05.1 NEW: STRICT_DNS resolves a DNS name at cluster-build time via
    // tokio::net::lookup_host. `localhost` is universally resolvable across
    // dev/CI (loopback-bound; no network dependency); see PLAN.md signpost D.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: localhost
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
    let mgr = crate::from_bootstrap(&bootstrap)
        .await
        .expect("STRICT_DNS cluster resolves localhost cleanly");
    let handle = mgr.get("backend").expect("cluster present");
    let picked = handle.pick_endpoint().expect("non-empty");
    assert_eq!(
        picked.port(),
        7000,
        "resolved endpoint should preserve configured port",
    );
    assert!(
        picked.ip().is_loopback(),
        "localhost should resolve to loopback (127.0.0.1 or ::1), got {picked:?}",
    );
}

#[tokio::test]
async fn strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain() {
    // 05.1 NEW: NXDOMAIN-equivalent path returns ClusterError::DnsResolutionFailed
    // with the diagnostic fields populated. `.invalid` TLD is RFC 6761 §6.4
    // reserved as non-resolvable (PLAN.md signpost E). If CI flakes due to a
    // misconfigured resolver synthesizing a positive answer, fall back to the
    // empty-host case per signpost E's documented escape hatch.
    let yaml = r#"
static_resources:
  listeners: []
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address:
                    socket_address:
                      address: this-host-does-not-exist.invalid
                      port_value: 7000
admin:
  address:
    socket_address:
      address: 127.0.0.1
      port_value: 9901
"#;
    let bootstrap = envoy_config::parse_bootstrap(yaml).expect("valid");
    let err = crate::from_bootstrap(&bootstrap)
        .await
        .expect_err("STRICT_DNS resolution of .invalid TLD must fail");
    assert!(
        matches!(
            err,
            ClusterError::DnsResolutionFailed {
                ref cluster,
                ref address,
                ..
            } if cluster == "backend" && address == "this-host-does-not-exist.invalid"
        ),
        "expected DnsResolutionFailed{{cluster:'backend',address:'this-host-does-not-exist.invalid',..}}, got {err:?}",
    );
}
```

- [ ] **Step 3: Update the 5 existing tests that call `crate::from_bootstrap` to add `.await`.**

The existing tests at the following lines call `crate::from_bootstrap(&bootstrap)`:

- Line 281: `from_bootstrap_builds_single_endpoint_cluster`
- Line 322: `from_bootstrap_builds_three_endpoint_cluster`
- Line 367-368: `from_bootstrap_rejects_empty_cluster`
- Line 419-420: `from_bootstrap_rejects_duplicate_cluster_name`
- Line 480-481: `from_bootstrap_rejects_malformed_endpoint_address`

Each is a `#[test]` synchronous test. They become `#[tokio::test] async` and add `.await` to the `crate::from_bootstrap` call. Concretely:

For each of the 5 tests, change `#[test]` to `#[tokio::test]` and `fn` to `async fn`, then add `.await` immediately after `crate::from_bootstrap(&bootstrap)` (before `.expect(...)` or `.expect_err(...)`).

Example transformation for `from_bootstrap_builds_single_endpoint_cluster`:

```rust
// BEFORE:
#[test]
fn from_bootstrap_builds_single_endpoint_cluster() {
    let bootstrap = envoy_config::parse_bootstrap(SINGLE_ENDPOINT_YAML).expect("valid");
    let mgr = crate::from_bootstrap(&bootstrap).expect("construct");
    // ...
}

// AFTER:
#[tokio::test]
async fn from_bootstrap_builds_single_endpoint_cluster() {
    let bootstrap = envoy_config::parse_bootstrap(SINGLE_ENDPOINT_YAML).expect("valid");
    let mgr = crate::from_bootstrap(&bootstrap).await.expect("construct");
    // ...
}
```

Apply the same transformation to the other 4 tests at the lines listed above.

- [ ] **Step 4: Run the 8 tests (3 new + 5 updated) to verify they fail at compile time.**

```bash
cargo test --package envoy-cluster --lib 2>&1 | head -30
```

Expected: compile errors. Multiple errors expected: (a) `error[E0599]: no variant or associated item named DnsResolutionFailed found for enum ClusterError` from the new test 3; (b) `error[E0277]: ... is not a future` or similar from `.await` on a non-async function (the existing `from_bootstrap` is sync); (c) `error[E0433]: failed to resolve: use of undeclared crate or module 'tokio'` from the `#[tokio::test]` attribute (since tokio isn't yet a dev-dep).

If the tests pass unexpectedly, debug per `superpowers:systematic-debugging`.

- [ ] **Step 5: Add `tokio` direct dep + dev-dep to `crates/envoy-cluster/Cargo.toml`.**

Replace the file contents with:

```toml
[package]
name = "envoy-cluster"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_cluster"
path = "src/lib.rs"

[dependencies]
envoy-config = { path = "../envoy-config" }
thiserror = "2"
tokio = { version = "1", features = ["net", "rt", "macros"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
```

Note: the dev-dep entry intentionally restates `tokio` (with the macros + rt features) for `#[tokio::test]`; the runtime dep entry covers the runtime needs (`net` for `lookup_host`, `rt` for the async runtime references in async-fn signatures). Cargo's feature unification merges them automatically.

- [ ] **Step 6: Add `ClusterError::DnsResolutionFailed` variant to `crates/envoy-cluster/src/cluster.rs`.**

Replace lines 94-107 (the `ClusterError` enum block):

```rust
/// Errors returned by `from_bootstrap`.
///
/// `EmptyCluster` and `DuplicateClusterName` are defense-in-depth: the
/// `envoy-config` validator also rejects these shapes (`EmptyClusterEndpoints`,
/// cluster-name collisions via per-cluster `UnknownCluster` checks). They exist
/// here because `envoy-cluster` is a library whose invariants must hold even
/// when callers construct `Bootstrap` values by hand.
///
/// `EndpointParse` is *not* defense-in-depth: `envoy-config` accepts any
/// serde-valid `SocketAddress { address: String, port_value: u16 }` shape
/// (including `"not-a-host"`); the `SocketAddr` parse is the first place that
/// rejects a malformed address.
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("cluster '{name}' has no lb_endpoints")]
    EmptyCluster { name: String },
    #[error("duplicate cluster name '{name}'")]
    DuplicateClusterName { name: String },
    #[error("cluster '{cluster}' endpoint address {addr:?} is not a valid SocketAddr: {source}")]
    EndpointParse {
        cluster: String,
        addr: String,
        #[source]
        source: std::net::AddrParseError,
    },
}
```

with:

```rust
/// Errors returned by `from_bootstrap`.
///
/// `EmptyCluster` and `DuplicateClusterName` are defense-in-depth: the
/// `envoy-config` validator also rejects these shapes (`EmptyClusterEndpoints`,
/// cluster-name collisions via per-cluster `UnknownCluster` checks). They exist
/// here because `envoy-cluster` is a library whose invariants must hold even
/// when callers construct `Bootstrap` values by hand.
///
/// `EndpointParse` is *not* defense-in-depth: `envoy-config` accepts any
/// serde-valid `SocketAddress { address: String, port_value: u16 }` shape
/// (including `"not-a-host"`); the `SocketAddr` parse is the first place that
/// rejects a malformed address. Reached only on the `Static` cluster-type arm;
/// the `StrictDns` arm uses `tokio::net::lookup_host` instead and surfaces
/// resolution failure via `DnsResolutionFailed`.
///
/// `DnsResolutionFailed` is the runtime counterpart of `EndpointParse` for
/// `STRICT_DNS` clusters: the configured `address` is a DNS name (not a
/// literal IP), and `tokio::net::lookup_host` either errored or returned zero
/// addresses. Per ADR-0023, `STRICT_DNS` resolves once at cluster-build time
/// and caches the result; periodic re-resolution defers to a future phase.
#[derive(Debug, thiserror::Error)]
pub enum ClusterError {
    #[error("cluster '{name}' has no lb_endpoints")]
    EmptyCluster { name: String },
    #[error("duplicate cluster name '{name}'")]
    DuplicateClusterName { name: String },
    #[error("cluster '{cluster}' endpoint address {addr:?} is not a valid SocketAddr: {source}")]
    EndpointParse {
        cluster: String,
        addr: String,
        #[source]
        source: std::net::AddrParseError,
    },
    #[error("cluster '{cluster}' STRICT_DNS resolution of '{address}' failed: {source}")]
    DnsResolutionFailed {
        cluster: String,
        address: String,
        #[source]
        source: std::io::Error,
    },
}
```

- [ ] **Step 7: Promote `from_bootstrap` to async + add the STRICT_DNS resolution branch.**

Replace the entire `from_bootstrap` function at lines 109-153:

```rust
/// Constructs a `ClusterManager` from a validated `Bootstrap`. The caller
/// should have already run `envoy_config::parse_bootstrap`, but this function
/// validates its own preconditions for library robustness.
pub fn from_bootstrap(bootstrap: &envoy_config::Bootstrap) -> Result<ClusterManager, ClusterError> {
    let mut clusters: HashMap<String, Arc<Cluster>> = HashMap::new();
    for cfg in &bootstrap.static_resources.clusters {
        // envoy-config enforces cluster_type == Static, lb_policy == RoundRobin,
        // load_assignment.cluster_name == cfg.name, and total endpoints ≥ 1 at
        // parse time. We don't re-check those here; we do re-check emptiness
        // and duplicate names as defense-in-depth, and we parse each address
        // (which envoy-config does NOT do).
        let mut endpoints: Vec<SocketAddr> = Vec::new();
        for locality in &cfg.load_assignment.endpoints {
            for lbe in &locality.lb_endpoints {
                let sa = &lbe.endpoint.address.socket_address;
                let addr_str = format!("{}:{}", sa.address, sa.port_value);
                let parsed: SocketAddr =
                    addr_str
                        .parse()
                        .map_err(|source| ClusterError::EndpointParse {
                            cluster: cfg.name.clone(),
                            addr: addr_str.clone(),
                            source,
                        })?;
                endpoints.push(parsed);
            }
        }
        if endpoints.is_empty() {
            return Err(ClusterError::EmptyCluster {
                name: cfg.name.clone(),
            });
        }
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints,
            cursor: AtomicUsize::new(0),
        });
        if clusters.insert(cfg.name.clone(), cluster).is_some() {
            return Err(ClusterError::DuplicateClusterName {
                name: cfg.name.clone(),
            });
        }
    }
    Ok(ClusterManager { clusters })
}
```

with:

```rust
/// Constructs a `ClusterManager` from a validated `Bootstrap`. The caller
/// should have already run `envoy_config::parse_bootstrap`, but this function
/// validates its own preconditions for library robustness.
///
/// Async since 05.1: `STRICT_DNS` clusters call `tokio::net::lookup_host`
/// (which is async). `STATIC` clusters don't await any I/O — the parse path
/// stays unchanged from phase 02.1 — but the function signature is uniformly
/// async because Rust doesn't have a "conditionally async" mechanism. The
/// single envoy-bin caller (`crates/envoy-bin/src/main.rs`) awaits this once
/// at startup, before serving any traffic.
pub async fn from_bootstrap(
    bootstrap: &envoy_config::Bootstrap,
) -> Result<ClusterManager, ClusterError> {
    let mut clusters: HashMap<String, Arc<Cluster>> = HashMap::new();
    for cfg in &bootstrap.static_resources.clusters {
        // envoy-config enforces cluster_type ∈ {Static, StrictDns} (post-05.1),
        // lb_policy == RoundRobin, load_assignment.cluster_name == cfg.name,
        // and total endpoints ≥ 1 at parse time. We don't re-check those here;
        // we do re-check emptiness and duplicate names as defense-in-depth,
        // and we resolve each endpoint to a SocketAddr (which envoy-config
        // does NOT do — neither the literal-IP parse for STATIC nor the DNS
        // lookup for STRICT_DNS).
        let mut endpoints: Vec<SocketAddr> = Vec::new();
        for locality in &cfg.load_assignment.endpoints {
            for lbe in &locality.lb_endpoints {
                let sa = &lbe.endpoint.address.socket_address;
                match cfg.cluster_type {
                    envoy_config::ClusterType::Static => {
                        // EXISTING path (phase 02.1): each endpoint's address
                        // parses as a literal SocketAddr via SocketAddr::from_str.
                        // Failure surfaces as ClusterError::EndpointParse —
                        // regression-guarded by the I3-closing test
                        // static_cluster_constructs_with_literal_ip.
                        let addr_str = format!("{}:{}", sa.address, sa.port_value);
                        let parsed: SocketAddr = addr_str
                            .parse()
                            .map_err(|source| ClusterError::EndpointParse {
                                cluster: cfg.name.clone(),
                                addr: addr_str.clone(),
                                source,
                            })?;
                        endpoints.push(parsed);
                    }
                    envoy_config::ClusterType::StrictDns => {
                        // 05.1 NEW per ADR-0023: each endpoint's address is a
                        // DNS name; resolve via tokio::net::lookup_host at
                        // cluster-build time. The lookup runs once; results
                        // cached for the cluster's lifetime, matching Envoy
                        // v1.33 STRICT_DNS semantics with default
                        // dns_refresh_rate (periodic re-resolution defers per
                        // parent-05 SPEC §4 / 05.1 SPEC §4).
                        let target = format!("{}:{}", sa.address, sa.port_value);
                        let resolved: Vec<SocketAddr> = tokio::net::lookup_host(&target)
                            .await
                            .map_err(|source| ClusterError::DnsResolutionFailed {
                                cluster: cfg.name.clone(),
                                address: sa.address.clone(),
                                source,
                            })?
                            .collect();
                        if resolved.is_empty() {
                            // Defensive zero-result guard: lookup_host can
                            // return an empty iterator on success on some
                            // platforms (e.g., NXDOMAIN may surface as empty
                            // rather than as an io::Error). Synthesise an
                            // io::Error so DnsResolutionFailed.source carries
                            // diagnostic info uniformly.
                            return Err(ClusterError::DnsResolutionFailed {
                                cluster: cfg.name.clone(),
                                address: sa.address.clone(),
                                source: std::io::Error::new(
                                    std::io::ErrorKind::NotFound,
                                    "DNS resolution returned zero addresses",
                                ),
                            });
                        }
                        endpoints.extend(resolved);
                    }
                }
            }
        }
        if endpoints.is_empty() {
            return Err(ClusterError::EmptyCluster {
                name: cfg.name.clone(),
            });
        }
        let cluster = Arc::new(Cluster {
            name: cfg.name.clone(),
            endpoints,
            cursor: AtomicUsize::new(0),
        });
        if clusters.insert(cfg.name.clone(), cluster).is_some() {
            return Err(ClusterError::DuplicateClusterName {
                name: cfg.name.clone(),
            });
        }
    }
    Ok(ClusterManager { clusters })
}
```

- [ ] **Step 8: Run the 8 tests (3 new + 5 updated) to verify they pass.**

```bash
cargo test --package envoy-cluster --lib 2>&1 | tail -20
```

Expected: `test result: ok. <existing-count + 3> passed; 0 failed`. Note the 5 existing tests count is unchanged (they're updated, not added); the new tests bring the count up by 3.

If `strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain` flakes (a misconfigured resolver synthesising a positive answer for `.invalid`), per signpost E switch the test target to a guaranteed-malformed string. The simplest fallback: change `address: this-host-does-not-exist.invalid` to `address: ` (empty string with a leading space — still serde-valid but `lookup_host` fails reliably with `io::Error` of kind `InvalidInput`). Document the fallback in PROGRESS.md.

- [ ] **Step 9: Update envoy-bin's `from_bootstrap` call site to add `.await`.**

`crates/envoy-bin/src/main.rs` line 83 currently reads:

```rust
let cluster_mgr = std::sync::Arc::new(
    envoy_cluster::from_bootstrap(&bootstrap).context("building cluster manager")?,
);
```

Change to:

```rust
let cluster_mgr = std::sync::Arc::new(
    envoy_cluster::from_bootstrap(&bootstrap).await.context("building cluster manager")?,
);
```

(Single token added: `.await` between `from_bootstrap(&bootstrap)` and `.context(...)`.)

Verify:

```bash
grep -n 'envoy_cluster::from_bootstrap' crates/envoy-bin/src/main.rs
```

Expected: returns `83:        envoy_cluster::from_bootstrap(&bootstrap).await.context("building cluster manager")?,` (or similar formatting).

- [ ] **Step 10: Run the full envoy-bin build + test to confirm the call-site update propagates cleanly.**

```bash
cargo build --package envoy-bin 2>&1 | tail -5
cargo test --package envoy-bin --lib 2>&1 | tail -10
```

Expected: `cargo build` clean (no errors). The `cargo test --package envoy-bin --lib` exercises the library-side tests of envoy-bin (admin/argv/etc.) without spawning subprocesses; it should pass cleanly since the change is purely at the runtime startup path.

If the build fails with `error[E0277]: ... is not a future` at line 83, the `.await` was missed. If it fails with `error[E0599]: method 'await' not found`, the function-signature change in Step 7 may not have landed correctly — re-verify the `pub async fn` qualifier.

- [ ] **Step 11: Run the full workspace build + clippy + fmt + test.**

```bash
cargo build --workspace --all-targets 2>&1 | tail -10
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -10
cargo fmt --all -- --check 2>&1 | tail -5
cargo test --workspace 2>&1 | tail -20
```

Expected: all four pass clean. The `cargo test --workspace` exercises every workspace crate's tests (envoy-config + envoy-cluster + envoy-tcp + envoy-tls + envoy-listener + envoy-http1 + envoy-bin + tests/differential + tests/helpers); the only changes that affect non-envoy-cluster crates are the envoy-bin call-site `.await` (which is exercised at build time, not test time).

Possible failure modes:
- A clippy lint on the new code (e.g., `clippy::let_and_return` or `clippy::needless_collect`) — fix in-place per the lint suggestion. The `.collect::<Vec<_>>()` on the `lookup_host` return is intentional (we need the Vec for `extend` + the zero-check); if clippy flags it, suppress with `#[allow(clippy::needless_collect)]` and a one-line comment explaining the intent.
- A fmt diff on the new function body — re-run `cargo fmt --all` and `git add -A` before re-running `cargo fmt --all -- --check`.
- A pre-existing test in another crate that depends on the sync `from_bootstrap` signature (unlikely — only envoy-bin calls it directly) — investigate via `grep -rn 'envoy_cluster::from_bootstrap' crates/ tests/` and fix the call site by adding `.await`.

- [ ] **Step 12: Append Task 2 section to `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md`.**

```markdown
## Task 2 — envoy-cluster: tokio dep + async from_bootstrap + ClusterError::DnsResolutionFailed + STRICT_DNS branch + 3 new tests + I3 close-out (2026-MM-DD)

**Commit:** `<SHA>`

**Change summary.** Promotes `crates/envoy-cluster/src/cluster.rs::from_bootstrap` from `pub fn` to `pub async fn`; adds a `STRICT_DNS` resolution branch via `tokio::net::lookup_host` (resolves once at cluster-build time per ADR-0023; results cached for cluster lifetime). Extends `ClusterError` with one new variant `DnsResolutionFailed { cluster: String, address: String, source: std::io::Error }`. Adds `tokio = { version = "1", features = ["net", "rt", "macros"] }` to envoy-cluster's `[dependencies]` (was previously absent — `tokio` is already a top-level dep on other workspace crates so no new transitive license surfaces). Adds `tokio = { version = "1", features = ["macros", "rt"] }` to `[dev-dependencies]` for `#[tokio::test]` flavor. Appends 3 new unit tests (`static_cluster_constructs_with_literal_ip` — closes phase-02.1 REVIEW I3; `strict_dns_cluster_resolves_localhost_at_build_time`; `strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain`). Updates 5 existing tests (`from_bootstrap_builds_single_endpoint_cluster`, `from_bootstrap_builds_three_endpoint_cluster`, `from_bootstrap_rejects_empty_cluster`, `from_bootstrap_rejects_duplicate_cluster_name`, `from_bootstrap_rejects_malformed_endpoint_address`) to `#[tokio::test] async fn` + `.await` on the `from_bootstrap` call (mechanical; ~5 LoC churn). Updates the single envoy-bin call site at `crates/envoy-bin/src/main.rs:83` with one `.await` token. Total: ~120 LoC.

**Phase-02.1 REVIEW I3 closes** at this commit. The carryforward chain (phase-02.1 → 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 → 05.1) ends here. The new positive `Static` regression guard `static_cluster_constructs_with_literal_ip` is structurally meaningful only because Task 1 added the second `ClusterType` variant.

**Verification tail.**

```
$ cargo test --package envoy-cluster --lib 2>&1 | tail -3
test result: ok. <N+3> passed; 0 failed; 0 ignored

$ cargo test --workspace 2>&1 | tail -3
test result: ok. <total> passed; 0 failed; 0 ignored

$ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
[clean]

$ cargo fmt --all -- --check
[clean — exit 0]
```

**Deviations from PLAN.** Signpost B applied — `tokio` is a NEW direct dep on envoy-cluster (was previously absent). The SPEC §3 D2 cross-crate dependency note suggesting tokio was already pulled is incorrect at HEAD `e626862`; the planner discovered the discrepancy at PLAN-write time and adds the dep here. No new top-level workspace dep (tokio is already on envoy-bin/envoy-listener/envoy-tcp/envoy-http1/envoy-tls/etc. with the `net` feature already activated in the workspace's resolved feature set). Signpost A applied — the new error variant lands on `ClusterError::DnsResolutionFailed`, NOT `ConfigError::ClusterDnsResolutionFailed` per the cross-statement ambiguity resolution in PLAN.md signpost A. Diagnostic shape unchanged.
```

- [ ] **Step 13: Commit Task 2.**

```bash
git add crates/envoy-cluster/Cargo.toml \
        crates/envoy-cluster/src/cluster.rs \
        crates/envoy-bin/src/main.rs \
        docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md

git commit -m "$(cat <<'EOF'
phase 05.1: tokio dep + async from_bootstrap + STRICT_DNS branch + I3 close

Promotes crates/envoy-cluster/src/cluster.rs::from_bootstrap from
pub fn to pub async fn. Adds a STRICT_DNS resolution branch via
tokio::net::lookup_host; resolves once at cluster-build time per
ADR-0023, results cached for cluster lifetime, matching Envoy v1.33
STRICT_DNS semantics with default dns_refresh_rate (periodic
re-resolution deferred per SPEC §4).

Extends ClusterError with one new variant:
- DnsResolutionFailed { cluster, address, source: std::io::Error }

Adds tokio = { version = "1", features = ["net", "rt", "macros"] }
to envoy-cluster's [dependencies] (previously absent — see PLAN.md
signpost B). Adds tokio dev-dep for #[tokio::test] flavor. No new
top-level workspace dep; tokio is already pulled by other crates.

Appends 3 new tests:
- static_cluster_constructs_with_literal_ip (closes phase-02.1 REVIEW I3)
- strict_dns_cluster_resolves_localhost_at_build_time
- strict_dns_cluster_returns_dns_resolution_failed_on_nxdomain

Updates 5 existing tests to #[tokio::test] async fn + .await on the
from_bootstrap call (mechanical churn).

Updates envoy-bin's call site at main.rs:83 with one .await token.

Phase-02.1 REVIEW I3 closes at this commit. The multi-phase
carryforward chain (02.1 → 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3
→ 05.1) ends here. The new positive Static regression guard is
structurally meaningful only because Task 1 added the second
ClusterType variant in DECISIONS.md ADR-0023.
EOF
)"
```

Verify:

```bash
git log -1 --oneline
git status
```

Expected: commit message starts with `phase 05.1: tokio dep + async from_bootstrap`; working tree clean.

---

### Task 3: 5-fixture coordinated YAML edit — flip `type: STATIC` → `type: STRICT_DNS` on `tests/fixtures/{0003,0004,0005,0006,0008}/{envoy.yaml,envoy-rust.yaml}`

**Files:** 10 YAML files modified in lockstep — bundled into one commit per SPEC §6 signpost 8.

- Modify: `tests/fixtures/0003-tcp-proxy/envoy.yaml` (line 27).
- Modify: `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml` (line 21).
- Modify: `tests/fixtures/0004-tls-downstream/envoy.yaml` (line 37).
- Modify: `tests/fixtures/0004-tls-downstream/envoy-rust.yaml` (line 31).
- Modify: `tests/fixtures/0005-tls-upstream/envoy.yaml` (line 16).
- Modify: `tests/fixtures/0005-tls-upstream/envoy-rust.yaml` (line 15).
- Modify: `tests/fixtures/0006-tls-sni/envoy.yaml` (line 40).
- Modify: `tests/fixtures/0006-tls-sni/envoy-rust.yaml` (line 39).
- Modify: `tests/fixtures/0008-http1-router-upstream/envoy.yaml` (line 49).
- Modify: `tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml` (line 27).
- Modify: `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md` (append Task 3 section).

**Why third:** the YAMLs become valid only after Tasks 1 + 2 land — before Task 1 the parser rejects `type: STRICT_DNS` (`unknown variant`); before Task 2 the runtime crashes at endpoint-build time even with the new tag accepted (the existing literal-IP `parse::<SocketAddr>()` path at the old line 125 would reject `host.docker.internal`).

**Scope.** ~10 LoC of YAML diff total (10 files × 1 line each — `type: STATIC` → `type: STRICT_DNS`; the strings are identical-length so no whitespace re-indent is needed). One bundled commit per SPEC §6 signpost 8 + PLAN.md signpost G.

- [ ] **Step 1: Confirm the current `type: STATIC` lines on all 10 files.**

```bash
grep -n "type: STATIC" tests/fixtures/0003-tcp-proxy/*.yaml tests/fixtures/0004-tls-downstream/*.yaml tests/fixtures/0005-tls-upstream/*.yaml tests/fixtures/0006-tls-sni/*.yaml tests/fixtures/0008-http1-router-upstream/*.yaml
```

Expected: 10 lines, one per file, matching the line numbers in the **Files** list above (0003 envoy.yaml:27 + envoy-rust.yaml:21; 0004 envoy.yaml:37 + envoy-rust.yaml:31; 0005 envoy.yaml:16 + envoy-rust.yaml:15; 0006 envoy.yaml:40 + envoy-rust.yaml:39; 0008 envoy.yaml:49 + envoy-rust.yaml:27).

If the line numbers differ, the fixtures may have drifted since PLAN-write time — the planner re-checks the actual line numbers via `grep -n "type: STATIC"` and adjusts the edits in Step 2 accordingly.

Confirm fixtures 0001/0002/0007 don't reference `host.docker.internal`:

```bash
grep -L 'host.docker.internal\|BACKEND_HOST' tests/fixtures/0001-tcp-echo/*.yaml tests/fixtures/0002-static-admin-ready/*.yaml tests/fixtures/0007-http1-direct-response/*.yaml
```

Expected: returns the paths of all matching files (i.e., all of them — `-L` lists files that DON'T match the pattern). The 3 unaffected fixtures don't reference `host.docker.internal` or the `BACKEND_HOST` substitution at any cluster.

- [ ] **Step 2: Apply the 10 edits.**

For each of the 10 files, use the Edit tool (or `sed -i` if the planner prefers, but Edit is more auditable per the project's tool-preference doctrine):

For `tests/fixtures/0003-tcp-proxy/envoy.yaml` (line 27):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0003-tcp-proxy/envoy-rust.yaml` (line 21):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0004-tls-downstream/envoy.yaml` (line 37):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0004-tls-downstream/envoy-rust.yaml` (line 31):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0005-tls-upstream/envoy.yaml` (line 16):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0005-tls-upstream/envoy-rust.yaml` (line 15):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0006-tls-sni/envoy.yaml` (line 40):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0006-tls-sni/envoy-rust.yaml` (line 39):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0008-http1-router-upstream/envoy.yaml` (line 49):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

For `tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml` (line 27):
- old: `      type: STATIC`
- new: `      type: STRICT_DNS`

Note: each file has exactly one occurrence of `type: STATIC` (verifiable per Step 1's grep output: 10 lines total, one per file). The leading whitespace (`      ` — 6 spaces) is the YAML indent at the cluster level; the replacement preserves the same indent.

Each fixture YAML may have other `type:` lines unrelated to clusters (e.g., `type:` keys inside `typed_config: { "@type": ... }` blocks). These are NOT to be edited — the grep in Step 1 narrowly matches the `type: STATIC` literal which only appears at cluster-level.

- [ ] **Step 3: Verify the 10 edits landed cleanly.**

```bash
grep -n "type: STRICT_DNS" tests/fixtures/0003-tcp-proxy/*.yaml tests/fixtures/0004-tls-downstream/*.yaml tests/fixtures/0005-tls-upstream/*.yaml tests/fixtures/0006-tls-sni/*.yaml tests/fixtures/0008-http1-router-upstream/*.yaml
grep -n "type: STATIC" tests/fixtures/0003-tcp-proxy/*.yaml tests/fixtures/0004-tls-downstream/*.yaml tests/fixtures/0005-tls-upstream/*.yaml tests/fixtures/0006-tls-sni/*.yaml tests/fixtures/0008-http1-router-upstream/*.yaml
```

Expected: the first grep returns exactly 10 lines (one per edited file). The second grep returns empty (no remaining `type: STATIC` in any of the 5 affected fixtures).

- [ ] **Step 4: Re-run the envoy-config tests + envoy-cluster tests to confirm fixture YAML still parses cleanly.**

```bash
cargo test --package envoy-config 2>&1 | tail -3
cargo test --package envoy-cluster 2>&1 | tail -3
```

Expected: both crates' tests pass. The fixture YAMLs aren't directly exercised by these test suites, but a regression in the parser/validator would surface here first.

- [ ] **Step 5: (OPTIONAL) Run a local Docker-gated smoke check on one fixture before pushing to CI.**

This step is OPTIONAL per SPEC §3 D3 (`"No locally-verified Docker run is required at D3 task time — the substantive verification happens at D4 via the CI re-push"`). If the planner has Docker available locally, run one of the affected fixtures to catch regressions before CI:

```bash
cargo test --package differential -- --test-threads=1 tcp_proxy 2>&1 | tail -20
```

Expected: the differential test for fixture 0003 spins up an Envoy container + envoy-rust subprocess + tcp-echo-server, drives a TCP proxy round-trip, and asserts byte-equality. Pass = green. If red, the planner inspects the test output for which side rejected: if the upstream Envoy container's startup-logs show `malformed IP address` (the C-1 trace), the YAML edit didn't land on `envoy.yaml`; if envoy-rust's logs show `EndpointParse` or `DnsResolutionFailed`, the YAML edit didn't land on `envoy-rust.yaml` or the runtime branch in Task 2 has a bug.

If Docker isn't locally available, skip this step — Task 4 catches regressions via the CI push.

- [ ] **Step 6: Append Task 3 section to `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md`.**

```markdown
## Task 3 — 5-fixture coordinated YAML edit: type: STATIC → type: STRICT_DNS (2026-MM-DD)

**Commit:** `<SHA>`

**Change summary.** Coordinated 10-file YAML edit — flips `type: STATIC` to `type: STRICT_DNS` on the cluster whose endpoints reference `{{BACKEND_HOST}}` in 5 fixtures: 0003-tcp-proxy, 0004-tls-downstream, 0005-tls-upstream, 0006-tls-sni, 0008-http1-router-upstream. Both `envoy.yaml` and `envoy-rust.yaml` flip in lockstep (per fixture). Fixtures 0001/0002/0007 are NOT edited (they don't reference `host.docker.internal` at any cluster — verified at PLAN-write + Task 3 entry time). Edits are mechanically identical: 10 files × 1 line change each, no whitespace re-indent (the replacement string `STRICT_DNS` is the same indent level as `STATIC`). One bundled commit per PLAN.md signpost G + SPEC §6 signpost 8. Total: ~10 LoC of YAML diff.

**Verification tail.**

```
$ grep -n "type: STRICT_DNS" tests/fixtures/0003-tcp-proxy/*.yaml tests/fixtures/0004-tls-downstream/*.yaml tests/fixtures/0005-tls-upstream/*.yaml tests/fixtures/0006-tls-sni/*.yaml tests/fixtures/0008-http1-router-upstream/*.yaml
[10 lines, one per edited file]

$ grep -n "type: STATIC" tests/fixtures/0003-tcp-proxy/*.yaml tests/fixtures/0004-tls-downstream/*.yaml tests/fixtures/0005-tls-upstream/*.yaml tests/fixtures/0006-tls-sni/*.yaml tests/fixtures/0008-http1-router-upstream/*.yaml
[empty]

$ cargo test --package envoy-config 2>&1 | tail -3
test result: ok. <N> passed; 0 failed; 0 ignored

$ cargo test --package envoy-cluster 2>&1 | tail -3
test result: ok. <N> passed; 0 failed; 0 ignored
```

**Deviations from PLAN.** None.
```

- [ ] **Step 7: Commit Task 3.**

```bash
git add tests/fixtures/0003-tcp-proxy/envoy.yaml \
        tests/fixtures/0003-tcp-proxy/envoy-rust.yaml \
        tests/fixtures/0004-tls-downstream/envoy.yaml \
        tests/fixtures/0004-tls-downstream/envoy-rust.yaml \
        tests/fixtures/0005-tls-upstream/envoy.yaml \
        tests/fixtures/0005-tls-upstream/envoy-rust.yaml \
        tests/fixtures/0006-tls-sni/envoy.yaml \
        tests/fixtures/0006-tls-sni/envoy-rust.yaml \
        tests/fixtures/0008-http1-router-upstream/envoy.yaml \
        tests/fixtures/0008-http1-router-upstream/envoy-rust.yaml \
        docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md

git commit -m "$(cat <<'EOF'
phase 05.1: 5-fixture coordinated YAML edit — STATIC → STRICT_DNS

Flips type: STATIC to type: STRICT_DNS on the cluster whose endpoints
reference {{BACKEND_HOST}} in 5 fixtures (10 YAML files total):

- tests/fixtures/0003-tcp-proxy/{envoy,envoy-rust}.yaml
- tests/fixtures/0004-tls-downstream/{envoy,envoy-rust}.yaml
- tests/fixtures/0005-tls-upstream/{envoy,envoy-rust}.yaml
- tests/fixtures/0006-tls-sni/{envoy,envoy-rust}.yaml
- tests/fixtures/0008-http1-router-upstream/{envoy,envoy-rust}.yaml

Both envoy.yaml (per-side rendered with BACKEND_HOST=host.docker.internal
per ADR-0015 host-gateway) and envoy-rust.yaml (rendered with
BACKEND_HOST=127.0.0.1) flip in lockstep. The existing per-side
substitutions are unchanged.

Under STRICT_DNS:
- Upstream Envoy container resolves host.docker.internal at startup
  via its STRICT_DNS resolver consulting /etc/hosts (Docker injects
  host.docker.internal -> host-gateway IP per with_host(...,
  Host::HostGateway)).
- envoy-rust resolves 127.0.0.1 at startup via tokio::net::lookup_host
  (literal IPs accepted by lookup_host and returned as-is).

Fixtures 0001-tcp-echo, 0002-static-admin-ready, and
0007-http1-direct-response are NOT edited (no upstream cluster /
admin-only / direct_response-only).

Materially closes phase-04.3 REVIEW C-1 once the Docker-gated CI
run confirms green at Task 4. The C-1 carryforward chain
(02.2 -> 03.1 -> 03.2 -> 04.1 -> 04.2 -> 04.3 -> 05.1) ends at
Task 4's verification commit.
EOF
)"
```

Verify:

```bash
git log -1 --oneline
git status
```

Expected: commit message starts with `phase 05.1: 5-fixture coordinated YAML edit`; working tree clean.

---

### Task 4: State-4 phase-done gate — full local gate + Cargo.lock sync + Docker-gated CI re-push + PROGRESS.md aggregate

**Files:**
- Modify: `Cargo.lock` (sync if needed; expected to be a no-op or single-line diff per signpost B + SPEC §1 acceptance signal (e)).
- Modify: `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md` (append Task 4 section + state-4 phase-done gate aggregate).

**Why last:** Task 4 is the state-4 phase-done verification commit per `BOOTSTRAP_PROMPT.md` §7.5. It runs the full 5-command stable-toolchain gate + the fuzz short-budget run + the Docker-gated CI re-push, and aggregates the results in PROGRESS.md per the standard verification cadence (precedent: 04.3's task-15 commit `89f7018` quoted the corresponding CI run; 04.3's state-4 verification commit `cb0949e` aggregated the full suite). This task materially closes phase-04.3 REVIEW C-1 — the 5 affected Docker-gated fixtures pass simultaneously, ending the cross-phase regression chain that spanned phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 since ADR-0015's landing at `435c6fa`.

**Scope.** 0 LoC of code changes. ~50 lines of PROGRESS.md prose + 1 Cargo.lock sync commit (if needed; expected no-op per signpost B). No new ADRs at this task per SPEC §7.

- [ ] **Step 1: Run the full local gate (5 stable commands + fuzz short-budget).**

```bash
cargo build --workspace --all-targets 2>&1 | tail -10
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -10
cargo fmt --all -- --check 2>&1 | tail -5
cargo test --workspace 2>&1 | tail -20
cargo deny check 2>&1 | tail -10
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 2>&1 | tail -10
```

Expected: each of the 6 commands exits with status 0 and no error/warning output.

- `cargo build`: clean.
- `cargo clippy`: no `error[clippy::*]`.
- `cargo fmt --check`: no diff.
- `cargo test --workspace`: `test result: ok. <total> passed; 0 failed; 0 ignored`. The `<total>` count is the sum of all workspace-crate tests; 05.1 added 6 (envoy-config) + 3 (envoy-cluster) = 9 new tests, plus the 5 existing envoy-cluster tests are now `#[tokio::test]` flavored (no count change). The previously-red Docker-gated tests (`tcp_proxy`, `tls_downstream`, `tls_upstream`, `tls_sni`, `http1_router_upstream`) should now pass — though if the local environment lacks Docker, they may be skipped or report as "ignored" depending on the test feature flags; this is OK since CI is the substantive verification.
- `cargo deny check`: clean (no new transitive licenses; tokio + its transitive graph were already in the workspace's resolved feature set).
- `cargo +nightly fuzz run parse_bootstrap`: completes the 30-second budget run with no crashes; the new `strict_dns_cluster.yaml` seed is read from the corpus.

If any command fails:
- `cargo build` failure → re-examine Task 1/2/3 work; the build was green at each task's commit so a regression is a red flag.
- `cargo clippy` failure → fix in-place per the lint suggestion, commit as `phase 05.1: review fix (state 4 clippy)` (mirrors 04.3's review-fix-commit pattern).
- `cargo test --workspace` failure → inspect the failing test. Most likely cause: a subprocess-spawning test in `tests/differential/tests/*.rs` failing because Docker isn't locally available — this is acceptable (CI has Docker); local runs may report these as ignored.
- `cargo deny check` failure → unlikely. If it fires on a transitive license issue, investigate; expected to be a no-op.
- `cargo +nightly fuzz run parse_bootstrap` failure → inspect the panic. Most likely cause: a typo in the new `strict_dns_cluster.yaml` seed; fix and re-commit.

- [ ] **Step 2: Sync `Cargo.lock` (likely no-op).**

```bash
cargo build --workspace --all-targets
git status -- Cargo.lock
```

If `git status` shows `Cargo.lock` as modified, the workspace's resolved feature set changed (likely a feature-flag activation propagated through tokio's transitive graph from the new envoy-cluster direct dep). Stage and commit:

```bash
git add Cargo.lock
git commit -m "$(cat <<'EOF'
phase 05.1: sync Cargo.lock (envoy-cluster gains direct tokio dep)

Cargo.lock sync after Task 2 added tokio = { version = "1",
features = ["net", "rt", "macros"] } to envoy-cluster's
[dependencies]. Workspace's resolved feature set may have unified
slightly; expected diff is minimal-to-zero since tokio was already
a top-level dep on envoy-bin/envoy-listener/envoy-tcp/envoy-http1
/envoy-tls/etc. with the net feature already activated.

Mirrors the dedicated-Cargo.lock-sync cadence established in
phase-01 (4955252), 02.1 (dea4d16), 02.2 (2146014), 03.1 (eb039e6),
03.2 (85685a3); phase-04.x went inline-at-scaffold per the in-flux
M5/M9 disposition (carryforward continues to 05.2+).
EOF
)"
```

If `git status` shows `Cargo.lock` clean, no commit is needed at this step — the workspace's resolved feature set was already saturated with tokio's `net` feature.

- [ ] **Step 3: Push to CI to confirm green Docker-gated runs.**

```bash
git push origin <current-branch>
```

Wait for CI to complete. Watch the runs via `gh run list --workflow ci.yml --limit 3` or via the GitHub web UI.

Expected runs:
- The standard `cargo test --workspace` job — green.
- The Docker-gated differential job — green for fixtures 0001/0002/0003/0004/0005/0006/0007/0008. The 5 previously-red fixtures (0003/0004/0005/0006/0008) are now green; 0001/0002/0007 remain green (unaffected by C-1).
- The fuzz job — green; the new `strict_dns_cluster.yaml` seed is exercised.

Capture the CI run URL and the per-test results for PROGRESS.md.

If a fixture remains red:
- 0003/0004/0005/0006/0008 red → inspect the test output. Possible causes:
  - Upstream Envoy container's startup logs show `malformed IP address: host.docker.internal` (the C-1 trace) → Task 3's edit didn't land on the corresponding `envoy.yaml`. Re-check via `grep -n "type: STATIC" tests/fixtures/...`.
  - envoy-rust subprocess logs show `DnsResolutionFailed` or `EndpointParse` → Task 2's runtime branch has a bug (zero-result guard misfiring? wrong cluster_type match arm?). Investigate; re-enter state 3 if needed (per `BOOTSTRAP_PROMPT.md` §5.2 REVIEW.md re-loop discipline).
  - Both proxies' logs are clean but the differential assertion fails on byte-equality → unlikely; would indicate a transport-layer regression, NOT a C-1 issue.
- 0001/0002/0007 red → critical regression. These weren't touched in 05.1; a red here means an unrelated regression was introduced. Investigate and re-enter state 3.

If the fuzz job fails → likely a typo in `strict_dns_cluster.yaml`; fix and re-commit as a review-fix.

- [ ] **Step 4: Append Task 4 section + state-4 phase-done gate aggregate to `docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md`.**

```markdown
## Task 4 — state-4 phase-done gate verification + Cargo.lock sync + CI re-push (2026-MM-DD)

**Commit:** `<SHA>` (verification commit; if Cargo.lock sync was needed, separate dedicated commit `<SHA-2>`)

**Change summary.** Runs the state-4 phase-done gate per `BOOTSTRAP_PROMPT.md` §7.5: the 5 stable-toolchain commands + the fuzz short-budget + the Docker-gated CI re-push. Aggregates results below. **Materially closes phase-04.3 REVIEW C-1** at this commit's CI run — the 5 affected Docker-gated fixtures pass simultaneously, ending the cross-phase regression chain that spanned phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 since ADR-0015's landing at `435c6fa`.

**Local gate (stable toolchain):**

```
$ cargo build --workspace --all-targets 2>&1 | tail -3
[clean]

$ cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -3
[clean]

$ cargo fmt --all -- --check
[clean — exit 0]

$ cargo test --workspace 2>&1 | tail -3
test result: ok. <total> passed; 0 failed; 0 ignored

$ cargo deny check 2>&1 | tail -3
[clean]
```

**Fuzz short-budget (nightly toolchain):**

```
$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 2>&1 | tail -5
Done <N> runs in 30 second(s)
[no crashes]
```

**Docker-gated CI run:**

CI run URL: <paste-from-GH>

Per-fixture results (the 5 affected fixtures + the 3 unaffected fixtures):

```
tests/differential/tests/tcp_proxy.rs              GREEN  (RESTORED — STRICT_DNS flip; was red since 02.2's ADR-0015)
tests/differential/tests/tls_downstream.rs         GREEN  (RESTORED — STRICT_DNS flip)
tests/differential/tests/tls_upstream.rs           GREEN  (RESTORED — STRICT_DNS flip)
tests/differential/tests/tls_sni.rs                GREEN  (RESTORED — STRICT_DNS flip)
tests/differential/tests/http1_router_upstream.rs  GREEN  (RESTORED — STRICT_DNS flip; was red at first push, the trigger for C-1 detection)
tests/differential/tests/tcp_echo.rs               GREEN  (unchanged; fixture 0001)
tests/differential/tests/static_admin_ready.rs     GREEN  (unchanged; fixture 0002)
tests/differential/tests/http1_direct_response.rs  GREEN  (unchanged; fixture 0007)
```

**Cargo.lock sync.** [No-op / Single-line diff — tokio's net feature already active in the workspace's resolved feature set.] Dedicated commit `<SHA-2>` if applicable.

**Phase-04.3 REVIEW C-1 closes** at this commit's CI run. The carryforward chain (02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3 → 05.1) ends here.

**Phase-04.1 REVIEW M-claim** (drive_http1 per-function unit test) is unblocked by the Docker-gated regression mask removal but stays deferred per the 04.3 disposition (the M-claim's own scope is a separate per-function unit test that mocks `tokio::io::AsyncRead`/`AsyncWrite`; 05.1 does NOT extend the harness). Carryforward chain continues.

**Deviations from PLAN.** None.
```

- [ ] **Step 5: Commit the state-4 verification.**

```bash
git add docs/envoy-rust/phases/05.1-fixture-hardening/PROGRESS.md

git commit -m "$(cat <<'EOF'
phase 05.1: state-4 phase-done gate verification (task 4)

Runs the 5 stable-toolchain commands + fuzz short-budget + Docker-gated
CI re-push per BOOTSTRAP_PROMPT.md §7.5. All 8 differential fixtures
green; the 5 previously-red fixtures (0003/0004/0005/0006/0008) restored
by Task 3's STRICT_DNS flip + Task 1/2's schema/runtime growth.

Materially closes phase-04.3 REVIEW C-1 at this commit's CI run. The
cross-phase carryforward chain (02.2 -> 03.1 -> 03.2 -> 04.1 -> 04.2
-> 04.3 -> 05.1) ends here. Phase-04.1 REVIEW M-claim unblocked but
stays deferred per the 04.3 disposition.

CI run URL aggregated in PROGRESS.md Task 4.
EOF
)"
```

Verify:

```bash
git log -3 --oneline
git status
```

Expected: the last 3 commits are Task 1, Task 2, Task 3, Task 4 (and possibly the Cargo.lock sync if it landed). Working tree clean.

---

## After PLAN.md execution

- **State 5 (REVIEW.md)** lands in a separate session per `BOOTSTRAP_PROMPT.md` §5.1's "one state per session" rule. The reviewer invokes `superpowers:requesting-code-review` and produces `docs/envoy-rust/phases/05.1-fixture-hardening/REVIEW.md`. Verdict expected: Approved (the surface is small + tightly scoped; the 4 PLAN tasks deliver the 4 SPEC deliverables D1–D4 cleanly; 0 Critical, 0 Important findings anticipated; possibly some Minor observations on the planner-time signposts A/B for forward-track audit).
- **State 6 (phase-done close)** lands in a third session: ROADMAP row `05.1` flips `planned` → `done`; STATE.md advances to phase `05.2-http2-downstream` lifecycle state 3 (05.2's SPEC was landed at parent-05 state-2 alongside this PLAN's predecessor SPEC; PLAN.md does not exist yet for 05.2); next-skill `superpowers:writing-plans` scoped to sub-phase 05.2. Notes section gains the carryforward bookkeeping per SPEC §6 signpost 17.
- The state-6 commit message follows the SPEC §9 format. Parent ROADMAP row `05` stays `in-progress` (flips at sub-phase 05.3's state-6 commit per the ROADMAP-schema invariant).
