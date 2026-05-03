# Phase 05.2 — Downstream HTTP/2 cleartext (H2C prior-knowledge): `envoy-http2` foundation + HCM-on-H2 dispatch + fixture 0009 + `h2spec` ≥95% gate — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task (fresh subagent per task + two-stage review). Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **For every code-writing task:** REQUIRED SUB-SKILL: `superpowers:test-driven-development` — failing test first, verify fails, minimal implementation, verify passes, commit. No exceptions (doctrine D-3.1).
>
> **Source of truth:** `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` (committed at parent-05 state-2 SHA `f1804a7`). This plan operationalizes SPEC §§D1–D7. Where this plan and the SPEC disagree, the SPEC wins — flag the drift, land an ADR per D-3.5, and continue. The parent phase-05 SPEC at `docs/envoy-rust/phases/05-http2/SPEC.md` (committed at parent-05 state-1 SHA `cd1a70e`) and the predecessor 05.1 + 05.4 SPECs are preserved unedited as historical artifacts; for execution they are superseded by this 05.2 SPEC.

**Goal.** Land downstream HTTP/2 cleartext (H2C prior-knowledge) on the data plane in five coordinated layers shipping in this single sub-phase: (1) new workspace member `crates/envoy-http2/` (sole-dep-owner of `h2 = "0.4"`, mirroring `envoy-http1`'s sole-owner-of-`httparse` posture from 04.1 + `envoy-tls`'s sole-owner-of-`rustls` posture from 03.1, per parent-05 SPEC §3 cross-sub-phase architectural rule 1); (2) `envoy-config` schema additions — `CodecType::HTTP2` flips from reject to accept; new listener-side `Http2ProtocolOptions` struct (4 optional `u32` fields per parent §6 signpost 2); 2 new `ConfigError` variants (`Http2OverTlsNotSupported`, `Http2ProtocolOptionsOutOfRange`); HTTP3 continues to reject; (3) HCM-on-H2 dispatch in `crates/envoy-http2/src/hcm.rs` — implements `envoy_listener::ConnectionHandler` (sibling of `envoy_http1::HCM`); per-connection `h2::server::handshake` + per-stream `tokio::spawn` task; reuses `envoy_http1::HCMConfig` + `envoy_http1::hcm::build_response` + the route-walk + `BuildOutcome` enum end-to-end (only the codec layer at the connection edge changes, per cross-sub-phase rule 2); `:authority` → `Host:` synthesis at the request-translation boundary (per cross-sub-phase rule 3); H2-forbidden hop-by-hop headers stripped defensively at the response-translation boundary (per cross-sub-phase rule 4); `BuildOutcome::Proxy` arm structurally exercised but stubbed with a 502 (real upstream H2 dispatch lands in 05.3); (4) `envoy-bin` HCM-on-H2 wiring — the existing `HCM_FILTER` arm at `crates/envoy-bin/src/main.rs:207` gains a second branch selecting between `envoy_http1::HCM` and `envoy_http2::HCM` based on `hcm_cfg.codec_type`; new in-process integration test at `crates/envoy-bin/tests/http2_direct_response.rs`; (5) differential harness extensions + fixture `0009-http2-direct-response` + Docker-gated test wrapper + first conformance suite `tests/conformance/h2spec/` at the **≥95% pass** gate with catalogued failures in `known-failures.txt`.

**Architecture.** The codec scaffold mirrors `envoy-http1`'s shape exactly: a thin codec-edge wrapper around the foundation crate (`h2` here, `httparse` in 04.1) that translates protocol-specific framing into the project's protocol-agnostic `Request`/`Response` value types. The HCM logic — route walking, header matching, action dispatch, `BuildOutcome` → wire emission — is **not duplicated** in `envoy-http2`; the H2 HCM consumes `envoy_http1::HCMConfig` + `envoy_http1::hcm::build_response` directly (re-exported as `envoy_http2::HCMConfig` for ergonomic naming). The dispatch-by-codec lives at the listener-walk site in `envoy-bin/src/main.rs` (per parent §6 signpost 22), not at the HCMConfig level. The `ConnectionHandler` trait at `crates/envoy-listener/src/lib.rs:29-34` returns a hand-boxed `BoxFuture` (NOT `async_trait`-shaped per phase 02.2 SPEC §6 signposts 2-3 deliberately avoiding the `async-trait` dep) — the SPEC's example code at `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md:277-285` shows `#[async_trait::async_trait]` annotation, but SPEC §6 local signpost 19 explicitly defers to in-tree posture; **this PLAN uses the BoxFuture posture verbatim** (matching `envoy_http1::HCM`'s impl at `crates/envoy-http1/src/hcm.rs:98-110`). Per-stream concurrency uses direct `tokio::spawn` (fire-and-forget; per-stream errors are logged via `tracing::error!` and do not propagate to the connection driver). H2-forbidden hop-by-hop headers (`connection`, `transfer-encoding`, `upgrade`, `keep-alive`, `proxy-connection` per RFC 7540 §8.1.2.2) are stripped at the codec edge in `response.rs::envoy_response_to_http2`. The `:authority` header is synthesized as a `Host:` row at the bottom of `Request.headers` so the existing `envoy_http1::hcm::build_response`'s Host-driven route-walk works without modification. The `Http2ProtocolOptions` struct is configured optionally on `HttpConnectionManagerConfig` and consumed at HCM construction time to configure `h2::server::Builder` (defaults sourced from the `h2` crate when fields are absent). Fixture 0009 mirrors fixture 0007's shape exactly (per parent §6 signpost 20), changing only `codec_type: HTTP1` → `codec_type: HTTP2`. The `h2spec` runner spawns envoy-bin against a minimal HCM `codec_type: HTTP2` config, runs `h2spec` as a subprocess, parses its output, and asserts ≥95% pass rate AND every failing test is enumerated in `known-failures.txt` (the gate also fails if a previously-listed failing test starts passing without the file being trimmed — this catches stale known-failures and forces lockstep maintenance).

**Tech stack.** Rust edition 2024 on pinned stable (`rust-toolchain.toml` D-3.9). **One new top-level Cargo dep on `crates/envoy-http2/Cargo.toml`:** `h2 = "0.4"` (D-3.2 permitted-foundation; sole runtime owner per cross-sub-phase architectural rule 1) plus `http = "1"` direct-dep grant via **ADR-0027** (the `http` crate's typed surfaces — `http::Request`, `http::Response`, `http::HeaderMap` — are required at the codec-edge translation boundary because `h2::server::Connection::accept` returns `http::Request<h2::RecvStream>`; planner-recommended posture per parent §6 signpost 21 is to land the ADR as a narrow grant). **One direct-dep carve-out on `tests/differential/Cargo.toml`:** `h2 = "0.4"` (per parent §6 signpost 8 — the `drive_http2` helper consumes `h2::client` directly, parallel to phase 04.1 REVIEW M-architectural-claim's `httparse` carve-out for `drive_http1`). `h2spec` is provisioned by CI (curl-tar fallback per parent §6 signpost 3 recommendation) and locally `eprintln!`-skipped if `which h2spec` fails. New typed surfaces: `envoy_http2::HCM` + `envoy_http2::HCMConfig` (re-exported alias for `envoy_http1::HCMConfig`) + `envoy_http2::Http2Error`; `envoy_config::Http2ProtocolOptions` (4 `Option<u32>` fields); `envoy_config::HttpConnectionManagerConfig.http2_protocol_options: Option<Http2ProtocolOptions>`; `envoy_config::ConfigError::Http2OverTlsNotSupported` + `Http2ProtocolOptionsOutOfRange { field, value, range }`. New harness surfaces: `differential::Driver::Http2 { method, path, host, expected_status, expected_body, expected_headers }`; `differential::drive_http2(addr, method, path, host, extra_headers) -> DriveHttp1Result` (reuses `DriveHttp1Result`'s shape for `assert_equivalence`'s `diff_headers` interop). New behavioral surface: HCM listeners with `codec_type: HTTP2` accept H2C prior-knowledge connections and route them through the existing 04.x HCM core. **No edits to:** `BEHAVIOR_CONTRACT.md` (per SPEC §2 — Row 4 of the equivalence matrix engages for the first time but is satisfied implicitly by `h2`-codec validation; the existing 3-row HEADER_ALLOW_LIST suffices), `rust-toolchain.toml`, `ENVOY_TARGET.md`, `crates/envoy-{cluster,tls,tcp,listener,http1}/` (consumed via existing public APIs unchanged), `tests/fixtures/{0001..0008}/` (must remain green at the 05.2 state-4 gate), `crates/envoy-config/fuzz/{Cargo.toml,fuzz_targets/}` (only the corpus directory grows by 1 seed). `deny.toml` likely no-op (`h2`, `http`, and their transitive surfaces — `slab`, `fnv`, `tokio-util` — are dual-licensed MIT/Apache-2.0 already on the allow-list); cross-checked at Task 14. `Cargo.lock` lands a non-trivial diff at Task 1 (h2 + http + their transitive surfaces formalize as direct deps).

**~14 tasks, ~2055 LoC total (per SPEC §3 deliverable estimates).** D1 ~50 + D2 ~380 + D3 ~790 + D4 ~160 + D5 ~170 + D6 ~130 + D7 ~375 = ~2055 LoC. The `BOOTSTRAP_PROMPT.md` §6.1 task-count gate (~25) holds with significant headroom (14 ≪ 25). The §6.1 LoC gate (~1500) is **crossed** at the SPEC-write-time estimate (~2055 ≈ 137% of guardrail). **Disposition: do not split** — per SPEC §6 signpost 28 (LoC-budget reality check) + parent-05 SPEC §5's "no nest-split" rule (05.2 is already a sub-phase produced by parent-05's split per ADR-0022). The drift (~58% over parent-05 brainstorm's projection of ~1300 LoC) is concentrated in (a) D3's multi-module decomposition (~790 LoC across 5 files: `hcm.rs` + `request.rs` + `response.rs` + `error.rs` + `codec.rs` + 12 unit tests) — this is the H2 codec test surface's first appearance in the project; and (b) D7's first-conformance-suite scaffolding (~375 LoC) — the runner + h2spec.yaml + known-failures.txt parser + gate-mechanics primitives — which is foundational for future conformance attaches (`h3spec` in QUIC family, `grpc-conformance` in gRPC family) and not amenable to trimming. Both are doctrine-mandated test surfaces, not creep. **The systematic-debugging confirmation is recorded inline here** (per SPEC signpost 28's invocation requirement) and does NOT require a separate session: the LoC drift is genuine scope (multi-module H2 codec + first conformance suite), not feature creep, and the test surface is non-negotiable per D-3.6 ("every phase is a green build"). Recorded in PROGRESS Task 1 narrative.

**Lands up to 1 ADR** (per SPEC §7 + the post-05.4 ADR-renumbering disposition — see "ADR renumbering" below). **ADR-0027 (recommended landing) at Task 1**: `http` crate typed-surface scoping — the `http = "1"` direct-dep on `crates/envoy-http2/Cargo.toml` is the codec-edge translation requirement (parallel to ADR-0021's narrow scoping for `regex` on `crates/envoy-config/`); the recommendation per parent §6 signpost 21 is to land the ADR. **ADR-0028 (recommended NOT to land) at Task 13**: `h2spec` integration posture — the gate-mechanics (binary provisioning via curl-tar; output parsing via line-grep over h2spec stdout; known-failures.txt one-line-per-test-id format) are mechanically deterministic per parent §6 signposts 3-4; recording inline in PROGRESS Task 13 suffices. The DECISIONS.md ledger after 05.2 reads `... ADR-0026 (05.4 Task 3) | ADR-0025 (05.4 Task 5) | ADR-0027 (05.2 Task 1) | ...` — landing-time order, not numeric order, per the append-only ledger discipline.

**ADR renumbering.** The 05.2 SPEC at `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` was written at parent-05 state-2 (commit `f1804a7`) when the DECISIONS.md ledger head was ADR-0023. SPEC §7 projects ADR-0024 (http typed-surface) and ADR-0025 (h2spec posture). Phase 05.4 has since landed ADR-0024 / ADR-0025 / ADR-0026 (per `STATE.md` "Phase-05.4 rollovers"; landing-time order ADR-0023 → 0024 → 0026 → 0025); the lexically-max ADR number is now ADR-0026, so the next-sequential available number is **ADR-0027**. **The 05.2-projected ADR-0024 → actual ADR-0027** (if landed); **the 05.2-projected ADR-0025 → actual ADR-0028** (if landed). This PLAN refers to ADR-0027 / ADR-0028 throughout; the SPEC's pre-renumbering text is the historical artifact (preserved unedited per D-3.5). Mirrors the phase-03 ADR renumbering precedent documented in `STATE.md` Notes section.

**No HTTP/2 upstream work in 05.2.** The `Client` + `ClientStream` types in `crates/envoy-http2/src/client.rs`, the router H2-arm dispatch, `tests/helpers/http2-echo-server/`, fixture `0010-http2-router-upstream`, the cluster-side `Http2ProtocolOptions` (via `typed_extension_protocol_options`), and the `Cluster.upstream_protocol` field all defer to sub-phase 05.3 per parent-05 SPEC §3 D11.3–D15.3 + SPEC §4. The `BuildOutcome::Proxy` arm in 05.2's H2 HCM is structurally exercised (must compile) but stubbed with a 502 Bad Gateway response — fixture 0009 is direct_response only, so this stub is unreachable from any 05.2 fixture; the stub flips to the real upstream H2 dispatch at 05.3 D13.3.

---

## File structure (created / modified / not touched)

**Created:**

- `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` (appended once per task during execution; created by Task 1 alongside the ADR-0027 landing).
- `crates/envoy-http2/Cargo.toml` (Task 1).
- `crates/envoy-http2/src/lib.rs` (with `#![forbid(unsafe_code)]` per D-3.8) (Task 1).
- `crates/envoy-http2/src/error.rs` (Task 5).
- `crates/envoy-http2/src/request.rs` (Task 6).
- `crates/envoy-http2/src/response.rs` (Task 7).
- `crates/envoy-http2/src/codec.rs` (Task 8).
- `crates/envoy-http2/src/hcm.rs` (Task 9).
- `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml` (Task 4).
- `crates/envoy-bin/tests/http2_direct_response.rs` (Task 10).
- `tests/conformance/h2spec/Cargo.toml` (Task 13).
- `tests/conformance/h2spec/src/lib.rs` (empty crate root with `#![forbid(unsafe_code)]`) (Task 13).
- `tests/conformance/h2spec/tests/h2spec_runner.rs` (Task 13).
- `tests/conformance/h2spec/h2spec.yaml` (Task 13).
- `tests/conformance/h2spec/known-failures.txt` (Task 13; populated at task time after h2spec is run end-to-end).
- `tests/fixtures/0009-http2-direct-response/envoy.yaml` (Task 12).
- `tests/fixtures/0009-http2-direct-response/envoy-rust.yaml` (Task 12).
- `tests/fixtures/0009-http2-direct-response/inputs/payload.bin` (Task 12; empty file, 0 bytes).
- `tests/fixtures/0009-http2-direct-response/expectations.yaml` (Task 12).
- `tests/fixtures/0009-http2-direct-response/README.md` (Task 12).
- `tests/differential/tests/http2_direct_response.rs` (Task 12; 7-line wrapper).

**Modified:**

- `Cargo.toml` (root) — `[workspace] members` gains `crates/envoy-http2` at Task 1 and `tests/conformance/h2spec` at Task 13.
- `Cargo.lock` — synced inline at Task 1 with the `h2` + `http` + transitive surfaces (`slab`, `fnv`, `tokio-util` formalize as direct via the workspace's resolved graph).
- `crates/envoy-config/src/bootstrap.rs` — at Task 2 (D2.a): narrow the `validate_hcm` rejection at lines 1111–1118 from `{HTTP2 | HTTP3}` to `{HTTP3 only}`, plus add a TLS-attached-listener detection arm that returns `Http2OverTlsNotSupported`. At Task 3 (D2.b): introduce `Http2ProtocolOptions` struct after `RouterConfig` at line 333 (before `RouteConfiguration` at line 335); add `http2_protocol_options: Option<Http2ProtocolOptions>` field on `HttpConnectionManagerConfig` at lines 296–301; extend `validate_hcm` with the RFC 7540 range checks. Update the existing `rejects_*_codec_type` test assertions at lines 3300/3335 to reflect the narrowed rejection. Append ~10 new validator unit tests + 1 corpus-walk acceptance test to the existing `mod tests` block (Tasks 2/3/4). Cross-reference: `crates/envoy-config/src/bootstrap.rs:308-313` is the existing `CodecType` enum (variants `AUTO | HTTP1 | HTTP2 | HTTP3`; PLAN preserves variant names verbatim).
- `crates/envoy-config/src/lib.rs` — at Task 2: append `ConfigError::Http2OverTlsNotSupported` variant after the existing `MixedTlsAndPlaintextFilterChainsOnListener` block ending at line 100 (between line 100 and the existing `UnsupportedCodecType` at line 102). At Task 3: append `ConfigError::Http2ProtocolOptionsOutOfRange { field, value, range }` after the `UnsupportedCodecType` block ending at line 102. Extend the `pub use bootstrap::{...}` re-export list at lines 10–19 to include `Http2ProtocolOptions` (alphabetic insertion between `HttpFilterTypedConfig` at line 14 and `Int64Range` at line 14).
- `crates/envoy-config/fuzz/.gitignore` — at Task 4: append `!corpus/parse_bootstrap/hcm_codec_http2.yaml` to the existing allow-list block.
- `crates/envoy-bin/src/main.rs` — at Task 10: extend the `HCM_FILTER` arm at line 207 with H1-vs-H2 dispatch on `hcm_cfg.codec_type`. The existing TLS-detect-and-bail at lines 235–241 stays unchanged on the H1/AUTO path; the H2 path skips TLS-detect (validator already rejected TLS+HTTP2 at parse time per Task 2's `Http2OverTlsNotSupported`).
- `crates/envoy-bin/Cargo.toml` — at Task 10: add `envoy-http2 = { path = "../envoy-http2" }` to `[dependencies]`. At Task 10: add `h2 = "0.4"` to `[dev-dependencies]` (the in-process integration test consumes `h2::client` directly per parent §6 signpost 18 — this is in the binary-crate's test surface, not its runtime dep set, so it's a dev-dep).
- `tests/differential/Cargo.toml` — at Task 11: add `h2 = "0.4"` to `[dependencies]` (the `drive_http2` helper consumes `h2::client` directly per parent §6 signpost 8 — the documented carve-out from cross-sub-phase architectural rule 1).
- `tests/differential/src/lib.rs` — at Task 11: add `Driver::Http2` variant to the existing `Driver` enum at lines 38–83; extend the `port_key` match in `run_fixture` at lines 836–842 with the new `Driver::Http2 { .. }` arm; add `pub async fn drive_http2` after `drive_http1` (line 779); extend the per-driver dispatch cascade in `run_fixture` (lines 1007+ — sibling of the `Driver::Http1` arm at line 1114) with the `Driver::Http2` arm. Append 1 new harness unit test (`drive_http2_round_trip_against_in_process_listener`).
- `docs/envoy-rust/DECISIONS.md` — at Task 1: append **ADR-0027** at the next-sequential position (immediately after the existing ADR-0025 block ending around line 493). The ADR-0027 block carries `Date / Status / Context / Options-considered / Decision / Rationale / Consequences / Provenance` per the established 24-block precedent. The Provenance footer names the renumbering chain (SPEC's projected ADR-0024 → actual ADR-0027) per the `STATE.md` Notes-section convention for ADR renumberings. **No** ADR-0028 lands per the SPEC §7 + parent §6 signpost 21 recommendation; the decision to NOT land is recorded inline in PROGRESS Task 13.
- `.github/workflows/ci.yml` — at Task 14: add an `h2spec` binary provisioning step before the existing `cargo test --workspace` step. The provisioning uses `curl -L https://github.com/summerwind/h2spec/releases/download/<version>/h2spec_linux_amd64.tar.gz | tar xz -C tools/` (cross-distro fallback per parent §6 signpost 3 recommendation; the planner picks the `<version>` at task time by checking the latest `summerwind/h2spec` GitHub release). Local `eprintln!`-skip if `which h2spec` fails (per the established Docker-binary-locator pattern from 02.2's `TcpProxyBackend`, 03.2's `tls-echo-server`, and 04.3's `http1-echo-server`).
- `docs/envoy-rust/ROADMAP.md` — at state 6 only (NOT a state-3 task), flip row `05.2` `status` `planned` → `done`. Parent row `05` stays `in-progress` (flips at sub-phase 05.3's state-6 commit per the ROADMAP-schema invariant). State-6 close-out is a separate session per `BOOTSTRAP_PROMPT.md` §5.1 ("one state per session") — not part of this PLAN's tasks.
- `docs/envoy-rust/STATE.md` — at state 6 only, advance active phase to sub-phase `05.3-http2-upstream` lifecycle state 2 (the 05.3 SPEC was landed at parent-05 state-2 commit `f1804a7` alongside this PLAN's SPEC; 05.3 PLAN.md does not exist yet). Next-skill `superpowers:writing-plans` scoped to sub-phase 05.3. Notes section gains the carryforward bookkeeping (the C-1 carryforward chain ended at 05.4; 05.2 introduces no new carryforward items per SPEC §1).

**Not touched in 05.2** (belong to other phases or are frozen):

- `docs/envoy-rust/phases/05-http2/SPEC.md` (parent) — unedited; remains the design artifact committed at SHA `cd1a70e`.
- `docs/envoy-rust/phases/{05.1-fixture-hardening,05.4-fixture-hardening-followup}/*` (closed) — unedited in 05.2.
- `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` (this sub-phase) — landed at parent-05 state-2 commit `f1804a7`; unedited in 05.2 execution per D-3.4.
- `docs/envoy-rust/phases/05.3-http2-upstream/SPEC.md` — landed at parent-05 state-2 alongside this SPEC; unedited in 05.2 (its PLAN/PROGRESS/REVIEW land in its own sub-phase execution window).
- `docs/envoy-rust/phases/{04*, 03*, 02*, 01, 00}/*` — closed in earlier phases.
- `docs/envoy-rust/BEHAVIOR_CONTRACT.md` — no edits in 05.2 (per SPEC §2 — Row 4 H2 framing equivalence is implicit; HEADER_ALLOW_LIST unchanged).
- `docs/envoy-rust/MISSION.md`, `docs/envoy-rust/SKILL_ROUTING.md` — frozen per their durability discipline.
- `crates/envoy-{tls,tcp,listener,cluster,http1}/` — consumed via existing public APIs without amendment. Notably: `envoy_http1::HCMConfig` + `envoy_http1::hcm::build_response` + the route-walk + `BuildOutcome` enum are all consumed unchanged from `envoy_http2::HCM`'s dispatch. The `BuildOutcome` enum is `pub(crate)` in `envoy-http1` today (`crates/envoy-http1/src/hcm.rs:311-314`) — `envoy_http2::HCM` consumes the public function `build_response` which returns `BuildOutcome`; verifiable at Task 9 time. **If `BuildOutcome` and/or `build_response` are not currently `pub` in `envoy_http1`'s public API, Task 9 lifts visibility from `pub(crate)` to `pub`** (this is in-scope for 05.2 per cross-sub-phase architectural rule 2: "HCM-on-H2 reuses 04.x's HCMConfig and route-walk wholesale; only the codec layer at the connection edge changes" — re-using the route-walk requires the public consumption surface).
- `crates/envoy-bin/src/{admin,argv,echo,tls_handler}.rs` — unchanged. The HCM dispatch lives in `main.rs` only.
- `tests/fixtures/0001-tcp-echo/`, `tests/fixtures/0002-static-admin-ready/`, `tests/fixtures/0003-tcp-proxy/`, `tests/fixtures/0004-tls-downstream/`, `tests/fixtures/0005-tls-upstream/`, `tests/fixtures/0006-tls-sni/`, `tests/fixtures/0007-http1-direct-response/`, `tests/fixtures/0008-http1-router-upstream/` — unedited; their fixtures must remain green at the 05.2 state-4 phase-done gate. Verified at 05.4 state-4 (CI run `25276504502`, all 8 green simultaneously).
- `tests/fixtures/0010-http2-router-upstream/` — does not exist at 05.2 close (lands in 05.3).
- `tests/helpers/{tcp,tls,http1}-echo-server/` — finalized in earlier phases. No `http2-echo-server` lands in 05.2 (lands in 05.3 alongside fixture 0010).
- `tests/differential/src/{backend,subject,tls,upstream}.rs` — unchanged. Only `lib.rs` is edited (Task 11 D5).
- `crates/envoy-config/fuzz/Cargo.toml`, `crates/envoy-config/fuzz/fuzz_targets/` — unchanged. Only the corpus directory grows (1 new seed file at Task 4).
- `deny.toml` — likely no-op (see Tech stack above). Cross-checked at Task 14 state-4 verification.
- `rust-toolchain.toml`, `docs/envoy-rust/ENVOY_TARGET.md` — frozen per D-3.7 / D-3.9.

---

## Task index

The 14 tasks group by deliverable:

- **Task 1** (D1) — Workspace member `crates/envoy-http2/` scaffold + Cargo.lock sync + ADR-0027 inline.
- **Task 2** (D2.a) — `envoy-config` `CodecType::HTTP2` accept-flip + `Http2OverTlsNotSupported` ConfigError variant + 3 new validator tests + existing-test assertion update.
- **Task 3** (D2.b) — `envoy-config` `Http2ProtocolOptions` struct + `Http2ProtocolOptionsOutOfRange` ConfigError variant + `HttpConnectionManagerConfig.http2_protocol_options` field + 7 new validator tests.
- **Task 4** (D2 fuzz) — `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml` seed + `.gitignore` allow-list entry + 1 new corpus-walk acceptance test.
- **Task 5** (D3 error.rs) — `crates/envoy-http2/src/error.rs` with `Http2Error` typed-error enum.
- **Task 6** (D3 request.rs) — `crates/envoy-http2/src/request.rs` with `http_to_envoy_request` adapter (`http::Request<h2::RecvStream>` → `envoy_http1::codec::Request` value-type translator) + 2 unit tests.
- **Task 7** (D3 response.rs) — `crates/envoy-http2/src/response.rs` with `envoy_response_to_http2` adapter (`envoy_http1::codec::Response` → `h2::SendStream` emitter) + H2-forbidden hop-by-hop strip + 2 unit tests.
- **Task 8** (D3 codec.rs) — `crates/envoy-http2/src/codec.rs` with `Http2Codec` adapter (`Http2ProtocolOptions` → `h2::server::Builder` configuration).
- **Task 9** (D3 hcm.rs) — `crates/envoy-http2/src/hcm.rs` with `HCM` struct + `ConnectionHandler` impl (BoxFuture posture; per-connection handshake + per-stream `tokio::spawn` + `BuildOutcome::Proxy` 502 stub) + 8 unit tests.
- **Task 10** (D4) — `envoy-bin` HCM-on-H2 wiring at `crates/envoy-bin/src/main.rs:207` H1-vs-H2 dispatch on `hcm_cfg.codec_type` + new in-process integration test `crates/envoy-bin/tests/http2_direct_response.rs`.
- **Task 11** (D5) — Differential harness extensions: `tests/differential/Cargo.toml` `h2 = "0.4"` carve-out + `Driver::Http2` variant + `drive_http2` helper + `run_fixture` dispatch arm + 1 new harness unit test.
- **Task 12** (D6) — Fixture `tests/fixtures/0009-http2-direct-response/` (5 files) + Docker-gated wrapper `tests/differential/tests/http2_direct_response.rs`.
- **Task 13** (D7 part 1) — `tests/conformance/h2spec/` runner crate scaffold (Cargo.toml + lib.rs + h2spec.yaml + h2spec_runner.rs + initial known-failures.txt) + workspace member registration. ADR-0028 disposition: NOT landed (recorded inline in PROGRESS).
- **Task 14** (D7 part 2 + state-4) — `.github/workflows/ci.yml` `h2spec` binary provisioning + state-4 phase-done gate verification (all 9 fixtures + h2spec ≥95% + 5 stable-toolchain commands + fuzz short-budget run + cargo deny + Cargo.lock cross-check; CI run URL + per-fixture matrix quoted in PROGRESS Task 14).

The plan executes tasks 1 → 14 in order. **Tasks 5–9 (D3) all touch `crates/envoy-http2/src/`**; they are sequenced in dependency order (`error.rs` first because every other module references `Http2Error`; `request.rs` and `response.rs` next because `hcm.rs` consumes both translation adapters; `codec.rs` and `hcm.rs` last). **Tasks 2–4 (D2) all touch `crates/envoy-config/src/bootstrap.rs`**; they are sequenced D2.a → D2.b → fuzz seed because the fuzz seed in Task 4 references the schema landed at Task 3. Subagent-driven execution per the user's standing preference (auto-memory `feedback_execution_style`).

---

## Task 1 — Workspace member `crates/envoy-http2/` scaffold + Cargo.lock sync + ADR-0027

**Files:**

- Create: `crates/envoy-http2/Cargo.toml`
- Create: `crates/envoy-http2/src/lib.rs`
- Create: `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md`
- Modify: `Cargo.toml` (root) — `[workspace] members` gains `crates/envoy-http2`.
- Modify: `Cargo.lock` — synced inline (`cargo build -p envoy-http2`).
- Modify: `docs/envoy-rust/DECISIONS.md` — append ADR-0027 block.

**Estimated LoC:** ~80 (Cargo.toml ~25 + lib.rs ~30 + workspace member edit ~1 + ADR ~50 in DECISIONS.md + PROGRESS.md preamble ~10).

**Signposts settled:**

- Parent §6 signpost 1 (`h2 = "0.4"`): cross-check `cargo search h2 | head -1` at Step 1; if the published stable line has shifted, record the actual version in PROGRESS Task 1 and use the actual line.
- Parent §6 signpost 7 / 21 (`http` typed surface) + cross-sub-phase architectural rule 7: ADR-0027 lands; `http = "1"` direct-dep on `crates/envoy-http2/Cargo.toml`.
- Parent §6 signpost 14 (Cargo.lock cadence): inline at this task.
- Parent §6 signpost 15 (deny.toml): cross-check at Task 14; expected no-op.
- SPEC §6 local signpost 23 (`#![forbid(unsafe_code)]`): present in `lib.rs` at line 1.
- SPEC §6 local signpost 28 (LoC-budget reality check): record posture (a) "accept the estimate" inline in PROGRESS Task 1 narrative; the systematic-debugging confirmation is the inline narrative at PLAN preamble (this PLAN's "~14 tasks, ~2055 LoC" paragraph above).

- [ ] **Step 1.1: Verify the `h2` published version line.**

Run: `cargo search h2 --limit 1`
Expected: a line of the form `h2 = "0.4.<patch>"   # An HTTP/2 client and server`. If the major.minor line is `0.4.x`, proceed with `h2 = "0.4"`. If it is `0.5.x`, record the actual version in PROGRESS Task 1 and use that line in `Cargo.toml`. If the search fails (network), proceed with the SPEC's pinned `h2 = "0.4"` and record in PROGRESS.

- [ ] **Step 1.2: Create `crates/envoy-http2/Cargo.toml`.**

```toml
[package]
name = "envoy-http2"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
name = "envoy_http2"
path = "src/lib.rs"

[dependencies]
h2 = "0.4"
http = "1"
bytes = "1"
tokio = { version = "1", features = ["net", "io-util", "macros", "sync", "time"] }
thiserror = "2"
tracing = "0.1"
envoy-config = { path = "../envoy-config" }
envoy-listener = { path = "../envoy-listener" }
envoy-http1 = { path = "../envoy-http1" }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "test-util"] }
```

- [ ] **Step 1.3: Create `crates/envoy-http2/src/lib.rs` with module declarations only.**

Module bodies are added in Tasks 5–9. At Task 1 the modules are stub files referenced in `lib.rs` only insofar as the crate must compile; the actual module files (`error.rs`, `request.rs`, `response.rs`, `codec.rs`, `hcm.rs`) **are NOT created at Task 1** — they are created in their dedicated tasks (Tasks 5–9). To keep the crate compiling, `lib.rs` declares NO modules at Task 1; modules are added one at a time in Tasks 5–9 each via a same-task `pub mod <name>;` line edit.

```rust
#![forbid(unsafe_code)]

//! envoy-http2 — HTTP/2 cleartext (H2C prior-knowledge) codec wrapper.
//!
//! Owns the workspace's only direct dependency on the `h2` crate. All other
//! workspace crates import `envoy_http2::*` types instead of `h2::*` types.
//! See parent-phase-05 SPEC §3 cross-sub-phase architectural rule 1 + ADR-0022
//! (parent-05 split decision).
//!
//! Module decomposition (lands across phase 05.2 Tasks 5–9):
//!   - `error`    — typed-error enum (Task 5).
//!   - `request`  — H2-RecvStream → envoy-Request value translator (Task 6).
//!   - `response` — envoy-Response → H2-SendStream emitter (Task 7).
//!   - `codec`    — Http2ProtocolOptions → h2::server::Builder configurer (Task 8).
//!   - `hcm`      — ConnectionHandler impl for downstream H2C listeners (Task 9).
//!
//! 05.3-projected (NOT in 05.2):
//!   - `client`   — upstream H2C origination (envoy_http2::Client + ClientStream).
```

- [ ] **Step 1.4: Add `crates/envoy-http2` to root `Cargo.toml` `[workspace] members`.**

Edit `Cargo.toml` at the workspace root: in the `[workspace] members` array (currently has 11 entries from `crates/envoy-bin` through `tests/helpers/tls-echo-server`), insert `"crates/envoy-http2",` in alphabetic order (between `"crates/envoy-http1",` and `"crates/envoy-listener",`).

- [ ] **Step 1.5: Sync Cargo.lock.**

Run: `cargo build -p envoy-http2`
Expected: clean build (no warnings, no errors). The `Cargo.lock` file gains entries for `h2`, `http`, plus their transitive surface (`slab`, `fnv`, `tokio-util` if not already present, etc.). This sync is the load-bearing Task 1 step for parent §6 signpost 14.

- [ ] **Step 1.6: Run `cargo deny check`.**

Run: `cargo deny check`
Expected: `advisories ok, bans ok, licenses ok, sources ok` final-line gate signal. The `h2` and `http` crates are dual-licensed MIT/Apache-2.0 (already on the allow-list per project-wide cargo-deny posture). If a transitive crate brings a new license, record in PROGRESS Task 1 and add to `deny.toml` allow-list (extending the existing `[licenses]` block). Otherwise: no-op as projected.

- [ ] **Step 1.7: Append ADR-0027 to `docs/envoy-rust/DECISIONS.md`.**

Append after the existing ADR-0025 block (currently ending around line 493 of DECISIONS.md). The block carries `Date / Status / Context / Options-considered / Decision / Rationale / Consequences / Provenance` per the established 24-block precedent (mirrors ADR-0024's shape from 05.4 at lines 437–455).

```markdown

---

## ADR-0027: `http` crate (`http::Request` / `http::Response` / `http::HeaderMap`) typed-surface scoping

- **Date:** 2026-MM-DD (the date 05.2 Task 1 lands; backdated to ADR landing day per the ADR-0021 / ADR-0024 / ADR-0025 / ADR-0026 precedent).
- **Status:** accepted.
- **Context:** Phase 05.2 introduces `crates/envoy-http2/`, the workspace's sole-dep-owner of `h2 = "0.4"` (per parent-05 SPEC §3 cross-sub-phase architectural rule 1 + ADR-0022). The `h2` crate's API exposes `http::*` types directly: `h2::server::Connection::accept` returns `(http::Request<h2::RecvStream>, h2::server::SendResponse<bytes::Bytes>)`; `h2::client::SendRequest::send_request` accepts `http::Request<()>`; etc. The codec-edge translation modules in `envoy-http2` (`request.rs` Task 6, `response.rs` Task 7) import these symbols by name. The narrow scope question is whether `http` belongs as a direct dep on `crates/envoy-http2/Cargo.toml` (with this ADR documenting the narrow scoping, parallel to ADR-0021's narrow scoping for `regex`), or stays transitive-only through `h2`'s public API.
- **Options considered:**
  - **(i) Add `http = "1"` as a direct dep on `crates/envoy-http2/Cargo.toml`.** Direct imports `use http::{Request, Response, HeaderMap};` work cleanly; static-analysis tools (cargo-deny, `cargo audit`, transitive-version drift detection) see the dep explicitly. **Chosen.**
  - **(ii) Use `h2::http` re-exports to avoid a direct dep.** The `h2` crate may re-export the `http` types; investigation showed the re-export surface is partial (e.g., `http::HeaderName` not re-exported) — codec-edge translation requires reading individual fields (`request.method()`, `request.uri().path()`, `request.headers()`); incomplete re-exports block the implementation. Rejected.
  - **(iii) Use `http::*` transitively via `h2`'s public API only.** Treat `http::*` types as opaque types touched only at function boundaries. Rejected: the codec-edge translation requires field access; opaque-only access blocks the implementation.
- **Decision:** Add `http = "1"` as a direct dep on `crates/envoy-http2/Cargo.toml`. Narrowly scoped: only `crates/envoy-http2/` imports `http::*` symbols; no other workspace crate imports `http::*` directly (verifiable by `grep -rn 'use http::' crates/`). This is parallel to ADR-0021's narrow scoping for `regex` (where `regex` is permitted only on `crates/envoy-config/` for header / route matching at config-load time).
- **Rationale:** Direct deps are easier to reason about at static-analysis time. The `http` crate is dual-licensed MIT/Apache-2.0 (already covered by `deny.toml` license allow-list) and is the de-facto Rust HTTP types crate (used by `hyper`, `reqwest`, `axum`, etc.; first-party `rust-lang` org maintenance). Treating its first use as a foundation grant — bounded narrowly to the codec-edge translation surface in `envoy-http2` — is the cheapest, most honest formalization of the dep direction.
- **Consequences:** `crates/envoy-http2/Cargo.toml`'s `[dependencies]` section lists `http = "1"`. `Cargo.lock` formalizes `http` as a direct surface (it's already present transitively via `h2`'s deps, so the lock-file diff is structural-only — `http` moves from a transitive dep to a direct dep). Future scope-extension ADRs (HCM internal use of `http::*` types beyond codec-edge translation, filter-framework `http::*` types) name this ADR explicitly. `cargo-deny` license check at Task 1 + Task 14 confirms the existing MIT/Apache-2.0 allow-list covers `http`.
- **Provenance:** projected as conditional ADR-0024 in 05.2 SPEC §7 + parent-05 SPEC §7. Renumbered to ADR-0027 at landing time because phase 05.4 landed ADR-0024 / ADR-0025 / ADR-0026 between SPEC writeup (parent-05 state-2 commit `f1804a7`) and 05.2 execution. Mirrors the phase-03 ADR-renumbering precedent documented in `STATE.md` Notes section. Lands at 05.2 Task 1 alongside the `crates/envoy-http2/Cargo.toml` scaffold creation.

```

- [ ] **Step 1.8: Create `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` with Task 1's narration.**

The PROGRESS.md preamble carries the standard 04.x / 05.1 / 05.4 shape: a 1-paragraph orientation reading "Phase 05.2 PROGRESS log. SPEC at `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md` (committed at parent-05 state-2 SHA `f1804a7`); PLAN at `docs/envoy-rust/phases/05.2-http2-downstream/PLAN.md` (this PLAN's commit). Tasks 1–14 land in numeric order; each task carries Commit / Deliverables / ADR landed / Files modified / LoC / Verification / Deviations / Carryforward sections per 05.4 PROGRESS.md precedent." Then a `## Task 1` section quoting the cargo build + deny outputs and the ADR-0027 landing confirmation. Record the SPEC §6 signpost 28 LoC-budget posture (a) inline.

- [ ] **Step 1.9: Verify the build is clean at the workspace level.**

Run: `cargo build --workspace --all-targets`
Expected: `Finished dev profile target(s) in <Xs>` clean. If any warning surfaces (e.g., unused dep), record in PROGRESS Task 1 and fix inline (do not defer).

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean exit, no output.

Run: `cargo fmt --all -- --check`
Expected: clean exit, no output. The new `crates/envoy-http2/src/lib.rs` matches `rustfmt` output by construction (the lib.rs above is already canonical-formatted; if `cargo fmt --all` modifies it, accept the modification and re-add).

- [ ] **Step 1.10: Commit Task 1.**

```bash
git add crates/envoy-http2/Cargo.toml \
        crates/envoy-http2/src/lib.rs \
        Cargo.toml \
        Cargo.lock \
        docs/envoy-rust/DECISIONS.md \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-http2 crate scaffold + ADR-0027 (task 1)

New workspace member crates/envoy-http2/ ships with Cargo.toml + lib.rs
only (modules land in Tasks 5–9). Sole-dep-owner of h2 = \"0.4\" per
parent-05 SPEC §3 cross-sub-phase architectural rule 1. ADR-0027 lands
inline as a narrow direct-dep grant for http = \"1\" at the codec-edge
translation boundary (parallel to ADR-0021's regex narrow grant).

Cargo.lock synced; cargo deny clean (h2 + http MIT/Apache-2.0 already on
the allow-list).

ADR-0027 (renumbered from SPEC's projected ADR-0024 because 05.4 landed
ADR-0024/0025/0026; mirrors the phase-03 ADR-renumbering precedent)."
```

---

## Task 2 — `envoy-config` `CodecType::HTTP2` accept-flip + `Http2OverTlsNotSupported`

**Files:**

- Modify: `crates/envoy-config/src/lib.rs` — append `ConfigError::Http2OverTlsNotSupported` variant.
- Modify: `crates/envoy-config/src/bootstrap.rs` — narrow `validate_hcm` rejection at lines 1111–1118; add TLS+HTTP2 detection arm; update existing-test assertions at lines 3300/3335; append 3 new validator unit tests.

**Estimated LoC:** ~80 (validator extension ~30 + ConfigError variant ~5 + 3 new tests ~45).

**Signposts settled:**

- Parent §6 signposts: N/A directly (this is the schema half of D2).
- SPEC §3 D2.a: codec_type accepts `HTTP2`; rejects `HTTP3`; rejects `HTTP2` if listener has TLS transport_socket.

- [ ] **Step 2.1: Write the failing test for HTTP2 accept-on-plaintext-listener.**

Append to `crates/envoy-config/src/bootstrap.rs::tests` (the existing `#[cfg(test)] mod tests` block; current last test at line ~3500 area — verify the line via `grep -n '#\[test\]\|#\[cfg(test)\]' crates/envoy-config/src/bootstrap.rs | tail -5`):

```rust
    #[test]
    fn parses_hcm_with_codec_type_http2() {
        let yaml = r#"
node: { id: x, cluster: y }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let listener = &bs.static_resources.listeners[0];
        let TypedConfig::HttpConnectionManager(hcm) =
            listener.filter_chains[0].filters[0].typed_config.as_ref().unwrap()
        else { panic!("expected HCM"); };
        assert!(matches!(hcm.codec_type, CodecType::HTTP2));
    }
```

- [ ] **Step 2.2: Run test to verify it fails.**

Run: `cargo test -p envoy-config parses_hcm_with_codec_type_http2 -- --nocapture`
Expected: FAIL with `parse_bootstrap` returning `ConfigError::UnsupportedCodecType { got: HTTP2 }` (the existing pre-Task-2 validator at `crates/envoy-config/src/bootstrap.rs:1111-1118` rejects HTTP2).

- [ ] **Step 2.3: Narrow the `validate_hcm` rejection.**

Edit `crates/envoy-config/src/bootstrap.rs:1111-1118`. The pre-Task-2 shape is:

```rust
    match hcm.codec_type {
        CodecType::AUTO | CodecType::HTTP1 => {}
        CodecType::HTTP2 | CodecType::HTTP3 => {
            return Err(crate::ConfigError::UnsupportedCodecType {
                got: hcm.codec_type,
            });
        }
    }
```

Replace with:

```rust
    match hcm.codec_type {
        CodecType::AUTO | CodecType::HTTP1 | CodecType::HTTP2 => {}
        CodecType::HTTP3 => {
            return Err(crate::ConfigError::UnsupportedCodecType {
                got: hcm.codec_type,
            });
        }
    }
```

The TLS+HTTP2 rejection is added in Step 2.5 below (it requires the listener context, which `validate_hcm` does not currently receive — needs a signature extension).

- [ ] **Step 2.4: Run test to verify it passes.**

Run: `cargo test -p envoy-config parses_hcm_with_codec_type_http2 -- --nocapture`
Expected: PASS.

Run also the pre-existing tests touching CodecType to confirm no regression:

Run: `cargo test -p envoy-config -- codec_type`
Expected: the existing `rejects_hcm_with_codec_type_http2` test at lines ~3300 area now FAILS (the test asserted HTTP2 rejects, but the new behavior accepts). This is expected — fix it in Step 2.6 below.

- [ ] **Step 2.5: Write the failing test for TLS+HTTP2 rejection.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn rejects_hcm_with_codec_type_http2_on_tls_listener() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: tls_h2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
          transport_socket:
            name: envoy.transport_sockets.tls
            typed_config:
              "@type": type.googleapis.com/envoy.extensions.transport_sockets.tls.v3.DownstreamTlsContext
              common_tls_context:
                tls_certificates:
                  - certificate_chain: { filename: /tmp/cert.pem }
                    private_key: { filename: /tmp/key.pem }
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject TLS+HTTP2");
        assert!(
            matches!(err, crate::ConfigError::Http2OverTlsNotSupported),
            "expected Http2OverTlsNotSupported, got {err:?}"
        );
    }
```

Run: `cargo test -p envoy-config rejects_hcm_with_codec_type_http2_on_tls_listener -- --nocapture`
Expected: FAIL — either `Http2OverTlsNotSupported` doesn't exist as a variant (compile error) OR the test panics with a different error (e.g., the listener accepted because no TLS-detect arm exists in `validate_hcm` yet).

- [ ] **Step 2.6: Add the `Http2OverTlsNotSupported` ConfigError variant.**

Edit `crates/envoy-config/src/lib.rs`. Insert immediately before the existing `UnsupportedCodecType` variant at line 101:

```rust
    /// HCM `codec_type: HTTP2` declared on a listener whose `filter_chains[*]`
    /// carries a `transport_socket` of name `envoy.transport_sockets.tls`. Phase
    /// 05's H2 posture is plaintext H2C only — TLS+ALPN+H2 is deferred per
    /// parent-05 SPEC §4. Whichever later phase ships ALPN-negotiated H2 over
    /// TLS retires this variant.
    #[error(
        "HTTP/2 over TLS is not supported in phase 05; the listener must be plaintext or use codec_type: HTTP1/AUTO"
    )]
    Http2OverTlsNotSupported,
```

- [ ] **Step 2.7: Add the TLS+HTTP2 detection arm to `validate_hcm`.**

The current `validate_hcm` signature at `crates/envoy-config/src/bootstrap.rs:1106-1109` takes `(hcm: &mut HCMConfig, clusters: &[Cluster])`. To detect TLS at the listener level, the caller (the validator iteration site at line 1033 — `validate_hcm(hcm, &bootstrap.static_resources.clusters)?;`) must hand in the listener's filter_chain context. The minimal-scope change:

(a) extend `validate_hcm`'s signature to accept a `listener_has_tls: bool` flag:

```rust
fn validate_hcm(
    hcm: &mut HttpConnectionManagerConfig,
    clusters: &[Cluster],
    listener_has_tls: bool,
) -> Result<(), crate::ConfigError> {
```

(b) at the call site (line 1033 area), compute `listener_has_tls` from the enclosing listener's filter_chain context. The exact line in the validator iteration that wraps `validate_hcm` lives inside a per-listener / per-filter-chain / per-filter loop; the `listener_has_tls` value is `filter_chain.transport_socket.is_some_and(|ts| ts.name == TLS_TRANSPORT_SOCKET)` for the enclosing filter_chain. Verify the exact loop structure at task time via `grep -nB 5 'validate_hcm(' crates/envoy-config/src/bootstrap.rs`.

(c) inside `validate_hcm`, after the existing codec_type match (post-Step-2.3 shape), add:

```rust
    // 05.2 NEW — D2.a TLS+HTTP2 rejection.
    if matches!(hcm.codec_type, CodecType::HTTP2) && listener_has_tls {
        return Err(crate::ConfigError::Http2OverTlsNotSupported);
    }
```

If the call-site refactor for `listener_has_tls` proves more invasive than expected (e.g., the iteration shape doesn't expose the filter_chain readily), the planner may instead do the TLS-detect at the call site BEFORE invoking `validate_hcm` and pass the resolved boolean. Same observable behavior; record the chosen shape in PROGRESS Task 2.

- [ ] **Step 2.8: Run both tests to verify they pass.**

Run: `cargo test -p envoy-config -- parses_hcm_with_codec_type_http2 rejects_hcm_with_codec_type_http2_on_tls_listener`
Expected: both PASS.

- [ ] **Step 2.9: Update the existing `rejects_*_codec_type` tests to reflect the narrowed rejection.**

The pre-Task-2 tests at `crates/envoy-config/src/bootstrap.rs:3300` (`rejects_hcm_with_codec_type_http2`) and lines `:3335` area (`rejects_hcm_with_codec_type_http3`) both assert `UnsupportedCodecType`. The HTTP2 test must now be DELETED (the new behavior accepts HTTP2 on plaintext listeners; the dedicated `parses_hcm_with_codec_type_http2` from Step 2.1 is the positive replacement). The HTTP3 test stays unchanged (HTTP3 still rejects with `UnsupportedCodecType`).

Verify the exact deletion target by re-reading the test bodies via `grep -nA 30 'rejects_hcm_with_codec_type_http2' crates/envoy-config/src/bootstrap.rs`.

- [ ] **Step 2.10: Add a `still_rejects_hcm_with_codec_type_http3` test for explicit coverage.**

Append to `crates/envoy-config/src/bootstrap.rs::tests` (this may be redundant with the existing `:3335`-area test — verify at task time; if redundant, skip this step and record in PROGRESS):

```rust
    #[test]
    fn still_rejects_hcm_with_codec_type_http3() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: http3_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP3
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject HTTP3");
        assert!(
            matches!(err, crate::ConfigError::UnsupportedCodecType { got: CodecType::HTTP3 }),
            "expected UnsupportedCodecType{{got: HTTP3}}, got {err:?}"
        );
    }
```

- [ ] **Step 2.11: Run all envoy-config tests.**

Run: `cargo test -p envoy-config`
Expected: all tests pass; the test count grew by 2–3 (the deleted `rejects_hcm_with_codec_type_http2` is replaced by 1 positive + 1 TLS-rejection + optionally 1 explicit HTTP3 retention test).

- [ ] **Step 2.12: Run full clippy + fmt on the workspace.**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean exit.

Run: `cargo fmt --all -- --check`
Expected: clean exit.

- [ ] **Step 2.13: Update PROGRESS Task 2.**

Append a `## Task 2` section to PROGRESS.md noting: deliverable D2.a closed; ConfigError variant landed; 3 new tests + 1 deleted test; net LoC delta; verification outputs quoted.

- [ ] **Step 2.14: Commit Task 2.**

```bash
git add crates/envoy-config/src/bootstrap.rs \
        crates/envoy-config/src/lib.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-config CodecType::HTTP2 accept-flip + Http2OverTlsNotSupported (task 2)

validate_hcm narrows the existing HTTP2/HTTP3 rejection to HTTP3-only;
HTTP2 is accepted on plaintext listeners. New ConfigError variant
Http2OverTlsNotSupported rejects HTTP2 on listeners with a TLS
transport_socket — TLS+ALPN+H2 deferred per parent-05 SPEC §4. 3 new
validator unit tests + 1 deleted (the now-obsolete HTTP2 rejection
test).

D2.a per phase 05.2 SPEC §3."
```

---

## Task 3 — `envoy-config` `Http2ProtocolOptions` struct + `Http2ProtocolOptionsOutOfRange` ConfigError variant

**Files:**

- Modify: `crates/envoy-config/src/lib.rs` — append `ConfigError::Http2ProtocolOptionsOutOfRange { field, value, range }` variant; extend re-exports with `Http2ProtocolOptions`.
- Modify: `crates/envoy-config/src/bootstrap.rs` — add `Http2ProtocolOptions` struct; add `http2_protocol_options: Option<Http2ProtocolOptions>` field on `HttpConnectionManagerConfig`; extend `validate_hcm` with the RFC 7540 range checks; append 7 new validator unit tests.

**Estimated LoC:** ~200 (struct + re-export ~25; field on HCMConfig ~5; validator extension ~30; 7 tests ~140).

**Signposts settled:**

- Parent §6 signpost 2 (Http2ProtocolOptions schema subset): 4 fields only (`max_concurrent_streams`, `initial_stream_window_size`, `initial_connection_window_size`, `max_frame_size`). All optional; defaults sourced from `h2`-crate at HCM construction time (not at parse time).
- SPEC §3 D2.b: ranges per RFC 7540 — `max_frame_size` ∈ [16384, 16777215]; `initial_stream_window_size` ∈ [0, 2^31-1]; `initial_connection_window_size` ∈ [0, 2^31-1]; `max_concurrent_streams` no upper bound.

- [ ] **Step 3.1: Write the failing test for the `Http2ProtocolOptions` happy-path parse.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn parses_hcm_http2_protocol_options_default() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else { panic!(); };
        assert!(hcm.http2_protocol_options.is_none());
    }

    #[test]
    fn parses_hcm_http2_protocol_options_all_fields() {
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                http2_protocol_options:
                  max_concurrent_streams: 50
                  initial_stream_window_size: 131072
                  initial_connection_window_size: 262144
                  max_frame_size: 32768
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let bs = crate::parse_bootstrap(yaml).expect("parses");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else { panic!(); };
        let opts = hcm.http2_protocol_options.as_ref().expect("present");
        assert_eq!(opts.max_concurrent_streams, Some(50));
        assert_eq!(opts.initial_stream_window_size, Some(131072));
        assert_eq!(opts.initial_connection_window_size, Some(262144));
        assert_eq!(opts.max_frame_size, Some(32768));
    }
```

- [ ] **Step 3.2: Run tests to verify they fail.**

Run: `cargo test -p envoy-config -- parses_hcm_http2_protocol_options`
Expected: FAIL with compile error — `Http2ProtocolOptions` does not exist; `HttpConnectionManagerConfig.http2_protocol_options` does not exist.

- [ ] **Step 3.3: Add the `Http2ProtocolOptions` struct.**

Edit `crates/envoy-config/src/bootstrap.rs`. Insert immediately before the existing `RouteConfiguration` struct at line 335:

```rust
/// HTTP/2 protocol-level tuning knobs, listener-side. Subset of Envoy's
/// `envoy.config.core.v3.Http2ProtocolOptions`. Phase 05.2 ships 4 optional
/// `u32` fields per parent-05 SPEC §6 signpost 2; further fields
/// (allow_connect, allow_metadata, hpack_table_size,
/// override_stream_error_on_invalid_http_message, connection_keepalive, ...)
/// default to RFC-conformant values via the `h2` crate and defer until a
/// fixture or h2spec test forces them. Validator-checked range constraints
/// per RFC 7540 §6.5.2 / §6.9.1 / §6.9.2 land in `validate_hcm`.
#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Http2ProtocolOptions {
    /// SETTINGS_MAX_CONCURRENT_STREAMS. h2-crate default: 100. No upper bound
    /// per RFC 7540; zero is valid (peer would refuse all stream creation).
    #[serde(default)]
    pub max_concurrent_streams: Option<u32>,

    /// SETTINGS_INITIAL_WINDOW_SIZE. h2-crate default: 65535. Range
    /// [0, 2^31 - 1] per RFC 7540 §6.9.2.
    #[serde(default)]
    pub initial_stream_window_size: Option<u32>,

    /// Connection-level initial window size. h2-crate default: 65535. Range
    /// [0, 2^31 - 1] per RFC 7540 §6.9.1.
    #[serde(default)]
    pub initial_connection_window_size: Option<u32>,

    /// SETTINGS_MAX_FRAME_SIZE. h2-crate default: 16384. Range
    /// [16384, 16777215] per RFC 7540 §6.5.2.
    #[serde(default)]
    pub max_frame_size: Option<u32>,
}
```

- [ ] **Step 3.4: Add the `http2_protocol_options` field on `HttpConnectionManagerConfig`.**

Edit `crates/envoy-config/src/bootstrap.rs:294-301`. Pre-Task-3 shape:

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectionManagerConfig {
    pub stat_prefix: String,
    pub codec_type: CodecType,
    pub route_config: RouteConfiguration,
    pub http_filters: Vec<HttpFilter>,
}
```

Insert the new field after `codec_type` (preserving existing field order so the serde derive's behavior is stable):

```rust
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HttpConnectionManagerConfig {
    pub stat_prefix: String,
    pub codec_type: CodecType,

    /// 05.2 NEW: listener-side HTTP/2 protocol tuning (per SPEC §3 D2.b).
    /// Optional; absent means "use h2-crate defaults". Validator runs the
    /// RFC 7540 range checks at parse time only when `Some`.
    #[serde(default)]
    pub http2_protocol_options: Option<Http2ProtocolOptions>,

    pub route_config: RouteConfiguration,
    pub http_filters: Vec<HttpFilter>,
}
```

- [ ] **Step 3.5: Extend the re-exports.**

Edit `crates/envoy-config/src/lib.rs:10-19`. Insert `Http2ProtocolOptions` in alphabetic order (between `HttpFilterTypedConfig` and `Int64Range`):

```rust
pub use bootstrap::{
    Address, Admin, Bootstrap, CertificateValidationContext, Cluster, ClusterType, CodecType,
    CommonTlsContext, DataSource, DirectResponse, DnsLookupFamily, DownstreamTlsContext, Endpoint,
    FilterChain, FilterChainMatch, HeaderMatcher, HeaderMatcherMode, Http2ProtocolOptions,
    HttpConnectionManagerConfig, HttpFilter, HttpFilterTypedConfig, Int64Range, LbEndpoint,
    LbPolicy, Listener, LoadAssignment, LocalityLbEndpoints, NetworkFilter, Node, Route,
    RouteAction, RouteAction_Route, RouteConfiguration, RouteMatch, RouterConfig, SafeRegex,
    SocketAddress, StaticResources, StringMatcher, StringMatcherMode, TcpProxyConfig,
    TlsCertificate, TransportSocket, TransportSocketTypedConfig, TypedConfig, UpstreamTlsContext,
    VirtualHost,
};
```

- [ ] **Step 3.6: Run the parse-side tests to verify they pass.**

Run: `cargo test -p envoy-config -- parses_hcm_http2_protocol_options`
Expected: both PASS (compile clean; default test asserts `None`; all-fields test asserts `Some(...)` round-trip).

- [ ] **Step 3.7: Write failing tests for the validator range checks.**

Append to `crates/envoy-config/src/bootstrap.rs::tests` 4 range-rejection tests (one per RFC 7540 dimension):

```rust
    #[test]
    fn rejects_http2_protocol_options_max_frame_size_too_small() {
        let yaml = http2_options_yaml(/* max_frame_size = */ Some(1024), None, None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        match err {
            crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, value, range } => {
                assert_eq!(field, "max_frame_size");
                assert_eq!(value, 1024);
                assert_eq!(range, (16384, 16777215));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn rejects_http2_protocol_options_max_frame_size_too_large() {
        let yaml = http2_options_yaml(Some(17_000_000), None, None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "max_frame_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_initial_stream_window_size_too_large() {
        // 2^31 = 2147483648 is one above the max.
        let yaml = http2_options_yaml(None, Some(2_147_483_648), None, None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "initial_stream_window_size"
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn rejects_http2_protocol_options_initial_connection_window_size_too_large() {
        let yaml = http2_options_yaml(None, None, Some(2_147_483_648), None);
        let err = crate::parse_bootstrap(&yaml).expect_err("must reject");
        assert!(
            matches!(
                err,
                crate::ConfigError::Http2ProtocolOptionsOutOfRange { field, .. }
                    if field == "initial_connection_window_size"
            ),
            "got {err:?}"
        );
    }

    /// Builds a minimal HCM `codec_type: HTTP2` bootstrap with the given
    /// http2_protocol_options field values. Helper for the 4 range-rejection
    /// tests above. Each Option<u32> argument controls one field.
    fn http2_options_yaml(
        max_frame_size: Option<u32>,
        initial_stream_window_size: Option<u32>,
        initial_connection_window_size: Option<u32>,
        max_concurrent_streams: Option<u32>,
    ) -> String {
        let mut opts_block = String::from("                http2_protocol_options:\n");
        if let Some(v) = max_frame_size {
            opts_block.push_str(&format!("                  max_frame_size: {v}\n"));
        }
        if let Some(v) = initial_stream_window_size {
            opts_block.push_str(&format!(
                "                  initial_stream_window_size: {v}\n"
            ));
        }
        if let Some(v) = initial_connection_window_size {
            opts_block.push_str(&format!(
                "                  initial_connection_window_size: {v}\n"
            ));
        }
        if let Some(v) = max_concurrent_streams {
            opts_block.push_str(&format!(
                "                  max_concurrent_streams: {v}\n"
            ));
        }
        format!(
            r#"
node: {{ id: x, cluster: y }}
static_resources:
  listeners:
    - name: h2
      address: {{ socket_address: {{ address: 0.0.0.0, port_value: 9000 }} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
{opts_block}                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#,
        )
    }
```

Run: `cargo test -p envoy-config -- rejects_http2_protocol_options`
Expected: FAIL with compile error — `Http2ProtocolOptionsOutOfRange` does not exist.

- [ ] **Step 3.8: Add the `Http2ProtocolOptionsOutOfRange` ConfigError variant.**

Edit `crates/envoy-config/src/lib.rs`. Insert after the `Http2OverTlsNotSupported` variant added in Task 2 Step 2.6:

```rust
    /// `http2_protocol_options.<field>` value violates RFC 7540's wire-format
    /// range constraint. `field` names the offending field; `value` is the
    /// configured value; `range` is the inclusive (min, max) interval.
    #[error(
        "Http2ProtocolOptions field {field} value {value} out of range; must be in [{}, {}]",
        .range.0, .range.1
    )]
    Http2ProtocolOptionsOutOfRange {
        field: &'static str,
        value: u32,
        range: (u32, u32),
    },
```

- [ ] **Step 3.9: Extend `validate_hcm` with the range checks.**

Edit `crates/envoy-config/src/bootstrap.rs::validate_hcm`. After the codec_type match (post-Task-2 shape) and the TLS-rejection arm, append the http2_protocol_options range checks:

```rust
    // 05.2 NEW — D2.b: validate Http2ProtocolOptions ranges per RFC 7540
    // §6.5.2 / §6.9.1 / §6.9.2. Run only if Some; absent = h2-crate defaults.
    if let Some(opts) = &hcm.http2_protocol_options {
        const MAX_FRAME_SIZE_RANGE: (u32, u32) = (16384, 16_777_215);
        const WINDOW_SIZE_RANGE: (u32, u32) = (0, (1u32 << 31) - 1);

        if let Some(v) = opts.max_frame_size {
            if v < MAX_FRAME_SIZE_RANGE.0 || v > MAX_FRAME_SIZE_RANGE.1 {
                return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                    field: "max_frame_size",
                    value: v,
                    range: MAX_FRAME_SIZE_RANGE,
                });
            }
        }
        if let Some(v) = opts.initial_stream_window_size {
            if v > WINDOW_SIZE_RANGE.1 {
                return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                    field: "initial_stream_window_size",
                    value: v,
                    range: WINDOW_SIZE_RANGE,
                });
            }
        }
        if let Some(v) = opts.initial_connection_window_size {
            if v > WINDOW_SIZE_RANGE.1 {
                return Err(crate::ConfigError::Http2ProtocolOptionsOutOfRange {
                    field: "initial_connection_window_size",
                    value: v,
                    range: WINDOW_SIZE_RANGE,
                });
            }
        }
        // max_concurrent_streams has no upper bound per RFC 7540 §6.5.2;
        // zero is valid. No range check.
    }
```

- [ ] **Step 3.10: Run the validator tests to verify they pass.**

Run: `cargo test -p envoy-config -- rejects_http2_protocol_options`
Expected: all 4 PASS.

- [ ] **Step 3.11: Add the unknown-field rejection test.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn rejects_http2_protocol_options_unknown_field() {
        // hpack_table_size is a real Envoy field; envoy-rust 05.2 doesn't ship
        // it. The struct's deny_unknown_fields rejects.
        let yaml = r#"
node: { id: x, cluster: y }
static_resources:
  listeners:
    - name: h2
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                http2_protocol_options:
                  hpack_table_size: 4096
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "ok\n" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#;
        let err = crate::parse_bootstrap(yaml).expect_err("must reject");
        assert!(
            matches!(err, crate::ConfigError::Yaml(_)),
            "expected serde Yaml error for unknown field, got {err:?}"
        );
    }
```

Run: `cargo test -p envoy-config -- rejects_http2_protocol_options_unknown_field`
Expected: PASS (the struct's `#[serde(deny_unknown_fields)]` rejects `hpack_table_size`; the error surfaces as `ConfigError::Yaml(_)` from the `#[from] serde_yaml::Error` conversion).

- [ ] **Step 3.12: Run all envoy-config tests + clippy + fmt.**

Run: `cargo test -p envoy-config`
Expected: all tests pass; the test count grew by 7 (1 default + 1 all-fields + 4 range-reject + 1 unknown-field).

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 3.13: Update PROGRESS Task 3 + commit.**

```bash
git add crates/envoy-config/src/bootstrap.rs \
        crates/envoy-config/src/lib.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-config Http2ProtocolOptions struct + validator (task 3)

New listener-side struct on HttpConnectionManagerConfig with 4 optional
u32 fields (max_concurrent_streams, initial_stream_window_size,
initial_connection_window_size, max_frame_size) per parent-05 SPEC §6
signpost 2. Validator runs RFC 7540 range checks (§6.5.2 / §6.9.1 /
§6.9.2). New ConfigError variant Http2ProtocolOptionsOutOfRange.
7 new validator unit tests.

D2.b per phase 05.2 SPEC §3."
```

---

## Task 4 — Fuzz corpus seed `hcm_codec_http2.yaml` + `.gitignore` allow-list + corpus-walk acceptance test

**Files:**

- Create: `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml`
- Modify: `crates/envoy-config/fuzz/.gitignore` — append `!corpus/parse_bootstrap/hcm_codec_http2.yaml`.
- Modify: `crates/envoy-config/src/bootstrap.rs::tests` — append 1 corpus-walk acceptance test mirroring 04.x's pattern.

**Estimated LoC:** ~30 (seed YAML ~25 + gitignore line ~1 + test ~12).

**Signposts settled:**

- SPEC §6 local signpost 25: fuzz seed file path is `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml`; allow-list entry mirrors the existing 13-entry block in `.gitignore`.

- [ ] **Step 4.1: Write the failing corpus-walk acceptance test.**

Append to `crates/envoy-config/src/bootstrap.rs::tests`:

```rust
    #[test]
    fn fuzz_corpus_hcm_codec_http2_seed_parses() {
        // Sanity-check that the new fuzz seed parses cleanly through the
        // serde + validator pipeline. Mirrors the 04.x corpus-walk acceptance
        // pattern (e.g., `fuzz_corpus_hcm_route_to_cluster_seed_parses`).
        let yaml = include_str!(
            "../fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml"
        );
        let bs = crate::parse_bootstrap(yaml).expect("seed must parse");
        let TypedConfig::HttpConnectionManager(hcm) =
            bs.static_resources.listeners[0].filter_chains[0].filters[0]
                .typed_config
                .as_ref()
                .unwrap()
        else { panic!(); };
        assert!(matches!(hcm.codec_type, CodecType::HTTP2));
        let opts = hcm.http2_protocol_options.as_ref().expect("seed has options");
        assert_eq!(opts.max_concurrent_streams, Some(100));
    }
```

- [ ] **Step 4.2: Run the test to verify it fails.**

Run: `cargo test -p envoy-config fuzz_corpus_hcm_codec_http2_seed_parses -- --nocapture`
Expected: FAIL with `include_str!` compile error — the seed file does not exist yet.

- [ ] **Step 4.3: Create the fuzz corpus seed.**

Create `crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml`:

```yaml
node: { id: fuzz-hcm-codec-http2, cluster: fuzz }
static_resources:
  listeners:
    - name: h2_listener
      address: { socket_address: { address: 0.0.0.0, port_value: 9000 } }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_h2
                codec_type: HTTP2
                http2_protocol_options:
                  max_concurrent_streams: 100
                  initial_stream_window_size: 65535
                  initial_connection_window_size: 65535
                  max_frame_size: 16384
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "fuzz\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 4.4: Add the seed to the fuzz `.gitignore` allow-list.**

Edit `crates/envoy-config/fuzz/.gitignore`. Append after the existing 13-entry allow-list block (the current last allow-list line is `!corpus/parse_bootstrap/strict_dns_cluster.yaml`):

```
!corpus/parse_bootstrap/hcm_codec_http2.yaml
```

- [ ] **Step 4.5: Run the test to verify it passes.**

Run: `cargo test -p envoy-config fuzz_corpus_hcm_codec_http2_seed_parses -- --nocapture`
Expected: PASS.

- [ ] **Step 4.6: Optionally run the fuzz target locally for a short budget.**

Run (optional, requires nightly toolchain): `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=10`
Expected: clean run; corpus walks the new seed without crash.

If the local environment doesn't have `cargo +nightly fuzz`, skip and rely on the CI fuzz job at Task 14 to exercise the seed.

- [ ] **Step 4.7: Verify the seed appears in `git status` (allow-list working).**

Run: `git status crates/envoy-config/fuzz/corpus/parse_bootstrap/`
Expected: shows `hcm_codec_http2.yaml` as a new file (NOT ignored). If the file does not appear, the `.gitignore` allow-list entry is wrong — re-check.

- [ ] **Step 4.8: Update PROGRESS Task 4 + commit.**

```bash
git add crates/envoy-config/fuzz/corpus/parse_bootstrap/hcm_codec_http2.yaml \
        crates/envoy-config/fuzz/.gitignore \
        crates/envoy-config/src/bootstrap.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: fuzz corpus seed hcm_codec_http2.yaml (task 4)

Seeds the existing parse_bootstrap fuzz target with one HCM codec_type:
HTTP2 + listener-side http2_protocol_options bootstrap. Allow-list
entry in fuzz/.gitignore. 1 new corpus-walk acceptance test verifies
the seed parses cleanly through the schema landed in tasks 2-3.

D2 fuzz per phase 05.2 SPEC §1 acceptance signal (d)."
```

---

## Task 5 — `crates/envoy-http2/src/error.rs` (Http2Error typed-error enum)

**Files:**

- Create: `crates/envoy-http2/src/error.rs`
- Modify: `crates/envoy-http2/src/lib.rs` — declare `mod error;` + `pub use error::Http2Error;`.

**Estimated LoC:** ~60 (error.rs ~50 + lib.rs delta ~3 + 1 small unit test ~7).

**Signposts settled:**

- SPEC §3 D3 error.rs variants: `H2Handshake { source: h2::Error }`, `H2StreamAccept { source: h2::Error }`, `H2BodyRead { source: h2::Error }`, `MissingAuthority`, `MalformedH2HeaderBlock`, `BadStatusCode { status: u16 }`. Use `thiserror = "2"` per the established library-crate posture.

- [ ] **Step 5.1: Add `mod error;` + `pub use error::Http2Error;` to `lib.rs`.**

Edit `crates/envoy-http2/src/lib.rs`. Insert before the closing `//! 05.3-projected ...` doc comment:

```rust
mod error;

pub use error::Http2Error;
```

(The module body lands in Step 5.3 below; declaring the mod first lets us iterate on the test before the impl exists.)

- [ ] **Step 5.2: Write the failing test for Http2Error::Display.**

Create `crates/envoy-http2/src/error.rs` with only the test module (no enum yet):

```rust
//! Typed errors for the envoy-http2 crate. See SPEC §3 D3.

#[cfg(test)]
mod tests {
    use super::Http2Error;

    #[test]
    fn missing_authority_displays_descriptively() {
        let e = Http2Error::MissingAuthority;
        let s = format!("{e}");
        assert!(s.contains("authority"), "expected mention of authority: {s}");
    }

    #[test]
    fn bad_status_code_displays_value() {
        let e = Http2Error::BadStatusCode { status: 999 };
        let s = format!("{e}");
        assert!(s.contains("999"), "expected mention of 999: {s}");
    }
}
```

Run: `cargo test -p envoy-http2`
Expected: FAIL with compile error — `Http2Error` not defined.

- [ ] **Step 5.3: Implement the `Http2Error` enum.**

Edit `crates/envoy-http2/src/error.rs`. Prepend (above the test module):

```rust
//! Typed errors for the envoy-http2 crate. See SPEC §3 D3.
//!
//! The enum carries variants for each codec-edge failure mode. Source-
//! preserving variants wrap `h2::Error` via `#[source]` so the original
//! framing-level diagnostic survives the type translation. No `From<h2::Error>`
//! blanket impl — call sites pick the right variant per failure context (e.g.,
//! handshake failure vs. stream accept failure vs. body-read failure).

#[derive(Debug, thiserror::Error)]
pub enum Http2Error {
    /// `h2::server::handshake` failed (no PRI preamble; bad SETTINGS; etc.).
    #[error("HTTP/2 handshake failed: {source}")]
    H2Handshake {
        #[source]
        source: h2::Error,
    },

    /// `h2::server::Connection::accept` returned a fatal error mid-connection.
    #[error("HTTP/2 stream accept failed: {source}")]
    H2StreamAccept {
        #[source]
        source: h2::Error,
    },

    /// Reading body bytes from `h2::RecvStream` failed.
    #[error("HTTP/2 body read failed: {source}")]
    H2BodyRead {
        #[source]
        source: h2::Error,
    },

    /// The H2 request carried no `:authority` pseudo-header. envoy-rust's HCM
    /// route-walk requires `Host:` (synthesized from `:authority`) per
    /// cross-sub-phase architectural rule 3.
    #[error("HTTP/2 request missing :authority pseudo-header (required for Host-driven route-walk)")]
    MissingAuthority,

    /// The H2 HEADERS block carried structurally invalid pseudo-headers
    /// (e.g., missing `:method`, missing `:path`, or an unrecognized
    /// pseudo-header name). The h2 codec normally catches these earlier;
    /// this variant is a defense-in-depth fallback.
    #[error("HTTP/2 header block is structurally malformed")]
    MalformedH2HeaderBlock,

    /// envoy-rust attempted to emit a status code outside the valid HTTP
    /// range (100..=599) on the H2 wire. Defense-in-depth — the route-walk
    /// validates status codes at parse time, so this should be unreachable
    /// from any valid config.
    #[error("invalid HTTP status code on H2 wire: {status}")]
    BadStatusCode { status: u16 },
}

```

- [ ] **Step 5.4: Run tests to verify they pass.**

Run: `cargo test -p envoy-http2`
Expected: 2 PASS (the 2 unit tests in error.rs).

Run: `cargo build -p envoy-http2`
Expected: clean (no warnings).

- [ ] **Step 5.5: Update PROGRESS Task 5 + commit.**

```bash
git add crates/envoy-http2/src/error.rs \
        crates/envoy-http2/src/lib.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-http2::Http2Error typed-error enum (task 5)

6-variant thiserror enum for the codec-edge failure modes. 2 unit
tests verify Display.

D3 error.rs per phase 05.2 SPEC §3."
```

---

## Task 6 — `crates/envoy-http2/src/request.rs` (`http_to_envoy_request` adapter + 2 tests)

**Files:**

- Create: `crates/envoy-http2/src/request.rs`
- Modify: `crates/envoy-http2/src/lib.rs` — declare `pub mod request;`.

**Estimated LoC:** ~110 (impl ~80 + 2 tests ~30).

**Signposts settled:**

- Parent §6 signpost 11 (Header lowercasing): `h2` already delivers lowercase headers; defensive lowercase NOT required on the request side (only enforced on the response side at Task 7).
- Parent §6 signpost 12 (`:method`/`:path`/`:authority`/`:scheme` translation): `:method` → `Request.method` (parsed via `envoy_http1::codec::Method::parse_token` if such a constructor exists; else direct enum match — verify at task time via `grep -n 'pub fn.*method\|impl.*Method' crates/envoy-http1/src/codec.rs`); `:path` → `Request.path`; `:authority` synthesized as `Host:` row at the bottom; `:scheme` ignored.
- Cross-sub-phase architectural rule 3: `:authority` → `Host:` mapping is mandatory.

- [ ] **Step 6.1: Add `pub mod request;` to `lib.rs`.**

Edit `crates/envoy-http2/src/lib.rs`. Insert immediately above the `mod error;` line:

```rust
pub mod request;
```

- [ ] **Step 6.2: Write the failing tests.**

Create `crates/envoy-http2/src/request.rs` with only the test module:

```rust
//! H2 → envoy-Request value translator. See SPEC §3 D3.

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, Method, Request as HttpRequest, Uri};

    /// Build an `http::Request` with the given pseudo-header values + extra
    /// headers, with a body of given bytes. Used by the tests below.
    fn build_request(
        method: &str,
        uri: &str,
        authority: Option<&str>,
        extras: &[(&str, &str)],
        body: Bytes,
    ) -> HttpRequest<Bytes> {
        let mut builder = HttpRequest::builder()
            .method(Method::from_bytes(method.as_bytes()).unwrap())
            .uri(uri.parse::<Uri>().unwrap());
        for (n, v) in extras {
            builder = builder.header(*n, *v);
        }
        let mut req = builder.body(body).unwrap();
        if let Some(a) = authority {
            req.headers_mut()
                .insert(http::header::HOST, a.parse().unwrap());
            // Note: in real H2, :authority is exposed via `request.uri().authority()`
            // when the Uri is in absolute form. Set the Uri appropriately instead:
            *req.uri_mut() = format!("http://{a}{uri}").parse().unwrap();
        }
        let _: &HeaderMap = req.headers();
        req
    }

    #[test]
    fn http_to_envoy_request_lowercases_headers() {
        let req = build_request(
            "GET",
            "/",
            Some("test.example"),
            &[("User-Agent", "testharness"), ("X-Foo", "bar")],
            Bytes::new(),
        );
        let out = http_to_envoy_request(req).expect("translates");
        // h2 lowercases header names on receive; verify our adapter preserves
        // (and that the value is unchanged).
        let names: Vec<&str> = out.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("user-agent")));
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("x-foo")));
        let ua = out
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("user-agent"))
            .unwrap();
        assert_eq!(ua.1, "testharness");
    }

    #[test]
    fn http_to_envoy_request_synthesizes_host_from_authority() {
        let req = build_request(
            "GET",
            "/",
            Some("test.example"),
            &[],
            Bytes::new(),
        );
        let out = http_to_envoy_request(req).expect("translates");
        let host = out
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("host"))
            .expect("Host header must be synthesized from :authority");
        assert_eq!(host.1, "test.example");
    }
}
```

Run: `cargo test -p envoy-http2`
Expected: FAIL with compile error — `http_to_envoy_request` not defined.

- [ ] **Step 6.3: Implement `http_to_envoy_request`.**

Prepend to `crates/envoy-http2/src/request.rs` (above the test module):

```rust
//! H2 → envoy-Request value translator. See SPEC §3 D3.
//!
//! The adapter consumes an `http::Request<B>` (where `B` is the body type —
//! typically `h2::RecvStream` post-drain into `bytes::Bytes` for the runtime
//! consumer; arbitrary body types for unit tests) and emits an
//! `envoy_http1::codec::Request` value-type. Pseudo-headers map per parent-05
//! SPEC §6 signpost 12:
//!   - `:method` → `Request.method`
//!   - `:path`   → `Request.path` (raw string; query string preserved if present)
//!   - `:authority` → synthesized as `Host: <authority>` row at the bottom of
//!                  `Request.headers` (per cross-sub-phase architectural rule 3,
//!                  required for the existing 04.x route-walk)
//!   - `:scheme` → ignored (envoy-rust's HCM doesn't dispatch on scheme)

use bytes::Bytes;
use envoy_http1::codec::{HttpVersion, Request};
use http::Request as HttpRequest;

use crate::error::Http2Error;

/// Translate an H2 request (post-body-drain into `Bytes`) into an
/// `envoy_http1::codec::Request` value type. Pseudo-headers are unpacked per
/// the SPEC §6 signpost 12 mapping.
pub fn http_to_envoy_request(req: HttpRequest<Bytes>) -> Result<Request, Http2Error> {
    let (parts, body) = req.into_parts();

    // :method → method (raw string preservation; the envoy_http1::codec::Request
    // carries the method as a String, matching the H1 codec's posture).
    let method = parts.method.as_str().to_string();

    // :path → path. h2 exposes the path through `parts.uri.path_and_query()`;
    // for absolute URIs (http://authority/path) the path component is just
    // `/path`. For path-only URIs it's the same. Preserve the query if present.
    let path = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());

    // :authority → Host: row. h2 exposes :authority via `parts.uri.authority()`
    // OR via the `Host:` header (depending on h2-version + handshake details).
    // Prefer authority(); fall back to Host header if present.
    let authority_str: Option<String> = parts
        .uri
        .authority()
        .map(|a| a.as_str().to_string())
        .or_else(|| {
            parts
                .headers
                .get(http::header::HOST)
                .and_then(|hv| hv.to_str().ok())
                .map(str::to_string)
        });

    let authority = authority_str.ok_or(Http2Error::MissingAuthority)?;

    // Translate regular headers. h2 delivers names lowercased; preserve as-is.
    // Skip the Host header here (we'll re-add the synthesized one at the bottom).
    let mut headers: Vec<(String, String)> = Vec::with_capacity(parts.headers.len() + 1);
    for (name, value) in parts.headers.iter() {
        if name.as_str().eq_ignore_ascii_case("host") {
            continue;
        }
        let value_str = value
            .to_str()
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?
            .to_string();
        headers.push((name.as_str().to_string(), value_str));
    }
    headers.push(("host".to_string(), authority));

    Ok(Request {
        method,
        path,
        version: HttpVersion::Http11, // route-walk treats this as H1.1; H2 framing is at the codec edge.
        headers,
        bytes_consumed: 0,
        body: Some(body),
    })
}
```

**Verify the `Request` struct shape at task time** by `grep -n 'pub struct Request' crates/envoy-http1/src/codec.rs`. The above assumes:

- `Request.method: String` (NOT a typed `Method` enum)
- `Request.path: String`
- `Request.version: HttpVersion` (with an `Http11` variant)
- `Request.headers: Vec<(String, String)>`
- `Request.bytes_consumed: usize`
- `Request.body: Option<Bytes>`

If the actual shape differs (e.g., `method` is a typed enum `Method`), adjust the constructor accordingly. The reference shape is verified at `crates/envoy-http1/src/hcm.rs:233-241` where the existing 04.3 router code constructs `Request { method, path, version: HttpVersion::Http11, headers, bytes_consumed: 0, body: Some(Bytes::new()) }` — the field set matches.

- [ ] **Step 6.4: Run the tests to verify they pass.**

Run: `cargo test -p envoy-http2`
Expected: all tests pass (2 from error.rs + 2 from request.rs = 4 PASS).

- [ ] **Step 6.5: Run clippy + fmt.**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 6.6: Update PROGRESS Task 6 + commit.**

```bash
git add crates/envoy-http2/src/request.rs \
        crates/envoy-http2/src/lib.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-http2::request::http_to_envoy_request adapter (task 6)

H2-side request translation: http::Request<Bytes> → envoy_http1::codec::
Request. :authority synthesized as Host: row at the bottom of headers
(per cross-sub-phase architectural rule 3, required for Host-driven
route-walk). 2 unit tests cover header lowercasing + Host synthesis.

D3 request.rs per phase 05.2 SPEC §3."
```

---

## Task 7 — `crates/envoy-http2/src/response.rs` (`envoy_response_to_http2` adapter + hop-by-hop strip + 2 tests)

**Files:**

- Create: `crates/envoy-http2/src/response.rs`
- Modify: `crates/envoy-http2/src/lib.rs` — declare `pub mod response;`.

**Estimated LoC:** ~120 (impl ~80 + hop-by-hop strip ~10 + 2 tests ~30).

**Signposts settled:**

- Cross-sub-phase architectural rule 4: H2-forbidden hop-by-hop headers stripped at the codec edge.
- Parent §6 signpost 11: defensive lowercase before send.

- [ ] **Step 7.1: Add `pub mod response;` to `lib.rs`.**

Edit `crates/envoy-http2/src/lib.rs`. Insert after `pub mod request;`:

```rust
pub mod response;
```

- [ ] **Step 7.2: Write the failing tests.**

Create `crates/envoy-http2/src/response.rs` with only the test module:

```rust
//! envoy-Response → H2 SendStream emitter. See SPEC §3 D3.

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use envoy_http1::Response;

    fn synth_response(
        status: u16,
        headers: Vec<(&str, &str)>,
        body: &[u8],
    ) -> Response {
        Response {
            status,
            headers: headers
                .into_iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            body: Bytes::copy_from_slice(body),
        }
    }

    #[test]
    fn envoy_response_to_http2_strips_h2_forbidden_headers() {
        let resp = synth_response(
            200,
            vec![
                ("server", "envoy-rust"),
                ("connection", "close"),
                ("transfer-encoding", "chunked"),
                ("upgrade", "h2c"),
                ("keep-alive", "timeout=5"),
                ("proxy-connection", "keep-alive"),
                ("content-type", "text/plain"),
            ],
            b"ok",
        );
        let http_resp = build_http_response(&resp).expect("builds");
        let names: Vec<&str> = http_resp
            .headers()
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        for forbidden in &[
            "connection",
            "transfer-encoding",
            "upgrade",
            "keep-alive",
            "proxy-connection",
        ] {
            assert!(
                !names.iter().any(|n| n.eq_ignore_ascii_case(forbidden)),
                "expected `{forbidden}` to be stripped, but found in {names:?}"
            );
        }
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("server")));
        assert!(names.iter().any(|n| n.eq_ignore_ascii_case("content-type")));
    }

    #[test]
    fn envoy_response_to_http2_preserves_status_and_body() {
        let resp = synth_response(418, vec![("content-type", "text/plain")], b"teapot");
        let http_resp = build_http_response(&resp).expect("builds");
        assert_eq!(http_resp.status().as_u16(), 418);
        // body() returns the unit body for an http::Response<()> (the actual
        // body bytes are sent via h2::SendStream::send_data; here we verify
        // build_http_response correctly carries the status + headers, and
        // we delegate the body-write check to the integration test).
        assert!(http_resp.headers().contains_key(http::header::CONTENT_TYPE));
    }
}
```

Run: `cargo test -p envoy-http2`
Expected: FAIL with compile error — `build_http_response` not defined.

- [ ] **Step 7.3: Implement the response-translation adapter.**

Prepend to `crates/envoy-http2/src/response.rs` (above the test module):

```rust
//! envoy-Response → H2 SendStream emitter. See SPEC §3 D3.
//!
//! The response-translation surface is split into two pieces:
//!   - `build_http_response(resp)` — translates an `envoy_http1::Response` into
//!     an `http::Response<()>` (status + headers; body is sent separately).
//!     Pure function; testable in isolation.
//!   - `send_envoy_response(send_response, resp)` — drives the actual H2 wire
//!     emission via `h2::server::SendResponse::send_response` + body
//!     send_data. Async; integration-tested via the HCM tests in Task 9.
//!
//! H2-forbidden hop-by-hop headers (RFC 7540 §8.1.2.2: connection,
//! transfer-encoding, upgrade, keep-alive, proxy-connection) are stripped
//! defensively in `build_http_response` per cross-sub-phase architectural
//! rule 4. Header names are emitted lowercase per RFC 7540 §8.1.2 (the h2
//! crate would reject uppercase names; defense-in-depth).

use envoy_http1::Response;
use http::{HeaderName, HeaderValue, Response as HttpResponse, StatusCode};

use crate::error::Http2Error;

/// Headers that MUST NOT appear on H2 wire (RFC 7540 §8.1.2.2). The H2 codec
/// would reject these at emission; the strip here is defense-in-depth and
/// keeps the route-walk's H1-shaped response objects compatible with H2 wire
/// emission.
const H2_FORBIDDEN_HOP_BY_HOP: &[&str] = &[
    "connection",
    "transfer-encoding",
    "upgrade",
    "keep-alive",
    "proxy-connection",
];

/// Translate an `envoy_http1::Response` into an `http::Response<()>` carrying
/// the status + headers (with H2-forbidden headers stripped). The body is
/// sent separately via `h2::SendStream::send_data` in `send_envoy_response`.
pub fn build_http_response(resp: &Response) -> Result<HttpResponse<()>, Http2Error> {
    let status = StatusCode::from_u16(resp.status).map_err(|_| Http2Error::BadStatusCode {
        status: resp.status,
    })?;
    let mut builder = HttpResponse::builder().status(status);
    for (name, value) in &resp.headers {
        let name_lc = name.to_ascii_lowercase();
        if H2_FORBIDDEN_HOP_BY_HOP.contains(&name_lc.as_str()) {
            continue;
        }
        let header_name = HeaderName::from_bytes(name_lc.as_bytes())
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        let header_value = HeaderValue::from_str(value)
            .map_err(|_| Http2Error::MalformedH2HeaderBlock)?;
        builder = builder.header(header_name, header_value);
    }
    builder
        .body(())
        .map_err(|_| Http2Error::MalformedH2HeaderBlock)
}

/// Drive the actual H2 response emission. Sends the response head via
/// `send_response`, then the body via `send_data(end_of_stream=true)`. Errors
/// surface as typed `Http2Error::H2BodyRead`-shaped variants on the wire side
/// (a misnomer when applied to body WRITE; future cleanup may rename the
/// variant — defer per SPEC §6 local signpost 21).
pub async fn send_envoy_response(
    mut send_response: h2::server::SendResponse<bytes::Bytes>,
    resp: Response,
) -> Result<(), Http2Error> {
    let head = build_http_response(&resp)?;
    let mut send_stream = send_response
        .send_response(head, /* end_of_stream = */ resp.body.is_empty())
        .map_err(|source| Http2Error::H2StreamAccept { source })?;
    if !resp.body.is_empty() {
        send_stream
            .send_data(resp.body, /* end_of_stream = */ true)
            .map_err(|source| Http2Error::H2BodyRead { source })?;
    }
    Ok(())
}
```

**Verify the `envoy_http1::Response` struct shape at task time** via `grep -nA 6 'pub struct Response' crates/envoy-http1/src/response.rs`. The above assumes:

- `Response.status: u16`
- `Response.headers: Vec<(String, String)>`
- `Response.body: Bytes`

If the actual shape differs, adjust the field accesses. (The actual file at `crates/envoy-http1/src/response.rs` is read by the executor at task time.)

- [ ] **Step 7.4: Run tests to verify they pass.**

Run: `cargo test -p envoy-http2`
Expected: all tests pass (2 error + 2 request + 2 response = 6 PASS).

- [ ] **Step 7.5: Run clippy + fmt.**

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 7.6: Update PROGRESS Task 7 + commit.**

```bash
git add crates/envoy-http2/src/response.rs \
        crates/envoy-http2/src/lib.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-http2::response adapters (task 7)

build_http_response translates envoy_http1::Response into http::Response
<()> with H2-forbidden hop-by-hop headers (connection, transfer-
encoding, upgrade, keep-alive, proxy-connection) stripped defensively
per RFC 7540 §8.1.2.2 + cross-sub-phase architectural rule 4. Header
names lowercased before emission per RFC 7540 §8.1.2. send_envoy_
response drives the H2 wire emission.

D3 response.rs per phase 05.2 SPEC §3."
```

---

## Task 8 — `crates/envoy-http2/src/codec.rs` (`Http2Codec` adapter / `h2::server::Builder` configurer)

**Files:**

- Create: `crates/envoy-http2/src/codec.rs`
- Modify: `crates/envoy-http2/src/lib.rs` — declare `pub mod codec;`.

**Estimated LoC:** ~80 (impl ~60 + 1 unit test ~20).

**Signposts settled:**

- SPEC §3 D3 codec.rs role: thin adapter exposing `h2::server::Builder` configuration; mostly delegates. Maps `Http2ProtocolOptions` to `Builder::max_concurrent_streams` / `Builder::initial_window_size` / `Builder::initial_connection_window_size` / `Builder::max_frame_size`.

- [ ] **Step 8.1: Add `pub mod codec;` to `lib.rs`.**

Edit `crates/envoy-http2/src/lib.rs`. Insert after `pub mod response;`:

```rust
pub mod codec;
```

- [ ] **Step 8.2: Write the failing test.**

Create `crates/envoy-http2/src/codec.rs` with only the test module:

```rust
//! H2 codec adapter: maps envoy-config Http2ProtocolOptions onto h2::server::Builder.
//! See SPEC §3 D3.

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::Http2ProtocolOptions;

    #[test]
    fn build_h2_server_applies_protocol_options() {
        let opts = Http2ProtocolOptions {
            max_concurrent_streams: Some(50),
            initial_stream_window_size: Some(131072),
            initial_connection_window_size: Some(262144),
            max_frame_size: Some(32768),
        };
        // The function returns a configured builder; we cannot easily
        // introspect the builder's private fields, so we just verify the
        // call compiles and returns a builder that can be subsequently
        // used (smoke test). The actual behavioral verification is in the
        // hcm.rs `h2_protocol_options_max_concurrent_streams_applied` test
        // (Task 9), which observes the wire effect.
        let _builder = build_h2_server(Some(&opts));
        let _builder_default = build_h2_server(None);
    }
}
```

Run: `cargo test -p envoy-http2`
Expected: FAIL with compile error — `build_h2_server` not defined.

- [ ] **Step 8.3: Implement `build_h2_server`.**

Prepend to `crates/envoy-http2/src/codec.rs`:

```rust
//! H2 codec adapter: maps envoy-config Http2ProtocolOptions onto
//! h2::server::Builder. See SPEC §3 D3.
//!
//! Thin adapter — the actual H2 codec lives in the `h2` crate. This module
//! exists to centralize the Http2ProtocolOptions → Builder field-by-field
//! mapping so the HCM and (in 05.3) the Client share the same configuration
//! shape. Only the listener-side Builder is mapped here in 05.2; the
//! client-side `h2::client::Builder` mapping lands in 05.3 alongside `client.rs`.

use envoy_config::Http2ProtocolOptions;

/// Build an `h2::server::Builder` configured per the given options. Absent
/// options leave the field at the `h2`-crate default.
pub fn build_h2_server(opts: Option<&Http2ProtocolOptions>) -> h2::server::Builder {
    let mut builder = h2::server::Builder::new();
    if let Some(o) = opts {
        if let Some(v) = o.max_concurrent_streams {
            builder.max_concurrent_streams(v);
        }
        if let Some(v) = o.initial_stream_window_size {
            builder.initial_window_size(v);
        }
        if let Some(v) = o.initial_connection_window_size {
            builder.initial_connection_window_size(v);
        }
        if let Some(v) = o.max_frame_size {
            builder.max_frame_size(v);
        }
    }
    builder
}
```

**Verify the `h2::server::Builder` setter signatures at task time** by `cargo doc -p envoy-http2 --no-deps` and inspecting the `h2` crate's docs (or `cargo tree | grep h2` then `cd ~/.cargo/registry/src/*/h2-*/src/server.rs`). The `h2 = "0.4"` line API may use slightly different setter names (e.g., `initial_window_size` vs. `initial_stream_window_size`). Adjust the field names in the impl to match the actual `h2 = "0.4"` API. **If any setter name differs**, record the actual signature in PROGRESS Task 8 and fix the call site.

- [ ] **Step 8.4: Run tests + clippy + fmt.**

Run: `cargo test -p envoy-http2`
Expected: all tests pass (1 codec + 2 error + 2 request + 2 response = 7 PASS).

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 8.5: Update PROGRESS Task 8 + commit.**

```bash
git add crates/envoy-http2/src/codec.rs \
        crates/envoy-http2/src/lib.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-http2::codec::build_h2_server (task 8)

Thin adapter mapping envoy_config::Http2ProtocolOptions onto
h2::server::Builder field-by-field. Centralizes the configuration shape
so the HCM (Task 9) and the future Client (05.3) share it.

D3 codec.rs per phase 05.2 SPEC §3."
```

---

## Task 9 — `crates/envoy-http2/src/hcm.rs` (HCM ConnectionHandler impl + 8 unit tests)

**Files:**

- Create: `crates/envoy-http2/src/hcm.rs`
- Modify: `crates/envoy-http2/src/lib.rs` — declare `pub mod hcm;` + `pub use hcm::{HCM, HCMConfig};`.
- Modify: `crates/envoy-http1/src/hcm.rs` — IF `BuildOutcome` and/or `build_response` are not currently `pub`, lift visibility from `pub(crate)` (or implicit private) to `pub`.
- Modify: `crates/envoy-http1/src/lib.rs` — IF lifting visibility, extend the existing `pub use hcm::{HCM, HCMConfig};` re-export to include `BuildOutcome` and `build_response`.

**Estimated LoC:** ~440 (HCM impl ~250 + 8 unit tests ~190; visibility lift in envoy-http1 ~3 LoC).

**Signposts settled:**

- SPEC §6 local signpost 19 (`async_trait`): NOT used. Mirror the existing in-tree `BoxFuture` posture from `envoy_http1::HCM` (`crates/envoy-http1/src/hcm.rs:98-110`).
- Parent §6 signpost 6 / SPEC §6 local signpost 20: per-stream `tokio::spawn` direct, fire-and-forget; per-stream errors logged via `tracing::error!` and do not propagate to connection driver.
- Parent §6 signpost 13: trust `h2` codec to reject malformed handshakes (the `H2Handshake` error is the typed surface).
- SPEC §6 local signpost 21: `BuildOutcome::Proxy` arm returns 502 with a generic body (no cluster names; defense-in-depth); 05.3 replaces the stub with the real upstream H2 dispatch.
- Cross-sub-phase architectural rule 2: reuse `envoy_http1::HCMConfig` + `envoy_http1::hcm::build_response` end-to-end.

- [ ] **Step 9.1: Verify `BuildOutcome` and `build_response` visibility in `envoy-http1`.**

Run: `grep -nE 'pub.*enum BuildOutcome|pub.*fn build_response|^enum BuildOutcome|^fn build_response' crates/envoy-http1/src/hcm.rs`
Expected: at sub-phase-05.1 close, `enum BuildOutcome` is `pub(crate)` (currently visible only inside `envoy-http1`'s `hcm` module — verify via the absence of `pub` prefix on lines 311/316). To satisfy cross-sub-phase architectural rule 2, both must be `pub` at the crate level.

If currently `pub(crate)`:

(a) Edit `crates/envoy-http1/src/hcm.rs:311-314`. Change `enum BuildOutcome {` to `pub enum BuildOutcome {`.

(b) Edit `crates/envoy-http1/src/hcm.rs:316`. Change `fn build_response(...)` to `pub fn build_response(...)`.

(c) Edit `crates/envoy-http1/src/lib.rs:26`. Extend the `pub use hcm::{HCM, HCMConfig};` line to include the lifted symbols:

```rust
pub use hcm::{build_response, BuildOutcome, HCM, HCMConfig};
```

If already `pub`, skip (a)/(b)/(c).

- [ ] **Step 9.2: Add `pub mod hcm;` + re-exports to `lib.rs`.**

Edit `crates/envoy-http2/src/lib.rs`. Insert after `pub mod codec;`:

```rust
pub mod hcm;

pub use hcm::{HCM, HCMConfig};
```

- [ ] **Step 9.3: Write the failing tests.**

Create `crates/envoy-http2/src/hcm.rs` with the test module. The tests are extensive (8 cases per SPEC §3 D3); each requires an in-process H2 listener that runs envoy-http2's HCM. Use a small helper to spawn the HCM on an ephemeral port:

```rust
//! HCM ConnectionHandler impl for downstream H2C listeners. See SPEC §3 D3.

#[cfg(test)]
mod tests {
    use super::*;
    use envoy_config::{
        CodecType, DataSource, DirectResponse, HttpConnectionManagerConfig, HttpFilter,
        HttpFilterTypedConfig, Route, RouteAction, RouteConfiguration, RouteMatch, RouterConfig,
        VirtualHost,
    };
    use envoy_http1::HCMConfig as Http1HCMConfig;
    use envoy_listener::ConnectionHandler;
    use std::sync::Arc;

    /// Build a minimal HCM config with a single VH + single direct_response
    /// route (status 200, body "ok\n"). Used by most tests below.
    fn synth_h2_hcm_config() -> Arc<Http1HCMConfig> {
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            route_config: RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![VirtualHost {
                    name: "vh".to_string(),
                    domains: vec!["*".to_string()],
                    routes: vec![Route {
                        r#match: RouteMatch {
                            prefix: Some("/".to_string()),
                            path: None,
                            headers: vec![],
                        },
                        action: RouteAction::DirectResponse(DirectResponse {
                            status: 200,
                            body: DataSource {
                                filename: None,
                                inline_string: Some("ok\n".to_string()),
                            },
                        }),
                    }],
                }],
            },
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        Arc::new(
            Http1HCMConfig::from_config(&cfg, cluster_mgr).expect("build HCM config"),
        )
    }

    /// Spawn an HCM on an ephemeral port; return the bound addr + a JoinHandle
    /// that owns the HCM-listener task. The handler accepts one connection
    /// then returns; for multi-request tests the helper must be re-spawned.
    async fn spawn_h2_hcm(
        config: Arc<Http1HCMConfig>,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hcm = HCM::new(config);
        let h = tokio::spawn(async move {
            // Accept loop runs the HCM until the test drops the listener.
            loop {
                let (stream, _peer) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let hcm_clone = hcm.clone();
                tokio::spawn(async move {
                    let _ = hcm_clone.handle(stream).await;
                });
            }
        });
        (addr, h)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_handshake_completes_against_in_process_listener() {
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config()).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) =
            h2::client::handshake(tcp).await.expect("handshake");
        tokio::spawn(async move {
            let _ = conn.await;
        });
        // Trivial probe: send a HEADERS-only GET / and expect a response.
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _stream) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.expect("response");
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_get_resolves_to_direct_response_synth() {
        let (addr, _server) = spawn_h2_hcm(synth_h2_hcm_config()).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        assert_eq!(resp.status().as_u16(), 200);
        let mut body = resp.into_body();
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            bytes.extend_from_slice(&chunk);
        }
        assert_eq!(&bytes[..], b"ok\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_authority_header_synthesizes_host_for_route_walk() {
        // Build an HCM config with TWO virtual hosts: one matching "test.example"
        // exactly, one catch-all "*". The matching VH responds with body
        // "specific\n"; the catch-all responds with "ok\n". Drive a request
        // with :authority = test.example; assert the matching VH is selected.
        let cfg = HttpConnectionManagerConfig {
            stat_prefix: "test".to_string(),
            codec_type: CodecType::HTTP2,
            http2_protocol_options: None,
            route_config: RouteConfiguration {
                name: "r".to_string(),
                virtual_hosts: vec![
                    VirtualHost {
                        name: "specific".to_string(),
                        domains: vec!["test.example".to_string()],
                        routes: vec![Route {
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("specific\n".to_string()),
                                },
                            }),
                        }],
                    },
                    VirtualHost {
                        name: "catch_all".to_string(),
                        domains: vec!["*".to_string()],
                        routes: vec![Route {
                            r#match: RouteMatch {
                                prefix: Some("/".to_string()),
                                path: None,
                                headers: vec![],
                            },
                            action: RouteAction::DirectResponse(DirectResponse {
                                status: 200,
                                body: DataSource {
                                    filename: None,
                                    inline_string: Some("ok\n".to_string()),
                                },
                            }),
                        }],
                    },
                ],
            },
            http_filters: vec![HttpFilter {
                name: "envoy.filters.http.router".to_string(),
                typed_config: HttpFilterTypedConfig::Router(RouterConfig {}),
            }],
        };
        let cluster_mgr = Arc::new(envoy_cluster::ClusterManager::empty());
        let config = Arc::new(Http1HCMConfig::from_config(&cfg, cluster_mgr).unwrap());
        let (addr, _server) = spawn_h2_hcm(config).await;
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut send_request, conn) = h2::client::handshake(tcp).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://test.example/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true).unwrap();
        let resp = response_fut.await.unwrap();
        let mut body = resp.into_body();
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk.unwrap();
            bytes.extend_from_slice(&chunk);
        }
        assert_eq!(
            &bytes[..],
            b"specific\n",
            ":authority test.example must select the specific VH not the catch-all"
        );
    }

    // Tests 4-8 follow the same pattern. Each test name + assertion is
    // listed below; the test bodies mirror tests 1-3's shape (spawn HCM,
    // drive H2 client, assert response).
    //
    // 4. h2_two_requests_share_one_tcp_connection — open one h2 client,
    //    send two GET / requests on different stream IDs, assert both 200.
    // 5. h2_response_strips_hop_by_hop_headers_defensively — config emits
    //    a route response with `connection: close` and `keep-alive`
    //    header values; assert client-side response.headers() omits both.
    // 6. h2_proxy_outcome_returns_502_in_05_2 — config has a Route action
    //    pointing at cluster "backend"; cluster_mgr has no clusters; HCM's
    //    Proxy arm should return 502 (the 05.2 stub). Will be replaced in
    //    05.3 D13.3 with the real upstream dispatch.
    // 7. h2_handshake_fails_on_garbage_preamble — open raw TCP, write
    //    `b"GET / HTTP/1.1\r\n\r\n"`, assert the connection is closed
    //    with a typed handshake error from the HCM driver (peer-side
    //    observation: the read returns 0 / RST).
    // 8. h2_protocol_options_max_concurrent_streams_applied — config has
    //    Http2ProtocolOptions { max_concurrent_streams: Some(1), .. };
    //    open h2 client, send 2 GET / requests concurrently, assert the
    //    second is refused at the SETTINGS frame level (or queued
    //    behind the first; assertion shape depends on h2-crate semantics
    //    which the planner verifies at task time).
    //
    // Each of tests 4-8 follows the same scaffolding as tests 1-3:
    // spawn_h2_hcm + h2::client::handshake + send_request + assertion.
    // For brevity at PLAN-write time, the bodies are listed inline as
    // PLAN-time stubs and elaborated by the executor at Task 9 time
    // following the 1-3 pattern verbatim.

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_two_requests_share_one_tcp_connection() {
        // [Body mirrors tests 1-3's shape; both requests assert status 200
        // and body "ok\n"; the assertion that the connection is reused is
        // observable as both responses returning successfully without a
        // reconnect.]
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_response_strips_hop_by_hop_headers_defensively() {
        // [Build an HCM whose route action emits headers
        // `[("connection", "close"), ("keep-alive", "timeout=5")]`. After
        // the H2 client receives the response, assert the hop-by-hop
        // names are absent from response.headers().]
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_proxy_outcome_returns_502_in_05_2() {
        // [Build an HCM with a RouteAction::Route { cluster: "backend" }
        // and an empty cluster_mgr. Drive a GET; assert response.status()
        // == 502.]
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_handshake_fails_on_garbage_preamble() {
        // [Spawn HCM, connect raw TCP, write
        // b"GET / HTTP/1.1\r\nHost: x\r\n\r\n", read; assert the read
        // returns 0 (peer closed) within a 1-second timeout. The H2 codec
        // rejects the bad preamble at handshake time.]
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn h2_protocol_options_max_concurrent_streams_applied() {
        // [Build an HCM with Http2ProtocolOptions { max_concurrent_streams:
        // Some(1), .. }; open h2 client; observe SETTINGS frame caps the
        // concurrent stream count to 1. Verification shape depends on
        // h2-crate's client-side observability; the planner picks the
        // assertion form at task time. If h2-crate doesn't expose enough
        // to make this test deterministic, the test is converted to
        // #[ignore] with a one-line PROGRESS note.]
    }
}
```

Run: `cargo test -p envoy-http2`
Expected: FAIL with compile error — `HCM` not defined.

- [ ] **Step 9.4: Implement `HCM` + `ConnectionHandler` impl.**

Prepend to `crates/envoy-http2/src/hcm.rs` (above the test module):

```rust
//! HCM ConnectionHandler impl for downstream H2C listeners. See SPEC §3 D3.
//!
//! The HCM consumes envoy_http1::HCMConfig (re-exported as HCMConfig from
//! envoy-http2's lib.rs for ergonomic naming) and dispatches per-stream
//! through envoy_http1::hcm::build_response, identical to the H1 HCM at
//! envoy_http1::HCM. Only the codec layer at the connection edge differs
//! (h2::server vs. Http1Codec). Per cross-sub-phase architectural rule 2.
//!
//! The trait shape (BoxFuture-returning, NOT async-trait) mirrors the
//! envoy-listener trait at crates/envoy-listener/src/lib.rs:29-34. SPEC §6
//! local signpost 19 mandates this — do NOT introduce async-trait ad-hoc.

use crate::codec::build_h2_server;
use crate::error::Http2Error;
use crate::request::http_to_envoy_request;
use crate::response::send_envoy_response;
use bytes::Bytes;
use envoy_http1::{build_response, BuildOutcome, HCMConfig as Http1HCMConfig, Response};
use envoy_listener::{BoxFuture, ConnectionHandler};
use std::sync::Arc;
use tokio::net::TcpStream;

/// Re-export of envoy_http1::HCMConfig under the envoy-http2 namespace.
/// Per cross-sub-phase architectural rule 2 the configuration is identical
/// across H1 and H2; only runtime dispatch differs.
pub type HCMConfig = Http1HCMConfig;

/// HTTP/2 cleartext (H2C prior-knowledge) HCM. Implements
/// `envoy_listener::ConnectionHandler`.
#[derive(Clone)]
pub struct HCM {
    config: Arc<HCMConfig>,
}

impl HCM {
    pub fn new(config: Arc<HCMConfig>) -> Self {
        Self { config }
    }
}

impl ConnectionHandler for HCM {
    fn handle(
        &self,
        downstream: TcpStream,
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        let config = self.config.clone();
        Box::pin(async move {
            serve_h2_connection(config, downstream)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

async fn serve_h2_connection(
    config: Arc<HCMConfig>,
    downstream: TcpStream,
) -> Result<(), Http2Error> {
    // Build the h2-server with optional protocol options. The HCMConfig
    // does not currently carry the Http2ProtocolOptions struct; the planner
    // verifies at Task 9 time whether HCMConfig should be extended to carry
    // the options through, or whether they should be threaded separately.
    // For 05.2 the simplest posture is: pass None at the connection level
    // (use h2-crate defaults). When the HCM-on-H2 dispatch site at
    // envoy-bin (Task 10) wires the actual options, it threads through
    // HCMConfig::from_config which already accepts the full
    // HttpConnectionManagerConfig (including http2_protocol_options).
    //
    // For Task 9 the simplest correct shape is: don't read options here;
    // accept defaults. Task 10 (envoy-bin wiring) decides whether the
    // dispatch site reads HCMConfig.http2_protocol_options OR threads
    // through a separate field. Recommendation: extend HCMConfig with an
    // optional http2_protocol_options field at Task 10 time; for Task 9
    // consume a None.
    let mut h2_conn = build_h2_server(None)
        .handshake(downstream)
        .await
        .map_err(|source| Http2Error::H2Handshake { source })?;

    while let Some(result) = h2_conn.accept().await {
        let (req, send_response) = match result {
            Ok(pair) => pair,
            Err(source) => {
                tracing::warn!(error = ?source, "H2 stream accept failed");
                return Err(Http2Error::H2StreamAccept { source });
            }
        };
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_one_stream(config, req, send_response).await {
                tracing::error!(error = ?e, "H2 stream handler failed");
            }
        });
    }
    Ok(())
}

async fn handle_one_stream(
    config: Arc<HCMConfig>,
    req: http::Request<h2::RecvStream>,
    send_response: h2::server::SendResponse<Bytes>,
) -> Result<(), Http2Error> {
    // Drain the body. For 05.2 fixture 0009 (direct_response) the body is
    // empty; the drain is a no-op. For future fixtures with a body, the
    // unbounded drain is per parent §6 signpost 9 (deferred body-budget
    // posture).
    let (parts, mut body) = req.into_parts();
    let mut body_bytes = bytes::BytesMut::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(|source| Http2Error::H2BodyRead { source })?;
        body_bytes.extend_from_slice(&chunk);
        // Release flow-control window for the chunk.
        body.flow_control()
            .release_capacity(chunk.len())
            .map_err(|source| Http2Error::H2BodyRead { source })?;
    }
    let req_with_body = http::Request::from_parts(parts, body_bytes.freeze());

    // Translate H2 request → envoy Request value-type.
    let envoy_req = http_to_envoy_request(req_with_body)?;

    // Hand to the existing 04.x route-walk. close=false because H2 has its
    // own connection lifecycle; the close flag is only meaningful for H1.
    let outcome = build_response(&config, &envoy_req, /* close = */ false);

    let resp: Response = match outcome {
        BuildOutcome::Synth(r) => r,
        BuildOutcome::Proxy { .. } => {
            // 05.2 STUB: the upstream H2 dispatch lands in 05.3 D13.3.
            // Per SPEC §6 local signpost 21: emit a generic 502 with a
            // doctrine-line body; no cluster names or endpoint addresses.
            tracing::warn!(
                "H2 BuildOutcome::Proxy reached at sub-phase 05.2 — upstream H2 dispatch \
                 not yet wired (lands in 05.3); responding 502 Bad Gateway"
            );
            Response {
                status: 502,
                headers: vec![
                    ("server".to_string(), "envoy-rust".to_string()),
                    ("content-type".to_string(), "text/plain".to_string()),
                ],
                body: Bytes::from_static(b"upstream H2 not yet wired (sub-phase 05.3)\n"),
            }
        }
    };

    send_envoy_response(send_response, resp).await
}
```

**Note on `BuildOutcome` variants.** The `BuildOutcome` enum at `crates/envoy-http1/src/hcm.rs:311-314` has 2 variants in 04.x: `Synth(Response)` and `Proxy { cluster: String }`. The SPEC §3 D3 step 5 refers to a 3rd `Reject(Response)` variant — verify at task time. **If only 2 variants exist**, treat `Reject(Response)` as folded into `Synth(Response)` (the 4xx/5xx synth path is already inside `Synth`). If a 3rd variant exists, add the matching arm. Confirmed at PLAN-write time: only 2 variants in `crates/envoy-http1/src/hcm.rs:311-314`; the SPEC's 3-variant reference is anticipatory.

**Note on `HCMConfig` extension.** The current `envoy_http1::HCMConfig` struct at `crates/envoy-http1/src/hcm.rs:28-32` has 3 fields (`stat_prefix`, `route_config`, `cluster_mgr`) — it does NOT carry `http2_protocol_options`. For 05.2's H2 HCM to use protocol options, either: (a) extend `HCMConfig` with `http2_protocol_options: Option<Http2ProtocolOptions>` (additive; the existing 04.x H1 path ignores it); or (b) pass options separately at the dispatch site. **Recommended posture:** extend `HCMConfig` at Task 9 alongside the H2 HCM landing. The extension adds `pub http2_protocol_options: Option<Http2ProtocolOptions>` as a 4th field on the 04.x struct + extends `HCMConfig::from_config` to populate it from the input `HttpConnectionManagerConfig.http2_protocol_options`. This keeps the dispatch site in envoy-bin (Task 10) clean — it constructs HCMConfig once and passes it to either H1 or H2 HCM via `Arc`.

Make this extension at Task 9 time:

(d) Edit `crates/envoy-http1/src/hcm.rs:28-32`:

```rust
#[derive(Debug)]
pub struct HCMConfig {
    pub stat_prefix: String,
    pub route_config: Arc<RouteConfiguration>,
    pub cluster_mgr: Arc<envoy_cluster::ClusterManager>,
    /// 05.2 NEW: listener-side HTTP/2 protocol options. Ignored on the H1
    /// dispatch path (envoy-http1's HCM doesn't read this); consumed on the
    /// H2 dispatch path (envoy-http2's HCM reads it at handshake time).
    pub http2_protocol_options: Option<envoy_config::Http2ProtocolOptions>,
}
```

(e) Edit `crates/envoy-http1/src/hcm.rs::HCMConfig::from_config` to populate the new field from `cfg.http2_protocol_options.clone()`. The `Http2ProtocolOptions` type needs `Clone` derived; confirm `#[derive(Debug, Default, Deserialize, PartialEq, Clone)]` was added at Task 3 (if not, add `Clone` now).

Then update the H2 HCM's `serve_h2_connection` to pass `config.http2_protocol_options.as_ref()` to `build_h2_server`.

- [ ] **Step 9.5: Run the tests to verify they pass.**

Run: `cargo test -p envoy-http2 -- --nocapture`
Expected: 8 PASS (the 8 hcm.rs tests + the 1 codec test + 2 error + 2 request + 2 response = 15 total). Some tests may be `#[ignore]`-marked at task time if h2-crate observability is insufficient — record in PROGRESS Task 9.

- [ ] **Step 9.6: Run the workspace verification cascade.**

Run: `cargo build --workspace --all-targets`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Run: `cargo test --workspace`
Expected: clean across the board; `cargo test --workspace` shows the new envoy-http2 tests + the unchanged 339 baseline tests from 05.4 close.

- [ ] **Step 9.7: Update PROGRESS Task 9 + commit.**

```bash
git add crates/envoy-http2/src/hcm.rs \
        crates/envoy-http2/src/lib.rs \
        crates/envoy-http1/src/hcm.rs \
        crates/envoy-http1/src/lib.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-http2::HCM ConnectionHandler impl (task 9)

HCM-on-H2 dispatch: per-connection h2::server::handshake + per-stream
tokio::spawn task running through the existing 04.x route-walk +
build_response + BuildOutcome dispatch. BuildOutcome::Proxy stubbed
with a 502 (real upstream H2 lands in 05.3 D13.3). Trait shape mirrors
envoy-listener's BoxFuture posture (NOT async-trait) per SPEC §6 local
signpost 19.

envoy-http1's BuildOutcome enum + build_response fn lifted from
pub(crate) to pub for cross-crate consumption; envoy-http1::HCMConfig
extended with http2_protocol_options field. 8 unit tests cover
handshake, direct_response synth, :authority → Host: synthesis,
multi-stream connection reuse, hop-by-hop strip, Proxy 502 stub,
malformed-preamble rejection, max_concurrent_streams enforcement.

D3 hcm.rs per phase 05.2 SPEC §3."
```

---

## Task 10 — `envoy-bin` HCM-on-H2 wiring + in-process integration test

**Files:**

- Modify: `crates/envoy-bin/Cargo.toml` — add `envoy-http2 = { path = "../envoy-http2" }` to `[dependencies]` + `h2 = "0.4"` to `[dev-dependencies]`.
- Modify: `crates/envoy-bin/src/main.rs` — extend the `HCM_FILTER` arm at line 207 with H1-vs-H2 dispatch on `hcm_cfg.codec_type`.
- Create: `crates/envoy-bin/tests/http2_direct_response.rs` — in-process integration test (sibling of 04.1's `http1_direct_response.rs`).

**Estimated LoC:** ~160 (envoy-bin/main.rs delta ~30; integration test ~120; Cargo.toml ~5; doctrine narration ~5).

**Signposts settled:**

- Parent §6 signpost 22 (HCMConfig polymorphism over codec): dispatch lives at the listener-walk site in `envoy-bin/src/main.rs:207` HCM arm.
- SPEC §6 local signpost 18 (in-process integration backstops): `crates/envoy-bin/tests/http2_direct_response.rs` lands here.
- SPEC §6 local signpost 22 (integration-test cleanup): `kill_on_drop(true)` posture on the spawned envoy-bin Child.

- [ ] **Step 10.1: Update `crates/envoy-bin/Cargo.toml`.**

Add `envoy-http2 = { path = "../envoy-http2" }` to `[dependencies]` (alphabetic insertion between `envoy-http1` and `envoy-listener`). Add `h2 = "0.4"` to `[dev-dependencies]` for the integration test consumer.

- [ ] **Step 10.2: Extend the HCM dispatch site at `crates/envoy-bin/src/main.rs:207`.**

The current code at lines 207–259 reads (relevant slice):

```rust
            envoy_config::HCM_FILTER => {
                let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm_cfg)) =
                    filter.typed_config.as_ref()
                else { /* ... */ };

                let hcm_config = std::sync::Arc::new(envoy_http1::HCMConfig::from_config(
                    hcm_cfg,
                    std::sync::Arc::clone(&cluster_mgr),
                )?);
                let hcm: std::sync::Arc<dyn envoy_listener::ConnectionHandler> =
                    std::sync::Arc::new(envoy_http1::HCM { config: hcm_config });

                if build_downstream_tls_for_listener(listener_cfg)?.is_some() {
                    anyhow::bail!(/* H1+TLS bail */);
                }
                /* ... bind listener ... */
            }
```

Replace with H1-vs-H2 dispatch on `hcm_cfg.codec_type`:

```rust
            envoy_config::HCM_FILTER => {
                let Some(envoy_config::TypedConfig::HttpConnectionManager(hcm_cfg)) =
                    filter.typed_config.as_ref()
                else {
                    anyhow::bail!(
                        "filter '{}' missing typed_config; envoy-config validator should have rejected at parse time",
                        envoy_config::HCM_FILTER,
                    );
                };

                let hcm_config = std::sync::Arc::new(envoy_http1::HCMConfig::from_config(
                    hcm_cfg,
                    std::sync::Arc::clone(&cluster_mgr),
                )?);

                // 05.2 NEW: H1-vs-H2 dispatch on hcm_cfg.codec_type.
                // - AUTO / HTTP1 → envoy_http1::HCM (existing 04.x path)
                // - HTTP2       → envoy_http2::HCM (new in 05.2)
                // - HTTP3       → unreachable (validator rejected at parse time)
                let hcm: std::sync::Arc<dyn envoy_listener::ConnectionHandler> = match hcm_cfg.codec_type {
                    envoy_config::CodecType::AUTO | envoy_config::CodecType::HTTP1 => {
                        std::sync::Arc::new(envoy_http1::HCM { config: hcm_config })
                    }
                    envoy_config::CodecType::HTTP2 => {
                        std::sync::Arc::new(envoy_http2::HCM::new(hcm_config))
                    }
                    envoy_config::CodecType::HTTP3 => {
                        unreachable!("CodecType::HTTP3 rejected by validator at parse time");
                    }
                };

                // TLS-detect-and-bail: only meaningful for the H1 path.
                // For H2 the validator already rejected TLS+HTTP2 at parse
                // time (Http2OverTlsNotSupported) so this branch is
                // unreachable for H2.
                if matches!(
                    hcm_cfg.codec_type,
                    envoy_config::CodecType::AUTO | envoy_config::CodecType::HTTP1
                ) && build_downstream_tls_for_listener(listener_cfg)?.is_some()
                {
                    anyhow::bail!(
                        "HCM listener with downstream TLS is not supported in phase 04.x; \
                         TlsAcceptingHandler is currently TcpProxy-only and will be \
                         generalized in phase 05+ (SPEC §3 D4)",
                    );
                }
                let handler: std::sync::Arc<dyn envoy_listener::ConnectionHandler> = hcm;

                let listener = envoy_listener::Listener::bind(listener_cfg, handler)
                    .await
                    .with_context(|| format!("binding HCM listener to {bind_addr}"))?;
                tracing::info!(
                    addr = %bind_addr,
                    stat_prefix = %hcm_cfg.stat_prefix,
                    codec_type = ?hcm_cfg.codec_type,
                    "envoy-rust listening (http_connection_manager)",
                );
                let shutdown = token.clone();
                set.spawn(async move {
                    listener
                        .serve(async move { shutdown.cancelled().await })
                        .await
                        .map_err(|e| anyhow::anyhow!(e))
                });
            }
```

The `envoy_http2::HCM::new(hcm_config)` constructor consumes the same `Arc<envoy_http1::HCMConfig>` (because `envoy_http2::HCMConfig` is a type alias per Task 9 lib.rs).

- [ ] **Step 10.3: Write the failing in-process integration test.**

Create `crates/envoy-bin/tests/http2_direct_response.rs`. Mirror the shape of `crates/envoy-bin/tests/http1_direct_response.rs` (read at PLAN-write time; ~150 lines). Substitute `codec_type: HTTP2` for `codec_type: HTTP1` in the YAML and use `h2::client::handshake` + `send_request` instead of raw HTTP/1.1 byte writes:

```rust
//! Phase 05.2 envoy-bin integration test: spawn `envoy-bin` against a minimal
//! HCM-direct_response config with codec_type: HTTP2, send a single H2C GET
//! via h2::client, and assert response shape (status 200, body "ok\n").
//! No Docker.
//!
//! This is the envoy-rust-only backstop so a regression in HCM-on-H2 wiring
//! shows up locally without Docker. The Docker-gated differential test in
//! `tests/differential/tests/http2_direct_response.rs` (Task 12) is the full
//! equivalence gate against upstream Envoy.
//!
//! Mirrors the binary-locate + retry-loop shape from
//! `crates/envoy-bin/tests/http1_direct_response.rs`.

use std::io::Write;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::process::Stdio;
use std::time::Duration;

use tokio::net::TcpStream;

fn reserve_port() -> u16 {
    let l = StdListener::bind(("127.0.0.1", 0)).unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

async fn wait_ready(addr: SocketAddr, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    let mut delay = Duration::from_millis(50);
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return,
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(500));
            }
            Err(e) => panic!("listener never became ready at {addr}: {e}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn http2_direct_response_round_trip() {
    let listener_port = reserve_port();
    let yaml = format!(
        r#"
node:
  id: x
  cluster: y
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address:
          address: 127.0.0.1
          port_value: {listener_port}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: default
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response:
                            status: 200
                            body: {{ inline_string: "ok\n" }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
    );

    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::File::create(&cfg)
        .unwrap()
        .write_all(yaml.as_bytes())
        .unwrap();

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn envoy-bin");

    let listener_addr: SocketAddr = format!("127.0.0.1:{listener_port}").parse().unwrap();
    wait_ready(listener_addr, Duration::from_secs(5)).await;

    let outcome = async {
        let tcp = TcpStream::connect(listener_addr).await?;
        let (mut send_request, conn) = h2::client::handshake(tcp).await?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        let req = http::Request::builder()
            .method("GET")
            .uri("http://envoy-rust.test/")
            .body(())
            .unwrap();
        let (response_fut, _) = send_request.send_request(req, true)?;
        let resp = response_fut.await?;
        let status = resp.status().as_u16();
        let mut body = resp.into_body();
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body.data().await {
            let chunk = chunk?;
            bytes.extend_from_slice(&chunk);
        }
        Ok::<_, anyhow::Error>((status, bytes.freeze()))
    }
    .await;

    drop(child); // SIGKILL via kill_on_drop(true).

    let (status, body) = outcome.expect("H2 round-trip");
    assert_eq!(status, 200);
    assert_eq!(&body[..], b"ok\n");
}
```

Add `tempfile` and `bytes` and `anyhow` to `crates/envoy-bin/Cargo.toml` `[dev-dependencies]` if not already present (verify at task time via `grep -A 20 '\[dev-dependencies\]' crates/envoy-bin/Cargo.toml`).

- [ ] **Step 10.4: Run the test to verify it passes.**

Run: `cargo test -p envoy-bin --test http2_direct_response -- --nocapture`
Expected: PASS. The envoy-bin subprocess starts, h2::client handshakes, GET / returns 200 + "ok\n".

If the test FAILS, common causes: (a) the `h2` crate API differs (e.g., `send_request` signature changed in 0.4.x); fix per the actual API. (b) the HCM dispatch site at main.rs has a bug — re-check the match arm. (c) the listener bind didn't bind to the expected port — check the spawned envoy-bin's stderr.

- [ ] **Step 10.5: Run the full workspace verification cascade.**

Run: `cargo build --workspace --all-targets`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Run: `cargo test --workspace`
Expected: clean.

- [ ] **Step 10.6: Update PROGRESS Task 10 + commit.**

```bash
git add crates/envoy-bin/Cargo.toml \
        crates/envoy-bin/src/main.rs \
        crates/envoy-bin/tests/http2_direct_response.rs \
        Cargo.lock \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: envoy-bin HCM-on-H2 dispatch wiring + integration test (task 10)

HCM_FILTER arm at main.rs:207 gains H1-vs-H2 dispatch on
hcm_cfg.codec_type. AUTO/HTTP1 continue through envoy_http1::HCM;
HTTP2 routes to envoy_http2::HCM::new(hcm_config). HTTP3 unreachable
(validator rejected at parse time). TLS-detect-and-bail bypassed for
H2 (validator's Http2OverTlsNotSupported handles that combination).

In-process integration test crates/envoy-bin/tests/
http2_direct_response.rs spawns envoy-bin via CARGO_BIN_EXE_envoy-bin,
drives a GET via h2::client, asserts status 200 + body 'ok\n'.

D4 per phase 05.2 SPEC §3."
```

---

## Task 11 — Differential harness `Driver::Http2` + `drive_http2` + `run_fixture` dispatch arm + 1 unit test

**Files:**

- Modify: `tests/differential/Cargo.toml` — add direct `h2 = "0.4"` dep (carve-out per parent §6 signpost 8).
- Modify: `tests/differential/src/lib.rs` — extend `Driver` enum with `Http2 { method, path, host, expected_status, expected_body, expected_headers }`; extend `port_key` match in `run_fixture` (line ~840); add `pub async fn drive_http2`; extend the per-driver dispatch cascade with the `Driver::Http2` arm; append 1 harness unit test.

**Estimated LoC:** ~170 (Cargo.toml +1 + Driver variant ~12 + drive_http2 ~80 + run_fixture dispatch arm ~40 + unit test ~30 + carryforward narration in PROGRESS ~5).

**Signposts settled:**

- Parent §6 signpost 8 (drive_http2 carve-out): `tests/differential/Cargo.toml` gains direct `h2 = "0.4"` dep; documented as carve-out from cross-sub-phase architectural rule 1 (parallel to phase-04.1 REVIEW M-architectural-claim's `httparse` posture).

- [ ] **Step 11.1: Add `h2 = "0.4"` to `tests/differential/Cargo.toml`.**

Edit `tests/differential/Cargo.toml`. Insert into `[dependencies]` (alphabetic insertion between existing `httparse = "1"` and `rcgen = "0.13"` lines):

```toml
h2 = "0.4"
```

- [ ] **Step 11.2: Write the failing harness unit test.**

Append to `tests/differential/src/lib.rs::tests` (the existing `#[cfg(test)] mod tests` block at the bottom of the file):

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn drive_http2_round_trip_against_in_process_listener() {
        // Spawn envoy-bin as a subprocess against an HCM HTTP2 direct_response
        // config; drive a GET via drive_http2; assert the returned tuple
        // matches expectations.
        let port = reserve_port().unwrap();
        let yaml = format!(
            r#"
node: {{ id: x, cluster: y }}
static_resources:
  listeners:
    - name: hcm_listener
      address:
        socket_address: {{ address: 127.0.0.1, port_value: {port} }}
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: {{ prefix: "/" }}
                          direct_response: {{ status: 200, body: {{ inline_string: "ok\n" }} }}
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
"#
        );
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("envoy-rust.yaml");
        std::fs::write(&cfg, yaml).unwrap();

        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_envoy-bin"))
            .arg("-c")
            .arg(&cfg)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn envoy-bin");

        // Wait for listener readiness.
        let listener_addr: std::net::SocketAddr =
            format!("127.0.0.1:{port}").parse().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if tokio::net::TcpStream::connect(listener_addr).await.is_ok() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                drop(child);
                panic!("listener never became ready");
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let result = drive_http2(
            listener_addr,
            &Http1Method::Get,
            "/",
            "test.example",
            &[],
        )
        .await;
        drop(child);
        let result = result.expect("drive_http2 returns Ok");
        assert_eq!(result.status, 200);
        assert_eq!(&result.body[..], b"ok\n");
    }
```

Run: `cargo test -p differential drive_http2_round_trip -- --nocapture`
Expected: FAIL with compile error — `drive_http2` not defined.

- [ ] **Step 11.3: Implement `drive_http2`.**

Append to `tests/differential/src/lib.rs` (after `drive_http1` at line 779-area):

```rust
/// Drive an HTTP/2 cleartext (H2C prior-knowledge) request against the given
/// listener address. Mirrors `drive_http1`'s shape so `assert_equivalence`'s
/// `diff_headers` works without modification. Per parent-05 SPEC §6 signpost 8
/// this helper consumes `h2 = "0.4"` directly — the documented carve-out from
/// cross-sub-phase architectural rule 1, parallel to phase-04.1 REVIEW
/// M-architectural-claim's `httparse` posture for `drive_http1`.
pub async fn drive_http2(
    addr: SocketAddr,
    method: &Http1Method,
    path: &str,
    host: &str,
    extra_headers: &[(String, String)],
) -> Result<DriveHttp1Result> {
    use tokio::net::TcpStream;

    let tcp = TcpStream::connect(addr)
        .await
        .with_context(|| format!("connecting to {addr}"))?;
    let (mut send_request, conn) = h2::client::handshake(tcp)
        .await
        .context("H2 handshake")?;

    // Drive the connection in the background.
    let conn_handle = tokio::spawn(async move {
        let _ = conn.await;
    });

    // Build the request. Use absolute-form URI so :authority is populated.
    let uri: http::Uri = format!("http://{host}{path}").parse().context("URI parse")?;
    let mut builder = http::Request::builder()
        .method(method.as_str())
        .uri(uri);
    for (n, v) in extra_headers {
        builder = builder.header(n.as_str(), v.as_str());
    }
    let req = builder.body(()).context("request build")?;

    // Send the request with end_of_stream=true (no body for GET).
    let (response_fut, _send_stream) = send_request
        .send_request(req, true)
        .context("H2 send_request")?;

    let resp = response_fut.await.context("H2 response")?;
    let status = resp.status().as_u16();
    let header_map = resp.headers().clone();
    let mut body_stream = resp.into_body();

    let mut body = Vec::new();
    while let Some(chunk) = body_stream.data().await {
        let chunk = chunk.context("H2 body data")?;
        body.extend_from_slice(&chunk);
        // Release flow-control window.
        body_stream
            .flow_control()
            .release_capacity(chunk.len())
            .ok();
    }

    let headers: Vec<(String, String)> = header_map
        .iter()
        .map(|(n, v)| {
            (
                n.as_str().to_string(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    drop(send_request);
    let _ = conn_handle.await;

    Ok(DriveHttp1Result {
        status,
        headers,
        body,
    })
}
```

- [ ] **Step 11.4: Add the `Driver::Http2` variant.**

Edit `tests/differential/src/lib.rs::Driver` enum at lines 38–83. Append after the existing `Http1ProbeList { ... }` arm:

```rust
    /// 05.2 NEW: drive an HTTP/2 cleartext (H2C prior-knowledge) request and
    /// assert the response shape. Mirrors `Http1`'s shape; the `host` field
    /// becomes `:authority` on the H2 wire. Per SPEC §3 D5.
    Http2 {
        method: Http1Method,
        path: String,
        host: String,
        #[serde(default)]
        expected_status: Option<u16>,
        #[serde(default)]
        expected_body: Option<Http1BodyRule>,
        #[serde(default)]
        expected_headers: Option<Http1HeaderRule>,
    },
```

- [ ] **Step 11.5: Extend the `port_key` match in `run_fixture`.**

Edit `tests/differential/src/lib.rs:835-842`. The existing match arm:

```rust
    let port_key = match &expectations.driver {
        Driver::TcpEcho
        | Driver::TlsTcp { .. }
        | Driver::TlsTcpProbeList { .. }
        | Driver::Http1 { .. }
        | Driver::Http1ProbeList { .. } => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    };
```

Add `Driver::Http2 { .. }` to the `"PORT"` arm:

```rust
    let port_key = match &expectations.driver {
        Driver::TcpEcho
        | Driver::TlsTcp { .. }
        | Driver::TlsTcpProbeList { .. }
        | Driver::Http1 { .. }
        | Driver::Http1ProbeList { .. }
        | Driver::Http2 { .. } => "PORT",
        Driver::HttpGet { .. } => "ADMIN_PORT",
    };
```

- [ ] **Step 11.6: Add the `Driver::Http2` arm to the per-driver dispatch cascade.**

Edit `tests/differential/src/lib.rs` in the `run_fixture` body (the per-driver match cascade starting at line 1007 with `Driver::TcpEcho`). Append after the `Driver::Http1ProbeList` arm:

```rust
        Driver::Http2 {
            method,
            path,
            host,
            expected_status,
            expected_body,
            expected_headers,
        } => {
            // 05.2 NEW: H2 dispatch arm. Mirrors Driver::Http1 in shape.
            let upstream_addr = upstream::host_addr_for_listener(&upstream_runtime);
            let subject_addr: SocketAddr = format!("127.0.0.1:{host_port}").parse()?;

            let upstream_result = drive_http2(upstream_addr, method, path, host, &[]).await
                .context("drive_http2 against upstream Envoy")?;
            let subject_result = drive_http2(subject_addr, method, path, host, &[]).await
                .context("drive_http2 against envoy-rust subject")?;

            // Per-axis equivalence assertions (mirror Http1 arm at line 1114).
            if let Some(expected) = expected_status {
                assert_eq!(upstream_result.status, *expected, "upstream status");
                assert_eq!(subject_result.status, *expected, "subject status");
            } else {
                assert_eq!(upstream_result.status, subject_result.status,
                    "upstream vs subject status divergence");
            }

            match expected_body {
                Some(Http1BodyRule::ByteExact { body }) => {
                    assert_eq!(upstream_result.body, body.as_bytes(), "upstream body");
                    assert_eq!(subject_result.body, body.as_bytes(), "subject body");
                }
                None => {
                    assert_eq!(upstream_result.body, subject_result.body,
                        "upstream vs subject body divergence");
                }
            }

            match expected_headers {
                Some(Http1HeaderRule::SetEqualModuloAllowList) | None => {
                    diff_headers(
                        &upstream_result.headers,
                        &subject_result.headers,
                        HEADER_ALLOW_LIST,
                    )?;
                }
            }
        }
```

The exact shape of the upstream / subject dispatch should mirror the existing `Driver::Http1` arm at line 1114; verify the helper names + `assert_equivalence`-style invocation at task time. The above is the minimal correct shape.

- [ ] **Step 11.7: Run the harness unit test.**

Run: `cargo test -p differential drive_http2_round_trip -- --nocapture`
Expected: PASS.

- [ ] **Step 11.8: Run the workspace verification cascade.**

Run: `cargo build --workspace --all-targets`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Run: `cargo test --workspace`
Expected: clean.

- [ ] **Step 11.9: Update PROGRESS Task 11 + commit.**

```bash
git add tests/differential/Cargo.toml \
        tests/differential/src/lib.rs \
        Cargo.lock \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: harness Driver::Http2 + drive_http2 + dispatch arm (task 11)

tests/differential/Cargo.toml gains direct h2 = '0.4' dep — the
documented carve-out from cross-sub-phase architectural rule 1 per
parent-05 SPEC §6 signpost 8 (parallel to phase-04.1 REVIEW
M-architectural-claim's httparse posture for drive_http1).

Driver enum gains Http2 variant. drive_http2 helper opens TCP, runs
h2::client::handshake, sends the request, drains the response into
DriveHttp1Result (shape-compatible with diff_headers). run_fixture's
port_key match + per-driver dispatch cascade extended with the H2 arm.

D5 per phase 05.2 SPEC §3."
```

---

## Task 12 — Fixture `0009-http2-direct-response/` + Docker-gated test wrapper

**Files:**

- Create: `tests/fixtures/0009-http2-direct-response/envoy.yaml`
- Create: `tests/fixtures/0009-http2-direct-response/envoy-rust.yaml`
- Create: `tests/fixtures/0009-http2-direct-response/inputs/payload.bin` (empty file).
- Create: `tests/fixtures/0009-http2-direct-response/expectations.yaml`
- Create: `tests/fixtures/0009-http2-direct-response/README.md`
- Create: `tests/differential/tests/http2_direct_response.rs`

**Estimated LoC:** ~130 (envoy.yaml ~40 + envoy-rust.yaml ~38 + payload.bin 0 + expectations.yaml ~10 + README.md ~30 + Docker wrapper test ~10).

**Signposts settled:**

- Parent §6 signpost 20 (Phase-04 fixture YAMLs precedent): mirror fixture 0007's shape. Only `codec_type: HTTP1` → `codec_type: HTTP2` differs.

- [ ] **Step 12.1: Create `tests/fixtures/0009-http2-direct-response/envoy.yaml`.**

```yaml
node: { id: envoy-rust-phase-05.2-fixture-0009, cluster: envoy-rust-phase-05.2 }
admin: { address: { socket_address: { address: 0.0.0.0, port_value: 0 } } }
static_resources:
  listeners:
    - name: http2_listener
      address:
        socket_address: { address: 0.0.0.0, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                generate_request_id: false
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 12.2: Create `tests/fixtures/0009-http2-direct-response/envoy-rust.yaml`.**

Mirror envoy.yaml with the per-side divergences (bind 127.0.0.1, no admin block, no `generate_request_id` field):

```yaml
node: { id: envoy-rust-phase-05.2-fixture-0009, cluster: envoy-rust-phase-05.2 }
static_resources:
  listeners:
    - name: http2_listener
      address:
        socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_http2
                codec_type: HTTP2
                route_config:
                  name: local_route
                  virtual_hosts:
                    - name: backend_vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response:
                            status: 200
                            body: { inline_string: "ok\n" }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 12.3: Create `tests/fixtures/0009-http2-direct-response/inputs/payload.bin` as an empty file.**

```bash
mkdir -p tests/fixtures/0009-http2-direct-response/inputs
: > tests/fixtures/0009-http2-direct-response/inputs/payload.bin
```

- [ ] **Step 12.4: Create `tests/fixtures/0009-http2-direct-response/expectations.yaml`.**

```yaml
driver:
  kind: http2
  method: get
  path: "/"
  host: envoy-rust.test
  expected_status: 200
  expected_body:
    kind: byte_exact
    body: "ok\n"
  expected_headers: set_equal_modulo_allow_list
equivalence:
  response_status: exact
  response_body: byte_exact
```

- [ ] **Step 12.5: Create `tests/fixtures/0009-http2-direct-response/README.md`.**

```markdown
# Fixture 0009 — HTTP/2 cleartext (H2C) direct_response

Phase 05.2 differential fixture. The first H2C surface in the project's history.

## Surface

- Listener bound on a single TCP port (`{{PORT}}` substituted by the harness).
- HCM filter chain with `codec_type: HTTP2`.
- Single virtual host `domains: ["*"]`.
- Single route `prefix: "/"` with action `direct_response { status: 200, body: { inline_string: "ok\n" } }`.
- No upstream cluster (`clusters: []`).

## What this fixture exercises

- HCM-on-H2 dispatch path in `envoy-bin/src/main.rs:207` (Task 10).
- `envoy-http2::HCM::handle` per-connection driver (Task 9).
- H2 prior-knowledge handshake via `h2::server::handshake`.
- Per-stream `tokio::spawn` task running through the existing 04.x route-walk
  + `BuildOutcome::Synth` arm (the `direct_response` happy path).
- `:authority` → `Host:` synthesis in `request.rs::http_to_envoy_request` (the
  driver's `host: envoy-rust.test` becomes the H2 `:authority` pseudo-header
  which becomes the synthesized `Host:` row that the route-walk reads).

## Cross-references

- Phase 05.2 SPEC §3 D6: `docs/envoy-rust/phases/05.2-http2-downstream/SPEC.md`.
- Architectural rules: parent-05 SPEC §3 cross-sub-phase rules 1–7.
- Sibling H1 fixture (same shape, different codec): `tests/fixtures/0007-http1-direct-response/`.
```

- [ ] **Step 12.6: Create the Docker-gated test wrapper.**

Create `tests/differential/tests/http2_direct_response.rs`:

```rust
//! Phase 05.2 differential acceptance test: drive an H2C GET / through an
//! HCM-direct_response listener with codec_type: HTTP2. Should produce
//! identical (status, body, header-set-modulo-allow-list) between upstream
//! Envoy v1.33.0 and envoy-rust. Docker-gated; in CI this runs on
//! `ubuntu-latest` alongside the phase-00 echo, phase-01 admin_ready, phase-
//! 02.2 tcp_proxy, phase-03 tls_*, and phase-04 http1_* fixtures.

use std::path::PathBuf;

#[tokio::test]
async fn http2_direct_response_fixture() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests/fixtures/0009-http2-direct-response");
    differential::run_fixture(&dir)
        .await
        .expect("fixture passes");
}
```

- [ ] **Step 12.7: Verify the fixture YAMLs parse via the in-process integration backstop.**

The Docker-gated test requires `docker` to actually run upstream Envoy. Skip the Docker run at PLAN-execution time if unavailable (the CI step at Task 14 covers it). The in-process integration test from Task 10 (`crates/envoy-bin/tests/http2_direct_response.rs`) already exercises the envoy-rust side end-to-end without Docker — re-run it to confirm fixture 0009's YAML shape is mechanically valid:

Run: `cargo test -p envoy-bin --test http2_direct_response`
Expected: PASS.

- [ ] **Step 12.8: Update PROGRESS Task 12 + commit.**

```bash
git add tests/fixtures/0009-http2-direct-response/ \
        tests/differential/tests/http2_direct_response.rs \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: fixture 0009-http2-direct-response + Docker-gated wrapper (task 12)

5-file fixture (envoy.yaml + envoy-rust.yaml + inputs/payload.bin
+ expectations.yaml + README.md) mirrors fixture 0007's shape with
codec_type: HTTP2 substituted for HTTP1. Docker-gated test wrapper
is a 7-line tokio::test that calls differential::run_fixture.

D6 per phase 05.2 SPEC §3."
```

---

## Task 13 — `tests/conformance/h2spec/` runner crate scaffold

**Files:**

- Create: `tests/conformance/h2spec/Cargo.toml`
- Create: `tests/conformance/h2spec/src/lib.rs` (with `#![forbid(unsafe_code)]`).
- Create: `tests/conformance/h2spec/tests/h2spec_runner.rs`
- Create: `tests/conformance/h2spec/h2spec.yaml`
- Create: `tests/conformance/h2spec/known-failures.txt`
- Modify: `Cargo.toml` (root) — `[workspace] members` gains `tests/conformance/h2spec`.

**Estimated LoC:** ~370 (Cargo.toml ~30 + lib.rs ~5 + h2spec_runner.rs ~250 + h2spec.yaml ~60 + known-failures.txt ~20 initial entries).

**Signposts settled:**

- Parent §6 signpost 3 (h2spec binary management): provisioning via curl-tar in CI; local `eprintln!`-skip if `which h2spec` fails. The version pin lands at PLAN-execution time (planner reads the latest `summerwind/h2spec` GitHub release at task time).
- Parent §6 signpost 4 (known-failures.txt format): one-line-per-test-id with `# reason`.
- SPEC §7 ADR-0028 (renumbered from SPEC's projected ADR-0025): RECOMMENDED NOT TO LAND. The gate-mechanics are mechanically deterministic per signposts 3-4; recording inline in PROGRESS Task 13 suffices. The decision is recorded inline in PROGRESS Task 13 narrative (no DECISIONS.md edit); ADR-0028 stays available for phase-06+.

- [ ] **Step 13.1: Create `tests/conformance/h2spec/Cargo.toml`.**

```toml
[package]
name = "h2spec-conformance"
version = "0.0.0"
edition = "2024"
publish = false
license = "Apache-2.0"

[lib]
path = "src/lib.rs"

[[test]]
name = "h2spec_runner"
path = "tests/h2spec_runner.rs"

[dependencies]

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "process", "time", "io-util", "net"] }
anyhow = "1"
```

- [ ] **Step 13.2: Create `tests/conformance/h2spec/src/lib.rs`.**

```rust
#![forbid(unsafe_code)]

//! envoy-rust h2spec conformance runner.
//!
//! The crate is a test-only workspace member; the lib is empty (the runner
//! lives in `tests/h2spec_runner.rs`). Per phase 05.2 SPEC §3 D7.
```

- [ ] **Step 13.3: Add `tests/conformance/h2spec` to root `Cargo.toml` `[workspace] members`.**

Edit `Cargo.toml`. Insert in alphabetic order after the existing `tests/differential` member:

```toml
"tests/conformance/h2spec",
```

(The existing root Cargo.toml has 12 members; the new one becomes 13.)

- [ ] **Step 13.4: Create `tests/conformance/h2spec/h2spec.yaml`.**

```yaml
# envoy-rust HCM config for the h2spec conformance target. Minimal HCM with
# codec_type: HTTP2 + a single VH + a single route returning a deterministic
# direct_response. h2spec primarily exercises codec-level behavior, so the
# response payload doesn't matter for test results — what matters is that
# envoy-rust correctly handles every H2 framing scenario h2spec drives.
#
# The {{PORT}} marker is substituted by the runner at test time; no harness
# template substitution is involved (this is a conformance runner, not a
# differential fixture).
node: { id: h2spec-target, cluster: envoy-rust-conformance }
static_resources:
  listeners:
    - name: h2spec_listener
      address:
        socket_address: { address: 127.0.0.1, port_value: {{PORT}} }
      filter_chains:
        - filters:
            - name: envoy.filters.network.http_connection_manager
              typed_config:
                "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
                stat_prefix: ingress_h2spec
                codec_type: HTTP2
                route_config:
                  name: r
                  virtual_hosts:
                    - name: vh
                      domains: ["*"]
                      routes:
                        - match: { prefix: "/" }
                          direct_response: { status: 200, body: { inline_string: "h2spec" } }
                http_filters:
                  - name: envoy.filters.http.router
                    typed_config:
                      "@type": type.googleapis.com/envoy.extensions.filters.http.router.v3.Router
  clusters: []
```

- [ ] **Step 13.5: Create `tests/conformance/h2spec/known-failures.txt` with header + initial entries.**

```
# h2spec known-failures for envoy-rust at phase 05.2.
#
# Each non-comment line is a single h2spec test ID (the dotted-numeric ID
# h2spec emits in its terminal output, e.g., "5.1.1/2") followed optionally
# by spaces and a `# <reason>` doctrine annotation. Failures NOT listed
# here regress the gate. Failures listed here BUT now passing also regress
# the gate (forces lockstep maintenance — when an h2spec test starts
# passing, this file MUST be trimmed in the same commit).
#
# Format: <h2spec-test-id>  # <one-line reason>
#
# Initial population: empty. Populated at Task 13 execution time after
# h2spec is run end-to-end for the first time (planner identifies which
# tests fail, classifies each as "deferred-to-future-phase" /
# "codec-foundation-limitation" / "intentional-Envoy-divergence-per-ADR" /
# "regression-blocker", and lands the file with one annotated entry per
# expected failure). If a failure cannot be classified as deferral or
# foundation limitation, it is a blocker — re-loop into REVIEW.md state 5
# rather than landing it as a known-failure.

# Example entries (replace at task time with the actual failures):
# 5.1.1/2                    # GOAWAY handling deferred to phase-08 graceful drain
# 6.5.3/1                    # SETTINGS_INITIAL_WINDOW_SIZE override deferred per parent SPEC §4
```

- [ ] **Step 13.6: Create `tests/conformance/h2spec/tests/h2spec_runner.rs`.**

```rust
//! h2spec conformance runner. Spawns envoy-bin against an HCM HTTP2 config,
//! runs h2spec via subprocess, parses output, asserts ≥95% pass rate +
//! every failing test enumerated in known-failures.txt.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::process::Command;

const PASS_RATE_GATE: f64 = 0.95;

#[tokio::test(flavor = "multi_thread")]
async fn h2spec_pass_rate_gate() {
    let outcome = run_h2spec_gate().await;
    if let Err(e) = outcome {
        // h2spec binary unavailable → eprintln!-skip per SPEC §3 D7. Don't
        // fail the test locally; CI provisions the binary at Task 14.
        if e.to_string().contains("h2spec not found") {
            eprintln!("h2spec_runner: {} — skipping locally", e);
            return;
        }
        panic!("h2spec gate failed: {e:#}");
    }
}

async fn run_h2spec_gate() -> Result<()> {
    // Locate the h2spec binary.
    let h2spec = locate_h2spec().context("h2spec not found")?;

    // Locate envoy-bin.
    let envoy_bin = env!("CARGO_BIN_EXE_envoy-bin");

    // Reserve a port + render the YAML.
    let port = reserve_port()?;
    let yaml_template = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("h2spec.yaml"),
    )?;
    let yaml = yaml_template.replace("{{PORT}}", &port.to_string());

    let dir = tempfile::tempdir()?;
    let cfg = dir.path().join("envoy-rust.yaml");
    std::fs::write(&cfg, yaml)?;

    // Spawn envoy-bin.
    let mut child = Command::new(envoy_bin)
        .arg("-c")
        .arg(&cfg)
        .env("ENVOY_RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("spawn envoy-bin")?;

    // Wait for accept-readiness.
    let listener_addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(listener_addr).await.is_ok() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("listener never became ready at {listener_addr}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Run h2spec.
    let output = Command::new(&h2spec)
        .args(["-h", "127.0.0.1", "-p", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("run h2spec")?;

    drop(child); // SIGKILL envoy-bin via kill_on_drop.

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    eprintln!("=== h2spec stdout ===\n{stdout}\n=== h2spec stderr ===\n{stderr}");

    // Parse h2spec's summary line. h2spec emits a final summary like:
    //   Tests: NNN
    //   Passed: MM
    //   Skipped: KK
    //   Failed:  LL
    // The exact format may vary by h2spec version; the planner verifies at
    // task time and adjusts the parser. The minimal correct parser greps
    // for "Passed:" and "Failed:" lines and parses the integer.
    let (passed, failed, failures) = parse_h2spec_output(&stdout)?;
    let total = passed + failed;
    let pass_rate = if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    };

    eprintln!(
        "h2spec: passed={passed} failed={failed} total={total} pass_rate={pass_rate:.4}"
    );

    // Read known-failures.txt.
    let kf_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("known-failures.txt");
    let kf_text = std::fs::read_to_string(&kf_path)?;
    let known_failures: std::collections::BTreeSet<String> = kf_text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| l.split('#').next().unwrap().trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    // Gate (a): overall pass rate.
    anyhow::ensure!(
        pass_rate >= PASS_RATE_GATE,
        "h2spec pass rate {pass_rate:.4} below gate {PASS_RATE_GATE}",
    );

    // Gate (b): every failing test must be in known-failures.txt.
    let unexpected: Vec<&String> = failures
        .iter()
        .filter(|t| !known_failures.contains(*t))
        .collect();
    anyhow::ensure!(
        unexpected.is_empty(),
        "h2spec regressed on unlisted tests: {unexpected:?}",
    );

    // Gate (c): every test in known-failures.txt must actually fail.
    let stale: Vec<&String> = known_failures
        .iter()
        .filter(|t| !failures.contains(t))
        .collect();
    anyhow::ensure!(
        stale.is_empty(),
        "known-failures.txt has stale entries (now passing): {stale:?} — trim the file",
    );

    Ok(())
}

fn locate_h2spec() -> Result<PathBuf> {
    // (1) Try `which h2spec` on PATH.
    if let Ok(out) = std::process::Command::new("which").arg("h2spec").output() {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }
    // (2) Try project-internal `tools/h2spec`.
    let project_tool = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("tools")
        .join("h2spec");
    if project_tool.exists() {
        return Ok(project_tool);
    }
    anyhow::bail!("h2spec not found on PATH or in tools/")
}

fn reserve_port() -> Result<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let p = l.local_addr()?.port();
    drop(l);
    Ok(p)
}

/// Parse h2spec's terminal output. Returns (passed, failed, failures) where
/// `failures` is a sorted list of failing test IDs.
///
/// h2spec's actual output format varies by version. The planner verifies at
/// task time by running `h2spec -h 127.0.0.1 -p <port>` against a known-
/// good H2 server and inspecting the output. The parser below assumes:
///   - A line `Failed: N tests` carries the failed count.
///   - Failed test IDs appear inline as `× <test-id>` markers.
///   - Passed test IDs appear inline as `✓ <test-id>` markers.
/// If the actual h2spec format differs, adjust the parser; this is the
/// load-bearing planner-time verification per parent §6 signpost 4.
fn parse_h2spec_output(stdout: &str) -> Result<(usize, usize, std::collections::BTreeSet<String>)> {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures = std::collections::BTreeSet::new();
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('×').or_else(|| trimmed.strip_prefix('x')) {
            // Failed test. The ID is the first whitespace-delimited token.
            if let Some(id) = rest.split_whitespace().next() {
                failures.insert(id.to_string());
                failed += 1;
            }
        } else if trimmed.starts_with('✓') || trimmed.starts_with('o') {
            passed += 1;
        }
    }
    // Cross-check via the summary line if present.
    if let Some(summary_passed) = extract_summary_count(stdout, "Passed") {
        passed = summary_passed;
    }
    if let Some(summary_failed) = extract_summary_count(stdout, "Failed") {
        failed = summary_failed;
    }
    Ok((passed, failed, failures))
}

fn extract_summary_count(stdout: &str, key: &str) -> Option<usize> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&format!("{key}:")) {
            for tok in rest.split_whitespace() {
                if let Ok(n) = tok.parse::<usize>() {
                    return Some(n);
                }
            }
        }
    }
    None
}
```

The parser shape is the planner's best-guess at PLAN-write time; **the executor verifies the actual h2spec output format at Task 13 execution time** and adjusts the regex / token-marker shapes if needed. Record any changes in PROGRESS Task 13.

- [ ] **Step 13.7: Build the workspace and verify the new crate compiles.**

Run: `cargo build --workspace --all-targets`
Expected: clean. The new `h2spec-conformance` crate compiles; `tests/h2spec_runner.rs` compiles cleanly. The actual test invocation requires the h2spec binary; `cargo test -p h2spec-conformance` will run the test, which will `eprintln!`-skip if `h2spec` is unavailable locally.

- [ ] **Step 13.8: Run the runner test (will likely skip locally).**

Run: `cargo test -p h2spec-conformance -- --nocapture`
Expected: PASS (the test skips with an `eprintln!` if `which h2spec` fails locally). If `h2spec` IS available locally and the test runs, expect a populated stdout dump; the runner classifies failures and asserts the gate. **If the gate fails on the first end-to-end run**, the planner classifies each unexpected failure as deferral / foundation-limitation / regression-blocker; deferral and foundation-limitation entries land in `known-failures.txt` with a one-line annotation, and regression-blocker entries force re-loop into REVIEW state 5.

For the planner's PLAN-execution session: skip the local run (CI handles provisioning at Task 14). Record the disposition in PROGRESS Task 13.

- [ ] **Step 13.9: Update PROGRESS Task 13 + commit.**

PROGRESS Task 13 records:
- The h2spec runner crate scaffold landed.
- The h2spec binary version pin chosen at Task 14 (the curl-tar URL in the CI workflow embeds the version; Task 14 Step 14.1 selects from the latest `summerwind/h2spec` GitHub release at execution time per parent §6 signpost 3).
- The known-failures.txt initial state (empty body; populated at Task 14 if h2spec runs end-to-end in CI).
- ADR-0028 disposition: NOT landed (the gate-mechanics are mechanically deterministic per parent §6 signposts 3-4; recording inline here suffices). The ADR-0028 number stays available for phase-06+.

```bash
git add tests/conformance/h2spec/ \
        Cargo.toml \
        Cargo.lock \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: tests/conformance/h2spec runner crate (task 13)

First conformance suite in the project. Workspace member at
tests/conformance/h2spec/ with Cargo.toml + lib.rs + h2spec.yaml +
h2spec_runner.rs + known-failures.txt. Runner spawns envoy-bin against
an HCM HTTP2 config, runs h2spec, parses output, asserts ≥95% pass
rate + lockstep known-failures.txt maintenance.

ADR-0028 (h2spec integration posture) NOT landed: gate-mechanics are
mechanically deterministic per parent-05 SPEC §6 signposts 3-4;
inline PROGRESS narration suffices. ADR-0028 number stays available
for phase-06+.

D7 part 1 per phase 05.2 SPEC §3."
```

---

## Task 14 — CI workflow `h2spec` provisioning + state-4 phase-done gate verification

**Files:**

- Modify: `.github/workflows/ci.yml` — add `h2spec` binary provisioning step before `cargo test --workspace`.
- Modify: `tests/conformance/h2spec/known-failures.txt` — populate with the actual h2spec failure surface from the first end-to-end run (if any).
- Modify: `docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md` — append Task 14 with state-4 evidence.

**Estimated LoC:** ~30 (CI workflow ~10 + known-failures.txt ~20 entries; state-4 narration ~quotes-only).

**Signposts settled:**

- Parent §6 signpost 3 (h2spec binary management): CI workflow provisioning.
- SPEC §1 acceptance signal (a)(b)(c)(d)(e): all 9 fixtures green; h2spec ≥95%; fuzz clean; 5 stable-toolchain commands clean.

- [ ] **Step 14.1: Edit `.github/workflows/ci.yml` to provision h2spec.**

Insert a step before the existing `cargo test --workspace` step. The exact YAML location depends on the current workflow file structure — the planner reads `.github/workflows/ci.yml` at task time and inserts in the appropriate job. Minimal shape:

```yaml
      - name: Install h2spec
        run: |
          set -euo pipefail
          H2SPEC_VERSION="2.6.0"  # planner reads latest at task time
          curl -fsSL "https://github.com/summerwind/h2spec/releases/download/v${H2SPEC_VERSION}/h2spec_linux_amd64.tar.gz" \
            | tar xz -C /usr/local/bin
          h2spec --version
```

The `H2SPEC_VERSION` value is verified at task time by checking https://github.com/summerwind/h2spec/releases/latest — record the chosen pin in PROGRESS Task 14.

- [ ] **Step 14.2: Run the workspace verification cascade locally.**

Run: `cargo build --workspace --all-targets`
Expected: clean.

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean.

Run: `cargo test --workspace`
Expected: clean. Test count grew by ~21 (envoy-config: ~11 new; envoy-http2: ~15 across 5 modules; envoy-bin: 1 new integration test; differential: 1 new harness unit test; h2spec-conformance: 1 runner test that skips locally without h2spec).

Run: `cargo deny check`
Expected: `advisories ok, bans ok, licenses ok, sources ok` final-line gate signal. The `h2`, `http`, and transitive surfaces are dual-licensed MIT/Apache-2.0 and already on the allow-list.

Run: `cargo +nightly fuzz run parse_bootstrap -- -max_total_time=30` (if nightly toolchain available locally; otherwise skip and rely on CI).
Expected: clean run; the new `hcm_codec_http2.yaml` corpus seed is exercised; no crash.

- [ ] **Step 14.3: Push to CI; capture run URL.**

```bash
git push origin main
```

Wait for the CI run to complete. Capture:
- The CI run URL (`gh run list --limit 1 --json url -q '.[0].url'`).
- The per-fixture matrix from the differential job (`gh run view <RUN_ID> --log | grep -E 'fixture$|test result:'`).
- The h2spec job output (parsed pass/fail counts; populated `known-failures.txt` if any).

If the h2spec gate fails on first end-to-end run, classify each failure:
- **Deferral**: file an entry in `known-failures.txt` with a `# deferred to phase-NN <reason>` annotation.
- **Foundation limitation**: file an entry with `# h2 crate doesn't expose <hook>`.
- **Intentional Envoy-divergence**: file an entry with `# intentional Envoy-divergence per ADR-NNNN`.
- **Regression-blocker**: do NOT file an entry; re-loop into REVIEW state 5.

Update `tests/conformance/h2spec/known-failures.txt` with the classified entries.

- [ ] **Step 14.4: Quote the state-4 evidence into PROGRESS Task 14.**

PROGRESS Task 14 must include:
- CI run URL.
- Per-fixture matrix (9 fixtures: 0001/0002/0003/0004/0005/0006/0007/0008/0009 — all GREEN).
- h2spec pass rate + the populated known-failures.txt entries.
- Local stable-toolchain output: cargo build, clippy, fmt, test --workspace, deny check, fuzz (if run).
- Cargo.lock diff summary (h2 + http + transitive surface formalized as direct).
- ADR-0027 landed at Task 1; ADR-0028 NOT landed (recorded in PROGRESS Task 13).

- [ ] **Step 14.5: Verify the state-4 phase-done gate.**

The gate (per SPEC §1 acceptance signal):

- (a) Fixture 0009 green at the Docker-gated CI level (CI run URL quoted).
- (b) Fixtures 0001-0008 remain green simultaneously (per-fixture matrix from CI shows all 8 GREEN).
- (c) `tests/conformance/h2spec/` runs at ≥95% pass with any failing tests catalogued in `known-failures.txt` with one-line doctrine reasons.
- (d) Fuzz target `parse_bootstrap` runs clean for short-budget CI run; the new `hcm_codec_http2.yaml` seed is exercised.
- (e) `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo deny check` all clean on stable-toolchain CI job.

If any of (a)-(e) fails, the state-4 gate is RED — re-loop into either Task 1+ (if a code change is needed) or systematic-debugging (if the failure is a flake / environmental issue). Do NOT advance to state 5 on a RED gate.

- [ ] **Step 14.6: Commit Task 14 (state-4 close-out).**

```bash
git add .github/workflows/ci.yml \
        tests/conformance/h2spec/known-failures.txt \
        docs/envoy-rust/phases/05.2-http2-downstream/PROGRESS.md
git commit -m "phase 05.2: state-4 phase-done gate verification (task 14)

CI workflow provisions h2spec via curl-tar. State-4 gate GREEN: all
9 Docker-gated fixtures (0001-0009) green simultaneously per CI run
<RUN_ID>; h2spec at <PASS_RATE>% with <N> entries catalogued in
tests/conformance/h2spec/known-failures.txt; fuzz parse_bootstrap
clean; 5 stable-toolchain commands clean; cargo deny clean (h2 + http
already on allow-list); Cargo.lock diff is the projected h2 + http +
transitive surface formalization.

ADR-0027 landed at Task 1; ADR-0028 NOT landed (mechanical scope per
parent-05 SPEC §6 signposts 3-4; recorded in PROGRESS Task 13).

D7 part 2 + phase-done gate per phase 05.2 SPEC §1."
```

---

## State 5 / State 6 routing (NOT part of this PLAN's tasks)

After Task 14 (state-4 close-out), the lifecycle advances out of state 3:

- **State 5** (next session): Invoke `superpowers:requesting-code-review` against the head commit. Output: `docs/envoy-rust/phases/05.2-http2-downstream/REVIEW.md`. If REVIEW finds issues → re-enter at state 3 (per SKILL_ROUTING.md §5.2).
- **State 6** (next session after REVIEW approves): commit the phase-done close-out (ROADMAP row `05.2` `planned` → `done`; STATE.md advances active phase to `05.3-http2-upstream` lifecycle state 2). Commit message format per SKILL_ROUTING.md §5.3 — see SPEC §9 for the full template.

These are separate sessions per `BOOTSTRAP_PROMPT.md` §5.1's "one state per session" discipline.

---
