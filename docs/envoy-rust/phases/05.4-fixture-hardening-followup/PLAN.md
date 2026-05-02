# Phase 05.4 — Fixture-hardening follow-up: 6 root-cause fixes substantively closing phase-04.3 REVIEW C-1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/05.4-fixture-hardening-followup/SPEC.md` (committed at the 05.4 state-2 brainstorm commit `06b46a9`). This plan operationalizes SPEC §§D1–D7. Where this plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-05 SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` (committed at parent-05 state-1 SHA `cd1a70e`) and the predecessor 05.1 SPEC at `docs/envoy-rust/phases/05.1-fixture-hardening/SPEC.md` are preserved unedited as historical artifacts; for execution they are superseded by this 05.4 SPEC.

**Goal.** Substantively close phase-04.3 REVIEW C-1 by landing the 6 root-cause fixes that 05.1's STRICT_DNS preamble proved necessary but not sufficient. The schema (`ClusterType::StrictDns`) + runtime (`tokio::net::lookup_host`) + 5-fixture YAML flip landed in 05.1 (commits `bfabcb6` / `f7a555d` / `0ce0aa2`); the canonical CI run `25258722850` against 05.1 head `4768fcd` revealed 6 distinct latent regressions exposed by the STRICT_DNS flip. 05.4 lands them under proper SPEC + ADR discipline (the procedural defect at the 05.1 aborted attempt — no SPEC anchor, no ADRs, blew Task 4's 0-LoC contract; preserved on backup branch `backup/task4-scope-creep-2026-05-02` commit `9279895` — is corrected here, not the technical content). The 6 fixes (mapped 1:1 to deliverables D1–D6 + a state-4 verification deliverable D7):

- **D1 (Task 1)** — `crates/envoy-config/src/bootstrap.rs::Cluster` gains an optional `dns_lookup_family: Option<DnsLookupFamily>` field; new `pub enum DnsLookupFamily { V4Only, V6Only, Auto }`. Re-exported from `crates/envoy-config/src/lib.rs`. **ADR-0024** lands inline at this task — the field is parsed-and-stored on envoy-rust's typed Cluster struct but NOT consumed at runtime in 05.4 (envoy-rust's existing 05.1-landed `tokio::net::lookup_host` resolution path is unchanged; only the upstream Envoy side observes the V4_ONLY knob via D2's envoy.yaml edit).
- **D2 (Task 2)** — coordinated 5-fixture `envoy.yaml` edit: add `dns_lookup_family: V4_ONLY` immediately after `type: STRICT_DNS` on `tests/fixtures/{0003-tcp-proxy,0004-tls-downstream,0005-tls-upstream,0006-tls-sni,0008-http1-router-upstream}/envoy.yaml`. **Only `envoy.yaml` is edited; `envoy-rust.yaml` is NOT edited** (envoy-rust uses `127.0.0.1` literal IP at the substituted `{{BACKEND_HOST}}` site; DNS family selection has no runtime semantics on envoy-rust per ADR-0024). One bundled commit per SPEC §6 signpost 15.
- **D3 (Task 3)** — `crates/envoy-config/src/bootstrap.rs::Listener` gains `listener_filters: Vec<serde_yaml::Value>` parse-and-ignore field; `tests/fixtures/0006-tls-sni/envoy.yaml` gains the explicit `tls_inspector` listener-filter block immediately after the `address:` line. **ADR-0026** lands inline at this task — establishes the parse-and-ignore pattern as a documented envoy-config posture; bounds it to `listener_filters` and lists the criteria for adding future parse-and-ignore fields. Hand-written `envoy_config::Listener` literal at `crates/envoy-tls/src/tests.rs:914-924::synth_listener_two_tls_chains` gains `listener_filters: vec![]`. (The `mk_listener_cfg` helper at `crates/envoy-listener/src/lib.rs:360` constructs via `serde_yaml::from_str`, so `#[serde(default)]` covers it without a literal-update; verified by the planner via `grep 'envoy_config::Listener {' crates/ tests/`.)
- **D4 (Task 4)** — three single-line bind-address flips: `tests/helpers/tcp-echo-server/src/main.rs:118` `TcpListener::bind(("127.0.0.1", port))` → `TcpListener::bind(("0.0.0.0", port))`; same flip at `tests/helpers/tls-echo-server/src/main.rs:109`; same at `tests/helpers/http1-echo-server/src/main.rs:98`. The corresponding `tracing::info!` lines (line 119 / line 110 / line 99) update to log `0.0.0.0:{port}`. The doc-comment headers update to drop "localhost-only" language (`tcp-echo-server` line 3, `tls-echo-server` line 3, `http1-echo-server` line 3). No new tests; the flip is mechanically observable (0.0.0.0 binds all interfaces including loopback so existing tests continue unchanged). **No ADR** — this is a test-helper bug fix; ADR-0015's host-gateway grant is the operative cross-reference (already landed at `435c6fa`).
- **D5 (Task 5)** — `crates/envoy-http1/src/client.rs::Client::send_request` (request-write path at lines 94–103) gains a `body_is_nonempty` guard: only inject the synthetic `content-length: <len>` when the request does not carry an explicit Content-Length AND the request body is non-empty. The 1 affected unit test at `crates/envoy-http1/src/client.rs:441-465::send_request_writes_serialized_request_bytes` flips its assertion from `s.contains("content-length: 0\r\n")` to `!s.contains("content-length: 0\r\n")`. `tests/fixtures/0008-http1-router-upstream/expectations.yaml` `expected_body` line drops `  content-length: 0\n`. **ADR-0025** lands inline at this task — RFC 7230 §3.3.2 compliance + Envoy v1.33 parity; bounded to empty-body requests; preserves existing behavior for non-empty bodies and explicit Content-Length.
- **D6 (Task 6)** — `tests/differential/src/upstream.rs:88` `tokio::time::sleep(Duration::from_millis(500))` becomes a conditional `let settle_ms = if host_gateway { 2000 } else { 500 }` followed by `tokio::time::sleep(Duration::from_millis(settle_ms))`. The `host_gateway` parameter is already bound at `upstream::start`'s signature at line 48 of the same file — no API change. The 3 unaffected fixtures (0001/0002/0007) do not pass `host_gateway = true` (the call site in `tests/differential/src/lib.rs:989` derives the flag via `upstream_yaml.contains("host.docker.internal")`; verified at PLAN-write time that fixtures 0001/0002/0007 do not reference `host.docker.internal`). **No ADR** — test-harness timing constant adjustment.
- **D7 (Task 7)** — state-4 phase-done gate: `cargo build` / `cargo clippy` / `cargo fmt --check` / `cargo test --workspace` / `cargo deny check` / `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` all green; Docker-gated CI re-push showing all 5 affected fixtures + 3 unaffected fixtures green simultaneously; per-fixture matrix + CI run URL quoted into PROGRESS.md Task 7. Substantively closes phase-04.3 REVIEW C-1 at this commit.

**Lands 3 ADRs** (per SPEC §7): **ADR-0024** at Task 1 (D1, DnsLookupFamily schema), **ADR-0026** at Task 3 (D3, Listener.listener_filters parse-and-ignore — new pattern in envoy-config), **ADR-0025** at Task 5 (D5, content-length: 0 suppression). The DECISIONS.md ledger after 05.4 reads `... ADR-0023 (05.1) | ADR-0024 (05.4 Task 1) | ADR-0026 (05.4 Task 3) | ADR-0025 (05.4 Task 5) | ...` — **landing-time order, not numeric order**, per the append-only ledger discipline (SPEC §6 signpost 9). Closes phase-04.3 REVIEW C-1 substantively at D7. Phase-04.1 REVIEW M-claim (drive_http1 per-function unit test) is unblocked by the fixture-mask removal but stays deferred per the 04.3 disposition (carryforward chain continues per SPEC §1 "Cross-phase items unblocked but not closed at 05.4").

**No HTTP/2 work in 05.4.** The `envoy-http2` crate, the `h2 = "0.4"` Cargo dep, HCM-on-H2 dispatch, fixtures 0009/0010, and h2spec conformance gate all defer to sub-phases 05.2 and 05.3 per ADR-0022 (parent-05 split decision). 05.4 introduces no new top-level Cargo deps — every new typed surface (DnsLookupFamily enum, listener_filters field, empty-body-CL suppression) lives in existing crates with their existing dep sets.

**~7 tasks, ~250 LoC** per SPEC §3 deliverable estimates (~30 D1 + ~5 D2 + ~130 D3 + ~10 D4 + ~30 D5 + ~5 D6 + ~0 D7). Both `BOOTSTRAP_PROMPT.md` §6.1 split-gates (~25 tasks / ~1500 LoC) hold with massive headroom (7 ≪ 25, ~250 ≪ 1500). **Do not split 05.4 further** — the scope is below the §6.1 gates and a nested split of an already-split sub-phase is explicitly flagged as suspicious in `BOOTSTRAP_PROMPT.md` §6.1; if execution surfaces drift, invoke `superpowers:systematic-debugging` first.

**Architecture.** The schema deltas are mechanical: `Cluster` already carries `#[serde(deny_unknown_fields)]` at `crates/envoy-config/src/bootstrap.rs:47` so adding `dns_lookup_family: Option<DnsLookupFamily>` with `#[serde(default)]` is a one-block addition that automatically lights up the `dns_lookup_family: V4_ONLY` serde tag (the new `DnsLookupFamily` enum carries `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]` mirroring `ClusterType`'s shape from 05.1; the `V4_ONLY` / `V6_ONLY` / `AUTO` tags map mechanically). The `Listener` struct at `crates/envoy-config/src/bootstrap.rs:107-112` similarly gains `listener_filters: Vec<serde_yaml::Value>` with `#[serde(default)]` — opaque YAML values stored without typing (the parse-and-ignore pattern). The runtime delta in `envoy-http1::Client::send_request` is a 2-line predicate addition guarding the 5-line synthetic-CL emission block at `crates/envoy-http1/src/client.rs:99-103`; the existing `request_has_cl` check at line 95 is preserved unchanged and the new `body_is_nonempty` check is composed via `&&`. The harness settle-time delta at `tests/differential/src/upstream.rs:88` is a 2-line conditional replacing the flat `Duration::from_millis(500)`. The 3 helper bind flips are 1 line each + 1 doc-comment line each. The 5 fixture YAML edits are 1 line each (D2). The fixture 0006 envoy.yaml `tls_inspector` block is ~5 lines (D3). The fixture 0008 expectations.yaml diff is 1 line removed (D5). Per-side substitutions (`envoy.yaml` rendered with `BACKEND_HOST=host.docker.internal` per ADR-0015; `envoy-rust.yaml` rendered with `BACKEND_HOST=127.0.0.1`) are unchanged — D2's `dns_lookup_family: V4_ONLY` is added only on the envoy.yaml side because envoy-rust's resolution of the literal `127.0.0.1` doesn't engage DNS family selection; D3's `tls_inspector` block is added only on the envoy.yaml side because envoy-rust performs SNI dispatch at the rustls layer (per phase 03.2 architectural choice) and `deny_unknown_fields` on envoy-rust's parser would reject the block.

**Tech stack.** Rust edition 2024 on pinned stable (`rust-toolchain.toml` D-3.9). **No new top-level Cargo deps.** Existing crates only: `envoy-config` already depends on `serde_yaml` (so `Vec<serde_yaml::Value>` requires no manifest change); `envoy-http1` already pulls everything Task 5 needs; the helpers and harness already have their existing dep sets. New runtime API surface: `DnsLookupFamily` enum (3 variants); `Cluster.dns_lookup_family: Option<DnsLookupFamily>` field; `Listener.listener_filters: Vec<serde_yaml::Value>` field. New behavioral surface: `Client::send_request` no longer emits `content-length: 0` on empty-body requests (RFC 7230 §3.3.2 compliance; bounded behavior change per ADR-0025). New harness behavior: settle-time bumps 500ms → 2000ms for `host_gateway = true` fixtures only (no API change; same `upstream::start` signature). No changes to `.github/workflows/ci.yml`, `deny.toml`, `BEHAVIOR_CONTRACT.md` (per SPEC §2), `rust-toolchain.toml`, `ENVOY_TARGET.md`, root `Cargo.toml` `[workspace] members` (no new crates), or any `Cargo.lock` line (no transitive surface change anticipated; cross-checked at Task 7).

---

## File structure (created / modified / not touched)

**Created:**

- `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md` (appended once per task during execution; created by Task 1 alongside the ADR-0024 landing).

**Modified:**

- `docs/envoy-rust/DECISIONS.md` — append **ADR-0024** at Task 1 (immediately after the existing ADR-0023 block ending at line 433), **ADR-0026** at Task 3 (immediately after ADR-0024), **ADR-0025** at Task 5 (immediately after ADR-0026). Final ledger order: ADR-0023 → ADR-0024 → ADR-0026 → ADR-0025 (landing-time order, not numeric — per SPEC §6 signpost 9). The ADR ledger head before this sub-phase is ADR-0023; ADR-0024/0025/0026 land at the next-sequential numbers with no renumbering needed.
- `crates/envoy-config/src/bootstrap.rs` — extend existing `Cluster` struct at lines 47-56 with `dns_lookup_family: Option<DnsLookupFamily>` field; add new `DnsLookupFamily` enum after the existing `ClusterType` enum at line 72 (before `LbPolicy`); extend existing `Listener` struct at lines 106-112 with `listener_filters: Vec<serde_yaml::Value>` field; append 2 new validator unit tests to the existing `#[cfg(test)] mod tests` block (`parses_cluster_with_dns_lookup_family_v4_only` at Task 1; `parses_listener_with_tls_inspector_listener_filter` at Task 3).
- `crates/envoy-config/src/lib.rs` — extend the existing `pub use bootstrap::{...}` re-export list at lines 10-19 to include `DnsLookupFamily` (alphabetic insertion between `DataSource` at line 12 and `DownstreamTlsContext` at line 12). `Listener.listener_filters` is a field on the already-re-exported `Listener` type; no new top-level re-export needed.
- `crates/envoy-cluster/src/cluster.rs` — at Task 1, add `dns_lookup_family: None` to the 2 hand-written `Cluster` test initialisers (planner verifies count via `grep -n 'envoy_config::Cluster {\|Cluster {$' crates/envoy-cluster/src/cluster.rs` at Task 1 Step 1 — SPEC §3 D1 says "2 sites at lines ~432 and ~474 of the backup-branch diff" but the planner re-confirms the actual line numbers at runtime; if 0 or >2 sites are found, the difference is recorded in PROGRESS.md and Task 1 proceeds with the actual count). No other changes — the runtime STRICT_DNS resolution path is unchanged from 05.1 (per SPEC §6 signpost 5).
- `crates/envoy-tls/src/tests.rs` — at Task 3, add `listener_filters: vec![]` to the 1 hand-written `envoy_config::Listener` literal in `synth_listener_two_tls_chains` at lines 914-924 (specifically: insert a `listener_filters: vec![],` line into the struct literal, after the `filter_chains: vec![chain_a, chain_b],` line at line 922). Verified via `grep -n 'envoy_config::Listener {' crates/ tests/` at PLAN-write time: 1 hit at this site; 1 hit at `crates/envoy-listener/src/lib.rs:360::mk_listener_cfg` constructs via `serde_yaml::from_str`, NOT via struct literal, so `#[serde(default)]` covers it without an update.
- `crates/envoy-http1/src/client.rs` — at Task 5, modify the request-write CL-emission block at lines 94-103 to add a `body_is_nonempty` guard; at Task 5, flip the assertion in the `send_request_writes_serialized_request_bytes` unit test at lines 460-463.
- `tests/helpers/tcp-echo-server/src/main.rs` — at Task 4, line 118: `TcpListener::bind(("127.0.0.1", port))` → `TcpListener::bind(("0.0.0.0", port))`; line 119: `tracing::info!(port, "tcp-echo-server listening")` → `tracing::info!(port, "tcp-echo-server listening on 0.0.0.0:{port}")`; line 3 doc-comment: drop "localhost-only" language.
- `tests/helpers/tls-echo-server/src/main.rs` — at Task 4, line 109: `TcpListener::bind(("127.0.0.1", args.port))` → `TcpListener::bind(("0.0.0.0", args.port))`; line 110: tracing log line update to log `0.0.0.0:{}`; line 3 doc-comment: drop "localhost-only" language.
- `tests/helpers/http1-echo-server/src/main.rs` — at Task 4, line 98: `TcpListener::bind(("127.0.0.1", args.port))` → `TcpListener::bind(("0.0.0.0", args.port))`; line 99: tracing log line update to log `0.0.0.0:{}`; line 3 doc-comment: drop "localhost-only" language.
- `tests/differential/src/upstream.rs` — at Task 6, replace `tokio::time::sleep(Duration::from_millis(500)).await;` at line 88 with a conditional `let settle_ms = if host_gateway { 2000 } else { 500 }; tokio::time::sleep(Duration::from_millis(settle_ms)).await;` (2 lines instead of 1).
- `tests/fixtures/0003-tcp-proxy/envoy.yaml` — at Task 2, add `      dns_lookup_family: V4_ONLY` immediately after `      type: STRICT_DNS` (line 27 today; the new line lands as line 28).
- `tests/fixtures/0004-tls-downstream/envoy.yaml` — at Task 2, same pattern after line 37.
- `tests/fixtures/0005-tls-upstream/envoy.yaml` — at Task 2, same pattern after line 16.
- `tests/fixtures/0006-tls-sni/envoy.yaml` — at Task 2, same pattern after line 40; at Task 3, also add an explicit `listener_filters: [...]` block on the `tcp_listener` listener immediately after the `address:` line (line 6).
- `tests/fixtures/0008-http1-router-upstream/envoy.yaml` — at Task 2, same `dns_lookup_family: V4_ONLY` pattern after line 49.
- `tests/fixtures/0008-http1-router-upstream/expectations.yaml` — at Task 5, drop `  content-length: 0\n` from the `expected_body` line (line 9); the line transforms from `body: "method: GET\npath: /\nheaders:\n  content-length: 0\n  host: envoy-rust.test\nbody: \n"` to `body: "method: GET\npath: /\nheaders:\n  host: envoy-rust.test\nbody: \n"`.
- `docs/envoy-rust/ROADMAP.md` — at state 6 only (NOT a state-3 task), flip row `05.4` `status` `in-progress` → `done`. Parent row `05` stays `in-progress` (flips at sub-phase 05.3's state-6 commit per the ROADMAP-schema invariant). State-6 close-out is a separate session per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session") — not part of this PLAN's tasks.
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase to sub-phase `05.2-http2-downstream` lifecycle state 2 (the 05.2 SPEC was landed at parent-05 state-2 commit `f1804a7` alongside 05.1's; 05.2 PLAN.md does not exist yet). Next-skill `superpowers:writing-plans` scoped to sub-phase 05.2. Notes section gains the carryforward bookkeeping per SPEC §6 signpost 17 (C-1 closed; M-claim still deferred).

**Not touched in 05.4** (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `cd1a70e`.
- `docs/envoy-rust/phases/05.1-fixture-hardening/*` (predecessor) — closed at the 05.1 phase-done commit `1d05cd0`; unedited in 05.4.
- `docs/envoy-rust/phases/05.4-fixture-hardening-followup/SPEC.md` (this sub-phase) — landed at brainstorm commit `06b46a9`; unedited in 05.4 execution.
- `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md`, `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` — landed at parent-05 state-2 commit `f1804a7`; unedited in 05.4 (their PLAN/PROGRESS/REVIEW land in their own sub-phase execution windows).
- `docs/envoy-rust/phases/{04*, 03*, 02*, 01, 00}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.4 (per SPEC §2).
- `docs/envoy-rust/MISSION.md`, `docs/envoy-rust/SKILL_ROUTING.md` — frozen per their self-described durability discipline.
- `crates/envoy-http2/` — does not exist at 05.4 close (lands in 05.2).
- `crates/envoy-tls/`, `crates/envoy-tcp/`, `crates/envoy-listener/`, `crates/envoy-bin/`, `crates/envoy-cluster/` (core logic) — unchanged. The schema growth is in envoy-config only; the envoy-http1 client behaviour change is bounded; the helpers + harness changes are in tests/. The 2 hand-written `Cluster` initialisers in `crates/envoy-cluster/src/cluster.rs::tests` pick up `dns_lookup_family: None` mechanically; the cluster runtime path is unchanged from 05.1 per SPEC §6 signpost 5.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0007-http1-direct-response/` — unedited; their fixtures must remain green at the 05.4 state-4 gate. Verified at PLAN-write time via `grep -L 'host.docker.internal\|BACKEND_HOST' tests/fixtures/000{1,2,7}*/envoy*.yaml` returning all three (fixture 0001 has no upstream cluster, 0002 is admin-only, 0007 is direct_response with no upstream — none reference `host.docker.internal`).
- `tests/fixtures/0009-http2-direct-response/`, `tests/fixtures/0010-http2-router-upstream/` — do not exist at 05.4 close (land in 05.2 and 05.3 respectively).
- `tests/differential/src/{lib,backend,subject,tls}.rs`, `tests/differential/Cargo.toml` — unchanged. Only `upstream.rs:88` settle-time line is edited (D6/Task 6).
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/`, `crates/envoy-config/fuzz/corpus/parse_bootstrap/` — unchanged. The existing 12 corpus seeds (including 05.1's `strict_dns_cluster.yaml`) continue to parse cleanly through the schema additions (the 2 new fields are `Option`/`Vec` with `#[serde(default)]`, so existing seeds without these fields continue to deserialize). Planner may optionally add a `cluster_with_dns_lookup_family.yaml` seed at PLAN discretion; not required by the gate (per SPEC §1 acceptance signal (d)).
- `Cargo.lock` — no edits anticipated. No new top-level deps; no transitive surface changes. Cross-checked at Task 7.
- `deny.toml` — no edits. No new top-level deps; no new transitive licenses surface. Cross-checked at Task 7.
- Root `Cargo.toml` — no `[workspace] members` changes (no new crates).
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.
- `.github/workflows/ci.yml` — untouched. Existing `cargo test --workspace` + `cargo +nightly fuzz run parse_bootstrap` jobs pick up the additions automatically.
- The 6 patches on `backup/task4-scope-creep-2026-05-02` are NOT cherry-picked or merged — they are the diagnostic reference (per SPEC §6 signpost 10); per-task TDD discipline re-derives them.

---

## Task index

Each task ends with a commit. `PROGRESS.md` gets a new section per task in the phase-04.x / 05.1 style (task id, commit SHA, change summary, verification tail, deviations from PLAN). Use the follow-up `phase 05.4: progress note (task N)` commit convention from 04.3 / 05.1 if a post-hoc note is needed (e.g., to backfill the just-landed commit's SHA into the PROGRESS narrative).

**Ordering rationale** (SPEC §3 deliverable order + §6 signposts 11 + 12):

- **Task 1 lands the `Cluster.dns_lookup_family` schema growth + ADR-0024 first** because Task 2's fixture YAML edit needs the parser to accept the new field — though the field is on the upstream-Envoy side only and no current envoy-config consumer parses envoy.yaml (signpost 4 of SPEC §6), defensive parse acceptance is the right posture per ADR-0024.
- **Task 2 lands the 5-fixture coordinated `dns_lookup_family: V4_ONLY` envoy.yaml edit second** because it depends on Task 1's schema (defensive acceptance) and produces the upstream-Envoy-side runtime knob that fixtures 0003/0004/0005/0006/0008 need.
- **Task 3 lands `Listener.listener_filters` schema + fixture 0006 tls_inspector block + ADR-0026 third.** Task 3 is mechanically coupled per SPEC §6 signpost 12: adding the `tls_inspector` block to fixture 0006's envoy.yaml without the schema growth would either (a) trigger no current consumer to fail (signpost 4 documents the open question — most likely currently green) or (b) trigger fail-parse at any fuzz seed walking. Schema first, then fixture edit, in one task.
- **Task 4 lands the 3 echo-server bind flips fourth.** Independent of Tasks 1-3; mechanically minimal. Could in principle be parallelized with Tasks 1-3 by a parallel-task-aware executor; the linear ordering here is for the subagent-driven-development cadence's clarity.
- **Task 5 lands the envoy-http1 CL: 0 suppression + fixture 0008 expectations + ADR-0025 fifth.** Task 5 is mechanically coupled per SPEC §6 signpost 11: removing `content-length: 0` from fixture 0008's expectations.yaml without the client-side suppression would red the fixture 0008 byte-equal echo body assertion (the request now carries CL: 0 on the envoy-rust side; the echo body still includes it; the expected body no longer expects it → mismatch). Client behavior change first, then expectations update, in one task.
- **Task 6 lands the harness settle-time bump sixth.** Independent of Tasks 1-5; mechanically minimal.
- **Task 7 closes the state-4 phase-done gate last** with the full `cargo build` / `clippy` / `fmt` / `test` / `deny` / fuzz short-budget run + Docker-gated CI re-push that substantively closes phase-04.3 REVIEW C-1 (per SPEC §3 D7).

Tasks:

1. **`envoy-config` — `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum + ADR-0024 inline + 1 new validator unit test + 2 hand-written `Cluster` initialiser updates in `envoy-cluster/src/cluster.rs::tests`**
2. **5-fixture coordinated YAML edit — add `dns_lookup_family: V4_ONLY` to `tests/fixtures/{0003,0004,0005,0006,0008}/envoy.yaml` (5 files; bundled commit per SPEC §6 signpost 15)**
3. **`envoy-config` — `Listener.listener_filters: Vec<serde_yaml::Value>` parse-and-ignore field + ADR-0026 inline + 1 new validator unit test + 1 hand-written `Listener` initialiser update in `envoy-tls/src/tests.rs::synth_listener_two_tls_chains` + fixture 0006 `envoy.yaml` `tls_inspector` block**
4. **3 echo-server helper bind flips — `tcp-echo-server`, `tls-echo-server`, `http1-echo-server` `TcpListener::bind(("127.0.0.1", ...))` → `TcpListener::bind(("0.0.0.0", ...))` + tracing log line + doc-comment header update**
5. **`envoy-http1::Client::send_request` — suppress synthetic `content-length: 0` on empty-body requests + ADR-0025 inline + 1 unit test assertion flip + fixture 0008 `expectations.yaml` expected-body update**
6. **`tests/differential/src/upstream.rs` — STRICT_DNS settle time 500ms → 2000ms for `host_gateway = true` fixtures (conditional bump)**
7. **State-4 phase-done gate verification — run all 5 stable commands + fuzz short-budget + Docker-gated CI re-push; quote outputs into PROGRESS.md; 5 affected fixtures + 3 unaffected fixtures all GREEN simultaneously substantively closes phase-04.3 REVIEW C-1**

Estimated total: 7 tasks, ~250 LoC. Both `BOOTSTRAP_PROMPT.md` §6.1 gates (~25 tasks / ~1500 LoC) hold with massive headroom (7 ≪ 25, ~250 ≪ 1500). **Do not split 05.4 further.** Per SPEC §6 + ADR-0022's express avoidance of nested splits, a 05.4.1 / 05.4.2 split would be a strong scope-creep signal and warrants `superpowers:systematic-debugging` first per `BOOTSTRAP_PROMPT.md` §6.1. Closest scope-creep vector at PLAN-write time: Task 3's parse-and-ignore field landing alongside the fixture YAML edit; if the parse test (`parses_listener_with_tls_inspector_listener_filter`) blows past ~120 LoC of YAML payload during execution, factor the test into a Task 3.5 standalone commit instead of nested-splitting the phase.

---

## Implementation signposts (planner-time clarifications + ambiguity resolutions)

**Signpost A — `DnsLookupFamily` enum lives in `envoy-config::bootstrap`, NOT a sibling module.**

SPEC §3 D1 prose locates the new enum at `crates/envoy-config/src/bootstrap.rs` (the existing module containing `ClusterType`, `LbPolicy`, etc.). The planner confirms this is the right placement: `bootstrap.rs` is the typed-surface module for the `Bootstrap` struct tree, and `DnsLookupFamily` is a sibling enum to `ClusterType` (mechanically the same shape — `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]` with 3 variants). No new module file is created. The re-export at `crates/envoy-config/src/lib.rs:10-19` lifts the enum into the public API.

**Signpost B — `dns_lookup_family` runtime non-consumption is deliberate (per ADR-0024).**

The 5-fixture YAML edit in Task 2 only adds the field to `envoy.yaml` (the upstream-Envoy side). `envoy-rust.yaml` is **not** edited. envoy-rust's `tokio::net::lookup_host` (the existing 05.1-landed STRICT_DNS resolution path at `crates/envoy-cluster/src/cluster.rs::from_bootstrap`) returns whatever the system stack delivers and does NOT filter resolved addresses by family. envoy-rust's typed `Cluster.dns_lookup_family` field is parsed-and-stored at zero runtime cost. ADR-0024 documents this; if a future fixture sets `dns_lookup_family: V6_Only` or `dns_lookup_family: Auto` on envoy-rust.yaml and envoy-rust observably misbehaves vs Envoy, that's a follow-up. 05.4 does not pre-emptively wire the runtime filter.

**Signpost C — `Listener.listener_filters` is parsed-and-stored as opaque `serde_yaml::Value`, NOT typed (per ADR-0026).**

SPEC §3 D3 + ADR-0026 both bound the field's typing to `Vec<serde_yaml::Value>` — opaque YAML values. The planner does NOT introduce a typed `ListenerFilter` enum (would require ~5 variants × ~10 LoC each + per-variant typed_config payloads + 5+ parse tests; envoy-rust gains zero runtime semantics from typing them; only one variant — `tls_inspector` — is needed for fixture 0006's actual fix). Whichever later phase first needs to ACTUALLY EXECUTE a listener filter lands a typed-variant extension on the field plus a runtime dispatch arm — not a new ADR (extending an existing pattern per ADR-0026 Consequences).

**Signpost D — `body_is_nonempty` predicate uses `request.body_bytes().is_some_and(|b| !b.is_empty())`.**

SPEC §3 D5 pseudocode uses `request.body_bytes().is_some_and(|b| !b.is_empty())`. The planner confirms `body_bytes` is a `pub(crate)` accessor on `crates/envoy-http1/src/codec.rs:62-64::Request` returning `Option<&[u8]>` (verified at PLAN-write time). The accessor is `#[allow(dead_code)]` and visibility is crate-local — both confirmed at the codec module. The predicate composes cleanly with the existing `request_has_cl` check at `crates/envoy-http1/src/client.rs:95-98`. **`Option::is_some_and`** is stable since Rust 1.70 (well before the toolchain pin of 1.95.0+). No new method on `Request` is needed.

**Signpost E — `host_gateway` parameter is already at `upstream::start`'s signature.**

`tests/differential/src/upstream.rs:46-50` declares `pub async fn start(envoy_yaml_path: &Path, host_gateway: bool, tls_pki: Option<&crate::tls::TlsTestPki>) -> Result<UpstreamProxy>`. The `host_gateway: bool` parameter is already bound. Task 6's conditional settle-time bump references this parameter directly — no API change is needed. The call site at `tests/differential/src/lib.rs:989-992` derives the flag via `let host_uses_host_gateway = upstream_yaml.contains("host.docker.internal")` and passes it through — fixtures that don't reference the hostname pass `false` and continue at 500ms; fixtures that reference it pass `true` and bump to 2000ms.

**Signpost F — fixture 0006's `tls_inspector` block lives between `address:` and `filter_chains:`.**

The natural insertion point on `tests/fixtures/0006-tls-sni/envoy.yaml` is immediately after the `address: { socket_address: { ... } }` line (line 6) and before the `filter_chains:` line (line 7). SPEC §3 D3 says "placed immediately after the `address:` line on the `tcp_listener` listener." The planner adopts this exact placement at Task 3 Step 4. The block adds ~5 lines to the file at Task 3 alongside the schema growth.

**Signpost G — One bundled commit for the 5-fixture YAML edit (per SPEC §6 signpost 15 — recommended posture).**

Task 2 lands all 5 fixture YAML edits in one commit (the `dns_lookup_family: V4_ONLY` line is added to each of `tests/fixtures/{0003,0004,0005,0006,0008}/envoy.yaml`). Cleanest in `git log`; one easy-to-read diff; the differential property is "all 5 fixtures green simultaneously," and splitting into 5 per-fixture commits would muddy the gate signal. Mirrors 05.1 Task 3's posture (`0ce0aa2`).

**Signpost H — Optional fuzz seed extension is NOT done in this PLAN.**

SPEC §1 acceptance signal (d) explicitly notes "the planner may optionally add a `cluster_with_dns_lookup_family.yaml` seed exercising the new field at PLAN discretion; not required by the gate." The planner elects NOT to add a new fuzz seed in 05.4. Reasoning: (a) the existing `strict_dns_cluster.yaml` seed continues to parse cleanly through the schema additions because `dns_lookup_family` is `Option` with `#[serde(default)]` (the seed simply doesn't set the field); (b) the schema additions are mechanically simple and the parse test at Task 1 covers the V4_ONLY path; (c) deferring the seed extension keeps 05.4 minimal per the SPEC's intent. If a future audit surfaces a regression that a fuzz seed would have caught, that's a follow-up in a hardening pass. PROGRESS.md Task 1 records this election.

**Signpost I — `cargo deny check` is expected to be a no-op at Task 7.**

No new top-level Cargo deps in 05.4. No transitive surface change (the `serde_yaml::Value` type used in Task 3's field is already a re-exported type from the existing `serde_yaml` dep on `envoy-config`; no new transitive crates). `cargo deny check` should report `0 errors` and the same warnings/notes as at HEAD `06b46a9`. Cross-checked at Task 7.

**Signpost J — Task 4 doc-comment header updates are NOT a content change; they reflect the bind flip.**

The current doc-comment headers on the 3 helpers describe them as "localhost-only" (`tcp-echo-server` line 3 — `"a minimal localhost-only echo server"`; `tls-echo-server` line 3 — `"a minimal localhost-only TLS echo server"`; `http1-echo-server` line 3 — `"minimal localhost-only HTTP/1.1 echo server"`). After Task 4's bind flip, the helpers accept on `0.0.0.0` (all interfaces, including loopback). The header language is updated to drop "localhost-only" — the planner uses literal find-and-replace substituting `localhost-only ` (with trailing space) for the empty string, leaving `"a minimal echo server"` / `"a minimal TLS echo server"` / `"minimal HTTP/1.1 echo server"`. PROGRESS.md Task 4 records the substitution.

**Signpost K — Task 7 may exclude differential suite locally if Docker is unavailable.**

Per SPEC §6 signpost 13, `cargo test --workspace` at the state-4 gate may exclude the differential suite if local Docker is unavailable on the executor's machine; the Docker-gated suite IS authoritative via CI. The state-4 PROGRESS.md narrative MUST quote the CI run URL + per-fixture matrix verbatim. If the executor has local Docker and runs the full suite, that's a bonus — but the CI run is the gate. Phase-05.1 set this precedent at PROGRESS.md Task 4.

**Signpost L — State-4 verification commit cadence mirrors 05.1's `b7fe910`.**

Task 7's commit message: `phase 05.4: state-4 phase-done gate verification (task 7)`. Touches PROGRESS.md only (the verification narrative + the per-fixture CI matrix). Substantive code changes land in Tasks 1–6. Task 7 is verification-only.

---

### Task 1: `envoy-config` — `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum + ADR-0024 inline + 1 new validator unit test + 2 hand-written `Cluster` initialiser updates

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md` (append ADR-0024 immediately after the existing ADR-0023 block ending at line 433).
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `Cluster` struct at lines 47-56 with `dns_lookup_family: Option<DnsLookupFamily>`; add new `DnsLookupFamily` enum at line 73 immediately after the `ClusterType::StrictDns` closing `}`; append 1 unit test to the existing `#[cfg(test)] mod tests` block).
- Modify: `crates/envoy-config/src/lib.rs` (extend the `pub use bootstrap::{...}` re-export list at lines 10-19 to include `DnsLookupFamily` alphabetically).
- Modify: `crates/envoy-cluster/src/cluster.rs` (add `dns_lookup_family: None` to 2 hand-written `Cluster` test initialisers; planner re-confirms count at Step 1).
- Create: `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md` (new file with Task 1 section).

**Why first:** Task 2's fixture YAML edit needs the parser to accept the new `dns_lookup_family` field. Defensive parse acceptance is the right posture per ADR-0024 (signpost 4 of SPEC §6 documents that no current consumer parses envoy.yaml through envoy-config; defensive acceptance prepares for any future test path). Task 1 also lands ADR-0024 inline per the SPEC §7 inline-at-Task-1 precedent (mirrors ADR-0021's `984aedd` and ADR-0023's `bfabcb6`).

**Scope.** ~5 LoC `Cluster` field addition + ~10 LoC `DnsLookupFamily` enum + ~1 LoC `lib.rs` re-export + ~25 LoC unit test + ~2 LoC `envoy-cluster::tests` initialiser updates + ~13 LoC ADR-0024 in DECISIONS.md + ~30 LoC PROGRESS.md Task 1 narrative = ~85 LoC total. Within SPEC §3 D1 estimate (~30 LoC code; ADR + PROGRESS in addition).

- [ ] **Step 1: Verify ADR ledger head + STATE.md routing + Cluster/Listener struct shape + cluster.rs initialiser count.**

```bash
grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -3
grep -A2 '^## Active phase' docs/envoy-rust/STATE.md | head -5
grep -n 'pub struct Cluster\b\|pub enum ClusterType\|pub struct Listener\b' crates/envoy-config/src/bootstrap.rs
grep -cn 'envoy_config::Cluster {\|^        Cluster {$' crates/envoy-cluster/src/cluster.rs
grep -n 'envoy_config::Listener {\|^    envoy_config::Listener {$' crates/envoy-tls/src/tests.rs crates/envoy-listener/src/lib.rs 2>/dev/null
```

Expected:
- ADR count `23` (latest ADR-0023 from 05.1 Task 1).
- The third grep returns `id: 05.4`, `slug: 05.4-fixture-hardening-followup`.
- The fourth grep returns lines `48:pub struct Cluster {`, `60:pub enum ClusterType {`, and `107:pub struct Listener {`.
- The fifth grep returns a count of hand-written `Cluster {}` literals in `crates/envoy-cluster/src/cluster.rs::tests` — SPEC §3 D1 says 2; the planner re-confirms here.
- The sixth grep returns 1 hit at `crates/envoy-tls/src/tests.rs:914` (`synth_listener_two_tls_chains`); 0 hits at `crates/envoy-listener/src/lib.rs` (the `mk_listener_cfg` at line 360 builds via YAML parse, not struct literal). If counts diverge from these expectations, record in PROGRESS.md and proceed with the actual count.

If any unexpected `ADR-00NN` appears beyond ADR-0023, debug per `superpowers:systematic-debugging` before continuing — phase 05.4 anticipates exactly three new ADRs (ADR-0024 / ADR-0025 / ADR-0026) and none thereafter (per SPEC §7).

- [ ] **Step 2: Append ADR-0024 to `docs/envoy-rust/DECISIONS.md`.**

Append after the existing ADR-0023 block ending at line 433 (with its trailing `---` separator):

```markdown
## ADR-0024: `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum (parse-only)

- Date: 2026-05-02
- Status: accepted
- Context: Phase 05.1's STRICT_DNS schema landing exposed a cross-platform regression on macOS Docker: Envoy v1.33's `STRICT_DNS` cluster default `dns_lookup_family: AUTO` prefers AAAA records; macOS Docker resolves `host.docker.internal` to an IPv6 address; the fixture helper backends bind on IPv4 only (per D4 in 05.4 / Fix 1 of the backup branch) and the upstream Envoy's connect to the resolved IPv6 endpoint fails with `Connection refused` → 503 to the client. Fixing this requires forcing Envoy to resolve V4_ONLY via the cluster-level `dns_lookup_family` knob; making `envoy-config`'s parser accept the new field on the existing `Cluster` struct requires extending the schema. Phase-04.3 REVIEW C-1's IPv6/IPv4 selection regression is the cross-phase carryforward driving this decision; the original Envoy v1.33 `malformed IP address` startup error is GONE after 05.1 (per 05.1 REVIEW.md §3 I1) but the residual 503 mismatch on fixture 0008 (CI run `25258722850`) requires this fix.
- Options considered: (i) **Add `dns_lookup_family: Option<DnsLookupFamily>` parse-only field with a 3-variant enum (V4Only / V6Only / Auto).** Schema growth is ~15 LoC; runtime is unchanged (envoy-rust's `tokio::net::lookup_host` returns the system-stack default; the field is parsed-and-stored at zero cost). **Chosen.** (ii) Add the field as a typed runtime knob and filter `lookup_host` results by family. Rejected: scope inflation. envoy-rust's runtime is consuming a literal IP at the substituted `127.0.0.1:port` site (envoy-rust.yaml is unchanged in 05.4); the family filter has no observable runtime effect on envoy-rust. Adding it would land code with no test that exercises it. (iii) Add only the V4_Only variant; reject V6_Only and Auto at parse time. Rejected: brittle. Envoy's proto enum has 3 variants for v1.33 (per `ENVOY_TARGET.md` pin); accepting only one would force any future fixture using V6_Only or Auto to fail-parse with a doctrine-correct reason but no clear remediation. Better to accept the full v1.33 surface upfront. (iv) Defer the schema growth; rely on field-set divergence (envoy.yaml has the field; envoy-rust.yaml does not). Rejected: SPEC §6 signpost 4 documents that some test path may need to parse envoy.yaml through envoy-config (likely the fuzz corpus walk if a planner adds a seed). Defensive parse acceptance is the right posture.
- Decision: Extend `crates/envoy-config/src/bootstrap.rs::Cluster` with `pub dns_lookup_family: Option<DnsLookupFamily>` field (defaults to `None` via `#[serde(default)]`). Add `pub enum DnsLookupFamily { V4Only, V6Only, Auto }` with `#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]`. Re-export from `crates/envoy-config/src/lib.rs`. **The field is parsed-and-stored on envoy-rust's typed Cluster struct but NOT consumed at runtime** in 05.4; the existing 05.1-landed `tokio::net::lookup_host` resolution path is unchanged. The runtime non-consumption is a deliberate scope-cap matching the C-1 fix's actual need: only the upstream Envoy side observes the V4_ONLY knob (via the per-fixture envoy.yaml D2 edit at Task 2).
- Rationale: `dns_lookup_family` is required for upstream Envoy v1.33 on macOS Docker to bypass the AAAA/A selection regression. envoy-rust's typed parser must accept the field for symmetry with Envoy's proto and for potential future test paths that parse envoy.yaml through envoy-config. The runtime non-consumption preserves D-3.6 minimalism (no code with no test exercises it); whichever later phase first needs envoy-rust to filter resolved addresses by family lands the runtime extension then, with its own test.
- Consequences:
  - `crates/envoy-config/src/bootstrap.rs::Cluster` gains the `dns_lookup_family: Option<DnsLookupFamily>` field (~5 LoC).
  - `crates/envoy-config/src/bootstrap.rs` gains the `DnsLookupFamily` enum (~10 LoC).
  - `crates/envoy-config/src/lib.rs` re-exports `DnsLookupFamily` from the public API (~1 LoC).
  - 1 new `envoy-config` parse test exercising V4_ONLY (~25 LoC).
  - 2 hand-written `Cluster` initialiser updates in `crates/envoy-cluster/src/cluster.rs::tests` (`dns_lookup_family: None`; ~2 LoC).
  - **D2 (5-fixture envoy.yaml `dns_lookup_family: V4_ONLY` edit) becomes safe to land** — the parser accepts the new field across any test path.
  - **Phase-04.3 REVIEW C-1's IPv6/IPv4 selection regression closes** at D7 (substantively, alongside the other 5 fixes).
  - V6Only and Auto runtime semantics in envoy-rust are explicitly NOT implemented in 05.4; future phase that needs them lands the runtime extension.
  - Fuzz corpus growth deferred to PLAN-discretion (PLAN signpost H elects NOT to add a new seed in 05.4).
- Provenance: This ADR was conditionally projected as ADR-0024 in 05.1 STATE.md ("the C-1 follow-up sub-phase brainstorm has first priority on this number; if the follow-up does not land an ADR, ADR-0024 stays available for 05.2 Task 1"); the 05.4 brainstorm exercises the priority. The DECISIONS.md ledger head before this commit is ADR-0023 (landed at 05.1 Task 1 `bfabcb6`); ADR-0024 lands at the next-sequential number with no renumbering needed.

---
```

Then verify:

```bash
grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
tail -30 docs/envoy-rust/DECISIONS.md
```

Expected: ADR count `24`. The tail shows the closing of ADR-0024 + a trailing `---` separator.

- [ ] **Step 3: Write the failing parse-shape test in `crates/envoy-config/src/bootstrap.rs::tests`.**

Locate the end of the existing `#[cfg(test)] mod tests { ... }` block (at the end of the file; verify via `grep -n '^mod tests\|#\[cfg(test)\]' crates/envoy-config/src/bootstrap.rs | tail -5`).

Append the following test (at the end of the `tests` block, before the closing `}` of `mod tests`):

```rust
/// 05.4 NEW (D1, ADR-0024): Cluster gains `dns_lookup_family: Option<DnsLookupFamily>`.
/// The field is parsed-and-stored on envoy-rust's typed Cluster struct; runtime
/// non-consumption is deliberate per ADR-0024 (only the upstream Envoy side
/// observes the V4_ONLY knob via the D2 envoy.yaml edit).
#[test]
fn parses_cluster_with_dns_lookup_family_v4_only() {
    let yaml = r#"
static_resources:
  clusters:
    - name: backend
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN
      dns_lookup_family: V4_ONLY
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: "host.docker.internal", port_value: 9001 } }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses");
    assert_eq!(bootstrap.clusters.len(), 1);
    let c = &bootstrap.clusters[0];
    assert!(matches!(c.cluster_type, ClusterType::StrictDns));
    assert_eq!(c.dns_lookup_family, Some(DnsLookupFamily::V4Only));
}
```

- [ ] **Step 4: Run the new test to verify it fails (compile error).**

```bash
cargo test -p envoy-config parses_cluster_with_dns_lookup_family_v4_only 2>&1 | tail -20
```

Expected: compile error — `error[E0412]: cannot find type 'DnsLookupFamily' in this scope` and/or `error[E0609]: no field 'dns_lookup_family' on type '&Cluster'`. The test does not yet compile because the enum and field do not exist.

- [ ] **Step 5: Add the `DnsLookupFamily` enum and the `Cluster.dns_lookup_family` field.**

In `crates/envoy-config/src/bootstrap.rs`, locate the `pub enum ClusterType { Static, StrictDns }` block at lines 58-72. Immediately after its closing `}` (line 72), and before the next struct (`LbPolicy` at line 74), insert:

```rust
/// DNS lookup family for STRICT_DNS / LOGICAL_DNS clusters. Mirrors Envoy
/// v1.33's `Cluster.DnsLookupFamily` proto enum (3 variants: V4_ONLY /
/// V6_ONLY / AUTO; v1.33 does not have V4_PREFERRED or ALL — those land
/// in later Envoy versions). 05.4 NEW per ADR-0024; parsed-and-stored
/// only — envoy-rust's `tokio::net::lookup_host` resolution path returns
/// the system-stack default and does NOT filter by family at runtime.
/// Whichever later phase needs the runtime filter lands it then.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum DnsLookupFamily {
    V4Only,
    V6Only,
    Auto,
}
```

Then locate the `Cluster` struct at lines 47-56. Add the new field at the end of the struct (after the existing `transport_socket: Option<TransportSocket>` field at line 55, before the closing `}`):

```rust
    /// 05.4 NEW per ADR-0024: optional DNS lookup family override for
    /// STRICT_DNS / LOGICAL_DNS clusters. Defaults to None, which lets
    /// the upstream Envoy honor its proto default (AUTO). envoy-rust does
    /// NOT consume this field at runtime in 05.4; only the upstream Envoy
    /// side observes the V4_ONLY knob via per-fixture envoy.yaml edits
    /// (D2 of phase 05.4 — see SPEC §3 D2).
    #[serde(default)]
    pub dns_lookup_family: Option<DnsLookupFamily>,
```

The struct's existing `#[serde(deny_unknown_fields)]` derive carries unchanged (the new field is opt-in via `#[serde(default)]` so existing fixtures without the field continue to deserialize cleanly).

- [ ] **Step 6: Re-export `DnsLookupFamily` from `crates/envoy-config/src/lib.rs`.**

In the `pub use bootstrap::{...}` re-export block at lines 10-19, insert `DnsLookupFamily` alphabetically. The existing list at HEAD `06b46a9` is `... CommonTlsContext, DataSource, DirectResponse, DownstreamTlsContext, ...` — and strict ASCII lexicographic order places `DnsLookupFamily` BETWEEN `DirectResponse` and `DownstreamTlsContext` (`Direct…` < `Dns…` because `i` (0x69) < `n` (0x6E); `Dns…` < `Down…` because `n` (0x6E) < `o` (0x6F)). The result:

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType, CodecType,
    CommonTlsContext, DataSource, DirectResponse, DnsLookupFamily, DownstreamTlsContext, Endpoint, FilterChain,
    FilterChainMatch, HeaderMatcher, HeaderMatcherMode, HttpConnectionManagerConfig, HttpFilter,
    HttpFilterTypedConfig, Int64Range, LbEndpoint, LbPolicy, Listener, LoadAssignment,
    LocalityLbEndpoints, NetworkFilter, Node, Route, RouteAction, RouteAction_Route,
    RouteConfiguration, RouteMatch, RouterConfig, SafeRegex, SocketAddress, StaticResources,
    StringMatcher, StringMatcherMode, TcpProxyConfig, TlsCertificate, TransportSocket,
    TransportSocketTypedConfig, TypedConfig, UpstreamTlsContext, VirtualHost,
};
```

- [ ] **Step 7: Run the new parse test to verify it passes.**

```bash
cargo test -p envoy-config parses_cluster_with_dns_lookup_family_v4_only 2>&1 | tail -5
```

Expected: `test parses_cluster_with_dns_lookup_family_v4_only ... ok` and `test result: ok. 1 passed`.

- [ ] **Step 8: Update the 2 hand-written `Cluster` test initialisers in `crates/envoy-cluster/src/cluster.rs::tests`.**

Locate the hand-written `Cluster {}` literals via:

```bash
grep -n 'Cluster {$\|Cluster {$' crates/envoy-cluster/src/cluster.rs
```

(SPEC §3 D1 says ~lines 432 and 474; verify here.) For each hit, add `dns_lookup_family: None,` immediately after the existing `transport_socket: None,` (or `transport_socket: Some(...)`) field, before the closing `}` of the struct literal. Example transformation:

```rust
// before:
let cluster = envoy_config::Cluster {
    name: "backend".to_string(),
    cluster_type: envoy_config::ClusterType::Static,
    lb_policy: envoy_config::LbPolicy::RoundRobin,
    load_assignment: ...,
    transport_socket: None,
};

// after:
let cluster = envoy_config::Cluster {
    name: "backend".to_string(),
    cluster_type: envoy_config::ClusterType::Static,
    lb_policy: envoy_config::LbPolicy::RoundRobin,
    load_assignment: ...,
    transport_socket: None,
    dns_lookup_family: None,
};
```

If the count differs from SPEC §3 D1's projection (2), update for the actual count and record in PROGRESS.md.

- [ ] **Step 9: Run the full envoy-config + envoy-cluster test suites to verify nothing regressed.**

```bash
cargo test -p envoy-config 2>&1 | tail -10
cargo test -p envoy-cluster 2>&1 | tail -10
```

Expected: both report `test result: ok` with no failures. The new test passes; existing tests continue to pass. If any existing test fails (e.g., a hand-written `Cluster {}` literal was missed), restore by adding `dns_lookup_family: None`.

- [ ] **Step 10: Run clippy + fmt on the touched crates to keep the in-tree shape clean.**

```bash
cargo clippy -p envoy-config -p envoy-cluster --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --all -- --check
```

Expected: `clippy` clean (no new warnings), `fmt --check` clean. If `fmt --check` fails, run `cargo fmt --all` and re-stage.

- [ ] **Step 11: Create `PROGRESS.md` with the Task 1 section.**

Create `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md`:

```markdown
# Phase 05.4 — Progress

Per-task running log. Append-only during execution; one section per task.
Source of truth: `SPEC.md` (D1–D7) + `PLAN.md` (Tasks 1–7).

---

## Task 1 — `envoy-config` `Cluster.dns_lookup_family` field + `DnsLookupFamily` enum + ADR-0024

- **Commit:** _(pending — fill in via post-hoc `phase 05.4: progress note (task 1)` if SHA needed cross-task)_
- **Deliverables:** SPEC §3 D1.
- **ADR landed:** ADR-0024 (`Cluster.dns_lookup_family` field + `DnsLookupFamily` enum, parse-only).
- **Files modified:**
  - `docs/envoy-rust/DECISIONS.md` (ADR-0024 appended after ADR-0023).
  - `crates/envoy-config/src/bootstrap.rs` (`DnsLookupFamily` enum; `Cluster.dns_lookup_family` field; `parses_cluster_with_dns_lookup_family_v4_only` parse test).
  - `crates/envoy-config/src/lib.rs` (re-export `DnsLookupFamily`).
  - `crates/envoy-cluster/src/cluster.rs` (2 hand-written `Cluster` initialisers gain `dns_lookup_family: None`; planner-confirmed count).
- **LoC:** ~85 (5 field + 10 enum + 1 re-export + 25 parse test + 2 initialiser updates + 13 ADR + 30 PROGRESS narrative).
- **Verification:**
  - `cargo test -p envoy-config parses_cluster_with_dns_lookup_family_v4_only` — `test result: ok. 1 passed`.
  - `cargo test -p envoy-config` — `test result: ok` (existing tests unchanged).
  - `cargo test -p envoy-cluster` — `test result: ok` (existing tests unchanged after the 2 initialiser updates).
  - `cargo clippy -p envoy-config -p envoy-cluster --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean.
- **Deviations from PLAN:** _(record any here; expected: none.)_
- **Carryforward note:** None — Task 1 is mechanically scoped per SPEC §3 D1.
- **Fuzz seed:** Not added (per PLAN signpost H — optional, deferred to PLAN discretion; planner elected NOT to add).
```

- [ ] **Step 12: Commit.**

```bash
git status
git diff --stat
git add docs/envoy-rust/DECISIONS.md \
        crates/envoy-config/src/bootstrap.rs \
        crates/envoy-config/src/lib.rs \
        crates/envoy-cluster/src/cluster.rs \
        docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md
git commit -m "phase 05.4: Cluster.dns_lookup_family + DnsLookupFamily enum + ADR-0024 (task 1)"
```

Expected: clean commit; no other files modified. Verify via `git status` post-commit returns `nothing to commit, working tree clean` (modulo the existing untracked artifacts noted in initial repo status).

---

### Task 2: 5-fixture coordinated YAML edit — `dns_lookup_family: V4_ONLY` on `tests/fixtures/{0003,0004,0005,0006,0008}/envoy.yaml`

**Files:**
- Modify: `tests/fixtures/0003-tcp-proxy/envoy.yaml` (add `      dns_lookup_family: V4_ONLY` line immediately after `      type: STRICT_DNS` at line 27).
- Modify: `tests/fixtures/0004-tls-downstream/envoy.yaml` (same pattern after line 37).
- Modify: `tests/fixtures/0005-tls-upstream/envoy.yaml` (same pattern after line 16).
- Modify: `tests/fixtures/0006-tls-sni/envoy.yaml` (same pattern after line 40 — Task 3 will additionally add the listener_filters block at line 6).
- Modify: `tests/fixtures/0008-http1-router-upstream/envoy.yaml` (same pattern after line 49).

**Why second:** depends on Task 1's schema (defensive parse acceptance); produces the upstream-Envoy-side runtime knob that enables fixtures 0003/0004/0005/0006/0008 to resolve `host.docker.internal` to IPv4 instead of (the macOS-Docker default) IPv6. **`envoy-rust.yaml` is NOT edited** because envoy-rust uses `127.0.0.1` literal IP at the substituted `{{BACKEND_HOST}}` site and DNS family selection has no runtime semantics on envoy-rust per ADR-0024.

**Scope.** ~5 LoC YAML diff total (5 files × 1 line each). One bundled commit per SPEC §6 signpost 15 / PLAN signpost G.

- [ ] **Step 1: Verify the 5 affected envoy.yaml line numbers + that envoy-rust.yaml siblings are not touched.**

```bash
grep -n 'type: STRICT_DNS' tests/fixtures/0003-tcp-proxy/envoy.yaml \
                          tests/fixtures/0004-tls-downstream/envoy.yaml \
                          tests/fixtures/0005-tls-upstream/envoy.yaml \
                          tests/fixtures/0006-tls-sni/envoy.yaml \
                          tests/fixtures/0008-http1-router-upstream/envoy.yaml
grep -L 'host.docker.internal\|BACKEND_HOST' tests/fixtures/0001-tcp-echo/envoy.yaml \
                                              tests/fixtures/0002-static-admin-ready/envoy.yaml \
                                              tests/fixtures/0007-http1-direct-response/envoy.yaml
```

Expected:
- The first grep returns 5 hits (one per file): line 27, 37, 16, 40, 49 respectively.
- The second grep returns all 3 file paths (none of them reference the substitution token), confirming fixtures 0001/0002/0007 are correctly NOT in the affected set per SPEC §1.

- [ ] **Step 2: Edit each of the 5 envoy.yaml files.**

For each affected file, insert the new line immediately after the existing `type: STRICT_DNS` line, with the same leading indentation. Example (for `tests/fixtures/0003-tcp-proxy/envoy.yaml`, line 27):

```yaml
# before:
      type: STRICT_DNS
      lb_policy: ROUND_ROBIN

# after:
      type: STRICT_DNS
      dns_lookup_family: V4_ONLY
      lb_policy: ROUND_ROBIN
```

Verify after each file:

```bash
grep -A1 'type: STRICT_DNS' tests/fixtures/0003-tcp-proxy/envoy.yaml \
                            tests/fixtures/0004-tls-downstream/envoy.yaml \
                            tests/fixtures/0005-tls-upstream/envoy.yaml \
                            tests/fixtures/0006-tls-sni/envoy.yaml \
                            tests/fixtures/0008-http1-router-upstream/envoy.yaml
```

Expected: each `type: STRICT_DNS` line is followed by `dns_lookup_family: V4_ONLY` with matching indentation (6 spaces). 5 files modified; no other files touched.

- [ ] **Step 3: Verify the envoy-rust.yaml siblings are NOT modified.**

```bash
grep -n 'dns_lookup_family' tests/fixtures/000*/envoy-rust.yaml 2>&1 | head
```

Expected: empty (no hits). The 5 envoy-rust.yaml siblings remain unchanged because envoy-rust performs DNS family selection at the system stack and does not honor the cluster-level knob (per ADR-0024 / PLAN signpost B).

- [ ] **Step 4: Append the Task 2 section to PROGRESS.md.**

```markdown
## Task 2 — 5-fixture coordinated `dns_lookup_family: V4_ONLY` edit

- **Commit:** _(pending — fill in post-hoc if needed)_
- **Deliverables:** SPEC §3 D2.
- **ADR landed:** None (D2 has no ADR; the ADR-0024 grant landed at Task 1).
- **Files modified:**
  - `tests/fixtures/0003-tcp-proxy/envoy.yaml` (line 28 inserted).
  - `tests/fixtures/0004-tls-downstream/envoy.yaml` (line 38 inserted).
  - `tests/fixtures/0005-tls-upstream/envoy.yaml` (line 17 inserted).
  - `tests/fixtures/0006-tls-sni/envoy.yaml` (line 41 inserted; further edits at Task 3).
  - `tests/fixtures/0008-http1-router-upstream/envoy.yaml` (line 50 inserted).
- **LoC:** 5 (1 line per fixture).
- **Cadence:** single bundled commit per SPEC §6 signpost 15 + PLAN signpost G.
- **Verification:**
  - `grep -A1 'type: STRICT_DNS' tests/fixtures/000{3,4,5,6,8}*/envoy.yaml` shows each `type:` line followed by `dns_lookup_family: V4_ONLY`.
  - `grep -n 'dns_lookup_family' tests/fixtures/000*/envoy-rust.yaml` returns empty (the envoy-rust.yaml siblings are intentionally unchanged).
  - The actual differential green re-baseline is at Task 7 — Tasks 2-6 land progressively; the gate fires once.
- **Deviations from PLAN:** _(none expected)_
```

- [ ] **Step 5: Commit.**

```bash
git status
git diff --stat
git add tests/fixtures/0003-tcp-proxy/envoy.yaml \
        tests/fixtures/0004-tls-downstream/envoy.yaml \
        tests/fixtures/0005-tls-upstream/envoy.yaml \
        tests/fixtures/0006-tls-sni/envoy.yaml \
        tests/fixtures/0008-http1-router-upstream/envoy.yaml \
        docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md
git commit -m "phase 05.4: 5-fixture coordinated YAML edit — dns_lookup_family: V4_ONLY (task 2)"
```

Expected: clean commit; only the 5 envoy.yaml files + PROGRESS.md modified. Verify via `git status` returns clean.

---

### Task 3: `envoy-config` — `Listener.listener_filters` parse-and-ignore field + ADR-0026 inline + 1 new validator unit test + 1 hand-written `Listener` initialiser update + fixture 0006 `tls_inspector` block

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md` (append ADR-0026 after the Task 1-landed ADR-0024 block).
- Modify: `crates/envoy-config/src/bootstrap.rs` (extend `Listener` struct at lines 106-112 with `listener_filters: Vec<serde_yaml::Value>` field; append `parses_listener_with_tls_inspector_listener_filter` test to the existing `#[cfg(test)] mod tests` block).
- Modify: `crates/envoy-tls/src/tests.rs` (add `listener_filters: vec![]` to the hand-written `envoy_config::Listener` literal in `synth_listener_two_tls_chains` at lines 914-924).
- Modify: `tests/fixtures/0006-tls-sni/envoy.yaml` (add the explicit `listener_filters: [tls_inspector]` block immediately after the `address:` line at line 6).

**Why third:** Task 3 is mechanically coupled per SPEC §6 signpost 12 — the schema growth and the fixture YAML edit MUST land in the same commit. Splitting them would either (a) red the parser (block exists; field doesn't) or (b) red the upstream Envoy on macOS Docker (field exists; block missing). Independent of Tasks 4/5/6.

**Scope.** ~5 LoC `Listener` field addition + ~85 LoC new parse test (with embedded YAML payload) + ~1 LoC `synth_listener_two_tls_chains` initialiser update + ~9 LoC fixture 0006 envoy.yaml block + ~13 LoC ADR-0026 in DECISIONS.md + ~25 LoC PROGRESS.md = ~138 LoC total. Within SPEC §3 D3 estimate (~130 LoC).

- [ ] **Step 1: Append ADR-0026 to `docs/envoy-rust/DECISIONS.md`.**

Append after the Task 1-landed ADR-0024 block (note: ADR-0025 is NOT landed yet — it lands at Task 5; ADR-0026 lands BEFORE ADR-0025 in landing-time order per SPEC §6 signpost 9):

```markdown
## ADR-0026: `Listener.listener_filters` parse-and-ignore field in `envoy-config` (new pattern)

- Date: 2026-05-02
- Status: accepted
- Context: Phase 05.1 Task 4's CI run revealed that fixture 0006's TLS-SNI test was masked behind fixture 0008's earlier failure (alphabetic ordering); after the other fixes land, fixture 0006 surfaces as RED on macOS Docker because Envoy v1.33 does NOT auto-inject the TLS inspector listener filter for SNI-based filter chain selection (the auto-injection works on Linux but not on the Docker-Desktop/macOS combination — verified by the 05.1 aborted attempt at backup branch `backup/task4-scope-creep-2026-05-02` commit `9279895`). The fix on the upstream Envoy side is to declare the listener filter explicitly in `envoy.yaml`: `listener_filters: [{name: envoy.filters.listener.tls_inspector, ...}]`. envoy-rust performs SNI dispatch at the rustls layer (per phase 03.2's design) and does NOT execute listener filters; the field has no envoy-rust runtime semantics. However, with `envoy-config`'s parser using `#[serde(deny_unknown_fields)]` on every struct including `Listener`, any test path that parses fixture 0006's envoy.yaml through envoy-config would fail-reject the new field — and even though no current test path does so (PLAN signpost from 05.4 SPEC §6 signpost 4), defensive acceptance is doctrinally cleaner than perpetual field-set divergence.
- Options considered: (i) **Add `Listener.listener_filters: Vec<serde_yaml::Value>` parse-and-ignore field with `#[serde(default)]`.** Stores listener-filter blocks as opaque `serde_yaml::Value`; envoy-rust never inspects or executes them. **Chosen.** (ii) Add `Listener.listener_filters: Vec<ListenerFilter>` typed-and-ignored field with a typed `ListenerFilter` enum exhausting the v1.33 set (`tls_inspector`, `original_dst`, `original_src`, `proxy_protocol`, `http_inspector`). Rejected: scope inflation. The typed enum would need ~5 variants × ~10 LoC each + per-variant typed_config payloads + 5+ parse tests; envoy-rust gains zero runtime semantics from typing them; only one variant is needed for fixture 0006's actual fix. (iii) Defer the schema growth; rely on field-set divergence (envoy.yaml has listener_filters; envoy-rust.yaml does not; no test path parses envoy.yaml through envoy-config). Rejected: brittle. SPEC §6 signpost 4 documents that the open question of "which test path parses envoy.yaml through envoy-config" may be answered YES by a future planner who adds an envoy.yaml-parsing test or fuzz seed. Defensive acceptance is the right default. (iv) Add the field as a strict `#[serde(skip)]` ignored-at-deserialization field. Rejected: this would skip the field at deserialization time entirely (the `Vec<serde_yaml::Value>` would always be empty), losing the ability for any future test or audit to introspect the parsed listener filters. Storing them as `Vec<serde_yaml::Value>` preserves the ability to inspect (e.g., a test could assert "fixture 0006 declares the tls_inspector filter" without typing the inspector itself).
- Decision: Extend `crates/envoy-config/src/bootstrap.rs::Listener` with `pub listener_filters: Vec<serde_yaml::Value>` field (defaults to `vec![]` via `#[serde(default)]`). The field is parsed-and-stored as opaque YAML values; envoy-rust does NOT interpret or execute them at runtime. Add a parse test (`parses_listener_with_tls_inspector_listener_filter`) exercising the full bootstrap with a TLS-bearing listener carrying the tls_inspector block. Add `listener_filters: vec![]` to the one hand-written `Listener` initialiser in `crates/envoy-tls/src/tests.rs::synth_listener_two_tls_chains`. Add the explicit `tls_inspector` block to fixture 0006's `envoy.yaml` (only — `envoy-rust.yaml` is unchanged because envoy-rust's SNI dispatch lives at the rustls layer).
- Rationale: This is the **introduction of a new pattern in envoy-config**: parse-and-ignore for fields that envoy-rust cannot or will not consume at runtime but that upstream Envoy requires for fixture validity. Every prior YAML divergence used field-set divergence (the field exists in envoy.yaml and is absent from envoy-rust.yaml). The parse-and-ignore pattern is the right call for `listener_filters` specifically because: (a) the field carries arbitrary listener-filter typed_config payloads (multiple filter types possible; future Envoy versions may surface more); typing the variants exhaustively would be a non-trivial growth surface; (b) envoy-rust never executes listener filters by design (architectural choice from phase 03.2 — SNI lives in the rustls layer); (c) making the parse-and-ignore explicit at the schema level is more honest than maintaining field-set divergence forever, and prepares for any future test path that parses envoy.yaml through envoy-config.
- Consequences:
  - `crates/envoy-config/src/bootstrap.rs::Listener` gains the `listener_filters: Vec<serde_yaml::Value>` field (~5 LoC).
  - 1 new `envoy-config` parse test exercising the tls_inspector block (~85 LoC including the YAML payload).
  - 1 hand-written `Listener` initialiser in `crates/envoy-tls/src/tests.rs` updated (`listener_filters: vec![]`; ~1 LoC).
  - `tests/fixtures/0006-tls-sni/envoy.yaml` gains the explicit listener_filters block (~9 LoC YAML).
  - **Fixture 0006's TLS-SNI handshake succeeds** on macOS Docker — substantively closes one of phase-04.3 REVIEW C-1's three latent regressions.
  - **The parse-and-ignore pattern is now a documented envoy-config posture.** Future fields that meet the criteria (Envoy-config-only with no envoy-rust runtime semantics; required for upstream-Envoy `envoy.yaml` parseability under any test path; reviewed under D-3.5 ambiguity-resolution discipline) may follow the same pattern. Whichever later phase first needs to ACTUALLY EXECUTE a listener filter lands a typed-variant extension on the field plus a runtime dispatch arm — not a new ADR (extending an existing pattern).
- Provenance: This ADR was conditionally projected as ADR-0024–0026 in 05.1 STATE.md. ADR-0024 (DnsLookupFamily) lands at 05.4 Task 1; this ADR lands at 05.4 Task 3; ADR-0025 (CL: 0 suppression) lands at 05.4 Task 5. The DECISIONS.md ledger is append-only and lists ADRs in landing-time order: ADR-0023 → ADR-0024 → ADR-0026 → ADR-0025.

---
```

Verify:

```bash
grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -3
```

Expected: ADR count `25`. The tail shows `ADR-0023`, `ADR-0024`, `ADR-0026` in that order.

- [ ] **Step 2: Write the failing parse-shape test in `crates/envoy-config/src/bootstrap.rs::tests`.**

Append (at the end of the existing `#[cfg(test)] mod tests { ... }` block, before the closing `}`):

```rust
/// 05.4 NEW (D3, ADR-0026): Listener gains `listener_filters: Vec<serde_yaml::Value>`
/// parse-and-ignore field. envoy-rust never executes listener filters by design
/// (phase 03.2 chose to put SNI dispatch at the rustls layer); the field is
/// purely for upstream-Envoy `envoy.yaml` parseability. New pattern in
/// envoy-config — see ADR-0026.
#[test]
fn parses_listener_with_tls_inspector_listener_filter() {
    let yaml = r#"
static_resources:
  listeners:
    - name: tcp_listener
      address: { socket_address: { address: "0.0.0.0", port_value: 0 } }
      listener_filters:
        - name: envoy.filters.listener.tls_inspector
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.listener.tls_inspector.v3.TlsInspector
      filter_chains:
        - filter_chain_match: { server_names: ["a.example.com"] }
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { inline_string: "-----BEGIN CERTIFICATE-----\nMIIB-fake-cert\n-----END CERTIFICATE-----\n" }
                    private_key:       { inline_string: "-----BEGIN PRIVATE KEY-----\nMIIB-fake-key\n-----END PRIVATE KEY-----\n" }
          filters:
            - name: envoy.filters.network.tcp_proxy
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy
                stat_prefix: ingress_tcp
                cluster: backend
  clusters:
    - name: backend
      type: STATIC
      lb_policy: ROUND_ROBIN
      load_assignment:
        cluster_name: backend
        endpoints:
          - lb_endpoints:
              - endpoint:
                  address: { socket_address: { address: "127.0.0.1", port_value: 9001 } }
"#;
    let bootstrap = crate::parse_bootstrap(yaml).expect("parses");
    assert_eq!(bootstrap.listeners.len(), 1);
    let listener = &bootstrap.listeners[0];
    assert_eq!(listener.listener_filters.len(), 1);
    // Smoke-check the opaque value contains the tls_inspector filter name.
    let filter_yaml = serde_yaml::to_string(&listener.listener_filters[0])
        .expect("filter serialises back");
    assert!(
        filter_yaml.contains("envoy.filters.listener.tls_inspector"),
        "filter yaml should contain tls_inspector name: {filter_yaml:?}"
    );
}
```

(Note: the embedded cert/key are stub strings; the parse path doesn't validate them. If the parse rejects them as malformed, replace with `filename:` references to a non-existent path — `inline_string` is accepted by the schema as a `DataSource` shape; the planner verifies the actual `DataSource` enum at edit time and adapts if needed. The point of the test is to assert listener_filters parses; the TLS context is incidental scaffolding required by `Listener.filter_chains[*].transport_socket` validation.)

- [ ] **Step 3: Run the new test to verify it fails (compile error).**

```bash
cargo test -p envoy-config parses_listener_with_tls_inspector_listener_filter 2>&1 | tail -20
```

Expected: compile error — `error[E0609]: no field 'listener_filters' on type '&Listener'`. The test does not yet compile because the field does not exist.

- [ ] **Step 4: Add the `Listener.listener_filters` field.**

In `crates/envoy-config/src/bootstrap.rs`, locate the `Listener` struct at lines 105-112. Add the new field at the end of the struct (after the existing `filter_chains: Vec<FilterChain>` field at line 111, before the closing `}`):

```rust
    /// 05.4 NEW per ADR-0026: optional listener filters declared by the
    /// upstream Envoy `envoy.yaml`. Parse-and-ignore: stored as opaque
    /// `serde_yaml::Value`s; envoy-rust does NOT execute listener filters
    /// (SNI dispatch lives at the rustls layer per phase 03.2). The field
    /// is accepted purely so envoy.yaml fixtures including a
    /// `listener_filters: [...]` block do not trigger `deny_unknown_fields`
    /// rejection on any path that parses envoy.yaml through envoy-config.
    #[serde(default)]
    pub listener_filters: Vec<serde_yaml::Value>,
```

The struct's existing `#[serde(deny_unknown_fields)]` derive carries unchanged.

- [ ] **Step 5: Run the parse test to verify it passes.**

```bash
cargo test -p envoy-config parses_listener_with_tls_inspector_listener_filter 2>&1 | tail -10
```

Expected: `test parses_listener_with_tls_inspector_listener_filter ... ok`. If the test fails because of an embedded-cert serde issue, replace `inline_string:` with `filename: "/dev/null"` (the parse path accepts the syntactic shape; runtime cert loading does not run during parse) and re-run.

- [ ] **Step 6: Update the `synth_listener_two_tls_chains` literal in `crates/envoy-tls/src/tests.rs:914-924`.**

Locate via `grep -n 'envoy_config::Listener {' crates/envoy-tls/src/tests.rs` (expect line 914). Insert `listener_filters: vec![],` into the struct literal — specifically, after the `filter_chains: vec![chain_a, chain_b],` line (line 922) and before the closing `}` of the literal (line 923). Resulting struct:

```rust
    envoy_config::Listener {
        name: "tcp_listener".to_string(),
        address: envoy_config::Address {
            socket_address: envoy_config::SocketAddress {
                address: "0.0.0.0".to_string(),
                port_value: 10010,
            },
        },
        filter_chains: vec![chain_a, chain_b],
        listener_filters: vec![],
    }
```

- [ ] **Step 7: Add the `tls_inspector` block to `tests/fixtures/0006-tls-sni/envoy.yaml`.**

Insert after line 6 (`address:` line) and before line 7 (`filter_chains:` line):

```yaml
      listener_filters:
        - name: envoy.filters.listener.tls_inspector
          typed_config:
            "@type": type.googleapis.com/envoy.extensions.filters.listener.tls_inspector.v3.TlsInspector
```

The block uses 6 spaces of leading indentation (matching the `filter_chains:` block's indentation; the block sits at the same YAML hierarchy level as `address:` and `filter_chains:` — children of the `tcp_listener:` listener).

Verify:

```bash
grep -A4 'listener_filters:' tests/fixtures/0006-tls-sni/envoy.yaml
```

Expected: the new block appears with the tls_inspector typed_config.

- [ ] **Step 8: Run the touched test suites to verify nothing regressed.**

```bash
cargo test -p envoy-config 2>&1 | tail -10
cargo test -p envoy-tls 2>&1 | tail -15
```

Expected: both report `test result: ok`. The new test passes; existing envoy-tls tests (which call `synth_listener_two_tls_chains`) continue to pass with the new `listener_filters: vec![]` field.

- [ ] **Step 9: Run clippy + fmt.**

```bash
cargo clippy -p envoy-config -p envoy-tls --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --all -- --check
```

Expected: clean. If `fmt --check` fails, run `cargo fmt --all` and re-stage.

- [ ] **Step 10: Append the Task 3 section to PROGRESS.md.**

```markdown
## Task 3 — `envoy-config` `Listener.listener_filters` parse-and-ignore + ADR-0026 + fixture 0006 tls_inspector block

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D3.
- **ADR landed:** ADR-0026 (`Listener.listener_filters` parse-and-ignore field; new pattern in envoy-config).
- **Files modified:**
  - `docs/envoy-rust/DECISIONS.md` (ADR-0026 appended after ADR-0024).
  - `crates/envoy-config/src/bootstrap.rs` (`Listener.listener_filters` field; `parses_listener_with_tls_inspector_listener_filter` parse test).
  - `crates/envoy-tls/src/tests.rs` (`synth_listener_two_tls_chains` gains `listener_filters: vec![]`).
  - `tests/fixtures/0006-tls-sni/envoy.yaml` (explicit `tls_inspector` listener-filter block inserted).
- **LoC:** ~138 (5 field + 85 parse test + 1 initialiser update + 9 fixture YAML + 13 ADR + 25 PROGRESS narrative).
- **Coupling per SPEC §6 signpost 12:** schema + fixture YAML in same commit (splitting would red the parser or red Envoy on macOS Docker).
- **Verification:**
  - `cargo test -p envoy-config parses_listener_with_tls_inspector_listener_filter` — `test result: ok. 1 passed`.
  - `cargo test -p envoy-config` — `test result: ok` (existing tests unchanged).
  - `cargo test -p envoy-tls` — `test result: ok` (existing tests still pass after the literal update).
  - `cargo clippy -p envoy-config -p envoy-tls --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean.
- **Deviations from PLAN:** _(record any here; e.g., if the parse test's embedded TLS cert needed a shape adjustment.)_
- **Pattern note:** parse-and-ignore is now a documented envoy-config posture per ADR-0026. Future fields meeting the criteria may follow the same pattern without a new ADR.
```

- [ ] **Step 11: Commit.**

```bash
git status
git diff --stat
git add docs/envoy-rust/DECISIONS.md \
        crates/envoy-config/src/bootstrap.rs \
        crates/envoy-tls/src/tests.rs \
        tests/fixtures/0006-tls-sni/envoy.yaml \
        docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md
git commit -m "phase 05.4: Listener.listener_filters parse-and-ignore + fixture 0006 tls_inspector + ADR-0026 (task 3)"
```

Expected: clean commit; only the listed files modified.

---

### Task 4: 3 echo-server helper bind flips — `tcp-echo-server`, `tls-echo-server`, `http1-echo-server`

**Files:**
- Modify: `tests/helpers/tcp-echo-server/src/main.rs` (line 118 bind; line 119 tracing log; line 3 doc comment).
- Modify: `tests/helpers/tls-echo-server/src/main.rs` (line 109 bind; line 110 tracing log; line 3 doc comment).
- Modify: `tests/helpers/http1-echo-server/src/main.rs` (line 98 bind; line 99 tracing log; line 3 doc comment).

**Why fourth:** independent of Tasks 1-3. Mechanically minimal (1 bind line + 1 log line + 1 doc-comment line per helper, 9 total LoC). The flip is observable mechanically — `0.0.0.0` binds all interfaces including loopback, so existing tests that connect via `127.0.0.1:0` continue unchanged. **No new tests needed** (per SPEC §3 D4 + PLAN signpost J).

**Scope.** ~10 LoC across 3 files (3 bind + 3 log + 3 doc-comment). No ADR. No tests.

- [ ] **Step 1: Confirm bind-line locations + doc-comment shapes.**

```bash
grep -n 'TcpListener::bind(("127.0.0.1"\|TcpListener::bind(("0.0.0.0"' \
        tests/helpers/tcp-echo-server/src/main.rs \
        tests/helpers/tls-echo-server/src/main.rs \
        tests/helpers/http1-echo-server/src/main.rs
head -5 tests/helpers/tcp-echo-server/src/main.rs tests/helpers/tls-echo-server/src/main.rs tests/helpers/http1-echo-server/src/main.rs
```

Expected:
- `tcp-echo-server/src/main.rs:118` is `TcpListener::bind(("127.0.0.1", port))`. (Lines 212 and 236 use `"127.0.0.1:0"` for ephemeral test ports — those are test-internal and stay unchanged; only the production bind at line 118 flips.)
- `tls-echo-server/src/main.rs:109` is `TcpListener::bind(("127.0.0.1", args.port))`. (Line 281 is a test-internal `"127.0.0.1:0"` ephemeral and stays.)
- `http1-echo-server/src/main.rs:98` is `TcpListener::bind(("127.0.0.1", args.port))`. (Line 332 is a test-internal `"127.0.0.1:0"` ephemeral and stays.)
- The `head` output shows each helper's `//!` doc-comment header containing `localhost-only` language.

- [ ] **Step 2: Flip the 3 bind lines to `"0.0.0.0"`.**

For `tests/helpers/tcp-echo-server/src/main.rs:118`:
- before: `let listener = TcpListener::bind(("127.0.0.1", port)).await?;`
- after: `let listener = TcpListener::bind(("0.0.0.0", port)).await?;`

For `tests/helpers/tls-echo-server/src/main.rs:109`:
- before: `let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;`
- after: `let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;`

For `tests/helpers/http1-echo-server/src/main.rs:98`:
- before: `let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;`
- after: `let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;`

The test-internal ephemeral ports (`"127.0.0.1:0"` at lines 212/236 of tcp-echo-server, 281 of tls-echo-server, 332 of http1-echo-server) are unchanged.

- [ ] **Step 3: Update the 3 tracing log lines.**

For `tests/helpers/tcp-echo-server/src/main.rs:119`:
- before: `tracing::info!(port, "tcp-echo-server listening");`
- after: `tracing::info!(port, "tcp-echo-server listening on 0.0.0.0:{port}");`

For `tests/helpers/tls-echo-server/src/main.rs:110`:
- before: `tracing::info!("tls-echo-server listening on 127.0.0.1:{}", args.port);`
- after: `tracing::info!("tls-echo-server listening on 0.0.0.0:{}", args.port);`

For `tests/helpers/http1-echo-server/src/main.rs:99`:
- before: `tracing::info!("http1-echo-server listening on 127.0.0.1:{}", args.port);`
- after: `tracing::info!("http1-echo-server listening on 0.0.0.0:{}", args.port);`

- [ ] **Step 4: Update the 3 doc-comment headers per PLAN signpost J.**

For `tests/helpers/tcp-echo-server/src/main.rs:3`:
- before: `//! `tcp-echo-server` — a minimal localhost-only echo server for the envoy-rust`
- after: `//! `tcp-echo-server` — a minimal echo server for the envoy-rust`

For `tests/helpers/tls-echo-server/src/main.rs:3`:
- before: `//! `tls-echo-server` — a minimal localhost-only TLS echo server for the`
- after: `//! `tls-echo-server` — a minimal TLS echo server for the`

For `tests/helpers/http1-echo-server/src/main.rs:3`:
- before: `//! `http1-echo-server` — minimal localhost-only HTTP/1.1 echo server for the`
- after: `//! `http1-echo-server` — minimal HTTP/1.1 echo server for the`

- [ ] **Step 5: Run the helpers' test suites to verify nothing regressed.**

```bash
cargo test -p tcp-echo-server -p tls-echo-server -p http1-echo-server 2>&1 | tail -20
```

(If the helpers are workspace members under different package names, `grep -n '^name' tests/helpers/*/Cargo.toml` first to confirm package names.)

Expected: all 3 helpers' test suites green. The bind-address flip is mechanically transparent — `0.0.0.0` is a superset of `127.0.0.1` reachability, so existing tests connecting to `127.0.0.1:<port>` still hit the listener.

- [ ] **Step 6: Run clippy + fmt.**

```bash
cargo clippy -p tcp-echo-server -p tls-echo-server -p http1-echo-server --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 7: Append the Task 4 section to PROGRESS.md.**

```markdown
## Task 4 — 3 echo-server helper bind flips (0.0.0.0)

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D4.
- **ADR landed:** None (D4 has no ADR; ADR-0015's host-gateway grant is the operative cross-reference).
- **Files modified:**
  - `tests/helpers/tcp-echo-server/src/main.rs` (line 118 bind; line 119 tracing log; line 3 doc comment).
  - `tests/helpers/tls-echo-server/src/main.rs` (line 109 bind; line 110 tracing log; line 3 doc comment).
  - `tests/helpers/http1-echo-server/src/main.rs` (line 98 bind; line 99 tracing log; line 3 doc comment).
- **LoC:** ~10 (3 bind + 3 log + 3 doc-comment + slight wording adjustments).
- **Verification:**
  - `cargo test -p tcp-echo-server -p tls-echo-server -p http1-echo-server` — all green.
  - `cargo clippy ... -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean.
  - Test-internal ephemeral binds at lines 212/236/281/332 (`"127.0.0.1:0"`) are intentionally unchanged.
- **Deviations from PLAN:** _(none expected)_
```

- [ ] **Step 8: Commit.**

```bash
git add tests/helpers/tcp-echo-server/src/main.rs \
        tests/helpers/tls-echo-server/src/main.rs \
        tests/helpers/http1-echo-server/src/main.rs \
        docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md
git commit -m "phase 05.4: 3 echo-server helpers bind 0.0.0.0 (task 4)"
```

Expected: clean commit; only the listed files modified.

---

### Task 5: `envoy-http1::Client::send_request` — suppress `content-length: 0` on empty-body requests + ADR-0025 inline + 1 unit test assertion flip + fixture 0008 expectations update

**Files:**
- Modify: `docs/envoy-rust/DECISIONS.md` (append ADR-0025 after the Task 3-landed ADR-0026 block).
- Modify: `crates/envoy-http1/src/client.rs` (request-write CL emission at lines 94-103; unit test assertion at lines 460-463).
- Modify: `tests/fixtures/0008-http1-router-upstream/expectations.yaml` (drop `  content-length: 0\n` from `expected_body` line 9).

**Why fifth:** Task 5 is mechanically coupled per SPEC §6 signpost 11 — the client behavior change and the expectations update MUST land in the same commit. Splitting them would either (a) red the client unit test (assertion expects `content-length: 0` but client no longer emits it) or (b) red fixture 0008 differential equivalence (request body now omits CL: 0 on envoy-rust side, but expected body still includes it). Independent of Tasks 4/6.

**Scope.** ~5 LoC client predicate addition + ~3 LoC unit test assertion flip + ~1 LoC fixture expectations.yaml diff + ~13 LoC ADR-0025 + ~25 LoC PROGRESS.md = ~47 LoC total. Within SPEC §3 D5 estimate (~30 LoC code, plus ADR + PROGRESS).

- [ ] **Step 1: Append ADR-0025 to `docs/envoy-rust/DECISIONS.md`.**

Append after the ADR-0026 block landed at Task 3:

```markdown
## ADR-0025: Suppress `content-length: 0` on empty-body GET in `envoy-http1::client` (RFC 7230 §3.3.2 + Envoy v1.33 parity)

- Date: 2026-05-02
- Status: accepted
- Context: envoy-http1's client at HEAD `1d05cd0` injects a synthetic `content-length: <len>` header on every outbound request that doesn't already carry an explicit Content-Length. For empty-body requests (e.g., the HTTP/1.1 GET that fixture 0008's `Driver::Http1` issues), this emits `content-length: 0` on the wire. Envoy v1.33 honors RFC 7230 §3.3.2 ("A user agent SHOULD NOT send a Content-Length header field when the request message does not contain a payload body and the method semantics do not anticipate such a body") and OMITS Content-Length on empty-body requests. Fixture 0008's deterministic-echo body shape is a byte-for-byte alphabetic list of received headers + the body bytes; the spurious envoy-rust-side `content-length: 0` lands in the echoed body and breaks `response_body: byte_exact` against the upstream Envoy side that omits it.
- Options considered: (i) **Suppress synthetic `content-length: 0` on empty-body requests; pass through explicit Content-Length unchanged.** Behaviour change: only inject when body is non-empty AND no explicit CL is set. **Chosen.** (ii) Always emit `content-length: <len>` (status quo). Rejected: violates RFC 7230 §3.3.2; breaks fixture 0008 differential equivalence. (iii) Always emit `content-length: 0` for empty-body GET; update upstream Envoy fixture to inject `content-length: 0` on its side too via `request_headers_to_add`. Rejected: increases envoy.yaml-side asymmetry burden; is the wrong direction (fixture YAML bending around envoy-rust's misbehaviour rather than fixing envoy-rust); doesn't honor the RFC. (iv) Make the suppression a configurable HCM/Router knob. Rejected: scope inflation. RFC compliance is not a per-request opt-in; it's the correct default. If a future use case wants explicit CL: 0 (e.g., to comply with a specific upstream's quirks), it can pass an explicit Content-Length on the request, which the new code correctly passes through unchanged.
- Decision: Modify `crates/envoy-http1/src/client.rs::Client::send_request` request-write path: only inject the synthetic `content-length: <len>` header when (a) the request does not carry an explicit Content-Length AND (b) the request body is non-empty. The `body_is_nonempty` predicate uses the existing `Request::body_bytes() -> Option<&[u8]>` accessor at `crates/envoy-http1/src/codec.rs:62-64`. The 1 affected envoy-http1 unit test in `crates/envoy-http1/src/client.rs::tests` (`send_request_writes_serialized_request_bytes`) flips its assertion from `s.contains("content-length: 0\r\n")` to `!s.contains("content-length: 0\r\n")`. Fixture 0008's `expectations.yaml` removes `  content-length: 0\n` from the expected echo body.
- Rationale: RFC 7230 §3.3.2 is unambiguous: empty-body requests SHOULD NOT carry Content-Length. Envoy v1.33 honors this. Fixture 0008's differential property is "envoy ↔ envoy-rust byte-equal echo body" — both proxies must omit the header for the fixture to be green. The fix is small (~10 LoC) and correctly bounded to empty-body requests: requests with explicit Content-Length pass through unchanged (preserving any caller's deliberate Content-Length emission); requests with non-empty body continue to emit synthetic Content-Length (preserving the existing happy path).
- Consequences:
  - `crates/envoy-http1/src/client.rs` request-write path gains a `body_is_nonempty` check (~5 LoC).
  - 1 envoy-http1 unit test flips its CL: 0 assertion (~5 LoC).
  - `tests/fixtures/0008-http1-router-upstream/expectations.yaml` `expected_body` line drops `  content-length: 0\n` (~1 LoC YAML).
  - **Fixture 0008 `response_body: byte_exact` differential equivalence holds** — substantively closes one of phase-04.3 REVIEW C-1's three latent regressions (the other two being the IPv4/IPv6 selection issue addressed by ADR-0024 and the listener-filter issue addressed by ADR-0026).
  - envoy-rust now matches Envoy v1.33's request-side header emission set on empty-body requests.
  - Future requests with non-empty body continue to receive synthetic Content-Length — no regression.
- Provenance: This ADR was conditionally projected as ADR-0024 or ADR-0025 in 05.1 STATE.md (the C-1 follow-up's projected ADRs start at ADR-0024 onward). ADR-0024 is taken by the DnsLookupFamily schema (lands at 05.4 Task 1); this ADR lands at 05.4 Task 5 alongside the envoy-http1 client behaviour change. ADR-0026 (listener_filters parse-and-ignore) lands at 05.4 Task 3 numerically before this one but the landing-time order in DECISIONS.md is by task-execution order: ADR-0024 (Task 1) → ADR-0026 (Task 3) → ADR-0025 (Task 5). The ledger remains append-only with no renumbering.

---
```

Verify:

```bash
grep -c '^## ADR-' docs/envoy-rust/DECISIONS.md
grep -n '^## ADR-' docs/envoy-rust/DECISIONS.md | tail -4
```

Expected: ADR count `26`. The tail shows `ADR-0023`, `ADR-0024`, `ADR-0026`, `ADR-0025` in landing-time order.

- [ ] **Step 2: Flip the assertion in the existing unit test (failing test).**

In `crates/envoy-http1/src/client.rs::tests::send_request_writes_serialized_request_bytes` at lines 460-463:

```rust
// before:
        assert!(
            s.contains("content-length: 0\r\n"),
            "missing content-length: {s:?}"
        );

// after:
        // 05.4 NEW per ADR-0025: empty-body GET requests do NOT carry
        // a synthetic content-length: 0 header (RFC 7230 §3.3.2 + Envoy
        // v1.33 parity). The previous assertion expected the spurious
        // header; the new assertion confirms it is suppressed.
        assert!(
            !s.contains("content-length: 0\r\n"),
            "spurious content-length: 0 must NOT be emitted on empty-body GET: {s:?}"
        );
```

- [ ] **Step 3: Run the unit test to verify it fails.**

```bash
cargo test -p envoy-http1 send_request_writes_serialized_request_bytes 2>&1 | tail -15
```

Expected: `test result: FAILED. 1 failed`. The current code emits `content-length: 0` on the empty-body request, so the new `!s.contains(...)` assertion fails. Failure message includes the wire dump showing the spurious `content-length: 0\r\n`.

- [ ] **Step 4: Apply the `body_is_nonempty` guard in `send_request`.**

In `crates/envoy-http1/src/client.rs:94-103`, modify the CL-emission block:

```rust
// before (lines 94-103):
        // CL header — only emit if the request doesn't already carry one.
        let request_has_cl = request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH));
        if !request_has_cl {
            wire.extend_from_slice(b"content-length: ");
            wire.extend_from_slice(request.body_len_string().as_bytes());
            wire.extend_from_slice(b"\r\n");
        }

// after:
        // CL header — emit synthetic content-length only when the request
        // does not carry an explicit Content-Length AND the body is
        // non-empty. RFC 7230 §3.3.2 + Envoy v1.33 parity per ADR-0025
        // ("a user agent SHOULD NOT send a Content-Length header field
        // when the request message does not contain a payload body").
        let request_has_cl = request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(hdr::CONTENT_LENGTH));
        let body_is_nonempty = request.body_bytes().is_some_and(|b| !b.is_empty());
        if !request_has_cl && body_is_nonempty {
            wire.extend_from_slice(b"content-length: ");
            wire.extend_from_slice(request.body_len_string().as_bytes());
            wire.extend_from_slice(b"\r\n");
        }
```

- [ ] **Step 5: Run the unit test to verify it passes.**

```bash
cargo test -p envoy-http1 send_request_writes_serialized_request_bytes 2>&1 | tail -10
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 6: Run the full envoy-http1 test suite to verify no other tests regressed.**

```bash
cargo test -p envoy-http1 2>&1 | tail -15
```

Expected: `test result: ok` overall. Any test that previously asserted on `content-length:` for an empty-body request would also need a flip; verify no other test names contain `content_length` or `content-length` and assert on the empty-body path. If any other test fails, record in PROGRESS.md and fix accordingly.

- [ ] **Step 7: Update fixture 0008's `expectations.yaml`.**

In `tests/fixtures/0008-http1-router-upstream/expectations.yaml`, modify line 9 (the `body:` field of `expected_body`):

```yaml
# before:
    body: "method: GET\npath: /\nheaders:\n  content-length: 0\n  host: envoy-rust.test\nbody: \n"

# after:
    body: "method: GET\npath: /\nheaders:\n  host: envoy-rust.test\nbody: \n"
```

(The `  content-length: 0\n` substring is removed; the rest of the line is unchanged.)

Verify:

```bash
grep 'body:' tests/fixtures/0008-http1-router-upstream/expectations.yaml
```

Expected: the `body:` line no longer contains `content-length: 0`.

- [ ] **Step 8: Run clippy + fmt.**

```bash
cargo clippy -p envoy-http1 --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 9: Append the Task 5 section to PROGRESS.md.**

```markdown
## Task 5 — `envoy-http1::Client` content-length: 0 suppression on empty-body + ADR-0025 + fixture 0008 expectations update

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D5.
- **ADR landed:** ADR-0025 (suppress synthetic `content-length: 0` on empty-body GET; RFC 7230 §3.3.2 + Envoy v1.33 parity).
- **Files modified:**
  - `docs/envoy-rust/DECISIONS.md` (ADR-0025 appended after ADR-0026).
  - `crates/envoy-http1/src/client.rs` (`body_is_nonempty` guard added; `send_request_writes_serialized_request_bytes` assertion flipped).
  - `tests/fixtures/0008-http1-router-upstream/expectations.yaml` (`expected_body` `body:` line drops `  content-length: 0\n`).
- **LoC:** ~47 (5 client predicate + 5 unit test flip + 1 fixture YAML + 13 ADR + 25 PROGRESS narrative).
- **Coupling per SPEC §6 signpost 11:** client behavior change + expectations update in same commit (splitting would red the unit test or red fixture 0008 byte-equal echo body).
- **Verification:**
  - `cargo test -p envoy-http1 send_request_writes_serialized_request_bytes` — `test result: ok. 1 passed` (new assertion passes after the predicate change).
  - `cargo test -p envoy-http1` — `test result: ok` (no other tests regressed).
  - `cargo clippy -p envoy-http1 --all-targets -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean.
  - Fixture 0008 differential green re-baseline materializes at Task 7.
- **Deviations from PLAN:** _(record any here.)_
```

- [ ] **Step 10: Commit.**

```bash
git add docs/envoy-rust/DECISIONS.md \
        crates/envoy-http1/src/client.rs \
        tests/fixtures/0008-http1-router-upstream/expectations.yaml \
        docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md
git commit -m "phase 05.4: envoy-http1 suppress synthetic content-length: 0 on empty-body + ADR-0025 (task 5)"
```

Expected: clean commit; only the listed files modified.

---

### Task 6: `tests/differential/src/upstream.rs` — STRICT_DNS settle time 500ms → 2000ms for `host_gateway = true` fixtures

**Files:**
- Modify: `tests/differential/src/upstream.rs:88` (replace flat `Duration::from_millis(500)` with conditional bump).

**Why sixth:** independent of Tasks 1-5. Mechanically minimal (2 lines instead of 1). The 3 unaffected fixtures (0001/0002/0007) do not pass `host_gateway = true` (verified by PLAN signpost E + the call site at `tests/differential/src/lib.rs:989` deriving the flag from `upstream_yaml.contains("host.docker.internal")`); they continue at the existing 500ms settle.

**Scope.** ~5 LoC. No ADR (test-harness timing constant; PLAN signpost L).

- [ ] **Step 1: Verify the settle line + the host_gateway parameter binding.**

```bash
sed -n '46,90p' tests/differential/src/upstream.rs
grep -n 'host_uses_host_gateway\|upstream::start' tests/differential/src/lib.rs | head
```

Expected:
- `tests/differential/src/upstream.rs:46-50` declares `pub async fn start(envoy_yaml_path: &Path, host_gateway: bool, tls_pki: Option<&crate::tls::TlsTestPki>) -> Result<UpstreamProxy>` — the `host_gateway: bool` parameter is in scope at line 88.
- `tests/differential/src/upstream.rs:88` is `tokio::time::sleep(Duration::from_millis(500)).await;`.
- `tests/differential/src/lib.rs:989` derives `host_uses_host_gateway` from `upstream_yaml.contains("host.docker.internal")` and passes it through at line 992.

- [ ] **Step 2: Apply the conditional bump.**

In `tests/differential/src/upstream.rs`, replace line 88 with:

```rust
        // 05.4 NEW per SPEC §3 D6: STRICT_DNS DNS resolution may not have
        // completed by the 500ms mark on host-gateway fixtures (DNS via
        // Docker's host-gateway races the first test probe); bump to 2000ms
        // for those. The 3 unaffected fixtures (0001/0002/0007) do NOT set
        // host_gateway = true and continue at 500ms.
        let settle_ms = if host_gateway { 2000 } else { 500 };
        tokio::time::sleep(Duration::from_millis(settle_ms)).await;
```

(2 effective lines + 5 lines of doc comment.)

- [ ] **Step 3: Run the differential crate's unit tests (NOT the Docker-gated ones, which need Docker).**

```bash
cargo test -p differential --lib 2>&1 | tail -10
```

Expected: `test result: ok` for the lib-only tests (no Docker required). The Docker-gated `#[ignore]`-style or fixture-binary tests are deferred to Task 7's CI run (per PLAN signpost K).

- [ ] **Step 4: Run clippy + fmt.**

```bash
cargo clippy -p differential --all-targets -- -D warnings 2>&1 | tail -10
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 5: Append the Task 6 section to PROGRESS.md.**

```markdown
## Task 6 — Harness STRICT_DNS settle time 500ms → 2000ms for host_gateway fixtures

- **Commit:** _(pending)_
- **Deliverables:** SPEC §3 D6.
- **ADR landed:** None (D6 has no ADR; test-harness timing constant per PLAN signpost L).
- **Files modified:**
  - `tests/differential/src/upstream.rs` (line 88 replaced with conditional bump).
- **LoC:** ~5.
- **Verification:**
  - `cargo test -p differential --lib` — `test result: ok`.
  - `cargo clippy ... -- -D warnings` — clean.
  - `cargo fmt --all -- --check` — clean.
  - Behavioral verification of the bump deferred to Task 7's CI run.
- **Deviations from PLAN:** _(none expected; planner does NOT tighten 2000ms in 05.4 per SPEC §6 signpost 16.)_
```

- [ ] **Step 6: Commit.**

```bash
git add tests/differential/src/upstream.rs \
        docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md
git commit -m "phase 05.4: STRICT_DNS settle time 500ms→2000ms for host_gateway fixtures (task 6)"
```

Expected: clean commit; only the listed files modified.

---

### Task 7: State-4 phase-done gate verification — Docker-gated CI re-push; substantively closes phase-04.3 REVIEW C-1

**Files:**
- Modify: `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md` (Task 7 section with verification narrative + per-fixture CI matrix).
- Optionally Modify: `Cargo.lock` (no-op expected; commit a sync if anything diffs).

**Why last:** the state-4 phase-done gate per `BOOTSTRAP_PROMPT.md` §7.5. Aggregates evidence that all 6 root-cause fixes (Tasks 1-6) substantively closed phase-04.3 REVIEW C-1: the 5 affected Docker-gated fixtures + 3 unaffected fixtures all GREEN simultaneously.

**Scope.** ~0 LoC code change. PROGRESS.md narrative + CI run URL + per-fixture matrix. Per SPEC §3 D7.

- [ ] **Step 1: Run the 5 stable-toolchain commands.**

```bash
cargo build --workspace --all-targets 2>&1 | tail -10
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tail -15
cargo fmt --all -- --check 2>&1 | tail -5
cargo test --workspace 2>&1 | tail -25
cargo deny check 2>&1 | tail -15
```

Expected:
- `cargo build`: `Finished` (no errors, no warnings beyond pre-existing baseline).
- `cargo clippy`: clean (`-D warnings` enforced; if any new warning, fix).
- `cargo fmt --check`: clean (no diff).
- `cargo test --workspace`: `test result: ok` across all crates. Note: if local Docker is unavailable, the differential suite's Docker-gated fixtures may show `0 passed; X ignored` for the Docker-gated binaries — that's acceptable per PLAN signpost K; the CI run is the gate.
- `cargo deny check`: `0 errors`. Per PLAN signpost I, this is expected to be a no-op.

Quote each command's tail output (last 10-15 lines) into PROGRESS.md Task 7 verbatim.

- [ ] **Step 2: Run the fuzz short-budget target.**

```bash
cd crates/envoy-config/fuzz
cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30 2>&1 | tail -20
cd ../../..
```

Expected: short-budget fuzz run completes in ~30 seconds with no crash and 12 corpus-walk seeds parsed cleanly (the existing 12 seeds — including 05.1's `strict_dns_cluster.yaml` — continue to parse through the schema additions because the new fields are `Option`/`Vec` with `#[serde(default)]`).

Quote the tail into PROGRESS.md.

- [ ] **Step 3: Sync `Cargo.lock` if any diff appears (expected: no-op).**

```bash
cargo build --workspace 2>&1 | tail -3
git status Cargo.lock
git diff Cargo.lock
```

Expected: `git status Cargo.lock` returns clean (no modification). Per PLAN signpost I + SPEC §6 signpost 2: 05.4 introduces no new top-level deps, so Cargo.lock should not change. If anything diffs, record in PROGRESS.md and stage the diff for the Task 7 commit.

- [ ] **Step 4: Push to remote and trigger the Docker-gated CI run.**

```bash
git status
git log --oneline origin/main..HEAD
git push origin HEAD
```

Wait for CI to complete. The Docker-gated job is the gate per SPEC §1 acceptance signal (a) + (b).

Pull the CI run URL via:

```bash
gh run list --branch $(git branch --show-current) --limit 5
gh run view <RUN_ID> --log-failed | tail -40    # if any failures
```

Expected: 1 successful CI run (no failures). The run ID + URL are quoted into PROGRESS.md Task 7.

- [ ] **Step 5: Quote the per-fixture matrix into PROGRESS.md Task 7.**

The acceptance signal is **all 5 affected fixtures GREEN + all 3 unaffected fixtures GREEN simultaneously** (per SPEC §1 (a) + (b)):

```
fixture 0001-tcp-echo                  : GREEN (unchanged from 05.1)
fixture 0002-static-admin-ready        : GREEN (unchanged)
fixture 0003-tcp-proxy                 : GREEN (RESTORED — V4_ONLY + 0.0.0.0 bind + settle 2000ms)
fixture 0004-tls-downstream            : GREEN (RESTORED — same)
fixture 0005-tls-upstream               : GREEN (RESTORED — same)
fixture 0006-tls-sni                   : GREEN (RESTORED — same + tls_inspector listener filter)
fixture 0007-http1-direct-response     : GREEN (unchanged; no host_gateway, settle 500ms)
fixture 0008-http1-router-upstream     : GREEN (RESTORED — same + content-length: 0 suppression)
```

If any fixture is RED, **re-enter state 3** (NOT state 4) per SPEC §1 + per `BOOTSTRAP_PROMPT.md` §5.2 review-feedback re-entry semantics. Diagnose via `gh run view <RUN_ID> --log` (or `--log-failed`); the most likely failure modes are (a) settle time still too short → bump to 3000ms in a follow-up task; (b) some fixture surfaced an unanticipated additional regression → add an 8th task and follow the same TDD discipline; (c) one of Tasks 1-6 has a subtle bug → fix and retry. PROGRESS.md records the diagnosis and the remedial task before re-running Task 7.

- [ ] **Step 6: Append the Task 7 section to PROGRESS.md with full verification evidence.**

```markdown
## Task 7 — State-4 phase-done gate verification — substantively closes phase-04.3 REVIEW C-1

- **Commit:** _(pending — this is the verification commit; see commit message below)_
- **Deliverables:** SPEC §3 D7.
- **ADR landed:** None (D7 is verification only).
- **Files modified:**
  - `docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md` (this section).
  - _(possibly: `Cargo.lock` — no-op expected)_

### Local stable-toolchain command outputs (tail-quoted)

```
$ cargo build --workspace --all-targets
<paste tail of output here>

$ cargo clippy --workspace --all-targets --all-features -- -D warnings
<paste tail of output here>

$ cargo fmt --all -- --check
<paste tail of output here; expect empty>

$ cargo test --workspace
<paste tail of output here>

$ cargo deny check
<paste tail of output here>
```

### Fuzz short-budget output (tail-quoted)

```
$ cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30
<paste tail of output here>
```

### Cargo.lock sync

`git status Cargo.lock` returned clean / `git diff Cargo.lock` showed _(diff or empty)_. Per PLAN signpost I + SPEC §6 signpost 2: 05.4 introduces no new top-level deps; Cargo.lock no-op as expected.

### Docker-gated CI run

- **Run URL:** _(paste full URL, e.g. https://github.com/<org>/envoy-rust/actions/runs/NNNN)_
- **Run ID:** _(paste)_
- **Result:** SUCCESS

### Per-fixture matrix

| Fixture | Status | Note |
|---|---|---|
| `tests/fixtures/0001-tcp-echo` | GREEN | unchanged from 05.1 |
| `tests/fixtures/0002-static-admin-ready` | GREEN | unchanged |
| `tests/fixtures/0003-tcp-proxy` | GREEN | RESTORED (V4_ONLY + 0.0.0.0 bind + settle 2000ms) |
| `tests/fixtures/0004-tls-downstream` | GREEN | RESTORED (same) |
| `tests/fixtures/0005-tls-upstream` | GREEN | RESTORED (same) |
| `tests/fixtures/0006-tls-sni` | GREEN | RESTORED (same + tls_inspector listener filter) |
| `tests/fixtures/0007-http1-direct-response` | GREEN | unchanged; no host_gateway, settle 500ms |
| `tests/fixtures/0008-http1-router-upstream` | GREEN | RESTORED (same + content-length: 0 suppression) |

**Substantively closes phase-04.3 REVIEW C-1.** The C-1 carryforward chain (originating at phase-02.2's ADR-0015 landing `435c6fa`, latent across phases 02.2 → 03.1 → 03.2 → 04.1 → 04.2 → 04.3, partially closed at 05.1 state-6 commit `1d05cd0`) ends here. **Phase-04.1 REVIEW M-claim** (drive_http1 per-function unit test) is unblocked by the fixture-mask removal but stays deferred per the 04.3 disposition. No new I3-style or A-style closures expected at 05.4.

### Deviations from PLAN

_(record any here)_
```

- [ ] **Step 7: Commit.**

```bash
git add docs/envoy-rust/phases/05.4-fixture-hardening-followup/PROGRESS.md
# include Cargo.lock only if it diffed (unlikely):
# git add Cargo.lock
git commit -m "phase 05.4: state-4 phase-done gate verification (task 7)"
```

Expected: a single verification-only commit. The phase-done close-out (state-5 review + state-6 commit) is a separate session per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session").

---

## Self-review

After landing all 7 tasks, the next session enters state 5 (`superpowers:requesting-code-review`) and produces `REVIEW.md`. The reviewer will check this PLAN against the SPEC and against the actual landed surface. Likely focal points:

- **ADR landing-time order vs. numeric order** (per SPEC §6 signpost 9 + ADR-0025/0026 provenance): ADR-0023 → ADR-0024 → ADR-0026 → ADR-0025. PLAN follows this; reviewer cross-checks DECISIONS.md.
- **No `BEHAVIOR_CONTRACT.md` edits** (per SPEC §2): PLAN does not touch the file; reviewer confirms.
- **No new top-level Cargo deps + `cargo deny` no-op** (per SPEC + PLAN signpost I): reviewer confirms `git diff` against `06b46a9` for `Cargo.toml` files.
- **Fixture 0008 expectations + envoy-http1 client coupling** (per SPEC §6 signpost 11): both must land in Task 5; reviewer confirms via `git log --oneline | grep 'task 5'`.
- **Fixture 0006 envoy.yaml + Listener.listener_filters coupling** (per SPEC §6 signpost 12): both must land in Task 3; reviewer confirms via `git log --oneline | grep 'task 3'`.
- **`envoy-rust.yaml` sibling files NOT modified** (per SPEC §3 D2 + ADR-0024 / ADR-0026 — only the upstream Envoy side observes the new knobs/blocks): reviewer confirms via `git diff <05.1 head>..<05.4 head> -- 'tests/fixtures/*/envoy-rust.yaml'` returns empty.
- **5 affected fixtures + 3 unaffected fixtures green simultaneously** (per SPEC §1 acceptance + Task 7 matrix): reviewer cross-checks the CI run URL.

If any of the above red-flags surface, the reviewer issues findings; per `BOOTSTRAP_PROMPT.md` §5.2 review re-entry, the executor re-enters at state 3, lands the fix as additional tasks 8+, then re-runs state-4 verification.

---

## Acceptance signal recap

The 05.4 phase-done commit (state-6, separate session per §5.1) flips ROADMAP row `05.4` `in-progress` → `done`. Parent row `05` stays `in-progress` (flips at sub-phase 05.3's state-6 commit). STATE.md advances active phase to `05.2-http2-downstream` lifecycle state 2 (PLAN.md does not exist yet for 05.2; SPEC was landed at parent-05 state-2 commit `f1804a7`). Next-skill `superpowers:writing-plans` scoped to sub-phase 05.2.

The state-6 commit message format (per SPEC §9):

```
phase 05.4: 6 root-cause fixes + Docker-gated 5-fixture green re-baseline [ADR-0024, ADR-0025, ADR-0026]

<summary per SPEC §9 — already drafted in SPEC>

Differential surface: tests/fixtures/0001-tcp-echo green (unchanged);
  tests/fixtures/0002-static-admin-ready green (unchanged);
  tests/fixtures/0003-tcp-proxy green (RESTORED);
  tests/fixtures/0004-tls-downstream green (RESTORED);
  tests/fixtures/0005-tls-upstream green (RESTORED);
  tests/fixtures/0006-tls-sni green (RESTORED);
  tests/fixtures/0007-http1-direct-response green (unchanged);
  tests/fixtures/0008-http1-router-upstream green (RESTORED).
Conformance: none (h2spec attaches in 05.2).
```

Substantively closes phase-04.3 REVIEW C-1. Phase-04.1 REVIEW M-claim unblocked; stays deferred per the 04.3 disposition.
